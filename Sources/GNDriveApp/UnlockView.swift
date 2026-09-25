// Unlock/setup screen — port of UnlockPage.vue.
import SwiftUI

struct UnlockView: View {
    @EnvironmentObject var state: AppState
    @State private var password = ""
    @State private var confirm = ""
    @State private var error = ""

    var mode: Mode { state.isSetup ? .unlock : .setup }
    enum Mode { case setup, unlock }

    var body: some View {
        VStack(spacing: 0) {
            Spacer()
            VStack(alignment: .leading, spacing: 16) {
                HStack(spacing: 8) {
                    Image(systemName: "cloud.fill").font(.title2)
                    Text("GN Drive").font(.title3).fontWeight(.semibold)
                }
                Text(mode == .setup
                     ? "Set a master password to encrypt your config at rest."
                     : "Enter your master password to unlock.")
                    .font(.callout).foregroundStyle(.secondary)

                SecureField("Password", text: $password)
                    .textFieldStyle(.roundedBorder)
                    .onSubmit(submit)
                if mode == .setup {
                    SecureField("Confirm password", text: $confirm)
                        .textFieldStyle(.roundedBorder)
                        .onSubmit(submit)
                }

                if !error.isEmpty {
                    Text(error).font(.caption).foregroundStyle(.red)
                }
                if state.authStatus.lockout.isLocked {
                    Text("Locked — retry in \(state.authStatus.lockout.retryAfterSecs)s")
                        .font(.caption).foregroundStyle(.orange)
                }

                Button(mode == .setup ? "Set password" : "Unlock") { submit() }
                    .buttonStyle(.borderedProminent)
                    .disabled(password.count < 4 || (mode == .setup && password != confirm))
            }
            .padding(28)
            .frame(width: 380)
            .background(.background, in: RoundedRectangle(cornerRadius: 10))
            .shadow(radius: 8)
            Spacer()
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }

    private func submit() {
        error = ""
        if mode == .setup {
            guard password == confirm else { error = "Passwords do not match"; return }
            guard password.count >= 4 else { error = "Password must be at least 4 characters"; return }
            state.setup(password)
        } else {
            state.unlock(password)
        }
    }
}
