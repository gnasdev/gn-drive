// In-process projection of live run state — port of internal/runtimehub.
// Listens to flow:execution and sync:* events, seeds from engine statuses and
// active tasks, serves runtime snapshots for UI hydration.
import Foundation

public final class RuntimeHub: @unchecked Sendable {
    private static let maxLog = 40

    private struct OpRuntime {
        var status = ""
        var lastError = ""
    }
    private struct FlowRuntime {
        var status = ""
        var lastError = ""
        var ops: [String: OpRuntime] = [:]
        var sync: SyncProgressEvent? = nil
        var log: [RuntimeLogEntry] = []
    }

    private let lock = NSLock()
    private var rev: Int64 = 0
    private var flows: [String: FlowRuntime] = [:]
    private weak var flowsSource: FlowEngine?
    private weak var tasksSource: SyncEngine?
    private var cancel: (() -> Void)?

    public init(bus: EventBus, flows: FlowEngine?, tasks: SyncEngine?) {
        self.flowsSource = flows
        self.tasksSource = tasks
        self.cancel = bus.subscribeAll(topics: [
            BusTopic.flowExecution, BusTopic.syncStarted, BusTopic.syncProgress,
            BusTopic.syncCompleted, BusTopic.syncFailed,
        ]) { [weak self] topic, ev in
            self?.onEvent(topic: topic, ev: ev)
        }
    }

    public func close() {
        cancel?()
        cancel = nil
    }

    /// Clear the projection (lock / data-plane teardown).
    public func reset() {
        lock.lock()
        flows.removeAll()
        rev += 1
        lock.unlock()
    }

    public func forget(_ flowID: String) {
        guard !flowID.isEmpty else { return }
        lock.lock()
        flows.removeValue(forKey: flowID)
        rev += 1
        lock.unlock()
    }

    /// Snapshot with engine statuses and active tasks overlaid.
    public func snapshot() -> RuntimeSnapshotEvent {
        lock.lock()
        let rev = self.rev
        let cloned = flows
        lock.unlock()

        var merged = cloned
        if let statuses = flowsSource?.statuses() {
            for (id, st) in statuses {
                var fr = merged[id] ?? FlowRuntime()
                if !st.isEmpty { fr.status = st }
                merged[id] = fr
            }
        }
        if let tasks = tasksSource?.activeTasks() {
            for t in tasks { applyTask(into: &merged, t) }
        }
        var out: [RuntimeFlowState] = []
        for (id, fr) in merged {
            out.append(toFlowState(id: id, fr: fr))
        }
        return RuntimeSnapshotEvent(revision: rev, flows: out)
    }

    private func onEvent(topic: String, ev: BusEvent) {
        lock.lock()
        defer { lock.unlock() }
        switch topic {
        case BusTopic.flowExecution:
            guard let fe = ev as? FlowExecutionEvent, !fe.flowID.isEmpty else { return }
            applyFlow(fe)
        case BusTopic.syncStarted, BusTopic.syncProgress, BusTopic.syncCompleted, BusTopic.syncFailed:
            applySync(topic: topic, ev: ev)
        default:
            return
        }
        rev += 1
    }

    private func applyFlow(_ fe: FlowExecutionEvent) {
        var fr = flows[fe.flowID] ?? FlowRuntime()
        if !fe.opID.isEmpty {
            var op = fr.ops[fe.opID] ?? OpRuntime()
            op.status = fe.status
            if !fe.error.isEmpty {
                op.lastError = fe.error
                if fe.status != "cancelled" && fe.status != "cancelling" {
                    fr.lastError = fe.error
                }
            }
            fr.ops[fe.opID] = op
            if ["running", "cancelling", "failed", "cancelled"].contains(fe.status) {
                fr.status = fe.status
            }
        } else {
            fr.status = fe.status
            if !fe.error.isEmpty && fe.status != "cancelled" && fe.status != "cancelling" {
                fr.lastError = fe.error
            }
            if fe.status == "cancelled" || fe.status == "cancelling" {
                fr.lastError = ""
            }
        }
        let label = fe.opID.isEmpty ? "Flow" : "Op " + String(fe.opID.prefix(8))
        fr.log = appendLog(fr.log, RuntimeLogEntry(
            at: Int64(Date().timeIntervalSince1970 * 1000),
            status: fe.status, opID: fe.opID, error: fe.error, label: label))
        flows[fe.flowID] = fr
    }

    private func applySync(topic: String, ev: BusEvent) {
        guard let prog = syncEvent(topic: topic, ev: ev) else { return }
        guard let (flowID, opID) = Self.splitBusyKey(prog.profileID) else { return }
        var fr = flows[flowID] ?? FlowRuntime()
        fr.sync = prog
        if !opID.isEmpty {
            var op = fr.ops[opID] ?? OpRuntime()
            if !prog.state.isEmpty {
                op.status = prog.state
            } else if topic == BusTopic.syncStarted {
                op.status = "running"
            }
            if !prog.errorMessage.isEmpty { op.lastError = prog.errorMessage }
            fr.ops[opID] = op
        }
        if topic == BusTopic.syncStarted {
            fr.status = "running"
        } else if prog.state == "running" {
            fr.status = "running"
        } else if ["failed", "cancelled", "completed"].contains(prog.state), fr.status.isEmpty {
            fr.status = prog.state
        }
        if !prog.errorMessage.isEmpty, prog.state != "cancelled" {
            fr.lastError = prog.errorMessage
        }
        flows[flowID] = fr
    }

    private func syncEvent(topic: String, ev: BusEvent) -> SyncProgressEvent? {
        switch topic {
        case BusTopic.syncStarted:
            guard let st = ev as? SyncStartedEvent else { return nil }
            var p = SyncProgressEvent()
            p.taskID = st.taskID; p.profileID = st.profileID; p.action = st.action
            p.state = "running"
            return p
        case BusTopic.syncProgress, BusTopic.syncFailed:
            return ev as? SyncProgressEvent
        case BusTopic.syncCompleted:
            guard let c = ev as? SyncCompletedEvent else { return nil }
            var p = SyncProgressEvent()
            p.taskID = c.taskID; p.profileID = c.profileID; p.action = c.action
            p.state = "completed"
            p.transferred = c.bytes
            p.errors = c.errors
            return p
        default:
            return nil
        }
    }

    private func applyTask(into dst: inout [String: FlowRuntime], _ t: SyncTask.Snapshot) {
        guard let (flowID, opID) = Self.splitBusyKey(t.name) else { return }
        var fr = dst[flowID] ?? FlowRuntime()
        let status = t.status.isEmpty ? "running" : t.status
        fr.status = status
        if !opID.isEmpty { fr.ops[opID] = OpRuntime(status: status) }
        var prog = SyncProgressEvent()
        let s = t.stats
        prog.taskID = t.id
        prog.profileID = t.name
        prog.action = t.action
        prog.state = status
        prog.transferred = s.bytes
        prog.total = s.bytesTotal
        prog.bytesPerSec = s.speed
        prog.eta = s.eta
        prog.errors = Int(s.errors)
        prog.currentFile = s.currentFile
        prog.stage = s.stage
        prog.stageDetail = s.stageDetail
        prog.filesTransferred = Int(s.files)
        prog.totalFiles = Int(s.filesTotal)
        prog.checks = s.checks
        prog.totalChecks = s.checksTotal
        prog.deletes = s.deletes
        prog.renames = s.renames
        prog.transfers = s.fileTransfers.map {
            FileTransferEvent(name: $0.name, size: $0.size, bytes: $0.bytes,
                              progress: $0.progress, status: $0.status,
                              speed: $0.speed, error: $0.error)
        }
        fr.sync = prog
        dst[flowID] = fr
    }

    private func toFlowState(id: String, fr: FlowRuntime) -> RuntimeFlowState {
        var st = RuntimeFlowState()
        st.id = id
        st.status = fr.status.isEmpty ? "idle" : fr.status
        st.lastError = fr.lastError
        st.ops = fr.ops.map { RuntimeOpState(id: $0.key, status: $0.value.status, lastError: $0.value.lastError) }
        st.sync = fr.sync
        st.log = fr.log
        return st
    }

    private func appendLog(_ prev: [RuntimeLogEntry], _ e: RuntimeLogEntry) -> [RuntimeLogEntry] {
        if let last = prev.last,
           last.status == e.status, last.opID == e.opID, last.error == e.error {
            return prev
        }
        var next = prev + [e]
        if next.count > Self.maxLog {
            next = Array(next.suffix(Self.maxLog))
        }
        return next
    }

    /// "flowID:opID" → (flowID, opID); busyKeys without a colon return false.
    public static func splitBusyKey(_ profileID: String) -> (String, String)? {
        guard let i = profileID.firstIndex(of: ":"),
              i != profileID.startIndex,
              profileID.index(after: i) != profileID.endIndex else { return nil }
        return (String(profileID[..<i]), String(profileID[profileID.index(after: i)...]))
    }
}
