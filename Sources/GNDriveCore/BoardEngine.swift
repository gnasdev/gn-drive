// Board DAG execution — port of internal/boardengine.
import Foundation

public enum BoardEngineError: Error {
    case notConfigured
    case emptyBoard
    case alreadyRunning
    case notRunning
    case cancelled
    case missingNode(edgeID: String)
    case failed(String)
    case completedWithErrors(String)
}

public struct BoardRunStatus: Sendable {
    public var runID: String = ""
    public var boardID: String = ""
    public var status: String = "" // running | completed | failed | cancelled
    public var error: String = ""
}

public final class BoardEngine: @unchecked Sendable {
    private var store: Store?
    private var sync: SyncClient?
    private let bus: EventBus
    private let log: Logger

    private struct Run {
        let id: String
        let token: CancellationToken
        var done = false
        var err: Error?
    }
    private let lock = NSLock()
    private var runs: [String: Run] = [:]

    public init(store: Store?, sync: SyncClient?, bus: EventBus, log: Logger) {
        self.store = store
        self.sync = sync
        self.bus = bus
        self.log = log
    }

    public func attach(store: Store, sync: SyncClient) {
        self.store = store
        self.sync = sync
    }

    public func detach() {
        store = nil
        sync = nil
    }

    public func status(_ boardID: String) -> BoardRunStatus? {
        lock.lock(); defer { lock.unlock() }
        guard let r = runs[boardID] else { return nil }
        var st = BoardRunStatus(runID: r.id, boardID: boardID)
        if r.done {
            if let err = r.err {
                if case .cancelled = (err as? BoardEngineError) ?? .notConfigured {
                    st.status = "cancelled"
                } else {
                    st.status = "failed"
                    st.error = String(describing: err)
                }
            } else {
                st.status = "completed"
            }
        } else {
            st.status = "running"
        }
        return st
    }

    /// Execute in the background; returns the run id.
    @discardableResult
    public func execute(boardID: String, stopOnError: Bool) throws -> String {
        guard let store, let sync else { throw BoardEngineError.notConfigured }
        let b = try store.loadBoardGraph(boardID)
        if b.nodes.isEmpty || b.edges.isEmpty { throw BoardEngineError.emptyBoard }

        lock.lock()
        if let existing = runs[boardID], !existing.done {
            lock.unlock()
            throw BoardEngineError.alreadyRunning
        }
        let token = CancellationToken()
        let run = Run(id: UUID().uuidString, token: token)
        runs[boardID] = run
        lock.unlock()

        publish(boardID: boardID, nodeID: "", edgeID: "", status: "running", action: "")

        DispatchQueue.global().async { [weak self] in
            guard let self else { return }
            var err: Error? = nil
            do {
                try self.runBoard(board: b, sync: sync, token: token, stopOnError: stopOnError, onProgress: nil)
            } catch { err = error }
            self.lock.lock()
            if var r = self.runs[boardID] { r.done = true; r.err = err; self.runs[boardID] = r }
            self.lock.unlock()
            if let err {
                var status = "failed"
                if let be = err as? BoardEngineError, case .cancelled = be { status = "cancelled" }
                self.publish(boardID: boardID, nodeID: "", edgeID: "", status: status,
                             action: String(describing: err))
                self.log.error("boardengine: finished with error", ("board", boardID),
                               ("err", String(describing: err)))
                return
            }
            self.publish(boardID: boardID, nodeID: "", edgeID: "", status: "completed", action: "")
            self.log.info("boardengine: completed", ("board", boardID), ("run", run.id))
        }
        return run.id
    }

    /// Synchronous execution with per-edge progress callback (used by CLI).
    public func executeSync(boardID: String, stopOnError: Bool,
                            onProgress: @escaping (_ layer: Int, _ idx: Int, _ total: Int,
                                         _ edge: BoardEdge, _ src: BoardNode,
                                         _ dst: BoardNode, _ err: Error?) -> Void) throws {
        guard let store, let sync else { throw BoardEngineError.notConfigured }
        let b = try store.loadBoardGraph(boardID)
        if b.nodes.isEmpty || b.edges.isEmpty { throw BoardEngineError.emptyBoard }
        try runBoard(board: b, sync: sync, token: CancellationToken(),
                     stopOnError: stopOnError, onProgress: onProgress)
    }

    public func stop(_ boardID: String) throws {
        lock.lock()
        guard let r = runs[boardID], !r.done else {
            lock.unlock()
            throw BoardEngineError.notRunning
        }
        let token = r.token
        lock.unlock()
        token.cancel()
    }

    private func runBoard(board b: Board, sync: SyncClient, token: CancellationToken,
                          stopOnError: Bool,
                          onProgress: ((_ layer: Int, _ idx: Int, _ total: Int,
                                        _ edge: BoardEdge, _ src: BoardNode,
                                        _ dst: BoardNode, _ err: Error?) -> Void)?) throws {
        var nodeByID: [String: BoardNode] = [:]
        for n in b.nodes { nodeByID[n.id] = n }
        let layers = try Self.topoLayers(nodes: b.nodes, edges: b.edges)

        var idx = 0
        let total = b.edges.count
        var firstErr: Error? = nil
        for (layerI, layer) in layers.enumerated() {
            for edge in layer {
                if token.isCancelled { throw BoardEngineError.cancelled }
                idx += 1
                guard let src = nodeByID[edge.sourceID], let dst = nodeByID[edge.targetID] else {
                    throw BoardEngineError.missingNode(edgeID: edge.id)
                }
                publish(boardID: b.id, nodeID: edge.sourceID, edgeID: edge.id,
                        status: "running", action: edge.action)
                var edgeErr: Error? = nil
                do { try Self.runEdge(sync: sync, edge: edge, src: src, dst: dst, token: token) }
                catch { edgeErr = error }
                onProgress?(layerI, idx, total, edge, src, dst, edgeErr)
                if let edgeErr {
                    publish(boardID: b.id, nodeID: edge.sourceID, edgeID: edge.id,
                            status: "failed", action: String(describing: edgeErr))
                    if stopOnError {
                        throw BoardEngineError.failed("board: stopped at edge \(edge.id): \(edgeErr)")
                    }
                    if firstErr == nil { firstErr = edgeErr }
                    continue
                }
                publish(boardID: b.id, nodeID: edge.sourceID, edgeID: edge.id,
                        status: "completed", action: edge.action)
            }
        }
        if let firstErr {
            throw BoardEngineError.completedWithErrors("board: completed with errors (first: \(firstErr))")
        }
    }

    private func publish(boardID: String, nodeID: String, edgeID: String, status: String, action: String) {
        bus.publish(BusTopic.boardExecution,
                    BoardExecutionEvent(boardID: boardID, nodeID: nodeID, edgeID: edgeID,
                                        status: status, action: action))
    }

    // MARK: - Edge + topo helpers

    public static func runEdge(sync: SyncClient, edge: BoardEdge, src: BoardNode,
                               dst: BoardNode, token: CancellationToken? = nil) throws {
        let source = nodePath(src)
        let dest = nodePath(dst)
        var cfg = SyncConfig()
        cfg.action = RcloneAction(rawValue: edge.action) ?? .push
        cfg.source = source
        cfg.dest = dest
        var flags = ProfileFlags()
        flags.transfers = 4
        cfg.profile = flags
        _ = try sync.sync(cfg, onProgress: nil, token: token)
    }

    static func nodePath(_ n: BoardNode) -> String {
        if !n.remoteName.isEmpty {
            return (n.path.isEmpty || n.path == "/") ? n.remoteName + ":" : n.remoteName + ":" + n.path
        }
        return n.path
    }

    /// Edges grouped by topological layer; error on cycle.
    public static func topoLayers(nodes: [BoardNode], edges: [BoardEdge]) throws -> [[BoardEdge]] {
        var indeg: [String: Int] = [:]
        for n in nodes { indeg[n.id] = 0 }
        for e in edges { indeg[e.targetID, default: 0] += 1 }

        var pending = [Bool](repeating: true, count: edges.count)
        var layers: [[BoardEdge]] = []
        var processed = 0
        while processed < edges.count {
            var layer: [BoardEdge] = []
            for (i, e) in edges.enumerated() where pending[i] {
                if (indeg[e.sourceID] ?? 0) == 0 { layer.append(e) }
            }
            if layer.isEmpty {
                throw BoardEngineError.failed("cycle detected: \(edges.count - processed) edges could not be ordered")
            }
            layer.sort { $0.id < $1.id }
            for e in layer {
                for (i, x) in edges.enumerated() where pending[i] && x.id == e.id {
                    pending[i] = false
                    break
                }
                indeg[e.targetID, default: 0] -= 1
            }
            layers.append(layer)
            processed += layer.count
        }
        return layers
    }
}
