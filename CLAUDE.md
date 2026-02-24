# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

NS-Drive is a desktop application for cloud storage synchronization powered by rclone. Built with Go 1.25 + Wails v3 backend and Angular 21 + Tailwind CSS + PrimeNG frontend. Data stored in SQLite (`~/.config/ns-drive/ns-drive.db`), encrypted when master password is enabled.

## Commands

```bash
# Development (requires 2 terminals)
task dev:fe                # Terminal 1: Angular dev server (port 9245)
task dev:be                # Terminal 2: Wails backend (after frontend ready)

# Build
task build                 # Production binary → ./ns-drive
task build:macos           # Signed macOS .app bundle

# Test
task test                  # All tests (backend + frontend)
task test:be               # Go tests only (cd desktop && go test -v ./...)
task test:fe               # Angular tests (Karma/Jasmine, headless Chrome)
task test:be:coverage      # Go tests with coverage report

# Single Go test
cd desktop && go test -v -run TestFunctionName ./backend/services/...

# Lint
task lint                  # Both frontend and backend
task lint:fe               # ESLint
task lint:be               # golangci-lint

# Regenerate TypeScript bindings after modifying Go services/models
cd desktop && wails3 generate bindings
```

## Architecture

### Backend (desktop/)

17 domain-separated services registered in [main.go](desktop/main.go). All service methods accept `context.Context` as first parameter for cancellation support.

**2-phase initialization**: Services defer DB/rclone loading until after auth unlock. `AuthService.ServiceStartup()` either initializes immediately (no auth) or waits for password unlock.

**Core Services** (`desktop/backend/services/`):
- **AuthService** — Master password (Argon2id + AES-256-GCM), encrypts/decrypts DB and rclone.conf
- **SyncService** — rclone sync operations (pull, push, bi, bi-resync) with context cancellation
- **ConfigService** — Profile CRUD
- **RemoteService** — rclone remote management
- **TabService** — Tab lifecycle and output logging
- **OperationService** — File operations (copy, move, check, dedupe, list, delete, mkdir, about, size)

**Workflow & Scheduling**:
- **BoardService** — Visual workflow DAG execution with topological sort and cycle detection
- **FlowService** — Named groups of sync operations
- **SchedulerService** — Cron-based scheduling (`robfig/cron/v3`)
- **HistoryService** — Operation history and statistics

**System Integration**:
- **TrayService** — System tray with quick board/flow execution
- **NotificationService** — Desktop notifications + app settings (minimize to tray, start at login)
- **LogService** — Reliable log delivery with sequence numbers
- **CryptService** — Encrypted remote creation (rclone crypt layer)
- **ExportService** / **ImportService** — Config backup/restore (`.nsd` files)

Legacy **App** service ([app.go](desktop/backend/app.go)) has additional methods exposed to frontend.

**Key packages**:
- `desktop/backend/rclone/` — rclone operations (sync, bisync, common, operations)
- `desktop/backend/models/` — Data models (Profile, Board/BoardNode/BoardEdge, ScheduleEntry, HistoryEntry, Flow)
- `desktop/backend/validation/` — ProfileValidator with path traversal prevention
- `desktop/backend/commands.go` — rclone command building logic

### Event-Driven Communication

Backend emits events to frontend via Wails event system. Format: `domain:action` (e.g., `sync:started`, `profile:updated`, `tab:output`). All events go through a unified `"tofe"` channel.

Frontend listens with:
```typescript
import { Events } from "@wailsio/runtime";
Events.On("sync:progress", (event) => { ... });
```

### Frontend (desktop/frontend/)

Angular 21 with standalone components (no modules), strict TypeScript, RxJS for state, PrimeNG for UI, Tailwind CSS for styling.

- **app.service.ts** — Main backend communication, event routing
- **tab.service.ts** — Tab state management
- **board/** — Visual workflow editor with drag-drop canvas
- **remotes/** — Remote storage management UI
- **settings/** — App settings (notifications, tray, security, start at login)
- **components/** — Shared: sidebar, sync-status, toast, confirm-dialog, path-browser

TypeScript bindings auto-generated from Go into `desktop/frontend/bindings/` (aliased as `wailsjs/`):
```typescript
import { Sync } from "../../wailsjs/desktop/backend/app";
import * as models from "../../wailsjs/desktop/backend/models/models";
```

### Configuration

All stored in `~/.config/ns-drive/`:
- `ns-drive.db` — SQLite database (encrypted when auth enabled)
- `rclone.conf` — rclone remotes (encrypted when auth enabled)
- `auth.json` — Auth metadata and pre-unlock app settings (always plaintext)

### Platform-Specific Code

- `platform_darwin.go` / `notification_darwin.go` — macOS tray, dock show/hide, native notifications (UNUserNotificationCenter)
- `platform_other.go` / `notification_other.go` — Linux/Windows stubs, beeep fallback for notifications

## Development Notes

- Frontend uses **Bun** as package manager
- Dev server port configurable via `WAILS_VITE_PORT` env var (default: 9245)
- `NS_DRIVE_DEBUG=true` enables debug mode in dev
- Services use `SetApp()` for EventBus access, wired in main.go after creation
- Shared SQLite via `GetSharedDB()` singleton pattern with mutex protection
- Tests clean DB tables for isolation (see `*_test.go` files for patterns)

## Git Conventions

- Commit format: `feat|fix|docs|refactor|test|chore: description`
