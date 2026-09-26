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
            ToolbarItem(placement: .navigation) {
                HStack(spacing: 4) {
                    // Engine status dot (replaces the web SSE indicator).
                    Circle()
                        .fill(Color.green)
                        .frame(width: 7, height: 7)
                    Text(t("topbar.connected"))
                        .font(.caption).fontWeight(.semibold).foregroundStyle(.secondary)
                }
            }
            ToolbarItemGroup(placement: .primaryAction) {
                Menu {
                    Picker("Language", selection: $locale) {
                        Text("English").tag("en")
                        Text("Tiếng Việt").tag("vi")
                    }
                } label: { Image(systemName: "globe") }
                    .help("Language")

                Button { theme = theme == "dark" ? "light" : "dark" } label: {
                    Image(systemName: theme == "dark" ? "moon.fill" : "sun.max")
                }
                .help("Theme")

                Button { showRemotes.toggle() } label: {
                    Image(systemName: "externaldrive.connected.to.line.below")
                }
                .help(t("workspace.remotes"))

                Button(action: onOpenSettings) { Image(systemName: "gear") }
                    .help(t("nav.settings"))

                Button(role: .destructive) { confirmLock = true } label: {
                    Image(systemName: "lock")
                }
                .help(t("topbar.lock"))
            }
        }
        .sheet(isPresented: $showRemotes) {
            RemotesView()
        }
        .alert(t("topbar.lockTitle"), isPresented: $confirmLock) {
            Button(t("topbar.lock"), role: .destructive) { state.lockNow() }
            Button(t("common.cancel"), role: .cancel) {}
        } message: {
            Text(t("topbar.lockMessage"))
        }
    }

    @AppStorage("gn-drive:theme") private var theme = "light"
    @AppStorage("gn-drive:locale") private var locale = "en"
    @State private var confirmLock = false
}

// MARK: - Flow rail

struct FlowRailView: View {
    @EnvironmentObject var state: AppState
    @Binding var showRemotes: Bool

    var body: some View {
        VStack(spacing: 0) {
            HStack {
                Text(t("workspace.flows")).font(.caption).fontWeight(.semibold).foregroundStyle(.secondary)
                Spacer()
                Button { _ = state.addFlow(name: "Untitled flow") } label: {
                    Image(systemName: "plus").help(t("flows.add"))
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
    @State private var confirmDelete = false

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
            Button(t("workspace.run")) { state.executeFlow(flow.id) }
            Button(t("workspace.stop")) { state.stopFlow(flow.id) }
            Divider()
            Button(t("common.delete"), role: .destructive) { confirmDelete = true }
        }
        .alert(t("flows.deleteTitle"), isPresented: $confirmDelete) {
            Button(t("common.delete"), role: .destructive) { state.deleteFlow(flow.id) }
            Button(t("common.cancel"), role: .cancel) {}
        } message: {
            Text(t("flows.deleteMessage", args: ["name": flow.name.isEmpty ? t("workspace.untitledFlow") : flow.name]))
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
