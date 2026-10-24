# GN Drive — complete use-case inventory (parity contract for the GPUI window)

Scope: every user-visible feature of GN Drive as it exists in the Swift rewrite
(`Sources/GNDriveApp`, `Sources/GNDriveCore`, `Sources/gn-drive`), plus every
feature that exists only in the legacy Go HTTP API / Vue SPA (explicitly marked
`LEGACY-ONLY` so it is not accidentally treated as a requirement).

Conventions:
- `[ ]` — not implemented in the Rust window yet (all items start unchecked).
- `Engine:` — the `gn-drive serve --stdio` method(s) that must back the item
  (see `rpc.md`). `—` means no engine call is needed.
- `Source:` — primary implementation reference. `Swift` = current native app;
  `Go`/`Vue` = legacy behavior kept for parity comparison.
- Items marked `GAP` describe a behavioral divergence or defect in the current
  Swift app that the Rust window must decide on explicitly. The recommended
  behavior for parity is stated in `Notes`.
- Field names in RPC payloads are camelCase unless stated otherwise; the
  persisted `sync_config` object keeps its historical snake_case keys.

---

## A. App shell, unlock, master password

- [ ] UC-1 First launch / no password configured — the user sees the setup card
  ("Set up master password") with a password field, a confirm field, the
  explanation that `gn-drive.db` and `rclone.conf` will be encrypted with
  Argon2id + AES-256-GCM, and a "Set up & unlock" button.
  Source: `Sources/GNDriveApp/UnlockView.swift:10,21-23,29-33,43`;
  `Sources/GNDriveCore/AuthService.swift:160-177`;
  `docs/features/master-password.md`.
  Engine: `auth.status`, `auth.setup`.
  Notes: setup is only offered when `status.setup == false`; the app is already
  unlocked in that state (no password on disk).

- [ ] UC-2 Setup input validation — the submit button stays disabled until the
  password has at least 4 characters and both fields match; submitting with a
  mismatch shows "Passwords do not match.", submitting too short shows
  "Password must be at least 4 characters.".
  Source: `UnlockView.swift:45,56-61`; `AuthService.swift:163`.
  Engine: `auth.setup`.
  Notes: Swift counts Characters (`.count`), legacy Go counted bytes; this is a
  deliberate, compatible difference for multibyte passwords.

- [ ] UC-3 Setup success — the screen is replaced by the workspace, a "Password
  set" toast appears, `auth:unlocked` is published, and flows/remotes/profiles/
  history/runtime are loaded.
  Source: `Sources/GNDriveApp/AppState.swift:96-107`.
  Engine: `auth.setup`, `state.snapshot`.
  Notes: setup never fails for "already setup" from this screen because the
  screen is only shown when `setup == false`; the engine still returns
  `already_setup` for racing clients.

- [ ] UC-4 Unlock screen — when a password exists and the process starts locked,
  the user sees "Enter your master password to decrypt and access gn-drive",
  one password field, and an "Unlock" button (also submitted with Enter).
  Source: `UnlockView.swift:10,21-28,43`; `AppContainer.swift:63-68`.
  Engine: `auth.status`, `auth.unlock`.
  Notes: the native app starts in portal mode (`AppState.swift:42`), so the
  engine is alive while locked and the data plane is deferred.

- [ ] UC-5 Unlock success — workspace appears, "Unlocked" toast, data plane
  opens (SQLite + rclone), runtime projection reloads.
  Source: `AppState.swift:80-94`; `AppContainer.swift:118-120`.
  Engine: `auth.unlock`, `state.snapshot`.
  Notes: unlock is a no-op success when already unlocked
  (`AuthService.swift:179+`).

- [ ] UC-6 Wrong password — an inline/toast error is shown, the form stays open,
  and the lockout counters update.
  Source: `AppState.swift:88-93`; `UnlockView.swift:35-41`.
  Engine: `auth.unlock` → `invalid_password`.
  Notes: error text is the engine's `String(describing:)` today; the RPC must
  return structured codes so the window can localize.

- [ ] UC-7 Lockout countdown — after repeated failures the screen shows
  "Locked — retry in Ns" and unlocking is rejected until the timer expires.
  Source: `UnlockView.swift:38-41`; `AppState.swift:88-89`;
  `AuthService.swift:146-158,179-233`.
  Engine: `auth.status` (lockout), `auth.unlock` → `locked`.
  Notes: 10 failures trigger a 5-minute lockout; failures 3–9 sleep
  2^(n−3) seconds before verifying (1,2,4,…,64 s). Expose
  `retryAfterSecs` and `failedAttempts`.

- [ ] UC-8 Lock from the workspace toolbar — a confirmation alert ("Lock app? You
  will need your master password to unlock again.") then the data plane closes,
  config files are re-encrypted, runtime state clears, and the unlock screen
  appears.
  Source: `WorkspaceView.swift:52-66`; `AppState.swift:109-120`;
  `AppContainer.swift:123-132`; `AuthService.swift:234-242`.
  Engine: `auth.lock`.
  Notes: `beforeLock` must run before `auth.lock` so SQLite/rclone release the
  plaintext files before encryption. On failure the app stays unlocked and
  toasts the error.

- [ ] UC-9 Lock from Settings — "Lock app" closes Settings and locks immediately
  (no confirmation dialog in the current Swift UI).
  Source: `SettingsView.swift:33`.
  Engine: `auth.lock`.
  Notes: GAP: legacy Settings also locked without confirmation, but the toolbar
  path confirms. Recommend one shared confirmation for both entry points.

- [ ] UC-10 Change master password — in Settings, enter current + new password;
  the button is enabled only when old is non-empty and new has ≥4 characters.
  On success inputs clear and "Password changed" is toasted.
  Source: `SettingsView.swift:20-31`; `AppState.swift:122-131`;
  `AuthService.swift:259-298`.
  Engine: `auth.changePassword`.
  Notes: wrong old password → `invalid_password`; new < 4 → `password_too_short`.
  The hash/keychain key are rotated and config is re-encrypted with the new key.

- [ ] UC-11 Password change session semantics — GAP: current Swift
  `AppState.changePassword` does not call `beforeLock()` and does not force a
  re-lock, while legacy Go cleared all sessions and locked the app after a
  change. Recommended for the RPC: keep the engine semantics
  (`AuthService.changePassword` stays unlocked and re-encrypts with the new
  key) but have the window navigate to the unlock screen and call `auth.lock`
  after a successful change, matching the documented legacy UX.
  Source: `AppState.swift:122-131` vs
  `~/.agent-scratch/qoder/gn/gnd-legacy-api.md` §1 (change-password).
  Engine: `auth.changePassword`, `auth.lock`.
  Notes: documenting this is required so the Rust window does not inherit the
  SwiftUI regression.

- [ ] UC-12 Remove master password — Settings → "Remove password" opens a
  confirmation alert containing a password field; confirming decrypts the
  config permanently, deletes `auth.json`/`.enc` files, reopens the data plane,
  and shows "Password removed".
  Source: `SettingsView.swift:35,95-103`; `AppState.swift:133-145`;
  `AuthService.swift:299-314`.
  Engine: `auth.removePassword`.
  Notes: wrong password → `invalid_password` and the app must remain usable.
  After removal `auth.status.setup == false` and the setup screen is not shown.

- [ ] UC-13 Remembered unlock key — in interactive sessions the derived key is
  stored in the login Keychain under service `gn-drive` and account
  `remember-unlock/v1/<sha256(configDir)>`; a restart can resume without
  prompting. Stale keys are deleted on mismatch.
  Source: `KeychainStore.swift`; `AuthService.swift:81-105,175,234+`;
  `AppState.swift:43`.
  Engine: `auth.status` (unlocked may be true after process start),
  `hello`.
  Notes: `gn-drive serve --stdio` must use the same Keychain store as the app
  so resume behaves identically. Service/CLI headless mode uses
  `--password`/`GN_DRIVE_PASSWORD` instead.

- [ ] UC-14 Quit suspends safely — quitting the app stops the sync engine and
  closes the container; if the config was locked-by-policy the files are
  re-encrypted while the remembered key is kept.
  Source: `GNDriveApp.swift:17-22`; `AppState.swift:70-73`;
  `AppContainer.swift:134-140`.
  Engine: `app.quit` (graceful) / stdin EOF.
  Notes: `RuntimeHub` and the event bus are closed, so no events may be emitted
  after the shutdown line.

- [ ] UC-15 Language switch — the toolbar globe menu switches English ↔
  Tiếng Việt immediately (whole tree re-renders) and persists in
  `UserDefaults` key `gn-drive:locale`.
  Source: `WorkspaceView.swift:31-37`; `GNDriveApp.swift:43,56`;
  `L10n.swift:5-14`.
  Engine: — (window-local preference; `settings.get` may return the initial
  value).
  Notes: the full en/vi string table is embedded in `L10n.swift` (~298 keys)
  and must be ported or exposed to the window.

- [ ] UC-16 Theme switch — the toolbar sun/moon button toggles light/dark,
  persisted in `UserDefaults` key `gn-drive:theme` (default light).
  Source: `WorkspaceView.swift:39-42`; `GNDriveApp.swift:42,55`.
  Engine: —.
  Notes: light is Solarized; dark is graphite/teal per
  `docs/features/workspace-portal.md`. A `system` value exists in
  `ThemePreference` but is not reachable from the UI.

- [ ] UC-17 Toast feedback — short success/error messages appear at the bottom
  center and auto-dismiss after 4 seconds.
  Source: `AppState.swift:305-311`; `GNDriveApp.swift:63-79`.
  Engine: — (all methods return errors; the window renders toasts).
  Notes: error toasts use red, success toasts use near-black.

- [ ] UC-18 Root routing — locked ⇒ unlock view; unlocked + settings open ⇒
  settings; unlocked otherwise ⇒ workspace. Toasts overlay all three.
  Source: `GNDriveApp.swift:45-60`.
  Engine: `auth.status`.
  Notes: closing Settings returns to the workspace without reloading the
  workspace (state is held by the engine/window).

---

## B. Workspace shell and flow rail

- [ ] UC-19 Three-pane workspace — rail (180–260 pt), canvas (≥400 pt),
  inspector (240–380 pt) in a resizable split.
  Source: `WorkspaceView.swift:10-18`.
  Engine: `state.snapshot`.
  Notes: minimum window size is 900×600 (`GNDriveApp.swift:29`).

- [ ] UC-20 Engine connection indicator — a green dot plus "connected" label in
  the toolbar. The native app has no reconnect state because the engine is
  in-process.
  Source: `WorkspaceView.swift:20-29`.
  Engine: `hello` / `ping`.
  Notes: the GPUI window should render `connected` only while the stdio
  handshake is alive, and show a reconnecting/offline state if the child dies.

- [ ] UC-21 Toolbar actions — language menu, theme toggle, remotes sheet,
  settings, lock.
  Source: `WorkspaceView.swift:30-56`.
  Engine: —.
  Notes: all five controls must have tooltips/accessibility labels.

- [ ] UC-22 Flow list — every flow is listed with a status dot, name (or
  "Untitled flow"), and a caption "{n} ops" plus "· ⏰" when a schedule is
  enabled. Selection is driven by `selectedFlowID`.
  Source: `WorkspaceView.swift:92-98,111-130`; `AppState.swift:169-175`.
  Engine: `flows.list`, `runtime.snapshot`.
  Notes: flows come back ordered by `sort_order, name`.

- [ ] UC-23 Add flow — the "+" button in the rail header creates a flow named
  "Untitled flow", selects it, and toasts "Flow added".
  Source: `WorkspaceView.swift:85-88`; `AppState.swift:180-193`.
  Engine: `flows.create`.
  Notes: the id is a UUID generated by the engine, not the client (legacy Go
  required a client-supplied id; do not reproduce that requirement).

- [ ] UC-24 Flow context menu — right-clicking a row offers Run, Stop, and
  Delete.
  Source: `WorkspaceView.swift:131-136`.
  Engine: `flows.execute`, `flows.stop`, `flows.delete`.
  Notes: Run/Stop are always enabled in the menu; the engine returns
  `already_running` / `not_running`, which the window should surface as a
  toast. Recommended: disable Run while active and Stop while idle.

- [ ] UC-25 Delete flow confirmation — deleting always asks
  `Delete flow "{name}"?`, then removes the flow, its operations, its cron
  schedule, and its runtime projection entry.
  Source: `WorkspaceView.swift:137-142`; `AppState.swift:220-235`.
  Engine: `flows.delete`.
  Notes: a running flow cannot be deleted (`flow_busy`); the Swift app toasts
  "Flow is running — stop it first".

- [ ] UC-26 Empty workspace — with no flows the canvas shows "Select or create a
  flow" and the rail is empty.
  Source: `FlowCanvasView.swift:146-149`; `InspectorView.swift:37-39`.
  Engine: `flows.list`.
  Notes: the legacy Vue canvas had a dedicated "No flows yet." string; the
  Swift app does not. Either state is acceptable, but it must be explicit.

- [ ] UC-27 Remotes entry in the rail — a bottom button "Remotes (N)" opens the
  remotes sheet and shows the configured remote count.
  Source: `WorkspaceView.swift:100-106`.
  Engine: `remotes.list`.
  Notes: the count must refresh after add/delete and after unlock.

- [ ] UC-28 Settings navigation — gear opens the Settings screen; Close returns
  to the workspace.
  Source: `WorkspaceView.swift:49-50`; `GNDriveApp.swift:49-51`;
  `SettingsView.swift:88-93`.
  Engine: —.
  Notes: settings state (password fields, history) reloads when reopened.

---

## C. Flow editing and lifecycle

- [ ] UC-29 Select a flow — clicking a rail row selects it; the canvas rebuilds
  for that flow and the inspector shows its fields.
  Source: `WorkspaceView.swift:92-97`; `FlowCanvasView.swift:172-175,229-240`.
  Engine: `flows.get`, `runtime.snapshot`.
  Notes: switching to a one-edge flow auto-selects that edge and opens its card
  (UC-72).

- [ ] UC-30 Rename flow — the inspector name field stages edits locally; the
  rail updates immediately but the database is untouched until Save.
  Source: `InspectorView.swift:110-115`; `AppState.swift:196-199`.
  Engine: `flows.save`.
  Notes: an empty name is allowed in the DB; the UI displays "Untitled flow".

- [ ] UC-31 Unsaved-changes state — staged name/cron/node/edge/option edits set
  the flow's dirty flag, show an orange "Discard changes?" caption and an
  exclamation badge on Save.
  Source: `InspectorView.swift:21-24,67-76`; `AppState.swift:196-199`.
  Engine: —.
  Notes: the dirty set is window state; it must survive selection changes but
  need not survive a process restart.

- [ ] UC-32 Save flow — explicitly persists the flow (name, schedule, canvas
  JSON, operations/sync config), re-registers its cron schedule, publishes
  `state:changed(domain: flows)`, clears dirty, and toasts "Flow saved: …".
  Source: `InspectorView.swift:67-71,89-92`; `AppState.swift:202-218`;
  `Store.swift:639-682`.
  Engine: `flows.save`.
  Notes: Save is disabled when clean or while the flow is running. Saving
  replaces all operation rows (`Store.saveFlow`), so the payload must contain
  every operation with a stable id.

- [ ] UC-33 Save is blocked while running — the Save button is disabled and
  `AppState.saveFlow` also guards, toasting "Cannot change this while a run is
  in progress".
  Source: `InspectorView.swift:70`; `AppState.swift:203-207`.
  Engine: `flows.save` → `flow_busy`.
  Notes: the engine returns `flow_busy` for `flows.save`/`flows.delete` when
  the flow status is running/cancelling (legacy HTTP 409 semantics).

- [ ] UC-34 Schedule toggle and cron expression — the inspector exposes an
  enabled toggle and a cron text field; both stage edits and are disabled while
  running.
  Source: `InspectorView.swift:117-127`; `SyncEngine.swift:300-330`.
  Engine: `flows.save`.
  Notes: saving a flow with `scheduleEnabled` registers cron job `flow:<id>`;
  disabling or deleting unregisters it. Cron accepts 5 fields, 6 fields with
  seconds, and descriptors (`@hourly`, `@daily`, …).

- [ ] UC-35 Cron presets — a "…" menu offers Every hour, Every 6 hours, Daily at
  midnight, Weekdays 9:00, Weekly Sunday; a hint explains the 5-field format.
  Source: `InspectorView.swift:294-322`.
  Engine: —.
  Notes: presets write into the same cron field (staged until Save).

- [ ] UC-36 Run flow from the inspector — Run is disabled for an empty flow;
  otherwise dirty edits are saved first, the flow executes in the background,
  and "Flow started: {name}" is toasted.
  Source: `InspectorView.swift:49-62,79-87`; `AppState.swift:237-244`;
  `FlowEngine.swift:62-81`.
  Engine: `flows.save` (if dirty), `flows.execute`.
  Notes: execution is sequential in `sortOrder`; the first failed operation
  fails the flow and later operations do not run. A concurrent execute returns
  `already_running`.

- [ ] UC-37 Run flow from the rail — the context menu runs the flow directly
  without the empty/dirty pre-checks.
  Source: `WorkspaceView.swift:132`.
  Engine: `flows.execute`.
  Notes: an empty flow returns `empty_flow`; dirty edits are not auto-saved on
  this path. Recommended: route both entry points through the same
  save-if-dirty + empty check behavior.

- [ ] UC-38 Empty-flow guard — running a flow with no operations must not start
  a run and must tell the user.
  Source: `InspectorView.swift:61,81-84`; `FlowEngine.swift:65`.
  Engine: `flows.execute` → `empty_flow`.
  Notes: the inspector also toasts "Flow has no operations" (not localized in
  the current Swift app).

- [ ] UC-39 Stop a running flow — Stop replaces Run in the toolbar and in the
  context menu; the flow enters "cancelling", the active rclone task is
  terminated, and the flow ends "cancelled".
  Source: `InspectorView.swift:52-56`; `AppState.swift:246-253`;
  `FlowEngine.swift:83-96,139-155`.
  Engine: `flows.stop`.
  Notes: stopping a flow that is not running returns `not_running`.

- [ ] UC-40 Flow run status — the flow status dot/label uses
  running/cancelling/completed/failed/cancelled/idle colors.
  Source: `WorkspaceView.swift:116-127,146-155`;
  `InspectorView.swift:130-138`.
  Engine: `runtime.snapshot`.
  Notes: `FlowEngine.status` retains the last terminal status after a run;
  RuntimeHub overlays it into the snapshot, defaulting to "idle".

- [ ] UC-41 Event log — the inspector shows the last 12 runtime log entries for
  the selected flow (`label`, `status`, optional humanized error).
  Source: `InspectorView.swift:143-157`; `RuntimeHub.swift:124-128,228-238`.
  Engine: `runtime.snapshot`, `flow:execution` events.
  Notes: engine keeps 40 entries per flow and drops consecutive duplicates
  (same status/opID/error). Labels are `Flow` or `Op <first 8 chars>`.

- [ ] UC-42 Humanized errors — error strings strip `rclone:`, cut `(stderr:`
  details, collapse JSON/stat blobs to "Sync failed. Check paths and remote
  configuration.", hide cancellation noise, and truncate at 160 characters.
  Source: `Helpers.swift:7-28`; `RuntimeHub.swift`.
  Engine: all run methods/events.
  Notes: cancellation detection covers `signal: killed`,
  `signal: interrupt`, `context canceled/cancelled`, `errtaskcancelled`,
  `task cancelled`; cancelled/cancelling statuses always hide the error.

- [ ] UC-43 Cancelled runs are not errors — a user stop must show "Stopped"
  rather than an error and must not set a last-error banner.
  Source: `Helpers.swift:30-36`; `RuntimeHub.swift:107-122`; `FlowEngine.swift`.
  Engine: `flows.stop`, `flow:execution`.
  Notes: when a flow is cancelled, RuntimeHub clears the flow-level last error.

- [ ] UC-44 Busy lock — while a flow is running: name, cron, enabled, node
  fields, remote picker, path editors, option panel, Add node, Save, Delete
  node/edge, canvas connect handle, and Delete key are unavailable; node drags
  do not move nodes.
  Source: `InspectorView.swift:52-70,115-241`; `FlowCanvasView.swift:44-48,123-130,372-385,497-498`.
  Engine: `runtime.snapshot`.
  Notes: this lock is derived from status running/cancelling, not from a local
  boolean.

- [ ] UC-45 Flow ordering — flows are persisted with `sortOrder` and returned
  sorted by `(sortOrder, name)`. There is no reorder UI.
  Source: `Store.swift:575-585`; `Models.swift:204-217`.
  Engine: `flows.list`, `flows.save`.
  Notes: legacy flow cards had the same model but no drag-reorder either.

- [ ] UC-46 Collapsed-flow state — `isCollapsed` is persisted and exposed but
  the current Swift UI never edits it.
  Source: `Models.swift:205`; `Store.swift:586-606`.
  Engine: `flows.save`.
  Notes: LEGACY-ONLY UI (Vue flow cards had expand/collapse). Keep the field
  round-trippable; the GPUI window may ignore it.

- [ ] UC-47 Discard staged edits — GAP: the Swift inspector shows "Discard
  changes?" but offers no discard action; switching flows preserves staged
  edits in memory, and there is no confirm-on-leave.
  Source: `InspectorView.swift:74-76`; `AppState.swift:196-199`.
  Engine: —.
  Notes: legacy Vue cards had a discard-edit confirmation. Recommended: add an
  explicit "Discard" that reloads the flow via `flows.get` and clears dirty.

- [ ] UC-48 Create-default values — new flows get a UUID, name "Untitled flow",
  empty operations, `canvasJson = "{}"`, schedules off.
  Source: `AppState.swift:180-188`; `Store.swift:639+`.
  Engine: `flows.create`, `flows.save`.
  Notes: a flow with no canvas renders a default layout once operations exist
  (UC-52).

---

## D. Flow canvas and graph

- [ ] UC-49 Render the flow graph — locations are nodes (`remote`, `path`,
  `label`, `x`, `y`) and each operation is a directed edge carrying its action;
  the graph is rebuilt whenever flows/canvas/runtime change.
  Source: `FlowCanvasView.swift:50-176,200-227`; `FlowGraph.swift:48-179`.
  Engine: `flows.get`, `runtime.snapshot`.
  Notes: the canvas is a projection of operations; execution never reads the
  canvas. `canvasJson` stores `{viewport:{x,y,zoom}, nodes:[…]}`.

- [ ] UC-50 Default layout — when `canvasJson` has no nodes, operations are laid
  out as sources in column 0 and targets in column 280, rows 140 apart, deduped
  by `(remote, normalizedPath)`.
  Source: `FlowGraph.swift:103-131`; `FlowCanvasView.swift:215-225`.
  Engine: —.
  Notes: missing endpoints in stored canvas nodes are auto-placed at column 280
  in operation order; edges referencing missing nodes are dropped.

- [ ] UC-51 Stable node identity — node ids are `n_<fnv1a32(key)>` of
  `remote\0path`; labels/positions persist.
  Source: `FlowGraph.swift:57-70,74-101`.
  Engine: —.
  Notes: do not generate random ids for derived nodes; use the same hash so old
  `canvas_json` and new layouts agree.

- [ ] UC-52 Pan — dragging the empty canvas pans the viewport.
  Source: `FlowCanvasView.swift:160,411-421`.
  Engine: —.
  Notes: legacy Vue panned with the scroll wheel; the Swift app uses drag-pan
  and has no scroll-wheel pan. The GPUI window should support both.

- [ ] UC-53 Zoom — pinch magnifies between 0.4× and 2.0×.
  Source: `FlowCanvasView.swift:161,423-430`.
  Engine: —.
  Notes: no zoom buttons, minimap, or double-click zoom.

- [ ] UC-54 Fit view — on first display, if no viewport is stored and nodes
  exist, the canvas scales/centers content (max 1.2×, clamped 0.4–2.0).
  Source: `FlowCanvasView.swift:165-169,179-196`.
  Engine: —.
  Notes: fitting writes the resulting viewport to `canvas_json` immediately.

- [ ] UC-55 Viewport persistence — pan/zoom end persists
  `{x,y,zoom}` into `canvas_json` without marking the flow dirty.
  Source: `FlowCanvasView.swift:417-420,451-458`.
  Engine: `flows.save`.
  Notes: this is a full flow save; it must be rejected/harmless while running
  (the Swift app calls `AppState.saveFlow` which toasts and returns).

- [ ] UC-56 Node drag — dragging a node moves it; releasing persists all node
  positions into `canvas_json`.
  Source: `FlowCanvasView.swift:120,372-386,434-449`.
  Engine: `flows.save`.
  Notes: while running, drag is ignored entirely.

- [ ] UC-57 Connect nodes — dragging the right-edge handle of a node onto
  another node creates a `push` operation between the two locations, persists
  immediately, and selects the new edge.
  Source: `FlowCanvasView.swift:123-130,388-409,460-486`.
  Engine: `flows.save`.
  Notes: legacy Vue also persisted on connect. The edge id is a fresh UUID; the
  operation inherits source/target remote+path and `sync_config.action=push`.

- [ ] UC-58 Duplicate connection — connecting the same ordered pair with the
  same action is rejected with "This connection already exists".
  Source: `FlowCanvasView.swift:462-465,488-494`; `FlowGraph.swift:180-198`.
  Engine: `connect_error` (client-side check).
  Notes: the reverse direction and the same pair with a different action are
  allowed.

- [ ] UC-59 Self-loop — connecting a node to itself is rejected with "Cannot
  connect a node to itself".
  Source: `FlowGraph.swift:190-191`; `FlowCanvasView.swift:488-494`.
  Engine: —.
  Notes: missing endpoint/unknown node are internal errors; do not surface raw
  codes.

- [ ] UC-60 Selection — clicking a node selects it; clicking near an edge
  (10 pt hit test along the path) selects that edge and opens its card; clicking
  empty canvas selects the flow and closes the card.
  Source: `FlowCanvasView.swift:121,152-159,365-370,517-527`.
  Engine: —.
  Notes: only one selection exists (no multi-select/box select).

- [ ] UC-61 Edge rendering — bezier edges with 1.6 pt strokes (2.5 pt when
  selected); colors: running accent, completed green, failed red,
  cancelled/cancelling orange, idle gray.
  Source: `FlowCanvasView.swift:71-109,309-317`.
  Engine: `runtime.snapshot`.
  Notes: bidirectional actions share the same single curve in the Swift app;
  the legacy Vue drew arrowheads on both ends. Recommended for parity: draw
  both arrowheads.

- [ ] UC-62 Edge label — `action · done/total · K/s`, where the file count and
  speed come from the edge's live sync snapshot.
  Source: `FlowCanvasView.swift:85-91,319-330`.
  Engine: `runtime.snapshot`.
  Notes: `K/s` is integer KiB/s; omitted when no bytes have transferred.

- [ ] UC-63 Live file dots — files with status transferring/checking/failed/
  completed/checked render as 7 px dots on the edge; pending files never render
  as dots.
  Source: `FlowCanvasView.swift:93-101,269-279`.
  Engine: `runtime.snapshot`, `sync:progress`.
  Notes: maps status→color: transferring accent, failed red, completed green,
  checking/checked teal. This is a hard AGENTS.md rule.

- [ ] UC-64 Dot placement — queued/live files ride between 18% and 70% of the
  path proportional to progress; failed park at 50%; success at 98%;
  bidirectional actions mirror to `1 − t`.
  Source: `FlowCanvasView.swift:34-38,281-297`.
  Engine: —.
  Notes: GAP vs legacy Vue: Vue used each file's raw progress (0–1) for
  transferring files and alternated direction by odd/even index for bisync;
  Swift maps progress into the queue band and mirrors every dot. Pick one and
  document it; current Swift behavior is authoritative for this spec.

- [ ] UC-65 Dot cap and order — at most 24 dots per edge, ordered by status
  rank (transferring, failed, checking, checked, completed).
  Source: `FlowCanvasView.swift:271-279`.
  Engine: —.
  Notes: pending is excluded before ranking.

- [ ] UC-66 Connect handle hidden while running — no handle is drawn and no new
  connection can be started.
  Source: `FlowCanvasView.swift:122-130,461`.
  Engine: `runtime.snapshot`.

- [ ] UC-67 Delete key — Delete/Forward-Delete removes the selected node (and
  all incident edges) or the selected edge, persists immediately, clears the
  card, and selects the flow.
  Source: `FlowCanvasView.swift:162-164,496-515`.
  Engine: `flows.save`.
  Notes: ignored while running. Inspector deletion uses the same rules (UC-76,
  UC-79).

- [ ] UC-68 Add node — "Add node" creates a local node at `/new-<epoch>`,
  appends it to the canvas, stages the flow, and selects it.
  Source: `InspectorView.swift:63-66,94-105`.
  Engine: `flows.save`.
  Notes: no operation is created until the node is connected; unconnected nodes
  are dropped from operations but remain in `canvas_json` (round-tripped).

- [ ] UC-69 Single-edge auto-select — switching to a flow with exactly one edge
  selects it and opens its file card.
  Source: `FlowCanvasView.swift:229-240`.
  Engine: —.

- [ ] UC-70 Edge card anchoring — the card is positioned near the edge midpoint
  (above, or below when there is no room), clamped to the canvas width, and
  survives pan/zoom because it is positioned in canvas coordinates.
  Source: `FlowCanvasView.swift:134-144`.
  Engine: —.
  Notes: AGENTS.md requires the card not to be dismissed by pan/zoom.

- [ ] UC-71 Card dismissal — the × button, an empty-canvas click, selecting
  another edge, or switching flows closes/changes the card.
  Source: `FlowCanvasView.swift:140,152-159,365-370`.
  Engine: —.
  Notes: selecting a node does not close the card in the current Swift code
  (only pane click does). Decide explicitly; AGENTS.md allows only an explicit
  outside click or user action.

- [ ] UC-72 Auto-open card for active operation — when the runtime reports a
  running/checking op for the selected flow, the card opens for that edge (and
  selects it when nothing is selected).
  Source: `FlowCanvasView.swift:242-251`.
  Engine: `runtime.snapshot`.
  Notes: this is the AGENTS.md "selected edge with active operation opens its
  file card" rule.

- [ ] UC-73 Node visuals — a 170×56 card showing a cloud icon when a remote is
  set (folder for local), the label (or remote/"Local"), and the path; selected
  nodes use an accent 2 pt border.
  Source: `FlowCanvasView.swift:532-561`.
  Engine: —.
  Notes: remote values `""` and `"local"` both mean local.

- [ ] UC-74 Canvas empty/unknown flow — "Select or create a flow" is shown when
  no flow is selected.
  Source: `FlowCanvasView.swift:146-149`.

---

## E. Inspector: node, edge, and sync-config editor

- [ ] UC-75 Inspector empty state — with no selected flow the panel shows
  "Select or add a flow.".
  Source: `InspectorView.swift:37-39`.
  Engine: —.

- [ ] UC-76 Node inspector — shows "Location", a label field, a Remote picker
  ("Local" + each remote name), a path text field with Browse, and Delete node.
  Source: `InspectorView.swift:162-188`.
  Engine: `remotes.list`, `fs.list`.
  Notes: every field stages the flow; Delete persists immediately.

- [ ] UC-77 Change a node's remote — picking a remote changes the node's
  `remote` and therefore every incident operation endpoint on save.
  Source: `InspectorView.swift:173-179`; `FlowGraph.swift:200-219`.
  Engine: `flows.save`.
  Notes: remote deletion can leave nodes pointing at a missing remote; the
  picker only lists existing remotes, so the stored value may not have a menu
  item. Preserve it rather than silently resetting.

- [ ] UC-78 Node path editing — typing a path stages it; Browse opens the
  picker (UC-82).
  Source: `InspectorView.swift:181-183`; `Helpers.swift:51-63`.

- [ ] UC-79 Delete node from the inspector — deletes the node, cascades its
  edges, saves immediately, and selects the flow.
  Source: `InspectorView.swift:185-186,256-265`.
  Engine: `flows.save`.
  Notes: disabled while running.

- [ ] UC-80 Edge inspector — shows "Sync", editable source/target path fields
  for the endpoint nodes, the live op status/error, the full options panel, and
  Delete.
  Source: `InspectorView.swift:192-242`.
  Engine: `runtime.snapshot`, `flows.save`.
  Notes: the source/target *remote* is edited through the endpoint node, not
  here; changing the source/target path here edits the node itself and thus all
  incident edges.

- [ ] UC-81 Operation action — a picker offers Push, Bi (two-way), and Bi
  resync; changing it updates both `operation.action` and
  `sync_config.action`.
  Source: `InspectorView.swift:229-237`; `SyncConfigEditor.swift:224-229`;
  `Models.swift:185-203`.
  Engine: `flows.save`.
  Notes: `pull` is not offered on flows; stored `pull`/unknown values normalize
  to `push`. Endpoint slots never swap when the action changes.

- [ ] UC-82 Dry run toggle — marks the operation to run rclone without changing
  files.
  Source: `SyncConfigEditor.swift:230`.
  Engine: `flows.save`; `flows.execute` honors `dryRun`.

- [ ] UC-83 Performance options — parallel transfers, bandwidth (MB/s),
  multi-thread streams, buffer size, retries, low-level retries, max duration,
  check first, order by, retries sleep, TPS limit, connect timeout, I/O
  timeout.
  Source: `SyncConfigEditor.swift:232-246`.
  Engine: `flows.save`.
  Notes: hints show example values (8, 0, 4, 16M, 3, 10, 24h, size,ascending,
  0s, 0, 60s, 300s). Empty input means "unset" (nil), not zero.

- [ ] UC-84 Filtering options — include paths, exclude paths (one per line),
  min size, max size, max age, min age, max depth, filter-from-file, exclude if
  present, use regex, delete excluded.
  Source: `SyncConfigEditor.swift:248-260`.
  Engine: `flows.save`.
  Notes: include/exclude lists serialize as string arrays; empty lists are
  omitted.

- [ ] UC-85 Safety options — max delete %, immutable, max transfer, max delete
  size, suffix, suffix keep extension, backup path.
  Source: `SyncConfigEditor.swift:262-270`.
  Engine: `flows.save`.

- [ ] UC-86 Comparison options — size only, update mode (skip newer), ignore
  existing.
  Source: `SyncConfigEditor.swift:272-276`.
  Engine: `flows.save`.

- [ ] UC-87 Push-only delete timing — the "Sync options" section appears only
  for push (and pull) and offers Default/Before/During/After.
  Source: `SyncConfigEditor.swift:218,278-290`.
  Engine: `flows.save`.
  Notes: stored value empty means default.

- [ ] UC-88 Bisync options — appears only for bi/bi-resync: conflict resolution
  (none/path1/path2/newer/older/larger/smaller), resilient, max lock, check
  access, conflict loser, conflict suffix.
  Source: `SyncConfigEditor.swift:219,292-312`.
  Engine: `flows.save`.
  Notes: `conflictResolution` defaults to `none` in the picker; empty stored
  value renders as none.

- [ ] UC-89 Dual-key serialization — every option is written under both its
  snake_case and camelCase key, except fields whose snake and camel names are
  identical (written once); nil, empty strings, and empty arrays are omitted;
  `action` is always written.
  Source: `SyncConfigEditor.swift:141-193`.
  Engine: `flows.save`.
  Notes: the engine reads both spellings
  (`FlowEngine.profileFromSyncConfig`, `SyncConfigEditor.parse`), so writing one
  spelling is sufficient for correctness but breaks parity with existing DB
  data. Keep both.

- [ ] UC-90 Parse tolerance — loading accepts snake_case or camelCase,
  normalizes the action, and falls back to the operation action when absent.
  Source: `SyncConfigEditor.swift:58-137`; `Store.swift:611-637`.
  Engine: `flows.get`.
  Notes: `Store.listOperations` also normalizes the action on read.

- [ ] UC-91 Option summary chips — a compact list of non-default options
  (dry-run, ×parallel, bandwidth, includes/excludes, max-age, size filter,
  conflict, immutable, size-only) exists in code but is not rendered anywhere in
  the Swift app.
  Source: `SyncConfigEditor.swift:195-210`.
  Engine: —.
  Notes: LEGACY-ONLY UI (Vue flow cards rendered chips). The Rust inspector may
  render them; not required for functional parity.

- [ ] UC-92 All editors disabled while running — every control in the panel and
  the node/edge editors is disabled when the flow is active.
  Source: `SyncConfigEditor.swift:224-312`; `InspectorView.swift:115-241`.
  Engine: `runtime.snapshot`.

- [ ] UC-93 Staging semantics — node/edge/option edits update the local flow and
  set dirty; only Delete node/edge, connect, node drag, and viewport persist
  immediately.
  Source: `InspectorView.swift:244-289`; `FlowCanvasView.swift:434-486`.
  Engine: `flows.save`.
  Notes: this split is intentional (legacy canvas behavior) and must be
  preserved or deliberately simplified in the GPUI window.

---

## F. Runtime telemetry, edge cards, file progress

- [ ] UC-94 Runtime snapshot hydration — after unlock/connect the UI loads a
  snapshot containing every flow's status, per-op status/error, the latest sync
  progress, and the log.
  Source: `AppState.swift:149-163`; `RuntimeHub.swift:61-83`.
  Engine: `runtime.snapshot`.
  Notes: snapshot shape is `{revision, flows:[{id,status,lastError,ops:[{id,status,lastError}],sync,log:[{at,status,opId,error,label}]}]}`;
  `status` defaults to `"idle"` when unknown.

- [ ] UC-95 Snapshot revision ordering — the window must ignore snapshots older
  than the last applied revision.
  Source: legacy Vue `lib/runtimeSync.ts` (revision guard);
  `RuntimeHub.swift:22,82`.
  Engine: `runtime.snapshot`, `runtime:snapshot` events.
  Notes: the SwiftUI app currently replaces state unconditionally; the Rust
  window must implement the revision guard so a slow response cannot roll back
  live state.

- [ ] UC-96 Live event routing — `sync:*` events route to the edge via
  `profileID` in the `flowId:operationId` form (split on the first colon; both
  sides non-empty).
  Source: `RuntimeHub.swift:131-157,241-246`; `EventBus.swift:55-78`.
  Engine: `sync:progress` events.
  Notes: this key format is a hard AGENTS.md rule; never emit a bare flow id
  for per-op progress.

- [ ] UC-97 Sync lifecycle events — started/progress/completed/failed update the
  flow status, op status, last error, and the sync payload.
  Source: `RuntimeHub.swift:85-180`; `SyncEngine.swift` (publish sites).
  Engine: `sync:started`, `sync:progress`, `sync:completed`, `sync:failed`.
  Notes: failed events carry the progress struct including `transfers`; completed
  maps to a synthesized progress event (bytes, errors, state=completed).

- [ ] UC-98 Active-task overlay — if the event stream is missed, active sync
  tasks are overlaid into the snapshot (status, stats, file transfers).
  Source: `RuntimeHub.swift:75-77,182-215`; `SyncEngine.swift:235-242`.
  Engine: `runtime.snapshot`, `runtime.tasks`.
  Notes: this makes the snapshot self-healing after a window reload.

- [ ] UC-99 Edge card header and stats — action badge, source→target, close
  button, and a stats row: `done/total files`, transferred bytes @ speed,
  errors (red), and current stage.
  Source: `EdgeCardView.swift:35-69`.
  Engine: `runtime.snapshot`.
  Notes: bytes use B/K/M/G formatting with 1024 base.

- [ ] UC-100 File row ordering — transferring, checking, failed, pending,
  checked, completed; ties sorted by localized name.
  Source: `EdgeCardView.swift:25-33`.
  Engine: `runtime.snapshot`.
  Notes: each row shows an icon, file name (truncated in the middle), a
  percentage for active rows, and "failed" for failures with an error.

- [ ] UC-101 Active percentage — rows in transferring/checking show
  `Int(clamp(progress, 0, 100))%`.
  Source: `EdgeCardView.swift:117-121`.
  Engine: `runtime.snapshot`.
  Notes: AGENTS.md requires this for active rows.

- [ ] UC-102 Pending rows — pending files appear in the card but never as edge
  dots.
  Source: `EdgeCardView.swift:1-4,71-83`; `FlowCanvasView.swift:269-279`.
  Engine: `runtime.snapshot`.
  Notes: hard AGENTS.md rule. They also must not be hidden just because they
  have no bytes yet.

- [ ] UC-103 Edge card empty states — no activity shows the op status if known,
  otherwise "No activity yet".
  Source: `EdgeCardView.swift:76-80`.
  Engine: `runtime.snapshot`.
  Notes: GAP vs Vue/AGENTS: the Vue card distinguished "preparing file list"
  from "no errors/processed files" tabs. The Swift app has a single list; the
  GPUI window may keep one list but should show a preparing state while
  transfers are empty and the op is running.

- [ ] UC-104 Dot status mapping — visible statuses and colors as listed in
  UC-63; `checked` and `completed` both mean success; `checking` is live.
  Source: `FlowCanvasView.swift:269-307`.
  Engine: `sync:progress`.

- [ ] UC-105 Auto-open on active op — see UC-72; the card must open for the
  selected edge when its op becomes running/checking.
  Source: `FlowCanvasView.swift:242-251`.
  Engine: `runtime.snapshot`.

- [ ] UC-106 Card survives viewport changes — see UC-70.
  Source: `FlowCanvasView.swift:134-144`.
  Engine: —.

- [ ] UC-107 Run log caps — 40 entries per flow in the engine, 12 rendered in
  the inspector; consecutive duplicates are dropped.
  Source: `RuntimeHub.swift:7,228-238`; `InspectorView.swift:147`.
  Engine: `runtime.snapshot`.

- [ ] UC-108 Lock clears runtime — locking resets flows, remotes, profiles, and
  the runtime projection to revision 0.
  Source: `AppState.swift:109-120`; `RuntimeHub.swift:45-50`;
  `AppContainer.swift:123-132`.
  Engine: `auth.lock`, `runtime.snapshot`.

- [ ] UC-109 Flow deletion forgets runtime — deleting a flow removes its
  runtime record and bumps the revision.
  Source: `AppState.swift:226-230`; `RuntimeHub.swift:52-58`.
  Engine: `flows.delete`.

- [ ] UC-110 History rows — every sync/flow run records a history row (running
  first, then the final state/bytes/errors/duration).
  Source: `SyncEngine.swift` (history save sites); `Store.swift:400-459`;
  `SettingsView.swift:71-86`.
  Engine: `history.list`, `history.stats`.
  Notes: the Settings screen shows the last 20 entries with profile name, state
  color, and start time; "No runs yet" when empty.

- [ ] UC-111 Unknown-flow events — runtime events for a flow not in the list
  must still be retained by id so a later reload can show the terminal status.
  Source: `RuntimeHub.swift:100-157`.
  Engine: `flow:execution`, `sync:*`.
  Notes: the engine does this; the window must not filter unknown ids out of
  its runtime map.

---

## G. Remotes

- [ ] UC-112 Remotes sheet — a 460×420 sheet lists remotes with name, type,
  Test, and trash buttons, and an Add button in the header.
  Source: `RemotesView.swift:53-98`.
  Engine: `remotes.list`.
  Notes: the sheet is reachable from the toolbar and the rail footer.

- [ ] UC-113 Remote row — shows a drive icon, name, and type (or "unknown"
  when the type is empty).
  Source: `RemotesView.swift:69-84`.
  Engine: `remotes.list`.
  Notes: `Remote` is `{name,type,description}`; description is unused by the UI.

- [ ] UC-114 Test remote — running Test calls `rclone lsd <name>:` and toasts
  "Remote {name}: OK" or "Remote {name} failed: …".
  Source: `RemotesView.swift:79`; `AppState.swift:279-286`;
  `RcloneClient.swift:244-249`.
  Engine: `remotes.test`.

- [ ] UC-115 Delete remote — asks `Delete remote "{name}"?`, then removes it
  from `rclone.conf` and reloads the list.
  Source: `RemotesView.swift:80-96`; `AppState.swift:269-277`;
  `RcloneClient.swift:240-243`.
  Engine: `remotes.delete`.
  Notes: GAP: no check whether flows reference the remote, and the Swift app
  allows deletion while a flow runs (legacy Vue blocked remote add/delete while
  any run was active). Recommended: block while active with a clear error.

- [ ] UC-116 Open Add remote — a sheet with Name, Type, type-specific fields,
  Cancel, and "Test & add".
  Source: `RemotesView.swift:100-149`.
  Engine: `remotes.providers`.

- [ ] UC-117 Choose remote type — picker over 18 entries: local, drive,
  dropbox, onedrive, iclouddrive, yandex, googlephotos, mega, s3, b2, sftp,
  ftp, webdav, crypt, alias, pcloud, box, sharepoint.
  Source: `RemotesView.swift:49-51,117-119`.
  Engine: `remotes.providers`.
  Notes: GAP: `sharepoint` is not an rclone backend (verified:
  `rclone config create x sharepoint` → "couldn't find backend"); `iclouddrive`
  needs `service`/`apple_id`/`password`; `googlephotos` is the backend prefix
  for `google photos`. The provider list must come from
  `rclone config providers` and be filtered/aliased, not hard-coded.

- [ ] UC-118 Per-type credential fields — mega: username/password; sftp/ftp:
  host/username/password; webdav: URL/username/password; s3:
  provider/access key/secret/region/endpoint; b2: account id/application key;
  crypt: wrapped remote/password; alias: wrapped remote; all others: no fields.
  Source: `RemotesView.swift:13-47`.
  Engine: `remotes.add`.
  Notes: secure fields use password inputs. The provider schema's option list
  is authoritative; the UI matrix is a compatibility subset.

- [ ] UC-119 Required-field validation — "Test & add" stays disabled until the
  name is non-empty and every required field for the selected type is filled.
  Source: `RemotesView.swift:109-111,144`.
  Engine: —.
  Notes: the current Swift UI does not show the Vue inline message "Fill the
  required account fields before testing."; disabled state is the only signal.

- [ ] UC-120 OAuth hint — for non-local types with no credential fields, the
  sheet explains that the provider may require OAuth, will be verified after
  creation, and a browser window may open.
  Source: `RemotesView.swift:127-130`.
  Engine: `remotes.add`, `remotes.oauth.start`.
  Notes: this hint is currently misleading: `rclone config create
  --non-interactive` does not complete OAuth and returns a JSON question blob.

- [ ] UC-121 Add + verify — "Test & add" calls `createRemote` then `lsd`; on
  success the remote is created and "Remote {name} added" is toasted; on failure
  the remote is deleted.
  Source: `RemotesView.swift:134-143`; `AppState.swift:257-267`;
  `RcloneClient.swift:222-239`.
  Engine: `remotes.add`.
  Notes: GAP: the sheet dismisses immediately regardless of success, and the
  `busy` flag is never reset. The Rust window must keep the sheet open on
  failure and show the error inline.

- [ ] UC-122 OAuth remote creation — when `remotes.add` reports
  `oauth_required`, the window starts an engine-driven `rclone authorize`
  session, opens/embeds the returned URL, receives the token blob, then
  completes creation with `token=<blob>` and verifies with `lsd`.
  Source: rclone behavior documented in `rpc.md` §7;
  `RcloneClient.swift` (no OAuth support today).
  Engine: `remotes.oauth.start`, `remotes.oauth.complete`,
  `remotes.oauth.cancel`, `remotes:oauth` events.
  Notes: this capability does not exist in the Swift app or Vue UI; it is a new
  requirement for the GPUI window. OAuth client credentials for backends that
  ship rclone's defaults are not needed; custom client IDs are provider options.

- [ ] UC-123 Remote list refresh — the list reloads on sheet open, after
  add/delete, after unlock, and on `state:changed(remotes)`.
  Source: `AppState.swift:149-158,261,272`; `EventBus.swift:183-193`.
  Engine: `remotes.list`, `state:changed`.
  Notes: `state:changed` is published by AppState, not by rclone; the serve
  engine must publish it itself after add/delete so any attached window stays in
  sync.

- [ ] UC-124 No remote editing — there is no rename/edit/re-auth UI; changing a
  remote means deleting and re-adding it.
  Source: `RemotesView.swift`.
  Engine: `remotes.delete`, `remotes.add`.
  Notes: the legacy Vue had no edit either; `Remote.description` is unused.

---

## H. File browse and path pickers

- [ ] UC-125 Editable path field — every location path is a monospaced text
  field; typing updates the node (staged while editing the inspector).
  Source: `Helpers.swift:51-56`; `InspectorView.swift:181-183`.
  Engine: —.

- [ ] UC-126 Open browser — "Browse…" opens a popover starting at the current
  path (or `/`), showing `remote:path` or `local:path` in the header.
  Source: `Helpers.swift:57-67,108-110`.
  Engine: `fs.list`.

- [ ] UC-127 Navigate up — an Up button appears when not at `/` and moves to the
  parent directory.
  Source: `Helpers.swift:70-76`.
  Engine: `fs.list`.
  Notes: the implementation uses `NSString.deletingLastPathComponent`; it must
  not escape the remote root.

- [ ] UC-128 Enter a directory — clicking a directory appends its name to the
  path and reloads; files are shown but non-clickable.
  Source: `Helpers.swift:79-93`.
  Engine: `fs.list`.
  Notes: entries come from `rclone lsjson` (`FileEntry`: Name, Size, MimeType,
  IsDir, ModTime, Path, ID). Sort order is rclone's.

- [ ] UC-129 Select a path — "Select" writes the browsed path into the field and
  closes the popover; "Cancel" closes without change.
  Source: `Helpers.swift:95-102`.
  Engine: —.

- [ ] UC-130 Browse states — empty directories show an empty list; loading and
  failures are not explicitly rendered in the Swift picker.
  Source: `Helpers.swift:79-94`; `AppState.swift:288-290`.
  Engine: `fs.list`.
  Notes: GAP vs Vue, which showed "Loading…", "Empty directory", and a red
  error. Recommended: show all three, and surface `browse_failed`.
  `AppState.browse` currently swallows errors into `[]`.

- [ ] UC-131 Picker disabled while running — Browse and the path field are
  disabled when the flow is active.
  Source: `InspectorView.swift:181-183`; `Helpers.swift:56,63`.
  Engine: `runtime.snapshot`.

- [ ] UC-132 Local vs remote paths — an empty or `local` remote composes to a
  plain absolute filesystem path; otherwise `name:path` (root path becomes
  `name:`).
  Source: `Models.swift:219-231`; `Helpers.swift:67,109`.
  Engine: `fs.list`.
  Notes: `fs.list` accepts the composed string; it must require a `:` or a
  leading `/` and reject relative paths (`RcloneClient.swift:140-153`).

---

## I. Profiles (engine/CLI; no GUI)

- [ ] UC-133 List profiles — shown by the CLI with name, from, to, parallel,
  bandwidth, dry-run; the Settings screen does not show profiles.
  Source: `Sources/gn-drive/Commands.swift:288-305`.
  Engine: `profiles.list`.

- [ ] UC-134 Create/update profile — CLI `profile add` requires name/from/to,
  direction in {push, bi, bi-resync}, parallel (default 4), bandwidth (MB/s),
  optional `--dry-run`; upserts by name.
  Source: `Commands.swift:307-339`; `Store.swift:224-277`.
  Engine: `profiles.save`.
  Notes: an empty direction becomes push; invalid directions are rejected by the
  CLI and by the store (`StoreError.invalid`).

- [ ] UC-135 Get/delete profile — fetch by name (`not_found` when absent) and
  delete by name (also `not_found`).
  Source: `Store.swift:217-223,278-284`.
  Engine: `profiles.get`, `profiles.delete`.

- [ ] UC-136 Profile schedule rows — the legacy profile-level `schedules` table
  still loads and can be registered (cron jobs by schedule id), but no current
  UI or CLI creates them.
  Source: `Store.swift:338-381`; `SyncEngine.swift:243-294`.
  Engine: `schedules.list` (legacy).
  Notes: LEGACY-ONLY surface. Flows use `flow:<id>` cron ids instead.

- [ ] UC-137 No profile GUI — the SwiftUI app has no profiles screen; profiles
  exist for one-shot CLI sync only.
  Source: `AppState.swift:13,152` (loads them, but no view uses them).
  Engine: `profiles.*`.
  Notes: the Rust window does not need a profiles screen for parity with the
  Swift app. Offer it only if the one-shot workflow is exposed.

---

## J. Boards (legacy DAG engine; no GUI)

- [ ] UC-138 Board storage — boards (nodes/edges, `syncConfig` JSON string,
  label, positions) are persisted and loaded with a graph.
  Source: `Store.swift:482-574`; `Models.swift:150-177`.
  Engine: `boards.list`, `boards.get`, `boards.save`, `boards.delete`.

- [ ] UC-139 Execute board — runs edges in topological layers (4 transfers per
  edge), stopping at the first failure by default; cycle detection returns an
  error.
  Source: `BoardEngine.swift:77-129,214-240`.
  Engine: `boards.execute`.
  Notes: `stopOnError` defaults true. Boards emit `board:execution` with
  node/edge ids, and FlowEngine mirrors flow events into the same topic for
  legacy clients.

- [ ] UC-140 Stop board — cancels the active run; stopping an idle board returns
  `not_running`.
  Source: `BoardEngine.swift:130+`.
  Engine: `boards.stop`.

- [ ] UC-141 Board CLI — `gn-drive board <board-id>` with
  `--no-stop-on-error`, printing layers and per-edge results.
  Source: `Commands.swift:239-277`.
  Engine: `boards.*`.
  Notes: LEGACY/DEPRECATED path; flows are the primary model. The Swift CLI has
  no `--concurrency` flag (Go had a no-op one).

- [ ] UC-142 No board GUI — neither the Swift app nor the current Vue app has a
  board screen. The legacy route redirects to `/`.
  Source: `~/.agent-scratch/qoder/gn/gnd-legacy-vue.md` §11.

---

## K. Settings: app preferences and service

- [ ] UC-143 Master-password section — current/new password + Change password;
  Lock app; Remove password (see UC-10/11/12). When no password is configured,
  the section shows the remove-password help text instead.
  Source: `SettingsView.swift:19-40`.
  Engine: `auth.status`, `auth.changePassword`, `auth.lock`,
  `auth.removePassword`.

- [ ] UC-144 App appearance toggles — Notifications, Debug mode, Minimize to
  tray, Start at login. Each toggle saves immediately to `auth.json`
  (`app_settings`).
  Source: `SettingsView.swift:42-48,106-109`; `AuthService.swift:15-29,315-324`.
  Engine: `settings.get`, `settings.set`.
  Notes: these values are persisted but only Debug mode currently has a
  plausible engine consumer; the window is responsible for acting on the rest.

- [ ] UC-145 Service control — the Settings section shows launchd status
  ("Not installed" / "Installed" / "Installed, not running" / "Running (pid N)")
  and buttons Install, Start, Stop, Restart.
  Source: `SettingsView.swift:127-180`; `Launchd.swift`.
  Engine: `service.status`, `service.install`, `service.start`,
  `service.stop`, `service.restart`.
  Notes: the launchd label is `com.gndrive.app`; the job runs
  `gn-drive run --service` with `GN_DRIVE_MODE=service`. Each action toasts
  "Service {action} ok/failed: …" and refreshes status.

- [ ] UC-146 CLI binary discovery for the service — the job's ExecPath is a
  sibling `gn-drive` next to the app executable if present, otherwise the first
  `gn-drive` on PATH, otherwise the literal `gn-drive`.
  Source: `SettingsView.swift:182-194`.
  Engine: `service.install` (execPath parameter).
  Notes: `GNDrive.app/Contents/MacOS/gn-drive` is shipped by
  `scripts/bundle-app.sh`; the GPUI window must pass an explicit exec path
  rather than relying on its own binary.

- [ ] UC-147 Service uninstall — not exposed in the Settings UI but available
  from the CLI; removes the plist and stops the job.
  Source: `Launchd.swift:88-95`; `Commands.swift:89-180`.
  Engine: `service.uninstall`.

- [ ] UC-148 System-scope service — `gn-drive service <action> --system`
  installs into `/Library/LaunchDaemons` (requires sudo); the UI only manages
  the user scope.
  Source: `Commands.swift:89-180`; `Launchd.swift:4-18`.
  Engine: `service.*` with `scope: "system"`.
  Notes: return a clear permission error if not root.

- [ ] UC-149 About — Version (build constant), rclone version (or "not found"),
  and "Check for updates".
  Source: `SettingsView.swift:55-69`; `AppState.swift:20,61-63`.
  Engine: `hello` (versions), `update.check`.

- [ ] UC-150 History section — the last 20 runs with profile name, state color,
  and start time; "No runs yet" when empty.
  Source: `SettingsView.swift:71-86`.
  Engine: `history.list`.
  Notes: the current app loads 100 rows and renders the first 20.

- [ ] UC-151 Service health file — service mode writes `service.health` in the
  config dir every 5 s (pid, mode, started_at, last_heartbeat, web_port=0,
  last_sync_at, next_schedule_at, last_error, active_tasks); staleness is
  >60 s.
  Source: `HealthWriter.swift`; `Commands.swift:41-49`.
  Engine: `service.health` (read).
  Notes: `web_port` is a legacy field and stays 0 in the native app. The legacy
  Go doctor claimed to read this file but did not; the RPC `doctor` must
  actually include it.

- [ ] UC-152 App settings are separate from window preferences — app toggles
  live in `auth.json`, while theme/locale live in the window's defaults.
  Source: `AuthService.swift:15-29`; `WorkspaceView.swift:69-70`.
  Engine: `settings.get/set`, `hello`.
  Notes: do not merge these stores; the window owns theme/locale.

---

## L. Self-update

- [ ] UC-153 Check for updates from Settings — calls GitHub Releases for
  `gnasdev/gn-drive`, compares the current build version, and toasts "Already
  on latest (x)" / "Update available: y" / "Update check failed: …".
  Source: `SettingsView.swift:111-124`; `SelfUpdate.swift:62-67`.
  Engine: `update.check`.
  Notes: no apply button exists in the Swift app; only the CLI can apply.

- [ ] UC-154 CLI check — `gn-drive self-update --check` prints current/latest
  and whether an update is available; `--repo-owner`/`--repo` override the
  source.
  Source: `Commands.swift:430-470`; `SelfUpdate.swift:47-67`.
  Engine: `update.check`.

- [ ] UC-155 Apply update — CLI downloads the platform archive, verifies its
  SHA256 sidecar, extracts, atomically swaps the running binary, keeps
  `<binary>.bak`, and prints a restart hint; `--force` reinstalls the same
  version.
  Source: `SelfUpdate.swift:68-240`; `Commands.swift:441-469`.
  Engine: `update.apply`.
  Notes: `GITHUB_TOKEN` avoids rate limits (read from the environment only; do
  not expose it over RPC).

- [ ] UC-156 Asset-name gap — `SelfUpdate` expects
  `gn-drive-darwin-<arm64|amd64>.tar.gz` + `.sha256`, while
  `.github/workflows/release.yml` publishes
  `gn-drive-$VERSION-macos-arm64.tar.gz(.sha256)` and
  `GNDrive-$VERSION-macos.zip(.sha256)`. The current check/download path cannot
  find the release asset.
  Source: `SelfUpdate.swift:68-160`; `.github/workflows/release.yml`.
  Engine: `update.check`, `update.apply`.
  Notes: GAP to resolve before exposing update.apply in the GPUI window. Either
  rename release assets or teach SelfUpdate the macOS names and `.zip` layout.
  Document the chosen mapping; do not silently report "no release".

- [ ] UC-157 Version reporting — `gn-drive version` prints the build version and
  commit (`dev`/`unknown` in development; stamped during bundling).
  Source: `main.swift:17-21`; `BuildVersion.swift`; `Commands.swift:472-479`.
  Engine: `hello`.

---

## M. CLI surface

- [ ] UC-158 `gn-drive run` (foreground) — builds the container with portal mode
  off, takes the instance lock, starts the sync engine, prints guidance, and
  waits for SIGINT/SIGTERM.
  Source: `Commands.swift:8-87`.
  Engine: `serve --stdio` is the new programmatic equivalent; `run` stays for
  humans/service.

- [ ] UC-159 `gn-drive run --service` — service logging mode, health writer
  enabled, no Keychain store, same signal shutdown.
  Source: `Commands.swift:26-58`.
  Engine: `service.*`.

- [ ] UC-160 `run --password` / `GN_DRIVE_PASSWORD` — starts unlocked with the
  provided password; used by launchd and CI.
  Source: `Commands.swift:18-35`.
  Engine: `hello` (unlocked state), `auth.unlock`.
  Notes: never echo the password in logs or RPC payloads.

- [ ] UC-161 Instance lock — both `run` and (new) `serve` take an exclusive
  flock on `<configDir>/gn-drive.lock` with `<configDir>/gn-drive.pid`; a
  conflict reports the holder's pid and suggests stopping the service.
  Source: `InstanceLock.swift:6-54`; `Commands.swift:41-49`.
  Engine: lifecycle (`instance_busy`).
  Notes: the SwiftUI app currently does not take the lock (GAP). The RPC spec
  requires the app and `serve` to share it so two engines never write the same
  SQLite/rclone config.

- [ ] UC-162 `sync <action> --profile` — one-shot sync for a profile, printing
  a progress line and a final summary; `--dry-run` forces the dry-run action.
  Source: `Commands.swift:182-238`.
  Engine: `sync.profile` (legacy one-shot), or implement as profile + execute.

- [ ] UC-163 `board <id>` — see UC-141.
  Source: `Commands.swift:239-277`.
  Engine: `boards.execute`.

- [ ] UC-164 `profile list|add|delete` — see UC-133..135.
  Source: `Commands.swift:280-353`.
  Engine: `profiles.*`.

- [ ] UC-165 `remote list|add|test|delete` — CLI equivalents of UC-112..124;
  add takes repeatable `k=v` pairs and does not perform OAuth.
  Source: `Commands.swift:355-428`.
  Engine: `remotes.*`.
  Notes: the legacy CLI printed a hint that OAuth remotes must be added via the
  web UI; the native app now has the same gap until UC-122 exists.

- [ ] UC-166 `self-update` — see UC-153..156.
  Source: `Commands.swift:430-470`.
  Engine: `update.*`.

- [ ] UC-167 `doctor [--data]` — prints rclone path/version, config dir, DB
  presence, auth state, remotes/profiles/recent history when accessible,
  platform, optional directory listing, and always ends with "All checks
  passed.".
  Source: `Commands.swift:481-570`.
  Engine: `doctor`.
  Notes: GAP: the final line is printed even when a check failed; the RPC must
  return structured statuses and the CLI should stop claiming success on errors.

- [ ] UC-168 `completion bash|zsh|fish` — emits shell completions.
  Source: `Commands.swift:597-620`.
  Engine: —.
  Notes: `powershell` is legacy-only (Windows).

- [ ] UC-169 `serve --stdio` — new subcommand backing the GPUI window; speaks
  newline-delimited JSON, shares the engine/app wiring, and shuts down on stdin
  EOF.
  Source: to be implemented (see `rpc.md`).
  Engine: all `rpc.md` methods.
  Notes: must use `portalMode = true` + `KeychainStore(service: "gn-drive")` so
  it behaves exactly like `AppState`, and must take the instance lock before
  serving.

---

## N. Logs and diagnostics

- [ ] UC-170 Foreground logs — timestamped text lines to stderr
  (`<iso8601> LEVEL msg key=value …`).
  Source: `Logger.swift:38-56`.
  Engine: `log:line` (optional stream).

- [ ] UC-171 Service logs — JSON lines to stderr with `time`, `level`, `msg`,
  plus context fields; launchd redirects them to `gn-drive.out.log` /
  `gn-drive.err.log` in the config dir.
  Source: `Logger.swift:26-38`; `Launchd.swift`.
  Engine: `logs.read`, `log:line`.

- [ ] UC-172 Log reading — the GPUI diagnostics view should show the last N
  lines of the service logs without shelling out.
  Source: `Paths.swift` (logDir == configDir); `Launchd.swift`.
  Engine: `logs.read` (`source: "out"|"err"`, `lines`).
  Notes: new capability; the SwiftUI app has no log viewer.

- [ ] UC-173 Doctor in the window — render UC-167 checks as a list with
  OK/WARN/ERROR/INFO statuses and a "data directory" disclosure.
  Source: `Commands.swift:481-570`.
  Engine: `doctor`.

- [ ] UC-174 Health status — show service health (running, pid, uptime,
  last/next sync, active tasks, last error) when the service is installed.
  Source: `HealthWriter.swift`; `ServiceStatus`.
  Engine: `service.status`, `service.health`.
  Notes: `next_schedule_at`/`active_tasks` are written but not currently updated
  by every path; the RPC should read the file and mark staleness >60 s.

---

## O. Legacy-only features (do NOT require reimplementation)

These exist in the Go HTTP server and/or the Vue SPA but were intentionally
dropped in the native rewrite. They are listed so the Rust window does not
mistake them for missing parity.

- [ ] UC-175 HTTP API — `/api/v1/*` REST endpoints, session cookie
  (`gn-drive-session`, 24 h), CORS, request IDs, gzip, and the auto-session
  `/status` resume behavior.
  Source: `~/.agent-scratch/qoder/gn/gnd-legacy-api.md` §1.
  Engine: — (replaced by stdio RPC).
  Notes: LEGACY-ONLY by design. Do not bind a loopback port.

- [ ] UC-176 SSE event stream — `GET /api/v1/events`, 25 s heartbeats, initial
  runtime snapshot.
  Source: legacy notes §2a.
  Engine: — (replaced by pushed events).

- [ ] UC-177 WebSocket state channel — `GET /api/v1/state`, `state.snapshot` on
  connect and after `state:changed`, `runtime.event` forwarding,
  `state.resync`, 500 ms→5 s reconnect backoff.
  Source: legacy notes §2b and `gnd-legacy-vue.md` §8.1.
  Engine: — (replaced by stdio + `state.snapshot` / `runtime.snapshot`).
  Notes: keep the reconnect/backoff UX in the Rust window for a dead child
  process.

- [ ] UC-178 `run --port` / `--no-browser` — loopback portal port allocation
  (default 53241) and browser launch.
  Source: legacy notes §3.
  Engine: —.
  Notes: LEGACY-ONLY; the native app has no HTTP UI.

- [ ] UC-179 `completion powershell` — Windows-only shell completion.
  Source: legacy notes §3.
  Engine: —.

- [ ] UC-180 systemd / Windows SCM service managers — Linux `.service` units
  and Windows service control.
  Source: legacy notes §3–4; `Launchd.swift`.
  Engine: —.
  Notes: macOS-only product; launchd is the only supported manager.

- [ ] UC-181 One-shot operations API — `POST /api/v1/operations` for
  copy/move/check/mkdir/purge/delete with no progress.
  Source: legacy notes §1.
  Engine: —.
  Notes: no UI or CLI used it; `RcloneClient` keeps the primitives
  (`mkdir`, `purge`, `deleteFile`) for future use.

- [ ] UC-182 Arbitrary settings keys — `GET/POST /api/v1/settings` accepted any
  string key and only returned `theme`, `notifications_enabled`, `debug_mode`.
  Source: legacy notes §1.
  Engine: `settings.*` (typed AppSettings only).
  Notes: do not reproduce the unvalidated key/value store.

- [ ] UC-183 Flow-card edit mode — per-flow Edit/Done cards with inline rename,
  expand/collapse, add/remove operation rows, dirty tracking, and a run status
  panel.
  Source: `gnd-legacy-vue.md` §12 (`WorkspaceFlowsSection.vue`, dead code).
  Engine: `flows.*`.
  Notes: LEGACY-ONLY; superseded by canvas + inspector.

- [ ] UC-184 FlowRunStatusPanel — flow-level progress with Syncing/Complete/
  Failed/Pending tabs, per-file rows, stage checklist, stats grid, ETA, and
  collapse bar.
  Source: `gnd-legacy-vue.md` §8.6 (dead component).
  Engine: `runtime.snapshot`.
  Notes: LEGACY-ONLY. The Swift app replaces it with the per-edge card and the
  inspector log. If the GPUI window wants richer tabs, implement them on the
  same snapshot data.

- [ ] UC-185 Edge overlay trigger buttons — separate Errors / Processed / Files
  round buttons with counts, opening three panel variants and a 5-step resolver
  checklist.
  Source: `gnd-legacy-vue.md` §7.3.
  Engine: `runtime.snapshot`.
  Notes: LEGACY-ONLY; Swift has one card (UC-99..103). Do not re-create the
  three-button overlay unless product asks for it.

- [ ] UC-186 Particle animation constants — per-file particle easing, queue
  band, success fade-out, direction alternation, and 24-particle cap.
  Source: `gnd-legacy-vue.md` §7.3.
  Engine: `runtime.snapshot`.
  Notes: Swift ported the dot positions but simplified direction/queueing
  (UC-64). Exact animation is not a functional requirement.

- [ ] UC-187 Wheel-pan / zoom-button canvas controls — Vue Flow's controls,
  wheel pan, pinch zoom, and lock button.
  Source: `gnd-legacy-vue.md` §7.2.
  Engine: —.
  Notes: LEGACY-ONLY control scheme; the Rust canvas should offer drag-pan plus
  wheel/pinch zoom (UC-52/53).

- [ ] UC-188 Inspector width persistence — the Vue inspector stored a resizable
  width; the Swift inspector is a fixed split pane.
  Source: `gnd-legacy-vue.md` §13.
  Engine: —.
  Notes: optional UI nicety; not required.

- [ ] UC-189 Remote description and test-icon states — Vue displayed a
  description field and an icon-only Test result (green check/red X) without
  the error text.
  Source: `gnd-legacy-vue.md` §9.
  Engine: `remotes.list`, `remotes.test`.
  Notes: LEGACY-ONLY. Swift toasts the result including the error; preferred.

- [ ] UC-190 Legacy route redirects — `/dashboard`, `/profiles`, `/remotes`,
  `/operations`, `/boards`, `/flows` all redirect to `/`.
  Source: `gnd-legacy-vue.md` §0.
  Engine: —.

- [ ] UC-191 Profile direction `pull` — legacy UI/TS types recognized `pull`;
  flow operations normalize it to `push` and profiles reject it.
  Source: `gnd-legacy-vue.md` §11; `Models.swift:4-21`.
  Engine: `flows.save`, `profiles.save`.
  Notes: do not expose `pull` as a flow action; for profiles, return
  `invalid_direction` (legacy API behavior) rather than silently normalizing.

- [ ] UC-192 Go doctor claims service.health — the docs/legacy API notes said
  doctor read `service.health`, but the Go code did not.
  Source: legacy notes §3–4.
  Engine: `doctor`, `service.health`.
  Notes: the RPC spec deliberately makes this real (UC-174).

- [ ] UC-193 Self-update HTTP route in docs — `POST /api/v1/self-update` was
  documented but never implemented.
  Source: `docs/features/self-update.md`; legacy notes §4.
  Engine: `update.*`.
  Notes: update is engine/CLI only; do not add an HTTP route.

- [ ] UC-194 `profile edit` usage string — Go's usage mentioned an edit
  subcommand that did not exist.
  Source: legacy notes §3.
  Engine: `profiles.save` (upsert covers edit).
  Notes: no dedicated edit command needed.

- [ ] UC-195 Board `--concurrency` — Go accepted a no-op concurrency flag;
  Swift dropped it.
  Source: legacy notes §3; `Commands.swift:239-256`.
  Engine: `boards.execute`.

- [ ] UC-196 Password length unit — Go counted bytes, Swift counts Characters.
  Source: legacy notes §5.
  Engine: `auth.*`.
  Notes: Swift behavior is the compatibility target for existing data; document
  that a 4-emoji password is accepted.

---

## P. Cross-cutting UX states the window must implement

- [ ] UC-197 Loading state — long operations (unlock, add remote, browse,
  update check) must show a busy state and prevent duplicate submits.
  Source: `UnlockView`, `RemotesView.swift:106,134-144`,
  `Helpers.swift` (no loading text), `SettingsView.swift:114-122`.
  Engine: all async methods.
  Notes: `busy` exists in `AddRemoteView` but is never cleared; fix in the Rust
  implementation.

- [ ] UC-198 Error text — every RPC error must be shown as a user-facing
  message; raw Swift `String(describing:)` output is not acceptable.
  Source: `AppState.swift:313-315`.
  Engine: all methods (`error.code`/`error.message`).
  Notes: map codes to localized strings; keep `humanizeError` for rclone stderr.

- [ ] UC-199 Confirmations — destructive actions that require confirmation:
  lock app (toolbar), delete flow, delete remote, remove password. Deleting a
  node/edge does not confirm.
  Source: `WorkspaceView.swift:61-66,137-142`; `RemotesView.swift:89-96`;
  `SettingsView.swift:95-103`; `InspectorView.swift:185,240`.
  Engine: —.
  Notes: use the localized title/message/confirm/cancel keys from `L10n.swift`.

- [ ] UC-200 Keyboard — Escape closes popovers/sheets; Delete removes the
  selected canvas item; Enter submits unlock/setup. No other global shortcuts.
  Source: `FlowCanvasView.swift:162-164`; `UnlockView.swift:28,32`;
  `Helpers.swift:64-104`.
  Engine: —.
  Notes: the legacy Vue bound only Delete as well; do not invent Cmd+S/run
  shortcuts without product approval.

- [ ] UC-201 Accessibility — every icon-only toolbar/row button needs a label or
  tooltip; the ASCII-canvas edge labels and color-coded statuses need textual
  equivalents in the inspector.
  Source: `WorkspaceView.swift:30-56`; `RemotesView.swift:79-82`;
  `EdgeCardView.swift`.
  Engine: —.

- [ ] UC-202 Localization coverage — all visible strings should go through the
  en/vi table; the Swift app still has hard-coded English in several places
  (e.g. "Untitled flow", "Flow has no operations", "Check for updates",
  "Service runs…").
  Source: `InspectorView.swift:82,86,91`; `SettingsView.swift:44-47,68,141`;
  `L10n.swift`.
  Engine: —.
  Notes: port the table and add the missing keys; do not ship a mixed-language
  UI.

- [ ] UC-203 Window lifecycle — the window must not leave the engine running
  after it exits; stdin EOF is the shutdown signal, and a graceful shutdown
  re-encrypts if locked-by-policy.
  Source: `AppContainer.swift:134-140`; see `rpc.md` §3.
  Engine: `app.quit`.

- [ ] UC-204 Single engine instance — at most one engine (SwiftUI app, `serve`,
  or `run --service`) may own a config dir at a time.
  Source: `InstanceLock.swift`; see `rpc.md` §3.
  Engine: lifecycle (`instance_busy`).
  Notes: GAP in the current app (it does not lock). The RPC requires it.

---

## Known parity gaps (summary)

1. `sharepoint` is offered but is not an rclone backend; `iclouddrive` needs
   credentials that the UI does not collect (`service`, `apple_id`, `password`).
2. OAuth remote creation is not implemented; `--non-interactive config create`
   returns a question blob instead of creating the remote.
3. Add-remote sheet dismisses on failure and never clears its busy flag.
4. Self-update asset names do not match the release workflow.
5. `AppState.changePassword` neither locks nor clears sessions (legacy did).
6. The SwiftUI app does not take the instance lock.
7. Browse errors are swallowed and rendered as an empty directory.
8. Remote add/delete is not blocked while a flow runs (legacy Vue blocked it).
9. Doctor always ends with "All checks passed." even when checks fail.
10. No discard-staged-edits action despite the "Discard changes?" hint.
11. Legacy-only UI (flow cards, run-status panel, three edge panels, particle
    animation, profiles/boards screens) is intentionally out of scope unless
    product asks for it.
