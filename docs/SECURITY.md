# Security: Password Protection & Encryption

NS-Drive supports optional master password protection that encrypts all sensitive data at rest. When enabled, users must enter a password to unlock the app on each launch.

## Overview

| Aspect | Detail |
|--------|--------|
| KDF | Argon2id (64 MB memory, 3 iterations, parallelism 4) |
| Encryption | AES-256-GCM (authenticated encryption) |
| File format | `[12-byte nonce][ciphertext + GCM tag]` |
| Password hash | `argon2id$v=19$m=65536,t=3,p=4$<salt_b64>$<hash_b64>` |
| Minimum password | 4 characters |

## What Gets Encrypted

When password protection is enabled and the app is locked:

| File | Contents | Encrypted form |
|------|----------|----------------|
| `rclone.conf` | OAuth tokens, API keys, remote configs | `rclone.conf.enc` |
| `ns-drive.db` | Profiles, boards, flows, schedules, history, settings | `ns-drive.db.enc` |

### Always Unencrypted

`auth.json` stores authentication metadata and pre-unlock settings:

```json
{
  "enabled": true,
  "password_hash": "argon2id$v=19$m=65536,t=3,p=4$<salt>$<hash>",
  "failed_attempts": 0,
  "lockout_until": "",
  "app_settings": {
    "minimize_to_tray": false,
    "start_at_login": false,
    "minimize_to_tray_on_startup": false
  }
}
```

`app_settings` is stored here because tray/startup behavior must be available before unlock (the DB is encrypted at that point).

## App Lifecycle

### Startup (no auth)

```
SetSharedConfig → AuthService.ServiceStartup():
  Read auth.json → auth disabled
  → InitDatabase → LoadSettings → load rclone config
  → emit auth:unlocked → app ready
```

### Startup (auth enabled)

```
SetSharedConfig → AuthService.ServiceStartup():
  Read auth.json → auth enabled
  → Load app_settings from auth.json (for tray/startup)
  → emit auth:locked → show unlock screen

User enters password → Unlock():
  Verify password (Argon2id)
  → Decrypt .enc files → remove .enc
  → InitDatabase → LoadSettings → load rclone config
  → emit auth:unlocked → app ready
```

### Lock

```
Lock():
  CloseDatabase → ResetSharedDB
  → Encrypt plaintext files → remove plaintext + WAL/SHM
  → Zero encryption key
  → emit auth:locked → show unlock screen
```

### Shutdown

```
ServiceShutdown():
  If auth enabled and unlocked:
    → lockInternal() (same as Lock, without event)
```

## Rate Limiting

Server-side enforcement prevents brute-force attacks:

| Attempts | Behavior |
|----------|----------|
| 1–3 | No delay |
| 4–9 | Delay of 2^(n-3) seconds (1s, 2s, 4s, 8s, 16s, 32s) |
| 10+ | Lockout for 5 minutes, counter resets to 0 |

- Delay is enforced server-side (mutex released during `time.Sleep`)
- Counter persists in `auth.json` across restarts
- Successful unlock resets the counter

## Password Operations

### Set Password (first time)

1. Derive key with Argon2id (random 32-byte salt)
2. Write `auth.json` with hash and `enabled: true`
3. Store key in memory — files stay plaintext for current session
4. On next Lock or Shutdown, `lockInternal()` encrypts files

### Change Password

1. Verify old password
2. Close database connection
3. Encrypt files with new key
4. Update `auth.json` with new hash
5. Decrypt files with new key (restore working state)
6. Re-initialize database
7. Zero old key, store new key

### Remove Password

1. Verify current password
2. Clean up any `.enc` files
3. Delete `auth.json`
4. Zero encryption key
5. App continues running without auth

## Crash Recovery

On startup, `recoverFromCrash()` handles interrupted encrypt/decrypt:

| State | Auth Enabled | Action |
|-------|-------------|--------|
| Both plaintext and `.enc` exist | Yes | Remove plaintext (encrypted is authoritative) |
| Both plaintext and `.enc` exist | No | Remove `.enc` (plaintext is authoritative) |
| Only `.enc` exists | Yes | Normal — wait for unlock to decrypt |
| Only plaintext exists | Yes | Normal — files weren't encrypted yet (e.g., SetupPassword without Lock) |

## Export Encryption

Export files (`.nsd`) can optionally be encrypted with a separate password:

- Uses same AES-256-GCM algorithm
- 16-byte random salt stored in the export header reserved bytes
- Key derived with Argon2id from the export password
- Each section is individually encrypted after compression
- `FlagEncrypted` bit set in header flags

Import detects the encrypted flag and prompts for the export password.

## Backend Service

### AuthService (`desktop/backend/services/auth_service.go`)

| Method | Description |
|--------|-------------|
| `IsAuthEnabled(ctx)` | Check if password protection is configured |
| `IsUnlocked(ctx)` | Check if app is currently unlocked |
| `SetupPassword(ctx, password)` | Enable password protection |
| `Unlock(ctx, password)` | Verify password, decrypt files, initialize app |
| `Lock(ctx)` | Encrypt files, zero key |
| `ChangePassword(ctx, old, new)` | Re-encrypt with new key |
| `RemovePassword(ctx, password)` | Disable password protection |
| `GetLockoutStatus(ctx)` | Get rate limiting state |
| `GetPreUnlockSettings()` | Get tray/startup settings before unlock |
| `SyncAppSettings(settings)` | Sync settings to auth.json |

### Events

| Event | When |
|-------|------|
| `auth:unlocked` | App unlocked (password verified or no auth) |
| `auth:locked` | App locked (user action or shutdown) |

## Frontend

### AuthService (`desktop/frontend/src/app/services/auth.service.ts`)

Observable state:
- `isLocked$` — whether the app is currently locked
- `authEnabled$` — whether password protection is configured
- `loading$` — whether initial auth check is in progress

### UnlockScreenComponent

Shown when `isLocked` is true. Self-initializes via `ngOnInit`:
- **Unlock mode** (auth enabled): password input with lockout display
- **Setup mode** (first time): password + confirm with "Skip" option

### Settings Dialog — Security Card

- **Set Password**: when no password configured
- **Change Password**: verify old, enter new + confirm
- **Remove Password**: verify current password to disable

### Topbar Lock Button

Visible when `authEnabled$` is true. Clicking calls `Lock()` on the backend.
