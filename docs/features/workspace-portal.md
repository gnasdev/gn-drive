---
type: feature
title: "Workspace Portal"
description: "Single-page workspace after unlock: remotes + flows with live run status."
tags: ["feature", "ui", "workspace"]
timestamp: 2026-07-11T00:00:00Z
status: active
---

# Workspace Portal

## Behavior

`gn-drive run` starts a local web portal. Users unlock (or set up a password), then land on a single **Workspace** page.

| Route | Page |
|-------|------|
| `/unlock` | Setup / unlock (public) |
| `/` | Workspace — remotes strip + flow cards |
| `/settings` | Theme, language, password, self-update |
| Legacy multi-page paths | Redirect to `/` |

## Workspace contents

- **Remotes** — chip strip + add/test/delete
- **Flows** — neo cards with operations, schedule, run/stop, edit mode, live `FlowRunStatusPanel`
- Profiles and boards are **not** primary workspace cards

## Auth gate

Router waits for `/api/v1/status`. Protected routes require `unlocked` (setup false still allows access until password is configured). Session cookie required when password is set and unlocked.

## SSE

SPA connects to `/api/v1/events` for live progress; UI shows connection state when injected.

## Related

- [frontend/SPEC.md](../../frontend/SPEC.md)
- [Flows](/features/flows-and-operations.md)
- [Master password](/features/master-password.md)
