# GN Drive

A local-only sync engine and web UI for cloud storage synchronization powered by [rclone](https://rclone.org/). One binary, one process, one port — the sync engine and the Vue 3 web UI run side by side and serve on loopback only.

> **Status**: v0.4.0-alpha (single-process CLI + Vue 3 web stack, opt-in service).
> The legacy Wails desktop app lives in `desktop/` and is removed in Phase 7.

## Features

- **Multi-Cloud Sync** — Google Drive, Dropbox, OneDrive, iCloud Drive, Yandex Disk, Google Photos, and [any rclone-supported provider](https://rclone.org/docs/)
- **Sync Profiles** — pull / push / bi-sync / bi-resync with bandwidth limits, parallel transfers, include/exclude patterns, and bisync conflict resolution
- **Schedules** — Cron-based automated sync (`robfig/cron/v3`)
- **Boards** — DAG-based multi-step workflows (topological execution)
- **Flows** — Sequential operations with embedded profile config
- **File Operations** — copy, move, check, list, mkdir, purge, delete on remotes
- **Local-only loopback** — Web UI and API bind `127.0.0.1`; no reverse proxy, no public URL
- **Opt-in service** — Run as a systemd / launchd / SCM service only when you choose
- **Auth** — Master password (Argon2id + AES-256-GCM) with rate limit and crash recovery
- **Self-update** — `gn-drive self-update` fetches the latest release and atomically swaps the binary

## Tech Stack

| Component | Technology |
|-----------|------------|
| Backend | Go 1.23+ — single binary `cmd/gn-drive` |
| Frontend | Vue 3.5+ + Vite 6+ + TypeScript (Phase 5) |
| HTTP | `chi` router + Server-Sent Events |
| Database | SQLite (via `modernc.org/sqlite`, pure-Go, no CGo) |
| Sync | rclone v1.74.2 (shell-out wrapper) |
| Auth | Argon2id + AES-256-GCM (`golang.org/x/crypto/argon2`) |
| CLI | `spf13/cobra` with subcommands and shell completion |

## Installation

### From source

```bash
go install github.com/gnasdev/gn-drive/cmd/gn-drive@latest
```

The binary lands in `$GOPATH/bin/gn-drive`.

### Build locally

```bash
git clone https://github.com/gnasdev/gn-drive.git
cd gn-drive
go build -o gn-drive ./cmd/gn-drive
```

## Usage

```bash
# Show version
gn-drive version

# Diagnose environment
gn-drive doctor

# Run in foreground (auto-port, opens browser)
gn-drive run

# Run without opening browser
gn-drive run --no-browser

# Run as background service (opt-in)
gn-drive service install
gn-drive service start
gn-drive service status
gn-drive service stop
gn-drive service uninstall

# One-shot sync (no web server)
gn-drive sync pull --profile backup
gn-drive sync push --profile photos
gn-drive sync bi --profile workspace
gn-drive sync dry-run --profile backup

# Manage profiles
gn-drive profile list
gn-drive profile add --name backup --from "local:/data" --to "gdrive:Backup"
gn-drive profile delete backup

# Manage rclone remotes
gn-drive remote list
gn-drive remote add --name gdrive --type drive
gn-drive remote test gdrive
gn-drive remote delete gdrive

# Update to latest release
gn-drive self-update
```

Run `gn-drive <subcommand> --help` for flag documentation.

## Configuration

Configuration lives in `~/.config/gn-drive/`:

```
gn-drive.db          # SQLite database (or .enc when locked)
rclone.conf          # rclone remotes (or .enc when locked)
auth.json            # password hash + rate limit state + app settings
service.health       # JSON, written by service mode every 5s
gn-drive.lock        # flock advisory lock
gn-drive.pid         # pid file
```

Override the config dir via `XDG_CONFIG_HOME` on Linux.

## License

MIT
