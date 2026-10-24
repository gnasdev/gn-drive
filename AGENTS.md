# Rust engine + CLI

- Engine and CLI are the Cargo workspace: `crates/gn-drive-core` (library) and `crates/gn-drive-cli` (binary `gn-drive`). Module map and data compatibility: `docs/rust-port.md`.
- The GPUI window is `~/Github/ngosangns/ns-gpui/crates/gn-drive-gpui`. It path-depends on `gn-drive-core` and calls `gn_drive_core::rpc::Dispatcher` in-process (`handle(method, params) -> Result<Value, RpcError>`, `subscribe()`). Do not edit ns-gpui from this repo. Keep that API stable and record any change under `docs/gpui/rpc.md` "Changelog". `gn-drive serve --stdio` is the same dispatcher over NDJSON.
- Dev flow: `task build`, `task test`, `task check`, `task cli:install:dev`, `task dev:install`, `task dev:run`. Dev CLI: `~/.local/state/gn-drive/dev/bin/gn-drive`. `task dev:run` sets `GN_DRIVE_MOCK=1`. `task dist` builds the release CLI; the app bundle is ns-gpui's job. Swift tasks are `task swift:*`.
- Never trigger real OAuth/sign-in in tests, smoke scripts, or app runs. Use `FakeRunner` / `FakeSpawner`, or `serve --rclone` plus a temp `GN_DRIVE_CONFIG_DIR`. Do not run `rclone authorize`, `reconnect`, or `config create` for a token backend, and do not let rclone bind `:53682` or open a browser. `remotes.add` and `gn-drive remote add` are `--non-interactive` and return a prompt instead of starting OAuth. OAuth starts only from an explicit `remotes.oauth.start`. One in-flight authorize at a time; it is cancellable and is not restarted automatically. Do not launch `/Applications/GN Drive.app`.

# Swift tree (kept until the coordinator deletes it)

- `Sources/GNDriveCore` and `Sources/gn-drive` are the Swift originals. Do not edit them for the Rust port. `GNDriveApp` is the SwiftUI app. Go (`internal/`, `cmd/`) and Vue (`frontend/`) are reference only.
- Argon2id hashes stay `$argon2id$v=19$m=65536,t=3,p=4$…`. `.enc` files are `[12-byte nonce][AES-256-GCM ct||tag]`. The SQLite schema is unchanged.
- Swift build/test/bundle: `task swift:build`, `task swift:test`, `task swift:dist`, `task swift:dev`. Those are not the default `task build` / `task test` / `task dist`.

# Runtime edge telemetry

- Backend `sync:*` events and runtime snapshots are the source of truth for edge telemetry. Keep `profile_id` in the `flowId:operationId` form so `FlowCanvas` can route a snapshot to its operation edge.
- During pre-sync, file rows are commonly `pending`. They must remain visible in the edge card, but must not render as static edge dots. Edge dots require a live or terminal file state, and an active file row must display its percentage.
- A selected edge with an active operation opens its file card automatically. A card opened this way must remain open across pan and zoom; only an explicit outside click or another user action may dismiss it.
- Any change to runtime event handling must cover: live WebSocket event, reload/runtime snapshot, pending-file rendering, and active-operation edge routing. Add a targeted regression test for each affected boundary.

## Branching

- Work only on `main`. Do not create new branches.
- Commit directly on `main` and push to `origin/main`.
- Exception: Firstmate delivery workers may use one short-lived `fm/<task>` branch per task to open a pull request into `main`; that branch is deleted as soon as it is merged.
