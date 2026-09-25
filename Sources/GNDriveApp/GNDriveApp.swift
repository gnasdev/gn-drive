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

struct RootView: View {
    @EnvironmentObject var state: AppState
    @State private var showSettings = false

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
