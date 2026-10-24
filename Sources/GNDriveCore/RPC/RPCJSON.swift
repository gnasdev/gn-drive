// JSON helpers: request params, result encoding, rclone payload normalization.
import Foundation

public enum RPCJSON {
    public static let protocolVersion = 1

    public static func rfc3339(_ date: Date) -> String {
        let f = ISO8601DateFormatter()
        f.formatOptions = [.withInternetDateTime]
        return f.string(from: date)
    }

    public static func nowMillis() -> Int64 {
        Int64(Date().timeIntervalSince1970 * 1000)
    }

    public static func millis(_ date: Date) -> Int64 {
        Int64(date.timeIntervalSince1970 * 1000)
    }

    static func isBool(_ value: Any) -> Bool? {
        guard let n = value as? NSNumber, CFGetTypeID(n) == CFBooleanGetTypeID() else { return nil }
        return n.boolValue
    }

    static func intValue(_ value: Any) -> Int? {
        if isBool(value) != nil { return nil }
        if let i = value as? Int { return i }
        if let n = value as? NSNumber { return n.intValue }
        if let s = value as? String { return Int(s.trimmingCharacters(in: .whitespaces)) }
        return nil
    }

    static func doubleValue(_ value: Any) -> Double? {
        if isBool(value) != nil { return nil }
        if let d = value as? Double { return d }
        if let n = value as? NSNumber { return n.doubleValue }
        if let s = value as? String { return Double(s.trimmingCharacters(in: .whitespaces)) }
        return nil
    }

    static func boolValue(_ value: Any) -> Bool? {
        if let b = isBool(value) { return b }
        if let n = value as? NSNumber { return n.intValue != 0 }
        if let s = value as? String {
            switch s.lowercased() {
            case "true", "1", "yes": return true
            case "false", "0", "no": return false
            default: return nil
            }
        }
        return nil
    }

    /// Case-tolerant lookup for rclone JSON (PascalCase in 1.75, lowercase in
    /// older builds and in this spec's examples).
    static func field(_ dict: [String: Any], _ name: String) -> Any? {
        if let v = dict[name] { return v }
        if let v = dict[name.prefix(1).lowercased() + name.dropFirst()] { return v }
        for (k, v) in dict where k.lowercased() == name.lowercased() { return v }
        return nil
    }

    static func jsonObject(_ value: Encodable) -> [String: Any] {
        guard let data = try? JSONEncoder().encode(value),
              let obj = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else {
            return [:]
        }
        return obj
    }

    static func parseObject(_ json: String) -> [String: Any]? {
        guard let data = json.data(using: .utf8),
              let obj = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else { return nil }
        return obj
    }

    static func stringifyJSON(_ dict: [String: Any]) -> String {
        guard JSONSerialization.isValidJSONObject(dict),
              let data = try? JSONSerialization.data(withJSONObject: dict),
              let s = String(data: data, encoding: .utf8) else { return "{}" }
        return s
    }

    // MARK: - Models

    public static func authStatus(_ s: AuthStatus) -> [String: Any] {
        [
            "setup": s.setup,
            "unlocked": s.unlocked,
            "lockedAt": s.lockedAt.map { rfc3339($0) } ?? NSNull(),
            "enabled": s.enabled,
            "lockout": [
                "failedAttempts": s.lockout.failedAttempts,
                "lockedUntil": s.lockout.lockedUntil,
                "isLocked": s.lockout.isLocked,
                "retryAfterSecs": s.lockout.retryAfterSecs,
            ],
        ]
    }

    public static func appSettings(_ s: AppSettings) -> [String: Any] {
        [
            "notificationsEnabled": s.notificationsEnabled,
            "debugMode": s.debugMode,
            "minimizeToTray": s.minimizeToTray,
            "startAtLogin": s.startAtLogin,
        ]
    }

    public static func profile(_ p: Profile) -> [String: Any] {
        jsonObject(p)
    }

    public static func remote(_ r: Remote) -> [String: Any] {
        ["name": r.name, "type": r.type, "description": r.description]
    }

    public static func fileEntry(_ e: FileEntry) -> [String: Any] {
        [
            "name": e.name, "size": e.size, "mimeType": e.mimeType,
            "isDir": e.isDir, "modTime": e.modTime, "path": e.path, "id": e.id,
        ]
    }

    public static func schedule(_ s: Schedule) -> [String: Any] {
        [
            "id": s.id, "profileName": s.profileName, "action": s.action,
            "cron": s.cron, "enabled": s.enabled, "lastRun": s.lastRun,
            "nextRun": s.nextRun, "lastResult": s.lastResult, "createdAt": s.createdAt,
        ]
    }

    public static func history(_ e: HistoryEntry) -> [String: Any] {
        [
            "id": e.id, "profileName": e.profileName, "action": e.action,
            "state": e.state, "startedAt": e.startedAt, "finishedAt": e.finishedAt,
            "duration": e.duration, "bytes": e.bytes, "errors": e.errors,
            "files": e.files, "errorMessage": e.errorMessage,
        ]
    }

    public static func historyStats(_ s: HistoryStats) -> [String: Any] {
        var by: [String: Any] = [:]
        for (k, v) in s.byProfile {
            by[k] = ["syncs": v.syncs, "bytes": v.bytes, "duration": v.duration, "errors": v.errors]
        }
        return [
            "totalSyncs": s.totalSyncs, "totalBytes": s.totalBytes,
            "totalDuration": s.totalDuration, "totalErrors": s.totalErrors,
            "byProfile": by,
        ]
    }

    public static func operation(_ op: FlowOperation) -> [String: Any] {
        [
            "id": op.id, "flowID": op.flowID,
            "sourceRemote": op.sourceRemote, "sourcePath": op.sourcePath,
            "targetRemote": op.targetRemote, "targetPath": op.targetPath,
            "action": op.resolvedAction(),
            "syncConfig": op.syncConfig.value,
            "isExpanded": op.isExpanded, "sortOrder": op.sortOrder,
        ]
    }

    public static func flow(_ f: Flow) -> [String: Any] {
        [
            "id": f.id, "name": f.name, "isCollapsed": f.isCollapsed,
            "scheduleEnabled": f.scheduleEnabled, "scheduleCron": f.scheduleCron,
            "sortOrder": f.sortOrder,
            "operations": f.operations.map(operation),
            "canvasJSON": f.canvasJSON,
            "canvas": parseObject(f.canvasJSON) ?? [:],
            "createdAt": f.createdAt, "updatedAt": f.updatedAt,
        ]
    }

    public static func boardNode(_ n: BoardNode) -> [String: Any] {
        ["id": n.id, "remoteName": n.remoteName, "path": n.path,
         "label": n.label, "x": n.x, "y": n.y]
    }

    public static func boardEdge(_ e: BoardEdge) -> [String: Any] {
        ["id": e.id, "sourceID": e.sourceID, "targetID": e.targetID,
         "action": e.action, "syncConfig": parseObject(e.syncConfig) ?? [:]]
    }

    public static func board(_ b: Board) -> [String: Any] {
        [
            "id": b.id, "name": b.name,
            "createdAt": b.createdAt, "updatedAt": b.updatedAt,
            "nodes": b.nodes.map(boardNode),
            "edges": b.edges.map(boardEdge),
        ]
    }

    static func fileTransfer(_ t: FileTransfer) -> [String: Any] {
        jsonObject(t)
    }

    static func fileTransferEvent(_ t: FileTransferEvent) -> [String: Any] {
        jsonObject(t)
    }

    public static func syncStats(_ s: SyncStats) -> [String: Any] {
        [
            "bytes": s.bytes, "bytesTotal": s.bytesTotal,
            "files": s.files, "filesTotal": s.filesTotal,
            "transfers": s.transfers, "errors": s.errors,
            "checks": s.checks, "checksTotal": s.checksTotal,
            "deletes": s.deletes, "renames": s.renames,
            "speed": s.speed, "eta": s.eta,
            "currentFile": s.currentFile, "stage": s.stage,
            "stageDetail": s.stageDetail, "lastUpdate": s.lastUpdate,
            "fileTransfers": s.fileTransfers.map(fileTransfer),
        ]
    }

    public static func task(_ t: SyncTask.Snapshot) -> [String: Any] {
        [
            "id": t.id, "name": t.name, "action": t.action, "status": t.status,
            "stats": syncStats(t.stats),
            "startedAt": rfc3339(t.startedAt),
            "endedAt": t.endedAt.map { rfc3339($0) } ?? NSNull(),
        ]
    }

    public static func syncProgress(_ e: SyncProgressEvent) -> [String: Any] {
        [
            "taskID": e.taskID, "profileID": e.profileID, "action": e.action,
            "state": e.state, "transferred": e.transferred, "total": e.total,
            "bytesPerSec": e.bytesPerSec, "eta": e.eta, "errors": e.errors,
            "currentFile": e.currentFile, "stage": e.stage, "stageDetail": e.stageDetail,
            "filesTransferred": e.filesTransferred, "totalFiles": e.totalFiles,
            "checks": e.checks, "totalChecks": e.totalChecks,
            "deletes": e.deletes, "renames": e.renames,
            "transfers": e.transfers.map(fileTransferEvent),
            "errorMessage": e.errorMessage,
        ]
    }

    public static func runtimeSnapshot(_ s: RuntimeSnapshotEvent) -> [String: Any] {
        [
            "revision": s.revision,
            "flows": s.flows.map { f -> [String: Any] in
                var d: [String: Any] = [
                    "id": f.id, "status": f.status.isEmpty ? "idle" : f.status,
                    "lastError": f.lastError,
                    "ops": f.ops.map { ["id": $0.id, "status": $0.status, "lastError": $0.lastError] },
                    "log": f.log.map {
                        ["at": $0.at, "status": $0.status, "opID": $0.opID,
                         "error": $0.error, "label": $0.label]
                    },
                ]
                if let sync = f.sync { d["sync"] = syncProgress(sync) }
                return d
            },
        ]
    }

    // MARK: - rclone payloads

    public static func normalizedQuestion(_ raw: [String: Any]) -> [String: Any] {
        let state = (field(raw, "State") as? String) ?? ""
        let error = (field(raw, "Error") as? String) ?? ""
        let result = (field(raw, "Result") as? String) ?? ""
        var out: [String: Any] = ["state": state, "error": error, "result": result]
        if let opt = field(raw, "Option") as? [String: Any] {
            out["option"] = normalizedOption(opt)
        } else {
            out["option"] = NSNull()
        }
        return out
    }

    static func normalizedOption(_ o: [String: Any]) -> [String: Any] {
        let examples: [[String: Any]] = ((field(o, "Examples") as? [[String: Any]]) ?? []).map {
            let value = field($0, "Value")
            let help = field($0, "Help")
            return ["value": stringOf(value), "help": stringOf(help)]
        }
        return [
            "name": stringOf(field(o, "Name")),
            "help": stringOf(field(o, "Help")),
            "default": field(o, "Default") ?? field(o, "DefaultStr") ?? "",
            "required": boolValue(field(o, "Required") ?? false) ?? false,
            "type": stringOf(field(o, "Type")),
            "examples": examples,
        ]
    }

    static func stringOf(_ v: Any?) -> String {
        switch v {
        case nil, is NSNull: return ""
        case let s as String: return s
        case let b as Bool: return b ? "true" : "false"
        case let n as NSNumber: return n.stringValue
        default: return String(describing: v!)
        }
    }

    public static func provider(_ raw: [String: Any]) -> [String: Any] {
        let options = (field(raw, "Options") as? [[String: Any]]) ?? []
        let oauth = options.contains { stringOf(field($0, "Name")) == "auth_url" }
        return [
            "name": stringOf(field(raw, "Name")),
            "description": stringOf(field(raw, "Description")),
            "prefix": stringOf(field(raw, "Prefix")),
            "hidden": boolValue(field(raw, "Hide") ?? false) ?? false,
            "oauth": oauth,
            "options": options.map { o -> [String: Any] in
                [
                    "name": stringOf(field(o, "Name")),
                    "help": stringOf(field(o, "Help")),
                    "required": boolValue(field(o, "Required") ?? false) ?? false,
                    "isPassword": boolValue(field(o, "IsPassword") ?? false) ?? false,
                    "sensitive": boolValue(field(o, "Sensitive") ?? false) ?? false,
                    "advanced": boolValue(field(o, "Advanced") ?? false) ?? false,
                    "hide": intValue(field(o, "Hide") ?? 0) ?? 0,
                    "type": stringOf(field(o, "Type")),
                    "default": stringOf(field(o, "DefaultStr")),
                    "examples": ((field(o, "Examples") as? [[String: Any]]) ?? []).map {
                        ["value": stringOf(field($0, "Value")),
                         "help": stringOf(field($0, "Help"))]
                    },
                ]
            },
        ]
    }
}

// MARK: - Parameters

public struct RPCParams {
    public let raw: [String: Any]

    public init(_ value: Any?) throws {
        switch value {
        case nil, is NSNull:
            raw = [:]
        case let d as [String: Any]:
            raw = d
        default:
            throw RPCError("invalid_params", "params must be an object")
        }
    }

    static func aliases(_ key: String) -> [String] {
        var out = [key]
        if key.hasSuffix("ID") { out.append(String(key.dropLast(2)) + "Id") }
        if key.hasSuffix("JSON") { out.append(String(key.dropLast(4)) + "Json") }
        if key.hasSuffix("IDs") { out.append(String(key.dropLast(3)) + "Ids") }
        return out
    }

    public func any(_ key: String) -> Any? {
        for k in Self.aliases(key) {
            if let v = raw[k], !(v is NSNull) { return v }
        }
        return nil
    }

    public func has(_ key: String) -> Bool { any(key) != nil }

    public func string(_ key: String) throws -> String? {
        guard let v = any(key) else { return nil }
        guard let s = v as? String else { throw RPCError.invalidParams(key) }
        return s
    }

    public func requireString(_ key: String) throws -> String {
        guard let s = try string(key), !s.trimmingCharacters(in: .whitespaces).isEmpty else {
            throw RPCError.invalidParams(key, "missing required field: \(key)")
        }
        return s
    }

    public func int(_ key: String) throws -> Int? {
        guard let v = any(key) else { return nil }
        guard let i = RPCJSON.intValue(v) else { throw RPCError.invalidParams(key) }
        return i
    }

    public func bool(_ key: String) throws -> Bool? {
        guard let v = any(key) else { return nil }
        guard let b = RPCJSON.boolValue(v) else { throw RPCError.invalidParams(key) }
        return b
    }

    public func dict(_ key: String) throws -> [String: Any]? {
        guard let v = any(key) else { return nil }
        guard let d = v as? [String: Any] else { throw RPCError.invalidParams(key) }
        return d
    }

    public func array(_ key: String) throws -> [Any]? {
        guard let v = any(key) else { return nil }
        guard let a = v as? [Any] else { throw RPCError.invalidParams(key) }
        return a
    }

    public func stringArray(_ key: String) throws -> [String]? {
        guard let a = try array(key) else { return nil }
        var out: [String] = []
        for item in a {
            guard let s = item as? String else { throw RPCError.invalidParams(key) }
            out.append(s)
        }
        return out
    }

    /// Flatten `config` values to rclone `key=value` arguments.
    public func configKVs(_ key: String = "config") throws -> [String] {
        guard let d = try dict(key) else { return [] }
        return d.keys.sorted().map { k in
            "\(k)=\(RPCJSON.stringOf(d[k]))"
        }
    }
}
