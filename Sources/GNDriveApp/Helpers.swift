// Shared UI helpers — port of lib/humanizeError.ts + RemotePathField.vue.
import SwiftUI
import GNDriveCore

/// Turn backend/rclone error blobs into short user-facing messages.
/// Port of lib/humanizeError.ts.
func humanizeError(_ raw: String?, status: String) -> String {
    if status == "cancelled" || status == "cancelling" { return "" }
    var s = (raw ?? "").trimmingCharacters(in: .whitespacesAndNewlines)
    if s.isEmpty { return "" }
    let lower = s.lowercased()
    if lower.contains("signal: killed") || lower.contains("signal: interrupt")
        || lower.contains("context canceled") || lower.contains("context cancelled")
        || lower.contains("errtaskcancelled") || lower.contains("task cancelled") {
        return ""
    }
    if s.hasPrefix("rclone:") || s.hasPrefix("rclone: ") {
        s = String(s.dropFirst("rclone:".count)).trimmingCharacters(in: .whitespaces)
    }
    if let i = s.range(of: "(stderr:")?.lowerBound {
        s = String(s[..<i]).trimmingCharacters(in: .whitespaces)
    }
    if s.hasPrefix("{") || s.contains("\"stats\"") {
        return "Sync failed. Check paths and remote configuration."
    }
    if s.count > 160 { s = String(s.prefix(157)) + "…" }
    return s
}

func isUserCancelError(_ raw: String?, status: String) -> Bool {
    if status == "cancelled" || status == "cancelling" { return true }
    let s = (raw ?? "").lowercased()
    return s.contains("signal: killed") || s.contains("signal: interrupt")
        || s.contains("context canceled") || s.contains("context cancelled")
        || s.contains("errtaskcancelled") || s.contains("task cancelled")
}

/// Remote path field: text input + remote-aware browser popover.
/// Port of forms/RemotePathField.vue.
struct RemotePathEditor: View {
    @EnvironmentObject var state: AppState
    let remote: String
    let path: String
    var disabled: Bool = false
    var onChange: (String) -> Void

    @State private var browsing = false
    @State private var entries: [FileEntry] = []
    @State private var browsePath = "/"

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            TextField("Path", text: .init(get: { path }, set: onChange))
                .textFieldStyle(.roundedBorder)
                .font(.caption.monospaced())
                .disabled(disabled)
            Button("Browse…") {
                browsePath = path == "/" ? "/" : path
                reload()
                browsing = true
            }
            .font(.caption)
            .disabled(disabled)
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
