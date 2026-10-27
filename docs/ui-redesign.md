# GN Drive UI audit and redesign (GN-59)

Scope: the GPUI window, `apps/gn-drive-gpui` (engine: `gn-drive-core`, unchanged). The Swift and Vue UIs are reference only.
Evidence: isolated mock instance (separate HOME and TMPDIR, `GN_DRIVE_MOCK=1`, dev socket), screenshots in `~/.agent-scratch/gnd-redesign/shots/before` and `.../after`. No real data, no OAuth.

## 1. Audit (before)

Severity: High = broken or blocks a task, Medium = hard to understand or find, Low = polish.

| # | Sev | Screen | Problem |
|---|-----|--------|---------|
| 1 | High | Settings | Opening Settings aborted the whole app: `ns_ui::TabStrip` called `tooltip` twice on one tab (compact strip with a tip). GPUI panics on that. Fixed first (milestone 0). |
| 2 | High | Navigation | Cloud accounts and Settings were reachable only through two unlabelled icons in a 40 px rail on the far right. A new user cannot tell where accounts live. The account count (UC-27) was only inside a tooltip. |
| 3 | High | Settings | Five icon-only tabs (General, Service, History, Logs, About) with no visible label, inside a ~230 px side column. History and Logs are cramped, timestamps are raw ISO (`2026-10-21T18:20:00Z`), rows can show raw ids (`F1:01`). |
| 4 | Medium | Canvas | Node cards were 170x56: three nodes read "This Mac", paths and cloud names are cut (`Google D…`, `/Users/me…`). The user cannot tell which folder is which without opening the inspector. |
| 5 | Medium | Workspace | The inspector has a fixed width and the sidebar is fixed. Nothing can be resized. |
| 6 | Medium | Status bar | Four overlapping messages: `Unlocked`, `Demo data (mock)`, a lock icon, `3 flows · demo data`. The flow count repeats the sidebar footer. |
| 7 | Medium | Sidebar | Footer spends its space on the shortcut text `⌘N new · ⌘R run`, a permanent hint, instead of navigation. |
| 8 | Medium | Canvas | A permanent one-line instruction bar sits under every canvas (`Drag a node to move it · …`). It never goes away and takes a full row. |
| 9 | Medium | Cloud accounts | Every account row has a filled red `Remove…` and a `Test connection` button, plus a `Not checked` chip that carries no information. Four accounts show four red buttons. |
| 10 | Medium | Inspector | Section headings are ALL CAPS and the long Connection form (include/exclude/min size…) has no summary at the top. |
| 11 | Low | Unlock | The settings-folder paragraph is a heavy bold box under the form; the password field is the only thing the user needs. |
| 12 | Low | Narrow window | At 390–600 pt the rail icons stay unlabelled and the flow list is a full-width page with no way to reach accounts or settings by name. |

Already fine and kept: Zed-style chrome and One Dark tokens, tooltip on every button, truncation with full text in a tooltip, wrapped errors, keyboard shortcuts, confirmations for destructive actions, language and theme switch, mock engine.

## 2. Redesign

### Information architecture

```
Sidebar (left, always visible, labelled)
  FLOWS            search / new
    Nightly Photos     3 connections · every day at 03:00
    …
  ────────────
  Cloud accounts  4     ⌘⇧R      (page)
  Settings              ⌘,       (page)

Main area (one of)
  Flow workspace   toolbar (name · status · summary · Add folder · Run/Stop)
                   canvas | inspector   (draggable divider)
  Cloud accounts   full page
  Settings         full page, labelled tabs: General · Service · History · Logs · About
Status bar         left: engine state · right: Demo data chip, Lock
```

Cloud accounts and Settings become full pages instead of a column that replaces the inspector, so History and Logs get room. The flow row click or Escape returns to the workspace. The rail icons stay (shared chrome) with the same tooltips.

### Components and visual language
- One nav row component: icon, label, count, shortcut, active state (accent bar + selected surface).
- Sentence-case headings everywhere; small-caps label only for the sidebar group name.
- Status colour is semantic only: green running/completed, amber waiting/stopped, red failed. Destructive actions are quiet buttons (outline, danger text) until confirmed; filled red only inside confirmation dialogs.
- Node card: title = label or last folder name, second line = place (`This Mac`, cloud account), third = path with middle ellipsis; kind icon tinted.
- Every state has a visible treatment: loading (skeleton), empty (what it is + one primary action), error (sentence + Details tooltip + retry).
- Every button, icon-only included, has a tooltip; every list row is keyboard-focusable.

## 3. Milestones and status

| Milestone | Scope | Status |
|-----------|-------|--------|
| M0 | `ns-ui` `TabStrip` double-tooltip crash (audit 1) | Done |
| M1 | Sidebar nav rows, Cloud accounts and Settings as pages, status bar (audit 2, 3, 6, 7, 9, 11) | Done |
| M2 | Node cards, canvas hint and toolbar, resizable inspector (audit 4, 5, 8, 10) | Done |
| M3 | Loading, empty and error states, cloud accounts list, unlock (audit 9, 11) | Done |
| M4 | Resizable sidebar, narrow windows, final polish (audit 5, 7, 12) | Done |

## 4. Results

Evidence: isolated mock instance (separate HOME and TMPDIR, `GN_DRIVE_MOCK=1`, dev socket, no OAuth, no real config). Full-size shots are in `~/.agent-scratch/gnd-redesign/shots/{before,m1,m2,m3,m4}`. Montages are next to this file in `ui-redesign/`.

| Audit row | Result |
|-----------|--------|
| 1 Settings crash | Fixed. `tab_tooltip` returns at most one tooltip per tab, with a unit test. |
| 2 Navigation | Cloud accounts (with count and `Cmd+Shift+R`) and Settings (`Cmd+,`) are labelled rows in the sidebar footer. Rail icons stay. |
| 3 Settings | Full page, labelled tabs, readable History and Logs. |
| 4 Node cards | Wider cards: title, place, middle-ellipsis path. Place and path stay visible at the compact minimum zoom. |
| 5 Resizing | Inspector and sidebar have draggable dividers (col-resize cursor, double-click resets, tooltip). Sidebar min 200, max 40% of the window, saved as `sidebarWidth`. |
| 6 Status bar | One message on the left, Demo data chip on the right. No flow count repeat. |
| 7 Sidebar footer | Navigation rows replace the shortcut hint. |
| 8 Canvas hint | Hint is dismissible and no longer a permanent row. |
| 9 Accounts | Quiet outline buttons, danger text only. Filled red only in confirmation dialogs. |
| 10 Inspector | Sentence-case sections with a summary on top. |
| 11 Unlock | One card, password field first. |
| 12 Narrow | The compact flow list shows the labelled nav rows. Pages fit 390 and 600 pt wide. A flow opens fitted at a readable zoom (about 60 percent, was 27). Dialog buttons wrap. |

Verification (macOS, Rust, `--features devtools`): `cargo fmt -p gn-drive-gpui`, `cargo clippy -p gn-drive-gpui --all-targets --features devtools -- -D warnings`, `cargo test -p gn-drive-gpui --features devtools` (90 passed, was 70 before M1), `cargo test -p ns-ui` (12 passed), `cargo build -p gn-drive-gpui --features devtools`. A unit test, `every_control_has_a_tooltip`, checks that each button uses a tip helper.

Shared crate `ns-ui` (also used by Hearth, Synca, Promptscope and GitButler GPUI): `TabStrip` tooltip fix (M0) and an additive `chrome_full_height_sized`. `chrome_full_height` keeps its signature and 260 pt default.

Known limits:
- A tooltip can stay on the canvas after a resize in screenshots. The synthetic dev pointer sends no mouse-leave, so this is a screenshot artifact. GPUI dismisses it on leave or move.
- At 390 pt the Settings tab strip truncates its labels. A long dialog title sits tight.
- Dev command added: `sidebar width=NNN` (also reported by `state.sidebarWidth`).

Screenshots: `final-before-after.png` (workspace, flow, node, accounts, history, OAuth dialog, unlock), `m3-states.png`, `m4-narrow-390.png`, `m4-narrow-600.png`, `m4-compact-flow-list.png`, `m4-sidebar-380.png`.
