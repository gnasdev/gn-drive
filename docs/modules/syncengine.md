---
type: module
title: "Sync Engine"
description: "Task registry, per-profile concurrency guard, cron schedules, and WaitTask outcomes."
tags: ["module", "sync"]
timestamp: 2026-07-11T00:00:00Z
status: active
compliance: current-state
---

# Sync Engine

Package: `internal/syncengine`.

## Responsibility

Orchestrate named sync tasks, emit progress events, run profile-level cron schedules, and expose wait/stop semantics used by flow execution.

## Key rules

- **One active sync per profile name** — concurrent same-profile starts return `ErrProfileBusy`
- Terminal **outcomes** retained after task leaves the active map so `WaitTask` distinguishes failed/cancelled from success
- Cron via `robfig/cron/v3`; on start/attach loads **profile** `schedules` and **flow** schedules (`flow:<id>` → `FlowExecutor.Execute`)
- `SetFlowExecutor` wires flowengine (interface, no import cycle)
- `SyncFlowSchedule` / `UnregisterFlowSchedule` used from flow API after save/delete
- Progress published on event bus as Wails-aligned `SyncProgressEvent` (transfers, checks, ETA, …)

## Attach / start

Engines may be constructed before store open; `AttachStore` wires store + rclone and loads schedules when the engine context is running.

## Related

- [Flow engine](/modules/flowengine.md)
- [Rclone](/modules/rclone.md)
