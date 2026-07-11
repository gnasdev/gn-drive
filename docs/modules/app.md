---
type: module
title: "App"
description: "Composition root: wires auth, store, rclone, engines, API, and portal lifecycle."
tags: ["module", "app"]
timestamp: 2026-07-11T00:00:00Z
status: active
compliance: current-state
---

# App

Package: `internal/app`.

## Responsibility

Construct and own the process graph for both portal (`run`) and one-shot CLI commands.

## Options

| Field | Role |
|-------|------|
| `ConfigDir` | Override config path |
| `LogMode` | Foreground vs service logging |
| `RcloneBinary` | Override rclone executable |
| `UnlockPassword` | Unlock at start (service/CLI) |
| `PortalMode` | Allow start while locked; defer data plane |
| `Version` | Reported in `/status` and self-update |

## Lifecycle

1. Detect/ensure config dir
2. Create event bus + logger + auth
3. Optional unlock from password
4. Construct engines with nil store/rclone when deferred
5. `openDataPlane` if readable → store, rclone, attach engines
6. Build `api.AppDeps` with `AfterUnlock` / `BeforeLock`
7. `Run` / `API.Serve` owned by CLI

## Portal hooks

- **AfterUnlock** — open data plane after web unlock/setup
- **BeforeLock** — close store/rclone before re-encrypt

## Related

- [Architecture](/architecture/overview.md)
- [Auth](/modules/auth.md)
- [API](/modules/api.md)
