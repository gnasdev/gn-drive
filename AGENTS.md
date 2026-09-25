# Swift rewrite

- The app was rewritten in Swift (SwiftPM). `GNDriveCore` holds the former `internal/*` packages (auth, store, rclone, sync/flow/board engines, runtime hub, launchd, self-update). `gn-drive` is the CLI (swift-argument-parser). `GNDriveApp` is the SwiftUI macOS app replacing the Vue SPA — the engine runs in-process; there is no loopback HTTP server or SSE.
- The Go backend (`internal/`, `cmd/`) and Vue frontend (`frontend/`) are kept for reference; do not extend them.
- Argon2id hashes must stay compatible with the existing `auth.json` wire format (`$argon2id$v=19$m=65536,t=3,p=4$…`, vendored `CArgon2` reference implementation). `.enc` files are `[12-byte nonce][AES-256-GCM ct||tag]` (CryptoKit `SealedBox.combined`).
- The SQLite schema is unchanged; `gn-drive.db` files load transparently.
- Build: `swift build` / `task swift-build`; bundle: `task swift-app`; tests: `swift test` / `task swift-test`.

# Runtime edge telemetry

- Backend `sync:*` events and runtime snapshots are the source of truth for edge telemetry. Keep `profile_id` in the `flowId:operationId` form so `FlowCanvas` can route a snapshot to its operation edge.
- During pre-sync, file rows are commonly `pending`. They must remain visible in the edge card, but must not render as static edge dots. Edge dots require a live or terminal file state, and an active file row must display its percentage.
- A selected edge with an active operation opens its file card automatically. A card opened this way must remain open across pan and zoom; only an explicit outside click or another user action may dismiss it.
- Any change to runtime event handling must cover: live WebSocket event, reload/runtime snapshot, pending-file rendering, and active-operation edge routing. Add a targeted regression test for each affected boundary.

## Branching

- Work only on `main`. Do not create new branches.
- Commit directly on `main` and push to `origin/main`.
- Exception: Firstmate delivery workers may use one short-lived `fm/<task>` branch per task to open a pull request into `main`; that branch is deleted as soon as it is merged.
