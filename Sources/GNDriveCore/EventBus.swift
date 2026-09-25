// In-process typed event bus — port of internal/eventbus.
//
// Each subscription gets its own serial queue and a bounded mailbox; when the
// mailbox is full the oldest queued event is dropped to make room for the
// newest (same semantics as the Go implementation).
import Foundation

public enum BusTopic {
    public static let syncStarted = "sync:started"
    public static let syncProgress = "sync:progress"
    public static let syncCompleted = "sync:completed"
    public static let syncFailed = "sync:failed"
    public static let authUnlocked = "auth:unlocked"
    public static let authLocked = "auth:locked"
    public static let serviceStatus = "service:status"
    public static let stateChanged = "state:changed"
    public static let scheduleTriggered = "schedule:triggered"
    public static let boardExecution = "board:execution"
    public static let flowExecution = "flow:execution"
    public static let runtimeSnapshot = "runtime:snapshot"

    public static var all: [String] {
        [syncStarted, syncProgress, syncCompleted, syncFailed, authUnlocked,
         authLocked, serviceStatus, stateChanged, scheduleTriggered,
         boardExecution, flowExecution]
    }
}

/// Base protocol for bus events.
public protocol BusEvent: Sendable {
    var eventType: String { get }
    var eventTimestamp: Date { get }
}

public struct FileTransferEvent: Sendable, Codable {
    public var name: String = ""
    public var size: Int64 = 0
    public var bytes: Int64 = 0
    public var progress: Double = 0
    public var status: String = "" // transferring | completed | failed | checking | checked | pending
    public var speed: Double = 0
    public var error: String = ""
    public init(name: String, size: Int64, bytes: Int64, progress: Double, status: String, speed: Double, error: String) {
        self.name = name
        self.size = size
        self.bytes = bytes
        self.progress = progress
        self.status = status
        self.speed = speed
        self.error = error
    }

}

public struct SyncProgressEvent: BusEvent {
    public var eventType: String = BusTopic.syncProgress
    public var eventTimestamp: Date = Date()
    public var taskID: String = ""
    public var profileID: String = ""
    public var action: String = ""
    public var state: String = "running"
    public var transferred: Int64 = 0
    public var total: Int64 = 0
    public var bytesPerSec: Double = 0
    public var eta: Int64 = 0
    public var errors: Int = 0
    public var currentFile: String = ""
    public var stage: String = ""
    public var stageDetail: String = ""
    public var filesTransferred: Int = 0
    public var totalFiles: Int = 0
    public var checks: Int64 = 0
    public var totalChecks: Int64 = 0
    public var deletes: Int64 = 0
    public var renames: Int64 = 0
    public var transfers: [FileTransferEvent] = []
    public var errorMessage: String = ""
}

public struct SyncStartedEvent: BusEvent {
    public var eventType: String = BusTopic.syncStarted
    public var eventTimestamp: Date = Date()
    public var taskID: String = ""
    public var profileID: String = ""
    public var action: String = ""
    public init(taskID: String, profileID: String, action: String) {
        self.taskID = taskID
        self.profileID = profileID
        self.action = action
    }

}

public struct SyncCompletedEvent: BusEvent {
    public var eventType: String = BusTopic.syncCompleted
    public var eventTimestamp: Date = Date()
    public var taskID: String = ""
    public var profileID: String = ""
    public var action: String = ""
    public var duration: Int64 = 0
    public var bytes: Int64 = 0
    public var errors: Int = 0
    public init(taskID: String, profileID: String, action: String, duration: Int64, bytes: Int64, errors: Int) {
        self.taskID = taskID
        self.profileID = profileID
        self.action = action
        self.duration = duration
        self.bytes = bytes
        self.errors = errors
    }

}

public struct AuthUnlockedEvent: BusEvent {
    public var eventType: String = BusTopic.authUnlocked
    public var eventTimestamp: Date = Date()
    public init() {}
}

public struct AuthLockedEvent: BusEvent {
    public var eventType: String = BusTopic.authLocked
    public var eventTimestamp: Date = Date()
    public init() {}
}

public struct ServiceStatusEvent: BusEvent {
    public var eventType: String = BusTopic.serviceStatus
    public var eventTimestamp: Date = Date()
    public var running: Bool = false
    public var webPort: Int = 0
    public var uptimeSecs: Int = 0
}

public struct ScheduleTriggeredEvent: BusEvent {
    public var eventType: String = BusTopic.scheduleTriggered
    public var eventTimestamp: Date = Date()
    public var scheduleID: String = ""
    public var profileID: String = ""
    public var action: String = ""
    public init(scheduleID: String, profileID: String, action: String) {
        self.scheduleID = scheduleID
        self.profileID = profileID
        self.action = action
    }

}

public struct BoardExecutionEvent: BusEvent {
    public var eventType: String = BusTopic.boardExecution
    public var eventTimestamp: Date = Date()
    public var boardID: String = ""
    public var nodeID: String = ""
    public var edgeID: String = ""
    public var status: String = ""
    public var profileID: String = ""
    public var action: String = ""
    public init(boardID: String, nodeID: String, edgeID: String, status: String, action: String) {
        self.boardID = boardID
        self.nodeID = nodeID
        self.edgeID = edgeID
        self.status = status
        self.action = action
    }

}

public struct FlowExecutionEvent: BusEvent {
    public var eventType: String = BusTopic.flowExecution
    public var eventTimestamp: Date = Date()
    public var flowID: String = ""
    public var opID: String = ""
    public var status: String = ""
    public var error: String = ""
    public init(flowID: String, opID: String, status: String, error: String) {
        self.flowID = flowID
        self.opID = opID
        self.status = status
        self.error = error
    }

}

public struct StateChangedEvent: BusEvent {
    public var eventType: String = BusTopic.stateChanged
    public var eventTimestamp: Date = Date()
    public var domain: String = ""
    public var id: String = ""

    public init(domain: String, id: String) {
        self.domain = domain
        self.id = id
    }
}

// --- Runtime snapshot ------------------------------------------------------

public struct RuntimeLogEntry: Sendable, Codable {
    public var at: Int64 = 0
    public var status: String = ""
    public var opID: String = ""
    public var error: String = ""
    public var label: String = ""
    public init(at: Int64, status: String, opID: String, error: String, label: String) {
        self.at = at
        self.status = status
        self.opID = opID
        self.error = error
        self.label = label
    }

}

public struct RuntimeOpState: Sendable, Codable {
    public var id: String = ""
    public var status: String = ""
    public var lastError: String = ""
    public init(id: String, status: String, lastError: String) {
        self.id = id
        self.status = status
        self.lastError = lastError
    }

}

public struct RuntimeFlowState: Sendable {
    public var id: String = ""
    public var status: String = ""
    public var lastError: String = ""
    public var ops: [RuntimeOpState] = []
    public var sync: SyncProgressEvent?
    public var log: [RuntimeLogEntry] = []
}

public struct RuntimeSnapshotEvent: BusEvent {
    public var eventType: String = BusTopic.runtimeSnapshot
    public var eventTimestamp: Date = Date()
    public var revision: Int64 = 0
    public var flows: [RuntimeFlowState] = []
    public init(revision: Int64, flows: [RuntimeFlowState]) {
        self.revision = revision
        self.flows = flows
    }

}

// --- Bus -------------------------------------------------------------------

private final class Subscription {
    let id = UUID()
    let queue = DispatchQueue(label: "gn-drive.eventbus.sub")
    let lock = NSLock()
    var mailbox: [(String, BusEvent)] = []
    let capacity = 64
    let handler: (String, BusEvent) -> Void
    var stopped = false

    init(handler: @escaping (String, BusEvent) -> Void) {
        self.handler = handler
    }

    func enqueue(_ topic: String, _ ev: BusEvent) {
        var item: (String, BusEvent)? = nil
        lock.lock()
        if !stopped {
            if mailbox.count >= capacity {
                mailbox.removeFirst() // drop oldest
            }
            mailbox.append((topic, ev))
        }
        lock.unlock()
        queue.async { [weak self] in
            guard let self else { return }
            self.lock.lock()
            if self.stopped || self.mailbox.isEmpty {
                self.lock.unlock()
                return
            }
            let next = self.mailbox.removeFirst()
            self.lock.unlock()
            self.handler(next.0, next.1)
        }
        _ = item
    }

    func stop() {
        lock.lock()
        stopped = true
        mailbox.removeAll()
        lock.unlock()
    }
}

public final class EventBus: @unchecked Sendable {
    private let lock = NSLock()
    private var topics: [String: [Subscription]] = [:]
    private var closed = false

    public init() {}

    /// Subscribe to one topic. Handler runs serially per subscription.
    /// Returns a cancel closure.
    @discardableResult
    public func subscribe(topic: String, handler: @escaping (BusEvent) -> Void) -> () -> Void {
        subscribeAll(topics: [topic]) { _, ev in handler(ev) }
    }

    /// Subscribe to several topics at once; handler receives (topic, event).
    @discardableResult
    public func subscribeAll(topics: [String], handler: @escaping (String, BusEvent) -> Void) -> () -> Void {
        let sub = Subscription(handler: handler)
        lock.lock()
        if closed {
            lock.unlock()
            return {}
        }
        for t in topics {
            self.topics[t, default: []].append(sub)
        }
        lock.unlock()
        return { [weak self, weak sub] in
            guard let self, let sub else { return }
            self.lock.lock()
            for t in topics {
                if var list = self.topics[t], let idx = list.firstIndex(where: { $0.id == sub.id }) {
                    list.remove(at: idx)
                    self.topics[t] = list
                }
            }
            self.lock.unlock()
            sub.stop()
        }
    }

    /// Broadcast to every subscriber of topic. Non-blocking; drop-oldest on
    /// full subscriber mailbox.
    public func publish(_ topic: String, _ event: BusEvent) {
        lock.lock()
        let subs = closed ? [] : (topics[topic] ?? [])
        lock.unlock()
        for s in subs { s.enqueue(topic, event) }
    }

    public func close() {
        lock.lock()
        if closed { lock.unlock(); return }
        closed = true
        let all = topics.values.flatMap { $0 }
        topics.removeAll()
        lock.unlock()
        for s in all { s.stop() }
    }
}
