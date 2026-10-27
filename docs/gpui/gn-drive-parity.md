# GN Drive GPUI parity table

Source of truth: `~/Github/gnasdev/gn-drive/docs/gpui/use-cases.md` (204 use cases). Audited against `apps/gn-drive-gpui/src` after the UX round (tooltips, plain-language schedules, one side panel, file card, in-process engine). Statuses come from reading the code. The window was exercised in mock mode for the screenshots in `docs/gn-drive-ux-review.md`. No real rclone and no OAuth.

Status key: **done** = the window behaviour is in place (wording differences noted); **partial** = present, with the gap stated; **missing** = not implemented; **n/a** = legacy-only, CLI-only, engine-side, or not a window requirement.

The default engine is in-process `gn-drive-core` (feature `core`, on by default). `GN_DRIVE_MOCK=1` selects the mock. `GN_DRIVE_BACKEND=stdio` is the child-process fallback. Auth never starts by itself: Sign in and Reconnect run only from a click, one authorize is in flight at a time, and a failure does not start another.

## Summary

| Status | Count |
|---|---|
| done | 155 |
| partial | 3 |
| missing | 0 |
| n/a | 46 |
| **Total** | **204** |

| Section | done | partial | missing | n/a |
|---|---|---|---|---|
| A. App shell, unlock, master password | 17 | 1 | 0 | 0 |
| B. Workspace shell and flow rail | 10 | 0 | 0 | 0 |
| C. Flow editing and lifecycle | 20 | 0 | 0 | 0 |
| D. Flow canvas and graph | 26 | 0 | 0 | 0 |
| E. Inspector: node, edge, and sync-config editor | 18 | 0 | 0 | 1 |
| F. Runtime telemetry, edge cards, file progress | 18 | 0 | 0 | 0 |
| G. Remotes | 13 | 0 | 0 | 0 |
| H. File browse and path pickers | 8 | 0 | 0 | 0 |
| I. Profiles (engine/CLI; no GUI) | 0 | 0 | 0 | 5 |
| J. Boards (legacy DAG engine; no GUI) | 0 | 0 | 0 | 5 |
| K. Settings: app preferences and service | 9 | 0 | 0 | 1 |
| L. Self-update | 3 | 0 | 0 | 2 |
| M. CLI surface | 2 | 0 | 0 | 10 |
| N. Logs and diagnostics | 3 | 1 | 0 | 1 |
| O. Legacy-only features (do NOT require reimplementation) | 1 | 0 | 0 | 21 |
| P. Cross-cutting UX states the window must implement | 7 | 1 | 0 | 0 |

## A. App shell, unlock, master password

| UC | Status | Evidence / reason |
|---|---|---|
| UC-1 | done | `unlock.rs::screen` setup card (password + repeat, `auth.setup` via `submit_auth`). Button reads "Set password" |
| UC-2 | done | `submit_auth`: at least 4 characters (`chars().count()`), match check; "Use at least 4 characters." / "The passwords do not match." Errors show on submit |
| UC-3 | done | `auth:unlocked` and `Pending::Auth` call `reload()` (`state.snapshot`). Status bar says "Unlocked" |
| UC-4 | done | Unlock mode, one masked field, Enter → `auth.unlock` |
| UC-5 | done | `auth:unlocked` → `reload()`; snapshot carries runtime; status "Unlocked" |
| UC-6 | done | `handle_error(Pending::Auth)` sets `auth_error`; the form stays open |
| UC-7 | done | `locked_out` + `retryAfterSecs` → `lockout_until`; chip "Try again in {n}s"; `submit_auth` ignores presses during lockout |
| UC-8 | done | Status-bar lock, ⌘⇧L, and the app menu call `ask_lock` ("Lock GN Drive?") before `auth.lock` |
| UC-9 | done | Settings "Lock now" uses the same confirm, then `lock_now`. `auth:locked` closes the side panel |
| UC-10 | done | Change-password dialog → `auth.changePassword` (new password at least 4 characters). Wrong old password stays inline |
| UC-11 | done | After a successful change, `lock_now` sends `auth.lock` so the new password is required on the next unlock |
| UC-12 | done | Remove password confirms, then asks for the current password, then `auth.removePassword` |
| UC-13 | done | Keychain storage is engine-side. The window only reflects `snapshot.locked` |
| UC-14 | done | Quit sends `app.quit`, then `engine.shutdown()`, then `app.quit()`. The stdio backend writes `app.quit` and closes stdin; the in-process backend stops its workers. The UI thread does not wait on rclone |
| UC-15 | partial | View menu toggles English / Tiếng Việt and `prefs::t` translates a short chrome table (Run, Save, Connected, This Mac, and similar). The Swift 298-key catalog is not ported, so most sentences stay English |
| UC-16 | done | View menu Light / Dark calls `prefs::set_light` → `ns_ui::set_light`. The GnDrive palette swaps and the choice is stored in `~/Library/Application Support/gn-drive-gpui/window.json` |
| UC-17 | done | Every GN Drive toast, success or failure, leaves after 4 seconds (`TOAST_STAY` through `ns_ui::notify_for`). The other GPUI apps keep the shared 5 s / 12 s default |
| UC-18 | done | `render`: fatal screen, then snapshot error, then unlock when locked, otherwise the workspace. Remotes and Settings are a side panel that replaces the inspector |

## B. Workspace shell and flow rail

| UC | Status | Evidence / reason |
|---|---|---|
| UC-19 | done | `chrome_full_height` with flow rail, canvas, and a 340px inspector. Remotes and Settings replace the inspector so the canvas is not squeezed. Widths are fixed, not a drag-to-resize split |
| UC-20 | done | Toolbar shows a green dot and "Connected" while `LinkStatus::Ready`, otherwise an amber dot and "Reconnecting". Both labels go through `prefs::t` |
| UC-21 | done | View menu has language and theme. The right rail opens Cloud accounts and Settings. The status bar locks. Each control has a tooltip or a menu label |
| UC-22 | done | Rail rows: status dot, `flow.title()`, operation count, and `cron_label` ("Every day at 03:00" or "Manual"). Sorted by `sort_order` |
| UC-23 | done | New flow (⌘N) asks for a name, then `flows.create`, selects it, and notices "Flow created" |
| UC-24 | done | Right-click on a rail row opens Run / Stop / Delete…. Delete confirms and names the flow. Run and Stop use the same paths as the toolbar |
| UC-25 | done | `delete_selected_flow` asks, naming the flow, then `flows.delete`. A running flow is refused and the engine `flow_busy` error is shown |
| UC-26 | done | Empty canvas "Build your first flow" and rail "No flows yet", with a next step |
| UC-27 | done | The sidebar's named "Cloud accounts" row shows the account count as a visible badge (wording: Cloud accounts, not Remotes). The rail tooltip also says "Cloud accounts (N)" |
| UC-28 | done | ⌘, toggles Settings. Opening it closes the inspector. Tabs reload when shown |

## C. Flow editing and lifecycle

| UC | Status | Evidence / reason |
|---|---|---|
| UC-29 | done | `pick_flow` adopts the snapshot copy, rebuilds the graph, and resets the selection. A single edge is then auto-selected |
| UC-30 | done | The name field writes `draft.name` and marks the flow dirty. Save persists it |
| UC-31 | done | Dirty shows an "Unsaved" chip and a dot on the title. Switching flows asks "Discard unsaved changes?" and names both flows. Pan and zoom do not set `dirty` |
| UC-32 | done | `save_flow` → `flows.save`, clears dirty, surfaces server warnings. Status becomes "Saved" |
| UC-33 | done | Save while running notices "Stop the flow before saving changes." |
| UC-34 | done | Schedule switch plus a preset menu. The raw cron field appears only for Custom, with `cron_error` when the five fields are wrong |
| UC-35 | done | Presets: Manual, Hourly, Every 6 hours, Daily at midnight, Daily at 03:00, Weekdays at 09:00, Weekly on Sunday, Custom. Custom seeds `15 4 * * *` so the Advanced field stays open |
| UC-36 | done | Run saves first when the draft is dirty, refuses an empty flow, then `flows.execute` |
| UC-37 | done | The rail menu, toolbar, inspector, and ⌘R share `run_flow` / `stop_flow` |
| UC-38 | done | "This flow has no connections to run." plus the engine `empty_flow` toast |
| UC-39 | done | Toolbar swaps Run for Stop while running. Stop is also ⌘. and the rail menu |
| UC-40 | done | `status_label` / `status_color`: Running, Stopping, Completed, Failed, Stopped, Idle |
| UC-41 | done | Inspector "Recent activity" renders the last 12 runtime log lines |
| UC-42 | done | `toast_error` and sync errors go through `humanize_error` (strips `rclone:` / stderr, drops cancellation noise, caps at 160 characters) |
| UC-43 | done | A cancelled run is labelled "Stopped", `last_error` is cleared, and no error toast is shown |
| UC-44 | done | While running, text fields and switches are disabled, selects use `select_disabled`, and the Push / Two-way / Re-sync strip is covered so it cannot be clicked. Handlers also return immediately |
| UC-45 | done | Flows sort by `sort_order`. There is no drag-to-reorder UI |
| UC-46 | done | `is_collapsed` round-trips on the flow and is ignored by the canvas, which the spec allows |
| UC-47 | done | Discard (when dirty) reloads the saved flow via `discard_draft` and notices "Changes discarded" |
| UC-48 | done | `flows.create` supplies the name from the dialog. The mock fills sort order and a default name |

## D. Flow canvas and graph

| UC | Status | Evidence / reason |
|---|---|---|
| UC-49 | done | `graph::to_graph` rebuilds nodes and edges from operations when the flow or runtime changes |
| UC-50 | done | `default_layout`: sources and targets in columns, rows 140 apart, deduped by location |
| UC-51 | done | `stable_node_id` is FNV-1a `n_<hash>` of `remote\0path` |
| UC-52 | done | Drag the empty canvas to pan. The scroll wheel also pans |
| UC-53 | done | Pinch, ⌘-scroll, and the ± buttons zoom, clamped 0.4–2.0. The buttons are extra; the spec only required pinch |
| UC-54 | done | Fit (⌘0 and the fit button) centers content, max 1.2, clamped 0.4–2.0, and writes the viewport |
| UC-55 | done | Pan, zoom, pinch, and fit call `note_viewport` (`viewport_dirty` only). A viewport-only save does not set the Unsaved chip. Node drag still sets `dirty` |
| UC-56 | done | Dragging a node moves it and schedules a save. Drag is ignored while running |
| UC-57 | done | Drag from the ring on a node's right edge onto another node: a rubber-band curve follows the pointer, and the drop creates a `push` edge and selects it. Dropping on empty canvas cancels; dropping on the same node warns. Click the ring, then click the target, still works |
| UC-58 | done | A duplicate ordered pair with the same action says "This connection already exists" |
| UC-59 | done | A self-loop says "Cannot connect a node to itself" |
| UC-60 | done | Node click selects the node. A click within 10px of the cubic (16 samples) selects the edge and opens its card. An empty click selects the flow and closes the card |
| UC-61 | done | Edges are cubics. Running is accent, completed is green, failed is red, cancelled is amber, idle is gray. Two-way edges get arrowheads on both ends. Strokes are 1.25px and 2px when selected or running (the spec's 1.6 / 2.5) |
| UC-62 | done | The edge chip tooltip is the plain-language action. While running, the chip shows bytes, speed, and transferred/total files |
| UC-63 | done | `file_dots` paints 7px dots for transferring, failed, checking, checked, and completed. Pending is excluded |
| UC-64 | done | Live dots sit from 18% to 70% by progress, failed at 50%, success at 98%. Two-way actions use `1 - t` |
| UC-65 | done | At most 24 dots, ordered transferring, failed, checking, checked, completed, then by name |
| UC-66 | done | The connect ring is hidden while the flow runs, and `start_connect` refuses |
| UC-67 | done | Delete removes the selected node (and its edges) or the selected edge immediately, with no confirm, and is ignored while running |
| UC-68 | done | Add node creates a local folder at the viewport centre and selects it. The path is `~/New folder`, not `/new-<epoch>` |
| UC-69 | done | `follow_edge` selects the only edge and opens its card |
| UC-70 | done | The file card is placed at the edge midpoint in canvas coordinates, so pan and zoom carry it. It clamps inside the canvas and flips above the edge when the bottom is close |
| UC-71 | done | The card closes from its hide button, an empty-canvas click, another edge, or a flow switch. It is not closed by pan or zoom |
| UC-72 | done | When the selection is the flow and an edge is running or checking, the card opens once. Dismissing it sets `auto_opened` so a later tick does not reopen it |
| UC-73 | done | Node cards stay 170×56. The title is the remote's name or "This Mac". The path is middle-ellipsized and the tooltip is the full path |
| UC-74 | done | No flow: "Build your first flow". A flow with no nodes explains the next step |

## E. Inspector: node, edge, and sync-config editor

| UC | Status | Evidence / reason |
|---|---|---|
| UC-75 | done | No flow shows the empty workspace, with a primary "New flow" action |
| UC-76 | done | Node inspector: label, path, remote menu (This Mac + each account), Browse…, connections, Remove folder |
| UC-77 | done | Changing the remote rewrites incident operations through `set_node_path` / `rebind_graph` |
| UC-78 | done | Path edits stage into the draft. Browse opens the folder dialog |
| UC-79 | done | Remove folder deletes the node and its connections immediately, with no confirm, matching UC-67 |
| UC-80 | done | Edge inspector: action, what the action does, From/To with Browse, live progress, the error, option groups, Remove connection |
| UC-81 | done | Push / Two-way / Re-sync updates the operation and `sync_config.action` |
| UC-82 | done | `dry_run` is a switch |
| UC-83 | done | Performance group: parallel, bandwidth, streams, buffer, retries, timeouts, and the rest of `FIELDS` |
| UC-84 | done | Filtering group, including path lists |
| UC-85 | done | Safety group |
| UC-86 | done | Comparison group |
| UC-87 | done | The Sync group (delete timing) is shown only for push |
| UC-88 | done | The Two-way group is shown only for `bi` and `bi-resync` |
| UC-89 | done | `SyncConfig::to_json` writes both spellings, drops empty strings, and always writes `action` |
| UC-90 | done | `SyncConfig::parse` accepts either spelling and normalizes the action |
| UC-91 | n/a | The spec marks the chip summary legacy-only. `SyncConfig::chips` exists only under `#[cfg(test)]` |
| UC-92 | done | Same lock as UC-44: fields, switches, selects, and the action strip cannot be changed while the flow runs |
| UC-93 | done | Text edits stay in the draft until Save or Run. Connect, delete, and drag schedule `flows.save`. Viewport changes save without the Unsaved chip |

## F. Runtime telemetry, edge cards, file progress

| UC | Status | Evidence / reason |
|---|---|---|
| UC-94 | done | `state.snapshot` via `reload` carries runtime. Unknown status reads as Idle. There is no separate `runtime.snapshot` call |
| UC-95 | done | `apply_snapshot` and `runtime:snapshot` ignore an older revision |
| UC-96 | done | `apply_sync` splits `profileID` on the first colon |
| UC-97 | done | `sync:started`, `progress`, `completed`, and `failed` update flow status, op status, last error, and the sync payload |
| UC-98 | done | After a reconnect or `state:changed`, the window reloads the snapshot. The overlay itself is engine-side |
| UC-99 | done | The file card shows the action, source → target, a hide button, files done/total, bytes, speed, the stage, and errors in red |
| UC-100 | done | Card and inspector rows use `ranked_transfers`: transferring, checking, failed, pending, checked, completed, then by name. Pending is included |
| UC-101 | done | Transferring and checking rows show `percent_text`. Failed rows say "failed" |
| UC-102 | done | Pending rows say "Waiting" on the card and are absent from `file_dots` |
| UC-103 | done | No sync payload: "No activity yet. Press Run to sync these folders." Empty transfer list: "Preparing…" |
| UC-104 | done | Dot colors follow UC-63: accent, red, green, teal for checking |
| UC-105 | done | Same auto-open as UC-72, including when a sync event arrives while the flow is selected |
| UC-106 | done | The card lives in canvas coordinates and is not dismissed by pan or zoom |
| UC-107 | done | The inspector shows the newest 12 log lines and skips a line that repeats the one just above it. The engine's 40-entry cap stays in the engine |
| UC-108 | done | `auth:locked` and `apply_snapshot` on a locked snapshot set `runtime` back to `Runtime::default()` and clear the draft, graph, and selection |
| UC-109 | done | Deleting a flow clears the selection and reloads. The snapshot no longer has that flow's runtime |
| UC-110 | done | History calls `history.list` with `limit: 20`, shows state, bytes, duration, and errors, and "No history yet" when empty |
| UC-111 | done | `runtime_flow_mut` creates a record for an unknown flow id |

## G. Remotes

| UC | Status | Evidence / reason |
|---|---|---|
| UC-112 | done | Cloud accounts is a side column (list, Test connection, Remove…, Add cloud account), not a 460×420 sheet. It replaces the inspector |
| UC-113 | done | Each row shows the name, the type, and a status chip: Connected, Needs reconnect, or Not checked |
| UC-114 | done | Test calls `remotes.test` and reports whether the account answered. A failure does not start OAuth |
| UC-115 | done | Remove asks and names the account. `any_running` blocks the delete while any flow is running |
| UC-116 | done | Add-cloud-account dialog: name, type, fields, Cancel, and Add or Continue. Providers come from `remotes.providers` |
| UC-117 | done | Hidden providers are filtered by the engine list. `sharepoint` is not hard-coded |
| UC-118 | done | Password and sensitive options are masked. Show/Hide reveals one field. Values are not logged |
| UC-119 | done | Submit checks the name, the type, and every required field, and writes the miss into the dialog |
| UC-120 | done | OAuth types say you will sign in in the browser after Continue. Nothing opens a browser until that click, and the mock never does |
| UC-121 | done | `add_busy` blocks a second submit, the dialog stays open on failure, and the error is inline |
| UC-122 | done | `remotes.oauth.start` opens the waiting dialog (URL, copy, open). `authorized` completes. Close sends `remotes.oauth.cancel`. Reconnect is a separate click and only appears after a failed test of an OAuth account |
| UC-123 | done | Add, delete, unlock, and `state:changed` reload the list. Providers load when the panel opens |
| UC-124 | done | There is no general edit form. OAuth accounts that failed a test offer Reconnect |

## H. File browse and path pickers

| UC | Status | Evidence / reason |
|---|---|---|
| UC-125 | done | Node and edge paths are text fields staged by `collect_editors` |
| UC-126 | done | Browse starts at the current location or home, shows `remote:path`, and lists with `fs.list` |
| UC-127 | done | Up calls `browse_parent` |
| UC-128 | done | A directory row opens that folder. Files are shown with their size and are not selectable |
| UC-129 | done | "Use this folder" applies the path. Cancel closes |
| UC-130 | done | Loading skeleton, "This folder is empty.", and a red `browse_error` |
| UC-131 | done | While a flow is running, Browse… is drawn disabled. The tooltip says "Stop the flow before choosing a different folder." `open_browse` still refuses if a click gets through |
| UC-132 | done | `split_location` / `location_text`: empty or `local` is this Mac; otherwise `name:path` |

## I. Profiles (engine/CLI; no GUI)

| UC | Status | Evidence / reason |
|---|---|---|
| UC-133 | n/a | CLI-only. The spec says Settings has no profiles screen |
| UC-134 | n/a | CLI `profile add` |
| UC-135 | n/a | Engine and CLI only |
| UC-136 | n/a | Legacy profile schedules |
| UC-137 | n/a | No profile GUI is required |

## J. Boards (legacy DAG engine; no GUI)

| UC | Status | Evidence / reason |
|---|---|---|
| UC-138 | n/a | Legacy boards, no GUI |
| UC-139 | n/a | Legacy DAG engine, CLI-only |
| UC-140 | n/a | Legacy, no GUI |
| UC-141 | n/a | Legacy CLI |
| UC-142 | n/a | The spec says no board GUI exists |

## K. Settings: app preferences and service

| UC | Status | Evidence / reason |
|---|---|---|
| UC-143 | done | Security: Lock now, Change password, Remove password, or "No password is set" |
| UC-144 | done | Notifications gate success toasts (errors still show). Start at login sends `service.install` with `execPath`; turning it off sends `service.uninstall`. With Minimize to tray on, the close button hides the app (`on_window_should_close` + `hide`) so syncs keep running; the Dock icon brings it back and Quit still exits. GPUI has no status-bar item, so there is no tray icon |
| UC-145 | done | Service chips (Installed, Running, Stopped, pid) and Install / Start / Stop / Restart / Uninstall |
| UC-146 | done | `service.install` sends `{scope: user, execPath}`. `discover_exec_path` prefers a sibling `gn-drive`, then `GN_DRIVE_BIN`, then `gn-drive` on PATH |
| UC-147 | done | Uninstall confirms, then `service.uninstall` |
| UC-148 | n/a | The window manages the user scope only. `--system` is CLI-only |
| UC-149 | done | About shows the app version, the rclone version, and Check for updates |
| UC-150 | done | History shows 20 rows (see UC-110) |
| UC-151 | done | `service.health`: present or stale, pid, uptime, heartbeats, active tasks, last error |
| UC-152 | done | The four switches go through `settings.set`. Language and theme stay in `window.json`, not `auth.json` |

## L. Self-update

| UC | Status | Evidence / reason |
|---|---|---|
| UC-153 | done | `update.check` fills the result card. A second click is ignored while `update_busy`, and the button is replaced by "Checking for updates…" |
| UC-154 | n/a | CLI-only `self-update --check` |
| UC-155 | done | Apply update confirms, then `update.apply`, and shows the restart hint |
| UC-156 | n/a | Release-asset naming is an engine concern, not a window behaviour |
| UC-157 | done | `version()` comes from the snapshot or `hello` |

## M. CLI surface

| UC | Status | Evidence / reason |
|---|---|---|
| UC-158 | n/a | CLI-only `run` |
| UC-159 | n/a | CLI-only `run --service` |
| UC-160 | n/a | CLI-only `--password` / `GN_DRIVE_PASSWORD` |
| UC-161 | done | `instance_busy` is a fatal screen that names the holder pid. The lock itself is engine-side |
| UC-162 | n/a | CLI-only one-shot sync |
| UC-163 | n/a | CLI-only board run |
| UC-164 | n/a | CLI-only profile commands |
| UC-165 | n/a | CLI-only remote commands. The window covers UC-112 through UC-124 |
| UC-166 | n/a | CLI-only self-update |
| UC-167 | n/a | CLI `doctor`. The window equivalent is UC-173 |
| UC-168 | n/a | CLI-only shell completion |
| UC-169 | done | Default backend is in-process `gn-drive-core`. `GN_DRIVE_MOCK=1` selects the mock. `GN_DRIVE_BACKEND=stdio` spawns `gn-drive serve --stdio` (`GN_DRIVE_BIN` override) and restarts it with backoff. The dev install no longer copies a `gn-drive` binary into the app; rclone is still bundled beside the executable |

## N. Logs and diagnostics

| UC | Status | Evidence / reason |
|---|---|---|
| UC-170 | n/a | CLI stderr logging |
| UC-171 | done | The Logs tab reads `logs.read` and appends `log:line` |
| UC-172 | partial | The filter is All lines versus Problems only (`level: warn`). The RPC `logs.read` params are `limit`, `level`, and `since`. There is no stdout/stderr source selector to bind |
| UC-173 | done | Doctor is called with `includeDataFiles: true` and the report lists `data_files` under "Data files" |
| UC-174 | done | Health card: Healthy / Stale / No heartbeat, pid, uptime, last and next sync, active tasks, last error |

## O. Legacy-only features (do NOT require reimplementation)

| UC | Status | Evidence / reason |
|---|---|---|
| UC-175 | n/a | Legacy HTTP API |
| UC-176 | n/a | Legacy SSE |
| UC-177 | n/a | Legacy WebSocket. The window still shows a reconnecting state if a stdio child dies |
| UC-178 | n/a | Legacy `--port` / `--no-browser` |
| UC-179 | n/a | Windows-only PowerShell completion |
| UC-180 | n/a | systemd / Windows SCM. Not a macOS window concern |
| UC-181 | n/a | Legacy one-shot operations API |
| UC-182 | n/a | Legacy arbitrary settings keys |
| UC-183 | n/a | Legacy Vue flow-card edit mode |
| UC-184 | n/a | Legacy FlowRunStatusPanel |
| UC-185 | n/a | Legacy three-button edge overlay |
| UC-186 | n/a | Legacy particle animation |
| UC-187 | n/a | Legacy control scheme. This window pans by drag and zooms by wheel and pinch anyway |
| UC-188 | n/a | Optional legacy Vue nicety |
| UC-189 | n/a | Legacy Vue remote UI |
| UC-190 | n/a | Legacy route redirects |
| UC-191 | n/a | Legacy `pull`. The window offers push, two-way, and re-sync |
| UC-192 | n/a | Legacy doc inaccuracy |
| UC-193 | n/a | Legacy undocumented HTTP route |
| UC-194 | n/a | Legacy CLI usage string |
| UC-195 | n/a | Legacy board `--concurrency` |
| UC-196 | done | Password length uses `chars().count()`, matching the Swift app |

## P. Cross-cutting UX states the window must implement

| UC | Status | Evidence / reason |
|---|---|---|
| UC-197 | done | Unlock, change password, add account, browse, OAuth, and update check each have a busy flag and ignore a second submit. A 25-second timeout clears the flag so the action can be tried again. Nothing retries OAuth on its own |
| UC-198 | done | RPC failures go through `explain`: one sentence, with the engine code on a Details line or tooltip. Cancellation noise is dropped |
| UC-199 | done | Lock, delete flow, delete remote, remove password, and apply update confirm and name the item. Deleting a node or a connection does not confirm |
| UC-200 | done | Escape closes the dialog, Delete removes the canvas selection, Enter submits unlock. The UX round also binds ⌘N, ⌘S, ⌘R, ⌘., ⌘F, ⌘,, ⌘I, ⌘⇧R, ⌘⇧L, ⌘=, ⌘-, and ⌘0. Those were requested for this window; the original spec said not to invent them |
| UC-201 | done | `every_control_has_a_tooltip` fails the build when a button, switch, select, or segmented control has no tooltip. Status chips also use words (Running, Failed, Stopped, Connected) |
| UC-202 | partial | Same gap as UC-15. Visible strings are English unless they are in the small `prefs::t` table |
| UC-203 | done | Same shutdown as UC-14. The window does not leave an in-process dispatcher or a stdio child running after Quit |
| UC-204 | done | A second engine on the same config dir surfaces `instance_busy` on the fatal screen, including the holder pid |

## What is still partial

1. **Language (UC-15, UC-202).** The View menu switches a short en/vi table. The Swift catalog is about 298 keys and is not ported, so a Vietnamese session is still mostly English.
2. **Log source (UC-172).** Problems only filters by level. `logs.read` has no stdout/stderr source; the engine (`gn-drive-core`) needs a `source: out|err` param reading the launchd `gn-drive.{out,err}.log` before the window can bind a selector.
