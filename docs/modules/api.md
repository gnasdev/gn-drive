---
type: module
title: "API Server"
description: "chi HTTP server: REST handlers, session auth, SSE, SPA mount."
tags: ["module", "api"]
timestamp: 2026-07-11T00:00:00Z
status: active
compliance: current-state
---

# API Server

Package: `internal/api`.

## Responsibility

Expose local REST + SSE for the SPA and tooling; serve embedded web UI.

## Middleware stack

1. Recoverer
2. Request ID
3. slog access log
4. Compress (html/plain/json only — not SSE)
5. CORS (reflect Origin + credentials)
6. On `/api/v1`: auth middleware

## AppDeps

Auth, Store, Rclone, SyncEngine, FlowEngine, Bus, WebUI handler, optional service health writer, Version, AfterUnlock, BeforeLock.

## Handlers (by file)

| File | Domain |
|------|--------|
| `auth_handlers.go` | unlock, setup, lock, change-password |
| `profile_handlers.go` | profile CRUD + direction normalize |
| `remote_handlers.go` | remotes list/create/delete/test |
| `sync_handlers.go` | start/list/stop tasks |
| `flow_handlers.go` | flow CRUD, execute, stop |
| `operation_handlers.go` | one-shot ops + FS browse |
| `settings_handlers.go` | app settings |
| `update_handlers.go` | self-update |
| `sse.go` | status + event stream |

## Session store

In-memory token set (`sessionAdd` / `sessionValid`). Process restart invalidates sessions; user re-unlocks.

## Related

- [API conventions](/shared/api-conventions.md)
- [Master password](/features/master-password.md)
