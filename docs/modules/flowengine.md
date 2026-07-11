---
type: module
title: "Flow Engine"
description: "Sequential execution of flow operations with stop and retained status."
tags: ["module", "flows"]
timestamp: 2026-07-11T00:00:00Z
status: active
compliance: current-state
---

# Flow Engine

Package: `internal/flowengine`.

## Responsibility

Run a flow's operations **in order** via syncengine; support cancel; retain last terminal status for API/UI polling.

## Rules

- One active run per flow ID (`ErrAlreadyRunning`)
- Empty operations → `ErrEmptyFlow`
- On op failure, flow fails (does not continue remaining ops as success)
- `Status(flowID)`: active → running/cancelling; after finish → last terminal; else `idle`
- Emits `FlowExecutionEvent` (and board-compatible events where needed)

## Attach

`Attach(store, syncEngine)` after portal unlock; detach store before lock.

## Related

- [Flows feature](/features/flows-and-operations.md)
- [Sync engine](/modules/syncengine.md)
