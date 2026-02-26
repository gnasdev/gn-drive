# Event System Documentation

GN Drive uses an event-driven architecture for backend-to-frontend communication. All events are emitted through a unified channel and handled by the frontend.

## Overview

```
Backend Service → EventBus.Emit() → JSON Serialize → "tofe" Channel → Frontend Event Handler
```

All events flow through the Wails `"tofe"` (to-frontend) event channel as JSON-serialized data.

## Event Types

### Auth Events

| Event Type | Description | Fields |
|------------|-------------|--------|
| `auth:unlocked` | App unlocked (password verified or no auth) | type, timestamp |
| `auth:locked` | App locked (user action or shutdown) | type, timestamp |

Auth events are emitted through the EventBus `tofe` channel. Frontend auth service also listens directly on `auth:unlocked` and `auth:locked` as backup, but primary state management is via direct method call responses.

```typescript
// Primary: state updated after method call
await Unlock(password);
this.isLocked$.next(false);

// Backup: event listener
Events.On("auth:unlocked", () => { this.isLocked$.next(false); });
Events.On("auth:locked", () => { this.isLocked$.next(true); });
```

---

### Sync Events

| Event Type | Description | Fields |
|------------|-------------|--------|
| `sync:started` | Sync operation started | tabId, action, status, message |
| `sync:progress` | Sync progress update | tabId, action, progress, status, message, seqNo |
| `sync:completed` | Sync completed successfully | tabId, action, status, message |
| `sync:failed` | Sync failed with error | tabId, action, status, message |
| `sync:cancelled` | Sync was cancelled | tabId, action, status, message |

**Progress Data:**
```go
type SyncProgressData struct {
    TransferredBytes int64  `json:"transferredBytes"`
    TotalBytes       int64  `json:"totalBytes"`
    TransferredFiles int    `json:"transferredFiles"`
    TotalFiles       int    `json:"totalFiles"`
    CurrentFile      string `json:"currentFile"`
    Speed            string `json:"speed"`
    ETA              string `json:"eta"`
    Errors           int    `json:"errors"`
    Checks           int    `json:"checks"`
    Deletes          int    `json:"deletes"`
    Renames          int    `json:"renames"`
    ElapsedTime      string `json:"elapsedTime"`
}
```

**Example Payload:**
```json
{
    "type": "sync:started",
    "timestamp": "2026-02-09T10:30:00Z",
    "tabId": "tab_abc123_1705312200000",
    "action": "pull",
    "status": "starting",
    "message": "Sync operation started"
}
```

---

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
    "timestamp": "2026-02-09T10:30:00Z",
    "profileId": "my-backup",
    "data": {
        "name": "my-backup",
        "from": "/local/path",
        "to": "gdrive:backup"
    }
}
```

---

### Remote Events

| Event Type | Description | Fields |
|------------|-------------|--------|
| `remote:added` | New remote added | remoteName, data |
| `remote:updated` | Remote modified | remoteName, data |
| `remote:deleted` | Remote removed | remoteName, data |
| `remote:tested` | Remote connection tested | remoteName, success, message |
| `remotes:list` | Remote list updated | data |

---

### Tab Events

| Event Type | Description | Fields |
|------------|-------------|--------|
| `tab:created` | New tab created | tabId, tabName, data |
| `tab:updated` | Tab state changed | tabId, tabName, data |
| `tab:deleted` | Tab removed | tabId, tabName |
| `tab:output` | Output added to tab | tabId, data |
| `tab:state_changed` | Tab state transition | tabId, oldState, newState |

---

### Board Events

| Event Type | Description | Fields |
|------------|-------------|--------|
| `board:created` | New board created | boardId, data |
| `board:updated` | Board modified | boardId, data |
| `board:deleted` | Board removed | boardId |
| `board:execution_status` | Execution status update | boardId, status, edgeId, message |

**Example Execution Status Payload:**
```json
{
    "type": "board:execution_status",
    "timestamp": "2026-02-09T10:30:00Z",
    "boardId": "board-123",
    "status": "running",
    "edgeId": "edge-1",
    "message": "Syncing edge-1"
}
```

---

### Operation Events

| Event Type | Description | Fields |
|------------|-------------|--------|
| `operation:started` | File operation started | tabId, operation, status, message |
| `operation:completed` | File operation completed | tabId, operation, status, message |
| `operation:failed` | File operation failed | tabId, operation, status, message |

---

### Schedule Events

| Event Type | Description | Fields |
|------------|-------------|--------|
| `schedule:added` | New schedule created | scheduleId, data |
| `schedule:updated` | Schedule modified | scheduleId, data |
| `schedule:deleted` | Schedule removed | scheduleId |
| `schedule:triggered` | Schedule executed | scheduleId, profileName, action |
| `schedule:completed` | Scheduled sync finished | scheduleId, result |

---

### History Events

| Event Type | Description | Fields |
|------------|-------------|--------|
| `history:added` | New history entry | entryId, data |
| `history:cleared` | History cleared | - |

---

### Crypt Events

| Event Type | Description | Fields |
|------------|-------------|--------|
| `crypt:created` | Crypt remote created | remoteName |
| `crypt:deleted` | Crypt remote deleted | remoteName |

---

### Log Events

| Event Type | Description | Fields |
|------------|-------------|--------|
| `log:message` | New log message | tabId, seqNo, message, level, timestamp |
| `sync:event` | Sync event log | tabId, seqNo, action, status, message |

**Example Log Payload:**
```json
{
    "type": "log:message",
    "timestamp": 1705312200000,
    "tabId": "tab_abc123",
    "seqNo": 42,
    "message": "Transferred: 100 MB / 1 GB",
    "level": "info"
}
```

**Log Levels:** `debug`, `info`, `warning`, `error`

---

### Error Events

| Event Type | Description | Fields |
|------------|-------------|--------|
| `error:occurred` | Error occurred | code, message, details, tabId |

**Example Payload:**
```json
{
    "type": "error:occurred",
    "timestamp": "2026-02-09T10:30:00Z",
    "code": "SYNC_ERROR",
    "message": "Failed to connect to remote",
    "details": "network timeout after 30s",
    "tabId": "tab_abc123_1705312200000"
}
```

---

### Notification Events

| Event Type | Description | Fields |
|------------|-------------|--------|
| `notification:sent` | Notification displayed | title, body |
| `settings:updated` | App settings changed | settings |

---

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

---

## Backend Usage

### Event Types (Go)

```go
// events/types.go
type EventType string

type BaseEvent struct {
    Type      EventType   `json:"type"`
    Timestamp time.Time   `json:"timestamp"`
    Data      interface{} `json:"data"`
}

type SyncEvent struct {
    BaseEvent
    TabId    string `json:"tabId,omitempty"`
    Action   string `json:"action"`
    Progress int    `json:"progress,omitempty"`
    Status   string `json:"status"`
    Message  string `json:"message,omitempty"`
    SeqNo    uint64 `json:"seqNo,omitempty"`
}

type ConfigEvent struct {
    BaseEvent
    ProfileId string `json:"profileId,omitempty"`
}

type RemoteEvent struct {
    BaseEvent
    RemoteName string `json:"remoteName,omitempty"`
}

type TabEvent struct {
    BaseEvent
    TabId   string `json:"tabId"`
    TabName string `json:"tabName,omitempty"`
}

type BoardEvent struct {
    BaseEvent
    BoardId string `json:"boardId"`
    EdgeId  string `json:"edgeId,omitempty"`
    Status  string `json:"status"`
    Message string `json:"message,omitempty"`
}

type OperationEvent struct {
    BaseEvent
    TabId     string `json:"tabId,omitempty"`
    Operation string `json:"operation"`
    Status    string `json:"status"`
    Message   string `json:"message,omitempty"`
}

type ScheduleEvent struct {
    BaseEvent
    ScheduleId string `json:"scheduleId"`
}

type HistoryEvent struct {
    BaseEvent
}

type CryptEvent struct {
    BaseEvent
    RemoteName string `json:"remoteName"`
}

type ErrorEvent struct {
    BaseEvent
    Code    string `json:"code"`
    Message string `json:"message"`
    Details string `json:"details,omitempty"`
    TabId   string `json:"tabId,omitempty"`
}
```

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

### EventBus Interface

```go
// events/bus.go
type EventBus interface {
    Emit(event interface{}) error
    EmitWithType(eventType EventType, data interface{}) error
}

// Convenience methods on WailsEventBus
func (b *WailsEventBus) EmitSyncEvent(event *SyncEvent) error
func (b *WailsEventBus) EmitConfigEvent(event *ConfigEvent) error
func (b *WailsEventBus) EmitRemoteEvent(event *RemoteEvent) error
func (b *WailsEventBus) EmitTabEvent(event *TabEvent) error
func (b *WailsEventBus) EmitBoardEvent(event *BoardEvent) error
func (b *WailsEventBus) EmitOperationEvent(event *OperationEvent) error
func (b *WailsEventBus) EmitScheduleEvent(event *ScheduleEvent) error
func (b *WailsEventBus) EmitHistoryEvent(event *HistoryEvent) error
func (b *WailsEventBus) EmitCryptEvent(event *CryptEvent) error
func (b *WailsEventBus) EmitErrorEvent(event *ErrorEvent) error
```

---

## Frontend Usage

### Listening to Events

```typescript
// app.service.ts
import { Events } from "@wailsio/runtime";
import { parseEvent, isSyncEvent, isConfigEvent, isErrorEvent, isLegacyCommandDTO } from "./models/events";

// Set up listener on "tofe" channel
this.eventCleanup = Events.On("tofe", (event) => {
    const parsedEvent = parseEvent(event.data);

    if (isSyncEvent(parsedEvent)) {
        this.handleSyncEvent(parsedEvent);
    } else if (isConfigEvent(parsedEvent)) {
        this.handleConfigEvent(parsedEvent);
    } else if (isErrorEvent(parsedEvent)) {
        this.handleErrorEvent(parsedEvent);
    } else if (isLegacyCommandDTO(parsedEvent)) {
        this.handleLegacyCommand(parsedEvent);
    }
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

export function isBoardEvent(event: unknown): event is BoardEvent {
    const e = event as Record<string, unknown>;
    return typeof e["type"] === "string" && e["type"].startsWith("board:");
}

export function isLogEvent(event: unknown): event is LogEvent {
    const e = event as Record<string, unknown>;
    return e["type"] === "log:message" || e["type"] === "sync:event";
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
    seqNo?: number;
}

export interface ConfigEvent {
    type: "config:updated" | "profile:added" | "profile:updated" | "profile:deleted";
    timestamp: string;
    profileId?: string;
    data?: unknown;
}

export interface BoardEvent {
    type: "board:created" | "board:updated" | "board:deleted" | "board:execution_status";
    timestamp: string;
    boardId: string;
    status?: string;
    edgeId?: string;
    message?: string;
    data?: unknown;
}

export interface LogEvent {
    type: "log:message" | "sync:event";
    timestamp: string;
    tabId: string;
    seqNo: number;
    message: string;
    level?: string;
    action?: string;
    status?: string;
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

---

## Event Routing

Events are routed based on the presence of `tabId`:

1. **Sync progress with `tabId`**: Routed to `LogConsumerService.handleLogEvent()` for deduplication
2. **Other sync events with `tabId`**: Routed to `TabService.handleTypedSyncEvent()`
3. **Error events with `tabId`**: Routed to the specific tab
4. **Events without `tabId`**: Handled globally in `AppService`
5. **Legacy command DTOs**: Handled by `AppService.handleLegacyCommand()`

```typescript
// app.service.ts
private handleSyncEvent(event: SyncEvent) {
    // Route progress events to LogConsumerService
    if (event.tabId && event.type === "sync:progress") {
        this.logConsumerService.handleLogEvent(event);
        return;
    }
    // Route other tab events to TabService
    if (event.tabId) {
        this.tabService.handleTypedSyncEvent(event);
        return;
    }
    // Handle global sync events
    // ...
}
```

---

## Log Event Sequencing

The LogService uses sequence numbers (uint64) for reliable log delivery:

```typescript
// log-consumer.service.ts
private lastSeqNo = 0;

async pollLogs(tabId: string) {
    const logs = await LogService.GetLogsSince(tabId, this.lastSeqNo);
    for (const log of logs) {
        this.lastSeqNo = log.seqNo;
        this.processLog(log);
    }
}
```

This ensures:
- No logs are missed even if events are dropped
- Logs can be retrieved on reconnection
- Proper ordering is maintained

---

## Best Practices

### Backend

1. Always use EventBus for event emission
2. Include tabId when event is tab-specific
3. Use appropriate event type for the action
4. Include meaningful messages for user display
5. Use sequence numbers for logs that need reliable delivery

### Frontend

1. Always clean up event listeners in ngOnDestroy
2. Use type guards before handling events
3. Handle unknown event types gracefully
4. Don't assume field presence - use optional chaining
5. For logs, use sequence-based polling for reliability

### Error Handling

1. Emit error events for user-visible errors
2. Include error code for programmatic handling
3. Include tabId to route errors to correct tab
4. Provide helpful error messages

### Board Events

1. Track execution status per edge
2. Update UI progressively as edges complete
3. Handle partial failures (some edges succeed, some fail)
4. Support cancellation at any point
