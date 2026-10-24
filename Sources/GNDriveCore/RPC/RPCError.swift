// Protocol v1 error envelope + mapping from engine errors.
import Foundation

public struct RPCError: Error, @unchecked Sendable {
    public let code: String
    public let message: String
    public let data: [String: Any]?

    public init(_ code: String, _ message: String, data: [String: Any]? = nil) {
        self.code = code
        self.message = message
        self.data = data
    }

    public static func invalidParams(_ field: String, _ message: String? = nil) -> RPCError {
        RPCError("invalid_params", message ?? "invalid params: \(field)", data: ["field": field])
    }

    public static func locked() -> RPCError {
        RPCError("locked", "auth: app is locked", data: ["setup": true])
    }
}

public enum RPCRedact {
    private static let patterns: [NSRegularExpression] = [
        try! NSRegularExpression(
            pattern: "(?i)(pass(?:word)?|token|secret|client_secret|config_token|access_token)\\s*[=:]\\s*[^\\s,;\"'}]+"),
        try! NSRegularExpression(
            pattern: "(?i)([\"'](?:pass(?:word)?|token|secret|key|client_secret|config_token|access_token)[\"']\\s*:\\s*)[\"'][^\"']*[\"']"),
        try! NSRegularExpression(
            pattern: "(?i)(--(?:[\\w-]*key[\\w-]*|password|token|secret))\\s+(?!-)\\S+"),
    ]

    /// Redact credential-looking values from rclone output before it reaches
    /// a client. Keeps command/flag names and messages readable.
    public static func sanitize(_ text: String) -> String {
        var out = text
        let range = NSRange(out.startIndex..<out.endIndex, in: out)
        for re in patterns {
            out = re.stringByReplacingMatches(in: out, range: range, withTemplate: "$1=***")
        }
        return out
    }

    public static func stderrField(_ text: String) -> String {
        sanitize(truncateString(text, 500))
    }
}

public func rpcMapError(_ error: Error) -> RPCError {
    if let e = error as? RPCError { return e }

    if let e = error as? AuthError {
        switch e {
        case .notSetup:
            return RPCError("not_setup", "auth: no master password is configured")
        case .alreadySetup:
            return RPCError("already_setup", "auth: a master password is already configured")
        case .notUnlocked:
            return RPCError("not_unlocked", "auth: app is locked")
        case .invalidPassword:
            return RPCError("invalid_password", "auth: invalid password")
        case .locked(let secs):
            return RPCError("locked_out", "auth: too many attempts — retry in \(secs)s",
                            data: ["retryAfterSecs": secs])
        case .alreadyUnlocked:
            return RPCError("already_unlocked", "auth: already unlocked")
        case .passwordTooShort:
            return RPCError("password_too_short", "auth: password must be at least 4 characters")
        }
    }

    if let e = error as? FlowEngineError {
        switch e {
        case .alreadyRunning:
            return RPCError("already_running", "flow is already running")
        case .notRunning:
            return RPCError("not_running", "flow is not running")
        case .emptyFlow:
            return RPCError("flow_empty", "flow has no operations")
        case .engineNotReady:
            return RPCError("engine_not_ready", "flow engine is not ready")
        }
    }

    if let e = error as? SyncEngineError {
        switch e {
        case .notRunning:
            return RPCError("sync_not_running", "sync engine is not running")
        case .storeNotReady:
            return RPCError("engine_not_ready", "sync engine has no data plane")
        case .profileBusy:
            return RPCError("profile_busy", "a sync for this profile is already running")
        case .taskFailed(let msg):
            return RPCError("task_failed", RPCRedact.sanitize(msg))
        case .taskCancelled:
            return RPCError("task_cancelled", "task was cancelled")
        case .taskNotFound:
            return RPCError("task_not_found", "task not found")
        }
    }

    if let e = error as? BoardEngineError {
        switch e {
        case .notConfigured:
            return RPCError("engine_not_ready", "board engine is not ready")
        case .emptyBoard:
            return RPCError("board_empty", "board has no nodes or edges")
        case .alreadyRunning:
            return RPCError("board_busy", "board is already running")
        case .notRunning:
            return RPCError("board_not_running", "board is not running")
        case .cancelled:
            return RPCError("board_not_running", "board run was cancelled")
        case .missingNode(let edgeID):
            return RPCError("board_missing_node", "board edge \(edgeID) references a missing node")
        case .failed(let msg), .completedWithErrors(let msg):
            if msg.contains("cycle detected") {
                return RPCError("board_cycle", "board graph contains a cycle")
            }
            return RPCError("internal_error", RPCRedact.sanitize(msg))
        }
    }

    if let e = error as? RcloneError {
        switch e {
        case .binaryNotFound(let p):
            return RPCError("rclone_missing", "rclone binary not found: \(p)", data: ["path": p])
        case .runFailed(let cmd, let stderr):
            return RPCError("rclone_failed", "rclone \(RPCRedact.sanitize(cmd)): \(RPCRedact.stderrField(stderr))",
                            data: ["command": RPCRedact.sanitize(cmd),
                                   "stderr": RPCRedact.stderrField(stderr)])
        case .cancelled:
            return RPCError("task_cancelled", "rclone was cancelled")
        }
    }

    if let e = error as? SyncFailedError {
        return RPCError("task_failed", e.cancelled ? "sync cancelled" : RPCRedact.sanitize(e.message),
                        data: ["cancelled": e.cancelled ? "true" : "false"])
    }

    if let e = error as? ServiceError {
        switch e {
        case .notInstalled:
            return RPCError("service_not_installed", "service is not installed")
        case .launchctl(let m):
            return RPCError("service_error", RPCRedact.sanitize(m))
        }
    }

    if let e = error as? SelfUpdateError {
        switch e {
        case .noRelease:
            return RPCError("no_release", "no release found")
        case .alreadyUpToDate:
            return RPCError("already_up_to_date", "already on the latest version")
        case .noMatchingAsset(let n):
            return RPCError("no_matching_asset", "no release asset matches this platform: \(n)")
        case .missingChecksum(let n):
            return RPCError("no_matching_asset", "missing checksum asset \(n)")
        case .checksumMismatch:
            return RPCError("checksum_mismatch", "downloaded archive failed SHA256 verification")
        case .http(let code, let body):
            return RPCError("update_failed", "update HTTP \(code): \(RPCRedact.stderrField(body))")
        case .extractFailed(let m):
            return RPCError("update_failed", "update extract failed: \(m)")
        }
    }

    if let e = error as? AppError {
        switch e {
        case .locked:
            return RPCError.locked()
        case .rcloneMissing(let underlying):
            return RPCError("rclone_missing", "rclone unavailable: \(underlying)")
        }
    }

    if let e = error as? InstanceError {
        switch e {
        case .anotherInstance(let pid):
            var d: [String: String] = [:]
            if let pid { d["pid"] = String(pid) }
            return RPCError("instance_busy", e.localizedDescription, data: d)
        }
    }

    if let e = error as? StoreError {
        switch e {
        case .notFound:
            return RPCError("not_found", "record not found")
        case .invalid(let m):
            if m.contains("name is required") {
                return RPCError("missing_name", m, data: ["field": "name"])
            }
            if m.contains("invalid direction") {
                return RPCError("invalid_direction", m, data: ["field": "direction"])
            }
            return RPCError("invalid_params", m)
        }
    }

    if let e = error as? CocoaError {
        return RPCError("internal_error", e.localizedDescription)
    }
    return RPCError("internal_error", String(describing: error))
}
