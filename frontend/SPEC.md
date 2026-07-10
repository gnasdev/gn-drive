# Frontend SPEC — single-page Workspace (desktop v0.4 port)

## Intent

Match the pre-Vue **Wails + Angular** product shell:

- No multi-page sidebar for Profiles / Remotes / Operations / Boards / Flows.
- **One workspace** after unlock: remotes strip + neo flow-style cards.
- **Solarized NeoBrutalism** tokens (light default; dark Solarized variant).
- Settings remains a secondary route; unlock is the auth gate.

## Shell

| Surface | Component | Notes |
| --- | --- | --- |
| Unlock | `UnlockPage.vue` | Public; full-screen |
| Workspace | `WorkspacePage.vue` | Home `/` — remotes + operations + boards + flows |
| Settings | `SettingsPage.vue` | `/settings` — theme, language, password, self-update |
| Layout | `App.vue` + `Topbar.vue` | Topbar only (accent bar); no sidebar |

Legacy paths (`/profiles`, `/remotes`, …) **redirect to `/`**.

## Domain mapping (old → current API)

| Desktop v0.4 concept | Backend today | Workspace UI |
| --- | --- | --- |
| Remote | `/api/v1/remotes` | Chip strip + add form at top |
| Operation (source/target + SyncConfig) | Profile (`/api/v1/profiles`) + `/api/v1/sync` | Operations card: each profile is a run unit |
| Board (DAG edge) | `/api/v1/boards` | Boards card list with Run |
| Flow (container + cron) | `/api/v1/flows` | Flow neo cards (steps not yet on BE) |

When flow **steps** land on the API, nest operations under flow cards like desktop `flow-card` + `operation-item`.

## Visual language

- 2px borders, hard offset shadow (`--shadow-neo`)
- Accent topbar (`--color-accent` Solarized yellow)
- Square corners (`--radius-md: 0`)
- Section headers use `.neo-header` (accent band)

## Checklist for new workspace blocks

1. Keep controls on `WorkspacePage` (or extract under `components/workspace/`).
2. Reuse stores (`profiles`, `remotes`, `boards`, `flows`, `operations`).
3. Prefer neo classes (`.neo-card`, `.btn-primary`, `.field-input`).
4. `pnpm exec vue-tsc --noEmit` + e2e smoke on `page-workspace`.
