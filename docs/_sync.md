---
type: sync
title: "Docs Sync State"
description: "Current synchronization state for the GN Drive docs tree."
tags: ["docs", "sync"]
timestamp: 2026-07-11T07:36:46Z
status: active
---

# Docs Sync State

## Meta

- Synced commit: `5d6a401696cee8525ba5ca1cad955fa4e93a88b6` + cleanup worktree (dead FE removed, flow cron wired)
- Synced at: `2026-07-11T07:36:46Z`
- Scope: full knowledge base + post-cleanup product surface (workspace-only FE, flow cron via syncengine, tooling)
- Status: synced
- Known unsynced: None known. Delta watcher still deferred (table/repo kept for schema compat).

## Current Snapshot

Single-process CLI + Vue Workspace portal on loopback **53241**. Product web surface: Unlock → Workspace (remotes + flows) → Settings. Dead multi-page FE stubs removed.

**Flow schedules** register into `syncengine` cron as `flow:<id>` and call `FlowEngine.Execute` on tick. Profile `schedules` table still loads for legacy rows. CLI board/profile/sync unchanged.

**Stack:** Go 1.25, chi, Vue 3 + Vite 6 + Pinia + Tailwind 4. Lint via `go vet` + `vue-tsc`. Docs under `docs/` with plan `specs/planning/cleanup-full-project.md` (implemented).
