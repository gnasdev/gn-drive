// Sequential flow execution — port of internal/flowengine.
import Foundation

public enum FlowEngineError: Error {
    case alreadyRunning
    case notRunning
    case emptyFlow
    case engineNotReady
}

public final class FlowEngine: @unchecked Sendable {
    private var store: Store?
    private var syncEngine: SyncEngine?
    private let bus: EventBus
    private let log: Logger

    private struct Run {
        var cancelled = false
        var status = "running"
    }
    private let lock = NSLock()
    private var runs: [String: Run] = [:]
    private var lastStatus: [String: String] = [:]

    public init(store: Store?, syncEngine: SyncEngine?, bus: EventBus, log: Logger) {
        self.store = store
        self.syncEngine = syncEngine
        self.bus = bus
        self.log = log
    }

    public func attach(store: Store, syncEngine: SyncEngine) {
        self.store = store
        self.syncEngine = syncEngine
    }

    public func detach() {
        store = nil
    }

    public func isRunning(_ flowID: String) -> Bool {
        lock.lock(); defer { lock.unlock() }
        return runs[flowID] != nil
    }

    /// Active run → running/cancelling; after finish → last terminal status;
    /// never started → "idle".
    public func status(_ flowID: String) -> String {
        lock.lock(); defer { lock.unlock() }
        if let r = runs[flowID] { return r.status }
        return lastStatus[flowID] ?? "idle"
    }

    public func statuses() -> [String: String] {
        lock.lock(); defer { lock.unlock() }
        var out = lastStatus.filter { !$0.value.isEmpty }
        for (id, r) in runs where !r.status.isEmpty { out[id] = r.status }
        return out
    }

    /// Begin sequential execution in the background.
    public func execute(flowID: String) throws {
        guard let store, let syncEngine else { throw FlowEngineError.engineNotReady }
        let f = try store.getFlow(flowID)
        if f.operations.isEmpty { throw FlowEngineError.emptyFlow }

        lock.lock()
        if runs[flowID] != nil {
            lock.unlock()
            throw FlowEngineError.alreadyRunning
        }
        runs[flowID] = Run()
        lastStatus.removeValue(forKey: flowID)
        lock.unlock()

        publish(flowID: flowID, opID: "", status: "running", msg: "")

        DispatchQueue.global().async { [weak self] in
            self?.run(f, syncEngine: syncEngine)
        }
    }

    public func stop(flowID: String) throws {
        lock.lock()
        guard var r = runs[flowID] else {
            lock.unlock()
            throw FlowEngineError.notRunning
        }
        r.cancelled = true
        r.status = "cancelling"
        runs[flowID] = r
        lock.unlock()
        publish(flowID: flowID, opID: "", status: "cancelling", msg: "")
    }

    private func isCancelled(_ flowID: String) -> Bool {
        lock.lock(); defer { lock.unlock() }
        return runs[flowID]?.cancelled ?? false
    }

    private func run(_ f: Flow, syncEngine: SyncEngine) {
        var final = "completed"
        defer {
            lock.lock()
            runs.removeValue(forKey: f.id)
            lastStatus[f.id] = final
            lock.unlock()
            publish(flowID: f.id, opID: "", status: final, msg: "")
            log.info("flow finished", ("flow", f.id), ("status", final))
        }

        for op in f.operations {
            if isCancelled(f.id) {
                final = "cancelled"
                return
            }
            // From=source, To=target always; pull reverses inside rclone.
            let action = op.resolvedAction()
            let from = composePath(remote: op.sourceRemote, path: op.sourcePath)
            let to = composePath(remote: op.targetRemote, path: op.targetPath)
            let opts = Self.profileFromSyncConfig(op.syncConfig.value)

            publish(flowID: f.id, opID: op.id, status: "running", msg: "")

            let busyKey = "\(f.id):\(op.id)"
            let taskID: String
            do {
                taskID = try syncEngine.startPathSync(action: action, busyKey: busyKey,
                                                      from: from, to: to, opts: opts)
            } catch {
                log.error("flow op start failed", ("flow", f.id), ("op", op.id),
                          ("err", String(describing: error)))
                publish(flowID: f.id, opID: op.id, status: "failed",
                        msg: String(describing: error))
                final = "failed"
                return
            }

            // Wait for the task; on flow cancel, stop the task.
            var taskErr: Error? = nil
            do {
                try syncEngine.waitTask(taskID) { [weak self] in
                    self?.isCancelled(f.id) ?? false
                }
            } catch {
                taskErr = error
            }

            if let err = taskErr {
                if isCancelled(f.id) || isTaskCancelled(err) {
                    try? syncEngine.stopSync(taskID)
                    publish(flowID: f.id, opID: op.id, status: "cancelled", msg: "")
                    final = "cancelled"
                    return
                }
                let msg = friendlyTaskErr(err)
                log.error("flow op failed", ("flow", f.id), ("op", op.id),
                          ("err", String(describing: err)))
                publish(flowID: f.id, opID: op.id, status: "failed", msg: msg)
                final = "failed"
                return
            }
            publish(flowID: f.id, opID: op.id, status: "completed", msg: "")
        }
    }

    private func isTaskCancelled(_ err: Error) -> Bool {
        if case SyncEngineError.taskCancelled = err { return true }
        return false
    }

    private func publish(flowID: String, opID: String, status: String, msg: String) {
        bus.publish(BusTopic.flowExecution,
                    FlowExecutionEvent(flowID: flowID, opID: opID, status: status, error: msg))
        // Also emit board:execution for older clients.
        bus.publish(BusTopic.boardExecution,
                    BoardExecutionEvent(boardID: flowID, nodeID: opID, edgeID: "", status: status, action: msg))
    }

    private func friendlyTaskErr(_ err: Error) -> String {
        var s = String(describing: err)
        for prefix in ["syncengine: task failed\n", "syncengine: task failed: ",
                       "taskFailed(", "taskFailed"] {
            s = s.replacingOccurrences(of: prefix, with: "")
        }
        s = s.trimmingCharacters(in: CharacterSet(charactersIn: "()\"' "))
        if let i = s.range(of: "(stderr:") {
            s = String(s[..<i.lowerBound]).trimmingCharacters(in: .whitespaces)
        }
        if s.count > 200 { s = String(s.prefix(200)) + "…" }
        return s
    }

    // MARK: - sync_config → Profile flags (port of syncconfig.go)

    /// Map a flow FlowOperation's sync_config bag (snake_case or camelCase keys)
    /// onto a Profile used as the flag carrier for startPathSync.
    public static func profileFromSyncConfig(_ m: [String: Any]) -> Profile {
        var p = Profile()
        p.parallel = 4

        func boolV(_ keys: String...) -> Bool? {
            for k in keys { if let v = m[k] as? Bool { return v } }
            return nil
        }
        func intV(_ keys: String...) -> Int? {
            for k in keys {
                if let v = m[k] as? Int { return v }
                if let v = m[k] as? Double { return Int(v) }
                if let s = m[k] as? String, let v = Int(s.trimmingCharacters(in: .whitespaces)) { return v }
            }
            return nil
        }
        func doubleV(_ keys: String...) -> Double? {
            for k in keys {
                if let v = m[k] as? Double { return v }
                if let v = m[k] as? Int { return Double(v) }
                if let s = m[k] as? String, let v = Double(s.trimmingCharacters(in: .whitespaces)) { return v }
            }
            return nil
        }
        func stringV(_ keys: String...) -> String? {
            for k in keys {
                if let v = m[k] as? String, !v.trimmingCharacters(in: .whitespaces).isEmpty {
                    return v.trimmingCharacters(in: .whitespaces)
                }
            }
            return nil
        }
        func stringsV(_ keys: String...) -> [String]? {
            for k in keys {
                if let arr = m[k] as? [String] {
                    return arr.compactMap { s in
                        let t = s.trimmingCharacters(in: .whitespaces)
                        return t.isEmpty ? nil : t
                    }
                }
                if let arr = m[k] as? [Any] {
                    return arr.compactMap { ($0 as? String)?.trimmingCharacters(in: .whitespaces) }
                        .filter { !$0.isEmpty }
                }
            }
            return nil
        }

        if let v = boolV("dry_run", "dryRun") { p.dryRun = v }
        if let v = intV("parallel"), v > 0 { p.parallel = v }
        if let v = intV("bandwidth"), v > 0 { p.bandwidth = v }
        if let v = intV("multi_thread_streams", "multiThreadStreams"), v > 0 { p.multiThreadStreams = v }
        if let v = stringV("buffer_size", "bufferSize") { p.bufferSize = v }
        if let v = intV("retries"), v > 0 { p.retries = v }
        if let v = intV("low_level_retries", "lowLevelRetries"), v > 0 { p.lowLevelRetries = v }
        if let v = stringV("max_duration", "maxDuration") { p.maxDuration = v }
        if let v = boolV("check_first", "checkFirst") { p.checkFirst = v }
        if let v = stringV("order_by", "orderBy") { p.orderBy = v }
        if let v = stringV("retries_sleep", "retriesSleep") { p.retriesSleep = v }
        if let v = doubleV("tps_limit", "tpsLimit"), v > 0 { p.tpsLimit = v }
        if let v = stringV("conn_timeout", "connTimeout") { p.connTimeout = v }
        if let v = stringV("io_timeout", "ioTimeout") { p.ioTimeout = v }

        if let v = stringsV("included_paths", "includedPaths") { p.includedPaths = v }
        if let v = stringsV("excluded_paths", "excludedPaths") { p.excludedPaths = v }
        if let v = stringV("min_size", "minSize") { p.minSize = v }
        if let v = stringV("max_size", "maxSize") { p.maxSize = v }
        if let v = stringV("max_age", "maxAge") { p.maxAge = v }
        if let v = stringV("min_age", "minAge") { p.minAge = v }
        if let v = intV("max_depth", "maxDepth"), v > 0 { p.maxDepth = v }
        if let v = stringV("filter_from_file", "filterFromFile") { p.filterFromFile = v }
        if let v = stringV("exclude_if_present", "excludeIfPresent") { p.excludeIfPresent = v }
        if let v = boolV("use_regex", "useRegex") { p.useRegex = v }
        if let v = boolV("delete_excluded", "deleteExcluded") { p.deleteExcluded = v }

        if let v = intV("max_delete", "maxDelete"), v > 0 { p.maxDelete = v }
        if let v = boolV("immutable") { p.immutable = v }
        if let v = stringV("max_transfer", "maxTransfer") { p.maxTransfer = v }
        if let v = stringV("max_delete_size", "maxDeleteSize") { p.maxDeleteSize = v }
        if let v = stringV("suffix") { p.suffix = v }
        if let v = boolV("suffix_keep_extension", "suffixKeepExtension") { p.suffixKeepExtension = v }
        if let v = stringV("backup_path", "backupPath") { p.backupPath = v }

        if let v = boolV("size_only", "sizeOnly") { p.sizeOnly = v }
        if let v = boolV("update_mode", "updateMode") { p.updateMode = v }
        if let v = boolV("ignore_existing", "ignoreExisting") { p.ignoreExisting = v }

        if let v = stringV("delete_timing", "deleteTiming") { p.deleteTiming = v }

        if let v = stringV("conflict_resolution", "conflictResolution") { p.conflictResolution = v }
        if let v = boolV("resilient") { p.resilient = v }
        if let v = stringV("max_lock", "maxLock") { p.maxLock = v }
        if let v = boolV("check_access", "checkAccess") { p.checkAccess = v }
        if let v = stringV("conflict_loser", "conflictLoser") { p.conflictLoser = v }
        if let v = stringV("conflict_suffix", "conflictSuffix") { p.conflictSuffix = v }

        return p
    }
}

extension FlowEngine: FlowExecutor {}
