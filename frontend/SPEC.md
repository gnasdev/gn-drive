---
type: spec
title: "Frontend SPEC — Workspace portal"
description: "Product surface and visual rules for the Vue SPA."
tags: ["frontend", "spec"]
timestamp: 2026-07-11T00:00:00Z
status: active
---

# Frontend SPEC — single-page Workspace

## Intent

Match the pre-Vue **Wails + Angular** product shell for day-to-day use:

- No multi-page sidebar for Profiles / Remotes / Operations / Boards / Flows
- **One workspace** after unlock: remotes strip + neo flow-style cards
- Solarized NeoBrutalism tokens (light default; dark Solarized variant)
- Settings is a secondary route; unlock is the auth gate

## Shell

| Surface | Component | Notes |
| --- | --- | --- |
| Unlock | `UnlockPage.vue` | Public; full-screen |
| Workspace | `WorkspacePage.vue` | Home `/` — remotes + flows |
| Settings | `SettingsPage.vue` | `/settings` — theme, language, password, self-update |
| Layout | `App.vue` + `Topbar.vue` | Topbar only (accent bar); no sidebar |

Legacy paths (`/profiles`, `/remotes`, `/operations`, `/boards`, `/flows`, `/dashboard`) **redirect to `/`**. Page components for some of those routes may still exist as unused stubs; do not re-wire them into primary navigation.

## Domain mapping

| Concept | Backend | Workspace UI |
| --- | --- | --- |
| Remote | `/api/v1/remotes` | Chip strip + add form |
| Flow | `/api/v1/flows` | Neo cards (primary unit) |
| Operation | nested under flow | Source/target + action + settings panel |
| Profile | `/api/v1/profiles` | **Not** a workspace unit (CLI/API option bag) |
| Board | store + CLI | **Not** on product surface |
| Sync progress | SSE + flow status | `FlowRunStatusPanel` (transfers tabs) |

Product actions for operations: **`push` | `bi` | `bi-resync`** only.

## Visual language

- 2px borders, hard offset shadow (`--shadow-neo`)
- Accent topbar (`--color-accent` Solarized yellow)
- Square corners (`--radius-md: 0`)
- Section headers use `.neo-header` (accent band)

## Stack

Vue 3.5, Vite 6, TypeScript, Pinia, vue-router, vue-i18n (en/vi), Tailwind 4, Phosphor icons, optional PWA plugin.

## Checklist for workspace changes

1. Keep controls on `WorkspacePage` (or extract under `components/flows/` / forms)
2. Reuse stores (`flows`, `remotes`, `auth`, …) — not a new multi-page IA
3. Prefer neo classes (`.neo-card`, `.btn-primary`, `.field-input`)
4. `pnpm exec vue-tsc --noEmit` + e2e smoke when touching run/auth paths
