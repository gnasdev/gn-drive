# GN Drive stdio RPC — protocol v1

Status: **specification only**. `gn-drive serve --stdio` does not exist yet; this
document is the contract for the Swift implementer and for the Rust GPUI window
(`apps/gn-drive-gpui`).

The engine stays in Swift. The Rust window spawns the CLI as a child process and
talks newline-delimited JSON over its stdin/stdout. There is no loopback HTTP
server, no SSE, and no WebSocket: this replaces the legacy Go API and the
in-process SwiftUI `AppState` bridge.

The parity checklist for every behavior described here is
[`use-cases.md`](./use-cases.md). Every `Engine:` reference in that document must
resolve to a method in this file; the coverage matrix in §13 proves it.

---

## 0. Quick reference

- Transport: one compact JSON object per line (`\n`), UTF-8, on stdio.
- Request: `{"id": <number|string>, "method": "...", "params": {...}}`
- Success: `{"id": <id>, "result": {...}}`
- Failure: `{"id": <id>, "error": {"code": "...", "message": "...", "data": {...}}}`
- Push: `{"event": "<name>", "ts": <unix-ms>, "data": {...}}`
- First line is `hello` (or `fatal` if startup fails).
- stdout carries protocol lines only. Logs go to stderr.
- One engine instance per config dir, enforced by `InstanceLocker`.

Method index. Gate: **A** = always available, **L** = available while locked,
**D** = needs an unlocked data plane (or no master password configured).

| Method | Gate | Purpose |
|---|---|---|
| `ping` | A | liveness / protocol check |
| `state.snapshot` | L | hydrate the whole window (auth + data) |
| `app.quit` | A | graceful shutdown |
| `doctor` | L | structured diagnostics |
| `logs.read` | A | read the bridge log buffer |
| `auth.status` | A | setup/unlocked/lockout |
| `auth.setup` | A | create the master password |
| `auth.unlock` | A | decrypt and open the data plane |
| `auth.lock` | L | close the data plane and re-encrypt |
| `auth.changePassword` | L | rotate hash/key (stays unlocked) |
| `auth.removePassword` | L | permanently decrypt and delete `auth.json` |
| `flows.list` | D | all flows with operations |
| `flows.get` | D | one flow |
| `flows.create` | D | create an empty flow |
| `flows.save` | D | upsert a flow + operations + canvas |
| `flows.delete` | D | delete a flow (rejects while running) |
| `flows.execute` | D | start sequential execution |
| `flows.stop` | D | request cancellation |
| `operations.create` | D | append an operation |
| `operations.update` | D | patch an operation |
| `operations.delete` | D | remove an operation |
| `operations.reorder` | D | rewrite `sortOrder` |
| `runtime.snapshot` | D | runtime projection (statuses + progress + logs) |
| `runtime.tasks` | D | active `SyncTask` snapshots |
| `remotes.providers` | L | rclone backend schema |
| `remotes.list` | D | configured remotes |
| `remotes.add` | D | non-interactive remote creation |
| `remotes.test` | D | `lsd` connectivity probe |
| `remotes.delete` | D | delete a remote |
| `remotes.oauth.start` | D | begin `rclone authorize` |
| `remotes.oauth.complete` | D | finish creation with the token |
| `remotes.oauth.cancel` | D | abort an OAuth session |
| `fs.list` | D | one-level or recursive listing |
| `profiles.list` | D | sync profiles |
| `profiles.get` | D | one profile |
| `profiles.save` | D | upsert a profile |
| `profiles.delete` | D | delete a profile |
| `schedules.list` | D | legacy profile schedules |
| `sync.profile` | D | one-shot profile sync |
| `sync.stop` | D | cancel a one-shot sync |
| `boards.list` | D | legacy board metadata |
| `boards.get` | D | board + graph |
| `boards.save` | D | upsert board graph |
| `boards.delete` | D | delete a board |
| `boards.execute` | D | run a board DAG |
| `boards.stop` | D | cancel a board run |
| `boards.status` | D | board run status |
| `history.list` | D | run history |
| `history.stats` | D | aggregate history |
| `history.clear` | D | clear history |
| `settings.get` | L | app settings from `auth.json` |
| `settings.set` | L | patch app settings |
| `service.status` | A | launchd state |
| `service.install` | A | install the launchd agent |
| `service.uninstall` | A | remove it |
| `service.start` | A | `launchctl kickstart` |
| `service.stop` | A | `launchctl bootout` |
| `service.restart` | A | stop + start |
| `service.health` | A | read `service.health` |
| `update.check` | A | query GitHub releases |
| `update.apply` | A | download + replace the binary |

Naming rule: property names follow the Swift model **verbatim**, including
acronym casing — `flowID`, `opID`, `taskID`, `profileID`, `boardID`, `nodeID`,
`edgeID`, `scheduleID`, `sourceID`, `targetID`, `canvasJSON`. On input the
bridge also accepts the lower-cased aliases (`flowId`, `sourceId`, …) and always
emits the Swift spelling.

---

## 1. Transport and framing

### 1.1 Process

```
gn-drive serve --stdio [--config-dir PATH] [--rclone PATH]
                        [--log-level debug|info|warn|error]
                        [--log-buffer N] [--password PW]
```

- `--stdio` is required in v1. Without it, print an error to stderr and exit 2.
- `--config-dir` overrides `Paths.detect().configDir`
  (`Sources/GNDriveCore/Paths.swift:21-38`).
- `--rclone` overrides binary resolution; default is the bundled sibling
  `rclone`, then `PATH` (`RcloneClient.resolveBinary`,
  `Sources/GNDriveCore/RcloneClient.swift:80-97`).
- `--log-level` filters the stderr logger and `log:line` pushes; default `info`.
- `--log-buffer` is the in-memory ring size for `logs.read`; default 2000.
- `--password` (or `GN_DRIVE_PASSWORD`) is for automation only. Interactive
  windows must start locked and call `auth.unlock`, so the remembered-key
  Keychain behavior matches the SwiftUI app.

stdout is reserved for protocol frames. Every diagnostic must go to stderr —
including code that currently prints to stdout, notably
`UpdateOptions.log` (`Sources/GNDriveCore/SelfUpdate.swift:53`), which the serve
command must override with a stderr logger.

### 1.2 Frames

- Encoding: UTF-8, no BOM.
- Exactly one JSON value per line, terminated by `\n`. No pretty printing, no
  embedded raw newlines.
- Requests larger than 1 MiB are rejected with `invalid_request` and the rest
  of the line is discarded.
- Responses larger than 8 MiB are a bug; `logs.read` is capped so this cannot
  happen in normal use.
- `id` is required and may be a number or a string. It is echoed verbatim.
- Responses may arrive out of order. Clients correlate on `id`.
- Unknown request fields are ignored. Unknown params are ignored unless the
  method validates them.
- A request with an unknown method still returns a normal error response; the
  connection stays open.

Request:

```json
{"id":7,"method":"flows.get","params":{"id":"F1"}}
```

Success:

```json
{"id":7,"result":{"flow":{"id":"F1","name":"Nightly","scheduleEnabled":true,
  "operations":[],"canvasJSON":"{}","createdAt":"2026-10-22 03:00:00",
  "updatedAt":"2026-10-22 03:00:00"}}}
```

Abbreviated: the full `Flow` shape is §6.4.

Failure:

```json
{"id":7,"error":{"code":"flow_not_found","message":"flow F1 not found"}}
```

Push:

```json
{"event":"sync:progress","ts":1729581234567,"data":{"taskID":"T1","profileID":"F1:O1","action":"push","state":"running","transferred":1024,"total":4096,"bytesPerSec":512.0,"eta":6,"errors":0,"currentFile":"a.bin","stage":"transferring","stageDetail":"","filesTransferred":1,"totalFiles":4,"checks":0,"totalChecks":0,"deletes":0,"renames":0,"transfers":[],"errorMessage":""}}
```

- `ts` is Unix milliseconds, set by the bridge. Clients must ignore it if
  absent.
- `data` is always an object. Event-specific fields are defined in §9.
- The bridge never batches multiple frames onto one line.

### 1.3 Ordering

- `hello` is written before any other frame. The only exception is a startup
  failure, where `fatal` is the first (and last) frame.
- `auth:unlocked` / `auth:locked` are written after the auth state has changed
  and the data plane has been opened/closed.
- `state:changed` is written after persistence.
- Terminal sync events are written before the history row is finalized (see
  §9.4); a client that fetches `history.list` immediately after
  `sync:completed` may still see the `running` row for a few milliseconds.

---

## 2. Lifecycle and handshake

### 2.1 Startup sequence

1. Build a foreground `Logger` writing to stderr. Never write to stdout.
2. Reject a non-stdio invocation (`unsupported_transport` to stderr, exit 2).
3. **Detect a running GN Drive UI.** Because the current SwiftUI app does not
   take the instance lock (see §2.3), check `/usr/bin/pgrep -x GNDriveApp`
   first. If found, emit a `fatal` frame with `instance_busy`, `holder:"app"`,
   then exit 3.
4. Acquire `InstanceLocker.acquire(configDir:)`
   (`Sources/GNDriveCore/InstanceLock.swift:28-44`). On
   `InstanceError.anotherInstance(pid:)`, classify the holder (§2.2) and emit a
   `fatal` frame, then exit 3.
5. Create `AppContainer` with:

   ```swift
   var opts = AppContainer.Options()
   opts.portalMode = true
   opts.keyStore = KeychainStore(service: "gn-drive")
   opts.version = BuildVersion.current
   opts.logMode = .foreground
   opts.configDir = <flag or "">
   opts.rcloneBinary = <flag or "">
   ```

   `portalMode` keeps the process alive while locked and defers the data plane
   until `afterUnlock()` (`Sources/GNDriveCore/AppContainer.swift:50-85`).
   A Keychain-remembered key may already have resumed the session; `hello`
   reports the resulting state.

6. Call `container.syncEngine.start()`
   (`Sources/GNDriveCore/SyncEngine.swift:124`).
7. Subscribe to `BusTopic.all` plus a runtime-snapshot debouncer (§9.6).
8. Write the `hello` frame.
9. Start reading stdin. If stdin is a TTY, log a hint to stderr but continue
   (useful for manual testing).

Startup failures before `hello`:

| Condition | `fatal.data.code` | Exit |
|---|---|---|
| another instance / SwiftUI app | `instance_busy` | 3 |
| `AppContainer` throws `AppError.rcloneMissing` | `rclone_missing` | 2 |
| `AppContainer` throws `AppError.locked` (should not happen in portal mode) | `locked` | 2 |
| protocol flag unsupported | `unsupported_protocol` | 2 |
| unexpected throw | `init_failed` | 2 |

`fatal` is a server push (no `id`). It is the only frame the server may emit
before `hello`.

### 2.2 Instance conflict

`fatal` payload:

```json
{"event":"fatal","ts":1729581234567,"data":{
  "code":"instance_busy",
  "message":"another gn-drive instance is running (pid=5123, holder=service). Stop it with 'gn-drive service stop' or close the other window.",
  "pid":5123,
  "holder":"app|service|window|cli"
}}
```

Classification:

- `pid` from the `gn-drive.pid` file via `InstanceError.anotherInstance`.
- Read `ps -p <pid> -o command=`:
  - contains `--service` → `service`
  - contains `serve` → `window`
  - otherwise → `cli`
- `pgrep -x GNDriveApp` hit (before the lock) → `app`.

The Rust window must surface this as a blocking error with the message and a
"Retry" action. It must not treat it as a crash.

**Required companion change:** the SwiftUI app must also acquire the same
`InstanceLocker` at startup. Today only `gn-drive run`
(`Sources/gn-drive/Commands.swift:41-48`) does. Until the app is changed, serve
relies on the `pgrep` fallback; that is racy and should be considered temporary.

### 2.3 Shutdown

Triggers:

- stdin reaches EOF (parent window closed/crashed).
- `SIGINT` or `SIGTERM`.
- `app.quit`.

Order:

1. Stop accepting new requests; let in-flight requests finish for up to 5s.
2. `container.syncEngine.stop()` — cancels active sync tasks and stops cron
   (`Sources/GNDriveCore/SyncEngine.swift:134`).
3. `container.close()` — stops the health writer, closes RuntimeHub and the
   bus, detaches engines, and calls `auth.suspend()`, which re-encrypts config
   files while keeping the Keychain key (`Sources/GNDriveCore/AppContainer.swift:134-140`,
   `AuthService.swift:243-246`).
4. `locker.release()` (`InstanceLock.swift:52-62`).
5. Emit `{"event":"app:shutdown","data":{"reason":"eof|signal|request"}}`.
6. Flush stdout and exit 0. On a 5s timeout, exit 0 anyway after logging to
   stderr.

Ignore `SIGPIPE` and treat a write failure (`EPIPE`) as a shutdown trigger.

### 2.4 Handshake (`hello`)

The first frame is:

```json
{"event":"hello","ts":1729581234567,"data":{
  "protocol":1,
  "appVersion":"0.4.0",
  "engine":"GNDriveCore",
  "pid":4312,
  "setup":true,
  "unlocked":false,
  "passwordRequired":true,
  "platform":{"os":"macos","arch":"arm64"},
  "paths":{
    "configDir":"/Users/me/.config/gn-drive",
    "dbPath":"/Users/me/.config/gn-drive/gn-drive.db",
    "rcloneConfigPath":"/Users/me/.config/gn-drive/rclone.conf",
    "logDir":"/Users/me/.config/gn-drive",
    "workingDir":"/Users/me/Applications/GNDrive.app/Contents/MacOS"
  },
  "rclone":{"path":"/Users/me/Applications/GNDrive.app/Contents/MacOS/rclone",
            "version":"rclone v1.75.0","bundled":true},
  "capabilities":{
    "oauth":true,"providers":true,"profiles":true,"boards":true,
    "service":true,"update":true,"logs":true,"history":true,"doctor":true
  },
  "instance":{"pid":4312,"lockPath":"/Users/me/.config/gn-drive/gn-drive.lock"}
}}
```

- `passwordRequired` = `setup && !unlocked`.
- `rclone.version` is `RcloneClient.version()` (`RcloneClient.swift:99-103`);
  omit the field (or use `null`) if rclone is unavailable.
- `bundled` = resolved path equals `RcloneClient.bundledBinary()`
  (`RcloneClient.swift:68-78`).
- `capabilities` is additive; clients must ignore unknown keys.

After `hello`, the window either shows the unlock screen
(`passwordRequired == true`) or calls `state.snapshot`.

---

## 3. Versioning

- `protocol` is an integer. This document is version 1.
- Additive methods, result fields, event names, and error codes are
  backward-compatible within v1.
- Removing/renaming a field or changing its meaning requires protocol 2.
- A request may include `"protocol":1`; a mismatch is rejected with
  `unsupported_protocol`.
- Clients must tolerate unknown fields and unknown event names.

---

## 4. Errors

### 4.1 Shape

```json
{"id":3,"error":{
  "code":"invalid_password",
  "message":"auth: invalid password",
  "data":{"failedAttempts":2}
}}
```

- `message` is human-readable and safe to show (after the client's own
  localization). It is never a secret and never includes rclone config values.
- `data` is optional and method-specific.

### 4.2 Codes

Transport / protocol:

| Code | Meaning |
|---|---|
| `parse_error` | line is not valid JSON |
| `invalid_request` | missing/duplicate id, missing method, oversized line |
| `unknown_method` | no such method |
| `invalid_params` | wrong type / missing required field (`data.field`) |
| `unsupported_protocol` | protocol mismatch |
| `unsupported` | capability not available on this build/platform |
| `internal_error` | unexpected Swift error |
| `init_failed` | startup failure before serving |
| `instance_busy` | fatal only |
| `rclone_missing` | binary not found |
| `rclone_failed` | rclone exited non-zero (`data.command`, `data.stderr` truncated to 500 chars) |

Auth:

| Code | Swift source |
|---|---|
| `locked` | data-plane method while locked (`data.setup=true`) |
| `not_setup` | `AuthError.notSetup` |
| `already_setup` | `AuthError.alreadySetup` |
| `not_unlocked` | `AuthError.notUnlocked` |
| `invalid_password` | `AuthError.invalidPassword` |
| `password_too_short` | `AuthError.passwordTooShort` |
| `locked_out` | `AuthError.locked(retryAfterSecs:)`; `data.retryAfterSecs` |

Flows / operations:

| Code | Source |
|---|---|
| `flow_not_found` | `StoreError.notFound` / `FlowEngine` load |
| `flow_exists` | `flows.create` id collision |
| `flow_busy` | save/delete while `running` or `cancelling` |
| `flow_empty` | `FlowEngineError.emptyFlow` |
| `already_running` | `FlowEngineError.alreadyRunning` |
| `not_running` | `FlowEngineError.notRunning` |
| `engine_not_ready` | `FlowEngineError.engineNotReady` |
| `operation_not_found` | unknown operation id |

Profiles / sync:

| Code | Source |
|---|---|
| `profile_not_found` | `StoreError.notFound` |
| `missing_name` | `StoreError.invalid("profile: name is required")` |
| `invalid_direction` | `StoreError.invalid("profile: invalid direction …")` |
| `profile_busy` | `SyncEngineError.profileBusy` |
| `invalid_action` | unknown rclone action |
| `sync_not_running` | `SyncEngineError.notRunning` |
| `task_not_found` | `SyncEngineError.taskNotFound` |
| `task_failed` | `SyncEngineError.taskFailed`; `data.taskID` |
| `task_cancelled` | `SyncEngineError.taskCancelled` |

Remotes / files:

| Code | Source |
|---|---|
| `remote_not_found` | name not in `listRemotes()` |
| `remote_exists` | `remotes.add` name collision |
| `auth_failed` | `RcloneError.runFailed` during test/create |
| `test_failed` | `RcloneClient.testRemote` failure |
| `oauth_required` | wizard reached an OAuth question |
| `oauth_cancelled` | user/session aborted |
| `oauth_failed` | authorize exited non-zero or token rejected |
| `oauth_session_not_found` | unknown/expired session |
| `oauth_session_active` | another `rclone authorize` session is already running (fixed callback port 53682) |
| `fs_missing_path` | empty or relative path |
| `browse_failed` | `RcloneClient.listFiles` failure |

Boards:

| Code | Source |
|---|---|
| `board_not_found` | `StoreError.notFound` |
| `board_empty` | `BoardEngineError.emptyBoard` |
| `board_cycle` | `topoLayers` cycle message |
| `board_missing_node` | `BoardEngineError.missingNode` |
| `board_busy` | `BoardEngineError.alreadyRunning` |
| `board_not_running` | `BoardEngineError.notRunning` |

Service / update:

| Code | Source |
|---|---|
| `service_not_installed` | `ServiceError.notInstalled` |
| `service_error` | `ServiceError.launchctl` / `permission_denied` |
| `permission_denied` | system-scope install without root |
| `update_failed` | any `SelfUpdateError` not listed below |
| `no_release` | `SelfUpdateError.noRelease` |
| `already_up_to_date` | `SelfUpdateError.alreadyUpToDate` |
| `no_matching_asset` | `SelfUpdateError.noMatchingAsset` |
| `checksum_mismatch` | `SelfUpdateError.checksumMismatch` |

### 4.3 Redaction

Before placing an rclone error in `message`/`data.stderr`:

- Cut at `(stderr:` and strip a leading `rclone: `, mirroring
  `SyncEngine.sanitizeRcloneErr` (`Sources/GNDriveCore/SyncEngine.swift:517+`).
- Redact any `key=value`, `--key value`, or JSON field whose key matches
  `pass|password|token|secret|key|client_secret|config_token|access_token`
  (case-insensitive), except the literal `key` in rclone flag names such as
  `--s3-access-key-id` only when the value follows `=`.
- Never echo an OAuth token or a config dump.

---

## 5. Auth gating

`AuthService` (`Sources/GNDriveCore/AuthService.swift`) always exists; the data
plane (Store + RcloneClient) exists only when `AppContainer.canOpenDataPlane`
is true (`AppContainer.swift:89-92`):

- no password configured → open;
- unlocked → open;
- locked with encrypted `.enc` files → deferred;
- locked but plaintext config → open (`auth.openPlaintextSession` path).

Rules:

- **A/L** methods work while locked.
- **D** methods return:

  ```json
  {"id":4,"error":{"code":"locked","message":"auth: app is locked","data":{"setup":true}}}
  ```

  when `auth.isSetup && !auth.isUnlocked` and the data plane is closed.
- `locked` mirrors `!auth.isUnlocked`, not the data-plane state. A plaintext
  recovery session keeps D methods working while `locked` stays `true`; the
  window must use the `locked` flag for its lock screen and not infer gating
  from data-plane availability.
- `state.snapshot` is **L**: while locked it returns `locked:true`, `auth`,
  `paths`, `appSettings`, and empty data arrays. The window can render the
  unlock screen from it without calling a D method.
- After `auth.unlock`, `auth.setup`, or `auth.removePassword`, the bridge calls
  `container.afterUnlock()` before returning, so a D call immediately after the
  response cannot race the data plane.
- After `auth.lock`, the bridge calls `container.beforeLock()` **before**
  `auth.lock()`, then clears the runtime projection.

---

## 6. Common data types

Unless noted, optional fields are omitted (Swift `encodeIfPresent` semantics) or
`null`; clients must accept both.

### 6.1 Auth

```json
{
  "setup": true,
  "unlocked": false,
  "lockedAt": "2026-10-22T12:00:00Z",
  "enabled": true,
  "lockout": {
    "failedAttempts": 2,
    "lockedUntil": "",
    "isLocked": false,
    "retryAfterSecs": 0
  }
}
```

`AuthStatus` (`AuthService.swift:54-60`), `LockoutStatus` (`:47-52`). `lockedAt`
is `null` when never locked. Timestamps are RFC3339 UTC with `Z`.

### 6.2 App settings

```json
{"notificationsEnabled":false,"debugMode":false,"minimizeToTray":false,"startAtLogin":false}
```

`AppSettings` (`AuthService.swift:15-29`). The on-disk `auth.json` keys remain
`notifications_enabled`, `debug_mode`, `minimize_to_tray`, `start_at_login`;
the RPC always uses camelCase.

### 6.3 Profile

All `Profile` fields (`Sources/GNDriveCore/Models.swift:24-89`) in camelCase:

```json
{
  "name":"photos","from":"/Users/me/Pictures","to":"gdrive:Photos","direction":"push",
  "includedPaths":[],"excludedPaths":[],"bandwidth":0,"parallel":4,
  "backupPath":"","cachePath":"",
  "minSize":"","maxSize":"","filterFromFile":"","excludeIfPresent":"",
  "useRegex":false,"maxAge":"","minAge":"","maxDepth":null,"deleteExcluded":false,
  "maxDelete":null,"immutable":false,"conflictResolution":"","dryRun":false,
  "maxTransfer":"","maxDeleteSize":"","suffix":"","suffixKeepExtension":false,
  "multiThreadStreams":null,"bufferSize":"","retries":null,"lowLevelRetries":null,
  "maxDuration":"","checkFirst":false,"orderBy":"","retriesSleep":"",
  "tpsLimit":null,"connTimeout":"","ioTimeout":"",
  "sizeOnly":false,"updateMode":false,"ignoreExisting":false,
  "deleteTiming":"","resilient":false,"maxLock":"","checkAccess":false,
  "conflictLoser":"","conflictSuffix":"","fastList":false
}
```

`direction` is `push | bi | bi-resync`. Empty normalizes to `push`; anything
else is `invalid_direction` (`Store.saveProfile`, `Store.swift:224-231`).

### 6.4 Flow and operation

```json
{
  "id":"F1","name":"Nightly","isCollapsed":false,
  "scheduleEnabled":true,"scheduleCron":"0 3 * * *","sortOrder":0,
  "operations":[
    {
      "id":"O1","flowID":"F1",
      "sourceRemote":"","sourcePath":"/Users/me/Pictures",
      "targetRemote":"gdrive","targetPath":"/Photos",
      "action":"push",
      "syncConfig":{"action":"push","parallel":4,"dry_run":true,"dryRun":true},
      "isExpanded":false,"sortOrder":0
    }
  ],
  "canvasJSON":"{\"viewport\":{\"x\":0,\"y\":0,\"zoom\":1},\"nodes\":[]}",
  "canvas":{"viewport":{"x":0,"y":0,"zoom":1},"nodes":[]},
  "createdAt":"2026-10-22 03:00:00","updatedAt":"2026-10-22 03:00:00"
}
```

- `Flow` / `FlowOperation` (`Models.swift:199-246`). Dates are the raw SQLite
  strings (`YYYY-MM-DD HH:MM:SS`, UTC), not RFC3339.
- `flowID` is server-owned: ignored on input, always set on output.
- `action` is `push | bi | bi-resync`; empty/invalid normalizes to `push`
  (`FlowAction`, `Models.swift:172-188`). The server also writes the resolved
  action into `syncConfig.action` (`FlowOperation.normalizeAction`).
- `syncConfig` is a JSON **object**. The persisted keys are snake_case
  (`parallel`, `dry_run`, `included_paths`, …) and are documented in
  `Sources/GNDriveApp/SyncConfigEditor.swift:8-57`. The bridge preserves all
  keys, including unknown ones, and normalizes `action`. Clients should send
  snake_case; camelCase aliases are accepted on read
  (`SyncConfigEditor.swift:61-137`). Unlike the SwiftUI editor, the RPC does not
  duplicate every key in both spellings.
- `parallel` semantics differ by path: a `Profile` with `parallel:0` omits
  `--transfers` (rclone's default of 4 applies); operation `syncConfig` with a
  missing or `<=0` `parallel` is coerced to 4 (`FlowEngine.profileFromSyncConfig`,
  `FlowEngine.swift:198-200,247`), so it always passes `--transfers 4`.
- `canvasJSON` is the exact stored string (`src/flows.canvas_json`). On input,
  a client may send `canvas` (object) instead; the bridge serializes it. If
  both are present, `canvasJSON` wins. On output both are present: `canvasJSON`
  verbatim and `canvas` parsed (or `{}`).
- Canvas schema (`Sources/GNDriveApp/FlowGraph.swift:73-108`):

  ```json
  {"viewport":{"x":0,"y":0,"zoom":1},
   "nodes":[{"id":"n_1a2b3c4d","remote":"gdrive","path":"/Photos",
             "label":"","x":280,"y":0}]}
  ```

  Node ids are `n_` + FNV-1a32 of `remote + "\0" + normalizedPath`
  (`FlowGraph.swift:54-71`). Edge ids are operation ids. A stale canvas is
  harmless: the execution engine only reads operations.

### 6.5 Remote

```json
{"name":"gdrive","type":"drive","description":""}
```

`Remote` (`Sources/GNDriveCore/RcloneTypes.swift:169-173`). `description` is
currently always empty because `listRemotes` only reads the type
(`RcloneClient.swift:191-219`); the window should display `name` and `type`.

### 6.6 File entry

```json
{"name":"a.jpg","size":12345,"mimeType":"image/jpeg","isDir":false,
 "modTime":"2026-10-22T03:00:00.000000000Z","path":"a.jpg","id":""}
```

`FileEntry` (`RcloneTypes.swift:143-161`) is decoded from rclone's PascalCase
JSON and re-emitted camelCase.

### 6.7 Sync stats and task

```json
{
  "bytes":1024,"bytesTotal":4096,"files":1,"filesTotal":4,"transfers":1,
  "errors":0,"checks":0,"checksTotal":0,"deletes":0,"renames":0,
  "speed":512.0,"eta":6,"currentFile":"a.bin",
  "stage":"transferring","stageDetail":"","lastUpdate":1729581234000,
  "fileTransfers":[
    {"name":"a.bin","size":4096,"bytes":1024,"progress":25.0,
     "status":"transferring","speed":512.0,"error":""}
  ]
}
```

`SyncStats` (`RcloneTypes.swift:32-52`). File statuses: `pending`,
`transferring`, `checking`, `checked`, `completed`, `failed`. `stage` values
seen from the parser: `starting`, `connecting`, `listing`, `checking`,
`transferring`, `retrying`. At most 150 file rows are tracked
(`RcloneClient.maxTrackedFiles`).

Active task (`SyncTask.Snapshot`, `SyncEngine.swift:53-61`):

```json
{"id":"T1","name":"F1:O1","action":"push","status":"running",
 "stats":{"bytes":1024,"bytesTotal":4096,"files":1,"filesTotal":4,"transfers":1,
          "errors":0,"checks":0,"checksTotal":0,"deletes":0,"renames":0,
          "speed":512.0,"eta":6,"currentFile":"a.bin","stage":"transferring",
          "stageDetail":"","lastUpdate":1729581234000,"fileTransfers":[]},
 "startedAt":"2026-10-22T12:00:00Z","endedAt":null}
```

`name` is the busy key: a profile name for `sync.profile`, or `flowID:opID`
for flow operations. Task timestamps are RFC3339.

`SyncProgressEvent` (`EventBus.swift:55-78`) is the `sync:progress` payload.
It is not nested under `stats`:

```json
{"taskID":"T1","profileID":"F1:O1","action":"push","state":"running",
 "transferred":1024,"total":4096,"bytesPerSec":512.0,"eta":6,"errors":0,
 "currentFile":"a.bin","stage":"transferring","stageDetail":"",
 "filesTransferred":1,"totalFiles":4,"checks":0,"totalChecks":0,
 "deletes":0,"renames":0,
 "transfers":[{"name":"a.bin","size":4096,"bytes":1024,"progress":25.0,
               "status":"transferring","speed":512.0,"error":""}],
 "errorMessage":""}
```

`state` is `running` during the run and the terminal state on the last event.
`transfers` is capped at 150 entries like `SyncStats.fileTransfers`.

### 6.8 Runtime snapshot

```json
{
  "revision": 12,
  "flows": [
    {
      "id":"F1","status":"running","lastError":"",
      "ops":[{"id":"O1","status":"completed","lastError":""}],
      "sync": {"taskID":"T1","profileID":"F1:O1","state":"running","transferred":1024},
      "log":[
        {"at":1729581234000,"status":"running","opID":"","error":"","label":"Flow"},
        {"at":1729581235000,"status":"running","opID":"O1","error":"","label":"Op O1abc1234"}
      ]
    }
  ]
}
```

`RuntimeSnapshotEvent` / `RuntimeFlowState` / `RuntimeOpState` / `RuntimeLogEntry`
(`Sources/GNDriveCore/EventBus.swift:197-247`). Notes:

- `status` is one of `idle | running | cancelling | completed | failed |
  cancelled` (RuntimeHub defaults empty to `idle`, `RuntimeHub.swift:217-226`).
- `ops` contains only operations the hub has seen; a never-run operation is
  absent, not `idle`.
- `sync` is the most recent `SyncProgressEvent` for that flow, or omitted.
- `log` is newest-last, deduplicated, capped at 40 (`RuntimeHub.maxLog`,
  `RuntimeHub.swift:7,228-238`).
- `revision` increases on every hub mutation (`RuntimeHub.swift:48,57,97`).
  Clients replace state only when the new revision is `>=` the current one.

### 6.9 History

```json
{"id":"T1","profileName":"F1:O1","action":"push","state":"completed",
 "startedAt":"2026-10-22T12:00:00Z","finishedAt":"2026-10-22T12:01:00Z",
 "duration":60,"bytes":4096,"errors":0,"files":4,"errorMessage":""}
```

`HistoryEntry` (`Models.swift:104-125`). `state` is `running | completed |
failed | cancelled`. `profileName` is the busy key (so flow rows look like
`F1:O1`). History stats:

```json
{"totalSyncs":10,"totalBytes":10240,"totalDuration":600,"totalErrors":1,
 "byProfile":{"photos":{"syncs":3,"bytes":2048,"duration":120,"errors":0}}}
```

`HistoryStats` / `ProfileStats` (`Models.swift:127-140`).

### 6.10 Board

```json
{
  "id":"B1","name":"Legacy","createdAt":"2026-10-22 03:00:00","updatedAt":"2026-10-22 03:00:00",
  "nodes":[{"id":"N1","remoteName":"gdrive","path":"/a","label":"A","x":0,"y":0}],
  "edges":[{"id":"E1","sourceID":"N1","targetID":"N2","action":"push",
            "syncConfig":{"action":"push"}}]
}
```

`Board` / `BoardNode` / `BoardEdge` (`Models.swift:142-169`). The DB stores the
edge `sync_config` as a JSON string; the RPC exposes an object (default `{}`).
`sourceID`/`targetID` are emitted with Swift casing; `sourceId`/`targetId` are
accepted on input. Boards have no UI in the Swift app or the Rust window; they
exist for legacy data and the `gn-drive board` CLI.

### 6.11 Provider

```json
{
  "name":"sftp",
  "description":"SSH/SFTP",
  "prefix":"sftp",
  "hidden":false,
  "oauth":false,
  "options":[
    {"name":"host","help":"SSH host to connect to.","required":true,
     "isPassword":false,"sensitive":true,"advanced":false,"hide":0,
     "type":"string","default":"","examples":[]}
  ]
}
```

Source: `rclone config providers` (returns the same JSON array as
`/tmp/gnd-prov.json`, 69 backends in rclone 1.75.0, 68 visible). `oauth` is true
when any option is named `auth_url` (drive, onedrive, dropbox, box, pcloud,
yandex, zoho, google photos, google cloud storage, hidrive, huaweidrive,
jottacloud, mailru, premiumizeme, putio, sharefile, internxt, pikpak). Field
names are converted from rclone's PascalCase to camelCase; `default` is the
`DefaultStr` value. Providers with `hidden:true` must not be offered.

### 6.12 OAuth question

The raw `rclone config create --non-interactive` question, normalized:

```json
{
  "state":"*oauth-islocal,teamdrive,oauth,",
  "option":{
    "name":"config_is_local",
    "help":"Use web browser to automatically authenticate rclone with remote?",
    "default":true,
    "required":false,
    "type":"bool",
    "examples":[{"value":"true","help":"Yes"},{"value":"false","help":"No"}]
  },
  "error":"",
  "result":""
}
```

Actions and the verified state machine are in §7.5.

---

## 7. Methods

Each section lists params, result, errors, and the GNDriveCore mapping. Params
omitted entirely are equivalent to `{}`.

### 7.1 System

#### `ping`

Params: none.
Result: `{"pong":true,"protocol":1,"version":"0.4.0","now":"2026-10-22T12:00:00Z"}`.
Errors: none.
Mapping: `BuildVersion.current`.

#### `state.snapshot`

Params:

```json
{"sections":["auth","flows","remotes","profiles","history","settings","runtime"]}
```

`sections` is optional; default is all. Unknown section names are ignored.
This method is allowed while locked.

Result:

```json
{
  "locked": false,
  "auth": {"...":"AuthStatus"},
  "flows": [{"...":"Flow"}],
  "remotes": [{"...":"Remote"}],
  "profiles": [{"...":"Profile"}],
  "history": [{"...":"HistoryEntry"}],
  "runtime": {"revision":5,"flows":[]},
  "appSettings": {"...":"AppSettings"},
  "rcloneVersion": "rclone v1.75.0",
  "version": "0.4.0",
  "paths": {"configDir":"...","dbPath":"...","rcloneConfigPath":"...","logDir":"..."}
}
```

- Fields written as `{"...":"Type"}` are placeholders for the full object shape
  in §6. History is the latest 100 rows, matching `AppState.reloadData`
  (`Sources/GNDriveApp/AppState.swift:149-158`).
- While locked: `locked:true`, `auth` + `appSettings` + `paths` + `version`,
  every data array empty, `runtime` `{revision:0,flows:[]}`. `rcloneVersion` is
  omitted because the data plane is closed.
- Errors: `locked` is **not** returned here.
- Mapping: `AuthService.status()` / `appSettings()`; `Store.listFlows()`,
  `listProfiles()`, `listHistory(limit:100, offset:0)`; `RcloneClient.listRemotes()`
  and `version()`; `RuntimeHub.snapshot()`.

#### `app.quit`

Params: `{"exitCode":0}` (optional, default 0).
Result: `{"quitting":true}`.
The response is flushed before shutdown; then the §2.3 sequence runs with
reason `request`.
Errors: none.
Mapping: `AppContainer.close()`.

#### `doctor`

Params: `{"includeDataFiles":true}`.
Result:

```json
{
  "ok": true,
  "checks": [
    {"id":"rclone_binary","label":"rclone binary","status":"ok",
     "detail":"/Applications/GNDrive.app/Contents/MacOS/rclone"},
    {"id":"rclone_version","label":"rclone version","status":"ok","detail":"rclone v1.75.0"},
    {"id":"config_dir","label":"config directory","status":"ok","detail":"/Users/me/.config/gn-drive"},
    {"id":"database","label":"database","status":"ok","detail":"gn-drive.db"},
    {"id":"auth","label":"auth","status":"info","detail":"configured: yes, unlocked: no"},
    {"id":"remotes","label":"remotes","status":"info","detail":"2 configured"},
    {"id":"profiles","label":"profiles","status":"info","detail":"0 configured"},
    {"id":"platform","label":"platform","status":"ok","detail":"darwin arm64"}
  ],
  "dataFiles": ["auth.json","gn-drive.db","rclone.conf"],
  "platform": {"os":"macos","arch":"arm64"}
}
```

- `status` ∈ `ok | info | warn | error`. `ok` (top level) is true iff no check
  is `error`. This deliberately fixes the CLI's unconditional "All checks
  passed." (`Sources/gn-drive/Commands.swift:481-568`).
- While locked, auth is `info` ("configured, locked") and remotes/profiles are
  skipped with `info` rather than opening the data plane.
- `dataFiles` is present only when `includeDataFiles` is true.
- Errors: none (a missing rclone is a check, not a method error).
- Mapping: logic from `DoctorCommand.run`, plus `AuthService.status()`,
  `ServiceHealth.read`.

#### `logs.read`

Params:

```json
{"limit":200,"level":"info","since":1729581200000}
```

- `limit` default 200, max 2000.
- `level` filters `debug|info|warn|error` (minimum severity), compared
  case-insensitively. Output `level` uses Logger's uppercase labels
  (`DEBUG|INFO|WARN|ERROR`).
- `since` is Unix ms; only newer lines are returned.

Result:

```json
{"lines":[{"time":"2026-10-22T12:00:00Z","level":"INFO","msg":"sync: started",
           "fields":{"task":"T1","profile":"F1:O1"}}],"dropped":0}
```

- The bridge keeps a ring buffer fed by a Logger sink (see §11.4). Logs still
  go to stderr.
- `dropped` counts lines evicted from the ring since startup.
- Errors: `invalid_params`.
- Mapping: `Logger` (`Sources/GNDriveCore/Logger.swift:38-56`), plus the
  service log files are **not** read here.

### 7.2 Auth

#### `auth.status`

Params: none.
Result: AuthStatus (§6.1).
Errors: none.
Mapping: `AuthService.status()` (`AuthService.swift:134-139`).

#### `auth.setup`

Params: `{"password":"..."}`.
Result: AuthStatus (now `setup:true, unlocked:true`).
Errors: `already_setup`, `password_too_short`, `internal_error`.

Side effects (mirror `AppState.setup`, `AppState.swift:96-107`):

1. `auth.setupPassword`.
2. `container.afterUnlock()`.
3. Publish `auth:unlocked`.
4. Data is now readable; the client should call `state.snapshot`.

Mapping: `AuthService.setupPassword` (`AuthService.swift:160-177`),
`AppContainer.afterUnlock` (`AppContainer.swift:118-120`).

#### `auth.unlock`

Params: `{"password":"..."}`.
Result: AuthStatus (`unlocked:true`).
Errors:

- `not_setup`
- `invalid_password`; `data.failedAttempts`
- `locked_out`; `data.retryAfterSecs`, `data.lockedUntil`
- `internal_error` (decrypt failure)

Behavior: `AuthService.unlock` enforces the 3-attempt exponential delay and the
10-attempt / 5-minute lockout (`AuthService.swift:179-231`). The sleep happens
inside the method; the request may take up to `retryAfterSecs`. The bridge must
run it off the reader thread.

Side effects: `container.afterUnlock()`, publish `auth:unlocked`, then the
client calls `state.snapshot`.

#### `auth.lock`

Params: none.
Result: AuthStatus (`unlocked:false`).
Errors: `internal_error` only (encrypt failures are logged, not thrown; see
`AuthService.lockInternal`, `AuthService.swift:248-257`).

Sequence, mirroring `AppState.lockNow` (`AppState.swift:109-120`):

1. `container.beforeLock()` — detaches engines, resets runtime, closes
   Store/rclone (`AppContainer.swift:123-132`).
2. `auth.lock()`.
3. Publish `auth:locked`.
4. Reset the bridge's runtime debouncer to revision 0.

`auth.lock` is idempotent: calling it while already locked still succeeds and
forgets the Keychain key.

#### `auth.changePassword`

Params: `{"oldPassword":"...","newPassword":"..."}`.
Result: AuthStatus (still unlocked).
Errors: `not_setup`, `not_unlocked`, `invalid_password`, `password_too_short`.

Behavior: `AuthService.changePassword` verifies the old password, rotates the
Argon2id hash, derives a new key, re-encrypts the config files, replaces the
Keychain key, and stays unlocked (`AuthService.swift:259-297`).

Parity note: the SwiftUI `AppState.changePassword` does not call `beforeLock`
and does not force a re-lock. Per UC-11, the Rust window should call
`auth.changePassword`, then `auth.lock`, and navigate to the unlock screen —
matching the documented legacy UX. The RPC keeps the engine semantics and does
not add an implicit lock.

#### `auth.removePassword`

Params: `{"password":"..."}`.
Result: AuthStatus (`setup:false, unlocked:true`).
Errors: `not_setup`, `not_unlocked`, `invalid_password`.

Sequence (mirror `AppState.removePassword`, `AppState.swift:133-145`):

1. `container.beforeLock()`.
2. `auth.removePassword(password)` — decrypts, deletes `.enc` files and
   `auth.json`, sets `enabled=false` (`AuthService.swift:299-312`).
3. `container.afterUnlock()`.
4. Publish `auth:unlocked` (deliberate: the app becomes usable without a
   password; the SwiftUI app silently reloads, but the window needs the event
   to leave the settings screen).

### 7.3 Flows

Flow saves are rejected while the flow is running, matching the legacy web API
and `AppState.saveFlow` (`AppState.swift:202-218`). Compute `active` from
`runtimeFlow.status ∈ {running, cancelling}` or `flowEngine.status`.

#### `flows.list`

Params: none.
Result: `{"flows":[Flow]}` ordered by `(sortOrder, name)` (`Store.listFlows`,
`Store.swift:575-585`).
Errors: `locked`.
Mapping: `Store.listFlows()`.

#### `flows.get`

Params: `{"id":"F1"}`.
Result: `{"flow":Flow}`.
Errors: `flow_not_found`, `invalid_params`, `locked`.
Mapping: `Store.getFlow` (`Store.swift:587-595`).

#### `flows.create`

Params:

```json
{"name":"Nightly","id":"optional-uuid","canvasJSON":"{}"}
```

- `name` defaults to `"Untitled flow"` when empty (matches
  `AppState.addFlow`, `AppState.swift:179-193`).
- `id` defaults to `UUID().uuidString`. If it already exists → `flow_exists`.
- `canvasJSON` defaults to `{}`.

Result: `{"flow":Flow}` with empty `operations`.
Errors: `flow_exists`, `invalid_params`, `locked`.
Mapping: `Store.saveFlow` + `Store.getFlow`.

#### `flows.save`

Params: `{"flow":Flow}` (`flow.id` required; operations may be empty).
Result: `{"flow":Flow,"warnings":[{"code":"invalid_cron","message":"..."}]}`.
Errors: `invalid_params` (empty flow id), `flow_busy`, `locked`, `internal_error`.

Behavior (mirror `AppState.saveFlow`, `AppState.swift:202-218`):

1. If the flow is running → `flow_busy`.
2. For every operation: if `id` is missing, generate a UUID (the store requires
   ids, `Store.saveFlow`, `Store.swift:660-666`); set `flowID`; if
   `sortOrder == 0`, set it to the array index; normalize the action into
   `action` + `syncConfig.action`.
3. `Store.saveFlow` replaces all operations atomically (`Store.swift:639-682`).
4. `SyncEngine.syncFlowSchedule(flow)` — registers/removes the `flow:<id>` cron
   (`SyncEngine.swift:300-320`). An invalid cron does not fail the save; it
   produces the `invalid_cron` warning.
5. Publish `state:changed` `{domain:"flows", id:flow.id}`.
6. Re-read the flow and return it (so `updatedAt` and generated ids are
   authoritative).

`flows.save` is an upsert; saving a new id creates the flow. Use
`flows.create` if the client wants collision detection. `flow.operations` is a
**full replacement**, not a patch: any persisted operation omitted from the
array is deleted. The same full-replacement rule applies to `profiles.save`
and `boards.save`.

#### `flows.delete`

Params: `{"id":"F1"}`.
Result: `{"deleted":true,"id":"F1"}`.
Errors: `flow_busy`, `flow_not_found`, `locked`.
Behavior: `Store.deleteFlow`, `syncEngine.unregisterFlowSchedule`, `runtime.forget`,
publish `state:changed` (`AppState.swift:220-235`).

#### `flows.execute`

Params: `{"id":"F1"}`.
Result: `{"flowID":"F1","status":"running"}` (returns immediately).
Errors: `flow_not_found`, `flow_empty`, `already_running`, `engine_not_ready`,
`locked`.
Notes:

- Execution loads the **persisted** flow (`FlowEngine.execute`,
  `FlowEngine.swift:62-81`); unsaved canvas/inspector edits are not used. The
  window should save dirty flows first, exactly as `InspectorView.runFlow`
  does (`Sources/GNDriveApp/InspectorView.swift:79-87`).
- Operations run sequentially in store order; the first failure stops the flow
  (`FlowEngine.swift:101+`).
- Progress arrives as `flow:execution` and `sync:*` events, and via
  `runtime:snapshot`.

#### `flows.stop`

Params: `{"id":"F1"}`.
Result: `{"flowID":"F1","status":"cancelling"}`.
Errors: `not_running`, `locked`.
Mapping: `FlowEngine.stop` (`FlowEngine.swift:83-94`). Cancellation is polled;
the final `flow:execution` status becomes `cancelled`.

#### Operation convenience methods

These persist one operation without sending the whole flow. They share the
same guards as `flows.save` (flow exists, not running, schedule re-sync,
`state:changed`). A missing operation id is generated. `syncConfig` replaces
the existing object wholesale when present (no deep merge).

##### `operations.create`

Params:

```json
{"flowID":"F1","operation":{
  "sourceRemote":"","sourcePath":"/tmp/a","targetRemote":"gdrive","targetPath":"/a",
  "action":"push","syncConfig":{"parallel":4}}}
```

Result: `{"flow":Flow,"operation":FlowOperation}` (appended at
`max(sortOrder)+1`).
Errors: `flow_not_found`, `flow_busy`, `invalid_params`, `locked`.

##### `operations.update`

Params:

```json
{"flowID":"F1","operationID":"O1","patch":{
  "targetPath":"/b","action":"bi","syncConfig":{"resilient":true}}}
```

`patch` is a shallow merge over the existing operation. `id` and `flowID`
cannot be changed; `sortOrder` changes are ignored here (use
`operations.reorder`).
Result: `{"flow":Flow,"operation":FlowOperation}`.
Errors: `flow_not_found`, `operation_not_found`, `flow_busy`, `locked`.

##### `operations.delete`

Params: `{"flowID":"F1","operationID":"O1"}`.
Result: `{"flow":Flow}` with remaining operations re-numbered from 0.
Errors: `flow_not_found`, `operation_not_found`, `flow_busy`, `locked`.

##### `operations.reorder`

Params: `{"flowID":"F1","operationIDs":["O2","O1"]}`.
`operationIDs` must be a permutation of the flow's operation ids, otherwise
`invalid_params`. Result: `{"flow":Flow}` with `sortOrder = index`.
Errors: `flow_not_found`, `flow_busy`, `invalid_params`, `locked`.

### 7.4 Runtime

#### `runtime.snapshot`

Params: none.
Result: Runtime snapshot (§6.8). `revision` is the hub's current revision.
Errors: `locked`.
Mapping: `RuntimeHub.snapshot()` (`RuntimeHub.swift:61-83`). This method never
mutates state and is safe to poll.

#### `runtime.tasks`

Params: none.
Result: `{"tasks":[SyncTask.Snapshot]}` (§6.7).
Errors: `locked`.
Mapping: `SyncEngine.activeTasks()` (`SyncEngine.swift:235-238`). Includes
in-flight flow operations and `sync.profile` tasks; it does not include
finished tasks (history is the source for those).

### 7.5 Remotes

#### `remotes.providers`

Params: `{"refresh":false}`.
Result:

```json
{"rcloneVersion":"rclone v1.75.0",
 "providers":[{"name":"sftp","description":"SSH/SFTP","prefix":"sftp",
               "hidden":false,"oauth":false,"options":[]}],
 "cached":true}
```

Behavior: run `rclone config providers` (stdout is a JSON array) once and cache
it for the process lifetime unless `refresh` is true. Convert fields per §6.11,
compute `oauth`, and drop nothing (clients filter `hidden`).
Errors: `rclone_missing`, `rclone_failed`, `invalid_params`.
Mapping: `RcloneClient.run` is private; add a `providers()` wrapper or run the
process in the bridge with the resolved binary
(`RcloneClient.resolveBinary`, `RcloneClient.swift:80-97`).

Notes:

- `sharepoint` is **not** an rclone 1.75 backend; it must not be offered.
- `iclouddrive` is a normal provider, not OAuth: it requires `service`,
  `apple_id`, `password`, and possibly a 2FA `trust_token`.
- The provider list is independent of the config file, so this method is
  allowed while locked.

#### `remotes.list`

Params: none.
Result: `{"remotes":[Remote]}`.
Errors: `locked`, `rclone_failed`.
Mapping: `RcloneClient.listRemotes()` (`RcloneClient.swift:191-207`).

#### `remotes.add`

Params:

```json
{"name":"nas","type":"sftp","config":{"host":"nas.local","user":"me","pass":"..."}}
```

`config` values are strings; booleans/numbers are stringified. The bridge
flattens them to `key=value` pairs for `rclone config create`.

Result: `{"remote":{"name":"nas","type":"sftp","description":""},"verified":true}`.
Errors:

- `invalid_params` (`name`/`type` empty, or a required option is missing;
  `data.question` carries the rclone question)
- `remote_exists`
- `oauth_required` — use §7.5.1; `data.state`, `data.questions`
- `auth_failed` / `test_failed` — the probe failed and the partial remote was
  deleted
- `rclone_missing`, `rclone_failed`, `locked`

Implementation contract:

1. Reject an existing name (`remote_exists`).
2. Run
   `rclone config create <name> <type> <k=v>… --non-interactive --obscure --config <path>`.
   **Do not** use the Swift `createRemote` as-is: it omits `--non-interactive`
   (`RcloneClient.swift:222-227`), which can block on a browser prompt. The
   serve bridge must implement the non-interactive form.
3. Parse stdout:
   - empty / `State == ""` → remote created.
   - `State` contains `oauth` → delete the partial remote and return
     `oauth_required`.
   - another question (`Option != null`) → delete the partial remote and return
     `invalid_params` with `data.question`.
4. Verify with `rclone lsd <name>: --max-depth 1 --config <path>`
   (`RcloneClient.testRemote`, `RcloneClient.swift:244-249`).
5. On verify failure: `rclone config delete <name>`, then `auth_failed` with
   sanitized stderr.
6. Publish `state:changed` `{domain:"remotes", id:name}`.

This fixes two app defects called out in the parity inventory: the Add-remote
sheet dismisses unconditionally and never clears its busy flag
(`Sources/GNDriveApp/RemotesView.swift:134-145`), and browse errors are
swallowed. The RPC reports errors and never leaves a half-created remote.

#### `remotes.test`

Params: `{"name":"nas"}`.
Result: `{"name":"nas","ok":true}`.
Errors: `remote_not_found`, `test_failed` (`data.stderr`), `rclone_failed`,
`locked`.
Mapping: `RcloneClient.testRemote` (`RcloneClient.swift:244-249`), which runs
`lsd name: --max-depth 1`.

#### `remotes.delete`

Params: `{"name":"nas"}`.
Result: `{"deleted":true,"name":"nas"}`.
Errors: `remote_not_found`, `rclone_failed`, `locked`.
Mapping: `RcloneClient.deleteRemote` (`RcloneClient.swift:240-243`). Publish
`state:changed` `{domain:"remotes", id:name}`.

#### 7.5.1 OAuth remote creation

This is the only way to add an OAuth backend. It mirrors what rclone's own
interactive config does and was verified against rclone 1.75.0:

- `config create name drive --non-interactive` → question
  `State:"client_id_warning"`, option `config_shared_client_id`.
- Answer `config_shared_client_id=true` → question
  `State:"*oauth-islocal,teamdrive,oauth,"`, option `config_is_local`.
- Answer `config_is_local=false` → question
  `State:"*oauth-authorize,teamdrive,oauth,"`, option `config_token`
  (`Required:true`).
- A bad token keeps the same state and sets `Error:"Couldn't decode response …"`.
- A partial `[name]` section with only `type = drive` is written by
  `config create` before the wizard completes, so cleanup is mandatory.

The state string is provider-specific (the drive suffix is `teamdrive,oauth,`);
never hardcode it — pass back the exact `State` returned by rclone.

##### `remotes.oauth.start`

Params:

```json
{"type":"drive","clientID":"optional.apps.googleusercontent.com",
 "clientSecret":"optional-secret"}
```

Result:

```json
{"sessionID":"O1","type":"drive",
 "authURL":"http://127.0.0.1:53682/auth?state=...",
 "expiresAt":"2026-10-22T12:10:00Z"}
```

Behavior:

1. Spawn
   `rclone authorize <type> [clientID clientSecret] --auth-no-open-browser --config <path>`.
   `rclone authorize` opens a callback server on `127.0.0.1:53682`; it prints
   `Please go to the following link: <url>` and `Waiting for code...` on
   stderr.
2. Parse the URL from stderr and return it. The window opens it in the user's
   browser (or renders it as a fallback). The Rust process must not proxy the
   OAuth callback.
3. Keep the child alive. When it exits 0, extract the token blob printed
   between `Paste the following into your remote machine --->` and
   `<---End paste` (stdout; fall back to stderr), keep it **in bridge memory
   only**, and publish `remotes:oauth`
   `{sessionID, status:"authorized"}`. Do not put the token in the event.
4. On non-zero exit, publish `status:"failed"` with a sanitized error.
5. Sessions expire after 10 minutes; cancel the child and zero the token.
6. Only one `rclone authorize` may run at a time: rclone binds the fixed
   callback port `127.0.0.1:53682`. If a session is already live, return
   `oauth_session_active` instead of spawning a second one.

Errors: `oauth_failed` (spawn/port/parse failure), `oauth_session_active`,
`invalid_params` (only one of clientID/clientSecret), `rclone_missing`, `locked`.

##### `remotes.oauth.complete`

Params:

```json
{"sessionID":"O1","name":"gdrive","type":"drive",
 "config":{"scope":"drive"}}
```

Result: `{"remote":{"name":"gdrive","type":"drive","description":""},"verified":true}`.
Errors: `oauth_session_not_found`, `oauth_cancelled`, `oauth_failed`,
`remote_exists`, `test_failed`, `rclone_failed`, `locked`.

Behavior:

1. Take the token blob for `sessionID` (never accept a token in params). Pass
   the state and token each as a single `Process` argument (no shell, no
   interpolation); the token can contain spaces and quotes.
2. Complete the wizard with `--continue` rather than replaying all answers:

   ```
   rclone config create <name> <type> \
     --continue --state '<State>' --result '<token-blob>' \
     --non-interactive --obscure --config <path>
   ```

   If the provider asks another question first (for example drive's
   `client_id_warning`), answer it in a loop:

   | Question | Answer |
   |---|---|
   | `config_shared_client_id` | `true` unless the client supplied its own ID |
   | `config_is_local` | `false` (headless token flow) |
   | an option present in `params.config` | that value |
   | anything else | abort, delete the partial remote, `oauth_failed` with `data.question` |

   Maximum 8 steps. Each step uses the exact `State` from the previous step.
3. Verify with `lsd <name>: --max-depth 1`. On failure delete the remote and
   return `test_failed`.
4. Forget the session token.
5. Publish `state:changed` `{domain:"remotes", id:name}`.

##### `remotes.oauth.cancel`

Params: `{"sessionID":"O1"}`.
Result: `{"cancelled":true,"sessionID":"O1"}`.
Errors: `oauth_session_not_found`, `locked`.
Behavior: terminate the authorize child, zero the token, publish
`remotes:oauth` `{sessionID, status:"cancelled"}`.

### 7.6 Filesystem browsing

#### `fs.list`

Params:

```json
{"path":"gdrive:Photos","recursive":false,"limit":150}
```

- `path` must contain `:` (remote) or start with `/` (local); otherwise
  `fs_missing_path`, matching `RcloneClient.listFiles`
  (`RcloneClient.swift:140-151`).
- `recursive:false` → one level (`lsjson --max-depth 1`).
- `recursive:true` → `lsjson --recursive --files-only` and truncate at `limit`
  (default 150, matching `listFilesForPending`, `RcloneClient.swift:154-165`).

Result:

```json
{"path":"gdrive:Photos","entries":[
  {"name":"a.jpg","size":12345,"mimeType":"image/jpeg","isDir":false,
   "modTime":"2026-10-22T03:00:00.000000000Z","path":"a.jpg","id":""}
],"truncated":false}
```

Errors: `fs_missing_path`, `browse_failed` (`data.path`, `data.stderr`),
`rclone_failed`, `locked`.
Mapping: `RcloneClient.listFiles` / `listFilesForPending`.

The SwiftUI `AppState.browse` swallows errors and returns `[]`
(`AppState.swift:288-290`); the RPC must not. `RemotePathEditor` shows the
header `remote:path`, clickable directories, dimmed files, Up/Cancel/Select
(`Sources/GNDriveApp/Helpers.swift:40-111`) — all client-side.

### 7.7 Profiles and legacy schedules

#### `profiles.list`

Params: none.
Result: `{"profiles":[Profile]}` ordered by name.
Errors: `locked`.
Mapping: `Store.listProfiles()` (`Store.swift:212-215`).

#### `profiles.get`

Params: `{"name":"photos"}`.
Result: `{"profile":Profile}`.
Errors: `profile_not_found`, `locked`.
Mapping: `Store.getProfile` (`Store.swift:217-222`).

#### `profiles.save`

Params: `{"profile":Profile}` (full object; `name` required).
Result: `{"profile":Profile}` (normalized direction).
Errors: `missing_name`, `invalid_direction`, `invalid_params`, `locked`.
Mapping: `Store.saveProfile` (`Store.swift:224-276`). Upsert by name; empty
direction becomes `push`. This is a superset of the CLI's `profile add`
(`Sources/gn-drive/Commands.swift:307-339`).

#### `profiles.delete`

Params: `{"name":"photos"}`.
Result: `{"deleted":true,"name":"photos"}`.
Errors: `profile_not_found`, `locked`.
Mapping: `Store.deleteProfile` (`Store.swift:278-281`).

#### `schedules.list`

Legacy-only. Params: none.
Result: `{"schedules":[Schedule]}`:

```json
{"id":"S1","profileName":"photos","action":"push","cron":"0 * * * *",
 "enabled":true,"lastRun":"","nextRun":"","lastResult":"","createdAt":""}
```

Errors: `locked`.
Mapping: `Store.listSchedules()` (`Store.swift:338-357`). There is no current UI
or CLI that creates schedules (`SyncEngine.registerSchedule`,
`SyncEngine.swift:243-261`); flows use the `flow:<id>` id instead.

### 7.8 One-shot sync (CLI parity)

#### `sync.profile`

Params:

```json
{"profile":"photos","action":"push","dryRun":false,"wait":false}
```

- `action` defaults to the profile's normalized direction; accepted values are
  `pull | push | bi | bi-resync | dry-run`.
- `dryRun:true` overrides the profile's `dryRun` for this run.
- `wait:false` (default) returns immediately; progress is delivered by
  `sync:*` events.

Result (async):

```json
{"taskID":"T1","profile":"photos","action":"push","state":"running"}
```

Result (wait): same, with `state` terminal; on failure the method returns
`task_failed` with `data.taskID`.

Errors: `profile_not_found`, `profile_busy`, `invalid_action`,
`sync_not_running`, `task_failed`, `task_cancelled`, `locked`.

Mapping: `SyncEngine.startSync(action:profileName:)`
(`SyncEngine.swift:157-162`) or, when `dryRun` overrides the profile,
`startPathSync(action:busyKey:from:to:opts:)` with a mutated copy
(`SyncEngine.swift:167-179`); `waitTask` for `wait:true`
(`SyncEngine.swift:209-224`). This backs the CLI `gn-drive sync`
(`Sources/gn-drive/Commands.swift:182-235`).

#### `sync.stop`

Params: `{"taskID":"T1"}`.
Result: `{"cancelled":true,"taskID":"T1"}`.
Errors: `task_not_found`, `locked`.
Mapping: `SyncEngine.stopSync` (`SyncEngine.swift:227-233`).

### 7.9 Boards (legacy DAG)

Boards have no UI; they are kept for existing databases and the CLI.

#### `boards.list`

Params: none.
Result: `{"boards":[{"id":"B1","name":"Legacy","createdAt":"...","updatedAt":"..."}]}`
(metadata only; `Store.listBoards`, `Store.swift:482-495`).
Errors: `locked`.

#### `boards.get`

Params: `{"id":"B1"}`.
Result: `{"board":Board}` with nodes and edges.
Errors: `board_not_found`, `locked`.
Mapping: `Store.loadBoardGraph` (`Store.swift:510-538`).

#### `boards.save`

Params: `{"board":Board}` (`id` and `name` required; node ids required; missing
edge ids are generated).
Result: `{"board":Board}` (re-read).
Errors: `board_not_found` is **not** used (upsert), `invalid_params`, `locked`.
Mapping: `Store.saveBoardGraph` (`Store.swift:540-566`). Note: this function
always writes `schedule_enabled=0` and `cron_expr=""`, so board scheduling is
not preserved — do not expose board schedules in the window.

#### `boards.delete`

Params: `{"id":"B1"}`.
Result: `{"deleted":true,"id":"B1"}`.
Errors: `board_not_found`, `locked`.

#### `boards.execute`

Params: `{"id":"B1","stopOnError":true}`.
Result: `{"runID":"R1","boardID":"B1","status":"running"}`.
Errors: `board_not_found`, `board_empty`, `board_busy`, `locked`.
Mapping: `BoardEngine.execute` (`BoardEngine.swift:77-116`). Progress via
`board:execution` events with `nodeID`/`edgeID`/`status`.

#### `boards.stop`

Params: `{"id":"B1"}`.
Result: `{"boardID":"B1","status":"cancelling"}`.
Errors: `board_not_running`, `board_not_found`, `locked`.

#### `boards.status`

Params: `{"id":"B1"}`.
Result: `{"run":{"runID":"R1","boardID":"B1","status":"completed","error":""}}`
or `{"run":null}` if the board never ran in this process.
Errors: `board_not_found`, `locked`.
Mapping: `BoardEngine.status` (`BoardEngine.swift:54-73`).

### 7.10 History

#### `history.list`

Params: `{"limit":20,"offset":0,"profile":"optional busy key"}`.
Result: `{"entries":[HistoryEntry],"total":null}`.
- `limit` default 20, max 1000.
- When `profile` is present, use `Store.listHistory(profile:limit:offset:)`.
- `total` is reserved (always `null` in v1).
Errors: `invalid_params`, `locked`.
Mapping: `Store.listHistory` (`Store.swift:382-399`).

#### `history.stats`

Params: none.
Result: HistoryStats (§6.9).
Errors: `locked`.
Mapping: `Store.historyStats()` (`Store.swift:420-459`).

#### `history.clear`

Params: none.
Result: `{"cleared":true}`.
Errors: `locked`.
Mapping: `Store.clearHistory()` (`Store.swift:416-418`).

### 7.11 Settings

#### `settings.get`

Params: none.
Result: `{"settings":AppSettings}`.
Errors: none.
Mapping: `AuthService.appSettings()` (`AuthService.swift:315-318`).

#### `settings.set`

Params: `{"settings":{"debugMode":true}}` — partial; only supplied keys change.
Result: `{"settings":AppSettings}` (full).
Errors: `invalid_params`, `internal_error`.
Mapping: read `appSettings()`, merge the patch, `setAppSettings(_:)`
(`AuthService.swift:320-324`). Allowed while locked. These values live in
`auth.json`, not SQLite.

### 7.12 Service (launchd)

All service methods accept `{"scope":"user"}` (default) or `"system"` and
return a status object. `ServiceSpec` is filled with `configDir` and an
executable path; the default executable is the running `gn-drive` binary (the
serve parent), which is correct because the plist runs
`<exec> run --service` (`Launchd.renderPlist`, `Sources/GNDriveCore/Launchd.swift:134-181`).

#### `service.status`

Params: `{"scope":"user"}`.
Result:

```json
{"installed":true,"running":true,"pid":5123,"mode":"service","scope":"user",
 "label":"com.gndrive.app",
 "plistPath":"/Users/me/Library/LaunchAgents/com.gndrive.app.plist"}
```

`installed:false` with `running:false` when not installed (the core
`LaunchdService.status` throws `notInstalled`; the bridge catches it).
Errors: `service_error`, `locked` is not required.
Mapping: `LaunchdService.status` (`Launchd.swift:115-132`).

#### `service.install`

Params: `{"scope":"user","execPath":"optional override"}`.
Result: same as `service.status`.
Errors: `permission_denied` (system scope without root), `service_error`.
Mapping: `LaunchdService.install` (`Launchd.swift:66-86`). Removes a stale
registration and the legacy `gn-drive.plist` first.

#### `service.uninstall`

Params: `{"scope":"user"}`.
Result: `{"installed":false,"running":false,"scope":"user"}`.
Errors: `service_error`.
Mapping: `LaunchdService.uninstall` (`Launchd.swift:88-94`).

#### `service.start`

Params: `{"scope":"user"}`.
Result: status.
Errors: `service_not_installed`, `service_error`.
Mapping: `LaunchdService.start` (`Launchd.swift:96-99`), i.e.
`launchctl kickstart -k gui/<uid>/com.gndrive.app`.

#### `service.stop`

Params: `{"scope":"user"}`.
Result: status (running false).
Errors: `service_not_installed`, `service_error`.
Mapping: `LaunchdService.stop` (`Launchd.swift:101-104`).

#### `service.restart`

Params: `{"scope":"user"}`.
Result: status.
Errors: `service_not_installed`, `service_error`.
Mapping: `LaunchdService.restart` (`Launchd.swift:106-109`).

#### `service.health`

Params: none.
Result:

```json
{"present":true,"stale":false,"pid":5123,"serviceName":"gn-drive","mode":"service",
 "startedAt":"2026-10-22T12:00:00Z","lastHeartbeat":"2026-10-22T12:05:00Z",
 "uptimeSecs":300,"webPort":0,"lastSyncAt":null,"nextScheduleAt":null,
 "lastError":"","activeTasks":["T1"]}
```

`present:false` when no `service.health` file exists. The file on disk is
snake_case JSON with ISO-8601 dates (`HealthWriter.writeNow`,
`HealthWriter.swift:100-109`); the bridge re-emits camelCase. `stale` is true
when the last heartbeat is older than 60s (`ServiceHealth.isStale`,
`Sources/GNDriveCore/HealthWriter.swift:41-42`). `webPort` is always `0` — the
HTTP server is gone; the field is retained for wire compatibility.
Errors: none.
Mapping: `ServiceHealth.read(configDir:)` (`HealthWriter.swift:33-39`).

### 7.13 Self-update

#### `update.check`

Params: `{"repoOwner":"gnasdev","repo":"gn-drive","currentVersion":"optional"}`.
Result:

```json
{"current":"0.4.0","latest":"0.5.0","updateAvailable":true,
 "checkedAt":"2026-10-22T12:00:00Z"}
```

Errors: `no_release`, `update_failed` (HTTP/JSON with sanitized detail).
Mapping: `SelfUpdate.check(opts:)` (`Sources/GNDriveCore/SelfUpdate.swift:62-65`).
`GITHUB_TOKEN` is read from the process environment; it is never returned.

#### `update.apply`

Params: `{"repoOwner":"gnasdev","repo":"gn-drive","force":false}`.
Result:

```json
{"updated":true,"oldVersion":"0.4.0","newVersion":"0.5.0",
 "binaryPath":"/Applications/GNDrive.app/Contents/MacOS/gn-drive",
 "restartHint":"Restart gn-drive …"}
```

Errors: `already_up_to_date`, `no_release`, `no_matching_asset`,
`checksum_mismatch`, `update_failed`, `unsupported`.

Important constraints:

- `SelfUpdate.update` replaces `Bundle.main.executablePath`, keeps a `.bak`, and
  downloads `gn-drive-darwin-<arm64|amd64>.tar.gz` + `.sha256`
  (`SelfUpdate.swift:126-143,226-237`).
- Release CI currently publishes `gn-drive-<VERSION>-macos-arm64.tar.gz(.sha256)`
  and `GNDrive-<VERSION>-macos.zip(.sha256)`, and only arm64
  (`.github/workflows/release.yml:36-59`). As written, `update.apply` will fail
  with `no_matching_asset` against the current releases. The parity inventory
  lists this as a known gap; the RPC must surface the error rather than
  pretending to update.
- When serve runs inside a `.app` bundle, replacing a nested `gn-drive` binary
  may invalidate the bundle signature. Return `unsupported`
  (`data.reason:"bundled_app"`) and direct the user to the app updater. The
  method is intended for standalone CLI/service installs.
- The SwiftUI app currently only calls `update.check`, never `apply`
  (`Sources/GNDriveApp/SettingsView.swift:111-124`).

---

## 8. Events

Events are `{"event":name,"ts":ms,"data":{...}}` pushes. Names match
`BusTopic` (`Sources/GNDriveCore/EventBus.swift:8-20`) plus bridge-reserved
names. `data` shapes are the Swift structs with camelCase and RFC3339 dates for
`Date` fields (the event `ts` is milliseconds).

### 8.1 Bus events

| Event | Data | Source |
|---|---|---|
| `sync:started` | `{taskID, profileID, action}` | `SyncStartedEvent`, `EventBus.swift:80-92` |
| `sync:progress` | `SyncProgressEvent` (§6.7) | `EventBus.swift:55-78` |
| `sync:completed` | `{taskID, profileID, action, duration, bytes, errors}` | `SyncCompletedEvent`, `:94-112` |
| `sync:failed` | `SyncProgressEvent` with `state:"failed"\|"cancelled"`, `errorMessage`, `transfers` | `SyncEngine.swift:395-403` |
| `flow:execution` | `{flowID, opID, status, error}` | `FlowExecutionEvent`, `:167-181` |
| `board:execution` | `{boardID, nodeID, edgeID, status, action}` | `BoardExecutionEvent`, `:148-165` |
| `schedule:triggered` | `{scheduleID, profileID, action}` | `ScheduleTriggeredEvent`, `:134-146` |
| `auth:unlocked` | `{}` | `AuthUnlockedEvent`, `:114-118` |
| `auth:locked` | `{}` | `AuthLockedEvent`, `:120-124` |
| `state:changed` | `{domain, id}` (domains used today: `flows`, `remotes`) | `StateChangedEvent`, `:183-195` |
| `service:status` | `{running, webPort, uptimeSecs}` | `ServiceStatusEvent`, `:126-132` |
| `runtime:snapshot` | Runtime snapshot (§6.8) | assembled from `RuntimeHub.snapshot()` |

Notes:

- `sync:progress` `state` is `running` during the run and the terminal state in
  the final progress event. `sync:failed` carries the full progress shape, not
  a separate error struct.
- `flow:execution` with an empty `opID` is a flow-level status
  (`running`, `cancelling`, `completed`, `failed`, `cancelled`); with a
  non-empty `opID` it is an operation status.
- `schedule:triggered.scheduleID` is `flow:<flowID>` for flow cron jobs
  (`SyncEngine.flowScheduleID`, `SyncEngine.swift:294-297`).
- `service:status` is never published by the current engine (no HTTP server);
  the bridge may emit it after service actions if desired.
- RuntimeHub ignores events whose `profileID` is not `flowID:opID`
  (`RuntimeHub.splitBusyKey`, `RuntimeHub.swift:241-246`). `sync.profile` tasks
  therefore appear in `runtime.tasks` and `sync:*` events but **not** in
  `runtime.snapshot.flows` — that is correct.

### 8.2 Bridge events

| Event | Data | When |
|---|---|---|
| `hello` | §2.4 | first frame |
| `fatal` | `{code, message, pid?, holder?}` | before exit; pre-`hello` only |
| `app:shutdown` | `{reason:"eof"\|"signal"\|"request"}` | final frame |
| `log:line` | `{time, level, msg, fields}` | each logger line at/above `--log-level` |
| `remotes:oauth` | `{sessionID, status:"waiting"\|"authorized"\|"failed"\|"cancelled", authURL?, error?}` | OAuth lifecycle |

`log:line` is optional but recommended; if a client cannot consume it, it can
poll `logs.read`. `remotes:oauth` never carries a token.

### 8.3 Runtime snapshot pushes

`runtime:snapshot` is a `BusTopic` string but is **not** published by
`RuntimeHub`, and it is excluded from `BusTopic.all`
(`EventBus.swift:22-26`). The SwiftUI app re-hydrates on every bus event
(`AppState.swift:48-53`) and calls `runtime.snapshot()`. The bridge must:

- push a snapshot after any `flow:execution` / `sync:*` event, debounced to at
  most 10 Hz;
- push at least every 2s while any flow is `running`/`cancelling` (so elapsed
  time/percentages keep moving even without new progress);
- push immediately on `auth:locked`/`auth:unlocked` (revision reset) and after
  `flows.execute/stop`;
- include only the same data as `runtime.snapshot`; no client-side merge is
  required.

---

## 9. Event ordering, coalescing, backpressure

1. **Never drop** `auth:*`, `state:changed`, `flow:execution`, terminal
   `sync:completed`/`sync:failed`, `app:shutdown`, `fatal`, `remotes:oauth`,
   or `hello`.
2. `sync:progress` may be coalesced per `profileID`: keep only the latest event
   and flush at 10 Hz. If more than 256 events are queued, drop intermediate
   progress for that key (latest wins). This mirrors the EventBus mailbox policy
   (64, drop-oldest, `EventBus.swift:252-268`) but is applied at the writer.
3. `runtime:snapshot` is conflated: only the newest snapshot matters.
4. `log:line` may be dropped under pressure; `logs.read` remains authoritative
   for the ring buffer.
5. The writer is a single serial queue, so frames on the wire preserve the
   order in which the bridge enqueued them.
6. Blocking methods (`auth.unlock` during backoff, `sync.profile` with
   `wait:true`, `update.apply`, `remotes.*`, `fs.list`, `doctor`) run on a
   worker queue (max 8 in flight); `ping` and `runtime.snapshot` must remain
   responsive.

---

## 10. Security

- The protocol is local-only. It must not bind a socket. There is no
  authentication beyond process ownership and the instance lock.
- Never send: the master password, the derived key, the remembered Keychain
  value, OAuth tokens, rclone config values for sensitive options, `GITHUB_TOKEN`,
  or `auth.json` contents.
- `remotes.providers` exposes option metadata (including `Sensitive:true`) but
  never values.
- `doctor` lists file names, not contents.
- `logs.read` / `log:line` may contain file paths. They must not contain
  config secrets; the rclone stderr sanitizer in §4.3 applies before logging
  rclone errors.
- OAuth tokens live only in bridge memory and are zeroed when used, cancelled,
  or expired.
- The window should show the `instance_busy` message verbatim; do not auto-kill
  the other process.

---

## 11. Swift bridge implementation notes

This section is guidance for the implementer of `gn-drive serve --stdio`; it is
not part of the wire contract.

### 11.1 New command

Add `ServeCommand: ParsableCommand` to `Sources/gn-drive/Commands.swift` and
register it in `GNDriveCLI.configuration.subcommands` (`main.swift:22-45`).
Suggested files:

- `Sources/gn-drive/ServeCommand.swift` — flags, lifecycle, signals.
- `Sources/gn-drive/ServeBridge.swift` — framing, dispatch, events.

### 11.2 Container

Use `AppContainer.Options` exactly as §2.1. Do not construct engines by hand:
`AppContainer` wires `SyncEngine`, `BoardEngine`, `FlowEngine`, and `RuntimeHub`
and attaches the data plane on unlock (`AppContainer.swift:74-84`).

### 11.3 I/O

- Read stdin with a blocking `readLine`/`FileHandle.readabilityHandler` loop.
  Handle a final partial line at EOF as a discarded line.
- Write through one serial queue. Use `FileHandle.standardOutput.write` and
  ignore `SIGPIPE`; treat `EPIPE` as shutdown.
- Parse requests with `JSONSerialization` (not `Codable` for the envelope) so
  ids and unknown fields survive. Per-method params are easiest as
  `[String: Any]`.
- Do not use `print` anywhere in the process while serving. Audit the engine:
  `SelfUpdate` defaults to `print` and must be given a logger closure.

### 11.4 Logger sink for `logs.read`

`Logger` currently writes only to stderr (`Logger.swift:38-56`). Add an optional
line sink (for example `public var sink: ((LogLine) -> Void)?` guarded by the
existing lock) set by the serve bridge. Capture `time`, `level`, `msg`, and
`fields`. Keep the sink fast and non-blocking; it feeds the ring buffer and the
`log:line` push.

### 11.5 Error mapping

Write one `mapError(_:) -> RPCError` helper covering:

- `AuthError`, `StoreError`, `FlowEngineError`, `SyncEngineError`,
  `BoardEngineError`, `RcloneError`, `SyncFailedError`, `ServiceError`,
  `SelfUpdateError`, `InstanceError`, `AppError`.

Match on the enum case, not the description string, except for
`BoardEngineError.failed` cycle detection (message contains `"cycle detected"`)
and `StoreError.invalid` (split `missing_name` / `invalid_direction`).

### 11.6 Runtime subscription

Subscribe with `bus.subscribeAll(topics: BusTopic.all)` (`EventBus.swift:22-26`)
and map each `BusEvent` to a JSON object manually — the structs are not
`Codable` except `FileTransferEvent` and the runtime entries. Use
`JSONSerialization` for the nested values.

### 11.7 Testing without a UI

A shell client is enough for smoke tests:

```sh
printf '%s\n' '{"id":1,"method":"ping"}' \
  '{"id":2,"method":"state.snapshot"}' |
  .build/debug/gn-drive serve --stdio --config-dir /tmp/gnd-test-config
```

The first line must be `hello`; then two responses. Killing the writer closes
stdin and must shut the process down cleanly. Use a throwaway config dir: serve
takes the instance lock and may resume a Keychain session.

---

## 12. Error/result examples by flow

### 12.1 Locked window startup

```
S: {"event":"hello",...,"data":{"setup":true,"unlocked":false,"passwordRequired":true,...}}
C: {"id":1,"method":"state.snapshot"}
S: {"id":1,"result":{"locked":true,"auth":{...},"flows":[],"remotes":[],"profiles":[],
     "history":[],"runtime":{"revision":0,"flows":[]},"appSettings":{...},"version":"..."}}
C: {"id":2,"method":"auth.unlock","params":{"password":"…"}}
S: {"id":2,"result":{"setup":true,"unlocked":true,...}}
S: {"event":"auth:unlocked","ts":...,"data":{}}
C: {"id":3,"method":"state.snapshot"}
S: {"id":3,"result":{"locked":false,"flows":[...],...}}
```

### 12.2 Run a flow

```
C: {"id":10,"method":"flows.execute","params":{"id":"F1"}}
S: {"id":10,"result":{"flowID":"F1","status":"running"}}
S: {"event":"flow:execution","data":{"flowID":"F1","opID":"","status":"running","error":""}}
S: {"event":"flow:execution","data":{"flowID":"F1","opID":"O1","status":"running","error":""}}
S: {"event":"sync:started","data":{"taskID":"T1","profileID":"F1:O1","action":"push"}}
S: {"event":"sync:progress","data":{"taskID":"T1","profileID":"F1:O1",...}}
S: {"event":"runtime:snapshot","data":{"revision":7,"flows":[...]}}
S: {"event":"sync:completed","data":{"taskID":"T1","profileID":"F1:O1","action":"push",
     "duration":12,"bytes":4096,"errors":0}}
S: {"event":"flow:execution","data":{"flowID":"F1","opID":"O1","status":"completed","error":""}}
S: {"event":"flow:execution","data":{"flowID":"F1","opID":"","status":"completed","error":""}}
```

### 12.3 Add an OAuth remote

```
C: {"id":20,"method":"remotes.oauth.start","params":{"type":"drive"}}
S: {"id":20,"result":{"sessionID":"O1","type":"drive","authURL":"http://127.0.0.1:53682/auth?state=…"}}
S: {"event":"remotes:oauth","data":{"sessionID":"O1","status":"authorized"}}
C: {"id":21,"method":"remotes.oauth.complete","params":{"sessionID":"O1","name":"gdrive","type":"drive"}}
S: {"id":21,"result":{"remote":{"name":"gdrive","type":"drive","description":""},"verified":true}}
S: {"event":"state:changed","data":{"domain":"remotes","id":"gdrive"}}
```

---

## 13. Use-case coverage

Every `Engine:` reference in `use-cases.md` maps as follows. `—` items need no
RPC (pure UI state, local preferences, or legacy HTTP that is intentionally not
reimplemented).

| UC range | Area | Methods |
|---|---|---|
| 1–18 | shell, unlock, password | `hello`, `auth.status`, `auth.setup`, `auth.unlock`, `auth.lock`, `auth.changePassword`, `auth.removePassword`, `state.snapshot` |
| 19–28 | workspace / flow rail | `state.snapshot`, `flows.list/create/delete/execute/stop`, `runtime.snapshot`, `remotes.list` |
| 29–48 | flow editing / lifecycle | `flows.get/save/create/delete/execute/stop`, `operations.*`, `runtime.snapshot` |
| 49–74 | canvas / graph | `flows.get/save` (`canvasJSON`), `operations.*`, `runtime.snapshot` |
| 75–93 | inspector / sync config | `flows.save`, `operations.update`, `remotes.list`, `fs.list` |
| 94–111 | telemetry / edge cards | `runtime.snapshot`, `runtime.tasks`, `flow:execution`, `sync:*`, `runtime:snapshot` |
| 112–124 | remotes | `remotes.providers/list/add/test/delete`, `remotes.oauth.*`, `fs.list`, `state:changed` |
| 125–132 | browse / path pickers | `fs.list` |
| 133–137 | profiles | `profiles.list/get/save/delete`, `schedules.list`, `sync.profile` |
| 138–142 | boards | `boards.list/get/save/delete/execute/stop/status` |
| 143–152 | settings / service | `settings.get/set`, `service.*`, `auth.*`, `update.check`, `history.list` |
| 153–157 | self-update | `update.check`, `update.apply` |
| 158–169 | CLI surface | — (CLI keeps working; `serve --stdio` is the new transport) |
| 170–174 | logs / diagnostics | `logs.read`, `log:line`, `doctor`, `service.health`, `runtime.tasks` |
| 175–196 | legacy-only | — (no HTTP/SSE/WS/loopback; `state.snapshot` + events replace them) |
| 197–204 | cross-cutting UX | `hello`, error envelope, `app:shutdown`, `fatal` |

Methods referenced by the checklist but not part of the public surface:

- `state.resync` and `runtime.event` are legacy WebSocket verbs; replaced by
  `state.snapshot`, `runtime.snapshot`, and pushes.
- `sync.profile` is implemented as §7.8 (the CLI's one-shot sync).

---

## 14. Known limitations and follow-ups

These are deliberate non-goals or gaps in the current Swift engine. The RPC
reports them honestly; it does not paper over them.

1. **SwiftUI app does not hold the instance lock.** Only `gn-drive run` does
   (`Sources/gn-drive/Commands.swift:41-48`). `serve` must add the app to the
   same lock (or keep the `pgrep` fallback). Until then, two processes can open
   the same SQLite DB.
2. **Self-update asset names do not match releases.** `SelfUpdate.pickAssets`
   expects `gn-drive-darwin-{arm64,amd64}.tar.gz`; CI publishes
   `gn-drive-<VERSION>-macos-arm64.tar.gz` and no x86_64 asset
   (`.github/workflows/release.yml:36-59`). `update.apply` will report
   `no_matching_asset`.
3. **OAuth cannot be completed headlessly.** The flow requires a browser
   session and provider consent (and Google may require 2FA or a user-owned
   client ID). The bridge can only surface the URL and wait. If an
   organization enforces a custom OAuth client, `clientID`/`clientSecret` must
   be supplied by the user; there is no config field for this yet.
4. **`sharepoint` is not an rclone backend** in 1.75; the legacy UI listed it.
   Do not offer it. `iclouddrive` requires `apple_id`/`password` and may need a
   2FA `trust_token`.
5. **Provider list is rclone-version dependent.** Cache it per process and
   expose `refresh`.
6. **No remote lock during runs.** The engine relies on rclone's own locking;
   `--max-lock`/`resilient` are passed through for bisync
   (`ProfileFlags`), but there is no cross-process guard. Two windows should
   not run the same remote concurrently.
7. **`doctor` in the CLI always prints success.** The RPC computes `ok`
   correctly; the CLI remains unchanged.
8. **Board schedules are discarded on save** (`Store.saveBoardGraph` writes
   `schedule_enabled=0`/`cron_expr=""`). Do not expose board scheduling.
9. **History is finalized after the terminal event.** A client may need one
   `history.list` retry.
10. **Change-password semantics.** The engine stays unlocked; the window must
    call `auth.lock` afterwards (UC-11). This is a client responsibility by
    design.

## Changelog

- 2026-10-22 — Rust port. `gn_drive_core::rpc::Dispatcher::handle(method, params) -> Result<Value, RpcError>` and `subscribe()` are unchanged. Pushed frames serialize with keys in the documented order `event`, `ts`, `data`. `remotes.oauth.start` still returns the consent URL (`…/auth?state=`); a quoted redirect-root notice on the same stderr chunk is not that URL. OAuth still starts only from `remotes.oauth.start` (one in-flight session, cancellable); `remotes.add` stays `--non-interactive` and returns `oauth_required` instead of launching a browser.
