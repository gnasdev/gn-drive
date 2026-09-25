// Platform and path detection for gn-drive.
// Ported from internal/config/paths.go.
import Foundation

public enum AppEnv: String, Sendable {
    case development
    case production
}

public struct Paths: Sendable {
    /// User's home directory (~).
    public var homeDir: String
    /// ~/.config/gn-drive on macOS.
    public var configDir: String
    /// Log directory (== configDir on macOS).
    public var logDir: String
    /// Directory containing the running binary.
    public var workingDir: String
    public var env: AppEnv

    public static func detect(env: [String: String] = ProcessInfo.processInfo.environment) -> Paths {
        let home = NSHomeDirectory().isEmpty ? "/tmp" : NSHomeDirectory()
        var cfgDir = (home as NSString).appendingPathComponent(".config/gn-drive")
        if let override = env["GN_DRIVE_CONFIG_DIR"], !override.isEmpty {
            cfgDir = override
        }
        let exePath = Bundle.main.executablePath ?? CommandLine.arguments.first ?? ""
        let workDir = exePath.isEmpty ? "" : (exePath as NSString).deletingLastPathComponent

        var appEnv: AppEnv = .production
        if env["GN_DRIVE_DEV"] != nil {
            appEnv = .development
        } else if workDir.hasSuffix("/gn-drive/bin") || workDir.contains("/desktop/bin") {
            appEnv = .development
        }

        return Paths(homeDir: home, configDir: cfgDir, logDir: cfgDir, workingDir: workDir, env: appEnv)
    }

    public func ensureConfigDir() throws {
        try FileManager.default.createDirectory(atPath: configDir, withIntermediateDirectories: true)
        // chmod 0700
        try FileManager.default.setAttributes([.posixPermissions: 0o700], ofItemAtPath: configDir)
    }
}
