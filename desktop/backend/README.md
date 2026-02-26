# GN Drive Backend

Go backend for GN Drive, built with Wails v3 and rclone integration.

## Structure

```
backend/
├── app.go              # Main App service (legacy)
├── commands.go         # Sync command building and execution
├── services/           # Domain services
│   ├── sync_service.go     # Sync operations
│   ├── config_service.go   # Profile management
│   ├── remote_service.go   # Remote configuration
│   └── tab_service.go      # Tab lifecycle
├── models/             # Data structures
│   ├── profile.go          # Profile model
│   └── config_info.go      # Configuration model
├── events/             # Event system
│   ├── types.go            # Event type definitions
│   └── bus.go              # EventBus implementation
├── errors/             # Error handling
│   └── types.go            # Error types and codes
├── validation/         # Input validation
│   └── profile_validator.go # Profile validation
├── utils/              # Utilities
│   └── sync_status.go      # Sync progress reporting
├── dto/                # Data transfer objects
├── config/             # Configuration
└── rclone/             # rclone integration
```

## Services

### SyncService

Handles synchronization operations:
- Pull (remote → local)
- Push (local → remote)
- Bi-sync (bidirectional)
- Bi-resync (force bidirectional)

Features:
- Context-based cancellation
- Concurrent task tracking
- Real-time progress via EventBus

### ConfigService

Manages sync profiles:
- CRUD operations
- Profile validation
- File persistence (`~/.config/gn-drive/profiles.json`)
- Rollback on save failure

### RemoteService

Manages rclone remotes:
- List configured remotes
- Add/delete remotes
- OAuth flow support

### TabService

Manages UI tab state:
- Tab creation/deletion
- Output buffering
- State management

## Event System

All services emit events through unified EventBus:

```go
// Emit sync event
event := events.NewSyncEvent(events.SyncStarted, tabId, "pull", "starting", "Started")
s.eventBus.EmitSyncEvent(event)
```

Event types:
- `sync:started`, `sync:progress`, `sync:completed`, `sync:failed`, `sync:cancelled`
- `config:updated`, `profile:added`, `profile:updated`, `profile:deleted`
- `remote:added`, `remote:updated`, `remote:deleted`
- `error:occurred`

## Validation

Input validation via `validation/profile_validator.go`:

```go
validator := validation.NewProfileValidator()
if err := validator.ValidateProfile(profile); err != nil {
    return err
}
```

Validates:
- Profile name (required, unique, safe characters)
- Paths (format, traversal prevention)
- Remote names (alphanumeric, dash, underscore)
- Parallel/bandwidth ranges

## Error Handling

Structured errors via `errors/` package:

```go
return &errors.AppError{
    Code:    errors.ValidationError,
    Message: "Invalid profile name",
    TraceId: errors.GenerateTraceId(),
}
```

## Configuration

Files stored in `~/.config/gn-drive/`:
- `profiles.json` - Sync profiles (0600 permissions)
- `rclone.conf` - Remote configurations (0600 permissions)

## Testing

```bash
go test ./...
go test -v ./services/...
go test -cover ./...
```

## Adding New Services

1. Create service file in `services/`
2. Implement `SetApp()` for EventBus access:
   ```go
   func (s *MyService) SetApp(app *application.App) {
       s.app = app
       s.eventBus = events.NewEventBus(app)
   }
   ```
3. Register in `main.go`
4. Add tests

## Dependencies

- `github.com/wailsapp/wails/v3` - Desktop framework
- `github.com/rclone/rclone` - Cloud sync library
