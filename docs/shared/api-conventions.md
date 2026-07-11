---
type: shared
title: "API Conventions"
description: "HTTP API base path, auth, errors, SSE, and endpoint map."
tags: ["api", "shared"]
timestamp: 2026-07-11T00:00:00Z
status: active
---

# API Conventions

Base path: **`/api/v1`**. Router: chi. SPA and static assets are served on `/*` outside the API mount.

## Auth

| Rule | Behavior |
|------|----------|
| Public | `GET /status`, `GET /events`, `POST /auth/unlock`, `/auth/setup`, `/auth/lock`, `/auth/change-password` |
| No password set up | Protected routes allowed without session (local single-user) |
| Locked | `401` `{ "code": "locked" }` |
| Unlocked, no/invalid session | `401` `{ "code": "unauthorized" }` |
| Session cookie | `gn-drive-session` — HttpOnly, SameSite=Strict, MaxAge 86400, Secure=false (loopback) |

Unlock/setup mint a session and return `{ "token": "…" }` (cookie is authoritative for the SPA).

## Errors

JSON body:

```json
{ "error": "human message", "code": "stable_code" }
```

HTTP status carries class (400/401/404/500). Prefer checking `code` in clients.

## Success

- `200` JSON body for GET/most POST
- `201` on create where applicable
- `204` empty where applicable

## SSE

`GET /api/v1/events` — `text/event-stream`. Frames are JSON event payloads from `eventbus` (sync progress, flow execution, auth, state changes). Not compressed.

## Endpoint map

| Method | Path | Purpose |
|--------|------|---------|
| GET | `/status` | setup/unlocked/session/version/lockout |
| POST | `/auth/unlock` | Unlock + open data plane + session |
| POST | `/auth/setup` | First-time password (min 4 chars) |
| POST | `/auth/lock` | Encrypt + clear session |
| POST | `/auth/change-password` | Rotate password |
| GET/POST | `/settings` | App settings |
| GET/POST | `/profiles` | List/create profiles |
| GET/PUT/DELETE | `/profiles/{name}` | Profile CRUD |
| GET/POST | `/remotes` | List/create remotes |
| DELETE | `/remotes/{name}` | Delete remote |
| POST | `/remotes/{name}/test` | Connectivity test |
| POST | `/sync` | Start profile sync task |
| GET | `/sync/tasks` | Active tasks |
| DELETE | `/sync/tasks/{id}` | Stop task |
| GET/POST | `/flows` | List/create flows |
| GET/PUT/DELETE | `/flows/{id}` | Flow CRUD (includes nested ops) |
| POST | `/flows/{id}/execute` | Run sequential ops |
| POST | `/flows/{id}/stop` | Cancel flow run |
| POST | `/operations` | One-shot file operation (API) |
| GET | `/operations/fs` | Browse remote/local path |
| POST | `/self-update` | Trigger self-update |
| GET | `/events` | SSE stream |

There is **no** HTTP board CRUD/execute surface in the current router; boards are CLI + store + `boardengine`.

## CORS

Reflects `Origin` when present; allows credentials. Intended for local tooling, not public multi-origin production.

## Related

- [API module](/modules/api.md)
- [Master password feature](/features/master-password.md)
