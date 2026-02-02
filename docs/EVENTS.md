# Event System Documentation

NS-Drive uses an event-driven architecture for backend-to-frontend communication. All events are emitted through a unified channel and handled by the frontend.

## Overview

```
Backend Service → EventBus.Emit() → JSON Serialize → "tofe" Channel → Frontend Event Handler
```

All events flow through the Wails `"tofe"` (to-frontend) event channel as JSON-serialized data.

## Event Types

### Sync Events

| Event Type | Description | Fields |
|------------|-------------|--------|
| `sync:started` | Sync operation started | tabId, action, status, message |
| `sync:progress` | Sync progress update | tabId, action, progress, status, message |
| `sync:completed` | Sync completed successfully | tabId, action, status, message |
| `sync:failed` | Sync failed with error | tabId, action, status, message |
| `sync:cancelled` | Sync was cancelled | tabId, action, status, message |

**Example Payload:**
```json
{
    "type": "sync:started",
    "timestamp": "2024-01-15T10:30:00Z",
    "tabId": "tab_abc123_1705312200000",
    "action": "pull",
    "status": "starting",
    "message": "Sync operation started"
}
```

### Config Events

| Event Type | Description | Fields |
|------------|-------------|--------|
| `config:updated` | Configuration changed | profileId, data |
| `profile:added` | New profile added | profileId, data |
| `profile:updated` | Profile modified | profileId, data |
| `profile:deleted` | Profile removed | profileId, data |

**Example Payload:**
```json
{
    "type": "profile:added",
    "timestamp": "2024-01-15T10:30:00Z",
    "profileId": "my-backup",
    "data": {
        "name": "my-backup",
        "from": "/local/path",
        "to": "gdrive:backup"
    }
}
```

### Remote Events

| Event Type | Description | Fields |
|------------|-------------|--------|
| `remote:added` | New remote added | remoteName, data |
| `remote:updated` | Remote modified | remoteName, data |
| `remote:deleted` | Remote removed | remoteName, data |
| `remotes:list` | Remote list updated | data |

### Tab Events

| Event Type | Description | Fields |
|------------|-------------|--------|
| `tab:created` | New tab created | tabId, tabName, data |
| `tab:updated` | Tab state changed | tabId, tabName, data |
| `tab:deleted` | Tab removed | tabId, tabName |
| `tab:output` | Output added to tab | tabId, data |

### Error Events

| Event Type | Description | Fields |
|------------|-------------|--------|
| `error:occurred` | Error occurred | code, message, details, tabId |

**Example Payload:**
```json
{
    "type": "error:occurred",
    "timestamp": "2024-01-15T10:30:00Z",
    "code": "SYNC_ERROR",
    "message": "Failed to connect to remote",
    "details": "network timeout after 30s",
    "tabId": "tab_abc123_1705312200000"
}
```

## Legacy Command DTOs

For backward compatibility, legacy command events are also supported:

| Command | Description |
|---------|-------------|
| `command_started` | Command execution started |
| `command_stoped` | Command execution stopped (note: typo preserved) |
| `command_output` | Command output data |
| `error` | Error occurred |
| `sync_status` | Real-time sync status update |

**Legacy Format:**
```json
{
    "command": "sync_status",
    "pid": 12345,
    "task": "pull",
    "error": null,
    "tab_id": "tab_abc123_1705312200000"
}
```

## Backend Usage

### Emitting Events

Use the EventBus for all event emission:

```go
// In service
func (s *SyncService) emitSyncEvent(eventType events.EventType, tabId, action, status, message string) {
    event := events.NewSyncEvent(eventType, tabId, action, status, message)
    if s.eventBus != nil {
        s.eventBus.EmitSyncEvent(event)
    }
}

// Usage
s.emitSyncEvent(events.SyncStarted, tabId, "pull", "starting", "Sync operation started")
```

### Event Constructors

```go
// events/types.go
func NewSyncEvent(eventType EventType, tabId, action, status, message string) *SyncEvent
func NewConfigEvent(eventType EventType, profileId string, data interface{}) *ConfigEvent
func NewRemoteEvent(eventType EventType, remoteName string, data interface{}) *RemoteEvent
func NewTabEvent(eventType EventType, tabId, tabName string, data interface{}) *TabEvent
func NewErrorEvent(code, message, details, tabId string) *ErrorEvent
```

### EventBus Interface

```go
// events/bus.go
type EventBus interface {
    Emit(event interface{}) error
    EmitWithType(eventType EventType, data interface{}) error
}

// Convenience methods
func (b *WailsEventBus) EmitSyncEvent(event *SyncEvent) error
func (b *WailsEventBus) EmitConfigEvent(event *ConfigEvent) error
func (b *WailsEventBus) EmitRemoteEvent(event *RemoteEvent) error
func (b *WailsEventBus) EmitTabEvent(event *TabEvent) error
func (b *WailsEventBus) EmitErrorEvent(event *ErrorEvent) error
```

## Frontend Usage

### Listening to Events

```typescript
// app.service.ts
import { Events } from "@wailsio/runtime";
import { parseEvent, isSyncEvent, isConfigEvent } from "./models/events";

// Set up listener
this.eventCleanup = Events.On("tofe", (event) => {
    const parsedEvent = parseEvent(event.data);

    if (isSyncEvent(parsedEvent)) {
        this.handleSyncEvent(parsedEvent);
    } else if (isConfigEvent(parsedEvent)) {
        this.handleConfigEvent(parsedEvent);
    }
    // ... handle other event types
});

// Cleanup on destroy
ngOnDestroy() {
    if (this.eventCleanup) {
        this.eventCleanup();
    }
}
```

### Type Guards

```typescript
// models/events.ts
export function isSyncEvent(event: unknown): event is SyncEvent {
    const e = event as Record<string, unknown>;
    return typeof e["type"] === "string" && e["type"].startsWith("sync:");
}

export function isConfigEvent(event: unknown): event is ConfigEvent {
    const e = event as Record<string, unknown>;
    const type = e["type"] as string;
    return type?.startsWith("config:") || type?.startsWith("profile:");
}

export function isErrorEvent(event: unknown): event is ErrorEvent {
    const e = event as Record<string, unknown>;
    return e["type"] === "error:occurred";
}

export function isLegacyCommandDTO(event: unknown): event is CommandDTO {
    const e = event as Record<string, unknown>;
    return typeof e["command"] === "string" && !("type" in e);
}
```

### TypeScript Event Types

```typescript
// models/events.ts
export interface SyncEvent {
    type: "sync:started" | "sync:progress" | "sync:completed" | "sync:failed" | "sync:cancelled";
    timestamp: string;
    tabId?: string;
    action: string;
    progress?: number;
    status: string;
    message?: string;
}

export interface ConfigEvent {
    type: "config:updated" | "profile:added" | "profile:updated" | "profile:deleted";
    timestamp: string;
    profileId?: string;
    data?: unknown;
}

export interface ErrorEvent {
    type: "error:occurred";
    timestamp: string;
    code: string;
    message: string;
    details?: string;
    tabId?: string;
}
```

## Event Routing

Events are routed based on the presence of `tabId`:

1. **Events with `tabId`**: Routed to `TabService.handleTypedSyncEvent()` or `TabService.handleCommandEvent()`
2. **Events without `tabId`**: Handled globally in `AppService`

```typescript
// app.service.ts
private handleSyncEvent(event: SyncEvent) {
    if (event.tabId) {
        this.tabService.handleTypedSyncEvent(event);
        return;
    }
    // Handle global sync events
    // ...
}
```

## Best Practices

### Backend

1. Always use EventBus for event emission
2. Include tabId when event is tab-specific
3. Use appropriate event type for the action
4. Include meaningful messages for user display

### Frontend

1. Always clean up event listeners in ngOnDestroy
2. Use type guards before handling events
3. Handle unknown event types gracefully
4. Don't assume field presence - use optional chaining

### Error Handling

1. Emit error events for user-visible errors
2. Include error code for programmatic handling
3. Include tabId to route errors to correct tab
4. Provide helpful error messages
