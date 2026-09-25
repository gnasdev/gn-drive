// Per-edge file transfer card — port of SyncEdge.vue's file panel.
// AGENTS.md rules:
//  - pending rows stay visible here but never become edge dots.
//  - active rows (transferring/checking) show a percentage.
import SwiftUI
import GNDriveCore

struct EdgeCardView: View {
    @EnvironmentObject var state: AppState
    let edge: GraphEdge
    let status: String
    var onClose: () -> Void

    private var op: FlowOperation { edge.operation }

    private var sync: SyncProgressEvent? {
        guard let rf = state.runtimeFlow(edge.operation.flowID),
              let s = rf.sync,
              RuntimeHub.splitBusyKey(s.profileID)?.1 == edge.id else { return nil }
        return s
    }

    private var files: [FileTransferEvent] { sync?.transfers ?? [] }

    private var orderedFiles: [FileTransferEvent] {
        let rank: [String: Int] = ["transferring": 0, "checking": 1, "failed": 2,
                                   "pending": 3, "checked": 4, "completed": 5]
        return files.sorted {
            (rank[$0.status] ?? 9) - (rank[$1.status] ?? 9) != 0
                ? (rank[$0.status] ?? 9) < (rank[$1.status] ?? 9)
                : $0.name.localizedCompare($1.name) == .orderedAscending
        }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack {
                Text(edge.action.uppercased())
                    .font(.caption).fontWeight(.bold)
                    .padding(.horizontal, 6).padding(.vertical, 2)
                    .background(statusColor(status).opacity(0.15), in: RoundedRectangle(cornerRadius: 4))
                VStack(alignment: .leading, spacing: 0) {
                    Text("\(op.sourceRemote.isEmpty ? "local" : op.sourceRemote):\(op.sourcePath)")
                        .font(.caption2).lineLimit(1)
                    Text("→ \(op.targetRemote.isEmpty ? "local" : op.targetRemote):\(op.targetPath)")
                        .font(.caption2).foregroundStyle(.secondary).lineLimit(1)
                }
                Spacer()
                Button(action: onClose) { Image(systemName: "xmark") }
                    .buttonStyle(.borderless)
            }

            if let s = sync {
                HStack(spacing: 12) {
                    if s.totalFiles > 0 {
                        Text("\(s.filesTransferred)/\(s.totalFiles) files")
                    }
                    if s.transferred > 0 {
                        Text("\(humanSize(s.transferred)) @ \(humanSize(Int64(s.bytesPerSec)))/s")
                    }
                    if s.errors > 0 {
                        Text("\(s.errors) errors").foregroundStyle(.red)
                    }
                    if !s.stage.isEmpty {
                        Text(s.stage).foregroundStyle(.secondary)
                    }
                }
                .font(.caption2)
            }

            ScrollView {
                LazyVStack(alignment: .leading, spacing: 2) {
                    ForEach(orderedFiles, id: \.name) { f in
                        FileRow(file: f)
                    }
                    if files.isEmpty {
                        Text(status.isEmpty ? "No activity yet" : status)
                            .font(.caption).foregroundStyle(.secondary)
                            .padding(.vertical, 8)
                    }
                }
            }
            .frame(maxHeight: 220)
        }
        .padding(12)
        .frame(width: 400)
        .background(.background, in: RoundedRectangle(cornerRadius: 10))
        .overlay(RoundedRectangle(cornerRadius: 10).stroke(Color.secondary.opacity(0.3)))
        .shadow(radius: 10)
    }

    private func humanSize(_ n: Int64) -> String {
        let k = Double(1024)
        switch n {
        case ..<Int64(k): return "\(n)B"
        case ..<Int64(k * k): return String(format: "%.1fK", Double(n) / k)
        case ..<Int64(k * k * k): return String(format: "%.1fM", Double(n) / (k * k))
        default: return String(format: "%.2fG", Double(n) / (k * k * k))
        }
    }
}

struct FileRow: View {
    let file: FileTransferEvent

    var body: some View {
        HStack(spacing: 6) {
            Image(systemName: icon)
                .font(.caption2)
                .foregroundStyle(color)
                .frame(width: 14)
            Text(file.name)
                .font(.caption)
                .lineLimit(1)
                .truncationMode(.middle)
            Spacer()
            // Active rows display their percentage (AGENTS rule).
            if file.status == "transferring" || file.status == "checking" {
                Text("\(Int(min(max(file.progress, 0), 100)))%")
                    .font(.caption2).monospacedDigit()
            }
            if file.status == "failed", !file.error.isEmpty {
                Text("failed").font(.caption2).foregroundStyle(.red)
            }
        }
        .padding(.vertical, 1)
    }

    private var icon: String {
        switch file.status {
        case "transferring": return "arrow.up.circle.fill"
        case "checking": return "magnifyingglass.circle"
        case "completed", "checked": return "checkmark.circle.fill"
        case "failed": return "exclamationmark.circle.fill"
        default: return "circle.dashed"
        }
    }
    private var color: Color {
        switch file.status {
        case "transferring", "checking": return .accentColor
        case "completed", "checked": return .green
        case "failed": return .red
        default: return .secondary
        }
    }
}
