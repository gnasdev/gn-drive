# Changelog

All notable changes to GN Drive will be documented in this file.

## [v1.0.0] - 2026-06-09

### Major: Web stack GA — Wails deprecated

- **Single-process CLI + Vue 3 web UI** is the only supported runtime. The Wails v3 desktop app (Angular 21) has been removed.
- One binary, one process, one port. Sync engine, HTTP API, and Vue SPA share the process. No IPC, no JSON-RPC, no separate daemon.
- Loopback-only binding (`127.0.0.1`) with auto-port. CORS restricted to `localhost` and `127.0.0.1`.
- 10 CLI subcommands: `run`, `service`, `sync`, `board`, `profile`, `remote`, `self-update`, `version`, `doctor`, `completion`.
- HTTP API at `/api/v1` (45+ endpoints) with chi router + SSE event stream.
- 11 Vue 3 routes: Unlock, Dashboard, Profiles, Remotes, Operations, Boards, Flows, Schedules, History, Service, Settings.
- Service mode (opt-in): `gn-drive service install` registers with systemd (Linux) / launchd (macOS) / SCM (Windows). `service.health` heartbeat file written every 5s.
- Cross-platform: Linux, macOS, Windows. Builds produce a single `~12MB` binary with embedded frontend.

### Migration from v0.3.x

- Existing `~/.config/gn-drive/` data is loadable without migration: `auth.json`, `rclone.conf`, `gn-drive.db` schema are unchanged.
- Just install the new binary: `go install github.com/gnasdev/gn-drive/cmd/gn-drive@latest` and run `gn-drive run`.
- If you had the Wails desktop app installed, uninstall it manually (the binary is no longer built).

### Developer migration

- `go.mod` is at the root (was at `desktop/`). Run `go build ./cmd/gn-drive`.
- Frontend in `frontend/` (was at `desktop/frontend/`). Run `pnpm install` then `task build:fe` (or `task build` for the full pipeline).
- See `docs/specs/planning/refactor-gn-drive-web-stack.md` for the full plan and rationale.

## [v0.4.0-alpha] - 2026-06-09

### Refactor

- **Web stack alpha** — Single-process CLI + Vue 3 frontend (Phase 1+2 of `docs/specs/planning/refactor-gn-drive-web-stack.md`).
  - New `cmd/gn-drive/` CLI with `cobra` (10 subcommands: run, service, sync, board, profile, remote, self-update, version, doctor, completion).
  - `internal/{config, ports, instance, eventbus, logging, auth, store, rclone, syncengine, app}` packages with constructor-based dependency injection (no package-level singletons).
  - SQLite store with 7 repositories (settings, profile, schedule, history, board, flow, delta) and 47-column profile schema (matches Wails format for cross-compat).
  - Argon2id + AES-256-GCM auth with rate limit, crash recovery, and `--unlock-stdin` for service mode.
  - rclone shell-out wrapper (`exec.Command("rclone", ...)`) — no rclone Go library dependency.
- **Cleanup** — Removed orphaned Wails build artifacts (`gn-drive`, `gn-drive.app/`, `scripts/build-macos.sh`, `desktop/frontend/`, `desktop/Taskfile.yml`, empty `internal/config/config.go` placeholder). Updated `README.md` and `.gitignore` to reflect the web stack. Legacy Wails desktop app retained in `desktop/` until Phase 7.

## [v0.3.0] - 2026-02-12

### Features

- **Zoneless Angular** — Removed zone.js and enabled Angular's `provideZonelessChangeDetection()`. All `NgZone.run()` calls eliminated, `detectChanges()` replaced with `markForCheck()` for idiomatic zoneless change detection. Canvas drag handlers retain `detectChanges()` for synchronous visual feedback.
- **Solarized Light theme** — Applied Solarized Light color scheme globally via CSS custom properties. Operation settings panel redesigned into card-based sections with colored headers using the Solarized palette.
- **Operation settings UI redesign** — Reorganized settings panel into collapsible card sections. Advanced settings collapsed by default for a cleaner initial view.
- **Flow delete confirmation** — Added confirmation dialog when deleting a non-empty flow to prevent accidental data loss.
- **Delta detection service** — Added backend delta detection service (`desktop/backend/delta/`) for change-based sync optimization with file watching and state persistence.
- **Check-phase progress display** — Added `TotalChecks` streaming from rclone and progress bar now shows check progress (`checks/totalChecks`) during the check-only phase.
- **Cache clearing before sync** — `ClearFsCache()` and `ClearStatsCache()` are now called before each flow edge execution for clean state.

### Improvements

- **Sync status UX** — Refactored sync-status and operation-logs-panel components to Angular signals (`input`/`computed`). Progress bar is green, syncing files yellow, status icons white.
- **Error/checks count accuracy** — Fixed error/checks count mismatch by deriving counts directly from the transfer list instead of separate counters.
- **Error file prioritization** — Error files are now prioritized in the transfer list (shown after syncing, before completed).
- **FastList always-on** — Removed FastList from user-configurable settings; now always enabled globally via `UseListR = true` for reduced API calls.
- **Reduced default window height** — Default window height reduced to 600px for better fit on smaller screens.

### Documentation

- Updated SECURITY.md — Fixed `LockoutStatus` fields, `auth.json` example, event routing.
- Updated API.md — Fixed `ExportOptions`/`ImportOptions`, added `FlowService`, updated all model structs.
- Updated ARCHITECTURE.md — Updated service count to 17, added 2-phase init, SQLite storage details.
- Updated EVENTS.md — Added `SyncProgressData`, Operation/Crypt events, fixed event routing.

---

## [v0.2.0] - 2026-02-10

### Features

- **Master password protection** — Added optional master password that encrypts `rclone.conf` and `gn-drive.db` at rest using AES-256-GCM with Argon2id key derivation. Includes lock/unlock lifecycle, rate limiting with exponential backoff, crash recovery, and encrypted export/import support.

### Fixes

- **TypeScript type mismatches** — Fixed `ExportOptions.encrypt_password` optional vs required, `ImportPreview` nullable fields, and null guard on `ValidateImportFileWithPassword` return.

### Documentation

- Added macOS Gatekeeper bypass instructions to README.

---

## [v0.1.0] - 2026-02-09

### Features

- **CI/CD pipeline** — GitHub Actions workflow for automated builds on Linux and macOS (ARM64).
- **Provider icons** — Added cloud provider icons in the remote management UI.
- **Path browser "." option** — Added current directory option in path browser for convenience.
- **Native macOS notifications** — Replaced `beeep` library with native macOS notifications via `UNUserNotificationCenter`.
- **Channel-driven sync status** — Replaced polling-based sync status with channel-driven structured DTOs for real-time progress updates.
- **Per-task rclone concurrency isolation** — Each sync task gets isolated config (`fs.AddConfig`), stats (`accounting.WithStatsGroup`), and filter context to prevent cross-task interference.
- **SyncConfig profile for operations** — Operations now use `SyncConfig Profile` (JSON column) instead of flat SQL columns with automatic DB migration.
- **Flows (replacing operations)** — Replaced operations tree with flows system, added DB service for persistence.
- **NeoBrutalism UI** — Reworked UI with NeoBrutalism-inspired operations tree interface.
- **Log service** — Added log buffer and log service for reliable log delivery with sequence numbers.
- **Import/Export** — Configuration backup (profiles, remotes, boards) with optional token inclusion and merge/replace restore modes.
- **System tray** — System tray integration with quick board execution.
- **Start at login** — Option to launch GN Drive automatically at system startup.
- **Desktop notifications** — Notifications for sync completion and failure events.
- **Board system** — Visual workflow orchestration with DAG execution, magnet highlight, and edge reconnection.
- **Profile editor** — Profile creation and editing dialog with validation.
- **Sidebar navigation** — Dashboard, file browser, history, schedules, and settings components.
- **macOS app bundle** — Enhanced macOS build process with app bundle creation and code signing.

### Fixes

- Fixed sync status display retention after flow execution completes.
- Fixed CI/CD: pinned wails3 version, reordered Linux deps, fixed Go cache path, recreated wailsjs symlink after bindings generation.
- Removed darwin/amd64 build target (macos-13 runner deprecated).
- Fixed tray menu functionality.
