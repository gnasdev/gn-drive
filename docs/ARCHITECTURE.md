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

### 4. 2-Phase Initialization

- **Phase 1** (ServiceStartup): Set up working directory, env config, event channel
- **Phase 2** (CompleteInitialization): Load profiles, rclone config — deferred until after auth unlock

## Service Architecture

NS-Drive has 17 domain-separated services registered in `main.go`:

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
- Load and save profiles
- Configuration initialization and validation

**Key Methods:**
```go
GetConfigInfo(ctx context.Context) (*models.ConfigInfo, error)
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
GetTab(ctx context.Context, tabId string) (*Tab, error)
GetAllTabs(ctx context.Context) (map[string]*Tab, error)
UpdateTab(ctx context.Context, tabId string, updates map[string]interface{}) error
RenameTab(ctx context.Context, tabId, newName string) error
SetTabProfile(ctx context.Context, tabId string, profile *models.Profile) error
AddTabOutput(ctx context.Context, tabId string, output string) error
ClearTabOutput(ctx context.Context, tabId string) error
SetTabState(ctx context.Context, tabId string, state TabState) error
SetTabError(ctx context.Context, tabId string, errorMsg string) error
DeleteTab(ctx context.Context, tabId string) error
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
GetStats(ctx context.Context) (*models.AggregateStats, error)
ClearHistory(ctx context.Context) error
```

---

### Workflow & Operations Services

#### BoardService (`desktop/backend/services/board_service.go`)

**Responsibilities:**
- Visual workflow orchestration
- DAG (Directed Acyclic Graph) execution
- Topological sort for execution order
- Cycle detection

**Key Methods:**
```go
GetBoards(ctx context.Context) ([]models.Board, error)
GetBoard(ctx context.Context, boardId string) (*models.Board, error)
AddBoard(ctx context.Context, board models.Board) error
UpdateBoard(ctx context.Context, board models.Board) error
DeleteBoard(ctx context.Context, id string) error
ExecuteBoard(ctx context.Context, id string) (*models.BoardExecutionStatus, error)
StopBoardExecution(ctx context.Context, id string) error
GetBoardExecutionStatus(ctx context.Context, id string) (*models.BoardExecutionStatus, error)
GetExecutionLogs(ctx context.Context) []string
ClearExecutionLogs(ctx context.Context)
OnRemoteDeleted(remoteName string) error
```

**Execution Features:**
- Topological sort for dependency resolution
- Cycle detection to prevent infinite loops
- Parallel edge execution where possible
- Status tracking per edge
- Execution log capture

---

#### FlowService (`desktop/backend/services/flow_service.go`)

**Responsibilities:**
- Manage flows (named groups of sync operations)
- Persist flows to database
- Handle remote deletion cascade

**Key Methods:**
```go
GetFlows(ctx context.Context) ([]models.Flow, error)
SaveFlows(ctx context.Context, flows []models.Flow) error
OnRemoteDeleted(ctx context.Context, remoteName string) error
```

**Flow Structure:**
- Each flow contains multiple operations (source → target sync pairs)
- Operations reference remotes and can have individual sync configs
- Flows support scheduling via cron expressions

---

#### OperationService (`desktop/backend/services/operation_service.go`)

**Responsibilities:**
- File operations beyond sync (copy, move, check, dry-run)
- Remote file browsing
- Storage information

**Key Methods:**
```go
// File operations (return task IDs)
Copy(ctx context.Context, profile models.Profile, tabId string) (int, error)
Move(ctx context.Context, profile models.Profile, tabId string) (int, error)
CheckFiles(ctx context.Context, profile models.Profile, tabId string) (int, error)
DryRun(ctx context.Context, action string, profile models.Profile, tabId string) (int, error)
StopOperation(ctx context.Context, taskId int) error
GetActiveTasks(ctx context.Context) (map[int]*OperationTask, error)

// File browsing
ListFiles(ctx context.Context, remotePath string, recursive bool) ([]models.FileEntry, error)
DeleteFile(ctx context.Context, remotePath string) error
PurgeDir(ctx context.Context, remotePath string) error
MakeDir(ctx context.Context, remotePath string) error

// Storage info
GetAbout(ctx context.Context, remoteName string) (*models.QuotaInfo, error)
GetSize(ctx context.Context, remotePath string) (int64, int64, error)
```

---

#### CryptService (`desktop/backend/services/crypt_service.go`)

**Responsibilities:**
- Encrypted remote creation and management
- Crypt layer over any backend

**Key Methods:**
```go
CreateCryptRemote(ctx context.Context, cfg CryptRemoteConfig) error
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
GetPreUnlockSettings() AppSettings
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
- Tray menu with board and flow shortcuts
- Minimize to tray functionality

**Key Methods:**
```go
Initialize() error
RefreshMenu()
```

---

#### NotificationService (`desktop/backend/services/notification_service.go`)

**Responsibilities:**
- Desktop notifications (native macOS via UNUserNotificationCenter)
- App settings management and persistence

**Key Methods:**
```go
SendNotification(ctx context.Context, title, body string) error
SetEnabled(ctx context.Context, enabled bool)
IsEnabled(ctx context.Context) bool
SetDebugMode(ctx context.Context, enabled bool)
IsDebugMode(ctx context.Context) bool
SetMinimizeToTray(ctx context.Context, enabled bool)
IsMinimizeToTray(ctx context.Context) bool
SetMinimizeToTrayOnStartup(ctx context.Context, enabled bool)
IsMinimizeToTrayOnStartup(ctx context.Context) bool
SetStartAtLogin(ctx context.Context, enabled bool) error
IsStartAtLogin(ctx context.Context) bool
GetSettings(ctx context.Context) AppSettings
LoadSettings()
```

**App Settings:**
```go
type AppSettings struct {
    NotificationsEnabled    bool `json:"notifications_enabled"`
    DebugMode               bool `json:"debug_mode"`
    MinimizeToTray          bool `json:"minimize_to_tray"`
    StartAtLogin            bool `json:"start_at_login"`
    MinimizeToTrayOnStartup bool `json:"minimize_to_tray_on_startup"`
}
```

---

#### LogService (`desktop/backend/services/log_service.go`)

**Responsibilities:**
- Reliable log delivery with sequencing
- Tab-specific logging
- Sync event logging

**Key Methods:**
```go
Log(tabId, message, level string) uint64
LogSync(tabId, action, status, message string) uint64
GetLogsSince(ctx context.Context, tabId string, afterSeqNo uint64) ([]LogEntry, error)
GetLatestLogs(ctx context.Context, tabId string, count int) ([]LogEntry, error)
GetCurrentSeqNo(ctx context.Context) (uint64, error)
ClearLogs(ctx context.Context, tabId string) error
GetBufferSize(ctx context.Context) (int, error)
```

**Log Levels:** debug, info, warning, error

**Events Emitted:**
- `log:message` - New log entry with sequence number
- `sync:event` - Sync event log

---

### Import/Export Services

#### ExportService (`desktop/backend/services/export_service.go`)

**Responsibilities:**
- Configuration backup to binary `.nsd` format
- Selective export (boards, remotes, settings)
- Optional token exclusion
- Optional password encryption (AES-256-GCM)

**Key Methods:**
```go
GetExportPreview(ctx context.Context, options ExportOptions) (*ExportManifest, error)
ExportToBytes(ctx context.Context, options ExportOptions) ([]byte, error)
ExportToFile(ctx context.Context, path string, options ExportOptions) error
SelectExportFile(ctx context.Context) (string, error)
ExportWithDialog(ctx context.Context, options ExportOptions) (string, error)
```

**Export Options:**
- `IncludeBoards` - Export boards
- `IncludeRemotes` - Export remotes
- `IncludeSettings` - Export app settings
- `ExcludeTokens` - Exclude OAuth tokens from remotes
- `EncryptPassword` - Encrypt the export file with a password

---

#### ImportService (`desktop/backend/services/import_service.go`)

**Responsibilities:**
- Configuration restore from `.nsd` files
- Overwrite vs merge modes
- Encrypted import support
- Validation and preview before import

**Key Methods:**
```go
ValidateImportFile(ctx context.Context, filePath string) (*ImportPreview, error)
ValidateImportFileWithPassword(ctx context.Context, filePath, password string) (*ImportPreview, error)
ImportFromFile(ctx context.Context, filePath string, options ImportOptions) (*ImportResult, error)
ImportFromBytes(ctx context.Context, data []byte, options ImportOptions) (*ImportResult, error)
SelectImportFile(ctx context.Context) (string, error)
PreviewWithDialog(ctx context.Context) (*ImportPreview, string, error)
ImportWithDialog(ctx context.Context, options ImportOptions) (*ImportResult, error)
```

**Import Options:**
- `OverwriteBoards` - Overwrite existing boards with same name
- `OverwriteRemotes` - Overwrite existing remotes with same name
- `MergeMode` - Add new items only, skip existing
- `Password` - Password for encrypted backups

---

## Data Storage

### SQLite Database (`ns-drive.db`)

Primary data store for all application data:
- Profiles
- Boards
- Flows and operations
- Schedules
- History entries
- App settings

Managed by `db.go` with shared connection via `GetSharedDB()`, `InitDatabase()`, `CloseDatabase()`, `ResetSharedDB()`.

### rclone Configuration (`rclone.conf`)

Standard rclone configuration format for remote storage backends.

### Auth Metadata (`auth.json`)

Stores password hash, rate limiting state, and pre-unlock app settings. Always unencrypted.

### Configuration Locations

| File | Purpose |
|------|---------|
| `~/.config/ns-drive/rclone.conf` | Rclone remotes (encrypted when locked) |
| `~/.config/ns-drive/ns-drive.db` | SQLite database (encrypted when locked) |
| `~/.config/ns-drive/auth.json` | Auth metadata and pre-unlock settings |

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
| Operation | `operation:started`, `operation:completed`, `operation:failed` |
| Log | `log:message`, `sync:event` |
| Error | `error:occurred` |

> See [EVENTS.md](EVENTS.md) for detailed event documentation.

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

// All structured events flow through "tofe" channel
Events.On("tofe", (event) => {
    const parsedEvent = parseEvent(event.data);
    if (isSyncEvent(parsedEvent)) { ... }
    if (isConfigEvent(parsedEvent)) { ... }
});

// Auth events also use the tofe channel
Events.On("auth:unlocked", () => { ... });
Events.On("auth:locked", () => { ... });
```

---

## Frontend Architecture

### Key Components

| Component | Purpose |
|-----------|---------|
| `board/` | Visual workflow editor with drag-drop canvas |
| `remotes/` | Remote storage management UI |
| `settings/` | App settings (notifications, tray, login, security) |
| `components/` | Shared components (sidebar, toast, dialog, unlock-screen) |

### Services

| Service | Purpose |
|---------|---------|
| `app.service.ts` | Backend communication, event routing |
| `auth.service.ts` | Auth state management (lock/unlock) |
| `tab.service.ts` | Tab state management |
| `log-consumer.service.ts` | Log event consumption with deduplication |
| `theme.service.ts` | Dark/light theme |
| `navigation.service.ts` | Route navigation |
| `error.service.ts` | Error handling and reporting |

---

## Development Guidelines

### Adding New Services

1. Create service in `desktop/backend/services/`
2. Implement `SetApp()` for EventBus access
3. Implement `ServiceName()`, `ServiceStartup()`, `ServiceShutdown()`
4. Register service in `main.go`
5. Generate bindings: `wails3 generate bindings`

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
