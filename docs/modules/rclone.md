---
type: module
title: "Rclone Client"
description: "Shell-out wrapper for rclone sync, remotes, and filesystem ops."
tags: ["module", "rclone"]
timestamp: 2026-07-11T00:00:00Z
status: active
compliance: current-state
---

# Rclone Client

Package: `internal/rclone`.

## Responsibility

Invoke the system `rclone` binary with a dedicated config path; parse JSON stats for progress; manage remotes and basic FS ops.

## Actions

| Action | Behavior |
|--------|----------|
| `pull` | Dest → Source (copy/sync orientation in buildArgs) |
| `push` | Source → Dest |
| `bi` | `rclone bisync` without `--resync` |
| `bi-resync` | `rclone bisync --resync --force` |
| `dry-run` | Preview / dry-run flags |

## Capabilities

- `Sync` with progress callback (`Stats`, transfers list for UI tabs)
- `ListFiles`, `Mkdir`, `Purge`, `DeleteFile`, `About`
- `ListRemotes`, `CreateRemote`, `DeleteRemote`, `TestRemote`
- Resolver policy: lower transfers/TPS for rate-limited cloud providers

## Constraints

- Requires rclone on `PATH` (or configured binary path)
- Uses process config file under gn-drive config dir
- Not an in-process rclone library binding

## Related

- [Sync engine](/modules/syncengine.md)
- [Remotes feature](/features/remotes.md)
