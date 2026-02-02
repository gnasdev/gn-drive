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

# Linting
task lint        # Lint both frontend and backend
task lint:fe     # ESLint on frontend only
task lint:be     # golangci-lint on backend only

# Testing
cd desktop && go test ./...                    # Run all Go tests
cd desktop/frontend && npm test                # Run Angular tests (Karma/Jasmine)

# Regenerate TypeScript bindings after modifying Go services/models
cd desktop && wails3 generate bindings
```

## Architecture

### Backend Services (desktop/backend/services/)

Four domain-separated services, all registered in [main.go](desktop/main.go):

- **SyncService** - Executes rclone sync operations (pull, push, bi, bi-resync), manages active tasks with context cancellation
- **ConfigService** - CRUD operations for profiles, loads/saves to `~/.config/ns-drive/profiles.json`
- **RemoteService** - Manages rclone remote configurations at `~/.config/ns-drive/rclone.conf`
- **TabService** - Manages tab lifecycle, state, and output logging

Legacy **App** service ([app.go](desktop/backend/app.go)) contains additional methods exposed to frontend.

### Event-Driven Communication

Backend emits structured events to frontend via Wails event system. Event format: `domain:action` (e.g., `sync:started`, `profile:updated`, `tab:output`).

Frontend listens with:
```typescript
import { Events } from "@wailsio/runtime";
Events.On("sync:progress", (event) => { ... });
```

### Frontend Structure (desktop/frontend/src/app/)

- **app.service.ts** - Main service interacting with backend bindings
- **tab.service.ts** - Tab state management
- **home/** - Dashboard with multi-tab sync operations
- **profiles/** - Profile CRUD UI
- **remotes/** - Remote storage management UI
- **wailsjs/** - Symlink to generated bindings in `bindings/`

### Bindings

TypeScript bindings are generated from Go services into `desktop/frontend/bindings/` (aliased as `wailsjs/`). Import pattern:
```typescript
import { Sync, GetConfigInfo } from "../../wailsjs/desktop/backend/app";
import * as models from "../../wailsjs/desktop/backend/models/models";
```

## Key Files

- [desktop/main.go](desktop/main.go) - Application entry, service registration
- [desktop/backend/commands.go](desktop/backend/commands.go) - rclone command building logic
- [desktop/build/config.yml](desktop/build/config.yml) - Wails dev mode configuration
- [Taskfile.yml](Taskfile.yml) - Build task definitions

## Development Notes

- Frontend dependencies require `npm install --legacy-peer-deps` due to peer dependency conflicts
- Dev server port is configurable via `WAILS_VITE_PORT` env var (default: 9245)
- All Go service methods accept `context.Context` as first parameter for cancellation support
