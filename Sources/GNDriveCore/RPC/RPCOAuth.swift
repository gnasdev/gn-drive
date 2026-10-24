// OAuth remote creation: `rclone authorize` session + non-interactive wizard.
//
// Tokens live only in this process, are never logged, and are zeroed when the
// session is completed, cancelled, or expires.
import Foundation

final class RPCOAuthSession: @unchecked Sendable {
    let id: String
    let type: String
    let startedAt = Date()
    let lock = NSLock()
    var process: Process?
    var token = ""
    var authURL: String?
    var status = "waiting" // waiting | authorized | failed | cancelled
    var errorMessage = ""
    var stdoutBuffer = ""
    var stderrBuffer = ""
    var finished = false

    init(id: String, type: String) {
        self.id = id
        self.type = type
    }

    func snapshot() -> (status: String, token: String, error: String) {
        lock.lock(); defer { lock.unlock() }
        return (status, token, errorMessage)
    }
}

final class RPCOAuthManager: @unchecked Sendable {
    static let sessionLifetime: TimeInterval = 10 * 60

    private let binary: String
    private let configPath: String
    private let logger: Logger
    private let lock = NSLock()
    private var sessions: [String: RPCOAuthSession] = [:]
    private var activeID: String?
    private var counter = 0

    /// Bridge push (`remotes:oauth`); set by RPCService.
    var pushEvent: ((String, [String: Any]) -> Void)?

    init(binary: String, configPath: String, logger: Logger) {
        self.binary = binary
        self.configPath = configPath
        self.logger = logger
    }

    var hasActiveSession: Bool {
        lock.lock(); defer { lock.unlock() }
        return activeID != nil
    }

    func start(type: String, clientID: String?, clientSecret: String?) throws -> RPCOAuthSession {
        lock.lock()
        if let activeID, let existing = sessions[activeID], !existing.finished {
            lock.unlock()
            throw RPCError("oauth_session_active",
                           "another OAuth authorization is already running (session \(activeID))")
        }
        counter += 1
        let session = RPCOAuthSession(id: "O\(counter)", type: type)
        sessions[session.id] = session
        activeID = session.id
        lock.unlock()

        var args = ["authorize", type]
        if let clientID, let clientSecret {
            args += [clientID, clientSecret]
        }
        args += ["--auth-no-open-browser", "--config", configPath]

        let proc = Process()
        proc.executableURL = URL(fileURLWithPath: binary)
        proc.arguments = args
        let outPipe = Pipe(), errPipe = Pipe()
        proc.standardOutput = outPipe
        proc.standardError = errPipe
        session.process = proc

        do {
            try proc.run()
        } catch {
            finish(session, status: "failed", error: "could not start rclone authorize: \(error)")
            throw RPCError("oauth_failed", "could not start rclone authorize")
        }

        consume(session, handle: outPipe.fileHandleForReading, isStderr: false)
        consume(session, handle: errPipe.fileHandleForReading, isStderr: true)
        proc.terminationHandler = { [weak self, weak session] _ in
            guard let self, let session else { return }
            self.processExited(session)
        }

        scheduleExpiry(session)

        // rclone prints the URL to stderr within a few seconds.
        let deadline = Date().addingTimeInterval(10)
        while Date() < deadline {
            session.lock.lock()
            let url = session.authURL
            let done = session.finished
            session.lock.unlock()
            if let url, !url.isEmpty { break }
            if done { break }
            Thread.sleep(forTimeInterval: 0.05)
        }
        session.lock.lock()
        let url = session.authURL
        let status = session.status
        let err = session.errorMessage
        let finished = session.finished
        session.lock.unlock()

        if let url, !url.isEmpty {
            pushEvent?("remotes:oauth", ["sessionID": session.id, "status": "waiting", "authURL": url])
            return session
        }
        if finished || status != "waiting" {
            forget(session.id)
            throw RPCError("oauth_failed",
                           err.isEmpty ? "rclone authorize did not produce a URL" : RPCRedact.sanitize(err))
        }
        // Still waiting: return without a URL (the client may poll the event).
        return session
    }

    func session(_ id: String) -> RPCOAuthSession? {
        lock.lock(); defer { lock.unlock() }
        return sessions[id]
    }

    func cancel(_ id: String) -> Bool {
        guard let session = session(id), !session.finished else { return false }
        session.lock.lock()
        session.status = "cancelled"
        session.token = ""
        session.lock.unlock()
        session.process?.terminate()
        finish(session, status: "cancelled", error: "")
        return true
    }

    func forget(_ id: String) {
        lock.lock()
        let removed = sessions.removeValue(forKey: id)
        if activeID == id { activeID = nil }
        lock.unlock()
        if let removed {
            removed.lock.lock()
            removed.token = ""
            removed.lock.unlock()
        }
    }

    // MARK: - Process plumbing

    private func consume(_ session: RPCOAuthSession, handle: FileHandle, isStderr: Bool) {
        DispatchQueue.global().async { [weak session] in
            let fd = handle.fileDescriptor
            var buf = [UInt8](repeating: 0, count: 4096)
            while true {
                let n = read(fd, &buf, buf.count)
                if n <= 0 { break }
                let chunk = String(decoding: buf[0..<n], as: UTF8.self)
                guard let session else { return }
                session.lock.lock()
                if isStderr { session.stderrBuffer += chunk } else { session.stdoutBuffer += chunk }
                if session.authURL == nil, isStderr {
                    session.authURL = Self.extractURL(from: session.stderrBuffer)
                }
                session.lock.unlock()
            }
        }
    }

    private func processExited(_ session: RPCOAuthSession) {
        let code = session.process?.terminationStatus ?? -1
        session.lock.lock()
        let stdout = session.stdoutBuffer
        let stderr = session.stderrBuffer
        session.lock.unlock()

        let token = Self.extractToken(from: stdout).isEmpty
            ? Self.extractToken(from: stderr) : Self.extractToken(from: stdout)

        if code == 0, !token.isEmpty {
            session.lock.lock()
            session.token = token
            session.status = "authorized"
            session.finished = true
            session.lock.unlock()
            pushEvent?("remotes:oauth", ["sessionID": session.id, "status": "authorized"])
        } else {
            let msg = RPCRedact.stderrField(stderr.isEmpty ? stdout : stderr)
            let wasCancelled = session.snapshot().status == "cancelled"
            finish(session, status: wasCancelled ? "cancelled" : "failed",
                   error: wasCancelled ? "" : (msg.isEmpty ? "rclone authorize exited \(code)" : msg))
        }
        lock.lock()
        if activeID == session.id { activeID = nil }
        lock.unlock()
    }

    private func finish(_ session: RPCOAuthSession, status: String, error: String) {
        session.lock.lock()
        if session.finished {
            session.lock.unlock()
            return
        }
        session.finished = true
        session.status = status
        session.errorMessage = error
        if status != "authorized" { session.token = "" }
        session.lock.unlock()
        var data: [String: Any] = ["sessionID": session.id, "status": status]
        if !error.isEmpty { data["error"] = error }
        pushEvent?("remotes:oauth", data)
        lock.lock()
        if activeID == session.id { activeID = nil }
        lock.unlock()
    }

    private func scheduleExpiry(_ session: RPCOAuthSession) {
        DispatchQueue.global().asyncAfter(deadline: .now() + Self.sessionLifetime) { [weak self, weak session] in
            guard let self, let session else { return }
            let snap = session.snapshot()
            guard snap.status == "waiting" else { return }
            session.process?.terminate()
            self.finish(session, status: "cancelled", error: "OAuth session expired")
            self.logger.info("oauth: session expired", ("session", session.id))
        }
    }

    // MARK: - Parsing

    private static let urlRegex = try! NSRegularExpression(pattern: "https?://[^\\s\"']+")

    static func extractURL(from text: String) -> String? {
        let range = NSRange(text.startIndex..<text.endIndex, in: text)
        guard let m = urlRegex.firstMatch(in: text, range: range),
              let r = Range(m.range, in: text) else { return nil }
        var url = String(text[r])
        while let last = url.last, ").,;\"'".contains(last) { url.removeLast() }
        return url
    }

    static func extractToken(from text: String) -> String {
        guard let start = text.range(of: "Paste the following into your remote machine --->"),
              let end = text.range(of: "<---End paste", range: start.upperBound..<text.endIndex) else {
            return ""
        }
        return text[start.upperBound..<end.lowerBound].trimmingCharacters(in: .whitespacesAndNewlines)
    }
}
