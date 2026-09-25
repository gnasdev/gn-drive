// Settings — port of SettingsPage.vue: master password, service, app prefs,
// self-update, history.
import SwiftUI
import GNDriveCore

struct SettingsView: View {
    @EnvironmentObject var state: AppState
    var onClose: () -> Void

    @State private var oldPwd = ""
    @State private var newPwd = ""
    @State private var removePwd = ""
    @State private var showRemoveConfirm = false

    var body: some View {
        NavigationStack {
            List {
                // Master password
                Section("Master password") {
                    if state.isSetup {
                        HStack {
                            SecureField("Current", text: $oldPwd)
                            SecureField("New", text: $newPwd)
                            Button("Change") {
                                if state.changePassword(old: oldPwd, new: newPwd) {
                                    oldPwd = ""; newPwd = ""
                                }
                            }
                            .disabled(newPwd.count < 4 || oldPwd.isEmpty)
                        }
                        .textFieldStyle(.roundedBorder)
                        HStack {
                            Button("Lock now") { state.lockNow(); onClose() }
                            Spacer()
                            Button("Remove password…", role: .destructive) { showRemoveConfirm = true }
                        }
                    } else {
                        Text("No master password set").foregroundStyle(.secondary)
                    }
                }

                // App settings (persisted in auth.json)
                Section("Application") {
                    Toggle("Notifications", isOn: binding(\.notificationsEnabled))
                    Toggle("Debug mode", isOn: binding(\.debugMode))
                    Toggle("Minimize to tray", isOn: binding(\.minimizeToTray))
                    Toggle("Start at login", isOn: binding(\.startAtLogin))
                }

                // Background service (launchd)
                Section("Background service") {
                    ServiceSection()
                }

                // Version / update
                Section("About") {
                    HStack {
                        Text("Version")
                        Spacer()
                        Text(BuildVersion.current).foregroundStyle(.secondary)
                    }
                    HStack {
                        Text("rclone")
                        Spacer()
                        Text(state.rcloneVersion.isEmpty ? "not found" : state.rcloneVersion)
                            .foregroundStyle(.secondary)
                    }
                    Button("Check for updates") { checkUpdate() }
                }

                // History
                Section("History") {
                    if state.history.isEmpty {
                        Text("No runs yet").foregroundStyle(.secondary)
                    } else {
                        ForEach(state.history.prefix(20), id: \.id) { h in
                            HStack {
                                Text(h.profileName).font(.caption)
                                Spacer()
                                Text(h.state).font(.caption2)
                                    .foregroundStyle(statusColor(h.state))
                                Text(h.startedAt).font(.caption2).foregroundStyle(.secondary)
                            }
                        }
                    }
                }
            }
            .navigationTitle("Settings")
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button("Done", action: onClose)
                }
            }
        }
        .alert("Remove master password? Config files will be decrypted permanently.",
               isPresented: $showRemoveConfirm) {
            SecureField("Password", text: $removePwd)
            Button("Remove", role: .destructive) {
                _ = state.removePassword(removePwd)
                removePwd = ""
            }
            Button("Cancel", role: .cancel) { removePwd = "" }
        }
    }

    private func binding(_ key: WritableKeyPath<AppSettings, Bool>) -> Binding<Bool> {
        Binding(get: { state.appSettings[keyPath: key] },
                set: { var s = state.appSettings; s[keyPath: key] = $0; state.saveSettings(s) })
    }

    private func checkUpdate() {
        var opts = UpdateOptions()
        opts.currentVersion = BuildVersion.current
        DispatchQueue.global().async {
            do {
                let (cur, latest) = try SelfUpdate.check(opts: opts)
                Task { @MainActor in
                    state.toast(cur == latest ? "Already on latest (\(cur))" : "Update available: \(latest)")
                }
            } catch {
                Task { @MainActor in state.toast("Update check failed: \(error)", isError: true) }
            }
        }
    }
}

struct ServiceSection: View {
    @EnvironmentObject var state: AppState
    @State private var status = ""

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(status).font(.caption).foregroundStyle(.secondary)
            HStack {
                Button("Install") { service("install") }
                Button("Start") { service("start") }
                Button("Stop") { service("stop") }
                Button("Restart") { service("restart") }
            }
            .controlSize(.small)
            Text("Service runs 'gn-drive run --service' via launchd (label \(LaunchdService.label)).")
                .font(.caption2).foregroundStyle(.tertiary)
        }
        .onAppear { refresh() }
    }

    private func refresh() {
        let mgr = LaunchdService()
        var spec = ServiceSpec()
        spec.configDir = Paths.detect().configDir
        if !mgr.isInstalled(spec) {
            status = "Not installed"
            return
        }
        if let st = try? mgr.status(spec) {
            status = st.running ? "Running (pid \(st.pid))" : "Installed, not running"
        } else {
            status = "Installed"
        }
    }

    private func service(_ action: String) {
        let mgr = LaunchdService()
        var spec = ServiceSpec()
        spec.configDir = Paths.detect().configDir
        spec.execPath = cliBinaryPath()
        do {
            switch action {
            case "install": try mgr.install(spec)
            case "start": try mgr.start(spec)
            case "stop": try mgr.stop(spec)
            case "restart": try mgr.restart(spec)
            default: break
            }
            state.toast("Service \(action) ok")
        } catch {
            state.toast("Service \(action) failed: \(error)", isError: true)
        }
        refresh()
    }

    /// The launchd job needs the CLI binary (gn-drive), not the app binary.
    /// Look for a sibling `gn-drive` next to the app executable, else PATH.
    private func cliBinaryPath() -> String {
        if let exe = Bundle.main.executablePath {
            let sibling = (exe as NSString).deletingLastPathComponent + "/gn-drive"
            if FileManager.default.isExecutableFile(atPath: sibling) { return sibling }
        }
        for dir in ProcessInfo.processInfo.environment["PATH"]?.split(separator: ":") ?? [] {
            let p = String(dir) + "/gn-drive"
            if FileManager.default.isExecutableFile(atPath: p) { return p }
        }
        return "gn-drive"
    }
}
