// rclone binary wrapper — port of internal/rclone (client, ops, remotes, sync).
// Shells out to `rclone`; no library dependency so any host rclone works.
import Foundation

public enum RcloneError: Error, LocalizedError {
    case binaryNotFound(String)
    case runFailed(command: String, stderr: String)
    case cancelled

    public var errorDescription: String? {
        switch self {
        case .binaryNotFound(let p): return "rclone: binary not found: \(p)"
        case .runFailed(let c, let e): return "rclone \(c): \(e)"
        case .cancelled: return "rclone: cancelled"
        }
    }
}

/// Cooperative cancellation token for sync tasks. When cancelled, any running
/// process is terminated and spawn sites return .cancelled.
public final class CancellationToken: @unchecked Sendable {
    private let lock = NSLock()
    private var _cancelled = false
    private var terminate: (() -> Void)?

    public var isCancelled: Bool { lock.lock(); defer { lock.unlock() }; return _cancelled }

    public func cancel() {
        lock.lock()
        _cancelled = true
        let t = terminate
        lock.unlock()
        t?()
    }

    func onTerminate(_ f: @escaping () -> Void) {
        lock.lock()
        terminate = f
        let already = _cancelled
        lock.unlock()
        if already { f() }
    }
}

/// Thrown when a sync exits non-zero or is cancelled; carries the partial
/// result so callers can still read stats/file lists (Go returns res + err).
public struct SyncFailedError: Error {
    public let result: SyncResult
    public let message: String
    public let cancelled: Bool
}

public final class RcloneClient: @unchecked Sendable {
    public let binary: String
    public let configPath: String
    private let logger: Logger

    public init(binaryPath: String? = nil, configPath: String, logger: Logger) throws {
        let bin = try Self.resolveBinary(binaryPath)
        self.binary = bin
        self.configPath = configPath
        self.logger = logger
    }

    /// rclone shipped inside the app bundle (Contents/MacOS/rclone or
    /// Contents/Resources/rclone). The bundled build always wins over PATH so
    /// the app never depends on a system rclone.
    public static func bundledBinary() -> String? {
        let fm = FileManager.default
        var candidates: [String] = []
        if let res = Bundle.main.resourcePath {
            candidates.append(res + "/rclone")
        }
        if let exe = Bundle.main.executablePath {
            candidates.append((exe as NSString).deletingLastPathComponent + "/rclone")
        }
        return candidates.first { fm.isExecutableFile(atPath: $0) }
    }

    public static func resolveBinary(_ path: String?) throws -> String {
        func lookPath(_ name: String) -> String? {
            for dir in ProcessInfo.processInfo.environment["PATH"]?.split(separator: ":") ?? [] {
                let p = String(dir) + "/" + name
                if FileManager.default.isExecutableFile(atPath: p) { return p }
            }
            return nil
        }
        guard let path, !path.isEmpty else {
            // No explicit override: bundled rclone first, PATH as dev fallback.
            if let p = bundledBinary() { return p }
            if let p = lookPath("rclone") { return p }
            throw RcloneError.binaryNotFound("rclone")
        }
        if let p = lookPath(path) { return p }
        if FileManager.default.isExecutableFile(atPath: path) { return path }
        throw RcloneError.binaryNotFound(path)
    }

    public func version() throws -> String {
        let out = try run(["version"])
        let first = out.split(separator: "\n", maxSplits: 1).first.map(String.init) ?? ""
        return first.trimmingCharacters(in: .whitespaces)
    }

    // MARK: - Simple run (capture stdout)

    @discardableResult
    private func run(_ args: [String], token: CancellationToken? = nil) throws -> String {
        let proc = Process()
        proc.executableURL = URL(fileURLWithPath: binary)
        proc.arguments = args
        let outPipe = Pipe(), errPipe = Pipe()
        proc.standardOutput = outPipe
        proc.standardError = errPipe

        token?.onTerminate { [weak proc] in proc?.terminate() }
        try proc.run()
        if token?.isCancelled == true { proc.terminate() }

        let outData = outPipe.fileHandleForReading.readDataToEndOfFile()
        let errData = errPipe.fileHandleForReading.readDataToEndOfFile()
        proc.waitUntilExit()

        let stdout = String(data: outData, encoding: .utf8) ?? ""
        let stderr = String(data: errData, encoding: .utf8) ?? ""
        if token?.isCancelled == true { throw RcloneError.cancelled }
        if proc.terminationStatus != 0 {
            let msg = stderr.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
                ? stdout.trimmingCharacters(in: .whitespacesAndNewlines)
                : stderr.trimmingCharacters(in: .whitespacesAndNewlines)
            throw RcloneError.runFailed(command: args.joined(separator: " "),
                                        stderr: truncateString(msg, 500))
        }
        return stdout
    }

    // MARK: - File ops

    /// One-level listing ("remote:path" or absolute local path).
    public func listFiles(_ remotePath: String) throws -> [FileEntry] {
        let remotePath = remotePath.trimmingCharacters(in: .whitespaces)
        guard !remotePath.isEmpty else { throw RcloneError.runFailed(command: "lsjson", stderr: "path is required") }
        guard remotePath.contains(":") || remotePath.hasPrefix("/") else {
            throw RcloneError.runFailed(command: "lsjson", stderr: "path must be absolute or remote:path")
        }
        let out = try run(["--log-level", "ERROR", "lsjson", remotePath,
                           "--config", configPath, "--max-depth", "1"])
        let trimmed = out.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty, let data = trimmed.data(using: .utf8) else { return [] }
        return (try? JSONDecoder().decode([FileEntry].self, from: data)) ?? []
    }

    /// Recursive files-only listing for the Pending tab seed.
    public func listFilesForPending(_ remotePath: String, limit: Int = maxTrackedFiles) throws -> [FileEntry] {
        let remotePath = remotePath.trimmingCharacters(in: .whitespaces)
        guard !remotePath.isEmpty,
              remotePath.contains(":") || remotePath.hasPrefix("/") else { return [] }
        let out = try run(["--log-level", "ERROR", "lsjson", remotePath,
                           "--config", configPath, "--recursive", "--files-only"])
        let trimmed = out.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty, let data = trimmed.data(using: .utf8) else { return [] }
        var entries = (try? JSONDecoder().decode([FileEntry].self, from: data)) ?? []
        if entries.count > limit { entries = Array(entries.prefix(limit)) }
        return entries
    }

    public func mkdir(_ remotePath: String) throws {
        _ = try run(["mkdir", remotePath, "--config", configPath])
    }

    public func purge(_ remotePath: String) throws {
        _ = try run(["purge", remotePath, "--config", configPath])
    }

    public func deleteFile(_ remotePath: String) throws {
        _ = try run(["deletefile", remotePath, "--config", configPath])
    }

    public func about(_ remoteName: String) throws -> QuotaInfo {
        let out = try run(["about", remoteName + ":", "--config", configPath, "--json"])
        struct AboutJSON: Decodable { var used: Int64?; var total: Int64?; var free: Int64? }
        guard let data = out.data(using: .utf8),
              let a = try? JSONDecoder().decode(AboutJSON.self, from: data) else {
            throw RcloneError.runFailed(command: "about", stderr: "parse error")
        }
        return QuotaInfo(used: a.used ?? 0, total: a.total ?? 0, free: a.free ?? 0)
    }

    // MARK: - Remotes CRUD

    public func listRemotes() throws -> [Remote] {
        let out: String
        do {
            out = try run(["listremotes", "--config", configPath])
        } catch let RcloneError.runFailed(cmd, stderr) {
            // Empty/missing config exits non-zero with usage text.
            if stderr.contains("Usage:") || stderr.contains("Available commands:") { return [] }
            throw RcloneError.runFailed(command: cmd, stderr: stderr)
        }
        let types = remoteTypesFromDump()
        return out.split(separator: "\n").compactMap { line in
            let name = line.trimmingCharacters(in: .whitespaces)
            guard !name.isEmpty else { return nil }
            let n = name.hasSuffix(":") ? String(name.dropLast()) : name
            return n.isEmpty ? nil : Remote(name: n, type: types[n] ?? "")
        }
    }

    private func remoteTypesFromDump() -> [String: String] {
        guard let out = try? run(["config", "dump", "--config", configPath]),
              let data = out.data(using: .utf8),
              let dump = try? JSONSerialization.jsonObject(with: data) as? [String: [String: Any]]
        else { return [:] }
        var map: [String: String] = [:]
        for (name, section) in dump {
            if let t = section["type"] as? String { map[name] = t }
        }
        return map
    }

    /// Create a remote non-interactively. configKVs are "key=value" pairs.
    public func createRemote(name: String, type: String, configKVs: [String]) throws {
        var args = ["config", "create", name, type]
        args.append(contentsOf: configKVs)
        args += ["--obscure", "--config", configPath]
        _ = try run(args)
    }

    /// Create then probe (lsd); deletes the remote if the probe fails.
    public func createRemoteVerified(name: String, type: String, configKVs: [String]) throws {
        try createRemote(name: name, type: type, configKVs: configKVs)
        do {
            try testRemote(name)
        } catch {
            try? deleteRemote(name)
            throw error
        }
    }

    public func deleteRemote(_ name: String) throws {
        _ = try run(["config", "delete", name, "--config", configPath])
    }

    public func testRemote(_ name: String) throws {
        _ = try run(["lsd", name + ":", "--config", configPath, "--max-depth", "1"])
    }

    // MARK: - Sync

    /// Run the configured action, streaming progress to onProgress.
    public func sync(_ cfg: SyncConfig, onProgress: (@Sendable (SyncStats) -> Void)? = nil,
                     token: CancellationToken? = nil) throws -> SyncResult {
        let args = try buildArgs(cfg)
        // Seed Pending tab with real file names while rclone runs.
        let seedPath = pendingSeedPath(cfg)
        return try execute(args: args, onProgress: onProgress, seedPath: seedPath, token: token)
    }

    private func pendingSeedPath(_ cfg: SyncConfig) -> String {
        var src = cfg.source, dst = cfg.dest
        if src.isEmpty || dst.isEmpty {
            if !cfg.sourceRemote.isEmpty, !cfg.sourcePath.isEmpty {
                src = cfg.sourceRemote + ":" + cfg.sourcePath
            }
            if !cfg.destRemote.isEmpty, !cfg.destPath.isEmpty {
                dst = cfg.destRemote + ":" + cfg.destPath
            }
        }
        return cfg.action == .pull ? dst : src
    }

    func buildArgs(_ cfg: SyncConfig) throws -> [String] {
        let (src, dst) = try resolveEndpoints(cfg)
        let interval = cfg.statsInterval.isEmpty ? "1s" : cfg.statsInterval
        let base = ["--config", configPath, "--stats", interval, "--use-json-log", "-v"]

        var args: [String]
        switch cfg.action {
        case .pull:     args = ["sync", dst, src, "--update"]
        case .push:     args = ["sync", src, dst, "--update"]
        case .bi:       args = ["bisync", src, dst]
        case .biResync: args = ["bisync", src, dst, "--resync", "--force"]
        case .copy:     args = ["copy", src, dst]
        case .move:     args = ["move", src, dst]
        case .check:    args = ["check", src, dst]
        case .dryRun:   args = ["sync", src, dst, "--dry-run", "--update"]
        }
        args += base
        if let p = cfg.profile { args += Self.profileToFlags(p) }
        return args
    }

    private func resolveEndpoints(_ cfg: SyncConfig) throws -> (String, String) {
        if !cfg.source.isEmpty, !cfg.dest.isEmpty { return (cfg.source, cfg.dest) }
        guard !cfg.sourceRemote.isEmpty, !cfg.sourcePath.isEmpty,
              !cfg.destRemote.isEmpty, !cfg.destPath.isEmpty else {
            throw RcloneError.runFailed(command: "sync", stderr: "requires Source+Dest or remotes+paths")
        }
        return (cfg.sourceRemote + ":" + cfg.sourcePath,
                cfg.destRemote + ":" + cfg.destPath)
    }

    static func profileToFlags(_ p: ProfileFlags) -> [String] {
        var f: [String] = []
        if !p.bandwidth.isEmpty { f += ["--bwlimit", p.bandwidth] }
        if p.transfers > 0 { f += ["--transfers", String(p.transfers)] }
        if p.checkers > 0 { f += ["--checkers", String(p.checkers)] }
        if p.tpsLimit > 0 { f += ["--tpslimit", String(p.tpsLimit)] }
        if !p.minAge.isEmpty { f += ["--min-age", p.minAge] }
        if !p.maxAge.isEmpty { f += ["--max-age", p.maxAge] }
        if !p.minSize.isEmpty { f += ["--min-size", p.minSize] }
        if !p.maxSize.isEmpty { f += ["--max-size", p.maxSize] }
        if !p.excludeIfPresent.isEmpty { f += ["--exclude-if-present", p.excludeIfPresent] }
        if p.maxDelete > 0 { f += ["--max-delete", String(p.maxDelete)] }
        if p.dryRun { f += ["--dry-run"] }
        if p.noUnicodeNormalize { f += ["--no-unicode-normalization"] }
        for inc in p.includes where !inc.trimmingCharacters(in: .whitespaces).isEmpty {
            f += ["--include", inc.trimmingCharacters(in: .whitespaces)]
        }
        for exc in p.excludes where !exc.trimmingCharacters(in: .whitespaces).isEmpty {
            f += ["--exclude", exc.trimmingCharacters(in: .whitespaces)]
        }
        if p.multiThreadStreams > 0 { f += ["--multi-thread-streams", String(p.multiThreadStreams)] }
        if !p.bufferSize.isEmpty { f += ["--buffer-size", p.bufferSize] }
        if p.retries > 0 { f += ["--retries", String(p.retries)] }
        if p.lowLevelRetries > 0 { f += ["--low-level-retries", String(p.lowLevelRetries)] }
        if !p.maxDuration.isEmpty { f += ["--max-duration", p.maxDuration] }
        if !p.retriesSleep.isEmpty { f += ["--retries-sleep", p.retriesSleep] }
        if !p.connTimeout.isEmpty { f += ["--contimeout", p.connTimeout] }
        if !p.ioTimeout.isEmpty { f += ["--timeout", p.ioTimeout] }
        if !p.orderBy.isEmpty { f += ["--order-by", p.orderBy] }
        if p.checkFirst { f += ["--check-first"] }
        if p.immutable { f += ["--immutable"] }
        if !p.maxTransfer.isEmpty { f += ["--max-transfer", p.maxTransfer] }
        if !p.maxDeleteSize.isEmpty { f += ["--max-delete-size", p.maxDeleteSize] }
        if !p.suffix.isEmpty { f += ["--suffix", p.suffix] }
        if p.suffixKeepExtension { f += ["--suffix-keep-extension"] }
        if !p.backupDir.isEmpty { f += ["--backup-dir", p.backupDir] }
        if p.sizeOnly { f += ["--size-only"] }
        if p.updateMode { f += ["--update"] }
        if p.ignoreExisting { f += ["--ignore-existing"] }
        if p.deleteExcluded { f += ["--delete-excluded"] }
        if p.maxDepth > 0 { f += ["--max-depth", String(p.maxDepth)] }
        switch p.deleteTiming.lowercased().trimmingCharacters(in: .whitespaces) {
        case "before": f += ["--delete-before"]
        case "after": f += ["--delete-after"]
        case "during": f += ["--delete-during"]
        default: break
        }
        if !p.conflictResolve.isEmpty { f += ["--conflict-resolve", p.conflictResolve] }
        if !p.conflictLoser.isEmpty { f += ["--conflict-loser", p.conflictLoser] }
        if !p.conflictSuffix.isEmpty { f += ["--conflict-suffix", p.conflictSuffix] }
        if p.resilient { f += ["--resilient"] }
        if !p.maxLock.isEmpty { f += ["--max-lock", p.maxLock] }
        if p.checkAccess { f += ["--check-access"] }
        return f
    }

    /// Run rclone with args; parse --use-json-log lines from stderr and text
    /// stats from either stream. Emits progress and tracks per-file status.
    private func execute(args: [String], onProgress: (@Sendable (SyncStats) -> Void)?,
                         seedPath: String, token: CancellationToken?) throws -> SyncResult {
        let proc = Process()
        proc.executableURL = URL(fileURLWithPath: binary)
        proc.arguments = args
        let outPipe = Pipe(), errPipe = Pipe()
        proc.standardOutput = outPipe
        proc.standardError = errPipe

        token?.onTerminate { [weak proc] in proc?.terminate() }
        try proc.run()
        if token?.isCancelled == true { proc.terminate() }

        var result = SyncResult(startedAt: unixNow(), exitCode: -1)

        final class Box: @unchecked Sendable {
            let lock = NSLock()
            var stats = SyncStats()
            let tracker = FileTransferTracker()
            var stderrBuf = ""
        }
        let box = Box()

        @Sendable func emitProgress() {
            box.lock.lock()
            box.stats.fileTransfers = box.tracker.snapshot(totalFiles: box.stats.filesTotal)
            var snap = box.stats
            snap.lastUpdate = unixNow()
            box.lock.unlock()
            onProgress?(snap)
        }

        // Immediate lifecycle marker for the pre-stats gap.
        box.lock.lock()
        box.stats.stage = "starting"
        box.lock.unlock()
        emitProgress()

        // Concurrent pending seed (bounded).
        let seedGroup = DispatchGroup()
        let seedCancel = CancellationToken()
        if !seedPath.isEmpty {
            seedGroup.enter()
            DispatchQueue.global().async { [weak self] in
                defer { seedGroup.leave() }
                // 20s timeout via token check is cooperative only.
                let deadline = Date().addingTimeInterval(20)
                if let entries = try? self?.listFilesForPending(seedPath),
                   !entries.isEmpty, Date() < deadline, !seedCancel.isCancelled {
                    box.lock.lock()
                    box.tracker.seedPending(entries)
                    box.lock.unlock()
                    emitProgress()
                }
            }
        }

        // Consume both streams line-by-line on background queues.
        let ioGroup = DispatchGroup()
        func consume(_ handle: FileHandle, captureStderr: Bool) {
            ioGroup.enter()
            DispatchQueue.global().async {
                var buf = Data()
                while true {
                    let chunk = handle.availableData
                    if chunk.isEmpty { break }
                    buf.append(chunk)
                    // Split on newlines
                    while let nl = buf.firstIndex(of: 0x0A) {
                        let lineData = buf[..<nl]
                        buf = buf[buf.index(after: nl)...]
                        guard let line = String(data: lineData, encoding: .utf8) else { continue }
                        if captureStderr {
                            box.lock.lock(); box.stderrBuf += line + "\n"; box.lock.unlock()
                        }
                        box.lock.lock()
                        if !StatsParser.parseJSONStatsLine(line, into: &box.stats) {
                            StatsParser.parseStatsLine(line, into: &box.stats)
                        }
                        StatsParser.updateStage(from: line, into: &box.stats)
                        StatsParser.ingestJSONLogLine(line, into: &box.stats, track: box.tracker)
                        box.lock.unlock()
                        emitProgress()
                    }
                }
                // Trailing partial line
                if let line = String(data: buf, encoding: .utf8), !line.isEmpty {
                    if captureStderr {
                        box.lock.lock(); box.stderrBuf += line; box.lock.unlock()
                    }
                    box.lock.lock()
                    if !StatsParser.parseJSONStatsLine(line, into: &box.stats) {
                        StatsParser.parseStatsLine(line, into: &box.stats)
                    }
                    StatsParser.updateStage(from: line, into: &box.stats)
                    StatsParser.ingestJSONLogLine(line, into: &box.stats, track: box.tracker)
                    box.lock.unlock()
                    emitProgress()
                }
                ioGroup.leave()
            }
        }
        consume(outPipe.fileHandleForReading, captureStderr: false)
        consume(errPipe.fileHandleForReading, captureStderr: true)

        ioGroup.wait()
        seedCancel.cancel()
        _ = seedGroup.wait(timeout: .now() + 1)

        proc.waitUntilExit()
        result.endedAt = unixNow()
        result.exitCode = Int(proc.terminationStatus)
        box.lock.lock()
        result.stderr = box.stderrBuf
        box.stats.fileTransfers = box.tracker.snapshot(totalFiles: box.stats.filesTotal)
        result.stats = box.stats
        box.lock.unlock()

        if token?.isCancelled == true {
            throw SyncFailedError(result: result, message: "cancelled", cancelled: true)
        }
        if result.exitCode != 0 {
            throw SyncFailedError(
                result: result,
                message: "rclone \(args.joined(separator: " ")): exit \(result.exitCode) (stderr: \(truncateString(box.stderrBuf, 500)))",
                cancelled: false)
        }
        return result
    }
}
