// Interactive flow canvas — port of FlowCanvas.vue + SyncEdge.vue semantics.
//
// AGENTS.md telemetry rules implemented here:
//  - Runtime routing key is "flowID:opID" (RuntimeHub.splitBusyKey).
//  - Pending file rows show inside the edge card but never render as edge dots.
//  - Edge dots only for live/terminal states; active rows show %.
//  - A selected edge with an active op auto-opens its file card; the card
//    survives pan/zoom and is dismissed only by outside click or user action.
//
// Dot placement follows lib/fileParticles.ts: queued/live files ride in
// [0.18, 0.7] of the path, failed parks at 0.5, completed at 0.98; bidir
// actions mirror the direction.
import SwiftUI
import GNDriveCore

struct FlowCanvasView: View {
    @EnvironmentObject var state: AppState

    @State private var graph = FlowGraph(nodes: [], edges: [], viewport: CanvasViewport())
    @State private var nodePos: [String: CGPoint] = [:]
    @State private var panOffset = CGSize.zero
    @State private var panCommit = CGSize.zero
    @State private var zoom: Double = 1
    @State private var zoomCommit: Double = 1
    @State private var dragStart: [String: CGPoint] = [:]
    @State private var connectFrom: String? = nil
    @State private var connectPoint: CGPoint? = nil
    @State private var cardOpenFor: String? = nil
    @State private var fitted = false
    @State private var pendingFit = false

    private let nodeSize = CGSize(width: 170, height: 56)

    // fileParticles.ts queue band
    private let queueStart: CGFloat = 0.18
    private let queueEnd: CGFloat = 0.7
    private let targetT: CGFloat = 0.98
    private let errorT: CGFloat = 0.5

    var flow: Flow? {
        state.flows.first { $0.id == state.selectedFlowID }
    }

    /// isActiveRun: running/cancelling → canvas locked (no edits).
    private var running: Bool {
        guard let f = flow else { return false }
        return state.isFlowActive(f.id)
    }

    var body: some View {
        GeometryReader { geo in
            ZStack {
                // Dot grid background
                Canvas { ctx, size in
                    let spacing = 24.0 * zoom
                    guard spacing > 8 else { return }
                    var path = Path()
                    var x = panOffset.width.truncatingRemainder(dividingBy: spacing)
                    while x < size.width {
                        var y = panOffset.height.truncatingRemainder(dividingBy: spacing)
                        while y < size.height {
                            path.addEllipse(in: CGRect(x: x, y: y, width: 1.5, height: 1.5))
                            y += spacing
                        }
                        x += spacing
                    }
                    ctx.fill(path, with: .color(.secondary.opacity(0.25)))
                }

                // Edges
                Canvas { ctx, _ in
                    let nodeIDs = Set(graph.nodes.map(\.id))
                    for e in graph.edges {
                        guard nodeIDs.contains(e.source), nodeIDs.contains(e.target),
                              let s = nodePos[e.source], let t = nodePos[e.target] else { continue }
                        let p1 = center(of: s)
                        let p2 = center(of: t)
                        let path = edgePath(p1, p2)
                        let st = opStatus(e)
                        let selected = state.selection == .edge(e.id)
                        let col: Color = selected ? .accentColor : edgeColor(st)
                        ctx.stroke(path, with: .color(col),
                                   lineWidth: selected ? 2.5 : 1.6)

                        // Edge label: action + file badge + speed.
                        let mid = pathPoint(path: path, t: 0.5)
                        let label = edgeLabel(e, status: st)
                        if !label.isEmpty {
                            ctx.draw(Text(label).font(.caption2).foregroundStyle(col),
                                     at: CGPoint(x: mid.x, y: mid.y - 8), anchor: .center)
                        }

                        // Live file dots — live/terminal states only (pending
                        // stays in the edge card per AGENTS.md).
                        for dot in liveDots(e) {
                            let t = desiredT(dot, action: e.action, running: st == "running")
                            let pos = pathPoint(path: path, t: t)
                            ctx.fill(
                                Circle().path(in: CGRect(x: pos.x - 3.5, y: pos.y - 3.5, width: 7, height: 7)),
                                with: .color(dotColor(dot.status)))
                        }
                    }
                    // Pending connect line
                    if let from = connectFrom, let s = nodePos[from], let cp = connectPoint {
                        ctx.stroke(edgePath(center(of: s), cp),
                                   with: .color(.accentColor.opacity(0.6)),
                                   style: StrokeStyle(lineWidth: 1.6, dash: [5, 4]))
                    }
                }

                // Nodes (nodePos stores top-left in canvas coords)
                ForEach(graph.nodes) { n in
                    if let pos = nodePos[n.id] {
                        let c = center(of: pos)
                        NodeView(node: n,
                                 selected: state.selection == .node(n.id),
                                 connecting: connectFrom == n.id)
                            .scaleEffect(zoom)
                            .position(c)
                            .gesture(nodeDrag(n))
                            .onTapGesture { select(.node(n.id)) }
                        // Connect handle on the node's right edge (locked while running)
                        if !running {
                            Circle()
                                .fill(Color.accentColor.opacity(0.15))
                                .overlay(Circle().stroke(Color.accentColor, lineWidth: 1))
                                .frame(width: 14, height: 14)
                                .position(x: c.x + nodeSize.width * zoom / 2, y: c.y)
                                .gesture(connectDrag(from: n.id))
                        }
                    }
                }

                // Edge card overlay — anchored in canvas coords, survives pan/zoom.
                if let edgeID = cardOpenFor,
                   let e = graph.edges.first(where: { $0.id == edgeID }),
                   let s = nodePos[e.source], let t = nodePos[e.target] {
                    let mid = pathPoint(path: edgePath(center(of: s), center(of: t)), t: 0.5)
                    EdgeCardView(edge: e, status: opStatus(e),
                                 onClose: { cardOpenFor = nil })
                        .position(x: min(max(mid.x, 200), geo.size.width - 200),
                                  y: mid.y - 140 > 60 ? mid.y - 140 : mid.y + 140)
                        .zIndex(10)
                }

                if flow == nil {
                    Text("Select or create a flow")
                        .foregroundStyle(.secondary)
                }
            }
            .contentShape(Rectangle())
            .onTapGesture { loc in
                if let edgeID = hitEdge(at: loc) {
                    select(.edge(edgeID))
                } else {
                    select(.flow)
                    cardOpenFor = nil
                }
            }
            .gesture(dragPan())
            .gesture(magnify())
            .focusable()
            .onKeyPress(.delete) { deleteSelected(); return .handled }
            .onKeyPress(.deleteForward) { deleteSelected(); return .handled }
            .task {
                // Apply the deferred fit once geometry is known.
                try? await Task.sleep(nanoseconds: 50_000_000)
                if pendingFit { fitView(in: geo.size) }
            }
        }
        .clipped()
        .onAppear { rebuild() }
        .onChange(of: state.flows.map { $0.id + $0.updatedAt + $0.canvasJSON }) { rebuild() }
        .onChange(of: state.selectedFlowID) { onFlowSwitch() }
        .onChange(of: state.runtime.revision) { refreshRuntimeBits() }
    }

    /// Scale + center all nodes into view (vue-flow fitView equivalent).
    private func fitView(in size: CGSize) {
        pendingFit = false
        guard !graph.nodes.isEmpty else { return }
        let xs = graph.nodes.map { $0.x }
        let ys = graph.nodes.map { $0.y }
        guard let minX = xs.min(), let maxX = xs.max(),
              let minY = ys.min(), let maxY = ys.max() else { return }
        let contentW = maxX - minX + nodeSize.width + 80
        let contentH = maxY - minY + nodeSize.height + 80
        let z = min(min(size.width / contentW, size.height / contentH), 1.2)
        zoom = max(0.4, min(z, 2.0))
        zoomCommit = zoom
        panOffset = CGSize(
            width: size.width / 2 - (minX + nodeSize.width / 2) * zoom - 120 * zoom,
            height: size.height / 2 - (minY + nodeSize.height / 2) * zoom - 80 * zoom)
        panCommit = panOffset
        persistViewport()
    }

    // MARK: - Graph build / runtime

    private func rebuild() {
        guard let f = flow else {
            graph = FlowGraph(nodes: [], edges: [], viewport: CanvasViewport())
            nodePos = [:]
            return
        }
        let g = toGraph(f)
        graph = g
        var pos: [String: CGPoint] = [:]
        for n in g.nodes {
            pos[n.id] = nodePos[n.id] ?? CGPoint(x: 120 + n.x, y: 80 + n.y)
        }
        nodePos = pos
        // Restore persisted viewport (once per flow); default → fit content
        // like vue-flow's fit-view-on-init.
        if !fitted {
            fitted = true
            if g.viewport == CanvasViewport() && !g.nodes.isEmpty {
                pendingFit = true
            } else {
                zoom = g.viewport.zoom
                zoomCommit = zoom
                panOffset = CGSize(width: g.viewport.x, height: g.viewport.y)
                panCommit = panOffset
            }
        }
        refreshRuntimeBits()
    }

    private func onFlowSwitch() {
        fitted = false
        rebuild()
        // Auto-select the edge when the flow has exactly one (canvas.selectFlow).
        if graph.edges.count == 1, let e = graph.edges.first {
            state.selection = .edge(e.id)
            cardOpenFor = e.id
        } else {
            state.selection = .flow
            cardOpenFor = nil
        }
    }

    private func refreshRuntimeBits() {
        guard let f = flow, let rf = state.runtimeFlow(f.id) else { return }
        // Auto-open the card for a running op on the selected edge.
        if let active = rf.ops.first(where: { $0.status == "running" || $0.status == "checking" }) {
            if state.selection == .edge(active.id) || state.selection == nil || cardOpenFor == nil {
                cardOpenFor = active.id
                if state.selection == nil { state.selection = .edge(active.id) }
            }
        }
    }

    private func runtimeOp(_ opID: String) -> RuntimeOpState? {
        guard let f = flow else { return nil }
        return state.runtimeFlow(f.id)?.ops.first { $0.id == opID }
    }

    private func syncFor(_ opID: String) -> SyncProgressEvent? {
        guard let f = flow else { return nil }
        let s = state.runtimeFlow(f.id)?.sync
        guard let s, RuntimeHub.splitBusyKey(s.profileID)?.1 == opID else { return nil }
        return s
    }

    private func opStatus(_ e: GraphEdge) -> String {
        runtimeOp(e.id)?.status ?? ""
    }

    /// Edge-visible dots: live/terminal states only — pending rows never
    /// become dots (isEdgeVisibleStatus in fileParticles.ts).
    private func liveDots(_ e: GraphEdge) -> [FileTransferEvent] {
        guard let s = syncFor(e.id) else { return [] }
        let visible = ["transferring", "checking", "failed", "completed", "checked"]
        let rank = ["transferring": 0, "failed": 1, "checking": 3, "checked": 4, "completed": 5]
        return s.transfers
            .filter { visible.contains($0.status) }
            .sorted { (rank[$0.status] ?? 9) < (rank[$1.status] ?? 9) }
            .prefix(24).map { $0 }
    }

    /// Port of desiredT(): queued/live files ride [0.18, 0.7]; failed parks
    /// at 0.5; success reaches 0.98; bidir actions reverse.
    private func desiredT(_ f: FileTransferEvent, action: String, running: Bool) -> CGFloat {
        let bi = action == "bi" || action == "bi-resync"
        let forward = !bi // bi uses dir both ways; keep source→target
        var local: CGFloat
        switch f.status {
        case "failed": local = errorT
        case "completed", "checked": local = targetT
        case "transferring", "checking":
            let p = min(max(CGFloat(f.progress) / 100, 0), 1)
            local = queueStart + p * (queueEnd - queueStart)
        default:
            local = queueStart
        }
        return forward ? local : 1 - local
    }

    private func dotColor(_ status: String) -> Color {
        switch status {
        case "transferring": return .accentColor
        case "failed": return .red
        case "completed": return .green
        case "checking", "checked": return .teal
        default: return .secondary
        }
    }

    private func edgeColor(_ status: String) -> Color {
        switch status {
        case "running": return .accentColor
        case "completed": return .green
        case "failed": return .red
        case "cancelled", "cancelling": return .orange
        default: return .secondary.opacity(0.6)
        }
    }

    private func edgeLabel(_ e: GraphEdge, status: String) -> String {
        var parts = [e.action]
        if let s = syncFor(e.id) {
            if s.totalFiles > 0 {
                parts.append("\(min(max(s.filesTransferred, 0), s.totalFiles))/\(s.totalFiles)")
            }
            if s.transferred > 0 {
                parts.append("\(Int(s.bytesPerSec / 1024))K/s")
            }
        }
        return parts.joined(separator: " · ")
    }

    // MARK: - Geometry

    private func center(of p: CGPoint) -> CGPoint {
        CGPoint(x: p.x * zoom + panOffset.width + nodeSize.width / 2 * zoom,
                y: p.y * zoom + panOffset.height + nodeSize.height / 2 * zoom)
    }

    private func edgePath(_ a: CGPoint, _ b: CGPoint) -> Path {
        var p = Path()
        let dx = max(abs(b.x - a.x) * 0.5, 30)
        p.move(to: a)
        p.addCurve(to: b, control1: CGPoint(x: a.x + dx, y: a.y),
                   control2: CGPoint(x: b.x - dx, y: b.y))
        return p
    }

    private func pathPoint(path: Path, t: CGFloat) -> CGPoint {
        var pt = CGPoint.zero
        let trimmed = path.trimmedPath(from: 0, to: t)
        trimmed.forEach { el in
            switch el {
            case .move(to: let p): pt = p
            case .line(to: let p): pt = p
            case .curve(to: let p, _, _): pt = p
            case .quadCurve(to: let p, _): pt = p
            case .closeSubpath: break
            }
        }
        return pt
    }

    // MARK: - Gestures / selection

    private func select(_ sel: AppState.Selection) {
        state.selection = sel
        if case .edge(let id) = sel {
            cardOpenFor = id // selected edge opens its card (AGENTS rule)
        }
    }

    private func nodeDrag(_ n: GraphNode) -> some Gesture {
        DragGesture()
            .onChanged { v in
                if running { return }
                if dragStart[n.id] == nil { dragStart[n.id] = nodePos[n.id] }
                let start = dragStart[n.id] ?? .zero
                nodePos[n.id] = CGPoint(x: start.x + v.translation.width / zoom,
                                        y: start.y + v.translation.height / zoom)
            }
            .onEnded { _ in
                guard !running else { return }
                dragStart[n.id] = nil
                persistLayout()
            }
    }

    private func connectDrag(from nodeID: String) -> some Gesture {
        DragGesture()
            .onChanged { v in
                connectFrom = nodeID
                connectPoint = v.location
            }
            .onEnded { v in
                defer { connectFrom = nil; connectPoint = nil }
                for n in graph.nodes {
                    guard n.id != nodeID, let pos = nodePos[n.id] else { continue }
                    let c = center(of: pos)
                    let rect = CGRect(x: c.x - nodeSize.width * zoom / 2,
                                      y: c.y - nodeSize.height * zoom / 2,
                                      width: nodeSize.width * zoom,
                                      height: nodeSize.height * zoom)
                    if rect.contains(v.location) {
                        addOperation(from: nodeID, to: n.id)
                        return
                    }
                }
            }
    }

    private func dragPan() -> some Gesture {
        DragGesture()
            .onChanged { v in
                panOffset = CGSize(width: panCommit.width + v.translation.width,
                                   height: panCommit.height + v.translation.height)
            }
            .onEnded { _ in
                panCommit = panOffset
                persistViewport()
            }
    }

    private func magnify() -> some Gesture {
        MagnificationGesture()
            .onChanged { v in zoom = min(max(zoomCommit * v, 0.4), 2.0) }
            .onEnded { _ in
                zoomCommit = zoom
                persistViewport()
            }
    }

    // MARK: - Persistence

    /// Node drag end persists positions immediately (web: updatePositions).
    private func persistLayout() {
        guard var f = flow else { return }
        var g = graph
        for i in g.nodes.indices {
            if let p = nodePos[g.nodes[i].id] {
                g.nodes[i].x = p.x - 120
                g.nodes[i].y = p.y - 80
            }
        }
        g.viewport = CanvasViewport(x: panOffset.width, y: panOffset.height, zoom: zoom)
        let (ops, canvas) = fromGraph(g, previousOps: f.operations)
        f.operations = ops
        f.canvasJSON = canvas
        state.saveFlow(f)
    }

    /// Pan/zoom persists into canvas_json.viewport (web: updateViewport).
    private func persistViewport() {
        guard var f = flow else { return }
        var c = parseCanvas(f.canvasJSON)
        c.viewport = CanvasViewport(x: panOffset.width, y: panOffset.height, zoom: zoom)
        f.canvasJSON = serializeCanvas(c)
        state.saveFlow(f)
    }

    private func addOperation(from sourceID: String, to targetID: String) {
        guard var f = flow, !running else { return }
        if let err = connectError(sourceID: sourceID, targetID: targetID,
                                  action: "push", graph: graph) {
            state.toast(connectErrorLabel(err), isError: true)
            return
        }
        var g = graph
        var op = FlowOperation()
        op.id = UUID().uuidString
        op.action = "push"
        op.syncConfig = JSONDict(["action": "push"])
        if let src = g.nodes.first(where: { $0.id == sourceID }),
           let dst = g.nodes.first(where: { $0.id == targetID }) {
            op.sourceRemote = src.remote
            op.sourcePath = src.path
            op.targetRemote = dst.remote
            op.targetPath = dst.path
        }
        g.edges.append(GraphEdge(id: op.id, source: sourceID, target: targetID,
                                 action: "push", operation: op))
        let (ops, canvas) = fromGraph(g, previousOps: f.operations)
        f.operations = ops
        f.canvasJSON = canvas
        state.saveFlow(f) // connect persists immediately (web: canvas.connect)
        state.selection = .edge(op.id)
    }

    private func connectErrorLabel(_ e: ConnectError) -> String {
        switch e {
        case .duplicateEdge: return "This connection already exists"
        case .selfLoop: return "Cannot connect a node to itself"
        default: return "Cannot connect"
        }
    }

    /// Delete key removes the selected node/edge (locked while running).
    private func deleteSelected() {
        guard var f = flow, !running else { return }
        var g = graph
        switch state.selection {
        case .node(let id):
            g.nodes.removeAll { $0.id == id }
            g.edges.removeAll { $0.source == id || $0.target == id }
        case .edge(let id):
            g.edges.removeAll { $0.id == id }
        default:
            return
        }
        let (ops, canvas) = fromGraph(g, previousOps: f.operations)
        f.operations = ops
        f.canvasJSON = canvas
        state.saveFlow(f)
        state.selection = .flow
        cardOpenFor = nil
    }

    private func hitEdge(at loc: CGPoint) -> String? {
        for e in graph.edges {
            guard let s = nodePos[e.source], let t = nodePos[e.target] else { continue }
            let path = edgePath(center(of: s), center(of: t))
            for i in stride(from: 0.0, through: 1.0, by: 0.05) {
                let pt = pathPoint(path: path, t: CGFloat(i))
                if hypot(pt.x - loc.x, pt.y - loc.y) < 10 { return e.id }
            }
        }
        return nil
    }
}

// MARK: - Node view

struct NodeView: View {
    let node: GraphNode
    var selected: Bool = false
    var connecting: Bool = false

    var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            HStack(spacing: 5) {
                Image(systemName: node.remote.isEmpty || node.remote == "local"
                      ? "folder" : "cloud.fill")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                Text(node.label.isEmpty ? (node.remote.isEmpty ? "Local" : node.remote) : node.label)
                    .font(.callout).fontWeight(.medium)
                    .lineLimit(1)
            }
            Text(node.path)
                .font(.caption2).foregroundStyle(.secondary)
                .lineLimit(1)
        }
        .frame(width: 170, height: 56, alignment: .leading)
        .padding(.horizontal, 10)
        .background(.background, in: RoundedRectangle(cornerRadius: 8))
        .overlay(RoundedRectangle(cornerRadius: 8)
            .stroke(selected || connecting ? Color.accentColor : Color.secondary.opacity(0.35),
                    lineWidth: selected ? 2 : 1))
        .shadow(color: .black.opacity(0.08), radius: 3)
        .frame(width: 170, height: 56)
    }
}
