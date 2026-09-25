// Workspace: flow rail + canvas + inspector — port of WorkspacePage.vue.
import SwiftUI
import GNDriveCore

struct WorkspaceView: View {
    @EnvironmentObject var state: AppState
    var onOpenSettings: () -> Void
    @State private var showRemotes = false

    var body: some View {
        HSplitView {
            FlowRailView(showRemotes: $showRemotes)
                .frame(minWidth: 180, idealWidth: 220, maxWidth: 260)
            FlowCanvasView()
                .frame(minWidth: 400)
            InspectorView()
                .frame(minWidth: 240, idealWidth: 300, maxWidth: 380)
        }
        .toolbar {
            ToolbarItem(placement: .primaryAction) {
                Button { showRemotes.toggle() } label: {
                    Image(systemName: "externaldrive.connected.to.line.below")
                }
                .help("Remotes")
            }
            ToolbarItem(placement: .primaryAction) {
                Button(action: onOpenSettings) { Image(systemName: "gear") }
                    .help("Settings")
            }
        }
        .sheet(isPresented: $showRemotes) {
            RemotesView()
        }
    }
}

// MARK: - Flow rail

struct FlowRailView: View {
    @EnvironmentObject var state: AppState
    @Binding var showRemotes: Bool

    var body: some View {
        VStack(spacing: 0) {
            HStack {
                Text("Flows").font(.caption).fontWeight(.semibold).foregroundStyle(.secondary)
                Spacer()
                Button { _ = state.addFlow(name: "Untitled flow") } label: {
                    Image(systemName: "plus")
                }
                .buttonStyle(.borderless)
            }
            .padding(8)

            List(selection: $state.selectedFlowID) {
                ForEach(state.flows, id: \.id) { f in
                    FlowRow(flow: f)
                        .tag(f.id)
                }
            }
            .listStyle(.sidebar)

            Divider()
            Button { showRemotes.toggle() } label: {
                Label("Remotes (\(state.remotes.count))", systemImage: "externaldrive.connected.to.line.below")
                    .font(.callout)
            }
            .buttonStyle(.borderless)
            .padding(8)
        }
    }
}

struct FlowRow: View {
    @EnvironmentObject var state: AppState
    let flow: Flow

    var body: some View {
        let status = state.flowStatus(flow.id)
        HStack(spacing: 6) {
            Circle()
                .fill(statusColor(status))
                .frame(width: 7, height: 7)
            VStack(alignment: .leading, spacing: 1) {
                Text(flow.name.isEmpty ? "Untitled flow" : flow.name)
                    .font(.callout)
                    .lineLimit(1)
                Text("\(flow.operations.count) ops\(flow.scheduleEnabled ? " · ⏰" : "")")
                    .font(.caption2).foregroundStyle(.secondary)
            }
            Spacer()
        }
        .contextMenu {
            Button("Run") { state.executeFlow(flow.id) }
            Button("Stop") { state.stopFlow(flow.id) }
            Divider()
            Button("Delete", role: .destructive) { state.deleteFlow(flow.id) }
        }
    }
}

func statusColor(_ s: String) -> Color {
    switch s {
    case "running": return .accentColor
    case "cancelling": return .orange
    case "completed": return .green
    case "failed": return .red
    case "cancelled": return .secondary
    default: return .gray.opacity(0.4)
    }
}
