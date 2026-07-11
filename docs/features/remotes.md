---
type: feature
title: "Remotes"
description: "Manage rclone remotes used by path pickers and operations."
tags: ["feature", "remotes"]
timestamp: 2026-07-11T00:00:00Z
status: active
---

# Remotes

## Behavior

Remotes are entries in `rclone.conf` managed via API/CLI and shown in the workspace strip.

| Action | API | CLI |
|--------|-----|-----|
| List | `GET /remotes` | `remote list` |
| Create | `POST /remotes` | `remote add` |
| Delete | `DELETE /remotes/{name}` | `remote delete` |
| Test | `POST /remotes/{name}/test` | `remote test` |

Path browsing for pickers: `GET /operations/fs`.

## Rules

- Requires unlocked data plane when config is encrypted
- Create may open interactive/config KVs depending on remote type
- Local paths use empty or `local` remote semantics in `ComposePath`

## Related

- [Rclone module](/modules/rclone.md)
- [Workspace](/features/workspace-portal.md)
