// Remotes management — port of WorkspaceRemotesSection + remoteConfig.ts.
import SwiftUI
import GNDriveCore

struct RemoteField {
    let key: String
    let label: String
    let isSecure: Bool
    let required: Bool
}

/// Fields to collect before create+test — same matrix as the web UI.
func remoteConfigFields(_ type: String) -> [RemoteField] {
    let userPass = [
        RemoteField(key: "user", label: "Username", isSecure: false, required: true),
        RemoteField(key: "pass", label: "Password", isSecure: true, required: true),
    ]
    let hostUserPass = [RemoteField(key: "host", label: "Host", isSecure: false, required: true)] + userPass
    switch type {
    case "mega": return userPass
    case "sftp", "ftp": return hostUserPass
    case "webdav":
        return [RemoteField(key: "url", label: "URL", isSecure: false, required: true)] + userPass
    case "s3":
        return [
            RemoteField(key: "provider", label: "Provider", isSecure: false, required: false),
            RemoteField(key: "access_key_id", label: "Access key ID", isSecure: false, required: true),
            RemoteField(key: "secret_access_key", label: "Secret access key", isSecure: true, required: true),
            RemoteField(key: "region", label: "Region", isSecure: false, required: false),
            RemoteField(key: "endpoint", label: "Endpoint", isSecure: false, required: false),
        ]
    case "b2":
        return [
            RemoteField(key: "account", label: "Account ID", isSecure: false, required: true),
            RemoteField(key: "key", label: "Application key", isSecure: true, required: true),
        ]
    case "crypt":
        return [
            RemoteField(key: "remote", label: "Wrapped remote", isSecure: false, required: true),
            RemoteField(key: "password", label: "Crypt password", isSecure: true, required: true),
        ]
    case "alias":
        return [RemoteField(key: "remote", label: "Wrapped remote", isSecure: false, required: true)]
    default:
        return []
    }
}

let remoteTypes = ["local", "drive", "dropbox", "onedrive", "iclouddrive", "yandex",
                   "googlephotos", "mega", "s3", "b2", "sftp", "ftp", "webdav",
                   "crypt", "alias", "pcloud", "box", "sharepoint"]

struct RemotesView: View {
    @EnvironmentObject var state: AppState
    @Environment(\.dismiss) private var dismiss
    @State private var adding = false
    @State private var confirmDelete: String? = nil

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack {
                Text(t("workspace.remotes")).font(.headline)
                Spacer()
                Button { adding = true } label: { Label(t("common.add"), systemImage: "plus") }
            }
            .padding()

            List {
                ForEach(state.remotes, id: \.name) { r in
                    HStack {
                        Image(systemName: "externaldrive.connected.to.line.below")
                            .foregroundStyle(.secondary)
                        VStack(alignment: .leading) {
                            Text(r.name).font(.callout)
                            Text(r.type.isEmpty ? "unknown" : r.type)
                                .font(.caption2).foregroundStyle(.secondary)
                        }
                        Spacer()
                        Button(t("common.test")) { state.testRemote(r.name) }.buttonStyle(.borderless)
                        Button(role: .destructive) { confirmDelete = r.name } label: {
                            Image(systemName: "trash")
                        }.buttonStyle(.borderless)
                    }
                }
            }
        }
        .frame(width: 460, height: 420)
        .sheet(isPresented: $adding) { AddRemoteView() }
        .alert(t("remotes.deleteTitle"), isPresented: .init(
            get: { confirmDelete != nil }, set: { if !$0 { confirmDelete = nil } })) {
            Button(t("common.delete"), role: .destructive) {
                if let n = confirmDelete { state.deleteRemote(n) }
                confirmDelete = nil
            }
            Button(t("common.cancel"), role: .cancel) { confirmDelete = nil }
        }
    }
}

struct AddRemoteView: View {
    @EnvironmentObject var state: AppState
    @Environment(\.dismiss) private var dismiss
    @State private var name = ""
    @State private var type = "local"
    @State private var values: [String: String] = [:]
    @State private var busy = false

    private var fields: [RemoteField] { remoteConfigFields(type) }
    private var missing: [String] {
        fields.filter { $0.required && (values[$0.key] ?? "").isEmpty }.map(\.key)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text(t("remotes.add")).font(.headline)
            TextField(t("common.name"), text: $name).textFieldStyle(.roundedBorder)
            Picker(t("common.type"), selection: $type) {
                ForEach(remoteTypes, id: \.self) { Text($0).tag($0) }
            }
            ForEach(fields, id: \.key) { f in
                if f.isSecure {
                    SecureField(f.label, text: binding(f.key)).textFieldStyle(.roundedBorder)
                } else {
                    TextField(f.label, text: binding(f.key)).textFieldStyle(.roundedBorder)
                }
            }
            if type != "local" && fields.isEmpty {
                Text("This provider may require OAuth — it will be verified after creation; a browser window may open.")
                    .font(.caption).foregroundStyle(.secondary)
            }
            HStack {
                Spacer()
                Button(t("common.cancel")) { dismiss() }
                Button(t("remotes.testAndAdd")) {
                    busy = true
                    let kvs = fields.compactMap { f -> String? in
                        let v = values[f.key]?.trimmingCharacters(in: .whitespaces) ?? ""
                        return v.isEmpty ? nil : "\(f.key)=\(v)"
                    }
                    state.addRemote(name: name, type: type, config: kvs)
                    dismiss()
                }
                .buttonStyle(.borderedProminent)
                .disabled(name.isEmpty || !missing.isEmpty || busy)
            }
        }
        .padding()
        .frame(width: 380)
    }

    private func binding(_ key: String) -> Binding<String> {
        Binding(get: { values[key] ?? "" }, set: { values[key] = $0 })
    }
}
