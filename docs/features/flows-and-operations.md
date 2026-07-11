---
type: feature
title: "Flows and Operations"
description: "Sequential multi-step sync units with optional cron and live status."
tags: ["feature", "flows"]
timestamp: 2026-07-11T00:00:00Z
status: active
---

# Flows and Operations

## Behavior

A **Flow** is the primary unit of work in the workspace. It contains ordered **Operations**. Execute runs ops sequentially; stop cancels the active run.

Each operation has fixed source/target remote+path and action `push` | `bi` | `bi-resync`, plus optional `sync_config` (filters, bandwidth, bisync flags, dry-run, …).

Optional **schedule** on the flow: enable + cron expression. On save (and on engine start / data-plane attach), `syncengine` registers a cron job (`flow:<id>`) that calls `FlowEngine.Execute`. Delete or disable unregisters the job. Profile-level rows in the `schedules` table still load for legacy data.

## API

- CRUD: `/api/v1/flows`, `/api/v1/flows/{id}`
- `POST …/execute`, `POST …/stop`
- Nested operations travel with the flow payload

## UI

Workspace cards: view vs edit mode, inline name edit, operation settings panel (Wails-aligned options), run status panel with per-file transfers tabs.

## Rules

- Product surface does not offer `pull` for flow operations (normalized to `push`)
- Source/target slots do not swap on disk when action changes
- Concurrent execute on same flow is rejected
- Failed op fails the flow; WaitTask outcome must not be treated as success

## Related

- [Flow engine](/modules/flowengine.md)
- [Data models](/shared/data-models.md)
