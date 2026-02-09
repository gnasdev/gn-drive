# NS-Drive Architecture Documentation

## Overview

NS-Drive is a desktop application for cloud storage synchronization with a modern, domain-separated service architecture following Wails v3 best practices. This document outlines the architecture and communication patterns.

## Technology Stack

| Component | Version | Description |
|-----------|---------|-------------|
| Go | 1.25 | Backend runtime |
| Wails | v3.0.0-alpha.57 | Desktop app framework |
| Angular | 21.1 | Frontend framework |
| rclone | v1.73.0 | Cloud sync engine |
| TypeScript | 5.9 | Frontend type system |
| Tailwind CSS | 3.4 | UI styling |

## Architecture Principles

### 1. Domain Separation

Services are organized by business domain with clear responsibilities.

### 2. Event-Driven Communication

- Structured event types with consistent naming conventions
- Real-time updates via Wails v3 event system
- Type-safe event payloads

### 3. Context-Aware Operations

- All service methods accept `context.Context` as first parameter
- Support for operation cancellation
- Window-aware operations

## Service Architecture

NS-Drive has 12+ domain-separated services registered in `main.go`:

### Core Services

#### SyncService (`desktop/backend/services/sync_service.go`)

**Responsibilities:**
- Execute sync operations (pull, push, bi-directional, bi-resync)
- Manage active sync tasks with context cancellation
- Handle rclone command execution
- Emit sync progress events

**Key Methods:**
```go
StartSync(ctx context.Context, action string, profile models.Profile, tabId string) (*SyncResult, error)
StopSync(ctx context.Context, taskId int) error
GetActiveTasks(ctx context.Context) (map[int]*SyncTask, error)
WaitForTask(ctx context.Context, taskId int) error
```

**Supported Actions:**
- `pull` - Download from remote to local
- `push` - Upload from local to remote
- `bi` - Bi-directional sync
- `bi-resync` - Bi-directional sync with resync

**Events Emitted:**
- `sync:started` - Sync operation initiated
- `sync:progress` - Progress updates during sync
- `sync:completed` - Sync completed successfully
- `sync:failed` - Sync operation failed
- `sync:cancelled` - Sync operation cancelled

---

#### ConfigService (`desktop/backend/services/config_service.go`)

**Responsibilities:**
- Manage application configuration and working directory
- CRUD operations for profiles
- Load and save profiles to JSON file
- Configuration initialization and validation

**Key Methods:**
```go
GetConfigInfo(ctx context.Context) (*models.ConfigInfo, error)
GetProfiles(ctx context.Context) ([]models.Profile, error)
AddProfile(ctx context.Context, profile models.Profile) error
UpdateProfile(ctx context.Context, profile models.Profile) error
DeleteProfile(ctx context.Context, profileName string) error
SaveProfiles(ctx context.Context) error
```

**Events Emitted:**
- `config:updated` - Configuration changed
- `profile:added` - New profile created
- `profile:updated` - Profile modified
- `profile:deleted` - Profile removed

---

#### RemoteService (`desktop/backend/services/remote_service.go`)

**Responsibilities:**
- Manage rclone remote storage configurations
- Initialize and maintain rclone config
- CRUD operations for remotes
- Test remote connections

**Key Methods:**
```go
GetRemotes(ctx context.Context) ([]RemoteInfo, error)
AddRemote(ctx context.Context, name, remoteType string, config map[string]string) error
UpdateRemote(ctx context.Context, name string, config map[string]string) error
DeleteRemote(ctx context.Context, name string) error
TestRemote(ctx context.Context, name string) error
```

**Events Emitted:**
- `remote:added` - New remote added
- `remote:updated` - Remote configuration updated
- `remote:deleted` - Remote removed
- `remote:tested` - Remote connection tested

---

#### TabService (`desktop/backend/services/tab_service.go`)

**Responsibilities:**
- Manage tab lifecycle and state
- Track tab operations and sync tasks
- Handle tab output and error logging
- Maintain tab-to-profile associations

**Key Methods:**
```go
CreateTab(ctx context.Context, name string) (*Tab, error)
GetTab(ctx context.Context, id string) (*Tab, error)
GetAllTabs(ctx context.Context) (map[string]*Tab, error)
UpdateTab(ctx context.Context, id string, updates map[string]interface{}) error
RenameTab(ctx context.Context, id string, name string) error
SetTabProfile(ctx context.Context, id string, profile models.Profile) error
AddTabOutput(ctx context.Context, id string, output string) error
ClearTabOutput(ctx context.Context, id string) error
SetTabState(ctx context.Context, id string, state TabState) error
DeleteTab(ctx context.Context, id string) error
```

**Tab States:**
- `Running` - Tab is executing an operation
- `Stopped` - Tab is idle
- `Completed` - Tab operation finished successfully
- `Failed` - Tab encountered an error
- `Cancelled` - Tab operation was cancelled

**Events Emitted:**
- `tab:created` - New tab created
- `tab:updated` - Tab state changed
- `tab:deleted` - Tab removed
- `tab:output` - New output added to tab
- `tab:state_changed` - Tab state transition

---

### Scheduling & History Services

#### SchedulerService (`desktop/backend/services/scheduler_service.go`)

**Responsibilities:**
- Cron-based schedule management
- Automatic sync execution on schedule
- Track last run and next run times

**Key Methods:**
```go
AddSchedule(ctx context.Context, entry models.ScheduleEntry) error
UpdateSchedule(ctx context.Context, entry models.ScheduleEntry) error
DeleteSchedule(ctx context.Context, id string) error
GetSchedules(ctx context.Context) ([]models.ScheduleEntry, error)
EnableSchedule(ctx context.Context, id string) error
DisableSchedule(ctx context.Context, id string) error
```

**Storage:** `~/.config/ns-drive/schedules.json`

**Schedule Entry Fields:**
- `Id` - Unique identifier
- `ProfileName` - Associated profile
- `Action` - Sync action (pull/push/bi/bi-resync/copy/move)
- `CronExpr` - Cron expression
- `Enabled` - Whether schedule is active
- `LastRun` / `NextRun` - Timestamps
- `LastResult` - success/failed/cancelled

---

#### HistoryService (`desktop/backend/services/history_service.go`)

**Responsibilities:**
- Track sync operation history
- Provide statistics and analytics
- Paginated history retrieval

**Key Methods:**
```go
AddEntry(ctx context.Context, entry models.HistoryEntry) error
GetHistory(ctx context.Context, limit, offset int) ([]models.HistoryEntry, error)
GetHistoryForProfile(ctx context.Context, profileName string) ([]models.HistoryEntry, error)
GetStats(ctx context.Context) (*HistoryStats, error)
ClearHistory(ctx context.Context) error
```

**Storage:** `~/.config/ns-drive/history.json`

**History Entry Fields:**
- `Id` - Unique identifier
- `Timestamp` - Operation time
- `ProfileName` - Associated profile
- `Action` - Sync action performed
- `Status` - success/failed/cancelled
- `BytesTransferred` - Data transferred
- `Message` - Result message

---

### Workflow & Operations Services

#### BoardService (`desktop/backend/services/board_service.go`)

**Responsibilities:**
- Visual workflow orchestration
- DAG (Directed Acyclic Graph) execution
- Topological sort for execution order
- Cycle detection

**Key Entities:**
```go
type BoardNode struct {
    Id         string
    RemoteName string
    Path       string
    Label      string
    X, Y       float64  // Canvas coordinates
}

type BoardEdge struct {
    Id         string
    SourceId   string
    TargetId   string
    Action     string  // pull/push/bi/bi-resync
    SyncConfig *Profile
}

type Board struct {
    Id          string
    Name        string
    Description string
    Nodes       []BoardNode
    Edges       []BoardEdge
    CreatedAt   time.Time
    UpdatedAt   time.Time
}
```

**Key Methods:**
```go
GetBoards(ctx context.Context) ([]models.Board, error)
AddBoard(ctx context.Context, board models.Board) error
UpdateBoard(ctx context.Context, board models.Board) error
DeleteBoard(ctx context.Context, id string) error
ExecuteBoard(ctx context.Context, id string) error
StopBoardExecution(ctx context.Context, id string) error
GetBoardExecutionStatus(ctx context.Context, id string) (*BoardExecutionStatus, error)
```

**Storage:** `~/.config/ns-drive/boards.json`

**Execution Features:**
- Topological sort for dependency resolution
- Cycle detection to prevent infinite loops
- Parallel edge execution where possible
- Status tracking per edge

---

#### OperationService (`desktop/backend/services/operation_service.go`)

**Responsibilities:**
- File operations beyond sync
- Remote file browsing
- Storage information

**Key Methods:**
```go
// File operations
Copy(ctx context.Context, source, dest string) error
Move(ctx context.Context, source, dest string) error
Check(ctx context.Context, source, dest string) error
DryRun(ctx context.Context, action string, profile models.Profile) (string, error)

// File browsing
ListFiles(ctx context.Context, remote, path string) ([]FileEntry, error)
DeleteFile(ctx context.Context, remote, path string) error
PurgeDir(ctx context.Context, remote, path string) error
MakeDir(ctx context.Context, remote, path string) error

// Storage info
GetAbout(ctx context.Context, remote string) (*QuotaInfo, error)
GetSize(ctx context.Context, remote, path string) (int64, error)
```

---

#### CryptService (`desktop/backend/services/crypt_service.go`)

**Responsibilities:**
- Encrypted remote creation and management
- Crypt layer over any backend

**Key Methods:**
```go
CreateCryptRemote(ctx context.Context, name, underlying, password, salt string) error
DeleteCryptRemote(ctx context.Context, name string) error
ListCryptRemotes(ctx context.Context) ([]string, error)
```

---

### Security Services

#### AuthService (`desktop/backend/services/auth_service.go`)

**Responsibilities:**
- Master password protection with Argon2id KDF
- AES-256-GCM file encryption for sensitive data (rclone.conf, ns-drive.db)
- App lock/unlock lifecycle management
- Rate limiting with exponential backoff
- Crash recovery for interrupted encrypt/decrypt operations
- 2-phase app initialization (deferred DB/rclone init until after unlock)

**Key Methods:**
```go
IsAuthEnabled(ctx context.Context) bool
IsUnlocked(ctx context.Context) bool
SetupPassword(ctx context.Context, password string) error
Unlock(ctx context.Context, password string) error
Lock(ctx context.Context) error
ChangePassword(ctx context.Context, oldPassword, newPassword string) error
RemovePassword(ctx context.Context, password string) error
GetLockoutStatus(ctx context.Context) LockoutStatus
GetPreUnlockSettings() PreUnlockSettings
SyncAppSettings(settings AppSettings)
```

**Encrypted Files (when locked):**
- `rclone.conf` → `rclone.conf.enc`
- `ns-drive.db` → `ns-drive.db.enc`

**Always Unencrypted:**
- `auth.json` — stores password hash, rate limit state, and pre-unlock app settings

**Events Emitted:**
- `auth:unlocked` — App unlocked (password verified or no auth)
- `auth:locked` — App locked (user action or shutdown)

> See [SECURITY.md](SECURITY.md) for detailed encryption design, rate limiting rules, and crash recovery behavior.

---

### System Integration Services

#### TrayService (`desktop/backend/services/tray_service.go`)

**Responsibilities:**
- System tray integration
- Tray menu with board shortcuts
- Minimize to tray functionality

**Key Methods:**
```go
Initialize() error
RefreshMenu() error
executeBoard(id string) error
showWindow() error
```

---

#### NotificationService (`desktop/backend/services/notification_service.go`)

**Responsibilities:**
- Desktop notifications
- App settings management

**Key Methods:**
```go
SendNotification(ctx context.Context, title, body string) error
SetEnabled(ctx context.Context, enabled bool) error
SetMinimizeToTray(ctx context.Context, enabled bool) error
SetStartAtLogin(ctx context.Context, enabled bool) error
GetSettings(ctx context.Context) (*AppSettings, error)
```

**App Settings:**
- `NotificationsEnabled` - Show desktop notifications
- `MinimizeToTray` - Minimize to system tray instead of closing
- `StartAtLogin` - Launch app at system startup
- `DebugMode` - Enable debug logging

---

#### LogService (`desktop/backend/services/log_service.go`)

**Responsibilities:**
- Reliable log delivery with sequencing
- Tab-specific logging
- Sync event logging

**Key Methods:**
```go
Log(tabId, message string, level LogLevel) error
LogSync(tabId, action, status, message string) error
GetLogsSince(ctx context.Context, tabId string, afterSeqNo int64) ([]LogEntry, error)
GetLatestLogs(ctx context.Context, tabId string, count int) ([]LogEntry, error)
GetCurrentSeqNo(ctx context.Context) (int64, error)
ClearLogs(ctx context.Context, tabId string) error
```

**Log Levels:** debug, info, warning, error

**Events Emitted:**
- `log:message` - New log entry with sequence number
- `sync:event` - Sync event log

---

### Import/Export Services

#### ExportService (`desktop/backend/services/export_service.go`)

**Responsibilities:**
- Configuration backup
- Selective export (profiles, remotes, boards)
- Optional token inclusion

**Key Methods:**
```go
GetExportPreview(ctx context.Context, options ExportOptions) (*ExportPreview, error)
ExportToBytes(ctx context.Context, options ExportOptions) ([]byte, error)
ExportToFile(ctx context.Context, path string, options ExportOptions) error
ExportWithDialog(ctx context.Context, options ExportOptions) error
```

**Export Options:**
- `IncludeProfiles` - Export profiles
- `IncludeRemotes` - Export remotes
- `IncludeBoards` - Export boards
- `IncludeTokens` - Include OAuth tokens (security risk)

---

#### ImportService (`desktop/backend/services/import_service.go`)

**Responsibilities:**
- Configuration restore
- Merge vs replace modes
- Validation before import

**Key Methods:**
```go
ValidateImportFile(ctx context.Context, path string) (*ImportValidation, error)
ImportFromFile(ctx context.Context, path string, options ImportOptions) (*ImportResult, error)
PreviewWithDialog(ctx context.Context) (*ImportPreview, error)
```

**Import Options:**
- `MergeProfiles` - Merge or replace existing profiles
- `MergeRemotes` - Merge or replace existing remotes
- `MergeBoards` - Merge or replace existing boards

---

## Data Models

All models located in `desktop/backend/models/`:

### Profile
```go
type Profile struct {
    Name          string   `json:"name"`
    From          string   `json:"from"`           // source remote:path
    To            string   `json:"to"`             // destination remote:path
    IncludedPaths []string `json:"included_paths"` // Include patterns
    ExcludedPaths []string `json:"excluded_paths"` // Exclude patterns
    Bandwidth     int      `json:"bandwidth"`      // MB/s limit
    Parallel      int      `json:"parallel"`       // Concurrent transfers
}
```

### ScheduleEntry
```go
type ScheduleEntry struct {
    Id          string     `json:"id"`
    ProfileName string     `json:"profile_name"`
    Action      string     `json:"action"`
    CronExpr    string     `json:"cron_expr"`
    Enabled     bool       `json:"enabled"`
    LastRun     *time.Time `json:"last_run"`
    NextRun     *time.Time `json:"next_run"`
    LastResult  string     `json:"last_result"`
}
```

### Board / BoardNode / BoardEdge
```go
type BoardNode struct {
    Id         string  `json:"id"`
    RemoteName string  `json:"remote_name"`
    Path       string  `json:"path"`
    Label      string  `json:"label"`
    X          float64 `json:"x"`
    Y          float64 `json:"y"`
}

type BoardEdge struct {
    Id         string   `json:"id"`
    SourceId   string   `json:"source_id"`
    TargetId   string   `json:"target_id"`
    Action     string   `json:"action"`
    SyncConfig *Profile `json:"sync_config"`
}

type Board struct {
    Id          string      `json:"id"`
    Name        string      `json:"name"`
    Description string      `json:"description"`
    Nodes       []BoardNode `json:"nodes"`
    Edges       []BoardEdge `json:"edges"`
    CreatedAt   time.Time   `json:"created_at"`
    UpdatedAt   time.Time   `json:"updated_at"`
}
```

### HistoryEntry
```go
type HistoryEntry struct {
    Id               string    `json:"id"`
    Timestamp        time.Time `json:"timestamp"`
    ProfileName      string    `json:"profile_name"`
    Action           string    `json:"action"`
    Status           string    `json:"status"`
    BytesTransferred int64     `json:"bytes_transferred"`
    Message          string    `json:"message"`
}
```

### FileEntry
```go
type FileEntry struct {
    Name    string    `json:"name"`
    Path    string    `json:"path"`
    Type    string    `json:"type"`  // file or dir
    Size    int64     `json:"size"`
    ModTime time.Time `json:"mod_time"`
    IsDir   bool      `json:"is_dir"`
}
```

---

## Event System

### Event Naming Convention

Format: `domain:action`

### Event Categories

| Category | Events |
|----------|--------|
| Auth | `auth:unlocked`, `auth:locked` |
| Sync | `sync:started`, `sync:progress`, `sync:completed`, `sync:failed`, `sync:cancelled` |
| Config | `config:updated`, `profile:added`, `profile:updated`, `profile:deleted` |
| Remote | `remote:added`, `remote:updated`, `remote:deleted`, `remote:tested` |
| Tab | `tab:created`, `tab:updated`, `tab:deleted`, `tab:output`, `tab:state_changed` |
| Board | `board:created`, `board:updated`, `board:deleted`, `board:execution_status` |
| Log | `log:message`, `sync:event` |
| Error | `error:occurred` |

---

## Frontend Integration

### Generated Bindings

Services are automatically exposed to the frontend via Wails v3 bindings. Bindings are generated in `frontend/bindings/` directory (symlinked to `frontend/wailsjs/`):

```typescript
import { Sync, GetConfigInfo } from "../../wailsjs/desktop/backend/app";
import * as models from "../../wailsjs/desktop/backend/models/models";
```

### Event Handling

```typescript
import { Events } from "@wailsio/runtime";

Events.On("sync:progress", (event) => {
    console.log("Sync progress:", event.data);
});

Events.On("board:execution_status", (event) => {
    // Update board execution UI
});
```

---

## Frontend Architecture

### Key Components

| Component | Purpose |
|-----------|---------|
| `board/` | Visual workflow editor with drag-drop canvas |
| `remotes/` | Remote storage management UI |
| `settings/` | App settings (notifications, tray, login) |
| `components/` | Shared components (sidebar, toast, dialog) |

### Services

| Service | Purpose |
|---------|---------|
| `app.service.ts` | Backend communication |
| `auth.service.ts` | Auth state management (lock/unlock) |
| `tab.service.ts` | Tab state management |
| `log-consumer.service.ts` | Log event consumption |
| `theme.service.ts` | Dark/light theme |
| `navigation.service.ts` | Route navigation |

---

## Configuration Storage

| File | Purpose |
|------|---------|
| `~/.config/ns-drive/profiles.json` | Sync profiles |
| `~/.config/ns-drive/rclone.conf` | Rclone remotes (encrypted when locked) |
| `~/.config/ns-drive/ns-drive.db` | SQLite database (encrypted when locked) |
| `~/.config/ns-drive/auth.json` | Auth metadata and pre-unlock settings |
| `~/.config/ns-drive/schedules.json` | Scheduled tasks |
| `~/.config/ns-drive/boards.json` | Workflow boards |
| `~/.config/ns-drive/history.json` | Operation history |
| `~/.config/ns-drive/app_settings.json` | App settings |

---

## Development Guidelines

### Adding New Services

1. Create service in `desktop/backend/services/`
2. Implement `SetApp()` for EventBus access
3. Register service in `main.go`
4. Generate bindings: `wails3 generate bindings`

### Event Best Practices

1. Use consistent naming: `domain:action`
2. Include timestamp in all events
3. Provide structured data payloads
4. Emit events for all state changes

### Error Handling

1. Use structured error types from `errors/types.go`
2. Provide user-friendly error messages
3. Include context information (tab ID, operation type)
4. Emit error events for frontend display
