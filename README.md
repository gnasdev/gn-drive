# GN Drive

Local-only sync engine and web UI for cloud storage, powered by [rclone](https://rclone.org/). One binary, one process, one port — the sync engine, HTTP API, and Vue 3 SPA share a single process and bind loopback only.

> **Status**: engine and CLI are Rust (`crates/gn-drive-core`, `crates/gn-drive-cli`). The window is the GPUI app in `apps/gn-drive-gpui`, which links `gn-drive-core` in-process. Swift (`Sources/`) stays until the coordinator removes it. Go (`internal/`, `cmd/`) and the Vue SPA are reference only.

## Features

- **Multi-cloud sync** — Google Drive, Dropbox, OneDrive, iCloud Drive, Yandex Disk, Google Photos, and [any rclone-supported provider](https://rclone.org/docs/)
- **Flows & operations** — sequential work units (source → target + action) with optional cron; primary product surface in the web UI
- **Sync profiles** — named option bags for CLI one-shot sync (`push` / `bi` / `bi-resync`; CLI also supports `pull` / `dry-run`)
- **Remotes** — manage rclone remotes (list / add / test / delete)
- **Boards** — DAG of nodes/edges for CLI `gn-drive board` (store + board engine; not shown in the current workspace UI)
- **File browse** — path pickers via `/api/v1/operations/fs`
- **Local-only loopback** — web UI and API bind `127.0.0.1` on static port **53241**
- **Opt-in service** — systemd (Linux), launchd (macOS), SCM (Windows)
- **Master password** — Argon2id + AES-256-GCM encrypts `rclone.conf` and the SQLite DB at rest; portal mode unlocks in the browser
- **Self-update** — `gn-drive self-update` or Settings UI

## Tech stack (Rust)

| Component | Technology |
|-----------|------------|
| Core | `gn-drive-core` — auth, SQLite, rclone, sync/flow/board, RPC `Dispatcher` |
| CLI | `gn-drive-cli` — binary `gn-drive` (clap) |
| UI | GPUI window in `apps/gn-drive-gpui` (path dep on this core) |
| Database | SQLite via `rusqlite` (bundled) — same schema and file names as Swift |
| Sync | rclone shell-out behind `RcloneRunner` (tests use `FakeRunner`) |
| Auth | Argon2id (`$argon2id$v=19$m=65536,t=3,p=4$…`) + AES-256-GCM `[12-byte nonce][ct\|\|tag]` |

Build: `cargo build --workspace` or `task build`. Tests: `cargo test --workspace` or `task test`. SwiftPM remains available as `task swift:build` / `task swift:test` until Swift is removed. Mapping and data-compat notes: [docs/rust-port.md](./docs/rust-port.md).

## Installation

### From source

### Build locally

```bash
git clone https://github.com/gnasdev/gn-drive.git
cd gn-drive
task build          # cargo build --workspace
task cli:install:dev
```

Requires: Rust 1.95+, rclone on `PATH` at runtime (dev and tests inject a fake). A Swift toolchain is only needed for `task swift:*` and the optional fixture verifier.

## Usage

```bash
# Diagnose environment
gn-drive doctor

# Foreground portal (http://127.0.0.1:53241/, opens browser)
gn-drive run

# Fixed port override (still loopback-only)
gn-drive run --port 54000

# No browser
gn-drive run --no-browser

# Background service (opt-in)
gn-drive service install
gn-drive service start
gn-drive service status

# One-shot sync (no web server)
gn-drive sync push --profile backup
gn-drive sync bi --profile workspace
gn-drive sync dry-run --profile backup

# Profiles / remotes / board DAG
gn-drive profile list
gn-drive remote list
gn-drive board <id-or-name>

# Update binary
gn-drive self-update
```

Run `gn-drive <subcommand> --help` for flags.

### Service unlock

Service mode has no UI. Pass the master password at start:

```bash
gn-drive run --service --password '…'
# or
GN_DRIVE_PASSWORD='…' gn-drive run --service
```

## Configuration

Default config dir: `~/.config/gn-drive/` (Linux honors `XDG_CONFIG_HOME`).

```text
gn-drive.db          # SQLite (or .enc when locked)
rclone.conf          # rclone remotes (or .enc when locked)
auth.json            # password hash, rate limit, app settings
service.health       # JSON heartbeat in service mode
gn-drive.lock        # flock advisory lock
gn-drive.pid         # pid file
```

## GPUI window & dev install

The GPUI window (`gn-drive-gpui` in `apps/gn-drive-gpui`) links `gn-drive-core` in-process and calls `Dispatcher::handle` / `subscribe` ([docs/gpui/rpc.md](./docs/gpui/rpc.md)). `gn-drive serve --stdio` is the same protocol on NDJSON. The SwiftUI app stays until the coordinator removes Swift.

```bash
task cli:install:dev   # release gn-drive -> ~/.local/state/gn-drive/dev/bin/gn-drive
task dev:install       # that, plus the Dev app via ns-gpui/scripts/install-dev.sh
task dev:run           # cargo run -p gn-drive-gpui; GN_DRIVE_MOCK=1 unless overridden
task check             # cargo fmt --check, clippy -D warnings, cargo test --workspace
task smoke             # scripts/rpc-smoke.sh (fake rclone, temp config)
```

`NS_GPUI_DIR` overrides the ns-gpui checkout that holds `scripts/install-dev.sh` (default `~/Github/ngosangns/ns-gpui`). `task dist` builds the release CLI; the Dev `.app` bundle comes from `task dev:install`. Tests and the smoke script use a fake rclone and a temp `GN_DRIVE_CONFIG_DIR`. They never start an OAuth flow. Do not launch the release `/Applications` GN Drive app from here.

## Documentation

| Doc | Purpose |
|-----|---------|
| [DEVELOPER.md](./DEVELOPER.md) | Setup, tasks, crate map |
| [docs/rust-port.md](./docs/rust-port.md) | Swift → Rust module map and data compatibility |
| [docs/README.md](./docs/README.md) | Knowledge base entry |
| [docs/overview.md](./docs/overview.md) | Product scope and runtime shape |
| [docs/_index.md](./docs/_index.md) | Navigation map |
| [frontend/SPEC.md](./frontend/SPEC.md) | Workspace UI product surface |

## License

MIT
