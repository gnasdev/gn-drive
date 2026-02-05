# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

NS-Drive is a desktop application for cloud storage synchronization powered by rclone. Built with Go 1.25 + Wails v3 backend and Angular 21 + Tailwind CSS frontend.

## Common Commands

```bash
# Development (requires 2 terminals)
task dev:fe      # Terminal 1: Start Angular dev server (wait for localhost:9245)
task dev:be      # Terminal 2: Start Wails backend (after frontend ready)

# Production build
task build       # Creates ns-drive binary in project root
task build:macos # Creates signed macOS .app bundle

# Linting
task lint        # Lint both frontend and backend
task lint:fe     # ESLint on frontend only
task lint:be     # golangci-lint on backend only

# Testing
cd desktop && go test ./...                    # Run all Go tests
cd desktop/frontend && bun run test            # Run Angular tests (Karma/Jasmine)

# Regenerate TypeScript bindings after modifying Go services/models
cd desktop && wails3 generate bindings
```

## Architecture

### Backend Services (desktop/backend/services/)

12 domain-separated services, all registered in [main.go](desktop/main.go):

#### Core Services
- **SyncService** - Executes rclone sync operations (pull, push, bi, bi-resync), manages active tasks with context cancellation
- **ConfigService** - CRUD operations for profiles, loads/saves to `~/.config/ns-drive/profiles.json`
- **RemoteService** - Manages rclone remote configurations at `~/.config/ns-drive/rclone.conf`
- **TabService** - Manages tab lifecycle, state, and output logging

#### Scheduling & History
- **SchedulerService** - Cron-based schedule management for automated syncs
- **HistoryService** - Tracks sync operation history with statistics

#### Workflow & Operations
- **BoardService** - Visual workflow orchestration with DAG execution (nodes represent remotes, edges represent sync operations)
- **OperationService** - File operations (copy, move, check, dedupe, list, delete, mkdir, about, size)
- **CryptService** - Encrypted remote creation and management

#### System Integration
- **TrayService** - System tray integration with quick board execution
- **NotificationService** - Desktop notifications and app settings (minimize to tray, start at login)
- **LogService** - Reliable log delivery with sequencing for tabs

Legacy **App** service ([app.go](desktop/backend/app.go)) contains additional methods exposed to frontend.

#### Import/Export
- **ExportService** - Configuration backup (profiles, remotes, boards) with optional token inclusion
- **ImportService** - Restore from exports with merge/replace modes

### Data Models (desktop/backend/models/)

- **Profile** - Sync profile configuration (name, from, to, included/excluded paths, bandwidth, parallel)
- **ScheduleEntry** - Scheduled task (id, profile, action, cron expression, enabled, last/next run)
- **Board/BoardNode/BoardEdge** - Workflow definitions with canvas coordinates and execution config
- **HistoryEntry** - Sync operation record (timestamp, profile, action, status, bytes transferred)
- **FileEntry** - File/directory info for browsing remotes
- **Remote** - Rclone remote configuration

### Event-Driven Communication

Backend emits structured events to frontend via Wails event system. Event format: `domain:action` (e.g., `sync:started`, `profile:updated`, `tab:output`).

Event categories:
- **Sync**: `sync:started`, `sync:progress`, `sync:completed`, `sync:error`
- **Config**: `config:updated`, `profile:added`, `profile:updated`, `profile:deleted`
- **Remote**: `remote:added`, `remote:updated`, `remote:deleted`, `remote:tested`
- **Tab**: `tab:created`, `tab:updated`, `tab:deleted`, `tab:output`, `tab:state_changed`
- **Board**: `board:created`, `board:updated`, `board:deleted`, `board:execution_status`
- **Log**: `log:message`, `sync:event` (with sequence numbers)

Frontend listens with:
```typescript
import { Events } from "@wailsio/runtime";
Events.On("sync:progress", (event) => { ... });
```

### Frontend Structure (desktop/frontend/src/app/)

- **app.service.ts** - Main service interacting with backend bindings
- **tab.service.ts** - Tab state management
- **board/** - Visual workflow editor with drag-drop canvas
- **remotes/** - Remote storage management UI
- **settings/** - App settings (notifications, tray, start at login)
- **components/** - Shared components (sidebar, sync-status, toast, confirm-dialog, path-browser)
- **services/** - Frontend services (log-consumer, logging, error, theme, navigation)

### Bindings

TypeScript bindings are generated from Go services into `desktop/frontend/bindings/` (aliased as `wailsjs/`). Import pattern:
```typescript
import { Sync, GetConfigInfo } from "../../wailsjs/desktop/backend/app";
import * as models from "../../wailsjs/desktop/backend/models/models";
```

## Key Files

- [desktop/main.go](desktop/main.go) - Application entry, service registration (12 services)
- [desktop/backend/commands.go](desktop/backend/commands.go) - rclone command building logic
- [desktop/backend/rclone/](desktop/backend/rclone/) - rclone operations (sync, bisync, common, operations)
- [desktop/build/config.yml](desktop/build/config.yml) - Wails dev mode configuration
- [Taskfile.yml](Taskfile.yml) - Build task definitions

## Configuration Locations

- `~/.config/ns-drive/profiles.json` - Sync profiles
- `~/.config/ns-drive/rclone.conf` - Rclone remotes
- `~/.config/ns-drive/schedules.json` - Scheduled tasks
- `~/.config/ns-drive/boards.json` - Workflow boards
- `~/.config/ns-drive/history.json` - Operation history
- `~/.config/ns-drive/app_settings.json` - App settings

## Development Notes

- Frontend uses Bun as package manager (`bun install` in `desktop/frontend/`)
- Dev server port is configurable via `WAILS_VITE_PORT` env var (default: 9245)
- All Go service methods accept `context.Context` as first parameter for cancellation support
- Board execution uses topological sort with cycle detection for DAG processing
- Log service uses sequence numbers for reliable delivery to frontend
