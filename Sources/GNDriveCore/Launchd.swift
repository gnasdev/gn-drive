// launchd service management — port of internal/service (darwin only).
import Foundation

public enum ServiceScope: String, Sendable {
    case user, system
}

public struct ServiceSpec: Sendable {
    public var name = "gn-drive"
    public var displayName = "GN Drive"
    public var description = ""
    public var execPath = ""
    public var configDir = ""
    public var scope: ServiceScope = .user
    public var env: [String] = []

    public init() {}
}

public struct ServiceStatus: Sendable {
    public var installed = false
    public var running = false
    public var pid = 0
    public var mode = "service"
    public var scope = ""
}

public enum ServiceError: Error, LocalizedError {
    case notInstalled
    case launchctl(String)

    public var errorDescription: String? {
        switch self {
        case .notInstalled: return "service: not installed"
        case .launchctl(let e): return "service: \(e)"
        }
    }
}

/// launchd manager for macOS.
/// User: ~/Library/LaunchAgents/com.gndrive.app.plist (gui/$UID domain).
/// System: /Library/LaunchDaemons/com.gndrive.app.plist.
public final class LaunchdService: @unchecked Sendable {
    public static let label = "com.gndrive.app"
    private static let legacyAgentName = "gn-drive"
    private static let userPlistDir = "Library/LaunchAgents"
    private static let systemPlistDir = "Library/LaunchDaemons"

    public init() {}

    private func plistPath(_ spec: ServiceSpec) -> String {
        if spec.scope == .system {
            return "/\(Self.systemPlistDir)/\(Self.label).plist"
        }
        return NSHomeDirectory() + "/\(Self.userPlistDir)/\(Self.label).plist"
    }

    private func domain(_ spec: ServiceSpec) -> String {
        spec.scope == .system ? "system" : "gui/\(getuid())"
    }

    public func isInstalled(_ spec: ServiceSpec) -> Bool {
        FileManager.default.fileExists(atPath: plistPath(spec))
    }

    public func install(_ spec: ServiceSpec) throws {
        cleanupLegacyAgent(spec)
        // Remove stale registration before writing a fresh plist.
        if isInstalled(spec) {
            try? bootout(spec)
            try? FileManager.default.removeItem(atPath: plistPath(spec))
        }
        let plist = renderPlist(spec)
        let path = plistPath(spec)
        try FileManager.default.createDirectory(
            atPath: (path as NSString).deletingLastPathComponent,
            withIntermediateDirectories: true)
        try plist.write(toFile: path, atomically: true, encoding: .utf8)
        do {
            try runLaunchctl(["bootstrap", domain(spec), path])
        } catch {
            // One retry (matches Go behaviour for transient bootstrap errors).
            do { try runLaunchctl(["bootstrap", domain(spec), path]) }
            catch { throw ServiceError.launchctl("bootstrap failed; plist written to: \(path)\n\(error)") }
        }
    }

    public func uninstall(_ spec: ServiceSpec) throws {
        if isInstalled(spec) {
            try? bootout(spec)
            try FileManager.default.removeItem(atPath: plistPath(spec))
        }
        cleanupLegacyAgent(spec)
    }

    public func start(_ spec: ServiceSpec) throws {
        guard isInstalled(spec) else { throw ServiceError.notInstalled }
        try runLaunchctl(["kickstart", "-k", "\(domain(spec))/\(Self.label)"])
    }

    public func stop(_ spec: ServiceSpec) throws {
        guard isInstalled(spec) else { throw ServiceError.notInstalled }
        try runLaunchctl(["bootout", "\(domain(spec))/\(Self.label)"])
    }

    public func restart(_ spec: ServiceSpec) throws {
        try stop(spec)
        try start(spec)
    }

    private func bootout(_ spec: ServiceSpec) throws {
        try runLaunchctl(["bootout", domain(spec), plistPath(spec)])
    }

    public func status(_ spec: ServiceSpec) throws -> ServiceStatus {
        var st = ServiceStatus()
        st.scope = spec.scope.rawValue
        st.installed = isInstalled(spec)
        guard st.installed else { throw ServiceError.notInstalled }
        // launchctl print gui/$UID/label — look for "state = running" / pid.
        let out = (try? runLaunchctlOutput(["print", "\(domain(spec))/\(Self.label)"])) ?? ""
        if let pidLine = out.split(separator: "\n").first(where: { $0.contains("pid = ") }) {
            let digits = pidLine.split(separator: "=").last?.trimmingCharacters(in: .whitespaces) ?? ""
            if let pid = Int(digits), pid > 0 {
                st.pid = pid
                st.running = true
            }
        } else if out.contains("state = running") {
            st.running = true
        }
        return st
    }

    private func renderPlist(_ spec: ServiceSpec) -> String {
        let exec = spec.execPath.isEmpty
            ? (Bundle.main.executablePath ?? CommandLine.arguments[0])
            : spec.execPath
        let home = NSHomeDirectory()
        let workDir = (exec as NSString).deletingLastPathComponent
        let logDir = spec.configDir.isEmpty ? home + "/.config/gn-drive" : spec.configDir
        return """
        <?xml version="1.0" encoding="UTF-8"?>
        <!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
        <plist version="1.0">
        <dict>
          <key>Label</key>
          <string>\(Self.label)</string>
          <key>ProgramArguments</key>
          <array>
            <string>\(exec)</string>
            <string>run</string>
            <string>--service</string>
          </array>
          <key>RunAtLoad</key>
          <true/>
          <key>KeepAlive</key>
          <dict>
            <key>SuccessfulExit</key>
            <false/>
            <key>Crashed</key>
            <true/>
          </dict>
          <key>ThrottleInterval</key>
          <integer>10</integer>
          <key>EnvironmentVariables</key>
          <dict>
            <key>GN_DRIVE_MODE</key>
            <string>service</string>
            <key>HOME</key>
            <string>\(home)</string>
          </dict>
          <key>WorkingDirectory</key>
          <string>\(workDir)</string>
          <key>StandardOutPath</key>
          <string>\(logDir)/gn-drive.out.log</string>
          <key>StandardErrorPath</key>
          <string>\(logDir)/gn-drive.err.log</string>
        </dict>
        </plist>
        """
    }

    /// Drop legacy ~/Library/LaunchAgents/gn-drive.plist (non reverse-DNS
    /// label rejected by modern launchd).
    private func cleanupLegacyAgent(_ spec: ServiceSpec) {
        guard spec.scope == .user else { return }
        let legacy = NSHomeDirectory() + "/\(Self.userPlistDir)/\(Self.legacyAgentName).plist"
        if FileManager.default.fileExists(atPath: legacy) {
            try? runLaunchctl(["bootout", domain(spec), legacy])
            try? FileManager.default.removeItem(atPath: legacy)
        }
    }

    @discardableResult
    private func runLaunchctlOutput(_ args: [String]) throws -> String {
        let proc = Process()
        proc.executableURL = URL(fileURLWithPath: "/bin/launchctl")
        proc.arguments = args
        let out = Pipe(), err = Pipe()
        proc.standardOutput = out
        proc.standardError = err
        try proc.run()
        let outData = out.fileHandleForReading.readDataToEndOfFile()
        let errData = err.fileHandleForReading.readDataToEndOfFile()
        proc.waitUntilExit()
        let stdout = String(data: outData, encoding: .utf8) ?? ""
        let stderr = String(data: errData, encoding: .utf8) ?? ""
        guard proc.terminationStatus == 0 else {
            throw ServiceError.launchctl(stderr.isEmpty ? stdout : stderr)
        }
        return stdout
    }

    private func runLaunchctl(_ args: [String]) throws {
        _ = try runLaunchctlOutput(args)
    }
}
