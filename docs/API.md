# Backend API Reference

This document describes the Go backend services API exposed to the frontend via Wails bindings.

## Table of Contents

- [App Service (Legacy)](#app-service-legacy)
- [AuthService](#authservice)
- [SyncService](#syncservice)
- [ConfigService](#configservice)
- [RemoteService](#remoteservice)
- [TabService](#tabservice)
- [SchedulerService](#schedulerservice)
- [HistoryService](#historyservice)
- [BoardService](#boardservice)
- [OperationService](#operationservice)
- [CryptService](#cryptservice)
- [NotificationService](#notificationservice)
- [LogService](#logservice)
- [ExportService](#exportservice)
- [ImportService](#importservice)
- [Data Models](#data-models)
- [Error Handling](#error-handling)

---

## App Service (Legacy)

Legacy service for sync operations and configuration management.

### Sync Methods

#### `Sync(task string, profile Profile) int`

Start a sync operation without tab association.

**Parameters:**
- `task`: Sync action type - `"pull"`, `"push"`, `"bi"`, `"bi-resync"`
- `profile`: Profile configuration

**Returns:** Task ID (int) for tracking

---

#### `SyncWithTabId(task string, profile Profile, tabId string) int`

Start a sync operation associated with a specific tab.

**Parameters:**
- `task`: Sync action type
- `profile`: Profile configuration
- `tabId`: Tab identifier for event routing

**Returns:** Task ID (int)

---

#### `StopCommand(id int)`

Stop a running sync operation.

---

### Configuration Methods

#### `GetConfigInfo() ConfigInfo`

Get current configuration including profiles.

---

#### `UpdateProfiles(profiles Profile[]) AppError | null`

Update all profiles.

---

### Remote Methods

#### `GetRemotes() Remote[]`

Get list of configured cloud remotes.

---

#### `AddRemote(name string, type string, config map[string]string) AppError | null`

Add a new cloud remote.

Supported remote types:
- `drive` - Google Drive
- `dropbox` - Dropbox
- `onedrive` - OneDrive
- `box` - Box
- `yandex` - Yandex Disk
- `gphotos` - Google Photos
- `iclouddrive` - iCloud Drive

---

#### `DeleteRemote(name string) AppError | null`

Delete a remote and associated profiles.

---

#### `StopAddingRemote() AppError | null`

Cancel an in-progress OAuth flow.

---

## AuthService

Service for master password protection and file encryption.

> See [SECURITY.md](SECURITY.md) for detailed encryption design, rate limiting rules, and crash recovery.

### Methods

#### `IsAuthEnabled(ctx Context) bool`

Check if password protection is configured (auth.json exists and enabled).

---

#### `IsUnlocked(ctx Context) bool`

Check if the app is currently unlocked.

---

#### `SetupPassword(ctx Context, password string) error`

Enable password protection for the first time. Derives encryption key with Argon2id, writes auth.json, and stores key in memory. Files are encrypted on next Lock or Shutdown.

**Validation:** Password must be at least 4 characters.

---

#### `Unlock(ctx Context, password string) error`

Verify password, decrypt .enc files, initialize database and rclone config. Subject to rate limiting.

**Events:** Emits `auth:unlocked` on success.

---

#### `Lock(ctx Context) error`

Close database, encrypt plaintext files, zero encryption key.

**Events:** Emits `auth:locked`.

---

#### `ChangePassword(ctx Context, oldPassword, newPassword string) error`

Verify old password, re-encrypt all files with new key, update auth.json.

---

#### `RemovePassword(ctx Context, password string) error`

Verify password, clean up .enc files, delete auth.json, zero key.

---

#### `GetLockoutStatus(ctx Context) LockoutStatus`

Get current rate limiting state.

**Returns:**
```go
type LockoutStatus struct {
    FailedAttempts int    `json:"failed_attempts"`
    LockedOut      bool   `json:"locked_out"`
    LockoutUntil   string `json:"lockout_until"`
    RetryAfter     int    `json:"retry_after"` // seconds
}
```

---

#### `GetPreUnlockSettings() PreUnlockSettings`

Get tray/startup settings from auth.json (available before unlock when DB is encrypted).

**Returns:**
```go
type PreUnlockSettings struct {
    MinimizeToTray          bool `json:"minimize_to_tray"`
    StartAtLogin            bool `json:"start_at_login"`
    MinimizeToTrayOnStartup bool `json:"minimize_to_tray_on_startup"`
}
```

---

#### `SyncAppSettings(settings AppSettings)`

Sync app settings to auth.json so they are available before unlock on next startup.

---

## SyncService

Service for managing sync operations with context support.

### Methods

#### `StartSync(ctx Context, action string, profile Profile, tabId string) (SyncResult, error)`

Start a sync operation with context cancellation support.

**Returns:**
```go
type SyncResult struct {
    TaskId    int       `json:"taskId"`
    Action    string    `json:"action"`
    Status    string    `json:"status"`
    Message   string    `json:"message"`
    StartTime time.Time `json:"startTime"`
    EndTime   *time.Time `json:"endTime,omitempty"`
}
```

---

#### `StopSync(ctx Context, taskId int) error`

Stop a running sync operation.

---

#### `GetActiveTasks(ctx Context) (map[int]*SyncTask, error)`

Get all currently active sync tasks.

---

#### `WaitForTask(ctx Context, taskId int) error`

Wait for a specific task to complete.

---

## ConfigService

Service for profile management.

### Methods

#### `GetConfigInfo(ctx Context) (*ConfigInfo, error)`

Get configuration with defensive copy.

---

#### `GetProfiles(ctx Context) ([]Profile, error)`

Get all profiles.

---

#### `AddProfile(ctx Context, profile Profile) error`

Add a new profile with validation.

**Validation:**
- Name required and unique
- From/To paths required
- Paths validated for format and security
- Parallel must be 0-256
- Bandwidth must be non-negative

---

#### `UpdateProfile(ctx Context, profile Profile) error`

Update existing profile.

---

#### `DeleteProfile(ctx Context, name string) error`

Delete a profile by name.

---

#### `SaveProfiles(ctx Context) error`

Persist profiles to disk.

---

## RemoteService

Service for rclone remote management.

### Methods

#### `GetRemotes(ctx Context) ([]RemoteInfo, error)`

Get all configured remotes with metadata.

---

#### `AddRemote(ctx Context, name, remoteType string, config map[string]string) error`

Add a new remote.

---

#### `UpdateRemote(ctx Context, name string, config map[string]string) error`

Update remote configuration.

---

#### `DeleteRemote(ctx Context, name string) error`

Delete a remote.

---

#### `TestRemote(ctx Context, name string) error`

Test remote connectivity.

---

## TabService

Service for tab lifecycle management.

### Methods

#### `CreateTab(ctx Context, name string) (*Tab, error)`

Create a new tab.

---

#### `GetTab(ctx Context, id string) (*Tab, error)`

Get tab by ID.

---

#### `GetAllTabs(ctx Context) (map[string]*Tab, error)`

Get all tabs.

---

#### `UpdateTab(ctx Context, id string, updates map[string]interface{}) error`

Update tab properties.

---

#### `RenameTab(ctx Context, id string, name string) error`

Rename a tab.

---

#### `SetTabProfile(ctx Context, id string, profile Profile) error`

Associate a profile with a tab.

---

#### `AddTabOutput(ctx Context, id string, output string) error`

Append output to tab.

---

#### `ClearTabOutput(ctx Context, id string) error`

Clear tab output.

---

#### `SetTabState(ctx Context, id string, state TabState) error`

Set tab state (Running, Stopped, Completed, Failed, Cancelled).

---

#### `DeleteTab(ctx Context, id string) error`

Delete a tab.

---

## SchedulerService

Service for cron-based scheduling.

### Methods

#### `AddSchedule(ctx Context, entry ScheduleEntry) error`

Add a new scheduled task.

---

#### `UpdateSchedule(ctx Context, entry ScheduleEntry) error`

Update a schedule.

---

#### `DeleteSchedule(ctx Context, id string) error`

Delete a schedule.

---

#### `GetSchedules(ctx Context) ([]ScheduleEntry, error)`

Get all schedules.

---

#### `EnableSchedule(ctx Context, id string) error`

Enable a schedule.

---

#### `DisableSchedule(ctx Context, id string) error`

Disable a schedule.

---

## HistoryService

Service for operation history tracking.

### Methods

#### `AddEntry(ctx Context, entry HistoryEntry) error`

Add a history entry.

---

#### `GetHistory(ctx Context, limit, offset int) ([]HistoryEntry, error)`

Get paginated history.

---

#### `GetHistoryForProfile(ctx Context, profileName string) ([]HistoryEntry, error)`

Get history for a specific profile.

---

#### `GetStats(ctx Context) (*HistoryStats, error)`

Get aggregate statistics.

---

#### `ClearHistory(ctx Context) error`

Clear all history.

---

## BoardService

Service for visual workflow management.

### Methods

#### `GetBoards(ctx Context) ([]Board, error)`

Get all workflow boards.

---

#### `AddBoard(ctx Context, board Board) error`

Create a new board.

---

#### `UpdateBoard(ctx Context, board Board) error`

Update a board.

---

#### `DeleteBoard(ctx Context, id string) error`

Delete a board.

---

#### `ExecuteBoard(ctx Context, id string) error`

Execute a workflow board (DAG execution with topological sort).

---

#### `StopBoardExecution(ctx Context, id string) error`

Stop a running board execution.

---

#### `GetBoardExecutionStatus(ctx Context, id string) (*BoardExecutionStatus, error)`

Get current execution status.

**Returns:**
```go
type BoardExecutionStatus struct {
    BoardId      string               `json:"boardId"`
    Status       string               `json:"status"` // running|completed|failed|cancelled
    EdgeStatuses []EdgeExecutionStatus `json:"edgeStatuses"`
    StartTime    time.Time            `json:"startTime"`
    EndTime      *time.Time           `json:"endTime"`
}
```

---

## OperationService

Service for file operations.

### File Operations

#### `Copy(ctx Context, source, dest string) error`

Copy files/directories.

---

#### `Move(ctx Context, source, dest string) error`

Move files/directories.

---

#### `Check(ctx Context, source, dest string) error`

Check for differences between source and dest.

---

#### `DryRun(ctx Context, action string, profile Profile) (string, error)`

Perform a dry run of sync operation.

---

### File Browsing

#### `ListFiles(ctx Context, remote, path string) ([]FileEntry, error)`

List files in a remote path.

---

#### `DeleteFile(ctx Context, remote, path string) error`

Delete a file.

---

#### `PurgeDir(ctx Context, remote, path string) error`

Purge a directory (delete including contents).

---

#### `MakeDir(ctx Context, remote, path string) error`

Create a directory.

---

### Storage Info

#### `GetAbout(ctx Context, remote string) (*QuotaInfo, error)`

Get storage quota information.

**Returns:**
```go
type QuotaInfo struct {
    Used  int64 `json:"used"`
    Total int64 `json:"total"`
}
```

---

#### `GetSize(ctx Context, remote, path string) (int64, error)`

Get size of a path.

---

## CryptService

Service for encrypted remote management.

### Methods

#### `CreateCryptRemote(ctx Context, name, underlying, password, salt string) error`

Create an encrypted remote.

---

#### `DeleteCryptRemote(ctx Context, name string) error`

Delete an encrypted remote.

---

#### `ListCryptRemotes(ctx Context) ([]string, error)`

List all encrypted remotes.

---

## NotificationService

Service for notifications and app settings.

### Methods

#### `SendNotification(ctx Context, title, body string) error`

Send a desktop notification.

---

#### `SetEnabled(ctx Context, enabled bool) error`

Enable/disable notifications.

---

#### `SetMinimizeToTray(ctx Context, enabled bool) error`

Set minimize to tray behavior.

---

#### `SetStartAtLogin(ctx Context, enabled bool) error`

Set start at login preference.

---

#### `GetSettings(ctx Context) (*AppSettings, error)`

Get all app settings.

**Returns:**
```go
type AppSettings struct {
    NotificationsEnabled bool `json:"notificationsEnabled"`
    MinimizeToTray       bool `json:"minimizeToTray"`
    StartAtLogin         bool `json:"startAtLogin"`
    DebugMode            bool `json:"debugMode"`
}
```

---

## LogService

Service for reliable log delivery.

### Methods

#### `Log(tabId, message string, level LogLevel) error`

Log a message with level (debug, info, warning, error).

---

#### `LogSync(tabId, action, status, message string) error`

Log a sync event.

---

#### `GetLogsSince(ctx Context, tabId string, afterSeqNo int64) ([]LogEntry, error)`

Get logs after a sequence number.

---

#### `GetLatestLogs(ctx Context, tabId string, count int) ([]LogEntry, error)`

Get latest N logs for a tab.

---

#### `GetCurrentSeqNo(ctx Context) (int64, error)`

Get current sequence number.

---

#### `ClearLogs(ctx Context, tabId string) error`

Clear logs for a tab.

---

## ExportService

Service for configuration export.

### Methods

#### `GetExportPreview(ctx Context, options ExportOptions) (*ExportPreview, error)`

Preview what will be exported.

---

#### `ExportToBytes(ctx Context, options ExportOptions) ([]byte, error)`

Export to compressed bytes.

---

#### `ExportToFile(ctx Context, path string, options ExportOptions) error`

Export to a file.

---

#### `ExportWithDialog(ctx Context, options ExportOptions) error`

Export with file picker dialog.

**Export Options:**
```go
type ExportOptions struct {
    IncludeProfiles bool `json:"includeProfiles"`
    IncludeRemotes  bool `json:"includeRemotes"`
    IncludeBoards   bool `json:"includeBoards"`
    IncludeTokens   bool `json:"includeTokens"` // Security risk
}
```

---

## ImportService

Service for configuration import.

### Methods

#### `ValidateImportFile(ctx Context, path string) (*ImportValidation, error)`

Validate an import file before importing.

---

#### `ImportFromFile(ctx Context, path string, options ImportOptions) (*ImportResult, error)`

Import from a file.

**Import Options:**
```go
type ImportOptions struct {
    MergeProfiles bool `json:"mergeProfiles"` // true=merge, false=replace
    MergeRemotes  bool `json:"mergeRemotes"`
    MergeBoards   bool `json:"mergeBoards"`
}
```

---

#### `PreviewWithDialog(ctx Context) (*ImportPreview, error)`

Preview import with file picker dialog.

---

## Data Models

### Profile

```typescript
interface Profile {
    name: string;           // Unique profile name
    from: string;           // Source path (local or remote:path)
    to: string;             // Destination path
    included_paths: string[]; // Include patterns (glob)
    excluded_paths: string[]; // Exclude patterns (glob)
    bandwidth: number;      // MB/s limit (0 = unlimited)
    parallel: number;       // Concurrent transfers (default 16)
}
```

### ConfigInfo

```typescript
interface ConfigInfo {
    working_dir: string;
    selected_profile_index: number;
    profiles: Profile[];
    env_config: EnvConfig;
}
```

### ScheduleEntry

```typescript
interface ScheduleEntry {
    id: string;
    profile_name: string;
    action: string;       // pull|push|bi|bi-resync|copy|move
    cron_expr: string;
    enabled: boolean;
    last_run?: string;    // ISO timestamp
    next_run?: string;
    last_result?: string; // success|failed|cancelled
}
```

### Board / BoardNode / BoardEdge

```typescript
interface BoardNode {
    id: string;
    remote_name: string;
    path: string;
    label: string;
    x: number;
    y: number;
}

interface BoardEdge {
    id: string;
    source_id: string;
    target_id: string;
    action: string;
    sync_config?: Profile;
}

interface Board {
    id: string;
    name: string;
    description: string;
    nodes: BoardNode[];
    edges: BoardEdge[];
    created_at: string;
    updated_at: string;
}
```

### HistoryEntry

```typescript
interface HistoryEntry {
    id: string;
    timestamp: string;
    profile_name: string;
    action: string;
    status: string;
    bytes_transferred: number;
    message: string;
}
```

### FileEntry

```typescript
interface FileEntry {
    name: string;
    path: string;
    type: string;      // file|dir
    size: number;
    mod_time: string;
    is_dir: boolean;
}
```

### AppError

```typescript
interface AppError {
    error: string;
    code?: string;
    details?: string;
}
```

---

## Error Handling

All methods that can fail return either:
- `null` on success with `AppError | null` return type
- `error` in Go's standard error return pattern

Error codes (from `errors/types.go`):
- `VALIDATION_ERROR` - Input validation failed
- `NOT_FOUND_ERROR` - Resource not found
- `RCLONE_ERROR` - rclone operation failed
- `FILE_SYSTEM_ERROR` - File I/O error
- `NETWORK_ERROR` - Network operation failed
- `EXTERNAL_SERVICE_ERROR` - External service error

---

## Usage Examples

### TypeScript Frontend

```typescript
import {
    Sync,
    GetConfigInfo,
    GetRemotes,
    AddRemote
} from "wailsjs/desktop/backend/app";
import * as models from "wailsjs/desktop/backend/models/models";

// Get configuration
const config = await GetConfigInfo();
console.log("Profiles:", config.profiles);

// Start sync
const taskId = await Sync("pull", profile);
console.log("Started task:", taskId);

// Add remote
const error = await AddRemote("my-drive", "drive", {});
if (error) {
    console.error("Failed:", error.error);
}
```

### Using Board Service

```typescript
import { BoardService } from "wailsjs/desktop/backend/services/boardservice";

// Get all boards
const boards = await BoardService.GetBoards();

// Execute a board
await BoardService.ExecuteBoard(boardId);

// Check execution status
const status = await BoardService.GetBoardExecutionStatus(boardId);
console.log("Status:", status.status);
```

### Using Scheduler Service

```typescript
import { SchedulerService } from "wailsjs/desktop/backend/services/schedulerservice";

// Add a schedule
await SchedulerService.AddSchedule({
    id: "schedule-1",
    profile_name: "my-profile",
    action: "pull",
    cron_expr: "0 0 * * *", // Daily at midnight
    enabled: true
});

// Get all schedules
const schedules = await SchedulerService.GetSchedules();
```
