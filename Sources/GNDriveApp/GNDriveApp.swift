// Native macOS app — replaces the Vue SPA + portal.
import SwiftUI
import GNDriveCore

@main
struct GNDriveApp: App {
    @StateObject private var state: AppState

    init() {
        let s: AppState
        do {
            s = try AppState()
        } catch {
            fatalError("gn-drive app init: \(error)")
        }
        _state = StateObject(wrappedValue: s)
        // Encrypt config files back on quit (auth.Suspend semantics).
        NotificationCenter.default.addObserver(
            forName: NSApplication.willTerminateNotification,
            object: nil, queue: .main) { _ in
                s.shutdown()
        }
    }

    var body: some Scene {
        WindowGroup("GN Drive") {
            RootView()
                .environmentObject(state)
                .frame(minWidth: 900, minHeight: 600)
        }
        .windowStyle(.titleBar)
    }
}

enum ThemePreference: String {
    case light, dark, system
}

struct RootView: View {
    @EnvironmentObject var state: AppState
    @State private var showSettings = false
    @AppStorage("gn-drive:theme") private var theme = "light"
    @AppStorage("gn-drive:locale") private var locale = "en"

    var body: some View {
        Group {
            if !state.isUnlocked {
                UnlockView()
            } else if showSettings {
                SettingsView(onClose: { showSettings = false })
            } else {
                WorkspaceView(onOpenSettings: { showSettings = true })
            }
        }
        .preferredColorScheme(theme == "dark" ? .dark : theme == "light" ? .light : nil)
        .id(locale) // re-render whole tree when the locale changes
        .overlay(alignment: .bottom) {
            ToastsView()
        }
    }
}

struct ToastsView: View {
    @EnvironmentObject var state: AppState

    var body: some View {
        VStack(spacing: 6) {
            ForEach(state.toasts) { t in
                Text(t.message)
                    .font(.callout)
                    .padding(.horizontal, 12).padding(.vertical, 7)
                    .background(t.isError ? Color.red.opacity(0.9) : Color.primary.opacity(0.85),
                                in: RoundedRectangle(cornerRadius: 6))
                    .foregroundStyle(.white)
            }
        }
        .padding(.bottom, 16)
    }
}
