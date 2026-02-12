# Changelog

All notable changes to NS-Drive will be documented in this file.

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

- **Master password protection** — Added optional master password that encrypts `rclone.conf` and `ns-drive.db` at rest using AES-256-GCM with Argon2id key derivation. Includes lock/unlock lifecycle, rate limiting with exponential backoff, crash recovery, and encrypted export/import support.

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
- **Start at login** — Option to launch NS-Drive automatically at system startup.
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
