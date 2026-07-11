---
type: development
title: "Development Conventions"
description: "Engineering conventions for backend, frontend, and docs in this repo."
tags: ["development"]
timestamp: 2026-07-11T00:00:00Z
status: active
---

# Development Conventions

Root practical guide: [DEVELOPER.md](../../DEVELOPER.md).

## Backend

- Wire dependencies in `app.New` / `AppDeps`; avoid new package-level service globals
- Keep store schema compatible with existing user data unless a migration is explicit
- Product surface actions for profiles/flows: `push` | `bi` | `bi-resync` only
- Prefer table-driven tests; override small funcs/vars for hermetic CLI/API tests
- English comments for contracts, invariants, and non-obvious edge cases only

## Frontend

- Workspace is the only primary surface after unlock; do not reintroduce multi-page nav for core domain
- Keep Pinia stores aligned with API types in `src/api/types.ts`
- Use neo/Solarized tokens and shared form components (`RemotePathField`, `CronField`, …)
- Always `credentials: 'same-origin'` on API calls
- Run `pnpm run type-check` before merging UI changes

## Docs

- OKF frontmatter with required `type`
- Current state only; changelog stays in root `CHANGELOG.md`
- Update `_sync.md` after meaningful docs/code alignment work
- Cross-link with bundle-relative paths where practical (`/modules/…`)

## Build / embed

Production and CI copy `frontend/dist` → `internal/webui/dist` before `go build`. Dev builds straight into `internal/webui/dist` via Vite `--outDir` so `//go:embed` sees changes after air restart.
