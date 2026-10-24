// stdio framing, request workers, event bridge and shutdown for protocol v1.
//
// One reader thread (the caller of `run()`), one serial writer queue, and a
// bounded pool of request workers. Never writes anything but protocol frames
// to stdout; diagnostics go through the Logger (stderr).
import Foundation
import Darwin

public enum RPCLineReadResult: Sendable {
    case line(Data)
    case tooLong
    case eof
}

public protocol RPCLineSource: AnyObject, Sendable {
    func nextLine(maxBytes: Int) -> RPCLineReadResult
}

public protocol RPCLineSink: AnyObject, Sendable {
    /// Returns false when the peer is gone (EPIPE).
    func writeLine(_ data: Data) -> Bool
}

// MARK: - stdio implementations

public final class StdioLineSource: RPCLineSource, @unchecked Sendable {
    private let fd: Int32
    private var buffer: [UInt8] = []
    private static let chunkSize = 64 * 1024

    public init(fd: Int32 = 0) {
        self.fd = fd
        buffer.reserveCapacity(Self.chunkSize)
    }

    public func nextLine(maxBytes: Int) -> RPCLineReadResult {
        var discarding = false
        var chunk = [UInt8](repeating: 0, count: Self.chunkSize)
        while true {
            if let idx = buffer.firstIndex(of: 0x0A) {
                let line = Array(buffer[..<idx])
                buffer.removeFirst(idx + 1)
                if discarding { return .tooLong }
                if line.count > maxBytes { return .tooLong }
                return .line(Data(line))
            }
            if buffer.count > maxBytes {
                // Oversized line without a terminator yet: drop and discard
                // until the next newline.
                buffer.removeAll(keepingCapacity: true)
                discarding = true
            }
            let n = read(fd, &chunk, chunk.count)
            if n < 0 {
                if errno == EINTR { continue }
                return .eof
            }
            if n == 0 {
                // A trailing partial line at EOF is discarded (rpc.md §11.3).
                return .eof
            }
            buffer.append(contentsOf: chunk[0..<n])
        }
    }
}

public final class StdioLineSink: RPCLineSink, @unchecked Sendable {
    private let fd: Int32
    private let lock = NSLock()

    public init(fd: Int32 = 1) {
        self.fd = fd
    }

    public func writeLine(_ data: Data) -> Bool {
        var line = data
        line.append(0x0A)
        lock.lock()
        defer { lock.unlock() }
        return line.withUnsafeBytes { (raw: UnsafeRawBufferPointer) -> Bool in
            guard let base = raw.baseAddress else { return true }
            var offset = 0
            while offset < raw.count {
                let n = write(fd, base + offset, raw.count - offset)
                if n < 0 {
                    if errno == EINTR { continue }
                    return false
                }
                offset += n
            }
            return true
        }
    }
}

// MARK: - Service

public final class RPCService: @unchecked Sendable {
    public struct Options {
        public var logLevel: LogLevel = .info
        public var logBufferCapacity = 2000
        public var exitOnShutdown = true
        public init() {}
    }

    public static let maxRequestBytes = 1 << 20

    private let container: AppContainer
    private let dispatcher: RPCDispatcher
    private let source: any RPCLineSource
    private let sink: any RPCLineSink
    private let logBuffer: RPCLogBuffer
    private let options: Options
    private let lockPath: String
    private let locker: InstanceLocker?

    private let outQueue = DispatchQueue(label: "gn-drive.rpc.out")
    private let outQueueKey = DispatchSpecificKey<UInt8>()
    private let scheduleQueue = DispatchQueue(label: "gn-drive.rpc.schedule")
    private let workerQueue = DispatchQueue(label: "gn-drive.rpc.workers",
                                            qos: .userInitiated, attributes: .concurrent)
    private let inFlight = DispatchGroup()
    private let workerSlots = DispatchSemaphore(value: 8)

    private let stateLock = NSLock()
    private var stopping = false
    private var didShutdown = false
    private var outputClosed = false
    private var pendingFrames = 0
    private var cancelBus: (() -> Void)?
    private var heartbeat: DispatchSourceTimer?

    private let progressLock = NSLock()
    private var pendingProgress: [String: [String: Any]] = [:]
    private var progressOrder: [String] = []
    private var progressScheduled = false

    private let runtimeLock = NSLock()
    private var runtimePending = false
    private var runtimeScheduled = false
    private var lastRuntimePush = Date.distantPast

    public init(container: AppContainer,
                source: any RPCLineSource = StdioLineSource(),
                sink: any RPCLineSink = StdioLineSink(),
                lockPath: String,
                locker: InstanceLocker? = nil,
                rcloneBinary: String = "",
                options: Options = Options()) {
        self.container = container
        self.source = source
        self.sink = sink
        self.options = options
        self.lockPath = lockPath
        self.locker = locker
        self.logBuffer = RPCLogBuffer(capacity: options.logBufferCapacity)
        self.dispatcher = RPCDispatcher(container: container, rcloneBinary: rcloneBinary,
                                        logBuffer: logBuffer)
        outQueue.setSpecific(key: outQueueKey, value: 1)
    }

    // MARK: - Lifecycle

    /// Subscribe, announce `hello`, then read until EOF or shutdown.
    @discardableResult
    public func run() -> Int32 {
        start()
        while !isStopping {
            switch source.nextLine(maxBytes: Self.maxRequestBytes) {
            case .eof:
                beginShutdown(reason: "eof")
                return 0
            case .tooLong:
                sendError(id: NSNull(), error: RPCError("invalid_request", "request exceeds 1 MiB"))
            case .line(let data):
                handleLine(data)
            }
        }
        return 0
    }

    /// Wire the bus, logger and OAuth bridge and write the first frame.
    public func start() {
        container.log.minLevel = options.logLevel
        logBuffer.onLine = { [weak self] record in self?.onLogRecord(record) }
        container.log.sink = { [weak self] record in self?.logBuffer.append(record) }

        dispatcher.pushEvent = { [weak self] name, data in
            self?.enqueueEvent(name, data, droppable: false)
        }
        cancelBus = container.bus.subscribeAll(topics: BusTopic.all) { [weak self] topic, ev in
            self?.onBusEvent(topic, ev)
        }

        let timer = DispatchSource.makeTimerSource(queue: scheduleQueue)
        timer.schedule(deadline: .now() + 2, repeating: 2)
        timer.setEventHandler { [weak self] in self?.heartbeatTick() }
        timer.resume()
        heartbeat = timer

        enqueueEvent("hello", dispatcher.helloData(lockPath: lockPath, pid: getpid()),
                     droppable: false)
    }

    /// Trigger the shutdown sequence (idempotent). Called on EOF, signals and
    /// `app.quit`, and on a broken stdout.
    public func beginShutdown(reason: String) {
        stateLock.lock()
        if didShutdown {
            stateLock.unlock()
            return
        }
        didShutdown = true
        stopping = true
        stateLock.unlock()

        _ = inFlight.wait(timeout: .now() + 5)
        heartbeat?.cancel()
        heartbeat = nil
        cancelBus?()
        cancelBus = nil
        container.log.sink = nil
        logBuffer.onLine = nil

        container.syncEngine.stop()
        container.close()
        locker?.release()

        if let frame = Self.encodeEventFrame("app:shutdown", ["reason": reason]) {
            enqueueFinal(frame)
        }
        flushOut()
        if options.exitOnShutdown { exit(0) }
    }

    private var isStopping: Bool {
        stateLock.lock(); defer { stateLock.unlock() }
        return stopping
    }

    // MARK: - Requests

    private func handleLine(_ data: Data) {
        guard !isStopping else { return }
        let obj: Any
        do {
            obj = try JSONSerialization.jsonObject(with: data, options: [.fragmentsAllowed])
        } catch {
            sendError(id: NSNull(), error: RPCError("invalid_request", "request is not valid JSON"))
            return
        }
        guard let dict = obj as? [String: Any] else {
            sendError(id: NSNull(), error: RPCError("invalid_request", "request must be a JSON object"))
            return
        }
        let rawID = dict["id"]
        guard let id = rawID, Self.isValidID(id) else {
            sendError(id: NSNull(), error: RPCError("invalid_request", "request id must be a number or string"))
            return
        }
        guard let method = dict["method"] as? String, !method.isEmpty else {
            sendError(id: id, error: RPCError("invalid_request", "missing required field: method"))
            return
        }
        let params: [String: Any]
        if let raw = dict["params"], !(raw is NSNull) {
            guard let p = raw as? [String: Any] else {
                sendError(id: id, error: RPCError.invalidParams("params", "params must be an object"))
                return
            }
            params = p
        } else {
            params = [:]
        }

        let request = RPCRequestBox(id: id, method: method, params: params)
        inFlight.enter()
        workerQueue.async { [weak self] in
            guard let self else { return }
            self.workerSlots.wait()
            defer {
                self.workerSlots.signal()
                self.inFlight.leave()
            }
            do {
                let result = try self.dispatcher.handle(method: request.method, params: request.params)
                self.sendResult(id: request.id, result: result)
            } catch let err as RPCError {
                self.sendError(id: request.id, error: err)
            } catch {
                self.sendError(id: request.id, error: rpcMapError(error))
            }
            if request.method == "app.quit" {
                DispatchQueue.global().async { self.beginShutdown(reason: "request") }
            }
        }
    }

    private final class RPCRequestBox: @unchecked Sendable {
        let id: Any
        let method: String
        let params: [String: Any]
        init(id: Any, method: String, params: [String: Any]) {
            self.id = id
            self.method = method
            self.params = params
        }
    }

    private static func isValidID(_ id: Any) -> Bool {
        if id is String { return true }
        if let n = id as? NSNumber { return CFGetTypeID(n) != CFBooleanGetTypeID() }
        return false
    }

    // MARK: - Event bridge

    private func onBusEvent(_ topic: String, _ ev: BusEvent) {
        switch topic {
        case BusTopic.syncProgress:
            guard let p = ev as? SyncProgressEvent else { return }
            enqueueProgress(p.profileID.isEmpty ? p.taskID : p.profileID, RPCJSON.syncProgress(p))
            requestRuntimeSnapshot()
        case BusTopic.syncStarted:
            guard let e = ev as? SyncStartedEvent else { return }
            enqueueEvent(topic, ["taskID": e.taskID, "profileID": e.profileID, "action": e.action],
                         droppable: false)
            requestRuntimeSnapshot()
        case BusTopic.syncCompleted:
            guard let e = ev as? SyncCompletedEvent else { return }
            enqueueEvent(topic, [
                "taskID": e.taskID, "profileID": e.profileID, "action": e.action,
                "duration": e.duration, "bytes": e.bytes, "errors": e.errors,
            ], droppable: false)
            requestRuntimeSnapshot()
        case BusTopic.syncFailed:
            guard let p = ev as? SyncProgressEvent else { return }
            enqueueEvent(topic, RPCJSON.syncProgress(p), droppable: false)
            requestRuntimeSnapshot()
        case BusTopic.authUnlocked, BusTopic.authLocked:
            enqueueEvent(topic, [:], droppable: false)
            requestRuntimeSnapshot(immediate: true)
        case BusTopic.flowExecution:
            guard let e = ev as? FlowExecutionEvent else { return }
            enqueueEvent(topic, ["flowID": e.flowID, "opID": e.opID,
                                 "status": e.status, "error": e.error], droppable: false)
            requestRuntimeSnapshot()
        case BusTopic.boardExecution:
            guard let e = ev as? BoardExecutionEvent else { return }
            enqueueEvent(topic, ["boardID": e.boardID, "nodeID": e.nodeID, "edgeID": e.edgeID,
                                 "status": e.status, "action": e.action], droppable: false)
        case BusTopic.scheduleTriggered:
            guard let e = ev as? ScheduleTriggeredEvent else { return }
            enqueueEvent(topic, ["scheduleID": e.scheduleID, "profileID": e.profileID,
                                 "action": e.action], droppable: false)
        case BusTopic.stateChanged:
            guard let e = ev as? StateChangedEvent else { return }
            enqueueEvent(topic, ["domain": e.domain, "id": e.id], droppable: false)
        case BusTopic.serviceStatus:
            guard let e = ev as? ServiceStatusEvent else { return }
            enqueueEvent(topic, ["running": e.running, "webPort": e.webPort,
                                 "uptimeSecs": e.uptimeSecs], droppable: false)
        default:
            return
        }
    }

    private func onLogRecord(_ record: LogRecord) {
        guard !isStopping else { return }
        enqueueEvent("log:line", [
            "time": RPCJSON.rfc3339(record.time),
            "level": record.level.label,
            "msg": record.msg,
            "fields": record.fields,
        ], droppable: true)
    }

    /// `sync:progress` is coalesced per profile at 10 Hz.
    private func enqueueProgress(_ key: String, _ data: [String: Any]) {
        progressLock.lock()
        if pendingProgress[key] == nil { progressOrder.append(key) }
        pendingProgress[key] = data
        let needsSchedule = !progressScheduled
        progressScheduled = true
        progressLock.unlock()
        guard needsSchedule else { return }
        scheduleQueue.asyncAfter(deadline: .now() + 0.1) { [weak self] in
            self?.flushProgress()
        }
    }

    private func flushProgress() {
        guard !isStopping else { return }
        progressLock.lock()
        let keys = progressOrder
        let batch = pendingProgress
        pendingProgress.removeAll(keepingCapacity: true)
        progressOrder.removeAll(keepingCapacity: true)
        progressScheduled = false
        progressLock.unlock()
        for key in keys {
            guard let data = batch[key] else { continue }
            enqueueEvent(BusTopic.syncProgress, data, droppable: false)
        }
    }

    private func requestRuntimeSnapshot(immediate: Bool = false) {
        runtimeLock.lock()
        runtimePending = true
        if runtimeScheduled {
            runtimeLock.unlock()
            return
        }
        runtimeScheduled = true
        let since = Date().timeIntervalSince(lastRuntimePush)
        let delay = immediate ? 0 : max(0, 0.1 - since)
        runtimeLock.unlock()
        scheduleQueue.asyncAfter(deadline: .now() + delay) { [weak self] in
            self?.flushRuntimeSnapshot()
        }
    }

    private func flushRuntimeSnapshot() {
        guard !isStopping else { return }
        runtimeLock.lock()
        guard runtimePending else {
            runtimeScheduled = false
            runtimeLock.unlock()
            return
        }
        runtimePending = false
        runtimeScheduled = false
        lastRuntimePush = Date()
        runtimeLock.unlock()
        let snapshot = container.runtime.snapshot()
        enqueueEvent("runtime:snapshot", RPCJSON.runtimeSnapshot(snapshot), droppable: false)
    }

    private func heartbeatTick() {
        guard !isStopping else { return }
        let snapshot = container.runtime.snapshot()
        let active = snapshot.flows.contains {
            $0.status == "running" || $0.status == "cancelling"
        }
        guard active else { return }
        enqueueEvent("runtime:snapshot", RPCJSON.runtimeSnapshot(snapshot), droppable: false)
    }

    // MARK: - Framing

    private func sendResult(id: Any, result: [String: Any]) {
        let frame: [String: Any] = ["id": id, "result": result]
        guard let data = try? JSONSerialization.data(withJSONObject: frame) else {
            sendError(id: id, error: RPCError("internal_error", "could not encode response"))
            return
        }
        enqueue(data, droppable: false)
    }

    private func sendError(id: Any, error: RPCError) {
        var payload: [String: Any] = ["code": error.code, "message": error.message]
        if let data = error.data { payload["data"] = data }
        let frame: [String: Any] = ["id": id, "error": payload]
        guard let data = try? JSONSerialization.data(withJSONObject: frame) else { return }
        enqueue(data, droppable: false)
    }

    private static func encodeEventFrame(_ name: String, _ data: [String: Any]) -> Data? {
        let frame: [String: Any] = ["event": name, "ts": RPCJSON.nowMillis(), "data": data]
        return try? JSONSerialization.data(withJSONObject: frame)
    }

    private func enqueueEvent(_ name: String, _ data: [String: Any], droppable: Bool) {
        guard let encoded = Self.encodeEventFrame(name, data) else { return }
        enqueue(encoded, droppable: droppable)
    }

    private func enqueue(_ data: Data, droppable: Bool) {
        stateLock.lock()
        defer { stateLock.unlock() }
        guard !outputClosed else { return }
        if droppable && pendingFrames > 512 { return }
        startWriteLocked(data)
    }

    /// Final frame: closes the output atomically with the enqueue so nothing
    /// can be written after `app:shutdown`.
    private func enqueueFinal(_ data: Data) {
        stateLock.lock()
        defer { stateLock.unlock() }
        outputClosed = true
        startWriteLocked(data)
    }

    /// Must be called with `stateLock` held; dispatching while locked keeps
    /// the writer queue in enqueue order.
    private func startWriteLocked(_ data: Data) {
        pendingFrames += 1
        outQueue.async { [weak self] in
            guard let self else { return }
            let ok = self.sink.writeLine(data)
            self.stateLock.lock()
            self.pendingFrames -= 1
            self.stateLock.unlock()
            if !ok {
                self.outQueue.async { self.beginShutdown(reason: "eof") }
            }
        }
    }

    private func flushOut() {
        if DispatchQueue.getSpecific(key: outQueueKey) == nil {
            outQueue.sync {}
        }
    }

    // MARK: - Startup helpers

    public static func emitFatal(code: String, message: String, pid: Int32? = nil,
                                 holder: String? = nil) {
        var data: [String: Any] = ["code": code, "message": message]
        if let pid { data["pid"] = Int(pid) }
        if let holder { data["holder"] = holder }
        let frame: [String: Any] = ["event": "fatal", "ts": RPCJSON.nowMillis(), "data": data]
        guard let encoded = try? JSONSerialization.data(withJSONObject: frame) else { return }
        _ = StdioLineSink().writeLine(encoded)
    }

    /// `/usr/bin/pgrep -x GNDriveApp` — the SwiftUI app does not take the lock
    /// on its own yet (rpc.md §2.2).
    public static func isAppRunning() -> Bool {
        let proc = Process()
        proc.executableURL = URL(fileURLWithPath: "/usr/bin/pgrep")
        proc.arguments = ["-x", "GNDriveApp"]
        proc.standardOutput = FileHandle.nullDevice
        proc.standardError = FileHandle.nullDevice
        guard (try? proc.run()) != nil else { return false }
        proc.waitUntilExit()
        return proc.terminationStatus == 0
    }

    /// Classify the lock holder for `fatal.data.holder`.
    public static func classifyHolder(pid: Int32) -> String {
        let proc = Process()
        proc.executableURL = URL(fileURLWithPath: "/bin/ps")
        proc.arguments = ["-p", "\(pid)", "-o", "command="]
        let pipe = Pipe()
        proc.standardOutput = pipe
        proc.standardError = FileHandle.nullDevice
        guard (try? proc.run()) != nil else { return "cli" }
        let data = pipe.fileHandleForReading.readDataToEndOfFile()
        proc.waitUntilExit()
        let command = String(data: data, encoding: .utf8) ?? ""
        if command.contains("--service") { return "service" }
        if command.contains("serve") { return "window" }
        return "cli"
    }
}
