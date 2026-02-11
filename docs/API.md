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
- [FlowService](#flowservice)
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

Legacy service (`desktop/backend/app.go`) for sync operations and configuration management. Methods are exposed to frontend via Wails bindings.

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

#### `ReauthRemote(name string) AppError | null`

Re-authenticate a remote (refresh OAuth token).

---

#### `StopAddingRemote() AppError | null`

Cancel an in-progress OAuth flow.

---

### Logging

#### `LogFrontendMessage(entry FrontendLogEntry) error`

Log a message from the frontend.

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

Enable password protection for the first time. Derives encryption key with Argon2id, writes auth.json, deletes legacy JSON config files, and stores key in memory. Files are encrypted on next Lock or Shutdown.

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
    LockedUntil    string `json:"locked_until"`
    IsLocked       bool   `json:"is_locked"`
    RetryAfterSecs int    `json:"retry_after_secs"`
}
```

---

#### `GetPreUnlockSettings() AppSettings`

Get tray/startup settings from auth.json (available before unlock when DB is encrypted).

**Returns:** `AppSettings` — see [NotificationService](#notificationservice) for struct definition.

---

#### `SyncAppSettings(settings AppSettings)`

Sync app settings to auth.json so they are available before unlock on next startup.

---

## SyncService

Service for managing sync operations with context support.

### Methods

#### `StartSync(ctx Context, action string, profile Profile, tabId string) (*SyncResult, error)`

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

## RemoteService

Service for rclone remote management.

### Methods

#### `GetRemotes(ctx Context) ([]RemoteInfo, error)`

Get all configured remotes with metadata.

**Returns:**
```go
type RemoteInfo struct {
    Name        string            `json:"name"`
    Type        string            `json:"type"`
    Config      map[string]string `json:"config"`
    Description string            `json:"description"`
}
```

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

#### `GetTab(ctx Context, tabId string) (*Tab, error)`

Get tab by ID.

---

#### `GetAllTabs(ctx Context) (map[string]*Tab, error)`

Get all tabs.

---

#### `UpdateTab(ctx Context, tabId string, updates map[string]interface{}) error`

Update tab properties.

---

#### `RenameTab(ctx Context, tabId, newName string) error`

Rename a tab.

---

#### `SetTabProfile(ctx Context, tabId string, profile *Profile) error`

Associate a profile with a tab.

---

#### `AddTabOutput(ctx Context, tabId string, output string) error`

Append output to tab.

---

#### `ClearTabOutput(ctx Context, tabId string) error`

Clear tab output.

---

#### `SetTabState(ctx Context, tabId string, state TabState) error`

Set tab state (Running, Stopped, Completed, Failed, Cancelled).

---

#### `SetTabError(ctx Context, tabId string, errorMsg string) error`

Set error message for a tab.

---

#### `DeleteTab(ctx Context, tabId string) error`

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

#### `GetStats(ctx Context) (*AggregateStats, error)`

Get aggregate statistics.

**Returns:**
```go
type AggregateStats struct {
    TotalOperations int    `json:"total_operations"`
    SuccessCount    int    `json:"success_count"`
    FailureCount    int    `json:"failure_count"`
    CancelledCount  int    `json:"cancelled_count"`
    TotalBytes      int64  `json:"total_bytes"`
    TotalFiles      int64  `json:"total_files"`
    AverageDuration string `json:"average_duration"`
}
```

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

#### `GetBoard(ctx Context, boardId string) (*Board, error)`

Get a single board by ID.

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

#### `ExecuteBoard(ctx Context, id string) (*BoardExecutionStatus, error)`

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
    BoardId      string                `json:"board_id"`
    Status       string                `json:"status"` // running|completed|failed|cancelled
    EdgeStatuses []EdgeExecutionStatus `json:"edge_statuses"`
    StartTime    time.Time             `json:"start_time"`
    EndTime      *time.Time            `json:"end_time,omitempty"`
}

type EdgeExecutionStatus struct {
    EdgeId    string     `json:"edge_id"`
    Status    string     `json:"status"`
    TaskId    int        `json:"task_id,omitempty"`
    Message   string     `json:"message,omitempty"`
    StartTime *time.Time `json:"start_time,omitempty"`
    EndTime   *time.Time `json:"end_time,omitempty"`
}
```

---

#### `GetExecutionLogs(ctx Context) []string`

Get board execution log entries.

---

#### `ClearExecutionLogs(ctx Context)`

Clear board execution logs.

---

#### `OnRemoteDeleted(remoteName string) error`

Handle remote deletion by cleaning up affected board nodes.

---

## FlowService

Service for managing flows (named groups of sync operations).

### Methods

#### `GetFlows(ctx Context) ([]Flow, error)`

Get all flows.

---

#### `SaveFlows(ctx Context, flows []Flow) error`

Save all flows (replaces existing).

---

#### `OnRemoteDeleted(ctx Context, remoteName string) error`

Handle remote deletion by cleaning up affected flow operations.

---

## OperationService

Service for file operations beyond sync.

### File Operations

#### `Copy(ctx Context, profile Profile, tabId string) (int, error)`

Copy files/directories. Returns task ID.

---

#### `Move(ctx Context, profile Profile, tabId string) (int, error)`

Move files/directories. Returns task ID.

---

#### `CheckFiles(ctx Context, profile Profile, tabId string) (int, error)`

Check for differences between source and dest. Returns task ID.

---

#### `DryRun(ctx Context, action string, profile Profile, tabId string) (int, error)`

Perform a dry run of a sync operation. Returns task ID.

---

#### `StopOperation(ctx Context, taskId int) error`

Stop a running file operation.

---

#### `GetActiveTasks(ctx Context) (map[int]*OperationTask, error)`

Get all currently active operation tasks.

---

### File Browsing

#### `ListFiles(ctx Context, remotePath string, recursive bool) ([]FileEntry, error)`

List files in a remote path.

---

#### `DeleteFile(ctx Context, remotePath string) error`

Delete a file.

---

#### `PurgeDir(ctx Context, remotePath string) error`

Purge a directory (delete including contents).

---

#### `MakeDir(ctx Context, remotePath string) error`

Create a directory.

---

### Storage Info

#### `GetAbout(ctx Context, remoteName string) (*QuotaInfo, error)`

Get storage quota information.

---

#### `GetSize(ctx Context, remotePath string) (int64, int64, error)`

Get size of a path. Returns (total bytes, file count, error).

---

## CryptService

Service for encrypted remote management.

### Methods

#### `CreateCryptRemote(ctx Context, cfg CryptRemoteConfig) error`

Create an encrypted remote.

**Parameters:**
```go
type CryptRemoteConfig struct {
    Name             string `json:"name"`
    WrappedRemote    string `json:"wrapped_remote"`
    Password         string `json:"password"`
    Password2        string `json:"password2"`
    FilenameEncrypt  string `json:"filename_encrypt"`
    DirectoryEncrypt bool   `json:"directory_encrypt"`
}
```

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

#### `SetEnabled(ctx Context, enabled bool)`

Enable/disable notifications.

---

#### `IsEnabled(ctx Context) bool`

Check if notifications are enabled.

---

#### `SetDebugMode(ctx Context, enabled bool)`

Enable/disable debug mode.

---

#### `IsDebugMode(ctx Context) bool`

Check if debug mode is enabled.

---

#### `SetMinimizeToTray(ctx Context, enabled bool)`

Set minimize to tray behavior.

---

#### `IsMinimizeToTray(ctx Context) bool`

Check if minimize to tray is enabled.

---

#### `SetMinimizeToTrayOnStartup(ctx Context, enabled bool)`

Set minimize to tray on startup behavior.

---

#### `IsMinimizeToTrayOnStartup(ctx Context) bool`

Check if minimize to tray on startup is enabled.

---

#### `SetStartAtLogin(ctx Context, enabled bool) error`

Set start at login preference.

---

#### `IsStartAtLogin(ctx Context) bool`

Check if start at login is enabled.

---

#### `GetSettings(ctx Context) AppSettings`

Get all app settings.

**Returns:**
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

#### `LoadSettings()`

Load settings from database (called internally after unlock).

---

## LogService

Service for reliable log delivery.

### Methods

#### `Log(tabId, message, level string) uint64`

Log a message with level (debug, info, warning, error). Returns sequence number.

---

#### `LogSync(tabId, action, status, message string) uint64`

Log a sync event. Returns sequence number.

---

#### `GetLogsSince(ctx Context, tabId string, afterSeqNo uint64) ([]LogEntry, error)`

Get logs after a sequence number.

---

#### `GetLatestLogs(ctx Context, tabId string, count int) ([]LogEntry, error)`

Get latest N logs for a tab.

---

#### `GetCurrentSeqNo(ctx Context) (uint64, error)`

Get current sequence number.

---

#### `ClearLogs(ctx Context, tabId string) error`

Clear logs for a tab.

---

#### `GetBufferSize(ctx Context) (int, error)`

Get current log buffer size.

---

## ExportService

Service for configuration export.

### Methods

#### `GetExportPreview(ctx Context, options ExportOptions) (*ExportManifest, error)`

Preview what will be exported.

---

#### `ExportToBytes(ctx Context, options ExportOptions) ([]byte, error)`

Export to binary format (.nsd).

---

#### `ExportToFile(ctx Context, path string, options ExportOptions) error`

Export to a file.

---

#### `SelectExportFile(ctx Context) (string, error)`

Open a save dialog and return the selected file path.

---

#### `ExportWithDialog(ctx Context, options ExportOptions) (string, error)`

Export with file picker dialog. Returns the saved file path.

---

**Export Options:**
```go
type ExportOptions struct {
    IncludeBoards   bool   `json:"include_boards"`
    IncludeRemotes  bool   `json:"include_remotes"`
    IncludeSettings bool   `json:"include_settings"`
    ExcludeTokens   bool   `json:"exclude_tokens"`    // Export remotes without sensitive tokens
    EncryptPassword string `json:"encrypt_password"`   // If set, encrypt the export file
}
```

**Export Manifest:**
```go
type ExportManifest struct {
    Version     string    `json:"version"`
    AppVersion  string    `json:"app_version"`
    ExportDate  time.Time `json:"export_date"`
    BoardCount  int       `json:"board_count"`
    RemoteCount int       `json:"remote_count"`
    Checksum    uint32    `json:"checksum"`
}
```

---

## ImportService

Service for configuration import.

### Methods

#### `ValidateImportFile(ctx Context, filePath string) (*ImportPreview, error)`

Validate an import file and return a preview. If file is encrypted, returns preview with `Encrypted=true`.

---

#### `ValidateImportFileWithPassword(ctx Context, filePath, password string) (*ImportPreview, error)`

Validate an encrypted import file with a password.

---

#### `ImportFromFile(ctx Context, filePath string, options ImportOptions) (*ImportResult, error)`

Import from a file.

---

#### `ImportFromBytes(ctx Context, data []byte, options ImportOptions) (*ImportResult, error)`

Import from binary data.

---

#### `SelectImportFile(ctx Context) (string, error)`

Open a file dialog and return the selected file path.

---

#### `PreviewWithDialog(ctx Context) (*ImportPreview, string, error)`

Preview import with file picker dialog. Returns (preview, filePath, error).

---

#### `ImportWithDialog(ctx Context, options ImportOptions) (*ImportResult, error)`

Open a file dialog and import from the selected file.

---

**Import Options:**
```go
type ImportOptions struct {
    OverwriteBoards  bool   `json:"overwrite_boards"`  // Overwrite existing boards with same name
    OverwriteRemotes bool   `json:"overwrite_remotes"` // Overwrite existing remotes with same name
    MergeMode        bool   `json:"merge_mode"`        // Add new items only, skip existing
    Password         string `json:"password"`           // Password for encrypted backups
}
```

**Import Preview:**
```go
type ImportPreview struct {
    Valid     bool                  `json:"valid"`
    Encrypted bool                 `json:"encrypted"`
    Manifest  *ExportManifest      `json:"manifest,omitempty"`
    Boards    *ImportPreviewSection `json:"boards,omitempty"`
    Remotes   *ImportPreviewSection `json:"remotes,omitempty"`
    Warnings  []string             `json:"warnings"`
    Errors    []string             `json:"errors"`
}

type ImportPreviewSection struct {
    ToAdd    []string `json:"to_add"`
    ToUpdate []string `json:"to_update"`
    ToSkip   []string `json:"to_skip"`
    Total    int      `json:"total"`
}
```

**Import Result:**
```go
type ImportResult struct {
    Success        bool     `json:"success"`
    BoardsAdded    int      `json:"boards_added"`
    BoardsUpdated  int      `json:"boards_updated"`
    BoardsSkipped  int      `json:"boards_skipped"`
    RemotesAdded   int      `json:"remotes_added"`
    RemotesUpdated int      `json:"remotes_updated"`
    RemotesSkipped int      `json:"remotes_skipped"`
    Warnings       []string `json:"warnings"`
    Errors         []string `json:"errors"`
}
```

---

## Data Models

### Profile

```go
type Profile struct {
    Name               string   `json:"name"`
    From               string   `json:"from"`
    To                 string   `json:"to"`
    IncludedPaths      []string `json:"included_paths"`
    ExcludedPaths      []string `json:"excluded_paths"`
    Bandwidth          int      `json:"bandwidth"`           // MB/s limit (0 = unlimited)
    Parallel           int      `json:"parallel"`            // Concurrent transfers
    BackupPath         string   `json:"backup_path"`
    CachePath          string   `json:"cache_path"`
    MinSize            string   `json:"min_size,omitempty"`
    MaxSize            string   `json:"max_size,omitempty"`
    FilterFromFile     string   `json:"filter_from_file,omitempty"`
    ExcludeIfPresent   string   `json:"exclude_if_present,omitempty"`
    UseRegex           bool     `json:"use_regex,omitempty"`
    MaxAge             string   `json:"max_age,omitempty"`
    MinAge             string   `json:"min_age,omitempty"`
    MaxDepth           *int     `json:"max_depth,omitempty"`
    DeleteExcluded     bool     `json:"delete_excluded,omitempty"`
    MaxDelete          *int     `json:"max_delete,omitempty"`
    Immutable          bool     `json:"immutable,omitempty"`
    ConflictResolution string   `json:"conflict_resolution,omitempty"`
    DryRun             bool     `json:"dry_run,omitempty"`
    MaxTransfer        string   `json:"max_transfer,omitempty"`
    MaxDeleteSize      string   `json:"max_delete_size,omitempty"`
    Suffix             string   `json:"suffix,omitempty"`
    SuffixKeepExt      bool     `json:"suffix_keep_extension,omitempty"`
    MultiThreadStreams *int     `json:"multi_thread_streams,omitempty"`
    BufferSize         string   `json:"buffer_size,omitempty"`
    FastList           bool     `json:"fast_list,omitempty"`
    Retries            *int     `json:"retries,omitempty"`
    LowLevelRetries    *int     `json:"low_level_retries,omitempty"`
    MaxDuration        string   `json:"max_duration,omitempty"`
    CheckFirst         bool     `json:"check_first,omitempty"`
    OrderBy            string   `json:"order_by,omitempty"`
    RetriesSleep       string   `json:"retries_sleep,omitempty"`
    TpsLimit           *float64 `json:"tps_limit,omitempty"`
    ConnTimeout        string   `json:"conn_timeout,omitempty"`
    IoTimeout          string   `json:"io_timeout,omitempty"`
    SizeOnly           bool     `json:"size_only,omitempty"`
    UpdateMode         bool     `json:"update_mode,omitempty"`
    IgnoreExisting     bool     `json:"ignore_existing,omitempty"`
    DeleteTiming       string   `json:"delete_timing,omitempty"`
    Resilient          bool     `json:"resilient,omitempty"`
    MaxLock            string   `json:"max_lock,omitempty"`
    CheckAccess        bool     `json:"check_access,omitempty"`
    ConflictLoser      string   `json:"conflict_loser,omitempty"`
    ConflictSuffix     string   `json:"conflict_suffix,omitempty"`
}
```

### ConfigInfo

```go
type ConfigInfo struct {
    WorkingDir           string        `json:"working_dir"`
    SelectedProfileIndex uint          `json:"selected_profile_index"`
    Profiles             []Profile     `json:"profiles"`
    EnvConfig            config.Config `json:"env_config"`
}
```

### ScheduleEntry

```go
type ScheduleEntry struct {
    Id          string     `json:"id"`
    ProfileName string     `json:"profile_name"`
    Action      string     `json:"action"`       // pull|push|bi|bi-resync|copy|move
    CronExpr    string     `json:"cron_expr"`
    Enabled     bool       `json:"enabled"`
    LastRun     *time.Time `json:"last_run,omitempty"`
    NextRun     *time.Time `json:"next_run,omitempty"`
    LastResult  string     `json:"last_result,omitempty"` // success|failed|cancelled
    CreatedAt   time.Time  `json:"created_at"`
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
    Id         string  `json:"id"`
    SourceId   string  `json:"source_id"`
    TargetId   string  `json:"target_id"`
    Action     string  `json:"action"`
    SyncConfig Profile `json:"sync_config"`
}

type Board struct {
    Id              string      `json:"id"`
    Name            string      `json:"name"`
    Description     string      `json:"description,omitempty"`
    Nodes           []BoardNode `json:"nodes"`
    Edges           []BoardEdge `json:"edges"`
    CreatedAt       time.Time   `json:"created_at"`
    UpdatedAt       time.Time   `json:"updated_at"`
    ScheduleEnabled bool        `json:"schedule_enabled"`
    CronExpr        string      `json:"cron_expr,omitempty"`
    LastRun         *time.Time  `json:"last_run,omitempty"`
    NextRun         *time.Time  `json:"next_run,omitempty"`
    LastResult      string      `json:"last_result,omitempty"`
}
```

### Flow / Operation

```go
type Flow struct {
    Id              string      `json:"id"`
    Name            string      `json:"name"`
    IsCollapsed     bool        `json:"is_collapsed"`
    ScheduleEnabled bool        `json:"schedule_enabled"`
    CronExpr        string      `json:"cron_expr,omitempty"`
    SortOrder       int         `json:"sort_order"`
    Operations      []Operation `json:"operations"`
    CreatedAt       string      `json:"created_at,omitempty"`
    UpdatedAt       string      `json:"updated_at,omitempty"`
}

type Operation struct {
    Id           string  `json:"id"`
    FlowId       string  `json:"flow_id"`
    SourceRemote string  `json:"source_remote"`
    SourcePath   string  `json:"source_path"`
    TargetRemote string  `json:"target_remote"`
    TargetPath   string  `json:"target_path"`
    Action       string  `json:"action"`
    SyncConfig   Profile `json:"sync_config"`
    IsExpanded   bool    `json:"is_expanded"`
    SortOrder    int     `json:"sort_order"`
}
```

### HistoryEntry

```go
type HistoryEntry struct {
    Id               string    `json:"id"`
    ProfileName      string    `json:"profile_name"`
    Action           string    `json:"action"`
    Status           string    `json:"status"`
    StartTime        time.Time `json:"start_time"`
    EndTime          time.Time `json:"end_time"`
    Duration         string    `json:"duration"`
    FilesTransferred int64     `json:"files_transferred"`
    BytesTransferred int64     `json:"bytes_transferred"`
    Errors           int       `json:"errors"`
    ErrorMessage     string    `json:"error_message,omitempty"`
}
```

### FileEntry

```go
type FileEntry struct {
    Path     string `json:"path"`
    Name     string `json:"name"`
    Size     int64  `json:"size"`
    ModTime  string `json:"mod_time"`
    IsDir    bool   `json:"is_dir"`
    MimeType string `json:"mime_type,omitempty"`
}
```

### QuotaInfo

```go
type QuotaInfo struct {
    Total   int64 `json:"total"`
    Used    int64 `json:"used"`
    Free    int64 `json:"free"`
    Trashed int64 `json:"trashed,omitempty"`
}
```

### Tab

```go
type Tab struct {
    Id            string    `json:"id"`
    Name          string    `json:"name"`
    Profile       *Profile  `json:"profile"`
    State         TabState  `json:"state"`
    CurrentAction string    `json:"current_action"`
    TaskId        int       `json:"task_id"`
    Output        []string  `json:"output"`
    CreatedAt     time.Time `json:"created_at"`
    UpdatedAt     time.Time `json:"updated_at"`
    LastError     string    `json:"last_error"`
}
```

**Tab States:** `Running`, `Stopped`, `Completed`, `Failed`, `Cancelled`

### FrontendLogEntry

```go
type FrontendLogEntry struct {
    Level       string    `json:"level"`
    Message     string    `json:"message"`
    Context     string    `json:"context,omitempty"`
    Details     string    `json:"details,omitempty"`
    Timestamp   time.Time `json:"timestamp"`
    BrowserInfo string    `json:"browser_info,omitempty"`
    UserAgent   string    `json:"user_agent,omitempty"`
    StackTrace  string    `json:"stack_trace,omitempty"`
    URL         string    `json:"url,omitempty"`
    Component   string    `json:"component,omitempty"`
    TraceID     string    `json:"trace_id,omitempty"`
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

### Using Export/Import

```typescript
import { ExportService } from "wailsjs/desktop/backend/services/exportservice";
import { ImportService } from "wailsjs/desktop/backend/services/importservice";

// Export with dialog
const filePath = await ExportService.ExportWithDialog({
    include_boards: true,
    include_remotes: true,
    include_settings: false,
    exclude_tokens: true,
    encrypt_password: "my-secret",
});

// Import with preview
const [preview, path] = await ImportService.PreviewWithDialog();
if (preview?.encrypted) {
    // File is encrypted, validate with password
    const decryptedPreview = await ImportService.ValidateImportFileWithPassword(path, password);
}
```
