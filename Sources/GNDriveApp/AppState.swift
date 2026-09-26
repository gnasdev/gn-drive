// App state — bridges GNDriveCore to SwiftUI (replaces Vue stores + SSE).
import Foundation
import GNDriveCore
import Combine

@MainActor
final class AppState: ObservableObject {
    let container: AppContainer

    @Published var authStatus: AuthStatus
    @Published var flows: [Flow] = []
    @Published var remotes: [Remote] = []
    @Published var profiles: [Profile] = []
    @Published var history: [HistoryEntry] = []
    @Published var runtime = RuntimeSnapshotEvent(revision: 0, flows: [])
    @Published var selectedFlowID: String? = nil
    @Published var selection: Selection? = nil
    @Published var toasts: [Toast] = []
    @Published var appSettings = AppSettings()
    @Published var rcloneVersion: String = ""
    @Published var serviceStatusText: String = ""
    /// Flows with unsaved local edits (canvas/inspector changes staged but
    /// not persisted) — mirrors the web canvas.dirty flag.
    @Published var dirtyFlowIDs: Set<String> = []

    enum Selection: Equatable {
        case node(String)
        case edge(String)
        case flow
    }

    struct Toast: Identifiable {
        let id = UUID()
        let message: String
        let isError: Bool
    }

    nonisolated(unsafe) private var busCancel: (() -> Void)?

    init() throws {
        var opts = AppContainer.Options()
        opts.portalMode = true
        opts.keyStore = KeychainStore(service: "gn-drive")
        opts.version = BuildVersion.current
        container = try AppContainer(opts: opts)
        authStatus = container.auth.status()

        // Re-hydrate runtime projection on every relevant bus event.
        busCancel = container.bus.subscribeAll(topics: BusTopic.all) { [weak self] _, _ in
            Task { @MainActor in
                self?.refreshRuntime()
            }
        }

        container.syncEngine.start()

        if container.store != nil {
            reloadData()
        }
        appSettings = container.auth.appSettings()
        if let rc = container.rclone {
            rcloneVersion = (try? rc.version()) ?? ""
        }
    }

    deinit { busCancel?() }

    /// Suspend on quit: re-encrypt config files if locked-by-policy (keeps
    /// the remembered key like Go's auth.Suspend).
    func shutdown() {
        container.syncEngine.stop()
        container.close()
    }

    var isUnlocked: Bool { authStatus.unlocked }
    var isSetup: Bool { authStatus.setup }

    // MARK: - Auth

    func unlock(_ password: String) {
        do {
            try container.auth.unlock(password)
            try container.afterUnlock()
            authStatus = container.auth.status()
            container.bus.publish(BusTopic.authUnlocked, AuthUnlockedEvent())
            reloadData()
            toast("Unlocked")
        } catch let AuthError.locked(secs) {
            toast("Locked — retry in \(secs)s", isError: true)
        } catch {
            authStatus = container.auth.status()
            toast(friendly(error), isError: true)
        }
    }

    func setup(_ password: String) {
        do {
            try container.auth.setupPassword(password)
            try container.afterUnlock()
            authStatus = container.auth.status()
            container.bus.publish(BusTopic.authUnlocked, AuthUnlockedEvent())
            reloadData()
            toast("Password set")
        } catch {
            toast(friendly(error), isError: true)
        }
    }

    func lockNow() {
        do {
            container.beforeLock()
            try container.auth.lock()
            authStatus = container.auth.status()
            container.bus.publish(BusTopic.authLocked, AuthLockedEvent())
            flows = []; remotes = []; profiles = []
            runtime = RuntimeSnapshotEvent(revision: 0, flows: [])
        } catch {
            toast(friendly(error), isError: true)
        }
    }

    func changePassword(old: String, new: String) -> Bool {
        do {
            try container.auth.changePassword(old: old, new: new)
            toast("Password changed")
            return true
        } catch {
            toast(friendly(error), isError: true)
            return false
        }
    }

    func removePassword(_ password: String) -> Bool {
        do {
            container.beforeLock()
            try container.auth.removePassword(password)
            try container.afterUnlock()
            authStatus = container.auth.status()
            toast("Password removed")
            return true
        } catch {
            toast(friendly(error), isError: true)
            return false
        }
    }

    // MARK: - Data

    func reloadData() {
        guard let store = container.store else { return }
        flows = (try? store.listFlows()) ?? []
        profiles = (try? store.listProfiles()) ?? []
        history = (try? store.listHistory(limit: 100, offset: 0)) ?? []
        if let rc = container.rclone {
            remotes = (try? rc.listRemotes()) ?? []
        }
        refreshRuntime()
    }

    func refreshRuntime() {
        runtime = container.runtime.snapshot()
        authStatus = container.auth.status()
    }

    func runtimeFlow(_ id: String) -> RuntimeFlowState? {
        runtime.flows.first { $0.id == id }
    }

    func flowStatus(_ id: String) -> String {
        runtimeFlow(id)?.status ?? container.flowEngine.status(id)
    }

    func isFlowActive(_ id: String) -> Bool {
        ["running", "cancelling"].contains(flowStatus(id))
    }

    // MARK: - Flows

    @discardableResult
    func addFlow(name: String) -> Flow {
        var f = Flow()
        f.id = UUID().uuidString
        f.name = name.isEmpty ? "Untitled flow" : name
        do {
            try container.store?.saveFlow(&f)
            reloadData()
            selectedFlowID = f.id
            toast("Flow added")
        } catch {
            toast(friendly(error), isError: true)
        }
        return f
    }

    /// Apply local edits without persisting (web: canvas.writeLocal + dirty).
    func stageFlow(_ f: Flow) {
        flows = flows.map { $0.id == f.id ? f : $0 }
        dirtyFlowIDs.insert(f.id)
    }

    /// Persist a flow to SQLite (clears the dirty flag).
    func saveFlow(_ f: Flow) {
        // Web API: PUT /flows rejects updates while the flow is running.
        guard !isFlowActive(f.id) else {
            toast("Cannot change this while a run is in progress", isError: true)
            return
        }
        var flow = f
        do {
            try container.store?.saveFlow(&flow)
            container.syncEngine.syncFlowSchedule(flow)
            container.bus.publish(BusTopic.stateChanged, StateChangedEvent(domain: "flows", id: flow.id))
            dirtyFlowIDs.remove(flow.id)
            reloadData()
        } catch {
            toast(friendly(error), isError: true)
        }
    }

    func deleteFlow(_ id: String) {
        guard !isFlowActive(id) else {
            toast("Flow is running — stop it first", isError: true)
            return
        }
        do {
            try container.store?.deleteFlow(id)
            container.syncEngine.unregisterFlowSchedule(id)
            container.runtime.forget(id)
            if selectedFlowID == id { selectedFlowID = flows.first(where: { $0.id != id })?.id }
            container.bus.publish(BusTopic.stateChanged, StateChangedEvent(domain: "flows", id: id))
            reloadData()
        } catch {
            toast(friendly(error), isError: true)
        }
    }

    func executeFlow(_ id: String) {
        do {
            try container.flowEngine.execute(flowID: id)
        } catch {
            toast(friendly(error), isError: true)
        }
        refreshRuntime()
    }

    func stopFlow(_ id: String) {
        do {
            try container.flowEngine.stop(flowID: id)
        } catch {
            toast(friendly(error), isError: true)
        }
        refreshRuntime()
    }

    // MARK: - Remotes

    func addRemote(name: String, type: String, config: [String]) {
        guard let rc = container.rclone else { return }
        do {
            try rc.createRemoteVerified(name: name, type: type, configKVs: config)
            container.bus.publish(BusTopic.stateChanged, StateChangedEvent(domain: "remotes", id: name))
            reloadData()
            toast("Remote \(name) added")
        } catch {
            toast(friendly(error), isError: true)
        }
    }

    func deleteRemote(_ name: String) {
        do {
            try container.rclone?.deleteRemote(name)
            reloadData()
            toast("Remote \(name) deleted")
        } catch {
            toast(friendly(error), isError: true)
        }
    }

    func testRemote(_ name: String) {
        do {
            try container.rclone?.testRemote(name)
            toast("Remote \(name): OK")
        } catch {
            toast("Remote \(name) failed: \(friendly(error))", isError: true)
        }
    }

    func browse(_ remotePath: String) -> [FileEntry] {
        (try? container.rclone?.listFiles(remotePath)) ?? []
    }

    // MARK: - Settings

    func saveSettings(_ s: AppSettings) {
        do {
            try container.auth.setAppSettings(s)
            appSettings = s
        } catch {
            toast(friendly(error), isError: true)
        }
    }

    // MARK: - Misc

    func toast(_ message: String, isError: Bool = false) {
        let t = Toast(message: message, isError: isError)
        toasts.append(t)
        DispatchQueue.main.asyncAfter(deadline: .now() + 4) { [weak self] in
            self?.toasts.removeAll { $0.id == t.id }
        }
    }

    func friendly(_ e: Error) -> String {
        String(describing: e)
    }
}
