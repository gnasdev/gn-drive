// Sync orchestration engine — port of internal/syncengine.
// Task registry, profile & flow cron schedules, event emission.
import Foundation

public enum SyncEngineError: Error, Equatable {
    case notRunning
    case storeNotReady
    case profileBusy
    case taskFailed(String)
    case taskCancelled
    case taskNotFound
}

/// A running or completed sync task.
public final class SyncTask: @unchecked Sendable {
    public let id: String
    public let name: String // profile name / busyKey
    public let action: String
    private let lock = NSLock()
    private var _status = "running"
    private var _error = ""
    private var _stats = SyncStats()
    private var _startedAt = Date()
    private var _endedAt: Date? = nil
    let token = CancellationToken()

    init(id: String, name: String, action: String) {
        self.id = id; self.name = name; self.action = action
    }

    public var status: String { lock.lock(); defer { lock.unlock() }; return _status }
    public var error: String { lock.lock(); defer { lock.unlock() }; return _error }
    public var stats: SyncStats { lock.lock(); defer { lock.unlock() }; return _stats }
    public var startedAt: Date { lock.lock(); defer { lock.unlock() }; return _startedAt }
    public var endedAt: Date? { lock.lock(); defer { lock.unlock() }; return _endedAt }

    func setStatus(_ s: String) { lock.lock(); _status = s; lock.unlock() }
    func setError(_ e: String) { lock.lock(); _error = e; lock.unlock() }
    func setStats(_ s: SyncStats) { lock.lock(); _stats = s; lock.unlock() }
    func setStartedAt(_ d: Date) { lock.lock(); _startedAt = d; lock.unlock() }
    func setEndedAt(_ d: Date) { lock.lock(); _endedAt = d; lock.unlock() }

    public func cancel() {
        token.cancel()
        setStatus("cancelled")
    }

    public var isCancelled: Bool { token.isCancelled }

    public struct Snapshot: Sendable {
        public let id: String, name: String, action: String, status: String
        public let stats: SyncStats
        public let startedAt: Date
        public let endedAt: Date?
    }

    public func snapshot() -> Snapshot {
        Snapshot(id: id, name: name, action: action, status: status,
                 stats: stats, startedAt: startedAt, endedAt: endedAt)
    }
}

/// Terminal outcome retained after a task leaves the active map.
struct TaskOutcome {
    var status: String // completed | failed | cancelled
    var errMsg: String
}

public protocol SyncClient: Sendable {
    func sync(_ cfg: SyncConfig, onProgress: (@Sendable (SyncStats) -> Void)?,
              token: CancellationToken?) throws -> SyncResult
}

extension RcloneClient: SyncClient {}

public protocol FlowExecutor: Sendable {
    func execute(flowID: String) throws
}

public final class SyncEngine: @unchecked Sendable {
    private let log: Logger
    private let bus: EventBus
    private var store: Store?
    private var rclone: SyncClient?
    private var flowExec: FlowExecutor?

    private let cron = CronRunner()
    private var started = false
    private var running = false // engine-level ctx equivalent
    private let stateLock = NSLock()

    private let activeLock = NSLock()
    private var active: [String: SyncTask] = [:]
    private var outcomes: [String: TaskOutcome] = [:]
    private var busyKeys: Set<String> = []

    private let schedLock = NSLock()
    private var registeredSchedules: Set<String> = []

    public init(logger: Logger, bus: EventBus, store: Store? = nil, rclone: SyncClient? = nil) {
        self.log = logger
        self.bus = bus
        self.store = store
        self.rclone = rclone
    }

    public func setFlowExecutor(_ fe: FlowExecutor) { flowExec = fe }

    /// Wire store+rclone after portal unlock (deferred data plane).
    public func attach(store: Store, rclone: SyncClient) {
        self.store = store
        self.rclone = rclone
        stateLock.lock()
        let running = self.running
        stateLock.unlock()
        if running { loadSchedules() }
    }

    public func detach() {
        store = nil
        rclone = nil
    }

    public func start() {
        stateLock.lock()
        if running { stateLock.unlock(); return }
        running = true
        stateLock.unlock()
        cron.start()
        if store != nil { loadSchedules() }
        log.info("syncengine: started")
    }

    public func stop() {
        stateLock.lock()
        running = false
        stateLock.unlock()
        cron.stop()
        activeLock.lock()
        let tasks = Array(active.values)
        active.removeAll()
        busyKeys.removeAll()
        activeLock.unlock()
        for t in tasks { t.cancel() }
        schedLock.lock(); registeredSchedules.removeAll(); schedLock.unlock()
        log.info("syncengine: stopped")
    }

    public var isRunning: Bool {
        stateLock.lock(); defer { stateLock.unlock() }; return running
    }

    // MARK: - Task lifecycle

    /// Start a sync for a named profile; returns taskID.
    @discardableResult
    public func startSync(action: String, profileName: String) throws -> String {
        guard isRunning else { throw SyncEngineError.notRunning }
        guard let store else { throw SyncEngineError.storeNotReady }
        let p = try store.getProfile(profileName)
        return try startWithProfile(action: action, busyKey: profileName, profile: p)
    }

    /// Ad-hoc sync without a stored profile (flow operations).
    /// busyKey is the concurrency lock key (e.g. "flowID:opID").
    @discardableResult
    public func startPathSync(action: String, busyKey: String, from: String, to: String,
                              opts: Profile? = nil) throws -> String {
        guard isRunning else { throw SyncEngineError.notRunning }
        guard !from.isEmpty, !to.isEmpty else {
            throw SyncEngineError.taskFailed("syncengine: from and to are required")
        }
        var p = opts ?? Profile()
        p.name = busyKey
        p.from = from
        p.to = to
        if p.parallel <= 0 { p.parallel = 4 }
        return try startWithProfile(action: action, busyKey: busyKey, profile: p)
    }

    private func startWithProfile(action: String, busyKey: String, profile: Profile) throws -> String {
        activeLock.lock()
        if busyKeys.contains(busyKey) {
            activeLock.unlock()
            throw SyncEngineError.profileBusy
        }
        busyKeys.insert(busyKey)
        activeLock.unlock()

        guard isRunning else {
            activeLock.lock(); busyKeys.remove(busyKey); activeLock.unlock()
            throw SyncEngineError.notRunning
        }

        let task = SyncTask(id: UUID().uuidString, name: busyKey, action: action)
        activeLock.lock(); active[task.id] = task; activeLock.unlock()

        DispatchQueue.global().async { [weak self] in
            self?.runSync(task, profile, action)
        }

        bus.publish(BusTopic.syncStarted, SyncStartedEvent(taskID: task.id, profileID: busyKey, action: action))
        return task.id
    }

    /// Block until the task finishes; nil only on success.
    /// `cancelled` is polled each tick — when it returns true the wait ends
    /// with .taskCancelled (the task itself is left running; caller stops it).
    public func waitTask(_ taskID: String, cancelled: () -> Bool = { false }) throws {
        while true {
            if cancelled() { throw SyncEngineError.taskCancelled }
            activeLock.lock()
            let outcome = outcomes.removeValue(forKey: taskID)
            activeLock.unlock()
            if let o = outcome {
                switch o.status {
                case "completed": return
                case "cancelled": throw SyncEngineError.taskCancelled
                default: throw SyncEngineError.taskFailed(o.errMsg)
                }
            }
            Thread.sleep(forTimeInterval: 0.1)
        }
    }

    /// Cancel an active sync task.
    public func stopSync(_ taskID: String) throws {
        activeLock.lock()
        let t = active[taskID]
        activeLock.unlock()
        guard let t else { throw SyncEngineError.taskNotFound }
        t.cancel()
    }

    public func activeTasks() -> [SyncTask.Snapshot] {
        activeLock.lock(); defer { activeLock.unlock() }
        return active.values.map { $0.snapshot() }
    }

    // MARK: - Schedules

    /// Add/update a cron job for a profile schedule.
    public func registerSchedule(_ sch: Schedule) {
        let id = sch.id
        schedLock.lock()
        registeredSchedules.remove(id)
        schedLock.unlock()
        cron.remove(id: id)
        guard sch.enabled, !sch.cron.isEmpty else { return }
        guard let cronExpr = try? CronSchedule(sch.cron) else {
            log.warn("cron: invalid expression", ("schedule", id), ("cron", sch.cron))
            return
        }
        let ok = cron.add(id: id, schedule: cronExpr) { [weak self] in
            self?.triggerSchedule(sch)
        }
        if ok {
            schedLock.lock(); registeredSchedules.insert(id); schedLock.unlock()
            log.info("cron: registered", ("schedule", id), ("cron", sch.cron))
        }
    }

    private func triggerSchedule(_ sch: Schedule) {
        log.info("cron: triggering", ("schedule", sch.id), ("profile", sch.profileName))
        bus.publish(BusTopic.scheduleTriggered,
                    ScheduleTriggeredEvent(scheduleID: sch.id, profileID: sch.profileName, action: sch.action))
        do {
            _ = try startSync(action: sch.action, profileName: sch.profileName)
        } catch {
            log.warn("cron: sync not started", ("schedule", sch.id), ("err", String(describing: error)))
        }
    }

    public func unregisterSchedule(_ id: String) {
        schedLock.lock(); registeredSchedules.remove(id); schedLock.unlock()
        cron.remove(id: id)
    }

    private func loadSchedules() {
        guard let store else { return }
        if let schedules = try? store.listSchedules() {
            for sch in schedules { registerSchedule(sch) }
        } else {
            log.warn("syncengine: load schedules failed")
        }
        loadFlowSchedules()
    }

    private func loadFlowSchedules() {
        guard let store, let flows = try? store.listFlows() else { return }
        for f in flows { syncFlowSchedule(f) }
    }

    public static let flowSchedulePrefix = "flow:"
    public static func flowScheduleID(_ flowID: String) -> String {
        flowSchedulePrefix + flowID.trimmingCharacters(in: .whitespaces)
    }

    /// Register/remove a flow's cron job from its persisted fields.
    public func syncFlowSchedule(_ f: Flow) {
        guard !f.id.trimmingCharacters(in: .whitespaces).isEmpty else { return }
        let id = Self.flowScheduleID(f.id)
        cron.remove(id: id)
        schedLock.lock(); registeredSchedules.remove(id); schedLock.unlock()
        let enabled = f.scheduleEnabled
        let cronExpr = f.scheduleCron.trimmingCharacters(in: .whitespaces)
        guard enabled, !cronExpr.isEmpty, flowExec != nil else { return }
        guard let sched = try? CronSchedule(cronExpr) else {
            log.warn("cron: flow schedule invalid", ("flow", f.id), ("cron", cronExpr))
            return
        }
        let flowID = f.id
        let ok = cron.add(id: id, schedule: sched) { [weak self] in
            self?.triggerFlowSchedule(flowID)
        }
        if ok {
            schedLock.lock(); registeredSchedules.insert(id); schedLock.unlock()
            log.info("cron: registered flow", ("flow", f.id), ("cron", cronExpr))
        }
    }

    public func unregisterFlowSchedule(_ flowID: String) {
        unregisterSchedule(Self.flowScheduleID(flowID))
    }

    private func triggerFlowSchedule(_ flowID: String) {
        log.info("cron: triggering flow", ("flow", flowID))
        bus.publish(BusTopic.scheduleTriggered,
                    ScheduleTriggeredEvent(scheduleID: Self.flowScheduleID(flowID),
                                           profileID: flowID, action: "flow"))
        guard let flowExec else { return }
        do {
            try flowExec.execute(flowID: flowID)
        } catch {
            log.warn("cron: flow execute", ("flow", flowID), ("err", String(describing: error)))
        }
    }

    // MARK: - Run

    private func runSync(_ t: SyncTask, _ p: Profile, _ action: String) {
        defer {
            var st = t.status
            if st.isEmpty || st == "running" { st = "failed" }
            activeLock.lock()
            outcomes[t.id] = TaskOutcome(status: st, errMsg: t.error)
            active.removeValue(forKey: t.id)
            busyKeys.remove(p.name)
            activeLock.unlock()
        }

        let startedAt = Date()
        t.setStartedAt(startedAt)
        log.info("sync: started", ("task", t.id), ("profile", p.name), ("action", action))

        // In-progress history row (upserted again at the end).
        saveHistory(HistoryEntry(id: t.id, profileName: p.name, action: action,
                                 state: "running",
                                 startedAt: AuthService.rfc3339(startedAt)))

        var syncErr: Error? = nil
        var result: SyncResult? = nil
        do {
            result = try rclone?.sync(SyncConfig(
                action: RcloneAction(rawValue: action) ?? .push,
                source: p.from, dest: p.to,
                profile: Self.profileFlags(from: p)
            ), onProgress: { [weak self] s in
                t.setStats(s)
                self?.publishSyncProgress(taskID: t.id, profileID: p.name, action: action,
                                         state: "running", stats: s, errMsg: "")
            }, token: t.token)
        } catch {
            syncErr = error
        }
        let endedAt = Date()
        var lastStats = t.stats
        if let res = result { lastStats = res.stats }
        if let e = syncErr as? SyncFailedError { lastStats = e.result.stats }

        let state: String
        if syncErr != nil {
            if t.isCancelled || t.status == "cancelled" {
                state = "cancelled"
            } else {
                state = "failed"
            }
            t.setStatus(state)
            let msg = state == "cancelled"
                ? ""
                : truncateString(sanitizeRcloneErr(describe(syncErr!)), 240)
            t.setError(msg)
            publishSyncProgress(taskID: t.id, profileID: p.name, action: action,
                                state: state, stats: lastStats, errMsg: msg)
            var failed = SyncProgressEvent()
            failed.taskID = t.id
            failed.profileID = p.name
            failed.action = action
            failed.state = state
            failed.errorMessage = msg
            failed.transfers = Self.fileTransferEvents(lastStats)
            failed.eventType = BusTopic.syncFailed
            bus.publish(BusTopic.syncFailed, failed)
            log.error("sync: \(state)", ("task", t.id), ("profile", p.name),
                      ("err", describe(syncErr!)))
        } else {
            state = "completed"
            t.setStatus(state)
            publishSyncProgress(taskID: t.id, profileID: p.name, action: action,
                                state: state, stats: lastStats, errMsg: "")
            bus.publish(BusTopic.syncCompleted, SyncCompletedEvent(
                taskID: t.id, profileID: p.name, action: action,
                duration: (result?.endedAt ?? 0) - (result?.startedAt ?? 0),
                bytes: lastStats.bytes, errors: Int(lastStats.errors)))
            log.info("sync: completed", ("task", t.id), ("profile", p.name),
                     ("bytes", String(lastStats.bytes)))
        }
        t.setEndedAt(endedAt)

        saveHistory(HistoryEntry(
            id: t.id, profileName: p.name, action: action, state: state,
            startedAt: AuthService.rfc3339(startedAt),
            finishedAt: AuthService.rfc3339(endedAt),
            duration: Int64(endedAt.timeIntervalSince(startedAt)),
            bytes: lastStats.bytes, errors: Int(lastStats.errors), files: Int(lastStats.files),
            errorMessage: syncErr.map { truncateString(describe($0), 1000) } ?? ""))
    }

    // MARK: - Helpers

    public static func profileFlags(from p: Profile) -> ProfileFlags {
        var f = ProfileFlags()
        f.transfers = p.parallel
        f.dryRun = p.dryRun
        f.maxAge = p.maxAge
        f.minAge = p.minAge
        f.maxSize = p.maxSize
        f.minSize = p.minSize
        f.excludeIfPresent = p.excludeIfPresent
        f.includes = p.includedPaths
        f.excludes = p.excludedPaths
        f.bufferSize = p.bufferSize
        f.maxDuration = p.maxDuration
        f.retriesSleep = p.retriesSleep
        f.connTimeout = p.connTimeout
        f.ioTimeout = p.ioTimeout
        f.orderBy = p.orderBy
        f.checkFirst = p.checkFirst
        f.immutable = p.immutable
        f.maxTransfer = p.maxTransfer
        f.maxDeleteSize = p.maxDeleteSize
        f.suffix = p.suffix
        f.suffixKeepExtension = p.suffixKeepExtension
        f.backupDir = p.backupPath
        f.sizeOnly = p.sizeOnly
        f.updateMode = p.updateMode
        f.ignoreExisting = p.ignoreExisting
        f.deleteExcluded = p.deleteExcluded
        f.deleteTiming = p.deleteTiming
        f.conflictResolve = p.conflictResolution
        f.conflictLoser = p.conflictLoser
        f.conflictSuffix = p.conflictSuffix
        f.resilient = p.resilient
        f.maxLock = p.maxLock
        f.checkAccess = p.checkAccess
        if p.bandwidth > 0 { f.bandwidth = "\(p.bandwidth)M" }
        if let v = p.tpsLimit, v > 0 { f.tpsLimit = v }
        if let v = p.multiThreadStreams, v > 0 { f.multiThreadStreams = v }
        if let v = p.retries, v > 0 { f.retries = v }
        if let v = p.lowLevelRetries, v > 0 { f.lowLevelRetries = v }
        if let v = p.maxDelete, v > 0 { f.maxDelete = v }
        if let v = p.maxDepth, v > 0 { f.maxDepth = v }
        return f
    }

    static func fileTransferEvents(_ s: SyncStats) -> [FileTransferEvent] {
        s.fileTransfers.map {
            FileTransferEvent(name: $0.name, size: $0.size, bytes: $0.bytes,
                              progress: $0.progress, status: $0.status,
                              speed: $0.speed, error: $0.error)
        }
    }

    private func publishSyncProgress(taskID: String, profileID: String, action: String,
                                     state: String, stats s: SyncStats, errMsg: String) {
        var ev = SyncProgressEvent()
        ev.taskID = taskID
        ev.profileID = profileID
        ev.action = action
        ev.state = state
        ev.transferred = s.bytes
        ev.total = s.bytesTotal
        ev.bytesPerSec = s.speed
        ev.eta = s.eta
        ev.filesTransferred = Int(s.files)
        ev.totalFiles = Int(s.filesTotal)
        ev.errors = Int(s.errors)
        ev.currentFile = s.currentFile
        ev.stage = s.stage
        ev.stageDetail = s.stageDetail
        ev.checks = s.checks
        ev.totalChecks = s.checksTotal
        ev.deletes = s.deletes
        ev.renames = s.renames
        ev.transfers = Self.fileTransferEvents(s)
        ev.errorMessage = errMsg
        bus.publish(BusTopic.syncProgress, ev)
    }

    private func saveHistory(_ e: HistoryEntry) {
        guard let store else { return }
        do { try store.saveHistory(e) }
        catch { log.warn("sync: persist history failed", ("task", e.id)) }
    }
}

func sanitizeRcloneErr(_ s: String) -> String {
    let lower = s.lowercased()
    if lower.contains("signal: killed") || lower.contains("signal: interrupt")
        || lower.contains("context canceled") || lower.contains("context cancelled")
        || lower.contains("cancelled") {
        return ""
    }
    var out = s
    if let i = out.range(of: "(stderr:") {
        out = String(out[..<i.lowerBound]).trimmingCharacters(in: .whitespaces)
    }
    if out.trimmingCharacters(in: .whitespaces).hasPrefix("{") {
        return "sync failed"
    }
    return out
}

private func describe(_ e: Error) -> String { String(describing: e) }
