// rclone shell-out wrapper — port of internal/rclone.
import Foundation

public enum RcloneAction: String, Sendable {
    case pull, push, bi
    case biResync = "bi-resync"
    case copy, move, check
    case dryRun = "dry-run"
}

public struct FileTransfer: Sendable, Codable {
    public var name: String = ""
    public var size: Int64 = 0
    public var bytes: Int64 = 0
    public var progress: Double = 0      // 0-100
    public var status: String = ""       // transferring | completed | failed | checking | checked | pending
    public var speed: Double = 0
    public var error: String = ""
    public init(name: String, size: Int64 = 0, bytes: Int64 = 0, progress: Double = 0, status: String = "", speed: Double = 0, error: String = "") {
        self.name = name
        self.size = size
        self.bytes = bytes
        self.progress = progress
        self.status = status
        self.speed = speed
        self.error = error
    }

}

/// Progress snapshot during a sync operation.
public struct SyncStats: Sendable {
    public var bytes: Int64 = 0
    public var bytesTotal: Int64 = 0
    public var files: Int64 = 0
    public var filesTotal: Int64 = 0
    public var transfers: Int64 = 0
    public var errors: Int64 = 0
    public var checks: Int64 = 0
    public var checksTotal: Int64 = 0
    public var deletes: Int64 = 0
    public var renames: Int64 = 0
    public var speed: Double = 0          // B/s
    public var eta: Int64 = 0             // seconds
    public var currentFile: String = ""
    public var stage: String = ""         // starting | connecting | listing | checking | transferring | retrying
    public var stageDetail: String = ""
    public var lastUpdate: Int64 = 0
    public var fileTransfers: [FileTransfer] = []

    public init() {}
}

public struct SyncResult: Sendable {
    public var stats = SyncStats()
    public var startedAt: Int64 = 0
    public var endedAt: Int64 = 0
    public var exitCode: Int = -1
    public var stderr: String = ""
}

/// Per-operation configuration.
public struct SyncConfig: Sendable {
    public var action: RcloneAction = .push
    public var source: String = ""        // remote:path or local path
    public var sourceRemote: String = ""
    public var sourcePath: String = ""
    public var dest: String = ""
    public var destRemote: String = ""
    public var destPath: String = ""
    public var resync: Bool = false
    public var profile: ProfileFlags? = nil
    public var statsInterval: String = "1s"

    public init() {}
    public init(action: RcloneAction, source: String = "", sourceRemote: String = "",
                sourcePath: String = "", dest: String = "", destRemote: String = "",
                destPath: String = "", resync: Bool = false,
                profile: ProfileFlags? = nil, statsInterval: String = "1s") {
        self.action = action
        self.source = source
        self.sourceRemote = sourceRemote
        self.sourcePath = sourcePath
        self.dest = dest
        self.destRemote = destRemote
        self.destPath = destPath
        self.resync = resync
        self.profile = profile
        self.statsInterval = statsInterval
    }
}

/// rclone flags a profile / flow sync_config can set.
public struct ProfileFlags: Sendable {
    public var bandwidth: String = ""
    public var transfers: Int = 0
    public var checkers: Int = 0
    public var tpsLimit: Double = 0
    public var minAge: String = ""
    public var maxAge: String = ""
    public var minSize: String = ""
    public var maxSize: String = ""
    public var excludeIfPresent: String = ""
    public var maxDelete: Int = 0
    public var dryRun: Bool = false
    public var useListR: Bool = false
    public var noUnicodeNormalize: Bool = false
    public var includes: [String] = []
    public var excludes: [String] = []
    public var multiThreadStreams: Int = 0
    public var bufferSize: String = ""
    public var retries: Int = 0
    public var lowLevelRetries: Int = 0
    public var maxDuration: String = ""
    public var retriesSleep: String = ""
    public var connTimeout: String = ""
    public var ioTimeout: String = ""
    public var orderBy: String = ""
    public var checkFirst: Bool = false
    public var immutable: Bool = false
    public var maxTransfer: String = ""
    public var maxDeleteSize: String = ""
    public var suffix: String = ""
    public var suffixKeepExtension: Bool = false
    public var backupDir: String = ""
    public var sizeOnly: Bool = false
    public var updateMode: Bool = false
    public var ignoreExisting: Bool = false
    public var deleteExcluded: Bool = false
    public var maxDepth: Int = 0
    public var deleteTiming: String = ""   // before|during|after
    public var conflictResolve: String = ""
    public var conflictLoser: String = ""
    public var conflictSuffix: String = ""
    public var resilient: Bool = false
    public var maxLock: String = ""
    public var checkAccess: Bool = false

    public init() {}
}

/// Mirrors rclone's lsjson output.
public struct FileEntry: Sendable, Codable {
    public var name: String = ""
    public var size: Int64 = 0
    public var mimeType: String = ""
    public var isDir: Bool = false
    public var modTime: String = ""
    public var path: String = ""
    public var id: String = ""

    enum CodingKeys: String, CodingKey {
        case name = "Name"
        case size = "Size"
        case mimeType = "MimeType"
        case isDir = "IsDir"
        case modTime = "ModTime"
        case path = "Path"
        case id = "ID"
    }
}

public struct QuotaInfo: Sendable {
    public var used: Int64 = 0
    public var total: Int64 = 0
    public var free: Int64 = 0
}

public struct Remote: Sendable {
    public var name: String = ""
    public var type: String = ""
    public var description: String = ""
}

/// Per-provider rate limits (simplified resolver policy).
public struct ResolverPolicy: Sendable {
    public var transfers: Int
    public var checkers: Int
    public var tpsLimit: Double

    public static func apply(sourceType: String, destType: String) -> ResolverPolicy {
        let limited = ["drive", "onedrive", "dropbox", "box", "icloud", "iclouddrive",
                       "googlephotos", "mega", "pcloud", "yandex", "mailru", "sharepoint"]
        if limited.contains(sourceType) || limited.contains(destType) {
            return ResolverPolicy(transfers: 4, checkers: 4, tpsLimit: 4)
        }
        return ResolverPolicy(transfers: 8, checkers: 8, tpsLimit: 0)
    }
}
