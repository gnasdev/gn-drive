# Swift → Rust port

GN Drive's engine and CLI live in this repo's Cargo workspace. The GPUI window is `crates/gn-drive-gpui` in `ns-gpui` (`$NS_GPUI_DIR`, default `~/Github/ngosangns/ns-gpui`). It links `gn-drive-core` in-process and calls `gn_drive_core::rpc::Dispatcher` (`handle` / `subscribe`). `gn-drive serve --stdio` is the same dispatcher over NDJSON.

Swift sources stay in `Sources/` until the coordinator deletes them. Do not delete them from this port.

## Workspace

| Crate | Path | Role |
|---|---|---|
| `gn-drive-core` | `crates/gn-drive-core` | Library: auth, store, rclone, engines, RPC |
| `gn-drive-cli` | `crates/gn-drive-cli` | Binary `gn-drive` |

Edition 2024, `rust-version` 1.95. Dev builds compile `argon2` and `blake2` at opt-level 3 so tests stay fast. Feature `test-util` exposes `rclone::testing::FakeRunner` and the OAuth fake spawner. Integration tests enable it through a dev-dependency on this crate, so `cargo test --workspace` runs them without `--features`.

## Module map

| Swift | Rust | Status |
|---|---|---|
| `Paths.swift` | `paths.rs` | Ported. `GN_DRIVE_CONFIG_DIR`, same file names |
| `Models.swift` | `models.rs` | Ported. Profile JSON is camelCase, `None` omitted |
| `Crypto.swift` | `crypto.rs` | Ported. File format `[12-byte nonce][AES-256-GCM ct\|\|tag]` |
| `Argon2` / `CArgon2` | `argon2id.rs` | Ported. `$argon2id$v=19$m=65536,t=3,p=4$…` |
| `AuthService.swift` | `auth.rs` | Ported. Setup, unlock, lock, suspend, change, remove |
| `KeychainStore.swift` | `keychain.rs` | Ported. Same service `gn-drive`, raw generic-password bytes. Tests use `MemorySecureStore`. Real keychain round-trip is `#[ignore]` unless `GN_DRIVE_TEST_KEYCHAIN=1` |
| `Database.swift` + `Store.swift` | `database.rs`, `store.rs` | Ported. Same SQLite schema and file name `gn-drive.db` |
| `RcloneClient.swift` | `rclone/client.rs` | Ported. All spawns go through `RcloneRunner` |
| `RcloneTypes.swift` | `rclone/types.rs` | Ported |
| `StatsParser.swift` | `rclone/stats.rs` | Ported |
| (test seam) | `rclone/testing.rs` | `FakeRunner`. Unmatched calls exit 0. No real rclone |
| `SyncEngine.swift` | `sync_engine.rs` | Ported |
| `FlowEngine.swift` | `flow_engine.rs` | Ported. Registered as `FlowExecutor` on the sync engine |
| `BoardEngine.swift` | `board_engine.rs` | Ported. Layer edges run in parallel. A cancelled sync cancels the board run |
| `Cron.swift` | `cron.rs` | Ported |
| `EventBus.swift` | `event_bus.rs` | Ported |
| `RuntimeHub.swift` | `runtime_hub.rs` | Ported. Holds `Weak` refs back to the engines |
| `AppContainer.swift` | `container.rs` | Ported. `close` stops the engine and breaks the flow cycle. `before_lock` does not |
| `Logger.swift` | `logger.rs` | Ported |
| `InstanceLock.swift` | `instance_lock.rs` | Ported |
| `HealthWriter.swift` | `health.rs` | Ported |
| `Launchd.swift` | `launchd.rs` | Ported. Same label and plist paths. Tests inject a fake `launchctl` |
| `SelfUpdate.swift` | `self_update.rs` | Ported. Asset-name mismatch with CI is unchanged (see `docs/gpui/rpc.md` §14.2) |
| `BuildVersion.swift` | `build_version.rs` | Ported |
| `RPC/*` | `rpc/` | Ported. Every method in `docs/gpui/rpc.md` §0 is dispatched |
| `Sources/gn-drive` | `crates/gn-drive-cli` | Ported. `run`, `serve`, `service`, `sync`, `board`, `profile`, `remote`, `self-update`, `version`, `doctor`, `completion` |

## Data compatibility

Fixtures from `scripts/gen-compat-fixtures.sh` live in `crates/gn-drive-core/tests/fixtures/` (password `compat-test-pw-123`, not a user secret).

`tests/compat_fixtures.rs` checks:

- The locked fixture's `auth.json` hash verifies, unlock decrypts `rclone.conf.enc` and `gn-drive.db.enc`, and profiles, schedules, history, settings, boards, flows, and delta state match `expected.json`.
- Locking again and unlocking reads the same rows and the same `rclone.conf` bytes (Rust ciphertext round-trip).
- The plaintext fixture opens with no password and matches the same dump.

Swift reading a Rust re-encrypted tree is `swift_reads_rust_ciphertext` (`#[ignore]`, so a normal `cargo test` does not rebuild Swift). It runs `scripts/gen-compat-fixtures.sh --verify` on a temp copy. That check passed on 2026-10-22:

```bash
cargo test -p gn-drive-core --test compat_fixtures -- --ignored swift_reads_rust_ciphertext
```

That command builds the Swift package. It uses `MemorySecureStore` and a temp dir.

`create_remote` (CLI `remote add`) passes `--non-interactive`. OAuth backends fail closed instead of opening a browser. The RPC wizard uses `create_remote_non_interactive` and returns `oauth_required`; `remotes.oauth.start` is the only path that spawns `rclone authorize`, and only for that call. Tests inject `FakeSpawner` / `FakeRunner`.

## What is not in this repo

- The GPUI window. Build it from ns-gpui. `task dev:install` / `task dev:run`. `task dev:run` sets `GN_DRIVE_MOCK=1` unless you override it. A non-mock run needs a fake rclone and a temp config dir.
- The `.app` bundle. `task dist` builds the release CLI and prints where the bundle step lives (ns-gpui). `task swift:dist` still packs the SwiftUI app.
- Deleting Swift. The coordinator does that after this port is accepted.

## Checks

```bash
cargo build --workspace
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
scripts/rpc-smoke.sh
```

`scripts/rpc-smoke.sh` builds the Rust CLI, points it at a temp `GN_DRIVE_CONFIG_DIR`, and a fake rclone that refuses `authorize` / `reconnect` / `config create`.
