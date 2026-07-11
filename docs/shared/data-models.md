---
type: shared
title: "Data Models"
description: "Core persistence and API models for profiles, flows, operations, boards, and tasks."
tags: ["models", "shared"]
timestamp: 2026-07-11T00:00:00Z
status: active
---

# Data Models

Source of truth: `internal/store/models.go` + schema DDL in `internal/store/schema.go`. Frontend mirrors in `frontend/src/api/types.ts`.

## Profile

Named from/to pair with rclone flags. Product directions: `push` | `bi` | `bi-resync`. Unknown/legacy (e.g. `pull`) normalizes to `push` for storage/UI.

Key fields: `name`, `from`, `to`, `direction`, include/exclude paths, bandwidth, parallel, safety/performance/bisync flags, `fast_list`.

## Flow

```text
Flow
  id, name, is_collapsed
  schedule_enabled / enabled
  schedule_cron / cron_expr
  sort_order
  operations[]
```

Schedule fields are dual-named for older FE clients. Cron uses robfig six-field form when provided.

## Operation

```text
Operation
  id, flow_id
  source_remote, source_path
  target_remote, target_path
  action          # push | bi | bi-resync
  sync_config     # JSON overlay
  is_expanded, sort_order
```

Paths are fixed slots: source and target do not swap in storage. Action is mirrored into `sync_config.action` via `NormalizeAction`.

Path composition: `ComposePath(remote, path)` → `remote:path` or local filesystem path when remote is empty/`local`.

## Board

```text
Board { id, name, nodes[], edges[] }
BoardNode { id, remote_name, path, label, x, y }
BoardEdge { id, source_id, target_id, action, sync_config }
```

Executed topologically by `boardengine` / CLI `board`.

## Schedule / History

- **Schedule**: profile_name + action + cron + enabled + last/next run (profile-level cron)
- **HistoryEntry**: past sync stats per profile run

Flow-level cron is stored on the flow row (`schedule_enabled`, `cron_expr`), not only in `schedules`.

## Sync task (runtime)

Not persisted as first-class rows for every UI poll; `syncengine` holds active tasks and terminal outcomes for `WaitTask`. Snapshots expose status, nested stats, transfers list (Wails-aligned for the flow status panel).

## Auth wire format

`auth.json`:

```json
{
  "enabled": true,
  "password_hash": "argon2id$v=19$m=65536,t=3,p=4$…$…",
  "failed_attempts": 0,
  "lockout_until": "",
  "app_settings": {
    "notifications_enabled": false,
    "debug_mode": false,
    "minimize_to_tray": false,
    "start_at_login": false
  }
}
```

`.enc` files: `[12-byte nonce][AES-256-GCM ciphertext]`.

## Related

- [API conventions](/shared/api-conventions.md)
- [Store module](/modules/store.md)
