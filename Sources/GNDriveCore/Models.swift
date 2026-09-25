// Model types for gn-drive persistence — port of internal/store/models.go.
import Foundation

public enum ProfileDirection {
    public static let push = "push"
    public static let bi = "bi"
    public static let biResync = "bi-resync"

    public static func isValid(_ d: String) -> Bool {
        switch d.trimmingCharacters(in: .whitespaces) {
        case push, bi, biResync: return true
        default: return false
        }
    }

    /// Empty or legacy values (e.g. pull) → push.
    public static func normalize(_ d: String) -> String {
        let d = d.trimmingCharacters(in: .whitespaces)
        return isValid(d) ? d : push
    }
}

/// Sync profile with all rclone flags. Mirrors the Go/Wails model.
public struct Profile: Codable, Sendable {
    public var name: String = ""
    public var from: String = ""
    public var to: String = ""
    /// push | bi | bi-resync
    public var direction: String = ""
    public var includedPaths: [String] = []
    public var excludedPaths: [String] = []
    public var bandwidth: Int = 0
    public var parallel: Int = 0
    public var backupPath: String = ""
    public var cachePath: String = ""

    // Filtering
    public var minSize: String = ""
    public var maxSize: String = ""
    public var filterFromFile: String = ""
    public var excludeIfPresent: String = ""
    public var useRegex: Bool = false
    public var maxAge: String = ""
    public var minAge: String = ""
    public var maxDepth: Int? = nil
    public var deleteExcluded: Bool = false

    // Safety
    public var maxDelete: Int? = nil
    public var immutable: Bool = false
    public var conflictResolution: String = ""
    public var dryRun: Bool = false
    public var maxTransfer: String = ""
    public var maxDeleteSize: String = ""
    public var suffix: String = ""
    public var suffixKeepExtension: Bool = false

    // Performance
    public var multiThreadStreams: Int? = nil
    public var bufferSize: String = ""
    public var retries: Int? = nil
    public var lowLevelRetries: Int? = nil
    public var maxDuration: String = ""
    public var checkFirst: Bool = false
    public var orderBy: String = ""
    public var retriesSleep: String = ""
    public var tpsLimit: Double? = nil
    public var connTimeout: String = ""
    public var ioTimeout: String = ""

    // Comparison
    public var sizeOnly: Bool = false
    public var updateMode: Bool = false
    public var ignoreExisting: Bool = false

    // Sync-specific
    public var deleteTiming: String = ""

    // Bisync-specific
    public var resilient: Bool = false
    public var maxLock: String = ""
    public var checkAccess: Bool = false
    public var conflictLoser: String = ""
    public var conflictSuffix: String = ""

    public var fastList: Bool = false

    public init() {}
}

public struct Schedule: Sendable {
    public var id: String = ""
    public var profileName: String = ""
    public var action: String = ""
    public var cron: String = ""
    public var enabled: Bool = false
    public var lastRun: String = ""
    public var nextRun: String = ""
    public var lastResult: String = ""
    public var createdAt: String = ""
    public init() {}
}

public struct HistoryEntry: Sendable {
    public var id: String = ""
    public var profileName: String = ""
    public var action: String = ""
    public var state: String = ""
    public var startedAt: String = ""
    public var finishedAt: String = ""
    public var duration: Int64 = 0
    public var bytes: Int64 = 0
    public var errors: Int = 0
    public var files: Int = 0
    public var errorMessage: String = ""
    public init() {}
    public init(id: String, profileName: String, action: String, state: String,
                startedAt: String = "", finishedAt: String = "", duration: Int64 = 0,
                bytes: Int64 = 0, errors: Int = 0, files: Int = 0, errorMessage: String = "") {
        self.id = id; self.profileName = profileName; self.action = action
        self.state = state; self.startedAt = startedAt; self.finishedAt = finishedAt
        self.duration = duration; self.bytes = bytes; self.errors = errors
        self.files = files; self.errorMessage = errorMessage
    }
}

public struct ProfileStats: Sendable {
    public var syncs: Int = 0
    public var bytes: Int64 = 0
    public var duration: Int64 = 0
    public var errors: Int = 0
}

public struct HistoryStats: Sendable {
    public var totalSyncs: Int = 0
    public var totalBytes: Int64 = 0
    public var totalDuration: Int64 = 0
    public var totalErrors: Int = 0
    public var byProfile: [String: ProfileStats] = [:]
}

public struct BoardNode: Sendable {
    public var id: String = ""
    public var remoteName: String = ""
    public var path: String = ""
    public var label: String = ""
    public var x: Double = 0
    public var y: Double = 0
    public init() {}
}

public struct BoardEdge: Sendable {
    public var id: String = ""
    public var sourceID: String = ""
    public var targetID: String = ""
    public var action: String = ""
    public var syncConfig: String = "{}"
    public init() {}
}

public struct Board: Sendable {
    public var id: String = ""
    public var name: String = ""
    public var createdAt: String = ""
    public var updatedAt: String = ""
    public var nodes: [BoardNode] = []
    public var edges: [BoardEdge] = []
    public init() {}
}

// Flow operation actions: push | bi | bi-resync (pull not offered on flows).
public enum FlowAction {
    public static let push = "push"
    public static let bi = "bi"
    public static let biResync = "bi-resync"

    public static func isValid(_ a: String) -> Bool {
        switch a.trimmingCharacters(in: .whitespaces) {
        case push, bi, biResync: return true
        default: return false
        }
    }

    public static func normalize(_ a: String) -> String {
        let a = a.trimmingCharacters(in: .whitespaces)
        return isValid(a) ? a : push
    }
}

/// Untyped JSON dictionary; contents come from JSONSerialization (property
/// list types only: String/NSNumber/Bool/Array/Dict), safe to share.
public struct JSONDict: @unchecked Sendable {
    public var value: [String: Any] = [:]
    public init() {}
    public init(_ v: [String: Any]) { value = v }
}

/// A single sync step inside a Flow.
public struct FlowOperation: Sendable {
    public var id: String = ""
    public var flowID: String = ""
    public var sourceRemote: String = ""
    public var sourcePath: String = "/"
    public var targetRemote: String = ""
    public var targetPath: String = "/"
    public var action: String = "push"
    /// Raw JSON object (sync_config). Stored as a dictionary for flexibility.
    public var syncConfig: JSONDict = JSONDict()
    public var isExpanded: Bool = false
    public var sortOrder: Int = 0

    public init() {}

    /// Effective flow action (column or sync_config.action), normalized.
    public func resolvedAction() -> String {
        if !action.trimmingCharacters(in: .whitespaces).isEmpty {
            return FlowAction.normalize(action)
        }
        if let a = syncConfig.value["action"] as? String, !a.trimmingCharacters(in: .whitespaces).isEmpty {
            return FlowAction.normalize(a)
        }
        return FlowAction.push
    }

    /// Keep action column and sync_config.action aligned (Wails SaveFlows).
    public mutating func normalizeAction() {
        let a = resolvedAction()
        action = a
        syncConfig.value["action"] = a
    }
}

public struct Flow: Sendable {
    public var id: String = ""
    public var name: String = ""
    public var isCollapsed: Bool = false
    public var scheduleEnabled: Bool = false
    public var scheduleCron: String = ""
    public var sortOrder: Int = 0
    public var operations: [FlowOperation] = []
    /// Workspace graph layout JSON (ignored by execution).
    public var canvasJSON: String = "{}"
    public var createdAt: String = ""
    public var updatedAt: String = ""
    public init() {}
}

/// Build an rclone path from remote name + path. Empty/local remote → local path.
public func composePath(remote: String, path: String) -> String {
    let path = path.trimmingCharacters(in: .whitespaces)
    let remote = remote.trimmingCharacters(in: .whitespaces)
    if remote.isEmpty || remote == "local" {
        return path.isEmpty ? "/" : path
    }
    if path.isEmpty || path == "/" {
        return remote + ":"
    }
    return remote + ":" + path
}

public struct DeltaState: Sendable {
    public var remoteKey: String = ""
    public var provider: String = ""
    public var lastFullSync: String = ""
    public var deltaCount: Int = 0
    public var isWatching: Bool = false
    public init() {}
}
