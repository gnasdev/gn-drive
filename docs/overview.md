---
type: architecture
title: "GN Drive Overview"
description: "Product scope, runtime shape, and operational guardrails."
tags: ["overview", "product"]
timestamp: 2026-07-11T00:00:00Z
status: active
---

# GN Drive Overview

## Purpose

GN Drive is a **local-only** desktop/server utility that runs cloud file synchronization via rclone, with a Vue 3 web UI and REST/SSE API in the same process.

It is not a multi-tenant SaaS, not a public reverse-proxied app, and not a Wails/native desktop shell.

## Goals

- One binary: CLI + sync engine + HTTP API + embedded SPA
- Loopback-only binding (`127.0.0.1`) on a **static** default port (`53241`)
- Optional master password encrypting config and DB at rest
- Flows as the primary workspace unit (sequential operations)
- Opt-in OS service for headless scheduling

## Non-goals

- Public internet exposure / reverse-proxy first-class support
- Embedding rclone as a Go library (shell-out only)
- Multi-user auth / remote accounts
- Shipping a separate desktop framework (Wails removed)

## Runtime modes

| Mode | Command | Portal unlock | Browser | Notes |
|------|---------|---------------|---------|-------|
| Foreground web | `gn-drive run` | Yes | Opens by default | Instance lock + fixed port |
| Dev | `task dev` / `run --dev` | Yes | Manual | Vite watch + air |
| Service | `run --service` after `service install` | No — password at start | No | Health file every ~5s |
| One-shot CLI | `sync`, `profile`, `remote`, `board`, … | No | No | Needs unlocked data plane |

## Product surface (web)

After unlock:

1. **Workspace** (`/`) — remotes strip + flow cards (operations, schedule, run/stop, live status)
2. **Settings** (`/settings`) — theme, locale, password, self-update
3. **Unlock** (`/unlock`) — setup or unlock master password

Profiles and boards remain in **store + CLI** (and some HTTP profile endpoints). The workspace does not treat profiles or boards as primary cards.

## Data on disk

Config directory: `~/.config/gn-drive/` (Linux: `$XDG_CONFIG_HOME/gn-drive` when set).

| File | Role |
|------|------|
| `auth.json` | Argon2id hash, lockout, app settings |
| `gn-drive.db` / `.enc` | SQLite (profiles, flows, boards, schedules, history, …) |
| `rclone.conf` / `.enc` | rclone remotes |
| `gn-drive.lock` / `.pid` | Single-instance coordination |
| `service.health` | Service heartbeat JSON |

## Security posture

- Bind only to loopback
- Master password optional; when set, DB + rclone conf encrypted (AES-256-GCM)
- Session cookie required for protected API after setup
- Rate limit on unlock (delay then 5-minute lockout)
- CORS reflects request origin for local tooling; not a public API design

## Related

- [Architecture](/architecture/overview.md)
- [API conventions](/shared/api-conventions.md)
- [Workspace feature](/features/workspace-portal.md)
- [Developer guide](../DEVELOPER.md)
