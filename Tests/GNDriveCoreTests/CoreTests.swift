import XCTest
@testable import GNDriveCore

final class CoreTests: XCTestCase {

    private func tempDir() throws -> String {
        let dir = NSTemporaryDirectory() + "gndrive-test-" + UUID().uuidString
        try FileManager.default.createDirectory(atPath: dir, withIntermediateDirectories: true)
        return dir
    }

    func testArgon2Roundtrip() throws {
        let hash = try Argon2.createHash(password: "hunter2")
        XCTAssertTrue(hash.hasPrefix("$argon2id$v=19$m=65536,t=3,p=4$"))
        XCTAssertTrue(Argon2.verify(password: "hunter2", encoded: hash))
        XCTAssertFalse(Argon2.verify(password: "wrong", encoded: hash))
        let salt = try Argon2.extractSalt(encoded: hash)
        XCTAssertEqual(salt.count, Argon2.saltLength)
    }

    func testCryptoRoundtrip() throws {
        let dir = try tempDir()
        defer { try? FileManager.default.removeItem(atPath: dir) }
        let src = (dir as NSString).appendingPathComponent("plain.txt")
        try "secret-config-contents".write(toFile: src, atomically: true, encoding: .utf8)
        let key = Data((0..<32).map { UInt8($0) })
        let enc = src + ".enc"
        try FileCrypto.encryptFile(src: src, dst: enc, key: key)
        try FileCrypto.decryptFile(src: enc, dst: src + ".out", key: key)
        let out = try String(contentsOfFile: src + ".out", encoding: .utf8)
        XCTAssertEqual(out, "secret-config-contents")
    }

    func testAuthSetupUnlockLock() throws {
        let dir = try tempDir()
        defer { try? FileManager.default.removeItem(atPath: dir) }
        // Plant a config file to encrypt.
        let rc = (dir as NSString).appendingPathComponent("rclone.conf")
        try "[gdrive]\ntype = drive\n".write(toFile: rc, atomically: true, encoding: .utf8)

        let log = Logger(mode: .foreground)
        let auth = try AuthService(configDir: dir, logger: log, keyStore: MemorySecureStore())
        XCTAssertFalse(auth.isSetup)
        XCTAssertTrue(auth.isUnlocked)

        try auth.setupPassword("testpass")
        XCTAssertTrue(auth.isSetup)
        XCTAssertTrue(auth.isUnlocked)

        try auth.lock()
        XCTAssertFalse(auth.isUnlocked)
        XCTAssertTrue(FileManager.default.fileExists(atPath: rc + ".enc"))
        XCTAssertFalse(FileManager.default.fileExists(atPath: rc))

        try auth.unlock("testpass")
        XCTAssertTrue(auth.isUnlocked)
        XCTAssertEqual(try String(contentsOfFile: rc), "[gdrive]\ntype = drive\n")
    }

    func testStoreFlowsRoundtrip() throws {
        let dir = try tempDir()
        defer { try? FileManager.default.removeItem(atPath: dir) }
        let log = Logger(mode: .foreground)
        let store = try Store(path: (dir as NSString).appendingPathComponent("gn-drive.db"), logger: log)

        var f = Flow()
        f.id = "flow-1"
        f.name = "Daily"
        var op = FlowOperation()
        op.id = "op-1"
        op.sourceRemote = "local"
        op.sourcePath = "/tmp/a"
        op.targetRemote = "gdrive"
        op.targetPath = "/backup"
        op.action = "push"
        f.operations = [op]
        try store.saveFlow(&f)

        let loaded = try store.getFlow("flow-1")
        XCTAssertEqual(loaded.name, "Daily")
        XCTAssertEqual(loaded.operations.count, 1)
        XCTAssertEqual(loaded.operations[0].resolvedAction(), "push")
    }

    func testCronNext() throws {
        let cal = Calendar.current
        // 5-field: every day at 02:30
        let c = try CronSchedule("30 2 * * *")
        let now = cal.date(from: DateComponents(year: 2026, month: 9, day: 26, hour: 10, minute: 0))!
        let next = c.next(after: now)!
        let comps = cal.dateComponents([.month, .day, .hour, .minute, .second], from: next)
        XCTAssertEqual(comps.day, 27)
        XCTAssertEqual(comps.hour, 2)
        XCTAssertEqual(comps.minute, 30)
        XCTAssertEqual(comps.second, 0)

        // 6-field with seconds
        let c2 = try CronSchedule("15 * * * * *")
        let n2 = c2.next(after: now)!
        XCTAssertEqual(cal.component(.second, from: n2), 15)

        // @hourly
        let c3 = try CronSchedule("@hourly")
        let n3 = c3.next(after: now)!
        XCTAssertEqual(cal.component(.minute, from: n3), 0)
    }

    func testFlowExecutesEndToEnd() throws {
        // Real rclone local→local sync through the whole engine stack.
        guard (try? RcloneClient.resolveBinary(nil)) != nil else {
            throw XCTSkip("rclone not on PATH")
        }
        let dir = try tempDir()
        defer { try? FileManager.default.removeItem(atPath: dir) }
        let src = (dir as NSString).appendingPathComponent("src")
        let dst = (dir as NSString).appendingPathComponent("dst")
        try FileManager.default.createDirectory(atPath: src, withIntermediateDirectories: true)
        try "payload".write(toFile: src + "/f.txt", atomically: true, encoding: .utf8)

        let cfgDir = (dir as NSString).appendingPathComponent("cfg")
        let log = Logger(mode: .foreground)
        let store = try Store(path: cfgDir + "/gn-drive.db", logger: log)
        let rclone = try RcloneClient(configPath: cfgDir + "/rclone.conf", logger: log)

        let bus = EventBus()
        let sync = SyncEngine(logger: log, bus: bus, store: store, rclone: rclone)
        let flows = FlowEngine(store: store, syncEngine: sync, bus: bus, log: log)
        sync.setFlowExecutor(flows)
        let hub = RuntimeHub(bus: bus, flows: flows, tasks: sync)

        var f = Flow()
        f.id = "f1"; f.name = "T"
        var op = FlowOperation()
        op.id = "op1"; op.sourceRemote = ""; op.sourcePath = src
        op.targetRemote = ""; op.targetPath = dst; op.action = "push"
        f.operations = [op]
        try store.saveFlow(&f)

        sync.start()
        defer { sync.stop() }
        try flows.execute(flowID: "f1")

        // Wait for terminal status
        let deadline = Date().addingTimeInterval(30)
        while Date() < deadline {
            if flows.status("f1") == "completed" { break }
            if flows.status("f1") == "failed" { break }
            Thread.sleep(forTimeInterval: 0.1)
        }
        XCTAssertEqual(flows.status("f1"), "completed")
        XCTAssertEqual(try String(contentsOfFile: dst + "/f.txt"), "payload")
        XCTAssertTrue(FileManager.default.fileExists(atPath: dst + "/f.txt"))

        let snap = hub.snapshot()
        XCTAssertTrue(snap.flows.contains { $0.id == "f1" })

        // History row written
        let hist = try store.listHistory(limit: 10, offset: 0)
        XCTAssertFalse(hist.isEmpty)
    }

    func testComposePath() {
        XCTAssertEqual(composePath(remote: "gdrive", path: "/backup"), "gdrive:/backup")
        XCTAssertEqual(composePath(remote: "gdrive", path: ""), "gdrive:")
        XCTAssertEqual(composePath(remote: "", path: "/tmp/x"), "/tmp/x")
        XCTAssertEqual(composePath(remote: "local", path: "/tmp/x"), "/tmp/x")
    }
}
