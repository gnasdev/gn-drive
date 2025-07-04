---
type: "always_apply"
---

Có thể dùng context7 mcp để tham khảo về rclone và wails v3 nếu cần.

Wails v3 document: https://v3alpha.wails.io

Dự án sử dụng rclone 1.70.1, wails v3, Angular 20, Golang 1.24.

# NS-Drive Architecture Documentation

## Overview

NS-Drive has been redesigned with a modern, domain-separated service architecture following Wails v3 best practices. This document outlines the new architecture and communication patterns.

## Architecture Principles

### 1. Domain Separation

Services are organized by business domain:

- **SyncService**: Handles all sync operations (pull, push, bi-directional)
- **ConfigService**: Manages profiles and application configuration
- **RemoteService**: Handles remote storage operations
- **TabService**: Manages tab state and operations

### 2. Event-Driven Communication

- Structured event types with consistent naming conventions
- Real-time updates via Wails v3 event system
- Type-safe event payloads

### 3. Context-Aware Operations

- All service methods accept `context.Context` as first parameter
- Support for operation cancellation
- Window-aware operations

## Service Architecture

### SyncService (`desktop/backend/services/sync_service.go`)

**Responsibilities:**

- Execute sync operations (pull, push, bi-directional)
- Manage active sync tasks
- Handle operation cancellation
- Emit sync progress events

**Key Methods:**

```go
StartSync(ctx context.Context, action string, profile models.Profile, tabId string) (*SyncResult, error)
StopSync(ctx context.Context, taskId int) error
GetActiveTasks(ctx context.Context) (map[int]*SyncTask, error)
```

**Events Emitted:**

- `sync:started` - Sync operation initiated
- `sync:progress` - Progress updates during sync
- `sync:completed` - Sync completed successfully
- `sync:failed` - Sync operation failed
- `sync:cancelled` - Sync operation cancelled

### ConfigService (`desktop/backend/services/config_service.go`)

**Responsibilities:**

- Manage application configuration
- CRUD operations for profiles
- Import/export functionality
- Configuration validation

**Key Methods:**

```go
GetProfiles(ctx context.Context) ([]models.Profile, error)
AddProfile(ctx context.Context, profile models.Profile) error
UpdateProfile(ctx context.Context, profile models.Profile) error
DeleteProfile(ctx context.Context, profileName string) error
```

**Events Emitted:**

- `config:updated` - Configuration changed
- `profile:added` - New profile created
- `profile:updated` - Profile modified
- `profile:deleted` - Profile removed

### RemoteService (`desktop/backend/services/remote_service.go`)

**Responsibilities:**

- Manage remote storage configurations
- Test remote connections
- Import/export remote configurations

**Key Methods:**

```go
GetRemotes(ctx context.Context) ([]RemoteInfo, error)
AddRemote(ctx context.Context, name, remoteType string, config map[string]string) error
UpdateRemote(ctx context.Context, name string, config map[string]string) error
DeleteRemote(ctx context.Context, name string) error
```

**Events Emitted:**

- `remote:added` - New remote added
- `remote:updated` - Remote configuration updated
- `remote:deleted` - Remote removed

### TabService (`desktop/backend/services/tab_service.go`)

**Responsibilities:**

- Manage tab lifecycle
- Track tab state and operations
- Handle tab output and errors

**Key Methods:**

```go
CreateTab(ctx context.Context, name string) (*Tab, error)
UpdateTab(ctx context.Context, tabId string, updates map[string]interface{}) error
DeleteTab(ctx context.Context, tabId string) error
AddTabOutput(ctx context.Context, tabId string, output string) error
```

**Events Emitted:**

- `tab:created` - New tab created
- `tab:updated` - Tab state changed
- `tab:deleted` - Tab removed
- `tab:output` - New output added to tab

## Event System

### Event Types (`desktop/backend/events/types.go`)

All events follow a structured format with consistent naming:

```go
type BaseEvent struct {
    Type      EventType   `json:"type"`
    Timestamp time.Time   `json:"timestamp"`
    Data      interface{} `json:"data"`
}
```

### Event Naming Convention

- Format: `domain:action`
- Examples: `sync:started`, `config:updated`, `tab:created`

### Event Categories

1. **Sync Events**: `sync:*`
2. **Config Events**: `config:*`, `profile:*`
3. **Remote Events**: `remote:*`
4. **Tab Events**: `tab:*`
5. **Error Events**: `error:occurred`

## Frontend Integration

### Generated Bindings

Services are automatically exposed to the frontend via Wails v3 bindings:

```typescript
// Example usage
import { SyncService, ConfigService } from "../../wailsjs/desktop/backend/app";

// Start a sync operation
const result = await SyncService.StartSync("pull", profile, tabId);

// Get all profiles
const profiles = await ConfigService.GetProfiles();
```

### Event Handling

Frontend can listen to backend events:

```typescript
import { Events } from "@wailsio/runtime";

// Listen for sync events
Events.On("sync:progress", (event) => {
  console.log("Sync progress:", event.data);
});

// Listen for config changes
Events.On("config:updated", (event) => {
  // Refresh UI
});
```

## Error Handling

### Structured Errors

All services use structured error handling:

```go
type ErrorEvent struct {
    BaseEvent
    Code    string `json:"code"`
    Message string `json:"message"`
    Details string `json:"details,omitempty"`
    TabId   string `json:"tabId,omitempty"`
}
```

### Error Categories

- `SYNC_ERROR`: Sync operation failures
- `CONFIG_ERROR`: Configuration issues
- `REMOTE_ERROR`: Remote storage problems
- `TAB_ERROR`: Tab operation failures

## Migration from Legacy Architecture

### Key Changes

1. **Service Separation**: Monolithic app service split into domain services
2. **Event Structure**: Consistent event naming and payload structure
3. **Context Support**: All operations support cancellation
4. **Type Safety**: Structured events and error types

### Backward Compatibility

- Legacy app service remains for compatibility
- Gradual migration path for frontend components
- Existing event handlers continue to work

## Future Improvements

### Phase 3: Frontend Refactoring

- Update frontend services to use new backend services
- Implement proper cancellation handling
- Simplify state management

### Phase 4: Error Handling & Testing

- Implement comprehensive error middleware
- Add unit tests for all services
- Integration testing for event flows

## Development Guidelines

### Adding New Services

1. Create service in `desktop/backend/services/`
2. Implement Wails service interface
3. Define event types in `events/types.go`
4. Register service in `main.go`
5. Generate bindings with `wails3 generate bindings`

### Event Best Practices

1. Use consistent naming: `domain:action`
2. Include timestamp in all events
3. Provide structured data payloads
4. Emit events for all state changes

### Error Handling

1. Use structured error types
2. Provide user-friendly error messages
3. Include context information (tab ID, operation type)
4. Log errors for debugging

## Conclusion

The new architecture provides:

- **Better Maintainability**: Clear separation of concerns
- **Improved Performance**: Context-aware cancellation
- **Enhanced User Experience**: Real-time updates and better error handling
- **Type Safety**: Structured events and data models
- **Future-Proof**: Follows Wails v3 best practices
