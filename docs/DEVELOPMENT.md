# Development Guide

This guide covers setting up your development environment and common development tasks for NG Drive.

## Prerequisites

### Required Tools

| Tool | Version | Installation |
|------|---------|--------------|
| Go | 1.25+ | https://golang.org/dl/ |
| Node.js | 18+ (v24 recommended) | https://nodejs.org/ |
| npm | Latest | Comes with Node.js |
| Taskfile | Latest | https://taskfile.dev/ |
| Wails v3 | Latest | `go install github.com/wailsapp/wails/v3/cmd/wails3@latest` |

### Verify Installation

```bash
# Check Go
go version  # Should show 1.25+

# Check Node.js
node --version  # Should show v18+

# Check Wails v3
wails3 version

# Ensure GOPATH/bin is in PATH
export PATH="$PATH:$(go env GOPATH)/bin"
```

## Initial Setup

```bash
# Clone repository
git clone <repository-url>
cd ng-drive

# Install Go dependencies
cd desktop && go mod tidy

# Install frontend dependencies
cd frontend && npm install --legacy-peer-deps
cd ../..
```

## Development Mode

Development requires running two separate processes.

### Terminal 1: Frontend Dev Server

```bash
task dev:fe
```

Wait for:
```
✔ Building...
Application bundle generation complete.
  ➜  Local:   http://localhost:9245/
```

### Terminal 2: Wails Backend

```bash
task dev:be
```

The app window opens automatically when ready.

### Hot Reload

- **Frontend changes**: Auto-reload by Angular dev server
- **Backend changes**: Wails watches `*.go` files and auto-rebuilds

## Build Commands

| Command | Description |
|---------|-------------|
| `task build` | Production build |
| `task dev:fe` | Start frontend dev server |
| `task dev:be` | Start Wails backend (requires frontend) |
| `task lint` | Lint both frontend and backend |
| `task lint:fe` | ESLint on frontend |
| `task lint:be` | golangci-lint on backend |

## Project Structure

```
ng-drive/
├── desktop/
│   ├── main.go                 # Application entry point (12 services)
│   ├── backend/
│   │   ├── app.go             # Legacy App service
│   │   ├── commands.go        # Sync command building
│   │   ├── services/          # Domain services
│   │   │   ├── sync_service.go      # Sync operations
│   │   │   ├── config_service.go    # Profile management
│   │   │   ├── remote_service.go    # Remote management
│   │   │   ├── tab_service.go       # Tab lifecycle
│   │   │   ├── scheduler_service.go # Cron scheduling
│   │   │   ├── history_service.go   # Operation history
│   │   │   ├── board_service.go     # Workflow boards
│   │   │   ├── operation_service.go # File operations
│   │   │   ├── crypt_service.go     # Encryption
│   │   │   ├── tray_service.go      # System tray
│   │   │   ├── notification_service.go # Notifications
│   │   │   ├── log_service.go       # Reliable logging
│   │   │   ├── export_service.go    # Config export
│   │   │   └── import_service.go    # Config import
│   │   ├── models/            # Data structures
│   │   ├── rclone/            # rclone operations
│   │   ├── events/            # Event system
│   │   ├── errors/            # Error handling
│   │   ├── config/            # Configuration
│   │   ├── validation/        # Input validation
│   │   ├── dto/               # Data transfer objects
│   │   └── utils/             # Utilities
│   ├── frontend/
│   │   ├── src/app/           # Angular components
│   │   │   ├── board/         # Visual workflow editor
│   │   │   ├── remotes/       # Remote management
│   │   │   ├── settings/      # App settings
│   │   │   ├── components/    # Shared components
│   │   │   └── services/      # Frontend services
│   │   ├── bindings/          # Generated TypeScript bindings
│   │   └── dist/              # Built assets
│   └── build/
│       └── config.yml         # Wails configuration
├── docs/                       # Documentation
├── Taskfile.yml               # Build tasks
└── README.md
```

## Generating TypeScript Bindings

After modifying Go services or models:

```bash
cd desktop
wails3 generate bindings
```

Bindings are generated to `desktop/frontend/bindings/`.

## Testing

### Go Tests

```bash
cd desktop
go test ./...

# With verbose output
go test -v ./...

# Specific package
go test ./backend/services/...
```

### Frontend Tests

```bash
cd desktop/frontend
npm test
```

## Debugging

### Backend Logs

Backend logs appear in the terminal running `task dev:be`:

```
NOTICE: SyncService starting up...
NOTICE: ConfigService starting up...
DEBUG: GetRemotes - found 3 remotes
```

### Frontend Console

Open browser DevTools (if using external browser) or check Wails dev console.

### Debug Commands

```bash
# Check if frontend is running
curl http://localhost:9245

# Verify Go modules
cd desktop && go mod verify

# Clean rebuild
cd desktop/frontend && rm -rf node_modules dist && npm install --legacy-peer-deps
cd desktop && go clean -cache
```

## Configuration

### Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `WAILS_VITE_PORT` | 9245 | Frontend dev server port |
| `DEBUG_MODE` | false | Enable debug logging |

### Configuration Files

| File | Location | Description |
|------|----------|-------------|
| `profiles.json` | `~/.config/ng-drive/` | Sync profiles |
| `rclone.conf` | `~/.config/ng-drive/` | Remote configurations |
| `schedules.json` | `~/.config/ng-drive/` | Scheduled tasks |
| `boards.json` | `~/.config/ng-drive/` | Workflow boards |
| `history.json` | `~/.config/ng-drive/` | Operation history |
| `app_settings.json` | `~/.config/ng-drive/` | App settings |
| `config.yml` | `desktop/build/` | Wails dev configuration |

## Common Issues

### Port 9245 in Use

```bash
# Find process
lsof -i :9245

# Kill it
kill -9 <PID>

# Or use different port
WAILS_VITE_PORT=9246 task dev:fe
```

### Wails3 Not Found

```bash
# Install Wails v3
go install github.com/wailsapp/wails/v3/cmd/wails3@latest

# Update PATH
export PATH="$PATH:$(go env GOPATH)/bin"
```

### npm Dependency Errors

```bash
# Use legacy peer deps flag
cd desktop/frontend && npm install --legacy-peer-deps
```

### Changes Not Reflecting

1. Frontend: Should auto-reload. Refresh if stuck.
2. Backend: Wails auto-rebuilds. Restart if stuck.
3. Clean rebuild if persistent issues.

## Code Style

### Go

- Run `task lint:be` before committing
- Follow standard Go conventions
- Use context for cancellation
- Handle errors explicitly

### TypeScript

- Run `task lint:fe` before committing
- Use TypeScript strict mode
- Prefer RxJS observables for state
- Document public interfaces

## Adding New Features

### New Backend Service

1. Create service in `desktop/backend/services/`
2. Implement `SetApp()` method for EventBus access:
   ```go
   func (s *MyService) SetApp(app *application.App) {
       s.app = app
       s.eventBus = events.NewEventBus(app)
   }
   ```
3. Register in `desktop/main.go`
4. Generate bindings: `wails3 generate bindings`
5. Add tests

### New Frontend Component

1. Generate with Angular CLI: `ng generate component components/my-component`
2. Add to appropriate module
3. Follow existing patterns for state management
4. Add tests

### New Event Type

1. Add to `desktop/backend/events/types.go`
2. Create constructor in same file
3. Add TypeScript type in `desktop/frontend/src/app/models/events.ts`
4. Add type guard function
5. Handle in AppService or TabService

### New Model

1. Create model in `desktop/backend/models/`
2. Add JSON tags for serialization
3. Generate bindings: `wails3 generate bindings`
4. TypeScript type will be auto-generated in bindings
