// Protocol v1 coverage: dispatcher round-trips per method group, gating,
// error codes, event pushes, coalescing and EOF shutdown.
import XCTest
@testable import GNDriveCore

// MARK: - Fixtures

private final class FakeLineSource: RPCLineSource, @unchecked Sendable {
    private let lock = NSLock()
    private var lines: [RPCLineReadResult] = []
    private var ended = false

    func push(_ json: String) {
        lock.lock()
        lines.append(.line(Data(json.utf8)))
        lock.unlock()
    }

    func pushRaw(_ data: Data) {
        lock.lock()
        lines.append(.line(data))
        lock.unlock()
    }

    func end() {
        lock.lock()
        ended = true
        lock.unlock()
    }

    func nextLine(maxBytes: Int) -> RPCLineReadResult {
        while true {
            lock.lock()
            if !lines.isEmpty {
                let next = lines.removeFirst()
                lock.unlock()
                if case .line(let data) = next, data.count > maxBytes { return .tooLong }
                return next
            }
            let done = ended
            lock.unlock()
            if done { return .eof }
            Thread.sleep(forTimeInterval: 0.005)
        }
    }
}

private final class FakeLineSink: RPCLineSink, @unchecked Sendable {
    private let lock = NSLock()
    private var frames: [[String: Any]] = []

    func writeLine(_ data: Data) -> Bool {
        guard let raw = try? JSONSerialization.jsonObject(with: data),
              let obj = raw as? [String: Any] else {
            return true
        }
        lock.lock()
        frames.append(obj)
        lock.unlock()
        return true
    }

    func all() -> [[String: Any]] {
        lock.lock(); defer { lock.unlock() }
        return frames
    }

    func waitFor(id: Any, timeout: TimeInterval = 5) -> [String: Any]? {
        wait(timeout: timeout) { frame in
            guard let frameID = frame["id"] else { return false }
            return Self.sameID(frameID, id)
        }
    }

    func waitFor(event: String, timeout: TimeInterval = 5) -> [String: Any]? {
        wait(timeout: timeout) { $0["event"] as? String == event }
    }

    func waitFor(event: String, where predicate: @escaping ([String: Any]) -> Bool,
                 timeout: TimeInterval = 5) -> [String: Any]? {
        wait(timeout: timeout) { frame in
            guard frame["event"] as? String == event,
                  let data = frame["data"] as? [String: Any] else { return false }
            return predicate(data)
        }
    }

    func waitForError(code: String, timeout: TimeInterval = 5) -> [String: Any]? {
        wait(timeout: timeout) {
            ($0["error"] as? [String: Any])?["code"] as? String == code
        }
    }

    private func wait(timeout: TimeInterval,
                      _ predicate: ([String: Any]) -> Bool) -> [String: Any]? {
        let deadline = Date().addingTimeInterval(timeout)
        while Date() < deadline {
            lock.lock()
            let match = frames.first(where: predicate)
            lock.unlock()
            if let match { return match }
            Thread.sleep(forTimeInterval: 0.01)
        }
        return nil
    }

    private static func sameID(_ a: Any, _ b: Any) -> Bool {
        if let x = a as? String, let y = b as? String { return x == y }
        if let x = a as? NSNumber, let y = b as? NSNumber { return x == y }
        return false
    }
}

final class RPCTests: XCTestCase {
    private var containers: [AppContainer] = []

    override func tearDown() {
        for c in containers { c.close() }
        containers.removeAll()
        super.tearDown()
    }

    private func tempDir() throws -> String {
        let dir = NSTemporaryDirectory() + "gndrive-rpc-" + UUID().uuidString
        try FileManager.default.createDirectory(atPath: dir, withIntermediateDirectories: true)
        return dir
    }

    /// Guarded rclone wrapper shared by every test in this class. It blocks
    /// `authorize`/`reconnect` and any `config create|update` for a type other
    /// than local/alias/memory (so no OAuth flow can start), and forwards the
    /// rest to a real rclone when one is on PATH; without a real rclone it
    /// answers `version` and exits 0.
    private static let guardedRclone: String = {
        let dir = NSTemporaryDirectory() + "gndrive-guarded-rclone-" + UUID().uuidString
        try? FileManager.default.createDirectory(atPath: dir, withIntermediateDirectories: true)
        let real = (try? RcloneClient.resolveBinary(nil)) ?? ""
        let script = """
        #!/bin/sh
        REAL="\(real)"
        args=" $* "
        case "$args" in
          *" authorize "*|*" reconnect "*) echo "GUARD: OAuth blocked in tests" >&2; exit 99 ;;
        esac
        if [ "$1" = "config" ] && { [ "$2" = "create" ] || [ "$2" = "update" ]; }; then
          case "$4" in
            local|alias|memory) ;;
            *) echo "GUARD: remote type '$4' blocked in tests" >&2; exit 98 ;;
          esac
        fi
        if [ -n "$REAL" ]; then exec "$REAL" "$@"; fi
        case "$args" in *" version "*) echo "rclone v1.99.0-test" ;; esac
        exit 0
        """
        let path = dir + "/rclone"
        try? script.write(toFile: path, atomically: true, encoding: .utf8)
        try? FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: path)
        return path
    }()

    private func makeContainer(dir: String) throws -> AppContainer {
        var opts = AppContainer.Options()
        opts.rcloneBinary = Self.guardedRclone
        opts.portalMode = true
        opts.configDir = dir
        opts.version = "test"
        let container = try AppContainer(opts: opts)
        container.syncEngine.start()
        containers.append(container)
        return container
    }

    private func makeDispatcher(_ container: AppContainer) -> RPCDispatcher {
        RPCDispatcher(container: container, logBuffer: RPCLogBuffer(capacity: 100))
    }

    private func rpcError(_ body: () throws -> Any) -> RPCError? {
        do {
            _ = try body()
            return nil
        } catch {
            return rpcMapError(error)
        }
    }

    private func requireRclone() throws {
        guard (try? RcloneClient.resolveBinary(nil)) != nil else {
            throw XCTSkip("rclone not on PATH")
        }
    }

    // MARK: - System

    func testPingSnapshotDoctorAndSettings() throws {
        let dir = try tempDir()
        defer { try? FileManager.default.removeItem(atPath: dir) }
        let c = try makeContainer(dir: dir)
        let d = makeDispatcher(c)

        let ping = try d.handle(method: "ping", params: [:])
        XCTAssertEqual(ping["pong"] as? Bool, true)
        XCTAssertEqual(ping["protocol"] as? Int, 1)

        let snap = try d.handle(method: "state.snapshot", params: ["sections": ["auth", "settings"]])
        XCTAssertEqual(snap["locked"] as? Bool, false)
        XCTAssertNotNil(snap["auth"])
        XCTAssertNotNil(snap["appSettings"])
        XCTAssertNil(snap["flows"])

        let doctor = try d.handle(method: "doctor", params: ["includeDataFiles": true])
        XCTAssertNotNil(doctor["ok"])
        XCTAssertFalse((doctor["checks"] as? [[String: Any]] ?? []).isEmpty)
        XCTAssertNotNil(doctor["dataFiles"])

        let set = try d.handle(method: "settings.set",
                               params: ["settings": ["debugMode": true]])
        XCTAssertEqual((set["settings"] as? [String: Any])?["debugMode"] as? Bool, true)

        let logs = try d.handle(method: "logs.read", params: ["limit": 5])
        XCTAssertNotNil(logs["lines"])

        let health = try d.handle(method: "service.health", params: [:])
        XCTAssertEqual(health["present"] as? Bool, false)

        let status = try d.handle(method: "service.status", params: ["scope": "user"])
        XCTAssertEqual(status["scope"] as? String, "user")
        XCTAssertNotNil(status["plistPath"])
    }

    func testUnknownMethodAndInvalidParams() throws {
        let dir = try tempDir()
        defer { try? FileManager.default.removeItem(atPath: dir) }
        let d = makeDispatcher(try makeContainer(dir: dir))

        XCTAssertEqual(rpcError { try d.handle(method: "nope", params: [:]) }?.code, "unknown_method")
        XCTAssertEqual(rpcError { try d.handle(method: "flows.get", params: [:]) }?.code, "invalid_params")
    }

    // MARK: - Auth gating

    func testAuthLifecycleAndLockedGate() throws {
        let dir = try tempDir()
        defer { try? FileManager.default.removeItem(atPath: dir) }
        let c = try makeContainer(dir: dir)
        let d = makeDispatcher(c)

        let setup = try d.handle(method: "auth.setup", params: ["password": "testpass"])
        XCTAssertEqual(setup["setup"] as? Bool, true)
        XCTAssertEqual(setup["unlocked"] as? Bool, true)

        _ = try d.handle(method: "auth.lock", params: [:])
        XCTAssertTrue(d.isLocked())
        XCTAssertEqual(rpcError { try d.handle(method: "flows.list", params: [:]) }?.code, "locked")
        XCTAssertEqual(rpcError { try d.handle(method: "runtime.snapshot", params: [:]) }?.code, "locked")

        // Always-available methods still work while locked.
        XCTAssertEqual(try d.handle(method: "ping", params: [:])["pong"] as? Bool, true)
        XCTAssertNotNil(try d.handle(method: "state.snapshot", params: [:])["auth"])

        XCTAssertEqual(rpcError { try d.handle(method: "auth.unlock", params: ["password": "wrong"]) }?.code,
                       "invalid_password")
        let unlocked = try d.handle(method: "auth.unlock", params: ["password": "testpass"])
        XCTAssertEqual(unlocked["unlocked"] as? Bool, true)

        _ = try d.handle(method: "auth.changePassword",
                         params: ["oldPassword": "testpass", "newPassword": "newpass"])
        _ = try d.handle(method: "auth.lock", params: [:])
        XCTAssertEqual(rpcError { try d.handle(method: "auth.unlock", params: ["password": "testpass"]) }?.code,
                       "invalid_password")
        _ = try d.handle(method: "auth.unlock", params: ["password": "newpass"])

        let removed = try d.handle(method: "auth.removePassword", params: ["password": "newpass"])
        XCTAssertEqual(removed["setup"] as? Bool, false)
        XCTAssertFalse(d.isLocked())
    }

    // MARK: - Flows and operations

    func testFlowAndOperationRoundTrip() throws {
        let dir = try tempDir()
        defer { try? FileManager.default.removeItem(atPath: dir) }
        let c = try makeContainer(dir: dir)
        let d = makeDispatcher(c)

        let created = try d.handle(method: "flows.create", params: ["name": "Nightly", "id": "F1"])
        XCTAssertEqual((created["flow"] as? [String: Any])?["id"] as? String, "F1")
        XCTAssertEqual(rpcError { try d.handle(method: "flows.create", params: ["id": "F1"]) }?.code,
                       "flow_exists")

        let op1 = try d.handle(method: "operations.create", params: [
            "flowID": "F1",
            "operation": ["id": "O1", "sourceRemote": "", "sourcePath": dir,
                          "targetRemote": "", "targetPath": dir, "action": "push"],
        ])
        XCTAssertEqual((op1["operation"] as? [String: Any])?["id"] as? String, "O1")

        let op2 = try d.handle(method: "operations.create", params: [
            "flowID": "F1",
            "operation": ["id": "O2", "sourceRemote": "", "sourcePath": dir,
                          "targetRemote": "", "targetPath": dir, "action": "pull"],
        ])
        XCTAssertEqual((op2["operation"] as? [String: Any])?["sortOrder"] as? Int, 1)

        let updated = try d.handle(method: "operations.update", params: [
            "flowID": "F1", "operationID": "O1", "patch": ["action": "bi"],
        ])
        XCTAssertEqual((updated["operation"] as? [String: Any])?["action"] as? String, "bi")

        let reordered = try d.handle(method: "operations.reorder",
                                     params: ["flowID": "F1", "operationIDs": ["O2", "O1"]])
        let ops = (reordered["flow"] as? [String: Any])?["operations"] as? [[String: Any]]
        XCTAssertEqual(ops?.map { $0["id"] as? String ?? "" }, ["O2", "O1"])

        XCTAssertEqual(rpcError {
            try d.handle(method: "operations.reorder", params: ["flowID": "F1", "operationIDs": ["O1"]])
        }?.code, "invalid_params")

        _ = try d.handle(method: "operations.delete", params: ["flowID": "F1", "operationID": "O2"])
        XCTAssertEqual(rpcError {
            try d.handle(method: "operations.delete", params: ["flowID": "F1", "operationID": "O2"])
        }?.code, "operation_not_found")

        let got = try d.handle(method: "flows.get", params: ["id": "F1"])
        XCTAssertEqual((got["flow"] as? [String: Any])?["name"] as? String, "Nightly")
        XCTAssertEqual(rpcError { try d.handle(method: "flows.get", params: ["id": "nope"]) }?.code,
                       "flow_not_found")

        let list = try d.handle(method: "flows.list", params: [:])
        XCTAssertEqual((list["flows"] as? [[String: Any]])?.count, 1)

        let save = try d.handle(method: "flows.save", params: ["flow": [
            "id": "F1", "name": "Nightly v2", "scheduleEnabled": true,
            "scheduleCron": "not a cron",
            "operations": [["id": "O1", "sourceRemote": "", "sourcePath": dir,
                            "targetRemote": "", "targetPath": dir, "action": "push"]],
        ]])
        XCTAssertEqual((save["warnings"] as? [[String: Any]])?.first?["code"] as? String, "invalid_cron")

        let deleted = try d.handle(method: "flows.delete", params: ["id": "F1"])
        XCTAssertEqual(deleted["deleted"] as? Bool, true)
        XCTAssertEqual(rpcError { try d.handle(method: "flows.get", params: ["id": "F1"]) }?.code,
                       "flow_not_found")
    }

    // MARK: - Profiles, schedules, sync

    func testProfileAndSyncRoundTrip() throws {
        try requireRclone()
        let dir = try tempDir()
        defer { try? FileManager.default.removeItem(atPath: dir) }
        let src = (dir as NSString).appendingPathComponent("src")
        let dst = (dir as NSString).appendingPathComponent("dst")
        try FileManager.default.createDirectory(atPath: src, withIntermediateDirectories: true)
        try "hello".write(toFile: src + "/a.txt", atomically: true, encoding: .utf8)

        let c = try makeContainer(dir: dir)
        let d = makeDispatcher(c)

        let saved = try d.handle(method: "profiles.save", params: ["profile": [
            "name": "local", "from": src, "to": dst, "direction": "push", "parallel": 2,
        ]])
        XCTAssertEqual((saved["profile"] as? [String: Any])?["name"] as? String, "local")

        let got = try d.handle(method: "profiles.get", params: ["name": "local"])
        XCTAssertEqual((got["profile"] as? [String: Any])?["direction"] as? String, "push")
        XCTAssertEqual(rpcError { try d.handle(method: "profiles.get", params: ["name": "x"]) }?.code,
                       "profile_not_found")

        XCTAssertEqual((try d.handle(method: "profiles.list", params: [:])["profiles"] as? [[String: Any]])?.count, 1)
        XCTAssertNotNil(try d.handle(method: "schedules.list", params: [:])["schedules"])

        let sync = try d.handle(method: "sync.profile",
                                params: ["profile": "local", "wait": true])
        XCTAssertEqual(sync["state"] as? String, "completed")
        XCTAssertTrue(FileManager.default.fileExists(atPath: dst + "/a.txt"))

        let tasks = try d.handle(method: "runtime.tasks", params: [:])
        XCTAssertNotNil(tasks["tasks"])

        var entries: [[String: Any]] = []
        let historyDeadline = Date().addingTimeInterval(3)
        while entries.isEmpty && Date() < historyDeadline {
            entries = (try d.handle(method: "history.list", params: ["limit": 10])["entries"] as? [[String: Any]]) ?? []
            if entries.isEmpty { Thread.sleep(forTimeInterval: 0.05) }
        }
        XCTAssertFalse(entries.isEmpty)
        let stats = try d.handle(method: "history.stats", params: [:])
        XCTAssertGreaterThanOrEqual(stats["totalSyncs"] as? Int ?? 0, 1)
        XCTAssertEqual(try d.handle(method: "history.clear", params: [:])["cleared"] as? Bool, true)
        XCTAssertTrue((try d.handle(method: "history.list", params: [:])["entries"] as? [[String: Any]] ?? []).isEmpty)

        XCTAssertEqual(rpcError { try d.handle(method: "sync.stop", params: ["taskID": "none"]) }?.code,
                       "task_not_found")

        let deleted = try d.handle(method: "profiles.delete", params: ["name": "local"])
        XCTAssertEqual(deleted["deleted"] as? Bool, true)
    }

    // MARK: - Remotes and filesystem

    func testRemotesAndFsList() throws {
        try requireRclone()
        let dir = try tempDir()
        defer { try? FileManager.default.removeItem(atPath: dir) }
        let c = try makeContainer(dir: dir)
        let d = makeDispatcher(c)

        let providers = try d.handle(method: "remotes.providers", params: [:])
        XCTAssertFalse((providers["providers"] as? [[String: Any]] ?? []).isEmpty)
        XCTAssertEqual(providers["cached"] as? Bool, false)
        let again = try d.handle(method: "remotes.providers", params: [:])
        XCTAssertEqual(again["cached"] as? Bool, true)

        let added = try d.handle(method: "remotes.add",
                                 params: ["name": "scratch", "type": "local"])
        XCTAssertEqual((added["remote"] as? [String: Any])?["name"] as? String, "scratch")
        XCTAssertEqual(added["verified"] as? Bool, true)
        XCTAssertEqual(rpcError {
            try d.handle(method: "remotes.add", params: ["name": "scratch", "type": "local"])
        }?.code, "remote_exists")

        let listed = try d.handle(method: "remotes.list", params: [:])
        XCTAssertTrue((listed["remotes"] as? [[String: Any]] ?? []).contains {
            $0["name"] as? String == "scratch"
        })

        let tested = try d.handle(method: "remotes.test", params: ["name": "scratch"])
        XCTAssertEqual(tested["ok"] as? Bool, true)
        XCTAssertEqual(rpcError { try d.handle(method: "remotes.test", params: ["name": "nope"]) }?.code,
                       "remote_not_found")

        try "x".write(toFile: (dir as NSString).appendingPathComponent("f.txt"),
                      atomically: true, encoding: .utf8)
        let files = try d.handle(method: "fs.list", params: ["path": dir])
        XCTAssertFalse((files["entries"] as? [[String: Any]] ?? []).isEmpty)
        XCTAssertEqual(files["truncated"] as? Bool, false)
        XCTAssertEqual(rpcError { try d.handle(method: "fs.list", params: ["path": "relative"]) }?.code,
                       "fs_missing_path")

        let deleted = try d.handle(method: "remotes.delete", params: ["name": "scratch"])
        XCTAssertEqual(deleted["deleted"] as? Bool, true)
        XCTAssertEqual(rpcError { try d.handle(method: "remotes.delete", params: ["name": "scratch"]) }?.code,
                       "remote_not_found")
    }

    // MARK: - Boards

    func testBoardCrudAndErrors() throws {
        let dir = try tempDir()
        defer { try? FileManager.default.removeItem(atPath: dir) }
        let c = try makeContainer(dir: dir)
        let d = makeDispatcher(c)

        let saved = try d.handle(method: "boards.save", params: ["board": [
            "id": "B1", "name": "Legacy",
            "nodes": [["id": "n1", "remoteName": "", "path": dir, "label": "one"]],
            "edges": [["id": "e1", "sourceID": "n1", "targetID": "n1", "action": "push"]],
        ]])
        XCTAssertEqual((saved["board"] as? [String: Any])?["id"] as? String, "B1")

        let got = try d.handle(method: "boards.get", params: ["id": "B1"])
        XCTAssertEqual(((got["board"] as? [String: Any])?["nodes"] as? [[String: Any]])?.count, 1)

        let list = try d.handle(method: "boards.list", params: [:])
        XCTAssertTrue((list["boards"] as? [[String: Any]] ?? []).contains { $0["id"] as? String == "B1" })

        let empty = try d.handle(method: "boards.save", params: ["board": [
            "id": "B2", "name": "Empty", "nodes": [], "edges": [],
        ]])
        XCTAssertEqual((empty["board"] as? [String: Any])?["id"] as? String, "B2")
        XCTAssertEqual(rpcError { try d.handle(method: "boards.execute", params: ["id": "B2"]) }?.code,
                       "board_empty")

        _ = try d.handle(method: "boards.delete", params: ["id": "B2"])
        XCTAssertEqual(rpcError { try d.handle(method: "boards.get", params: ["id": "B2"]) }?.code,
                       "board_not_found")
    }

    // MARK: - Service plumbing

    func testServiceRoundTrip() throws {
        let dir = try tempDir()
        defer { try? FileManager.default.removeItem(atPath: dir) }
        let c = try makeContainer(dir: dir)

        // Simulate the request/response envelope over the in-memory transport.
        let source = FakeLineSource()
        let sink = FakeLineSink()
        var options = RPCService.Options()
        options.exitOnShutdown = false
        let service = RPCService(container: c, source: source, sink: sink,
                                 lockPath: dir + "/gn-drive.lock",
                                 options: options)
        let done = expectation(description: "service finished")
        DispatchQueue.global().async {
            _ = service.run()
            done.fulfill()
        }

        XCTAssertNotNil(sink.waitFor(event: "hello"))

        source.push(#"{"id":1,"method":"ping"}"#)
        let ping = sink.waitFor(id: 1)
        XCTAssertEqual((ping?["result"] as? [String: Any])?["pong"] as? Bool, true)

        source.push(#"{"id":"s","method":"state.snapshot","params":{"sections":["auth"]}}"#)
        XCTAssertNotNil(sink.waitFor(id: "s"))

        source.push("{not json")
        XCTAssertNotNil(sink.waitForError(code: "invalid_request"))

        source.push(#"{"id":3,"method":"bogus"}"#)
        let unknown = sink.waitFor(id: 3)
        XCTAssertEqual((unknown?["error"] as? [String: Any])?["code"] as? String, "unknown_method")

        source.end()
        wait(for: [done], timeout: 10)
        XCTAssertEqual(sink.all().last?["event"] as? String, "app:shutdown")
        XCTAssertEqual((sink.all().last?["data"] as? [String: Any])?["reason"] as? String, "eof")
    }

    func testEventPushAndProgressCoalescing() throws {
        let dir = try tempDir()
        defer { try? FileManager.default.removeItem(atPath: dir) }
        let c = try makeContainer(dir: dir)

        let source = FakeLineSource()
        let sink = FakeLineSink()
        var options = RPCService.Options()
        options.exitOnShutdown = false
        let service = RPCService(container: c, source: source, sink: sink,
                                 lockPath: dir + "/gn-drive.lock",
                                 options: options)
        let done = expectation(description: "service finished")
        DispatchQueue.global().async {
            _ = service.run()
            done.fulfill()
        }
        XCTAssertNotNil(sink.waitFor(event: "hello"))

        c.bus.publish(BusTopic.stateChanged, StateChangedEvent(domain: "flows", id: "F9"))
        let state = sink.waitFor(event: "state:changed")
        XCTAssertEqual((state?["data"] as? [String: Any])?["domain"] as? String, "flows")
        XCTAssertEqual((state?["data"] as? [String: Any])?["id"] as? String, "F9")

        c.bus.publish(BusTopic.authUnlocked, AuthUnlockedEvent())
        XCTAssertNotNil(sink.waitFor(event: "auth:unlocked"))
        XCTAssertNotNil(sink.waitFor(event: "runtime:snapshot"))

        var first = SyncProgressEvent()
        first.taskID = "T1"; first.profileID = "F1:O1"; first.transferred = 10
        var second = first
        second.transferred = 99
        c.bus.publish(BusTopic.syncProgress, first)
        c.bus.publish(BusTopic.syncProgress, second)
        let progress = sink.waitFor(event: "sync:progress",
                                    where: { ($0["transferred"] as? Int) == 99 })
        XCTAssertNotNil(progress)
        let progressCount = sink.all().filter { $0["event"] as? String == "sync:progress" }.count
        XCTAssertLessThanOrEqual(progressCount, 1)

        c.log.info("rpc test log line", ("k", "v"))
        let logLine = sink.waitFor(event: "log:line")
        XCTAssertEqual((logLine?["data"] as? [String: Any])?["msg"] as? String, "rpc test log line")

        source.end()
        wait(for: [done], timeout: 10)
    }

    func testOversizedRequestRejected() throws {
        let dir = try tempDir()
        defer { try? FileManager.default.removeItem(atPath: dir) }
        let c = try makeContainer(dir: dir)
        let source = FakeLineSource()
        let sink = FakeLineSink()
        var options = RPCService.Options()
        options.exitOnShutdown = false
        let service = RPCService(container: c, source: source, sink: sink,
                                 lockPath: dir + "/gn-drive.lock",
                                 options: options)
        let done = expectation(description: "service finished")
        DispatchQueue.global().async {
            _ = service.run()
            done.fulfill()
        }
        XCTAssertNotNil(sink.waitFor(event: "hello"))

        let padding = String(repeating: "a", count: RPCService.maxRequestBytes + 16)
        let big = Data(("{\"id\":1,\"method\":\"ping\",\"params\":{\"x\":\"" + padding + "\"}}\n").utf8)
        source.pushRaw(big)
        source.push(#"{"id":2,"method":"ping"}"#)

        XCTAssertNotNil(sink.waitForError(code: "invalid_request"))
        XCTAssertNotNil(sink.waitFor(id: 2))

        source.end()
        wait(for: [done], timeout: 10)
    }
}
