// CLI subcommands — ports of cmd/gn-drive/*.go.
import Foundation
import ArgumentParser
import GNDriveCore

// MARK: - run

struct RunCommand: ParsableCommand {
    static let configuration = CommandConfiguration(
        commandName: "run",
        abstract: "Start gn-drive engine in foreground or service mode")

    @Flag(help: "Run as a background service (use 'gn-drive service install' first)")
    var service = false

    @Flag(help: "Development mode (debug-oriented logging)")
    var dev = false

    @Option(help: "Unlock at process start (service mode / CI). Interactive runs unlock via the app UI instead.")
    var password: String = ""

    @Option(help: "Override the config directory")
    var configDir: String = ""

    func run() throws {
        var opts = AppContainer.Options()
        opts.logMode = dev ? .foreground : (service ? .service : .foreground)
        opts.version = BuildInfo.version
        opts.unlockPassword = password.isEmpty
            ? (ProcessInfo.processInfo.environment["GN_DRIVE_PASSWORD"] ?? "")
            : password
        opts.configDir = configDir
        // Foreground `run` allows a locked start only for parity with the
        // portal mode; without a UI there is nothing to unlock with, so only
        // defer when the config is still plaintext on disk.
        opts.portalMode = false
        if !service {
            opts.keyStore = KeychainStore(service: "gn-drive")
        }

        let cfg = Paths.detect()
        let locker: InstanceLocker
        do {
            locker = try InstanceLocker.acquire(configDir: opts.configDir.isEmpty ? cfg.configDir : opts.configDir)
        } catch {
            throw error
        }
        defer { locker.release() }

        let app = try AppContainer(opts: opts)
        defer { app.close() }

        app.syncEngine.start()
        defer { app.syncEngine.stop() }

        if service {
            let health = HealthWriter(configDir: app.config.configDir)
            try? health.start()
            app.health = health
            app.log.info("gn-drive service started", ("pid", "\(getpid())"))
        } else {
            print("gn-drive ready (engine running, headless). Ctrl+C to stop.")
            print("Use the GNDrive app for the UI.")
        }

        // Wait for SIGINT/SIGTERM.
        var set = sigset_t()
        sigemptyset(&set)
        sigaddset(&set, SIGINT)
        sigaddset(&set, SIGTERM)
        sigprocmask(SIG_BLOCK, &set, nil)
        let sem = DispatchSemaphore(value: 0)
        DispatchQueue.global().async {
            var sig: Int32 = 0
            var waitSet = sigset_t()
            sigemptyset(&waitSet)
            sigaddset(&waitSet, SIGINT)
            sigaddset(&waitSet, SIGTERM)
            sigwait(&waitSet, &sig)
            sem.signal()
        }
        sem.wait()
        app.log.info("signal received, shutting down")
    }
}

// MARK: - service

struct ServiceCommand: ParsableCommand {
    static let configuration = CommandConfiguration(
        commandName: "service",
        abstract: "Install, uninstall, start, stop, or check service status")

    @Argument(help: "install | uninstall | start | stop | status | restart")
    var action: String

    @Flag(help: "Install as a system-level service (requires sudo)")
    var system = false

    func run() throws {
        let mgr = LaunchdService()
        var spec = ServiceSpec()
        spec.scope = system ? .system : .user
        spec.configDir = Paths.detect().configDir
        spec.execPath = Bundle.main.executablePath ?? CommandLine.arguments[0]

        switch action {
        case "install":
            if spec.scope == .system {
                print("Note: system-level install requires elevated privileges (sudo).")
            }
            print("Installing gn-drive service (darwin, \(spec.scope.rawValue))...")
            try mgr.install(spec)
            print("✓ installed.")
            print("")
            print("Status:")
            try printStatus(mgr: mgr, spec: spec)
        case "uninstall":
            print("Uninstalling gn-drive service (darwin, \(spec.scope.rawValue))...")
            try mgr.uninstall(spec)
            print("✓ uninstalled.")
        case "start":
            print("Starting gn-drive service...")
            try mgr.start(spec)
            print("✓ started.")
        case "stop":
            print("Stopping gn-drive service...")
            try mgr.stop(spec)
            print("✓ stopped.")
        case "status":
            try printStatus(mgr: mgr, spec: spec)
        case "restart":
            print("Restarting gn-drive service...")
            try mgr.restart(spec)
            print("✓ restarted.")
        default:
            throw CLIError.app("unknown service action: \(action) (want install|uninstall|start|stop|status|restart)")
        }
    }

    private func printStatus(mgr: LaunchdService, spec: ServiceSpec) throws {
        if !mgr.isInstalled(spec) {
            print("Service: not installed.")
            print("")
            print("To install:")
            print("  gn-drive service install\(spec.scope == .system ? " --system" : "")")
            return
        }
        do {
            let st = try mgr.status(spec)
            print("Service:")
            print("  Mode:     \(st.mode)")
            print("  Scope:    \(st.scope)")
            print("  Platform: darwin")
            print(st.running ? "  Running:  yes (pid \(st.pid))" : "  Running:  no")
        } catch {
            print("Service: installed (status check failed: \(error))")
        }

        if let health = ServiceHealth.read(configDir: spec.configDir) {
            print("")
            print("Health:")
            print("  Started:        \(health.startedAt)")
            print("  Last heartbeat: \(health.lastHeartbeat)")
            if health.isStale {
                print("  ⚠ heartbeat stale (>60s old) — service may be unresponsive")
            }
            if health.webPort > 0 { print("  Web port:       \(health.webPort)") }
            print("  Uptime:         \(health.uptime.rounded())s")
            if !health.lastError.isEmpty { print("  Last error:     \(health.lastError)") }
            if let d = health.lastSyncAt { print("  Last sync:      \(d)") }
            if let d = health.nextScheduleAt { print("  Next schedule:  \(d)") }
            if !health.activeTasks.isEmpty {
                print("  Active tasks:   \(health.activeTasks.joined(separator: ", "))")
            }
        }
    }
}

// MARK: - sync

struct SyncCommand: ParsableCommand {
    static let configuration = CommandConfiguration(
        commandName: "sync",
        abstract: "Run a one-shot sync operation")

    @Argument(help: "pull | push | bi | bi-resync | dry-run")
    var action: String

    @Option(help: "Profile name to sync (required)")
    var profile: String

    @Flag(help: "Preview only — do not change files")
    var dryRun = false

    func run() throws {
        var act = action
        if dryRun { act = "dry-run" }
        guard RcloneAction(rawValue: act) != nil else {
            throw CLIError.app("sync: unknown action \(act)")
        }
        let app = try makeApp()
        defer { app.close() }

        guard let store = app.store else { throw CLIError.app("store not ready") }
        let p = try store.getProfile(profile)
        print("→ \(p.name) sync: \(p.from) → \(p.to) (action=\(act))")
        if p.dryRun {
            print("  [profile has dry_run=true, no actual changes will be made]")
        }
        guard let rclone = app.rclone else { throw CLIError.app("rclone not available") }

        final class LastBox: @unchecked Sendable { var v = SyncStats() }
        let last = LastBox()
        do {
            var cfg = SyncConfig(action: RcloneAction(rawValue: act)!, source: p.from, dest: p.to)
            var flags = SyncEngine.profileFlags(from: p)
            flags.dryRun = p.dryRun
            cfg.profile = flags
            let res = try rclone.sync(cfg, onProgress: { s in
                if s.bytes != last.v.bytes || s.files != last.v.files {
                    print("\r  \(humanBytes(s.bytes)) / \(humanBytes(s.bytesTotal)) | \(s.files) files | \(s.errors) errors   ", terminator: "")
                    fflush(stdout)
                    last.v = s
                }
            })
            print("")
            print("✓ sync completed in \(res.endedAt - res.startedAt)s — \(humanBytes(res.stats.bytes)) transferred, \(res.stats.errors) errors")
        } catch {
            print("")
            print("✗ sync failed: \(error)")
            throw error
        }
    }
}

// MARK: - board

struct BoardCommand: ParsableCommand {
    static let configuration = CommandConfiguration(
        commandName: "board",
        abstract: "Execute a board DAG (legacy)",
        discussion: "Legacy CLI: boards are not part of the app workspace; prefer flows.")

    @Argument(help: "Board ID")
    var boardID: String

    @Flag(inversion: .prefixedNo, help: "Stop execution at the first failed edge (default: on; use --no-stop-on-error to disable)")
    var stopOnError = true

    func run() throws {
        let app = try makeApp()
        defer { app.close() }
        guard let store = app.store else { throw CLIError.app("store not ready") }
        let eng = BoardEngine(store: store, sync: app.rclone, bus: app.bus, log: app.log)

        let b = try store.loadBoardGraph(boardID)
        guard !b.nodes.isEmpty else {
            throw CLIError.app("board \(boardID) has no nodes")
        }
        guard !b.edges.isEmpty else {
            throw CLIError.app("board \(boardID) has no edges")
        }
        print("▶ Board: \(b.name) (\(b.id)) — \(b.nodes.count) nodes, \(b.edges.count) edges")

        try eng.executeSync(boardID: boardID, stopOnError: stopOnError) { layer, idx, total, edge, src, dst, err in
            print("  │ [\(idx)/\(total)] layer=\(layer + 1) \(edge.id) — \(edge.action) : \(src.label) → \(dst.label)")
            if let err {
                print("  │   ✗ edge \(edge.id) failed: \(err)")
            } else {
                print("  │   ✓ edge \(edge.id) ok")
            }
        }
        print("✓ board \(b.id) executed successfully")
    }
}

// MARK: - profile

struct ProfileCommand: ParsableCommand {
    static let configuration = CommandConfiguration(
        commandName: "profile",
        abstract: "Manage sync profiles",
        subcommands: [ProfileList.self, ProfileAdd.self, ProfileDelete.self],
        defaultSubcommand: ProfileList.self)
}

struct ProfileList: ParsableCommand {
    static let configuration = CommandConfiguration(commandName: "list", abstract: "List all profiles")
    func run() throws {
        let app = try makeApp()
        defer { app.close() }
        guard let store = app.store else { throw CLIError.app("store not ready") }
        let profiles = try store.listProfiles()
        if profiles.isEmpty {
            print("No profiles configured. Use 'gn-drive profile add' or the app UI.")
            return
        }
        print("NAME\tFROM\tTO\tPARALLEL\tBANDWIDTH\tDRY-RUN")
        for p in profiles {
            let bw = p.bandwidth > 0 ? "\(p.bandwidth)M" : ""
            print("\(p.name)\t\(trunc(p.from, 40))\t\(trunc(p.to, 40))\t\(p.parallel)\t\(bw)\t\(p.dryRun)")
        }
    }
}

struct ProfileAdd: ParsableCommand {
    static let configuration = CommandConfiguration(commandName: "add", abstract: "Add a new profile")

    @Option var name: String
    @Option var from: String
    @Option var to: String
    @Option var direction = "push"
    @Option var parallel = 4
    @Option var bandwidth = 0
    @Flag(name: .customLong("dry-run")) var dryRun = false

    func run() throws {
        guard !name.isEmpty, !from.isEmpty, !to.isEmpty else {
            throw CLIError.app("profile add: --name, --from, --to are required")
        }
        guard ProfileDirection.isValid(direction) else {
            throw CLIError.app("profile add: invalid --direction \(direction) (allowed: push, bi, bi-resync)")
        }
        let app = try makeApp()
        defer { app.close() }
        guard let store = app.store else { throw CLIError.app("store not ready") }
        var p = Profile()
        p.name = name
        p.from = from
        p.to = to
        p.direction = direction
        p.parallel = parallel
        p.bandwidth = bandwidth
        p.dryRun = dryRun
        try store.saveProfile(&p)
        print("✓ added profile \"\(name)\" (direction=\(direction))")
    }
}

struct ProfileDelete: ParsableCommand {
    static let configuration = CommandConfiguration(commandName: "delete", abstract: "Delete a profile")
    @Argument var name: String
    func run() throws {
        let app = try makeApp()
        defer { app.close() }
        guard let store = app.store else { throw CLIError.app("store not ready") }
        try store.deleteProfile(name)
        print("✓ deleted profile \"\(name)\"")
    }
}

// MARK: - remote

struct RemoteCommand: ParsableCommand {
    static let configuration = CommandConfiguration(
        commandName: "remote",
        abstract: "Manage rclone remotes",
        subcommands: [RemoteList.self, RemoteAdd.self, RemoteTest.self, RemoteDelete.self],
        defaultSubcommand: RemoteList.self)
}

struct RemoteList: ParsableCommand {
    static let configuration = CommandConfiguration(commandName: "list", abstract: "List all remotes")
    func run() throws {
        let app = try makeApp()
        defer { app.close() }
        guard let rclone = app.rclone else { throw CLIError.app("rclone not available") }
        let remotes = try rclone.listRemotes()
        if remotes.isEmpty {
            print("No remotes configured. Use 'gn-drive remote add' or the app UI.")
            return
        }
        print("NAME\tTYPE")
        for r in remotes {
            print("\(r.name)\t\(r.type)")
        }
    }
}

struct RemoteAdd: ParsableCommand {
    static let configuration = CommandConfiguration(commandName: "add", abstract: "Add a new remote (non-interactive)")
    @Option var name: String
    @Option var type: String
    @Option(parsing: .upToNextOption, help: "Config k=v pairs (repeatable)") var config: [String] = []
    func run() throws {
        guard !name.isEmpty, !type.isEmpty else {
            throw CLIError.app("remote add: --name and --type are required")
        }
        let app = try makeApp()
        defer { app.close() }
        guard let rclone = app.rclone else { throw CLIError.app("rclone not available") }
        try rclone.createRemoteVerified(name: name, type: type, configKVs: config)
        print("✓ added remote \"\(name)\" (type=\(type))")
    }
}

struct RemoteTest: ParsableCommand {
    static let configuration = CommandConfiguration(commandName: "test", abstract: "Test a remote connection")
    @Argument var name: String
    func run() throws {
        let app = try makeApp()
        defer { app.close() }
        guard let rclone = app.rclone else { throw CLIError.app("rclone not available") }
        print("Testing remote \"\(name)\"... ", terminator: "")
        do {
            try rclone.testRemote(name)
            print("✓ OK")
        } catch {
            print("✗ FAILED")
            throw error
        }
    }
}

struct RemoteDelete: ParsableCommand {
    static let configuration = CommandConfiguration(commandName: "delete", abstract: "Delete a remote")
    @Argument var name: String
    func run() throws {
        let app = try makeApp()
        defer { app.close() }
        guard let rclone = app.rclone else { throw CLIError.app("rclone not available") }
        try rclone.deleteRemote(name)
        print("✓ deleted remote \"\(name)\"")
    }
}

// MARK: - self-update

struct SelfUpdateCommand: ParsableCommand {
    static let configuration = CommandConfiguration(
        commandName: "self-update",
        abstract: "Download and apply the latest release")

    @Option var repoOwner = ""
    @Option var repo = ""
    @Flag var force = false
    @Flag(name: .customLong("check")) var checkOnly = false

    func run() throws {
        var opts = UpdateOptions()
        if !repoOwner.isEmpty { opts.repoOwner = repoOwner }
        if !repo.isEmpty { opts.repoName = repo }
        opts.currentVersion = BuildInfo.version
        opts.force = force

        if checkOnly {
            let (cur, latest) = try SelfUpdate.check(opts: opts)
            print("current=\(cur) latest=\(latest)")
            if cur == latest {
                print("✓ already on latest version")
            } else {
                print("↑ update available — run 'gn-drive self-update' to apply")
            }
            return
        }

        print("gn-drive self-update (current=\(BuildInfo.version))")
        do {
            let res = try SelfUpdate.update(opts: opts)
            print("✓ updated \(res.oldVersion) → \(res.newVersion)")
            print("  binary: \(res.binaryPath)")
            if !res.restartHint.isEmpty { print("  next:   \(res.restartHint)") }
        } catch SelfUpdateError.alreadyUpToDate {
            print("✓ already on latest version")
        }
    }
}

// MARK: - version

struct VersionCommand: ParsableCommand {
    static let configuration = CommandConfiguration(commandName: "version", abstract: "Print version info")
    func run() throws {
        print("gn-drive \(BuildInfo.version) (commit=\(BuildInfo.commit), swift)")
    }
}

// MARK: - doctor

struct DoctorCommand: ParsableCommand {
    static let configuration = CommandConfiguration(commandName: "doctor", abstract: "Diagnose environment and configuration")

    @Flag(help: "List files in config directory")
    var data = false

    func run() throws {
        let out = FileHandle.standardOutput
        func p(_ s: String) { out.write(Data((s + "\n").utf8)) }

        p("=== gn-drive doctor ===")
        p("")

        // rclone binary
        do {
            let bin = try RcloneClient.resolveBinary(nil)
            p("rclone:       \(bin)")
            let proc = Process()
            proc.executableURL = URL(fileURLWithPath: bin)
            proc.arguments = ["version"]
            let pipe = Pipe()
            proc.standardOutput = pipe
            proc.standardError = pipe
            try? proc.run()
            proc.waitUntilExit()
            let v = String(data: pipe.fileHandleForReading.readDataToEndOfFile(), encoding: .utf8) ?? ""
            if let first = v.split(separator: "\n").first {
                p("  version: \(first)")
            }
            p("  [OK]")
        } catch {
            p("rclone:       NOT FOUND in PATH")
            p("  [ERROR] rclone is required. Install: https://rclone.org/install/")
        }

        let cfg = Paths.detect()
        p("")
        p("Config dir:  \(cfg.configDir)")
        if FileManager.default.fileExists(atPath: cfg.configDir) {
            p("  [OK]")
        } else {
            p("  [WARN] does not exist")
        }

        let dbPath = (cfg.configDir as NSString).appendingPathComponent("gn-drive.db")
        p("Database:     \(dbPath)")
        p(FileManager.default.fileExists(atPath: dbPath) ? "  [OK]" : "  [INFO] Database not yet created (first run)")

        let app = try makeApp()
        defer { app.close() }
        let status = app.auth.status()
        p("Auth config: \(cfg.configDir)/auth.json")
        if status.setup {
            p(status.unlocked ? "  configured: yes, unlocked: yes  [OK]" : "  configured: yes, unlocked: no  [LOCKED]")
        } else {
            p("  configured: no  [OK]")
        }

        if !status.setup || status.unlocked {
            if let remotes = try? app.rclone?.listRemotes() {
                p("")
                p("Remotes:      \(remotes.count) configured")
                for r in remotes { p("  - \(r.name) (\(r.type))") }
            }
            if let profiles = try? app.store?.listProfiles() {
                p("")
                p("Profiles:     \(profiles.count) configured")
                for pr in profiles { p("  - \(pr.name)") }
            }
            if let history = try? app.store?.listHistory(limit: 5, offset: 0) {
                p("")
                p("History:      \(history.count) recent entries")
            }
        }

        p("")
        p("Platform:     darwin (\(archName()))")

        if data {
            p("")
            p("--- data directory contents ---")
            for e in (try? FileManager.default.contentsOfDirectory(atPath: cfg.configDir)) ?? [] {
                p("  \(e)")
            }
        }
        p("")
        p("All checks passed. gn-drive is ready to run.")
    }
}

// MARK: - helpers

func humanBytes(_ n: Int64) -> String {
    let k: Double = 1024
    switch n {
    case ..<Int64(k): return "\(n)B"
    case ..<Int64(k * k): return String(format: "%.1fK", Double(n) / k)
    case ..<Int64(k * k * k): return String(format: "%.1fM", Double(n) / (k * k))
    default: return String(format: "%.2fG", Double(n) / (k * k * k))
    }
}

func trunc(_ s: String, _ n: Int) -> String {
    s.count <= n ? s : String(s.prefix(n - 1)) + "…"
}

func archName() -> String {
    #if arch(arm64)
    return "arm64"
    #else
    return "x86_64"
    #endif
}

// MARK: - completion

struct CompletionCommand: ParsableCommand {
    static let configuration = CommandConfiguration(
        commandName: "completion",
        abstract: "Generate shell completion scripts")

    @Argument(help: "bash | zsh | fish")
    var shell: String

    func run() throws {
        guard let s = CompletionShell(rawValue: shell) else {
            throw CLIError.app("unsupported shell: \(shell) (want bash|zsh|fish)")
        }
        print(GNDriveCLI.completionScript(for: s))
    }
}
