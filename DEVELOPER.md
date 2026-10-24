# Developer Guide

Contributor notes for GN Drive. User-facing install/usage stays in [README.md](./README.md). Architecture detail lives under [docs/](./docs/).

## Project map

```text
crates/gn-drive-core/     Engine library (auth, store, rclone, engines, rpc::Dispatcher)
crates/gn-drive-cli/      Binary `gn-drive`
Sources/                  Swift engine + CLI + SwiftUI app (kept until the coordinator deletes it)
internal/, cmd/, frontend/  Go + Vue reference only — do not extend
docs/gpui/                RPC protocol and use cases the GPUI window codes against
docs/rust-port.md         Swift → Rust module map and data-compat notes
~/Github/ngosangns/ns-gpui/crates/gn-drive-gpui
                          GPUI window. Path-depends on gn-drive-core and calls Dispatcher in-process.
```

## Runtime shape

```text
gn-drive-gpui  --in process-->  rpc::Dispatcher::handle / subscribe
gn-drive serve --stdio     -->  same Dispatcher over NDJSON
Dispatcher --> AppContainer --> auth, store, sync/flow/board engines, rclone
```

**Portal mode** (`gn-drive run`, and `serve`): the process starts while locked. Unlock opens the store and rclone and attaches the engines.

**CLI one-shots** (`sync`, `profile`, `remote`, `board`, …): non-portal. A configured master password needs `--password` / `GN_DRIVE_PASSWORD`, or an already-plaintext config.

There is no loopback HTTP server and no embedded SPA in the Rust build.

## Requirements

- Rust **1.95+** (edition 2024)
- **rclone** on `PATH` for a real sync. Tests, smoke, and `task doctor` put a fake rclone first on `PATH` or pass `--rclone`
- Optional: [Task](https://taskfile.dev)
- Swift toolchain only for `task swift:*` and `scripts/gen-compat-fixtures.sh`

## Setup

```bash
git clone https://github.com/gnasdev/gn-drive.git
cd gn-drive
task build
task test
```

## Common commands

| Command | Purpose |
|---------|---------|
| `task build` | `cargo build --workspace` |
| `task test` | `cargo test --workspace` |
| `task check` | `cargo fmt --all -- --check`, `clippy -D warnings`, tests |
| `task smoke` | `scripts/rpc-smoke.sh` (fake rclone, temp config) |
| `task cli:install:dev` | Release CLI → `~/.local/state/gn-drive/dev/bin/gn-drive` |
| `task dev:install` | Dev CLI + `ns-gpui/scripts/install-dev.sh gn-drive` |
| `task dev:run` | `cargo run -p gn-drive-gpui` with `GN_DRIVE_MOCK=1` |
| `task dist` | Release CLI. The `.app` bundle is built in ns-gpui |
| `task swift:*` | Previous SwiftPM tasks (`swift:build`, `swift:test`, `swift:dist`, …) |

The Go server and Vue SPA are reference only. Do not start them for GN Drive development.

## Frontend notes (reference only)

- **Product shell**: Unlock → Workspace (`/`) → Settings (`/settings`). Legacy multi-page routes redirect to `/`.
- **Domain**: Flow = unit of work; Operation = one source/target step; Remotes for path pickers. Profiles are CLI/API option bags, not workspace cards. Boards are not on the workspace surface.
- Types mirror Go JSON contracts in `frontend/src/api/types.ts`.
- i18n: `en` + `vi` under `frontend/src/i18n/locales/`.
- E2E: Puppeteer via `frontend/scripts/e2e-run.mjs` (`pnpm run test:e2e` / `test:e2e:coverage`).

See [frontend/SPEC.md](./frontend/SPEC.md).

## Backend notes

- **Config dir**: `~/.config/gn-drive` (override `GN_DRIVE_CONFIG_DIR`). Files: `auth.json`, `rclone.conf` / `.enc`, `gn-drive.db` / `.enc`, `gn-drive.lock`, `service.health`.
- **Instance lock**: flock under the config dir.
- **Profile / flow actions**: `push` | `bi` | `bi-resync`. Legacy `pull` normalizes to `push`.
- **SQLite schema** is the Swift/Wails schema. Existing `gn-drive.db` files open unchanged. See [docs/rust-port.md](./docs/rust-port.md).
- **OAuth**: only `remotes.oauth.start` spawns `rclone authorize`, and only from that call. `remotes.add` and `gn-drive remote add` pass `--non-interactive` and do not open a browser. Tests use `FakeRunner` / `FakeSpawner`. Never point a test at the real rclone for a token backend.

## Release

`task dist` builds `target/release/gn-drive`. Packing `GN Drive.app` (window + bundled rclone) belongs to ns-gpui. The GitHub workflow that publishes the Go/Swift artifacts is unchanged until the coordinator switches it. The known self-update asset-name mismatch is documented in `docs/gpui/rpc.md` §14.2.

## GPUI window & dev install

`ns-gpui/crates/gn-drive-gpui` depends on this repo's `gn-drive-core` by path and calls `gn_drive_core::rpc::Dispatcher` in-process (`handle(method, params) -> Result<Value, RpcError>`, `subscribe()`). Protocol: `docs/gpui/rpc.md`. Use cases: `docs/gpui/use-cases.md`. Keep that API stable; note any change under rpc.md "Changelog".

| Command | Purpose |
|---------|---------|
| `task check` | fmt, clippy `-D warnings`, `cargo test --workspace` |
| `task cli:install:dev` | Release `gn-drive` → `~/.local/state/gn-drive/dev/bin/gn-drive` (does not touch `~/.local/bin`) |
| `task dev:install` | `cli:install:dev` + `$NS_GPUI_DIR/scripts/install-dev.sh gn-drive` |
| `task dev:run` | `cargo run -p gn-drive-gpui` in `$NS_GPUI_DIR`, with `GN_DRIVE_MOCK=1` |

`NS_GPUI_DIR` defaults to `~/Github/ngosangns/ns-gpui`.

Safety: temp `GN_DRIVE_CONFIG_DIR`, fake rclone (`--rclone` or a temp `PATH`). Never `config create` / `reconnect` / `authorize` against a token backend, and never loop those tests. Do not launch `/Applications/GN Drive.app`. `task dev:run` stays on `GN_DRIVE_MOCK=1` unless you pass a fake rclone and a temp config yourself.

## Docs

Knowledge base under `docs/` follows Open Knowledge Format (YAML frontmatter + markdown). Entry points:

- [docs/overview.md](./docs/overview.md)
- [docs/_index.md](./docs/_index.md)
- [docs/_sync.md](./docs/_sync.md)

When behavior changes, update the smallest set of docs and refresh `_sync.md`.

## Layout conventions

- `AppContainer` is the composition root. Engines are created before the data plane; store and rclone attach on unlock.
- English comments for non-obvious contracts only.
- Unit tests sit next to the module. Crate integration tests: `crates/gn-drive-core/tests/`, `crates/gn-drive-cli/tests/`.
- Do not print secrets. Do not read the user's real config, keychain, or launchd from tests.
