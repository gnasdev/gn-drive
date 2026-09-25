// Application wiring — port of internal/app/app.go.
// Constructor-based DI: builds engines before the data plane (store/rclone)
// and attaches the data plane on unlock.
import Foundation

public enum AppError: Error, LocalizedError {
    case locked
    case rcloneMissing(Error)

    public var errorDescription: String? {
        switch self {
        case .locked: return "auth: app is locked — provide --password"
        case .rcloneMissing(let e): return "init rclone: \(e)"
        }
    }
}

public final class AppContainer: @unchecked Sendable {
    public let config: Paths
    public let bus = EventBus()
    public let log: Logger
    public private(set) var store: Store?
    public let auth: AuthService
    public private(set) var rclone: RcloneClient?
    public let syncEngine: SyncEngine
    public let boardEngine: BoardEngine
    public let flowEngine: FlowEngine
    public let runtime: RuntimeHub
    public var health: HealthWriter?
    public let version: String

    private let rcloneBinary: String?
    private let portalMode: Bool
    private let mu = NSLock()

    public struct Options {
        public var configDir: String = ""
        public var logMode: LogMode = .foreground
        public var rcloneBinary: String = ""
        /// Unlock at process start (service / CI / CLI --password).
        public var unlockPassword: String = ""
        /// Start even while locked (app shows unlock UI; data plane deferred).
        public var portalMode = false
        public var version = "dev"
        /// Session resumption via the OS keychain (interactive UI sessions).
        public var keyStore: SecureStore? = nil
        public init() {}
    }

    public init(opts: Options) throws {
        var cfg = Paths.detect()
        if !opts.configDir.isEmpty { cfg.configDir = opts.configDir }
        try cfg.ensureConfigDir()
        self.config = cfg

        let log = Logger(mode: opts.logMode)
        self.log = opts.logMode == .service ? log.with("service", "gn-drive") : log

        auth = try AuthService(configDir: cfg.configDir, logger: log, keyStore: opts.keyStore)
        if !opts.unlockPassword.isEmpty {
            try auth.unlock(opts.unlockPassword)
        }
        if !opts.portalMode, auth.isSetup, !auth.isUnlocked {
            throw AppError.locked
        }
        if opts.portalMode, auth.isSetup, !auth.isUnlocked {
            log.info("portal: starting locked — unlock via UI")
        }

        self.version = opts.version.isEmpty ? "dev" : opts.version
        self.rcloneBinary = opts.rcloneBinary.isEmpty ? nil : opts.rcloneBinary
        self.portalMode = opts.portalMode

        syncEngine = SyncEngine(logger: log, bus: bus)
        boardEngine = BoardEngine(store: nil, sync: nil, bus: bus, log: log)
        flowEngine = FlowEngine(store: nil, syncEngine: syncEngine, bus: bus, log: log)
        syncEngine.setFlowExecutor(flowEngine)
        runtime = RuntimeHub(bus: bus, flows: flowEngine, tasks: syncEngine)

        if Self.canOpenDataPlane(auth: auth) {
            try openDataPlane()
        } else {
            log.info("portal: data plane deferred until unlock (encrypted config)")
        }
    }

    /// Whether sqlite/rclone config files are readable now.
    /// Unlocked, never-setup, or locked-with-plaintext all qualify.
    static func canOpenDataPlane(auth: AuthService) -> Bool {
        if !auth.isSetup || auth.isUnlocked { return true }
        return !auth.hasEncryptedConfig()
    }

    /// Open store + rclone and attach them to engines.
    public func openDataPlane() throws {
        mu.lock(); defer { mu.unlock() }
        if store != nil { return }

        let dbPath = (config.configDir as NSString).appendingPathComponent("gn-drive.db")
        let st = try Store(path: dbPath, logger: log)

        let rcloneCfg = (config.configDir as NSString).appendingPathComponent("rclone.conf")
        do {
            let rc = try RcloneClient(binaryPath: rcloneBinary, configPath: rcloneCfg, logger: log)
            store = st
            rclone = rc
            syncEngine.attach(store: st, rclone: rc)
            boardEngine.attach(store: st, sync: rc)
            flowEngine.attach(store: st, syncEngine: syncEngine)
        } catch {
            st.close()
            throw AppError.rcloneMissing(error)
        }
        log.info("data plane ready", ("db", dbPath))
    }

    /// Called after auth unlock/setup — opens deferred data plane.
    public func afterUnlock() throws {
        try openDataPlane()
    }

    /// Close the data plane so auth can re-encrypt config safely.
    public func beforeLock() {
        mu.lock(); defer { mu.unlock() }
        syncEngine.detach()
        boardEngine.detach()
        flowEngine.detach()
        runtime.reset()
        store?.close()
        store = nil
        rclone = nil
    }

    public func close() {
        health?.stop()
        runtime.close()
        bus.close()
        beforeLock()
        _ = auth.suspend()
    }
}
