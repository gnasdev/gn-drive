// Pure mapping between a Flow and its location-node / sync-edge canvas graph.
// Port of frontend/src/lib/flowGraph.ts — node ids persist in canvas_json;
// operations keep source/target paths so flowengine runs without the canvas.
import Foundation
import GNDriveCore

let defaultColGap: Double = 280
let defaultRowGap: Double = 140

struct CanvasLocation: Equatable {
    var id: String
    var remote: String
    var path: String
    var label: String
    var x: Double
    var y: Double
}

struct CanvasViewport: Equatable {
    var x: Double = 0
    var y: Double = 0
    var zoom: Double = 1
}

struct FlowCanvasModel: Equatable {
    var viewport = CanvasViewport()
    var nodes: [CanvasLocation] = []
}

struct GraphNode: Equatable, Identifiable {
    var id: String
    var remote: String
    var path: String
    var label: String
    var x: Double
    var y: Double
    var key: String { locationKey(remote: remote, path: path) }
}

struct GraphEdge: Identifiable {
    var id: String
    var source: String
    var target: String
    var action: String
    var operation: FlowOperation
}

struct FlowGraph {
    var nodes: [GraphNode]
    var edges: [GraphEdge]
    var viewport: CanvasViewport
}

func locationKey(remote: String, path: String) -> String {
    remote.trimmingCharacters(in: .whitespaces) + "\0" + normalizePath(path)
}

func normalizePath(_ path: String?) -> String {
    let p = (path ?? "").trimmingCharacters(in: .whitespaces)
    return p.isEmpty ? "/" : p
}

/// FNV-1a 32-bit — stable node id like the TS stableNodeId.
func stableNodeId(_ key: String) -> String {
    var h: UInt32 = 2166136261
    for u in key.utf8 {
        h ^= UInt32(u)
        h = h &* 16777619
    }
    return "n_" + String(h, radix: 16)
}

func parseCanvas(_ raw: String) -> FlowCanvasModel {
    var out = FlowCanvasModel()
    guard let data = raw.data(using: .utf8),
          let obj = try? JSONSerialization.jsonObject(with: data) as? [String: Any]
    else { return out }
    if let vp = obj["viewport"] as? [String: Any] {
        out.viewport = CanvasViewport(
            x: (vp["x"] as? NSNumber)?.doubleValue ?? 0,
            y: (vp["y"] as? NSNumber)?.doubleValue ?? 0,
            zoom: (vp["zoom"] as? NSNumber).map { $0.doubleValue } ?? 1)
    }
    let nodesIn = obj["nodes"] as? [[String: Any]] ?? []
    out.nodes = nodesIn.enumerated().map { i, n in
        CanvasLocation(
            id: (n["id"] as? String) ?? "n_\(i)",
            remote: (n["remote"] as? String) ?? "",
            path: normalizePath(n["path"] as? String ?? "/"),
            label: (n["label"] as? String) ?? "",
            x: (n["x"] as? NSNumber)?.doubleValue ?? 0,
            y: (n["y"] as? NSNumber)?.doubleValue ?? Double(i) * defaultRowGap)
    }
    return out
}

func serializeCanvas(_ c: FlowCanvasModel) -> String {
    let nodes: [[String: Any]] = c.nodes.map {
        ["id": $0.id, "remote": $0.remote, "path": $0.path, "label": $0.label,
         "x": $0.x, "y": $0.y]
    }
    let obj: [String: Any] = [
        "viewport": ["x": c.viewport.x, "y": c.viewport.y, "zoom": c.viewport.zoom],
        "nodes": nodes,
    ]
    guard let d = try? JSONSerialization.data(withJSONObject: obj) else { return "{}" }
    return String(data: d, encoding: .utf8) ?? "{}"
}

func defaultLayout(ops: [FlowOperation]) -> FlowCanvasModel {
    var nodes: [CanvasLocation] = []
    var seen = Set<String>()
    func place(_ remote: String, _ path: String, _ x: Double, _ y: Double) {
        let key = locationKey(remote: remote, path: path)
        if seen.contains(key) { return }
        seen.insert(key)
        nodes.append(CanvasLocation(
            id: stableNodeId(key),
            remote: remote.trimmingCharacters(in: .whitespaces),
            path: normalizePath(path), label: "", x: x, y: y))
    }
    for (i, op) in ops.enumerated() {
        place(op.sourceRemote, op.sourcePath, 0, Double(i) * defaultRowGap)
        place(op.targetRemote, op.targetPath, defaultColGap, Double(i) * defaultRowGap)
    }
    return FlowCanvasModel(nodes: nodes)
}

/// Build a graph: unique (remote, path) → node; each operation → edge.
func toGraph(_ flow: Flow) -> FlowGraph {
    let ops = flow.operations
    let stored = parseCanvas(flow.canvasJSON)
    var byKey: [String: GraphNode] = [:]
    var byId: [String: GraphNode] = [:]

    for n in stored.nodes {
        let node = GraphNode(id: n.id, remote: n.remote, path: normalizePath(n.path),
                             label: n.label, x: n.x, y: n.y)
        byKey[node.key] = node
        byId[node.id] = node
    }

    var needed = Set<String>()
    for op in ops {
        needed.insert(locationKey(remote: op.sourceRemote, path: op.sourcePath))
        needed.insert(locationKey(remote: op.targetRemote, path: op.targetPath))
    }

    if stored.nodes.isEmpty, !ops.isEmpty {
        for n in defaultLayout(ops: ops).nodes {
            let node = GraphNode(id: n.id, remote: n.remote, path: n.path, label: n.label, x: n.x, y: n.y)
            byKey[node.key] = node
            byId[node.id] = node
        }
    } else {
        var extra = 0.0
        for key in needed where byKey[key] == nil {
            let parts = key.split(separator: "\0", maxSplits: 1).map(String.init)
            let remote = parts.first ?? ""
            let path = parts.count > 1 ? parts[1] : "/"
            let node = GraphNode(id: stableNodeId(key), remote: remote, path: path,
                                 label: "", x: defaultColGap, y: extra * defaultRowGap)
            extra += 1
            byKey[key] = node
            byId[node.id] = node
        }
    }

    var edges: [GraphEdge] = []
    for op in ops {
        guard let src = byKey[locationKey(remote: op.sourceRemote, path: op.sourcePath)],
              let dst = byKey[locationKey(remote: op.targetRemote, path: op.targetPath)]
        else { continue }
        edges.append(GraphEdge(id: op.id, source: src.id, target: dst.id,
                               action: FlowAction.normalize(op.action), operation: op))
    }
    return FlowGraph(nodes: Array(byKey.values), edges: edges, viewport: stored.viewport)
}

enum ConnectError: String {
    case missingEndpoint = "missing-endpoint"
    case selfLoop = "self-loop"
    case unknownNode = "unknown-node"
    case duplicateEdge = "duplicate-edge"
}

func connectError(sourceID: String, targetID: String, action: String,
                  graph: FlowGraph) -> ConnectError? {
    if sourceID.isEmpty || targetID.isEmpty { return .missingEndpoint }
    if sourceID == targetID { return .selfLoop }
    guard graph.nodes.contains(where: { $0.id == sourceID }),
          graph.nodes.contains(where: { $0.id == targetID }) else { return .unknownNode }
    let act = FlowAction.normalize(action)
    if graph.edges.contains(where: { $0.source == sourceID && $0.target == targetID && FlowAction.normalize($0.action) == act }) {
        return .duplicateEdge
    }
    return nil
}

/// Project the graph back onto operations + canvas_json.
func fromGraph(_ graph: FlowGraph, previousOps: [FlowOperation]) -> (ops: [FlowOperation], canvasJSON: String) {
    let nodesById = Dictionary(uniqueKeysWithValues: graph.nodes.map { ($0.id, $0) })
    let prevById = Dictionary(uniqueKeysWithValues: previousOps.map { ($0.id, $0) })
    var ops: [FlowOperation] = []

    let validEdges = graph.edges.filter { nodesById[$0.source] != nil && nodesById[$0.target] != nil }
    for (i, e) in validEdges.enumerated() {
        let src = nodesById[e.source]!
        let dst = nodesById[e.target]!
        var op = prevById[e.id] ?? e.operation
        let action = FlowAction.normalize(e.action.isEmpty ? op.action : e.action)
        var sc = op.syncConfig.value
        sc["action"] = action
        op.id = e.id
        op.sourceRemote = src.remote
        op.sourcePath = normalizePath(src.path)
        op.targetRemote = dst.remote
        op.targetPath = normalizePath(dst.path)
        op.action = action
        op.syncConfig = JSONDict(sc)
        op.sortOrder = i
        ops.append(op)
    }

    let canvas = FlowCanvasModel(
        viewport: graph.viewport,
        nodes: graph.nodes.map { CanvasLocation(id: $0.id, remote: $0.remote, path: normalizePath($0.path), label: $0.label, x: $0.x, y: $0.y) })
    return (ops, serializeCanvas(canvas))
}
