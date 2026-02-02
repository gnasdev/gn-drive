# Backend API Reference

This document describes the Go backend services API exposed to the frontend via Wails bindings.

## App Service

Legacy service for sync operations and configuration management.

### Sync Methods

#### `Sync(task string, profile Profile) int`

Start a sync operation without tab association.

**Parameters:**
- `task`: Sync action type - `"pull"`, `"push"`, `"bi"`, `"bi-resync"`
- `profile`: Profile configuration

**Returns:** Task ID (int) for tracking

**Example:**
```typescript
const taskId = await Sync("pull", profile);
```

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

**Parameters:**
- `id`: Task ID to stop

---

### Configuration Methods

#### `GetConfigInfo() ConfigInfo`

Get current configuration including profiles.

**Returns:**
```typescript
interface ConfigInfo {
    working_dir: string;
    selected_profile_index: number;
    profiles: Profile[];
    env_config: EnvConfig;
}
```

---

#### `UpdateProfiles(profiles Profile[]) AppError | null`

Update all profiles.

**Parameters:**
- `profiles`: Array of profiles to save

**Returns:** null on success, AppError on failure

---

### Remote Methods

#### `GetRemotes() Remote[]`

Get list of configured cloud remotes.

**Returns:** Array of rclone Remote objects

---

#### `AddRemote(name string, type string, config map[string]string) AppError | null`

Add a new cloud remote.

**Parameters:**
- `name`: Remote name (alphanumeric, dash, underscore only)
- `type`: Remote type (e.g., "drive", "dropbox", "onedrive")
- `config`: Additional configuration parameters

**Returns:** null on success, AppError on failure

Supported remote types:
- `drive` - Google Drive
- `dropbox` - Dropbox
- `onedrive` - OneDrive
- `box` - Box
- `yandex` - Yandex Disk
- `gphotos` - Google Photos
- `iclouddrive` - iCloud Drive (requires manual setup)

---

#### `DeleteRemote(name string) AppError | null`

Delete a remote and associated profiles.

**Parameters:**
- `name`: Remote name to delete

**Returns:** null on success, AppError on failure

**Note:** This will cascade delete any profiles using this remote.

---

#### `StopAddingRemote() AppError | null`

Cancel an in-progress OAuth flow.

---

## SyncService

Service for managing sync operations with context support.

### Methods

#### `StartSync(ctx Context, action string, profile Profile, tabId string) (SyncResult, error)`

Start a sync operation with context cancellation support.

**Parameters:**
- `ctx`: Context for cancellation
- `action`: Sync action type
- `profile`: Profile configuration
- `tabId`: Tab identifier

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
    backup_path: string;    // Unused
    cache_path: string;     // Unused
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

### EnvConfig

```typescript
interface EnvConfig {
    profile_file_path: string;  // Path to profiles.json
    rclone_file_path: string;   // Path to rclone.conf
    debug_mode: boolean;
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
