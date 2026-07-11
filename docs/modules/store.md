---
type: module
title: "Store"
description: "SQLite persistence: schema, migrations, and repositories."
tags: ["module", "store"]
timestamp: 2026-07-11T00:00:00Z
status: active
compliance: current-state
---

# Store

Package: `internal/store`.

## Responsibility

Own SQLite open/migrate and repositories for settings, profiles, schedules, history, boards, flows/operations, delta state.

## Schema tables

`settings`, `profiles`, `schedules`, `history`, `boards`, `board_nodes`, `board_edges`, `flows`, `operations`, `delta_state`.

Profile column set is wide (Wails-compatible) with additive migrations for newer flag columns.

## Repositories

| Repo | Domain |
|------|--------|
| SettingsRepo | key/value |
| ProfileRepo | profile upsert/get/list/delete |
| ScheduleRepo | profile cron rows |
| HistoryRepo | run history + aggregates |
| BoardRepo | boards + nodes/edges |
| FlowRepo | flows + nested operations |
| DeltaRepo | delta watcher state (deferred product use) |

## Rules

- Profile direction and flow action normalize to the closed product sets
- Flow save keeps operation `action` and `sync_config.action` aligned
- Foreign keys: operations → flows CASCADE; board nodes/edges → boards CASCADE

## Related

- [Data models](/shared/data-models.md)
- [Flows feature](/features/flows-and-operations.md)
