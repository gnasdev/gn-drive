// Method dispatcher for protocol v1 — every method in docs/gpui/rpc.md.
// Holds no transport state; RPCService owns framing and event pushes.
import Foundation
import Darwin

public final class RPCDispatcher: @unchecked Sendable {
    public let container: AppContainer
    public let logBuffer: RPCLogBuffer?
    public let rcloneBinaryOverride: String

    /// Bridge push for events originating above the bus (OAuth lifecycle).
    public var pushEvent: ((String, [String: Any]) -> Void)?

    private let stateLock = NSLock()
    private var providerCache: [[String: Any]]?
    private var oauthManager: RPCOAuthManager?

    private static let alwaysMethods: Set<String> = [
        "ping", "state.snapshot", "app.quit", "doctor", "logs.read",
        "auth.status", "auth.setup", "auth.unlock", "auth.lock",
        "auth.changePassword", "auth.removePassword",
        "remotes.providers", "settings.get", "settings.set",
        "service.status", "service.install", "service.uninstall", "service.start",
        "service.stop", "service.restart", "service.health",
        "update.check", "update.apply",
    ]

    private static let knownMethods: Set<String> = alwaysMethods.union([
        "flows.list", "flows.get", "flows.create", "flows.save", "flows.delete",
        "flows.execute", "flows.stop",
        "operations.create", "operations.update", "operations.delete", "operations.reorder",
        "runtime.snapshot", "runtime.tasks",
        "remotes.list", "remotes.add", "remotes.test", "remotes.delete",
        "remotes.oauth.start", "remotes.oauth.complete", "remotes.oauth.cancel",
        "fs.list",
        "profiles.list", "profiles.get", "profiles.save", "profiles.delete",
        "schedules.list", "sync.profile", "sync.stop",
        "boards.list", "boards.get", "boards.save", "boards.delete",
        "boards.execute", "boards.stop", "boards.status",
        "history.list", "history.stats", "history.clear",
    ])

    public init(container: AppContainer, rcloneBinary: String = "", logBuffer: RPCLogBuffer? = nil) {
        self.container = container
        self.rcloneBinaryOverride = rcloneBinary
        self.logBuffer = logBuffer
    }

    public func isLocked() -> Bool {
        container.auth.isSetup && !container.auth.isUnlocked && container.store == nil
    }

    // MARK: - Entry

    public func handle(method: String, params: [String: Any]) throws -> [String: Any] {
        guard Self.knownMethods.contains(method) else {
            throw RPCError("unknown_method", "unknown method: \(method)")
        }
        if !Self.alwaysMethods.contains(method), isLocked() {
            throw RPCError.locked()
        }
        let p = try RPCParams(params)
        switch method {
        case "ping": return ping()
        case "state.snapshot": return try stateSnapshot(p)
        case "app.quit": return ["quitting": true]
        case "doctor": return try doctor(p)
        case "logs.read": return try logsRead(p)

        case "auth.status": return RPCJSON.authStatus(container.auth.status())
        case "auth.setup": return try authSetup(p)
        case "auth.unlock": return try authUnlock(p)
        case "auth.lock": return try authLock()
        case "auth.changePassword": return try authChangePassword(p)
        case "auth.removePassword": return try authRemovePassword(p)

        case "flows.list": return try flowsList()
        case "flows.get": return try flowsGet(p)
        case "flows.create": return try flowsCreate(p)
        case "flows.save": return try flowsSave(p)
        case "flows.delete": return try flowsDelete(p)
        case "flows.execute": return try flowsExecute(p)
        case "flows.stop": return try flowsStop(p)
        case "operations.create": return try operationsCreate(p)
        case "operations.update": return try operationsUpdate(p)
        case "operations.delete": return try operationsDelete(p)
        case "operations.reorder": return try operationsReorder(p)

        case "runtime.snapshot": return RPCJSON.runtimeSnapshot(container.runtime.snapshot())
        case "runtime.tasks": return ["tasks": container.syncEngine.activeTasks().map(RPCJSON.task)]

        case "remotes.providers": return try remotesProviders(p)
        case "remotes.list": return try remotesList()
        case "remotes.add": return try remotesAdd(p)
        case "remotes.test": return try remotesTest(p)
        case "remotes.delete": return try remotesDelete(p)
        case "remotes.oauth.start": return try oauthStart(p)
        case "remotes.oauth.complete": return try oauthComplete(p)
        case "remotes.oauth.cancel": return try oauthCancel(p)

        case "fs.list": return try fsList(p)

        case "profiles.list": return try profilesList()
        case "profiles.get": return try profilesGet(p)
        case "profiles.save": return try profilesSave(p)
        case "profiles.delete": return try profilesDelete(p)
        case "schedules.list": return try schedulesList()

        case "sync.profile": return try syncProfile(p)
        case "sync.stop": return try syncStop(p)

        case "boards.list": return try boardsList()
        case "boards.get": return try boardsGet(p)
        case "boards.save": return try boardsSave(p)
        case "boards.delete": return try boardsDelete(p)
        case "boards.execute": return try boardsExecute(p)
        case "boards.stop": return try boardsStop(p)
        case "boards.status": return try boardsStatus(p)

        case "history.list": return try historyList(p)
        case "history.stats": return try historyStats()
        case "history.clear": return try historyClear()

        case "settings.get":
            return ["settings": RPCJSON.appSettings(container.auth.appSettings())]
        case "settings.set": return try settingsSet(p)

        case "service.status": return try serviceStatus(p)
        case "service.install": return try serviceInstall(p)
        case "service.uninstall": return try serviceUninstall(p)
        case "service.start": return try serviceAction(p, .start)
        case "service.stop": return try serviceAction(p, .stop)
        case "service.restart": return try serviceAction(p, .restart)
        case "service.health": return serviceHealth()

        case "update.check": return try updateCheck(p)
        case "update.apply": return try updateApply(p)

        default:
            throw RPCError("unknown_method", "unknown method: \(method)")
        }
    }

    /// Payload for the first frame.
    public func helloData(lockPath: String, pid: Int32) -> [String: Any] {
        let status = container.auth.status()
        let paths: [String: Any] = [
            "configDir": container.config.configDir,
            "dbPath": (container.config.configDir as NSString).appendingPathComponent("gn-drive.db"),
            "rcloneConfigPath": (container.config.configDir as NSString).appendingPathComponent("rclone.conf"),
            "logDir": container.config.logDir,
            "workingDir": container.config.workingDir,
        ]
        var rclone: [String: Any]? = nil
        let bin = (try? resolvedRcloneBinary()) ?? container.rclone?.binary
        if let bin {
            let version = (try? RcloneClient.resolveBinary(nil)).flatMap { _ in
                container.rclone.flatMap { try? $0.version() }
            }
            rclone = [
                "path": bin,
                "version": version ?? NSNull(),
                "bundled": bin == RcloneClient.bundledBinary(),
            ]
        }
        var hello: [String: Any] = [
            "protocol": RPCJSON.protocolVersion,
            "appVersion": container.version,
            "engine": "GNDriveCore",
            "pid": Int(pid),
            "setup": status.setup,
            "unlocked": status.unlocked,
            "passwordRequired": status.setup && !status.unlocked,
            "platform": ["os": "macos", "arch": Self.arch],
            "paths": paths,
            "capabilities": [
                "oauth": true, "providers": true, "profiles": true, "boards": true,
                "service": true, "update": true, "logs": true, "history": true, "doctor": true,
            ],
            "instance": ["pid": Int(pid), "lockPath": lockPath],
        ]
        if let rclone { hello["rclone"] = rclone }
        return hello
    }

    private static var arch: String {
        #if arch(arm64)
        return "arm64"
        #else
        return "x86_64"
        #endif
    }

    // MARK: - System

    private func ping() -> [String: Any] {
        [
            "pong": true,
            "protocol": RPCJSON.protocolVersion,
            "version": container.version,
            "now": RPCJSON.rfc3339(Date()),
        ]
    }

    private func stateSnapshot(_ p: RPCParams) throws -> [String: Any] {
        let requested = try p.stringArray("sections")
        let all: Set<String> = ["auth", "flows", "remotes", "profiles", "history", "settings", "runtime"]
        let sections = requested.map { Set($0).intersection(all) } ?? all
        let status = container.auth.status()
        let locked = !status.unlocked

        var out: [String: Any] = [
            "locked": locked,
            "auth": RPCJSON.authStatus(status),
            "version": container.version,
            "paths": [
                "configDir": container.config.configDir,
                "dbPath": (container.config.configDir as NSString).appendingPathComponent("gn-drive.db"),
                "rcloneConfigPath": (container.config.configDir as NSString).appendingPathComponent("rclone.conf"),
                "logDir": container.config.logDir,
            ],
        ]
        if sections.contains("settings") {
            out["appSettings"] = RPCJSON.appSettings(container.auth.appSettings())
        }
        if sections.contains("runtime") {
            out["runtime"] = locked
                ? ["revision": 0, "flows": []] as [String: Any]
                : RPCJSON.runtimeSnapshot(container.runtime.snapshot())
        }
        guard !locked, let store = container.store else {
            if sections.contains("flows") { out["flows"] = [] }
            if sections.contains("remotes") { out["remotes"] = [] }
            if sections.contains("profiles") { out["profiles"] = [] }
            if sections.contains("history") { out["history"] = [] }
            return out
        }
        if sections.contains("flows") { out["flows"] = (try? store.listFlows())?.map(RPCJSON.flow) ?? [] }
        if sections.contains("remotes") {
            out["remotes"] = (try? container.rclone?.listRemotes())?.map(RPCJSON.remote) ?? []
        }
        if sections.contains("profiles") { out["profiles"] = (try? store.listProfiles())?.map(RPCJSON.profile) ?? [] }
        if sections.contains("history") {
            out["history"] = (try? store.listHistory(limit: 100, offset: 0))?.map(RPCJSON.history) ?? []
        }
        if let version = try? container.rclone?.version() { out["rcloneVersion"] = version }
        return out
    }

    private func doctor(_ p: RPCParams) throws -> [String: Any] {
        let includeFiles = try p.bool("includeDataFiles") ?? false
        var checks: [[String: Any]] = []
        func add(_ id: String, _ label: String, _ status: String, _ detail: String) {
            checks.append(["id": id, "label": label, "status": status, "detail": detail])
        }

        let bin = (try? resolvedRcloneBinary()) ?? container.rclone?.binary
        if let bin {
            add("rclone_binary", "rclone binary", "ok", bin)
            let version = container.rclone.flatMap { try? $0.version() } ?? Self.versionOf(bin) ?? "unknown"
            add("rclone_version", "rclone version",
                version == "unknown" ? "warn" : "ok", version)
        } else {
            add("rclone_binary", "rclone binary", "error", "not found")
        }

        let cfgDir = container.config.configDir
        let exists = FileManager.default.fileExists(atPath: cfgDir)
        add("config_dir", "config directory", exists ? "ok" : "error", cfgDir)
        let db = (cfgDir as NSString).appendingPathComponent("gn-drive.db")
        add("database", "database",
            FileManager.default.fileExists(atPath: db) ? "ok" : "info",
            FileManager.default.fileExists(atPath: db) ? "gn-drive.db" : "gn-drive.db (not created yet)")

        let status = container.auth.status()
        let authText = status.setup
            ? "configured: yes, unlocked: \(status.unlocked ? "yes" : "no")"
            : "configured: no"
        add("auth", "auth", "info", authText)

        if isLocked() {
            add("remotes", "remotes", "info", "locked")
            add("profiles", "profiles", "info", "locked")
        } else {
            if let remotes = try? container.rclone?.listRemotes() {
                add("remotes", "remotes", "info", "\(remotes.count) configured")
            } else {
                add("remotes", "remotes", "warn", "unavailable")
            }
            if let profiles = try? container.store?.listProfiles() {
                add("profiles", "profiles", "info", "\(profiles.count) configured")
            } else {
                add("profiles", "profiles", "warn", "unavailable")
            }
        }
        add("platform", "platform", "ok", "darwin \(Self.arch)")

        var out: [String: Any] = [
            "ok": !checks.contains { ($0["status"] as? String) == "error" },
            "checks": checks,
            "platform": ["os": "macos", "arch": Self.arch],
        ]
        if includeFiles {
            out["dataFiles"] = ((try? FileManager.default.contentsOfDirectory(atPath: cfgDir)) ?? []).sorted()
        }
        return out
    }

    private static func versionOf(_ bin: String) -> String? {
        let proc = Process()
        proc.executableURL = URL(fileURLWithPath: bin)
        proc.arguments = ["version"]
        let pipe = Pipe()
        proc.standardOutput = pipe
        proc.standardError = pipe
        guard (try? proc.run()) != nil else { return nil }
        let data = pipe.fileHandleForReading.readDataToEndOfFile()
        proc.waitUntilExit()
        guard proc.terminationStatus == 0 else { return nil }
        let text = String(data: data, encoding: .utf8) ?? ""
        return text.split(separator: "\n").first.map(String.init)?
            .trimmingCharacters(in: .whitespaces)
    }

    private func logsRead(_ p: RPCParams) throws -> [String: Any] {
        var limit = try p.int("limit") ?? 200
        if limit < 0 { throw RPCError.invalidParams("limit") }
        limit = min(limit, 2000)
        var minLevel: LogLevel? = nil
        if let level = try p.string("level") {
            guard let parsed = LogLevel(label: level) else {
                throw RPCError.invalidParams("level")
            }
            minLevel = parsed
        }
        let since = try p.int("since").map(Int64.init)
        guard let logBuffer else { return ["lines": [], "dropped": 0] }
        let result = logBuffer.read(limit: limit, minLevel: minLevel, since: since)
        return [
            "lines": result.lines.map(logBuffer.encode),
            "dropped": result.dropped,
        ]
    }

    // MARK: - Auth

    private func authSetup(_ p: RPCParams) throws -> [String: Any] {
        let password = try p.requireString("password")
        try container.auth.setupPassword(password)
        try container.afterUnlock()
        container.bus.publish(BusTopic.authUnlocked, AuthUnlockedEvent())
        return RPCJSON.authStatus(container.auth.status())
    }

    private func authUnlock(_ p: RPCParams) throws -> [String: Any] {
        let password = try p.requireString("password")
        do {
            try container.auth.unlock(password)
        } catch AuthError.invalidPassword {
            let lockout = container.auth.lockoutStatus()
            throw RPCError("invalid_password", "auth: invalid password",
                           data: ["failedAttempts": lockout.failedAttempts])
        } catch AuthError.locked(let secs) {
            let lockout = container.auth.lockoutStatus()
            throw RPCError("locked_out", "auth: locked — retry in \(secs)s",
                           data: ["retryAfterSecs": secs, "lockedUntil": lockout.lockedUntil])
        }
        try container.afterUnlock()
        container.bus.publish(BusTopic.authUnlocked, AuthUnlockedEvent())
        return RPCJSON.authStatus(container.auth.status())
    }

    private func authLock() throws -> [String: Any] {
        container.beforeLock()
        try container.auth.lock()
        container.bus.publish(BusTopic.authLocked, AuthLockedEvent())
        return RPCJSON.authStatus(container.auth.status())
    }

    private func authChangePassword(_ p: RPCParams) throws -> [String: Any] {
        let old = try p.requireString("oldPassword")
        let new = try p.requireString("newPassword")
        try container.auth.changePassword(old: old, new: new)
        return RPCJSON.authStatus(container.auth.status())
    }

    private func authRemovePassword(_ p: RPCParams) throws -> [String: Any] {
        let password = try p.requireString("password")
        container.beforeLock()
        try container.auth.removePassword(password)
        try container.afterUnlock()
        container.bus.publish(BusTopic.authUnlocked, AuthUnlockedEvent())
        return RPCJSON.authStatus(container.auth.status())
    }

    // MARK: - Flows

    private func flowsList() throws -> [String: Any] {
        let store = try requireStore()
        return ["flows": try store.listFlows().map(RPCJSON.flow)]
    }

    private func flowsGet(_ p: RPCParams) throws -> [String: Any] {
        let store = try requireStore()
        let id = try p.requireString("id")
        let flow = try notFound("flow_not_found", "flow \(id) not found") { try store.getFlow(id) }
        return ["flow": RPCJSON.flow(flow)]
    }

    private func flowsCreate(_ p: RPCParams) throws -> [String: Any] {
        let store = try requireStore()
        let name = try p.string("name") ?? ""
        let id = try p.string("id") ?? ""
        let flowID = id.isEmpty ? UUID().uuidString : id
        if (try? store.getFlow(flowID)) != nil {
            throw RPCError("flow_exists", "flow \(flowID) already exists")
        }
        var f = Flow()
        f.id = flowID
        f.name = name.trimmingCharacters(in: .whitespaces).isEmpty ? "Untitled flow" : name
        if let canvasJSON = try p.string("canvasJSON"), !canvasJSON.isEmpty {
            f.canvasJSON = canvasJSON
        } else if let canvas = try p.dict("canvas") {
            f.canvasJSON = RPCJSON.stringifyJSON(canvas)
        } else {
            f.canvasJSON = "{}"
        }
        try store.saveFlow(&f)
        let saved = try store.getFlow(flowID)
        container.bus.publish(BusTopic.stateChanged, StateChangedEvent(domain: "flows", id: flowID))
        return ["flow": RPCJSON.flow(saved)]
    }

    private func flowsSave(_ p: RPCParams) throws -> [String: Any] {
        let store = try requireStore()
        guard let raw = try p.dict("flow") else { throw RPCError.invalidParams("flow") }
        var flow = try parseFlow(raw)
        guard !flow.id.isEmpty else { throw RPCError.invalidParams("flow.id") }
        try ensureFlowMutable(flow.id)

        for i in flow.operations.indices {
            if flow.operations[i].id.isEmpty { flow.operations[i].id = UUID().uuidString }
            flow.operations[i].flowID = flow.id
            if flow.operations[i].sortOrder == 0 { flow.operations[i].sortOrder = i }
            flow.operations[i].normalizeAction()
        }

        var warnings: [[String: Any]] = []
        if flow.scheduleEnabled, !flow.scheduleCron.trimmingCharacters(in: .whitespaces).isEmpty,
           (try? CronSchedule(flow.scheduleCron)) == nil {
            warnings.append(["code": "invalid_cron",
                             "message": "invalid cron expression: \(flow.scheduleCron)"])
        }

        try store.saveFlow(&flow)
        container.syncEngine.syncFlowSchedule(flow)
        container.bus.publish(BusTopic.stateChanged, StateChangedEvent(domain: "flows", id: flow.id))
        let saved = try store.getFlow(flow.id)
        return ["flow": RPCJSON.flow(saved), "warnings": warnings]
    }

    private func flowsDelete(_ p: RPCParams) throws -> [String: Any] {
        let store = try requireStore()
        let id = try p.requireString("id")
        _ = try notFound("flow_not_found", "flow \(id) not found") { try store.getFlow(id) }
        try ensureFlowMutable(id)
        try store.deleteFlow(id)
        container.syncEngine.unregisterFlowSchedule(id)
        container.runtime.forget(id)
        container.bus.publish(BusTopic.stateChanged, StateChangedEvent(domain: "flows", id: id))
        return ["deleted": true, "id": id]
    }

    private func flowsExecute(_ p: RPCParams) throws -> [String: Any] {
        let id = try p.requireString("id")
        try container.flowEngine.execute(flowID: id)
        return ["flowID": id, "status": "running"]
    }

    private func flowsStop(_ p: RPCParams) throws -> [String: Any] {
        let id = try p.requireString("id")
        try container.flowEngine.stop(flowID: id)
        return ["flowID": id, "status": "cancelling"]
    }

    private func operationsCreate(_ p: RPCParams) throws -> [String: Any] {
        let store = try requireStore()
        let flowID = try p.requireString("flowID")
        guard let raw = try p.dict("operation") else { throw RPCError.invalidParams("operation") }
        var flow = try notFound("flow_not_found", "flow \(flowID) not found") { try store.getFlow(flowID) }
        try ensureFlowMutable(flowID)
        var op = try parseOperation(raw, flowID: flowID)
        if op.id.isEmpty { op.id = UUID().uuidString }
        op.sortOrder = (flow.operations.map(\.sortOrder).max() ?? -1) + 1
        op.normalizeAction()
        flow.operations.append(op)
        try store.saveFlow(&flow)
        container.syncEngine.syncFlowSchedule(flow)
        container.bus.publish(BusTopic.stateChanged, StateChangedEvent(domain: "flows", id: flowID))
        let savedFlow = try store.getFlow(flowID)
        let savedOp = savedFlow.operations.first { $0.id == op.id } ?? op
        return ["flow": RPCJSON.flow(savedFlow), "operation": RPCJSON.operation(savedOp)]
    }

    private func operationsUpdate(_ p: RPCParams) throws -> [String: Any] {
        let store = try requireStore()
        let flowID = try p.requireString("flowID")
        let opID = try p.requireString("operationID")
        guard let patch = try p.dict("patch") else { throw RPCError.invalidParams("patch") }
        var flow = try notFound("flow_not_found", "flow \(flowID) not found") { try store.getFlow(flowID) }
        try ensureFlowMutable(flowID)
        guard let idx = flow.operations.firstIndex(where: { $0.id == opID }) else {
            throw RPCError("operation_not_found", "operation \(opID) not found")
        }
        var merged = RPCJSON.operation(flow.operations[idx])
        for (k, v) in patch where !["id", "flowID", "sortOrder"].contains(k) {
            merged[k] = v
        }
        var op = try parseOperation(merged, flowID: flowID)
        op.id = opID
        op.sortOrder = flow.operations[idx].sortOrder
        op.normalizeAction()
        flow.operations[idx] = op
        try store.saveFlow(&flow)
        container.syncEngine.syncFlowSchedule(flow)
        container.bus.publish(BusTopic.stateChanged, StateChangedEvent(domain: "flows", id: flowID))
        let savedFlow = try store.getFlow(flowID)
        let savedOp = savedFlow.operations.first { $0.id == opID } ?? op
        return ["flow": RPCJSON.flow(savedFlow), "operation": RPCJSON.operation(savedOp)]
    }

    private func operationsDelete(_ p: RPCParams) throws -> [String: Any] {
        let store = try requireStore()
        let flowID = try p.requireString("flowID")
        let opID = try p.requireString("operationID")
        var flow = try notFound("flow_not_found", "flow \(flowID) not found") { try store.getFlow(flowID) }
        try ensureFlowMutable(flowID)
        guard flow.operations.contains(where: { $0.id == opID }) else {
            throw RPCError("operation_not_found", "operation \(opID) not found")
        }
        flow.operations.removeAll { $0.id == opID }
        for i in flow.operations.indices { flow.operations[i].sortOrder = i }
        try store.saveFlow(&flow)
        container.syncEngine.syncFlowSchedule(flow)
        container.bus.publish(BusTopic.stateChanged, StateChangedEvent(domain: "flows", id: flowID))
        return ["flow": RPCJSON.flow(try store.getFlow(flowID))]
    }

    private func operationsReorder(_ p: RPCParams) throws -> [String: Any] {
        let store = try requireStore()
        let flowID = try p.requireString("flowID")
        let ids = try p.stringArray("operationIDs") ?? []
        var flow = try notFound("flow_not_found", "flow \(flowID) not found") { try store.getFlow(flowID) }
        try ensureFlowMutable(flowID)
        let existing = flow.operations.map(\.id)
        guard ids.count == existing.count, Set(ids).count == ids.count,
              Set(ids) == Set(existing) else {
            throw RPCError.invalidParams("operationIDs", "operationIDs must be a permutation of the flow's operations")
        }
        var byID: [String: FlowOperation] = [:]
        for op in flow.operations { byID[op.id] = op }
        var reordered: [FlowOperation] = []
        for (i, id) in ids.enumerated() {
            guard var op = byID[id] else { throw RPCError.invalidParams("operationIDs") }
            op.sortOrder = i
            reordered.append(op)
        }
        flow.operations = reordered
        try store.saveFlow(&flow)
        container.syncEngine.syncFlowSchedule(flow)
        container.bus.publish(BusTopic.stateChanged, StateChangedEvent(domain: "flows", id: flowID))
        return ["flow": RPCJSON.flow(try store.getFlow(flowID))]
    }

    // MARK: - Remotes

    private func remotesProviders(_ p: RPCParams) throws -> [String: Any] {
        let refresh = try p.bool("refresh") ?? false
        stateLock.lock()
        let cached = providerCache
        stateLock.unlock()
        let fromCache = !refresh && cached != nil
        let raw: [[String: Any]]
        if let cached, fromCache {
            raw = cached
        } else {
            let rc = try adminRclone()
            raw = try rc.providers()
            stateLock.lock(); providerCache = raw; stateLock.unlock()
        }
        let version: Any = (try? adminRclone().version()) ?? NSNull()
        return [
            "rcloneVersion": version,
            "providers": raw.map(RPCJSON.provider),
            "cached": fromCache,
        ]
    }

    private func remotesList() throws -> [String: Any] {
        let rc = try requireRclone()
        do {
            return ["remotes": try rc.listRemotes().map(RPCJSON.remote)]
        } catch {
            throw rpcMapError(error)
        }
    }

    private func remotesAdd(_ p: RPCParams) throws -> [String: Any] {
        let rc = try requireRclone()
        let name = try p.requireString("name")
        let type = try p.requireString("type")
        let kvs = try p.configKVs()
        if try remoteExists(rc, name) {
            throw RPCError("remote_exists", "remote \(name) already exists")
        }

        let res = try rc.createRemoteNonInteractive(name: name, type: type, configKVs: kvs)
        if let q = Self.question(from: res.stdout) {
            let state = RPCJSON.stringOf(RPCJSON.field(q, "State"))
            let option = RPCJSON.field(q, "Option")
            let hasOption = option != nil && !(option is NSNull)
            if hasOption {
                try? rc.deleteRemote(name)
                let normalized = RPCJSON.normalizedQuestion(q)
                if state.lowercased().contains("oauth") {
                    throw RPCError("oauth_required",
                                   "remote \(name) requires OAuth authorization",
                                   data: ["state": state, "questions": [normalized]])
                }
                throw RPCError("invalid_params",
                               "remote \(name) requires additional options",
                               data: ["field": "config", "question": normalized])
            }
        }
        if res.exitCode != 0 {
            try? rc.deleteRemote(name)
            let msg = res.stderr.isEmpty ? res.stdout : res.stderr
            throw RPCError("rclone_failed", "rclone config create failed: \(RPCRedact.stderrField(msg))",
                           data: ["command": "config create", "stderr": RPCRedact.stderrField(msg)])
        }
        do {
            try rc.testRemote(name)
        } catch {
            try? rc.deleteRemote(name)
            let msg: String
            if case RcloneError.runFailed(_, let stderr) = error { msg = stderr } else { msg = String(describing: error) }
            throw RPCError("auth_failed", "remote \(name) could not be verified: \(RPCRedact.stderrField(msg))",
                           data: ["stderr": RPCRedact.stderrField(msg)])
        }
        container.bus.publish(BusTopic.stateChanged, StateChangedEvent(domain: "remotes", id: name))
        return ["remote": RPCJSON.remote(Remote(name: name, type: type, description: "")),
                "verified": true]
    }

    private func remotesTest(_ p: RPCParams) throws -> [String: Any] {
        let rc = try requireRclone()
        let name = try p.requireString("name")
        guard try remoteExists(rc, name) else {
            throw RPCError("remote_not_found", "remote \(name) not found")
        }
        do {
            try rc.testRemote(name)
        } catch RcloneError.runFailed(_, let stderr) {
            throw RPCError("test_failed", "remote \(name) failed: \(RPCRedact.stderrField(stderr))",
                           data: ["stderr": RPCRedact.stderrField(stderr)])
        }
        return ["name": name, "ok": true]
    }

    private func remotesDelete(_ p: RPCParams) throws -> [String: Any] {
        let rc = try requireRclone()
        let name = try p.requireString("name")
        guard try remoteExists(rc, name) else {
            throw RPCError("remote_not_found", "remote \(name) not found")
        }
        try rc.deleteRemote(name)
        container.bus.publish(BusTopic.stateChanged, StateChangedEvent(domain: "remotes", id: name))
        return ["deleted": true, "name": name]
    }

    private func oauthStart(_ p: RPCParams) throws -> [String: Any] {
        let type = try p.requireString("type")
        let clientID = try p.string("clientID")
        let clientSecret = try p.string("clientSecret")
        guard (clientID == nil) == (clientSecret == nil) else {
            throw RPCError.invalidParams("clientID", "clientID and clientSecret must be supplied together")
        }
        let manager = try oauth()
        let session = try manager.start(type: type, clientID: clientID, clientSecret: clientSecret)
        session.lock.lock()
        let url = session.authURL
        session.lock.unlock()
        var out: [String: Any] = [
            "sessionID": session.id,
            "type": type,
            "expiresAt": RPCJSON.rfc3339(session.startedAt.addingTimeInterval(RPCOAuthManager.sessionLifetime)),
        ]
        out["authURL"] = url ?? NSNull()
        return out
    }

    private func oauthComplete(_ p: RPCParams) throws -> [String: Any] {
        let rc = try requireRclone()
        let sessionID = try p.requireString("sessionID")
        let name = try p.requireString("name")
        let config = try p.dict("config") ?? [:]
        let manager = try oauth()
        guard let session = manager.session(sessionID) else {
            throw RPCError("oauth_session_not_found", "OAuth session \(sessionID) not found")
        }
        if try remoteExists(rc, name) {
            throw RPCError("remote_exists", "remote \(name) already exists")
        }

        // Wait briefly for the authorize child to hand over its token.
        let deadline = Date().addingTimeInterval(5)
        while Date() < deadline {
            let snap = session.snapshot()
            if snap.status != "waiting" { break }
            Thread.sleep(forTimeInterval: 0.05)
        }
        let snap = session.snapshot()
        switch snap.status {
        case "cancelled":
            throw RPCError("oauth_cancelled", "OAuth session \(sessionID) was cancelled")
        case "failed":
            throw RPCError("oauth_failed", snap.error.isEmpty ? "OAuth authorization failed" : snap.error)
        case "authorized":
            break
        default:
            throw RPCError("oauth_cancelled", "OAuth authorization has not completed yet")
        }
        let token = snap.token
        guard !token.isEmpty else {
            throw RPCError("oauth_failed", "OAuth session \(sessionID) has no token")
        }

        let type = try p.string("type") ?? session.type
        let kvs = config.keys.sorted().map { "\($0)=\(RPCJSON.stringOf(config[$0]))" }

        func cleanup() { try? rc.deleteRemote(name) }

        do {
            var current = try rc.createRemoteNonInteractive(name: name, type: type, configKVs: kvs)
            var steps = 0
            while true {
                guard let q = Self.question(from: current.stdout) else {
                    if current.exitCode != 0 {
                        cleanup()
                        throw RPCError("oauth_failed",
                                       "remote creation failed: \(RPCRedact.stderrField(current.stderr))")
                    }
                    break
                }
                let state = RPCJSON.stringOf(RPCJSON.field(q, "State"))
                guard let option = RPCJSON.field(q, "Option") as? [String: Any] else {
                    if state.isEmpty && current.exitCode == 0 { break }
                    cleanup()
                    throw RPCError("oauth_failed", "remote creation stopped at state \(state)",
                                   data: ["question": RPCJSON.normalizedQuestion(q)])
                }
                steps += 1
                if steps > 8 {
                    cleanup()
                    throw RPCError("oauth_failed", "too many configuration steps")
                }
                let optionName = RPCJSON.stringOf(RPCJSON.field(option, "Name"))
                let answer: String
                if optionName == "config_token"
                    || (state.lowercased().contains("oauth-authorize") && optionName.contains("token")) {
                    answer = token
                } else if optionName == "config_shared_client_id" {
                    answer = "true"
                } else if optionName == "config_is_local" {
                    answer = "false"
                } else if let v = config[optionName] {
                    answer = RPCJSON.stringOf(v)
                } else {
                    cleanup()
                    throw RPCError("oauth_failed", "unexpected configuration question: \(optionName)",
                                   data: ["question": RPCJSON.normalizedQuestion(q)])
                }
                current = try rc.continueRemoteCreate(name: name, type: type, state: state,
                                                      result: answer, configKVs: kvs)
                if current.exitCode != 0 {
                    cleanup()
                    throw RPCError("oauth_failed",
                                   "remote creation failed: \(RPCRedact.stderrField(current.stderr))")
                }
            }
        } catch {
            if error is RPCError {
                manager.forget(sessionID)
                throw error
            }
            cleanup()
            manager.forget(sessionID)
            throw RPCError("oauth_failed", RPCRedact.sanitize(String(describing: error)))
        }

        do {
            try rc.testRemote(name)
        } catch {
            cleanup()
            manager.forget(sessionID)
            let msg: String
            if case RcloneError.runFailed(_, let stderr) = error { msg = stderr } else { msg = String(describing: error) }
            throw RPCError("test_failed", "remote \(name) could not be verified: \(RPCRedact.stderrField(msg))",
                           data: ["stderr": RPCRedact.stderrField(msg)])
        }
        manager.forget(sessionID)
        container.bus.publish(BusTopic.stateChanged, StateChangedEvent(domain: "remotes", id: name))
        return ["remote": RPCJSON.remote(Remote(name: name, type: type, description: "")),
                "verified": true]
    }

    private func oauthCancel(_ p: RPCParams) throws -> [String: Any] {
        let sessionID = try p.requireString("sessionID")
        let manager = try oauth()
        guard manager.session(sessionID) != nil else {
            throw RPCError("oauth_session_not_found", "OAuth session \(sessionID) not found")
        }
        _ = manager.cancel(sessionID)
        manager.forget(sessionID)
        return ["cancelled": true, "sessionID": sessionID]
    }

    private func fsList(_ p: RPCParams) throws -> [String: Any] {
        let rc = try requireRclone()
        let path = try p.requireString("path")
        let recursive = try p.bool("recursive") ?? false
        var limit = try p.int("limit") ?? 150
        if limit < 0 { throw RPCError.invalidParams("limit") }
        limit = min(limit, 5000)
        guard path.contains(":") || path.hasPrefix("/") else {
            throw RPCError("fs_missing_path", "path must be absolute or remote:path")
        }
        do {
            var entries = try recursive
                ? rc.listFilesForPending(path, limit: limit + 1)
                : rc.listFiles(path)
            var truncated = false
            if entries.count > limit {
                entries = Array(entries.prefix(limit))
                truncated = true
            }
            return [
                "path": path,
                "entries": entries.map(RPCJSON.fileEntry),
                "truncated": truncated,
            ]
        } catch RcloneError.runFailed(_, let stderr) {
            throw RPCError("browse_failed", "could not list \(path): \(RPCRedact.stderrField(stderr))",
                           data: ["path": path, "stderr": RPCRedact.stderrField(stderr)])
        }
    }

    // MARK: - Profiles / schedules

    private func profilesList() throws -> [String: Any] {
        let store = try requireStore()
        return ["profiles": try store.listProfiles().map(RPCJSON.profile)]
    }

    private func profilesGet(_ p: RPCParams) throws -> [String: Any] {
        let store = try requireStore()
        let name = try p.requireString("name")
        let profile = try notFound("profile_not_found", "profile \(name) not found") {
            try store.getProfile(name)
        }
        return ["profile": RPCJSON.profile(profile)]
    }

    private func profilesSave(_ p: RPCParams) throws -> [String: Any] {
        let store = try requireStore()
        guard let raw = try p.dict("profile") else { throw RPCError.invalidParams("profile") }
        var profile = try parseProfile(raw)
        try store.saveProfile(&profile)
        container.bus.publish(BusTopic.stateChanged, StateChangedEvent(domain: "profiles", id: profile.name))
        return ["profile": RPCJSON.profile(try store.getProfile(profile.name))]
    }

    private func profilesDelete(_ p: RPCParams) throws -> [String: Any] {
        let store = try requireStore()
        let name = try p.requireString("name")
        try notFound("profile_not_found", "profile \(name) not found") { try store.deleteProfile(name) }
        container.bus.publish(BusTopic.stateChanged, StateChangedEvent(domain: "profiles", id: name))
        return ["deleted": true, "name": name]
    }

    private func schedulesList() throws -> [String: Any] {
        let store = try requireStore()
        return ["schedules": try store.listSchedules().map(RPCJSON.schedule)]
    }

    // MARK: - Sync

    private func syncProfile(_ p: RPCParams) throws -> [String: Any] {
        let store = try requireStore()
        let name = try p.requireString("profile")
        var profile = try notFound("profile_not_found", "profile \(name) not found") {
            try store.getProfile(name)
        }
        let requested = try p.string("action") ?? ""
        let action: String
        if requested.isEmpty {
            action = ProfileDirection.normalize(profile.direction)
        } else {
            guard ["pull", "push", "bi", "bi-resync", "dry-run"].contains(requested) else {
                throw RPCError("invalid_action", "unknown sync action: \(requested)")
            }
            action = requested
        }
        let dryRunOverride = try p.bool("dryRun") ?? false
        let wait = try p.bool("wait") ?? false

        let taskID: String
        do {
            if dryRunOverride {
                profile.dryRun = true
                taskID = try container.syncEngine.startPathSync(
                    action: action, busyKey: profile.name,
                    from: profile.from, to: profile.to, opts: profile)
            } else {
                taskID = try container.syncEngine.startSync(action: action, profileName: profile.name)
            }
        } catch SyncEngineError.taskFailed(let msg) {
            throw RPCError("task_failed", RPCRedact.sanitize(msg))
        }

        if wait {
            do {
                try container.syncEngine.waitTask(taskID)
            } catch SyncEngineError.taskFailed(let msg) {
                throw RPCError("task_failed", RPCRedact.sanitize(msg), data: ["taskID": taskID])
            } catch SyncEngineError.taskCancelled {
                throw RPCError("task_cancelled", "task was cancelled", data: ["taskID": taskID])
            }
            return ["taskID": taskID, "profile": name, "action": action, "state": "completed"]
        }
        return ["taskID": taskID, "profile": name, "action": action, "state": "running"]
    }

    private func syncStop(_ p: RPCParams) throws -> [String: Any] {
        let taskID = try p.requireString("taskID")
        do {
            try container.syncEngine.stopSync(taskID)
        } catch SyncEngineError.taskNotFound {
            throw RPCError("task_not_found", "task \(taskID) not found")
        }
        return ["cancelled": true, "taskID": taskID]
    }

    // MARK: - Boards

    private func boardsList() throws -> [String: Any] {
        let store = try requireStore()
        let boards = try store.listBoards().map {
            ["id": $0.id, "name": $0.name, "createdAt": $0.createdAt, "updatedAt": $0.updatedAt] as [String: Any]
        }
        return ["boards": boards]
    }

    private func boardsGet(_ p: RPCParams) throws -> [String: Any] {
        let store = try requireStore()
        let id = try p.requireString("id")
        let board = try notFound("board_not_found", "board \(id) not found") {
            try store.loadBoardGraph(id)
        }
        return ["board": RPCJSON.board(board)]
    }

    private func boardsSave(_ p: RPCParams) throws -> [String: Any] {
        let store = try requireStore()
        guard let raw = try p.dict("board") else { throw RPCError.invalidParams("board") }
        var board = try parseBoard(raw)
        guard !board.id.trimmingCharacters(in: .whitespaces).isEmpty else {
            throw RPCError.invalidParams("board.id")
        }
        guard !board.name.trimmingCharacters(in: .whitespaces).isEmpty else {
            throw RPCError.invalidParams("board.name")
        }
        for i in board.nodes.indices where board.nodes[i].id.isEmpty {
            throw RPCError.invalidParams("board.nodes[\(i)].id")
        }
        for i in board.edges.indices where board.edges[i].id.isEmpty {
            board.edges[i].id = UUID().uuidString
        }
        try store.saveBoardGraph(board)
        return ["board": RPCJSON.board(try store.loadBoardGraph(board.id))]
    }

    private func boardsDelete(_ p: RPCParams) throws -> [String: Any] {
        let store = try requireStore()
        let id = try p.requireString("id")
        try notFound("board_not_found", "board \(id) not found") { try store.deleteBoard(id) }
        return ["deleted": true, "id": id]
    }

    private func boardsExecute(_ p: RPCParams) throws -> [String: Any] {
        let store = try requireStore()
        let id = try p.requireString("id")
        _ = try notFound("board_not_found", "board \(id) not found") { try store.getBoard(id) }
        let stopOnError = try p.bool("stopOnError") ?? true
        let runID = try container.boardEngine.execute(boardID: id, stopOnError: stopOnError)
        return ["runID": runID, "boardID": id, "status": "running"]
    }

    private func boardsStop(_ p: RPCParams) throws -> [String: Any] {
        let store = try requireStore()
        let id = try p.requireString("id")
        _ = try notFound("board_not_found", "board \(id) not found") { try store.getBoard(id) }
        try container.boardEngine.stop(id)
        return ["boardID": id, "status": "cancelling"]
    }

    private func boardsStatus(_ p: RPCParams) throws -> [String: Any] {
        let store = try requireStore()
        let id = try p.requireString("id")
        _ = try notFound("board_not_found", "board \(id) not found") { try store.getBoard(id) }
        guard let st = container.boardEngine.status(id) else { return ["run": NSNull()] }
        return ["run": ["runID": st.runID, "boardID": st.boardID,
                        "status": st.status, "error": st.error]]
    }

    // MARK: - History

    private func historyList(_ p: RPCParams) throws -> [String: Any] {
        let store = try requireStore()
        var limit = try p.int("limit") ?? 20
        if limit < 0 { throw RPCError.invalidParams("limit") }
        limit = min(limit, 1000)
        let offset = try p.int("offset") ?? 0
        if offset < 0 { throw RPCError.invalidParams("offset") }
        let profile = try p.string("profile")
        let entries: [HistoryEntry]
        if let profile, !profile.isEmpty {
            entries = try store.listHistory(profile: profile, limit: limit, offset: offset)
        } else {
            entries = try store.listHistory(limit: limit, offset: offset)
        }
        return ["entries": entries.map(RPCJSON.history), "total": NSNull()]
    }

    private func historyStats() throws -> [String: Any] {
        let store = try requireStore()
        return RPCJSON.historyStats(try store.historyStats())
    }

    private func historyClear() throws -> [String: Any] {
        let store = try requireStore()
        try store.clearHistory()
        return ["cleared": true]
    }

    // MARK: - Settings

    private func settingsSet(_ p: RPCParams) throws -> [String: Any] {
        guard let patch = try p.dict("settings") else { throw RPCError.invalidParams("settings") }
        var settings = container.auth.appSettings()
        if let v = try optionalBool(patch, "notificationsEnabled") { settings.notificationsEnabled = v }
        if let v = try optionalBool(patch, "debugMode") { settings.debugMode = v }
        if let v = try optionalBool(patch, "minimizeToTray") { settings.minimizeToTray = v }
        if let v = try optionalBool(patch, "startAtLogin") { settings.startAtLogin = v }
        try container.auth.setAppSettings(settings)
        return ["settings": RPCJSON.appSettings(settings)]
    }

    private func optionalBool(_ dict: [String: Any], _ key: String) throws -> Bool? {
        guard let v = dict[key], !(v is NSNull) else { return nil }
        guard let b = RPCJSON.boolValue(v) else { throw RPCError.invalidParams("settings.\(key)") }
        return b
    }

    // MARK: - Service

    private enum ServiceAction { case start, stop, restart }

    private func serviceSpec(_ p: RPCParams) throws -> ServiceSpec {
        let scopeRaw = try p.string("scope") ?? "user"
        guard let scope = ServiceScope(rawValue: scopeRaw) else {
            throw RPCError.invalidParams("scope")
        }
        var spec = ServiceSpec()
        spec.scope = scope
        spec.configDir = container.config.configDir
        spec.execPath = try p.string("execPath") ?? (Bundle.main.executablePath ?? CommandLine.arguments[0])
        return spec
    }

    private func serviceStatusData(_ spec: ServiceSpec, _ mgr: LaunchdService, _ st: ServiceStatus?) -> [String: Any] {
        let installed = mgr.isInstalled(spec)
        var out: [String: Any] = [
            "installed": installed,
            "running": st?.running ?? false,
            "mode": st?.mode ?? "service",
            "scope": spec.scope.rawValue,
            "label": LaunchdService.label,
            "plistPath": mgr.plistPath(spec),
        ]
        if let pid = st?.pid, pid > 0 { out["pid"] = pid }
        return out
    }

    private func serviceStatus(_ p: RPCParams) throws -> [String: Any] {
        let spec = try serviceSpec(p)
        let mgr = LaunchdService()
        guard mgr.isInstalled(spec) else { return serviceStatusData(spec, mgr, nil) }
        do {
            return serviceStatusData(spec, mgr, try mgr.status(spec))
        } catch {
            throw rpcMapError(error)
        }
    }

    private func serviceInstall(_ p: RPCParams) throws -> [String: Any] {
        let spec = try serviceSpec(p)
        if spec.scope == .system, getuid() != 0 {
            throw RPCError("permission_denied", "system-scope install requires root (sudo)")
        }
        let mgr = LaunchdService()
        do {
            try mgr.install(spec)
            return serviceStatusData(spec, mgr, try? mgr.status(spec))
        } catch {
            throw rpcMapError(error)
        }
    }

    private func serviceUninstall(_ p: RPCParams) throws -> [String: Any] {
        let spec = try serviceSpec(p)
        let mgr = LaunchdService()
        do {
            try mgr.uninstall(spec)
        } catch {
            throw rpcMapError(error)
        }
        return ["installed": false, "running": false, "scope": spec.scope.rawValue]
    }

    private func serviceAction(_ p: RPCParams, _ action: ServiceAction) throws -> [String: Any] {
        let spec = try serviceSpec(p)
        let mgr = LaunchdService()
        do {
            switch action {
            case .start: try mgr.start(spec)
            case .stop: try mgr.stop(spec)
            case .restart: try mgr.restart(spec)
            }
            return serviceStatusData(spec, mgr, try? mgr.status(spec))
        } catch {
            throw rpcMapError(error)
        }
    }

    private func serviceHealth() -> [String: Any] {
        guard let h = ServiceHealth.read(configDir: container.config.configDir) else {
            return ["present": false]
        }
        return [
            "present": true,
            "stale": h.isStale,
            "pid": h.pid,
            "serviceName": h.serviceName,
            "mode": h.mode,
            "startedAt": RPCJSON.rfc3339(h.startedAt),
            "lastHeartbeat": RPCJSON.rfc3339(h.lastHeartbeat),
            "uptimeSecs": Int(h.uptime),
            "webPort": 0,
            "lastSyncAt": h.lastSyncAt.map { RPCJSON.rfc3339($0) } ?? NSNull(),
            "nextScheduleAt": h.nextScheduleAt.map { RPCJSON.rfc3339($0) } ?? NSNull(),
            "lastError": h.lastError,
            "activeTasks": h.activeTasks,
        ]
    }

    // MARK: - Update

    private func updateOptions(_ p: RPCParams, apply: Bool) throws -> UpdateOptions {
        var opts = UpdateOptions()
        opts.repoOwner = try p.string("repoOwner") ?? opts.repoOwner
        opts.repoName = try p.string("repo") ?? opts.repoName
        opts.currentVersion = try p.string("currentVersion") ?? container.version
        if apply { opts.force = try p.bool("force") ?? false }
        opts.log = { [weak self] msg in self?.container.log.info("update: \(msg)") }
        return opts
    }

    private func updateCheck(_ p: RPCParams) throws -> [String: Any] {
        let opts = try updateOptions(p, apply: false)
        do {
            let (current, latest) = try SelfUpdate.check(opts: opts)
            return [
                "current": current,
                "latest": latest,
                "updateAvailable": current != latest,
                "checkedAt": RPCJSON.rfc3339(Date()),
            ]
        } catch {
            throw rpcMapError(error)
        }
    }

    private func updateApply(_ p: RPCParams) throws -> [String: Any] {
        if Bundle.main.bundleURL.pathExtension == "app" {
            throw RPCError("unsupported", "self-update is not supported inside the app bundle",
                           data: ["reason": "bundled_app"])
        }
        let opts = try updateOptions(p, apply: true)
        do {
            let result = try SelfUpdate.update(opts: opts)
            return [
                "updated": result.updated,
                "oldVersion": result.oldVersion,
                "newVersion": result.newVersion,
                "binaryPath": result.binaryPath,
                "restartHint": result.restartHint,
            ]
        } catch {
            throw rpcMapError(error)
        }
    }

    // MARK: - Plumbing

    private func requireStore() throws -> Store {
        guard let store = container.store else { throw RPCError.locked() }
        return store
    }

    private func requireRclone() throws -> RcloneClient {
        guard let rc = container.rclone else { throw RPCError.locked() }
        return rc
    }

    private func resolvedRcloneBinary() throws -> String {
        if let rc = container.rclone { return rc.binary }
        return try RcloneClient.resolveBinary(rcloneBinaryOverride.isEmpty ? nil : rcloneBinaryOverride)
    }

    private func adminRclone() throws -> RcloneClient {
        if let rc = container.rclone { return rc }
        let cfg = (container.config.configDir as NSString).appendingPathComponent("rclone.conf")
        do {
            return try RcloneClient(binaryPath: rcloneBinaryOverride.isEmpty ? nil : rcloneBinaryOverride,
                                    configPath: cfg, logger: container.log)
        } catch {
            throw rpcMapError(error)
        }
    }

    private func oauth() throws -> RPCOAuthManager {
        stateLock.lock()
        if let manager = oauthManager {
            stateLock.unlock()
            return manager
        }
        stateLock.unlock()
        let bin: String
        do { bin = try resolvedRcloneBinary() } catch { throw rpcMapError(error) }
        let cfg = (container.config.configDir as NSString).appendingPathComponent("rclone.conf")
        let manager = RPCOAuthManager(binary: bin, configPath: cfg, logger: container.log)
        manager.pushEvent = { [weak self] name, data in self?.pushEvent?(name, data) }
        stateLock.lock()
        if let existing = oauthManager {
            stateLock.unlock()
            return existing
        }
        oauthManager = manager
        stateLock.unlock()
        return manager
    }

    private func remoteExists(_ rc: RcloneClient, _ name: String) throws -> Bool {
        let remotes: [Remote]
        do { remotes = try rc.listRemotes() } catch { throw rpcMapError(error) }
        return remotes.contains { $0.name == name }
    }

    private func ensureFlowMutable(_ id: String) throws {
        if container.flowEngine.isRunning(id) { throw flowBusy(id) }
        if let st = container.runtime.snapshot().flows.first(where: { $0.id == id })?.status,
           st == "running" || st == "cancelling" {
            throw flowBusy(id)
        }
    }

    private func flowBusy(_ id: String) -> RPCError {
        RPCError("flow_busy", "flow \(id) is running — stop it first")
    }

    private func notFound<T>(_ code: String, _ message: String, _ body: () throws -> T) throws -> T {
        do {
            return try body()
        } catch StoreError.notFound {
            throw RPCError(code, message)
        } catch {
            throw rpcMapError(error)
        }
    }

    private static func question(from stdout: String) -> [String: Any]? {
        for line in stdout.split(separator: "\n").reversed() {
            let text = line.trimmingCharacters(in: .whitespaces)
            guard text.hasPrefix("{"), let obj = RPCJSON.parseObject(text) else { continue }
            if RPCJSON.field(obj, "Option") != nil || RPCJSON.field(obj, "State") != nil {
                return obj
            }
        }
        return nil
    }

    // MARK: - Parsing

    private func parseFlow(_ d: [String: Any]) throws -> Flow {
        let p = try RPCParams(d)
        var f = Flow()
        f.id = try p.string("id") ?? ""
        f.name = try p.string("name") ?? ""
        f.isCollapsed = try p.bool("isCollapsed") ?? false
        f.scheduleEnabled = try p.bool("scheduleEnabled") ?? false
        f.scheduleCron = try p.string("scheduleCron") ?? ""
        f.sortOrder = try p.int("sortOrder") ?? 0
        if let canvasJSON = try p.string("canvasJSON"), !canvasJSON.isEmpty {
            f.canvasJSON = canvasJSON
        } else if let canvas = try p.dict("canvas") {
            f.canvasJSON = RPCJSON.stringifyJSON(canvas)
        }
        if let rawOps = try p.array("operations") {
            for raw in rawOps {
                guard let opDict = raw as? [String: Any] else { throw RPCError.invalidParams("operations") }
                f.operations.append(try parseOperation(opDict, flowID: f.id))
            }
        }
        return f
    }

    private func parseOperation(_ d: [String: Any], flowID: String) throws -> FlowOperation {
        let p = try RPCParams(d)
        var op = FlowOperation()
        op.id = try p.string("id") ?? ""
        op.flowID = flowID
        op.sourceRemote = try p.string("sourceRemote") ?? ""
        op.sourcePath = try p.string("sourcePath") ?? "/"
        op.targetRemote = try p.string("targetRemote") ?? ""
        op.targetPath = try p.string("targetPath") ?? "/"
        op.action = try p.string("action") ?? ""
        if let cfg = try p.dict("syncConfig") {
            op.syncConfig = JSONDict(cfg)
        } else if let cfgString = try p.string("syncConfig"),
                  let parsed = RPCJSON.parseObject(cfgString) {
            op.syncConfig = JSONDict(parsed)
        }
        op.isExpanded = try p.bool("isExpanded") ?? false
        op.sortOrder = try p.int("sortOrder") ?? 0
        op.normalizeAction()
        return op
    }

    private func parseProfile(_ d: [String: Any]) throws -> Profile {
        var merged = RPCJSON.jsonObject(Profile())
        for (k, v) in d where merged[k] != nil { merged[k] = v }
        do {
            let data = try JSONSerialization.data(withJSONObject: merged)
            return try JSONDecoder().decode(Profile.self, from: data)
        } catch {
            throw RPCError("invalid_params", "invalid profile: \(error)")
        }
    }

    private func parseBoard(_ d: [String: Any]) throws -> Board {
        let p = try RPCParams(d)
        var b = Board()
        b.id = try p.string("id") ?? ""
        b.name = try p.string("name") ?? ""
        if let nodes = try p.array("nodes") {
            for raw in nodes {
                guard let nd = raw as? [String: Any] else { throw RPCError.invalidParams("nodes") }
                let np = try RPCParams(nd)
                var n = BoardNode()
                n.id = try np.string("id") ?? ""
                n.remoteName = try np.string("remoteName") ?? ""
                n.path = try np.string("path") ?? ""
                n.label = try np.string("label") ?? ""
                n.x = RPCJSON.doubleValue(np.any("x") ?? 0) ?? 0
                n.y = RPCJSON.doubleValue(np.any("y") ?? 0) ?? 0
                b.nodes.append(n)
            }
        }
        if let edges = try p.array("edges") {
            for raw in edges {
                guard let ed = raw as? [String: Any] else { throw RPCError.invalidParams("edges") }
                let ep = try RPCParams(ed)
                var e = BoardEdge()
                e.id = try ep.string("id") ?? ""
                e.sourceID = try ep.string("sourceID") ?? ""
                e.targetID = try ep.string("targetID") ?? ""
                e.action = try ep.string("action") ?? ""
                if let cfg = try ep.dict("syncConfig") {
                    e.syncConfig = RPCJSON.stringifyJSON(cfg)
                } else if let s = try ep.string("syncConfig"), !s.isEmpty {
                    e.syncConfig = s
                } else {
                    e.syncConfig = "{}"
                }
                b.edges.append(e)
            }
        }
        return b
    }
}
