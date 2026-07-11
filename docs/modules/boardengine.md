---
type: module
title: "Board Engine"
description: "Topological DAG execution of board edges."
tags: ["module", "boards"]
timestamp: 2026-07-11T00:00:00Z
status: active
compliance: current-state
---

# Board Engine

Package: `internal/boardengine`.

## Responsibility

Execute board graphs: topo-sort edges into layers, run edges (optionally concurrent within a layer), stop on error when configured.

## Surface

- Used by CLI `gn-drive board`
- Store holds boards/nodes/edges
- **Not** exposed on the current HTTP router or workspace UI

## Rules

- Cycle detection fails execution
- Missing node references fail
- One active run per board ID
- Edge action + sync_config drive rclone Sync

## Related

- [CLI module](/modules/cli.md)
- [Data models](/shared/data-models.md)
