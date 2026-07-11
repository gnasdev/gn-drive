---
type: index
title: "Docs Index"
description: "Navigation and dependency map for the GN Drive knowledge base."
tags: ["docs", "index"]
timestamp: 2026-07-11T00:00:00Z
status: active
---

# Docs Index

## Core

- [README.md](./README.md)
- [overview.md](./overview.md)
- [_sync.md](./_sync.md)
- [architecture/overview.md](./architecture/overview.md)
- [development/conventions.md](./development/conventions.md)
- Root [DEVELOPER.md](../DEVELOPER.md) · [README.md](../README.md)

## Shared

- [shared/glossary.md](./shared/glossary.md)
- [shared/data-models.md](./shared/data-models.md)
- [shared/api-conventions.md](./shared/api-conventions.md)

## Features

- [features/workspace-portal.md](./features/workspace-portal.md)
- [features/master-password.md](./features/master-password.md)
- [features/flows-and-operations.md](./features/flows-and-operations.md) · [requirements](./features/flows-and-operations/requirements.md)
- [features/remotes.md](./features/remotes.md)
- [features/service-mode.md](./features/service-mode.md)
- [features/self-update.md](./features/self-update.md)

## Modules

- [modules/app.md](./modules/app.md)
- [modules/api.md](./modules/api.md)
- [modules/auth.md](./modules/auth.md) · [requirements](./modules/auth/requirements.md)
- [modules/store.md](./modules/store.md)
- [modules/rclone.md](./modules/rclone.md)
- [modules/syncengine.md](./modules/syncengine.md)
- [modules/flowengine.md](./modules/flowengine.md)
- [modules/boardengine.md](./modules/boardengine.md)
- [modules/service.md](./modules/service.md)
- [modules/webui.md](./modules/webui.md)
- [modules/cli.md](./modules/cli.md)

## Planning

- [specs/planning/cleanup-full-project.md](./specs/planning/cleanup-full-project.md) — audit cleanup dead FE/flows/tooling (proposed)

## Dependency map

```text
cmd/gn-drive
  → internal/app          composition root
      → auth              password + encrypt
      → store             SQLite
      → rclone            shell-out client
      → syncengine        tasks + profile cron
      → flowengine        sequential ops (uses syncengine)
      → boardengine       DAG edges (uses rclone)
      → eventbus          in-process → SSE
      → api               HTTP + session + handlers
      → webui             embedded SPA
      → service           OS service + health (optional)

frontend SPA
  → /api/v1/*             cookie session
  → /api/v1/events        SSE progress
  stores: auth, flows, remotes, profiles, theme, locale
```

| Module | Depends on | Used by |
|--------|------------|---------|
| app | all engines + auth + api | CLI `run` and one-shots |
| api | auth, store, rclone, engines, bus | SPA, external local clients |
| flowengine | store, syncengine, bus | api flows, cron via syncengine schedules on flows |
| syncengine | store, rclone, bus | flowengine, api sync, profile schedules |
| boardengine | store, rclone, bus | CLI board |
| auth | config dir files | app, api middleware |
| store | SQLite path | all data-plane features |
| rclone | rclone binary + conf | sync, remotes, browse, board edges |
