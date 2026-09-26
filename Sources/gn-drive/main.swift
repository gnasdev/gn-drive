// gn-drive CLI entry point — port of cmd/gn-drive (cobra → ArgumentParser).
import Foundation
import ArgumentParser
import GNDriveCore

enum CLIError: Error, LocalizedError {
    case app(String)

    var errorDescription: String? {
        switch self {
        case .app(let m): return m
        }
    }
}

/// Build-time version (overridable via -D GN_VERSION at build config level;
/// simplest: read from compiled constant).
public enum BuildInfo {
    public static let version = BuildVersion.current
    public static let commit = "unknown"
}

@main
struct GNDriveCLI: ParsableCommand {
    static let configuration = CommandConfiguration(
        commandName: "gn-drive",
        abstract: "GN Drive — local-only sync engine and native macOS app",
        discussion: """
        Subcommands:
          run          Start the engine in foreground or service mode
          service      Install, uninstall, start, stop, or check service status
          sync         One-shot sync (pull, push, bi, bi-resync, dry-run)
          board        (legacy) Execute a board DAG — prefer flows
          profile      Manage sync profiles
          remote       Manage rclone remotes
          self-update  Download and apply updates from GitHub Releases
          version      Print version info
          doctor       Diagnose environment and configuration
        """,
        version: BuildInfo.version,
        subcommands: [
            RunCommand.self,
            ServiceCommand.self,
            SyncCommand.self,
            BoardCommand.self,
            ProfileCommand.self,
            RemoteCommand.self,
            SelfUpdateCommand.self,
            VersionCommand.self,
            DoctorCommand.self,
            CompletionCommand.self,
        ]
    )
}

/// Shared helper: build the app container for CLI one-shot commands.
func makeApp(password: String? = nil, configDir: String? = nil) throws -> AppContainer {
    var opts = AppContainer.Options()
    opts.logMode = .foreground
    opts.version = BuildInfo.version
    if let password { opts.unlockPassword = password }
    if let configDir { opts.configDir = configDir }
    return try AppContainer(opts: opts)
}
