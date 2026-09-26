// Inspector — port of CanvasInspector.vue + OperationSettingsPanel.vue.
// Semantics: toolbar (run/stop/add-node/save+dirty), flow name + cron +
// enabled, node editing (label/remote/path + remotes), edge editing
// (source/target + full sync options). Edits stage locally (dirty flag);
// "Save" persists — matching the web canvas.dirty flow.
import SwiftUI
import GNDriveCore

struct InspectorView: View {
    @EnvironmentObject var state: AppState

    private var flow: Flow? {
        state.flows.first { $0.id == state.selectedFlowID }
    }

    private var running: Bool {
        guard let f = flow else { return false }
        return state.isFlowActive(f.id)
    }

    private var dirty: Bool {
        guard let f = flow else { return false }
        return state.dirtyFlowIDs.contains(f.id)
    }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 12) {
                if let f = flow {
                    toolbar(f)
                    flowFields(f)
                    switch state.selection ?? .flow {
                    case .node(let id): nodeInspector(id, f)
                    case .edge(let id): edgeInspector(id, f)
                    case .flow: flowStatus(f)
                    }
                } else {
                    Text(t("workspace.canvas.pickFlow")).foregroundStyle(.secondary).padding()
                }
            }
            .padding(12)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
        .background(Color(nsColor: .controlBackgroundColor))
    }

    // MARK: - Toolbar (Run/Stop, Add node, Save)

    @ViewBuilder
    private func toolbar(_ f: Flow) -> some View {
        HStack(spacing: 6) {
            if running {
                Button { state.stopFlow(f.id) } label: {
                    Label(t("workspace.stop"), systemImage: "stop.fill")
                }
                .tint(.red)
            } else {
                Button { runFlow(f) } label: {
                    Label(t("workspace.run"), systemImage: "play.fill")
                }
                .disabled(f.operations.isEmpty)
            }
            Button { addNode() } label: {
                Label(t("workspace.canvas.addNode"), systemImage: "plus")
            }
            .disabled(running)
            Button { saveFlow(f) } label: {
                Label(t("workspace.saveFlow"), systemImage: dirty ? "exclamationmark.circle" : "checkmark")
            }
            .disabled(running || !dirty)
            .help(dirty ? "Unsaved changes" : "Saved")
        }
        .controlSize(.small)
        if dirty {
            Text(t("workspace.discardEditTitle")).font(.caption2).foregroundStyle(.orange)
        }
    }

    private func runFlow(_ f: Flow) {
        if dirty { state.saveFlow(f) } // persist before running (web: canvas.persist)
        if f.operations.isEmpty {
            state.toast("Flow has no operations", isError: true)
            return
        }
        state.executeFlow(f.id)
        state.toast("Flow started: \(f.name.isEmpty ? "Untitled" : f.name)")
    }

    private func saveFlow(_ f: Flow) {
        state.saveFlow(f)
        state.toast("Flow saved: \(f.name.isEmpty ? "Untitled" : f.name)")
    }

    private func addNode() {
        guard var f = flow, !running else { return }
        var g = toGraph(f)
        let n = GraphNode(id: "n_\(UUID().uuidString.prefix(8))",
                          remote: "", path: "/new-\(Int(Date().timeIntervalSince1970))",
                          label: "", x: defaultColGap, y: Double(g.nodes.count) * defaultRowGap)
        g.nodes.append(n)
        let (ops, canvas) = fromGraph(g, previousOps: f.operations)
        f.operations = ops; f.canvasJSON = canvas
        state.stageFlow(f)
        state.selection = .node(n.id)
    }

    // MARK: - Flow fields

    @ViewBuilder
    private func flowFields(_ f: Flow) -> some View {
        TextField(t("workspace.flowName"), text: .init(
            get: { f.name },
            set: { var x = f; x.name = $0; state.stageFlow(x) }))
        .textFieldStyle(.roundedBorder)
        .disabled(running)

        HStack {
            CronField(cron: .init(
                get: { f.scheduleCron },
                set: { var x = f; x.scheduleCron = $0; state.stageFlow(x) }))
            .disabled(running)
            Toggle(t("common.enabled"), isOn: .init(
                get: { f.scheduleEnabled },
                set: { var x = f; x.scheduleEnabled = $0; state.stageFlow(x) }))
            .disabled(running)
            .font(.caption)
        }
    }

    @ViewBuilder
    private func flowStatus(_ f: Flow) -> some View {
        let status = state.flowStatus(f.id)
        if !status.isEmpty && status != "idle" {
            HStack {
                Circle().fill(statusColor(status)).frame(width: 7, height: 7)
                Text(status).font(.caption).foregroundStyle(.secondary)
            }
        }
        if let rf = state.runtimeFlow(f.id), !rf.lastError.isEmpty {
            Text(humanizeError(rf.lastError, status: rf.status))
                .font(.caption).foregroundStyle(.red)
        }
        let log = state.runtimeFlow(f.id)?.log ?? []
        if !log.isEmpty {
            Divider()
            Text(t("workspace.runLog")).font(.subheadline).fontWeight(.medium)
            ForEach(Array(log.suffix(12).enumerated()), id: \.offset) { _, e in
                HStack(spacing: 4) {
                    Text(e.label).font(.caption2).fontWeight(.medium)
                    Text(e.status).font(.caption2).foregroundStyle(statusColor(e.status))
                    if !e.error.isEmpty {
                        Text(humanizeError(e.error, status: e.status))
                            .font(.caption2).foregroundStyle(.red).lineLimit(1)
                    }
                }
            }
        }
    }

    // MARK: - Node inspector

    @ViewBuilder
    private func nodeInspector(_ id: String, _ f: Flow) -> some View {
        let g = toGraph(f)
        if let node = g.nodes.first(where: { $0.id == id }) {
            Text(t("workspace.canvas.nodeSettings")).font(.headline)
            TextField(t("workspace.canvas.label"), text: .init(
                get: { node.label },
                set: { v in updateNode(id, f, g) { $0.label = v } }))
            .textFieldStyle(.roundedBorder)
            .disabled(running)

            Picker("Remote", selection: .init(
                get: { node.remote },
                set: { v in updateNode(id, f, g) { $0.remote = v } })) {
                Text("Local").tag("")
                ForEach(state.remotes, id: \.name) { r in Text(r.name).tag(r.name) }
            }
            .disabled(running)

            RemotePathEditor(remote: node.remote, path: node.path,
                             disabled: running,
                             onChange: { v in updateNode(id, f, g) { $0.path = v } })

            Button(t("workspace.canvas.deleteNode"), role: .destructive) { removeNode(id, f, g) }
                .disabled(running)
        }
    }

    // MARK: - Edge inspector

    @ViewBuilder
    private func edgeInspector(_ id: String, _ f: Flow) -> some View {
        let g = toGraph(f)
        if let e = g.edges.first(where: { $0.id == id }) {
            edgeBody(e, f, g)
        }
    }

    @ViewBuilder
    private func edgeBody(_ e: GraphEdge, _ f: Flow, _ g: FlowGraph) -> some View {
        let id = e.id
        let op = e.operation

        Text(t("workspace.canvas.edgeSettings")).font(.headline)

        // Source / target editable via RemotePathField equivalents.
        if let src = g.nodes.first(where: { $0.id == e.source }) {
            LabeledContent(t("workspace.source")) {
                RemotePathEditor(remote: src.remote, path: src.path, disabled: running,
                                 onChange: { v in updateNode(src.id, f, g) { $0.path = v } })
            }
        }
        if let dst = g.nodes.first(where: { $0.id == e.target }) {
            LabeledContent(t("workspace.target")) {
                RemotePathEditor(remote: dst.remote, path: dst.path, disabled: running,
                                 onChange: { v in updateNode(dst.id, f, g) { $0.path = v } })
            }
        }

        if let st = state.runtimeFlow(f.id)?.ops.first(where: { $0.id == id }) {
            Text("Status: \(st.status)").font(.caption)
            if !st.lastError.isEmpty {
                Text(humanizeError(st.lastError, status: st.status))
                    .font(.caption).foregroundStyle(.red)
            }
        }

        OperationOptionsPanel(config: .init(
            get: { SyncConfigModel.parse(op.syncConfig.value, fallbackAction: op.resolvedAction()) },
            set: { cfg in
                updateEdge(id, f, g) {
                    $0.operation.syncConfig = JSONDict(cfg.serialize())
                    // Keep op.action in sync with cfg.action (web: update:action).
                    $0.action = cfg.action
                }
            }),
            disabled: running)

        Button(t("common.delete"), role: .destructive) { removeEdge(id, f, g) }
            .disabled(running)
    }

    // MARK: - mutations (stage only; Save persists)

    private func updateNode(_ id: String, _ f: Flow, _ g: FlowGraph, _ mutate: (inout GraphNode) -> Void) {
        var g = g
        guard let i = g.nodes.firstIndex(where: { $0.id == id }) else { return }
        mutate(&g.nodes[i])
        var flow = f
        let (ops, canvas) = fromGraph(g, previousOps: f.operations)
        flow.operations = ops; flow.canvasJSON = canvas
        state.stageFlow(flow)
    }

    private func removeNode(_ id: String, _ f: Flow, _ g: FlowGraph) {
        var g = g
        g.nodes.removeAll { $0.id == id }
        g.edges.removeAll { $0.source == id || $0.target == id }
        var flow = f
        let (ops, canvas) = fromGraph(g, previousOps: f.operations)
        flow.operations = ops; flow.canvasJSON = canvas
        state.saveFlow(flow)
        state.selection = .flow
    }

    private func updateEdge(_ id: String, _ f: Flow, _ g: FlowGraph, _ mutate: (inout GraphEdge) -> Void) {
        var g = g
        guard let i = g.edges.firstIndex(where: { $0.id == id }) else { return }
        mutate(&g.edges[i])
        var flow = f
        let (ops, canvas) = fromGraph(g, previousOps: f.operations)
        flow.operations = ops; flow.canvasJSON = canvas
        state.stageFlow(flow)
    }

    private func updateEdgeConfig(_ id: String, _ f: Flow, _ g: FlowGraph, _ cfg: [String: Any]) {
        updateEdge(id, f, g) { $0.operation.syncConfig = JSONDict(cfg) }
    }

    private func removeEdge(_ id: String, _ f: Flow, _ g: FlowGraph) {
        var g = g
        g.edges.removeAll { $0.id == id }
        var flow = f
        let (ops, canvas) = fromGraph(g, previousOps: f.operations)
        flow.operations = ops; flow.canvasJSON = canvas
        state.saveFlow(flow)
        state.selection = .flow
    }
}

// MARK: - Cron field with presets (port of CronField.vue + CRON_PRESETS)

struct CronField: View {
    @Binding var cron: String

    private static let presets: [(value: String, label: String)] = [
        ("0 * * * *", "Every hour"),
        ("0 */6 * * *", "Every 6 hours"),
        ("0 0 * * *", "Daily at midnight"),
        ("0 9 * * 1-5", "Weekdays 9:00"),
        ("0 0 * * 0", "Weekly Sunday"),
    ]

    var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            HStack(spacing: 4) {
                TextField("cron", text: $cron)
                    .textFieldStyle(.roundedBorder)
                    .font(.caption.monospaced())
                Menu("…") {
                    ForEach(Self.presets, id: \.value) { p in
                        Button(p.label) { cron = p.value }
                    }
                }
                .menuStyle(.borderlessButton)
                .fixedSize()
            }
            Text("min hour dom month dow (or 6-field with seconds, @hourly, @daily…)")
                .font(.caption2).foregroundStyle(.tertiary)
        }
    }
}
