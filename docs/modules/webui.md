---
type: module
title: "Web UI Embed"
description: "go:embed of Vue dist with SPA fallback handler."
tags: ["module", "webui", "frontend"]
timestamp: 2026-07-11T00:00:00Z
status: active
compliance: current-state
---

# Web UI Embed

Package: `internal/webui`.

## Responsibility

Embed `dist/` via `//go:embed all:dist` and serve with SPA fallback to `index.html` for client routes.

## Build contract

| Mode | Output path |
|------|-------------|
| `task build` / CI | `frontend` build → copy to `internal/webui/dist` |
| `task dev` | Vite `--outDir ../internal/webui/dist` directly |

Hand-editing `internal/webui/dist` is not supported; regenerate from `frontend/`.

## Source SPA

`frontend/` — see [frontend/SPEC.md](../../frontend/SPEC.md) and [Workspace feature](/features/workspace-portal.md).
