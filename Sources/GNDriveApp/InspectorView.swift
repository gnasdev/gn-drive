// Inspector — port of CanvasInspector + OperationSettingsPanel essentials.
import SwiftUI
import GNDriveCore

struct InspectorView: View {
    @EnvironmentObject var state: AppState

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 16) {
                switch state.selection ?? .flow {
                case .node(let id): nodeInspector(id)
                case .edge(let id): edgeInspector(id)
                case .flow: flowInspector()
                }
            }
            .padding(12)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
        .background(Color(nsColor: .controlBackgroundColor))
    }

    // MARK: - Flow inspector

    @ViewBuilder
    private func flowInspector() -> some View {
        if let f = state.flows.first(where: { $0.id == state.selectedFlowID }) {
            let status = state.flowStatus(f.id)
            Text("Flow").font(.headline)
            TextField("Name", text: .init(
                get: { f.name },
                set: { v in var x = f; x.name = v; state.saveFlow(x) }
            ))
            .textFieldStyle(.roundedBorder)

            HStack(spacing: 8) {
                if state.isFlowActive(f.id) {
                    Button("Stop") { state.stopFlow(f.id) }
                        .tint(.red)
                } else {
                    Button("Run") { state.executeFlow(f.id) }
                }
                Text(status).font(.caption).foregroundStyle(.secondary)
            }

            if let rf = state.runtimeFlow(f.id), !rf.lastError.isEmpty {
                Text(rf.lastError).font(.caption).foregroundStyle(.red)
            }

            Divider()
            Text("Schedule").font(.subheadline).fontWeight(.medium)
            Toggle("Enabled", isOn: .init(
                get: { f.scheduleEnabled },
                set: { v in var x = f; x.scheduleEnabled = v; state.saveFlow(x) }
            ))
            TextField("cron (e.g. 0 * * * *)", text: .init(
                get: { f.scheduleCron },
                set: { v in var x = f; x.scheduleCron = v; state.saveFlow(x) }
            ))
            .textFieldStyle(.roundedBorder)
            .font(.caption.monospaced())

            Divider()
            Text("Run log").font(.subheadline).fontWeight(.medium)
            let log = state.runtimeFlow(f.id)?.log ?? []
            if log.isEmpty {
                Text("—").font(.caption).foregroundStyle(.secondary)
            } else {
                ForEach(Array(log.suffix(12).enumerated()), id: \.offset) { _, e in
                    HStack(spacing: 4) {
                        Text(e.label).font(.caption2).fontWeight(.medium)
                        Text(e.status).font(.caption2).foregroundStyle(statusColor(e.status))
                        if !e.error.isEmpty {
                            Text(e.error).font(.caption2).foregroundStyle(.red).lineLimit(1)
                        }
                    }
                }
            }
        } else {
            Text("No flow selected").foregroundStyle(.secondary)
        }
    }

    // MARK: - Node inspector

    @ViewBuilder
    private func nodeInspector(_ id: String) -> some View {
        if let f = state.flows.first(where: { $0.id == state.selectedFlowID }) {
            let g = toGraph(f)
            if let node = g.nodes.first(where: { $0.id == id }) {
                Text("Location").font(.headline)
                TextField("Label", text: .init(
                    get: { node.label },
                    set: { v in updateNode(id, f, g) { $0.label = v } }
                )).textFieldStyle(.roundedBorder)

                Picker("Remote", selection: .init(
                    get: { node.remote },
                    set: { v in updateNode(id, f, g) { $0.remote = v } }
                )) {
                    Text("Local").tag("")
                    ForEach(state.remotes, id: \.name) { r in
                        Text(r.name).tag(r.name)
                    }
                }

                RemotePathEditor(
                    remote: node.remote,
                    path: node.path,
                    onChange: { v in updateNode(id, f, g) { $0.path = v } })

                Button("Delete node", role: .destructive) {
                    removeNode(id, f, g)
                }
            } else {
                Text("Unknown node").foregroundStyle(.secondary)
            }
        }
    }

    private func updateNode(_ id: String, _ f: Flow, _ g: FlowGraph, _ mutate: (inout GraphNode) -> Void) {
        var g = g
        guard let i = g.nodes.firstIndex(where: { $0.id == id }) else { return }
        mutate(&g.nodes[i])
        var flow = f
        let (ops, canvas) = fromGraph(g, previousOps: f.operations)
        flow.operations = ops
        flow.canvasJSON = canvas
        state.saveFlow(flow)
    }

    private func removeNode(_ id: String, _ f: Flow, _ g: FlowGraph) {
        var g = g
        g.nodes.removeAll { $0.id == id }
        g.edges.removeAll { $0.source == id || $0.target == id }
        var flow = f
        let (ops, canvas) = fromGraph(g, previousOps: f.operations)
        flow.operations = ops
        flow.canvasJSON = canvas
        state.saveFlow(flow)
        state.selection = .flow
    }

    // MARK: - Edge inspector

    @ViewBuilder
    private func edgeInspector(_ id: String) -> some View {
        if let f = state.flows.first(where: { $0.id == state.selectedFlowID }) {
            let g = toGraph(f)
            if let e = g.edges.first(where: { $0.id == id }) {
                Text("Operation").font(.headline)
                let op = e.operation

                HStack {
                    VStack(alignment: .leading) {
                        Text("From").font(.caption).foregroundStyle(.secondary)
                        Text("\(op.sourceRemote.isEmpty ? "local" : op.sourceRemote):\(op.sourcePath)")
                            .font(.caption).lineLimit(2)
                    }
                    Image(systemName: "arrow.right")
                    VStack(alignment: .leading) {
                        Text("To").font(.caption).foregroundStyle(.secondary)
                        Text("\(op.targetRemote.isEmpty ? "local" : op.targetRemote):\(op.targetPath)")
                            .font(.caption).lineLimit(2)
                    }
                }

                Picker("Action", selection: .init(
                    get: { e.action },
                    set: { v in updateEdge(id, f, g) { $0.action = v } }
                )) {
                    Text("Push").tag("push")
                    Text("Bi (two-way)").tag("bi")
                    Text("Bi resync").tag("bi-resync")
                }

                if let st = state.runtimeFlow(f.id)?.ops.first(where: { $0.id == id }) {
                    Text("Status: \(st.status)").font(.caption)
                    if !st.lastError.isEmpty {
                        Text(st.lastError).font(.caption).foregroundStyle(.red)
                    }
                }

                OperationFlagsEditor(op: op) { newCfg in
                    updateEdgeConfig(id, f, g, newCfg)
                }

                Button("Delete operation", role: .destructive) {
                    removeEdge(id, f, g)
                }
            }
        }
    }

    private func updateEdge(_ id: String, _ f: Flow, _ g: FlowGraph, _ mutate: (inout GraphEdge) -> Void) {
        var g = g
        guard let i = g.edges.firstIndex(where: { $0.id == id }) else { return }
        mutate(&g.edges[i])
        var flow = f
        let (ops, canvas) = fromGraph(g, previousOps: f.operations)
        flow.operations = ops
        flow.canvasJSON = canvas
        state.saveFlow(flow)
    }

    private func updateEdgeConfig(_ id: String, _ f: Flow, _ g: FlowGraph, _ cfg: [String: Any]) {
        var g = g
        guard let i = g.edges.firstIndex(where: { $0.id == id }) else { return }
        g.edges[i].operation.syncConfig = JSONDict(cfg)
        var flow = f
        let (ops, canvas) = fromGraph(g, previousOps: f.operations)
        flow.operations = ops
        flow.canvasJSON = canvas
        state.saveFlow(flow)
    }

    private func removeEdge(_ id: String, _ f: Flow, _ g: FlowGraph) {
        var g = g
        g.edges.removeAll { $0.id == id }
        var flow = f
        let (ops, canvas) = fromGraph(g, previousOps: f.operations)
        flow.operations = ops
        flow.canvasJSON = canvas
        state.saveFlow(flow)
        state.selection = .flow
    }
}

// MARK: - Remote path editor (browse via rclone lsjson)

struct RemotePathEditor: View {
    @EnvironmentObject var state: AppState
    let remote: String
    let path: String
    var onChange: (String) -> Void

    @State private var browsing = false
    @State private var entries: [FileEntry] = []
    @State private var browsePath = "/"

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            TextField("Path", text: .init(get: { path }, set: onChange))
                .textFieldStyle(.roundedBorder)
                .font(.caption.monospaced())
            Button("Browse…") {
                browsePath = path == "/" ? "/" : path
                reload()
                browsing = true
            }
            .font(.caption)
            .popover(isPresented: $browsing) {
                VStack(spacing: 0) {
                    HStack {
                        Text("\(remote.isEmpty ? "local" : remote):\(browsePath)")
                            .font(.caption.monospaced())
                        Spacer()
                        if browsePath != "/" {
                            Button("Up") {
                                browsePath = (browsePath as NSString).deletingLastPathComponent
                                if browsePath.isEmpty { browsePath = "/" }
                                reload()
                            }
                        }
                    }.padding(8)
                    Divider()
                    List(entries, id: \.path) { e in
                        HStack {
                            Image(systemName: e.isDir ? "folder" : "doc")
                                .foregroundStyle(e.isDir ? Color.accentColor : Color.secondary)
                            Text(e.name)
                            Spacer()
                        }
                        .contentShape(Rectangle())
                        .onTapGesture {
                            if e.isDir {
                                browsePath = browsePath == "/" ? "/" + e.name : browsePath + "/" + e.name
                                reload()
                            }
                        }
                    }
                    .frame(width: 320, height: 280)
                    HStack {
                        Button("Cancel") { browsing = false }
                        Spacer()
                        Button("Select") {
                            onChange(browsePath)
                            browsing = false
                        }.buttonStyle(.borderedProminent)
                    }.padding(8)
                }
            }
        }
    }

    private func reload() {
        entries = state.browse(remote.isEmpty ? browsePath : remote + ":" + browsePath)
    }
}

// MARK: - Operation flags editor (subset of OperationSettingsPanel)

struct OperationFlagsEditor: View {
    let op: FlowOperation
    var onChange: ([String: Any]) -> Void

    private func cfg(_ key: String) -> String {
        (op.syncConfig.value[key] as? String) ?? String(describing: op.syncConfig.value[key] ?? "")
    }
    private func boolCfg(_ key: String) -> Bool {
        (op.syncConfig.value[key] as? Bool) ?? false
    }
    private func set(_ key: String, _ v: Any?) {
        var c = op.syncConfig.value
        c[key] = v
        onChange(c)
    }

    var body: some View {
        DisclosureGroup("Options") {
            VStack(alignment: .leading, spacing: 8) {
                Toggle("Dry run", isOn: .init(get: { boolCfg("dry_run") }, set: { set("dry_run", $0) }))
                HStack {
                    Text("Parallel").font(.caption)
                    TextField("", text: .init(get: { cfg("parallel") }, set: { set("parallel", Int($0) ?? 0) }))
                        .textFieldStyle(.roundedBorder).frame(width: 60)
                    Text("Bandwidth MB/s").font(.caption)
                    TextField("", text: .init(get: { cfg("bandwidth") }, set: { set("bandwidth", Int($0) ?? 0) }))
                        .textFieldStyle(.roundedBorder).frame(width: 60)
                }
                TextField("Includes (one per line)", text: .init(
                    get: { (op.syncConfig.value["included_paths"] as? [String])?.joined(separator: "\n") ?? "" },
                    set: { set("included_paths", $0.split(separator: "\n").map(String.init)) }
                ), axis: .vertical)
                .textFieldStyle(.roundedBorder).font(.caption.monospaced())
                TextField("Excludes (one per line)", text: .init(
                    get: { (op.syncConfig.value["excluded_paths"] as? [String])?.joined(separator: "\n") ?? "" },
                    set: { set("excluded_paths", $0.split(separator: "\n").map(String.init)) }
                ), axis: .vertical)
                .textFieldStyle(.roundedBorder).font(.caption.monospaced())
            }
        }
        .font(.caption)
    }
}
