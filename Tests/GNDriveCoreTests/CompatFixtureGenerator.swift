// Compat fixture generator / verifier for the Rust port.
// Skipped unless an env var is set, so normal `swift test` is unaffected.
//   GN_DRIVE_GEN_FIXTURES=<outdir>   generate locked/, plain/, expected.json, expected/rclone.conf
//   GN_DRIVE_VERIFY_DIR=<configdir>  unlock (pw below) + dump all data to <configdir>/swift-dump.json
// Driven by scripts/gen-compat-fixtures.sh. Uses MemorySecureStore (no keychain) and temp dirs only.
import XCTest
@testable import GNDriveCore

final class CompatFixtureGenerator: XCTestCase {
    static let password = "compat-test-pw-123"
    static let settingKeys = ["theme", "notifications", "legacy_flag", "language", "empty_value"]
    static let deltaKeys = ["gdrive:root", "local:/tmp/x"]

    static let rcloneConf = """
    [local-src]
    type = local

    [fake-s3]
    type = s3
    provider = AWS
    access_key_id = AKIAFAKEFAKEFAKE0000
    secret_access_key = fakeSecretKeyDoNotUse/0123456789abcdefABCDEF
    region = us-east-1

    """

    // MARK: dump

    private func obj<T: Encodable>(_ v: T) throws -> [String: Any] {
        let e = JSONEncoder()
        e.outputFormatting = [.sortedKeys]
        return try JSONSerialization.jsonObject(with: e.encode(v)) as! [String: Any]
    }

    private func dumpProfile(_ p: Profile) throws -> [String: Any] {
        var o = try obj(p)
        for k in ["maxDepth", "maxDelete", "multiThreadStreams", "retries", "lowLevelRetries", "tpsLimit"]
        where o[k] == nil { o[k] = NSNull() }
        return o
    }

    func dump(_ store: Store) throws -> [String: Any] {
        var out: [String: Any] = [:]
        out["profiles"] = try store.listProfiles().map(dumpProfile)
        out["schedules"] = try store.listSchedules().map { s -> [String: Any] in
            ["id": s.id, "profileName": s.profileName, "action": s.action, "cron": s.cron,
             "enabled": s.enabled, "lastRun": s.lastRun, "nextRun": s.nextRun,
             "lastResult": s.lastResult, "createdAt": s.createdAt]
        }
        out["history"] = try store.listHistory(limit: 1000, offset: 0).map { h -> [String: Any] in
            ["id": h.id, "profileName": h.profileName, "action": h.action, "state": h.state,
             "startedAt": h.startedAt, "finishedAt": h.finishedAt, "duration": h.duration,
             "bytes": h.bytes, "errors": h.errors, "files": h.files, "errorMessage": h.errorMessage]
        }
        let hs = try store.historyStats()
        out["historyStats"] = [
            "totalSyncs": hs.totalSyncs, "totalBytes": hs.totalBytes,
            "totalDuration": hs.totalDuration, "totalErrors": hs.totalErrors,
            "byProfile": Dictionary(uniqueKeysWithValues: hs.byProfile.map {
                ($0.key, ["syncs": $0.value.syncs, "bytes": $0.value.bytes,
                          "duration": $0.value.duration, "errors": $0.value.errors] as [String: Any])
            }),
        ] as [String: Any]
        var settings: [String: Any] = [:]
        for k in Self.settingKeys { settings[k] = try store.getSetting(k) }
        out["settings"] = settings
        out["boards"] = try store.listBoards().map { b0 -> [String: Any] in
            let b = try store.loadBoardGraph(b0.id)
            return ["id": b.id, "name": b.name, "createdAt": b.createdAt, "updatedAt": b.updatedAt,
                    "nodes": b.nodes.map { n -> [String: Any] in
                        ["id": n.id, "remoteName": n.remoteName, "path": n.path, "label": n.label, "x": n.x, "y": n.y]
                    },
                    "edges": b.edges.map { e -> [String: Any] in
                        ["id": e.id, "sourceID": e.sourceID, "targetID": e.targetID, "action": e.action,
                         "syncConfig": e.syncConfig]
                    }]
        }
        out["flows"] = try store.listFlows().map { f0 -> [String: Any] in
            let f = try store.getFlow(f0.id)
            let ops = try store.listOperations(flowID: f.id)
            return ["id": f.id, "name": f.name, "isCollapsed": f.isCollapsed,
                    "scheduleEnabled": f.scheduleEnabled, "scheduleCron": f.scheduleCron,
                    "sortOrder": f.sortOrder, "canvasJSON": f.canvasJSON,
                    "createdAt": f.createdAt, "updatedAt": f.updatedAt,
                    "operations": ops.map { o -> [String: Any] in
                        ["id": o.id, "flowID": o.flowID, "sourceRemote": o.sourceRemote,
                         "sourcePath": o.sourcePath, "targetRemote": o.targetRemote,
                         "targetPath": o.targetPath, "action": o.action, "syncConfig": o.syncConfig.value,
                         "isExpanded": o.isExpanded, "sortOrder": o.sortOrder]
                    }]
        }
        out["deltaStates"] = try Self.deltaKeys.compactMap { k -> [String: Any]? in
            guard let d = try? store.getDeltaState(k) else { return nil }
            return ["remoteKey": d.remoteKey, "provider": d.provider, "lastFullSync": d.lastFullSync,
                    "deltaCount": d.deltaCount, "isWatching": d.isWatching]
        }
        return out
    }

    private func writeJSON(_ o: Any, to path: String) throws {
        let d = try JSONSerialization.data(withJSONObject: o, options: [.prettyPrinted, .sortedKeys, .withoutEscapingSlashes])
        try d.write(to: URL(fileURLWithPath: path))
    }

    // MARK: populate

    private func populate(_ store: Store) throws {
        var a = Profile()
        a.name = "docs-push"; a.from = "local-src:/Users/me/Documents"; a.to = "fake-s3:bucket/docs"
        a.direction = "push"
        a.includedPaths = ["photos/2024/**", "日本語/ドキュメント/*.txt", "a b/c d.md"]
        a.excludedPaths = ["**/.DS_Store", "tmp/", "Ünïcødé/ñ/*.log"]
        a.bandwidth = 1024; a.parallel = 4; a.minSize = "1k"; a.maxSize = "10G"; a.useRegex = true
        a.maxDelete = 50; a.immutable = true; a.multiThreadStreams = 8; a.bufferSize = "16M"
        a.fastList = true; a.retries = 5; a.lowLevelRetries = 10; a.maxDuration = "2h"
        a.maxAge = "30d"; a.minAge = "1m"; a.maxDepth = 3; a.deleteExcluded = true; a.dryRun = true
        a.maxTransfer = "100G"; a.maxDeleteSize = "1G"; a.suffix = ".bak"; a.suffixKeepExtension = true
        a.checkFirst = true; a.orderBy = "size,desc"; a.retriesSleep = "5s"; a.tpsLimit = 2.5
        a.connTimeout = "30s"; a.ioTimeout = "1m"; a.sizeOnly = true; a.updateMode = true
        a.ignoreExisting = true; a.deleteTiming = "after"; a.resilient = true; a.maxLock = "15m"
        a.checkAccess = true; a.conflictLoser = "delete"; a.conflictSuffix = ".conflict"
        a.backupPath = "/backup/dir"; a.cachePath = "/cache/dir"; a.filterFromFile = "/etc/filters.txt"
        a.excludeIfPresent = ".nosync"; a.conflictResolution = "newer"
        try store.saveProfile(&a)

        var b = Profile()
        b.name = "bi-minimal"; b.from = "local-src:/tmp/a"; b.to = "local-src:/tmp/b"; b.direction = "bi"
        // all nullable ints/tpsLimit unset; tpsLimit explicitly nil
        try store.saveProfile(&b)

        var c = Profile()
        c.name = "résumé/ünï 日本"; c.from = "fake-s3:src"; c.to = "local-src:/dst"; c.direction = "bi-resync"
        c.includedPaths = ["only/this/path/"]; c.maxDelete = 0; c.retries = 0; c.tpsLimit = 0.0
        c.maxDepth = -1
        try store.saveProfile(&c)

        var s1 = Schedule()
        s1.id = "sched-1"; s1.profileName = "docs-push"; s1.action = "push"; s1.cron = "0 */6 * * *"
        s1.enabled = true; s1.lastRun = "2025-01-02T03:04:05Z"; s1.nextRun = "2025-01-02T09:00:00Z"
        s1.lastResult = "success"
        try store.saveSchedule(s1)
        var s2 = Schedule()
        s2.id = "sched-2"; s2.profileName = "bi-minimal"; s2.action = "bi"; s2.cron = "*/15 * * * *"
        s2.enabled = false
        try store.saveSchedule(s2)

        let hist: [HistoryEntry] = [
            HistoryEntry(id: "h1", profileName: "docs-push", action: "push", state: "success",
                         startedAt: "2025-01-01T00:00:00Z", finishedAt: "2025-01-01T00:05:30Z",
                         duration: 330_000_000_000, bytes: 1_234_567_890_123, errors: 0, files: 4242),
            HistoryEntry(id: "h2", profileName: "docs-push", action: "push", state: "error",
                         startedAt: "2025-01-02T00:00:00Z", finishedAt: "2025-01-02T00:00:07Z",
                         duration: 7_000_000_000, bytes: 0, errors: 3, files: 0,
                         errorMessage: "couldn't list: 403 \"Forbidden\"\nline2 日本語"),
            HistoryEntry(id: "h3", profileName: "bi-minimal", action: "bi", state: "cancelled",
                         startedAt: "2025-01-03T10:00:00Z", finishedAt: "2025-01-03T10:00:01Z",
                         duration: 1_000_000_000, bytes: 12, errors: 0, files: 1),
            HistoryEntry(id: "h4", profileName: "résumé/ünï 日本", action: "bi-resync", state: "success",
                         startedAt: "2025-01-04T11:11:11Z", finishedAt: "2025-01-04T11:12:11Z",
                         duration: 60_000_000_000, bytes: 999, errors: 1, files: 7,
                         errorMessage: "1 file skipped"),
            HistoryEntry(id: "h5", profileName: "bi-minimal", action: "bi", state: "running",
                         startedAt: "2025-01-05T12:00:00Z"),
        ]
        for h in hist { try store.saveHistory(h) }

        try store.setSetting("theme", "dark")
        try store.setSetting("notifications", "true")
        try store.setSetting("legacy_flag", "1")
        try store.setSetting("language", "日本語")
        try store.setSetting("empty_value", "")

        var board = Board()
        board.id = "board-1"; board.name = "Main board / ü"
        var n1 = BoardNode(); n1.id = "n1"; n1.remoteName = "local-src"; n1.path = "/Users/me"; n1.label = "Home"; n1.x = 10.5; n1.y = -20
        var n2 = BoardNode(); n2.id = "n2"; n2.remoteName = "fake-s3"; n2.path = "bucket/docs"; n2.label = "S3"; n2.x = 300; n2.y = 40.25
        var n3 = BoardNode(); n3.id = "n3"; n3.remoteName = "local-src"; n3.path = "/tmp/日本"; n3.label = ""; n3.x = 0; n3.y = 0
        board.nodes = [n1, n2, n3]
        var e1 = BoardEdge(); e1.id = "e1"; e1.sourceID = "n1"; e1.targetID = "n2"; e1.action = "push"
        e1.syncConfig = "{\"bandwidth\":\"5M\",\"filters\":{\"exclude\":[\"*.tmp\"],\"depth\":2}}"
        var e2 = BoardEdge(); e2.id = "e2"; e2.sourceID = "n2"; e2.targetID = "n3"; e2.action = "bi"
        board.edges = [e1, e2]  // e2 keeps default syncConfig "{}"
        try store.saveBoardGraph(board)

        var f1 = Flow()
        f1.id = "flow-1"; f1.name = "Daily backup"; f1.isCollapsed = true; f1.scheduleEnabled = true
        f1.scheduleCron = "30 2 * * *"; f1.sortOrder = 0
        f1.canvasJSON = "{\"nodes\":[{\"id\":\"a\",\"x\":1.5,\"y\":2}],\"zoom\":1.25,\"label\":\"日本語\"}"
        var o1 = FlowOperation(); o1.id = "op-1"; o1.sourceRemote = "local-src"; o1.sourcePath = "/Users/me/Documents"
        o1.targetRemote = "fake-s3"; o1.targetPath = "bucket/docs"; o1.action = "push"
        o1.syncConfig = JSONDict(["bandwidth": "10M", "parallel": 4,
                                  "filters": ["include": ["*.txt", "日本語/**"], "depth": 3, "nested": ["deep": ["x": 1.5, "ok": true]]] as [String: Any],
                                  "flags": [1, 2.5, true, NSNull(), "s"] as [Any]])
        o1.isExpanded = true
        var o2 = FlowOperation(); o2.id = "op-2"; o2.sourceRemote = "fake-s3"; o2.sourcePath = "bucket/docs"
        o2.targetRemote = "local-src"; o2.targetPath = "/tmp/restore"; o2.action = "bi"; o2.sortOrder = 5
        try store.saveFlow(&f1)  // placeholder so ordering below is deterministic
        f1.operations = [o1, o2]
        try store.saveFlow(&f1)

        var f2 = Flow()
        f2.id = "flow-2"; f2.name = "Ad-hoc ü"; f2.sortOrder = 1
        var p1 = FlowOperation(); p1.id = "op-3"; p1.sourceRemote = "local-src"; p1.targetRemote = "local-src"
        p1.sourcePath = "/"; p1.targetPath = "/"; p1.action = "bi-resync"
        var p2 = FlowOperation(); p2.id = "op-4"; p2.sourceRemote = "local-src"; p2.sourcePath = "/a"
        p2.targetRemote = "fake-s3"; p2.targetPath = "b"; p2.action = ""
        p2.syncConfig = JSONDict(["action": "push", "dryRun": true])
        var p3 = FlowOperation(); p3.id = "op-5"; p3.sourceRemote = "fake-s3"; p3.targetRemote = "local-src"
        p3.action = "push"
        f2.operations = [p1, p2, p3]
        try store.saveFlow(&f2)

        try store.recordFullSync(remoteKey: "gdrive:root", provider: "drive")
        try store.recordFullSync(remoteKey: "local:/tmp/x", provider: "local")
    }

    private func tmp() throws -> String {
        let d = NSTemporaryDirectory() + "gndrive-compat-" + UUID().uuidString
        try FileManager.default.createDirectory(atPath: d, withIntermediateDirectories: true)
        return d
    }

    // MARK: entrypoints

    func testGenerateFixtures() throws {
        guard let outDir = ProcessInfo.processInfo.environment["GN_DRIVE_GEN_FIXTURES"], !outDir.isEmpty else {
            throw XCTSkip("set GN_DRIVE_GEN_FIXTURES=<outdir> to generate compat fixtures")
        }
        let fm = FileManager.default
        let log = Logger(mode: .foreground)
        let lockedTmp = try tmp(), plainTmp = try tmp()
        defer { try? fm.removeItem(atPath: lockedTmp); try? fm.removeItem(atPath: plainTmp) }
        let rcText = Self.rcloneConf

        // locked
        try rcText.write(toFile: lockedTmp + "/rclone.conf", atomically: true, encoding: .utf8)
        let auth = try AuthService(configDir: lockedTmp, logger: log, keyStore: MemorySecureStore())
        try auth.setupPassword(Self.password)
        let ls = try Store(path: lockedTmp + "/gn-drive.db", logger: log)
        try populate(ls)
        let expected = try dump(ls)
        ls.close()
        // plain = byte copy of the same populated db (identical timestamps)
        try rcText.write(toFile: plainTmp + "/rclone.conf", atomically: true, encoding: .utf8)
        try fm.copyItem(atPath: lockedTmp + "/gn-drive.db", toPath: plainTmp + "/gn-drive.db")
        try auth.lock()
        XCTAssertTrue(fm.fileExists(atPath: lockedTmp + "/gn-drive.db.enc"))
        XCTAssertTrue(fm.fileExists(atPath: lockedTmp + "/rclone.conf.enc"))
        XCTAssertFalse(fm.fileExists(atPath: lockedTmp + "/gn-drive.db"))

        // copy out
        try? fm.removeItem(atPath: outDir + "/locked"); try? fm.removeItem(atPath: outDir + "/plain")
        try fm.createDirectory(atPath: outDir + "/locked", withIntermediateDirectories: true)
        try fm.createDirectory(atPath: outDir + "/plain", withIntermediateDirectories: true)
        try fm.createDirectory(atPath: outDir + "/expected", withIntermediateDirectories: true)
        for n in ["auth.json", "rclone.conf.enc", "gn-drive.db.enc"] {
            try fm.copyItem(atPath: lockedTmp + "/" + n, toPath: outDir + "/locked/" + n)
        }
        for n in ["gn-drive.db", "rclone.conf"] {
            try fm.copyItem(atPath: plainTmp + "/" + n, toPath: outDir + "/plain/" + n)
        }
        try rcText.write(toFile: outDir + "/expected/rclone.conf", atomically: true, encoding: .utf8)

        let authJSON = try JSONSerialization.jsonObject(
            with: Data(contentsOf: URL(fileURLWithPath: lockedTmp + "/auth.json"))) as! [String: Any]
        var top = expected
        top["password"] = Self.password
        top["passwordHash"] = authJSON["passwordHash"] ?? authJSON["password_hash"] ?? ""
        top["authJSONKeys"] = (authJSON.keys.sorted())
        top["rcloneConf"] = rcText
        try writeJSON(top, to: outDir + "/expected.json")
        try authJSONHashFile(authJSON, outDir)

        // self-check: unlock a COPY of locked and compare to expected
        let chk = try tmp(); defer { try? fm.removeItem(atPath: chk) }
        for n in ["auth.json", "rclone.conf.enc", "gn-drive.db.enc"] {
            try fm.copyItem(atPath: outDir + "/locked/" + n, toPath: chk + "/" + n)
        }
        let a2 = try AuthService(configDir: chk, logger: log, keyStore: MemorySecureStore())
        try a2.unlock(Self.password)
        let s2 = try Store(path: chk + "/gn-drive.db", logger: log)
        let d2 = try dump(s2); s2.close()
        let enc = { (o: [String: Any]) in try JSONSerialization.data(withJSONObject: o, options: [.sortedKeys]) }
        XCTAssertEqual(try enc(d2), try enc(expected), "locked dump differs from expected")
        XCTAssertEqual(try String(contentsOfFile: chk + "/rclone.conf", encoding: .utf8), rcText)
    }

    private func authJSONHashFile(_ a: [String: Any], _ outDir: String) throws {
        let h = (a["passwordHash"] ?? a["password_hash"] ?? "") as? String ?? ""
        try (h + "\n").write(toFile: outDir + "/expected/auth.json.hash", atomically: true, encoding: .utf8)
    }

    func testVerifyConfigDir() throws {
        guard let dir = ProcessInfo.processInfo.environment["GN_DRIVE_VERIFY_DIR"], !dir.isEmpty else {
            throw XCTSkip("set GN_DRIVE_VERIFY_DIR=<config dir copy> to dump it")
        }
        let log = Logger(mode: .foreground)
        let auth = try AuthService(configDir: dir, logger: log, keyStore: MemorySecureStore())
        if auth.isSetup { try auth.unlock(Self.password) }
        let store = try Store(path: dir + "/gn-drive.db", logger: log)
        var d = try dump(store)
        store.close()
        if let rc = try? String(contentsOfFile: dir + "/rclone.conf", encoding: .utf8) { d["rcloneConf"] = rc }
        d["authSetup"] = auth.isSetup
        try writeJSON(d, to: dir + "/swift-dump.json")
    }
}
