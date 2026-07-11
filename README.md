# GN Drive

Local-only sync engine and web UI for cloud storage, powered by [rclone](https://rclone.org/). One binary, one process, one port — the sync engine, HTTP API, and Vue 3 SPA share a single process and bind loopback only.

> **Status**: web stack GA (single-process CLI + Vue 3). Wails desktop is removed. Default UI is a single **Workspace** after unlock (flows + remotes), plus Settings.

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

## Tech stack

| Component | Technology |
|-----------|------------|
| Backend | Go 1.25 — `cmd/gn-drive` |
| Frontend | Vue 3.5 + Vite 6 + TypeScript + Tailwind 4 + Pinia |
| HTTP | `chi` + Server-Sent Events |
| Database | SQLite (`modernc.org/sqlite`, pure Go) |
| Sync | rclone shell-out (`exec.Command`) |
| Auth | Argon2id + AES-256-GCM |
| CLI | `spf13/cobra` |

## Installation

### From source

```bash
go install github.com/gnasdev/gn-drive/cmd/gn-drive@latest
```

### Build locally

```bash
git clone https://github.com/gnasdev/gn-drive.git
cd gn-drive
task build
# or without task:
#   cd frontend && pnpm install && pnpm run build
#   rm -rf internal/webui/dist && cp -r frontend/dist internal/webui/dist
#   go build -o bin/gn-drive ./cmd/gn-drive
```

Requires: Go 1.25+, pnpm, rclone on `PATH` at runtime.

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

## Documentation

| Doc | Purpose |
|-----|---------|
| [DEVELOPER.md](./DEVELOPER.md) | Setup, tasks, package map, validation |
| [docs/README.md](./docs/README.md) | Knowledge base entry |
| [docs/overview.md](./docs/overview.md) | Product scope and runtime shape |
| [docs/_index.md](./docs/_index.md) | Navigation map |
| [frontend/SPEC.md](./frontend/SPEC.md) | Workspace UI product surface |

## License

MIT
