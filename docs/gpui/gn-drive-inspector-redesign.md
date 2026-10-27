# GN Drive inspector redesign

Scope: `apps/gn-drive-gpui` only. The panel is the right-hand inspector (`workspace.rs`, fed by `sync_config.rs` and window state in `main.rs`). Screenshots and app launch stay with the coordinator. This pass does not start GN Drive, rclone, or a browser.

## Findings

The panel is one 340px column. A header always says “Draft until saved” (or “Read-only while running”). The body is a `ScrollArea` that stacks every field for the current selection. Nothing is grouped in plain language, and nothing is collapsed.

| Bug | Where | Root cause |
|---|---|---|
| Toggle labels paint one character per line (“Use regex”, “Delete excluded”, “Dry run”) | `workspace.rs` bool row, about 2222–2253 | The label column sets `min_w(px(0.))` and does not set `flex_1()`. Next to the switch, its minimum size is 0, so the column shrinks away. GPUI then lays the string out in a zero-width box and breaks one glyph per line. The label never asks for word wrapping (`whitespace_normal`), and the switch slot is not `flex_shrink_0`, so the switch keeps its width and the label loses the row. |
| “Max delete” shows a stray `0` under the input | `sync_config.rs` 270–277 (`hint: "0"`); painted at `workspace.rs` 2296–2299 | `FieldDef.hint` is the empty-input placeholder (the example default). The edge inspector paints every non-empty hint under the control with `prose(def.hint)`. The value already in the field is repeated as a second line. The same echo hits Parallel transfers (`"4"`), Bandwidth (`"0"`), retries, depths, and the other numeric placeholders (`sync_config.rs` 69–405). |
| “SAFETY” wears a yellow bar | `workspace.rs` 2214 via `ns_ui::section_label`; local `eyebrow` at 2495–2509 | Both headers draw a 3×12 bar in `theme.accent`. GN Drive’s accent is amber `#f0c674` (`ns-ui` tint). Safety is not a warning, so the bar reads as a stray caution. |
| Long vertical sprawl, hard to scan | `workspace.rs` 2192–2214 | Every rclone group is expanded, in jargon order: Performance, Filtering, Safety, Comparison, then Sync or Two-way options. About forty fields are always on screen. There is no Advanced disclosure and no remembered open/closed state. |
| “Draft until saved” even when nothing changed | `workspace.rs` 1604–1612 | The header subtitle is a fixed string. It does not follow `dirty`, and typed text fields do not join that flag until Save (`main.rs` `collect_editors`, about 1990). Save / Discard for a flow sit in the scrolling body (`workspace.rs` 1802–1820), so they scroll away. An edge has no Save in the panel at all. |
| No empty state | `workspace.rs` 1640–1642, 1850–1852, 2080–2082 | A missing draft, node, or edge returns an empty `div`. Clicking the canvas always selects the flow (`main.rs` `canvas_down`, about 3803), so “nothing selected” never has its own panel. |
| Bad numbers disappear on save | `main.rs` `collect_sync_values`, about 1969–1971 | `parse::<i64>().ok()` turns an unreadable number into “unset” and drops the stored value. Nothing is drawn under the field, and Save stays enabled. |
| Width is a single 340px | `workspace.rs` 1616 | Not clamped. A short window can crush it; a wide one never grows it to a readable max. |

Selection coverage before the redesign:

| Selection | What the panel shows |
|---|---|
| Nothing | Not a state. Canvas background click selects the flow. |
| Flow | Name, schedule toggle, cron preset, custom cron, status chips, Run/Stop, Save/Discard, log, delete. |
| Node | Label, path, remote select, browse, location, incoming and outgoing links, remove. |
| Edge `push` | Direction, From, To, progress when live, then all groups except bisync. Includes delete timing. |
| Edge `bi` / `bi-resync` | Same chrome. Bisync fields replace delete timing. |
| Running edge | Same fields, switches and menus disabled, text inputs locked, progress panel and error text. Header says read-only. |

No field is stored only in the view. The source of truth is `FIELDS` in `sync_config.rs` (41 flags) plus the flow name, cron, node label/path/remote, and edge endpoints.

## Information architecture

The panel is a fixed column: preferred 320px, minimum 280px, maximum 380px (full width on a compact window). Header and footer stay pinned. The middle scrolls (`id="inspector-scroll"`, `overflow_y_scroll`).

Header: small-caps “INSPECTOR” and a muted context word (Flow, Folder, Connection, or Nothing selected). While a flow is running, the context is “Read-only”. No draft sentence and no accent bar.

Footer, always pinned:

| State | Status | Discard | Save |
|---|---|---|---|
| Clean, not running | “All changes saved” (muted) | hidden | disabled, tooltip “Nothing to save.” |
| Unsaved, valid | “Unsaved changes” | enabled | enabled, tooltip “Save this flow”, shortcut ⌘S |
| Unsaved, invalid | “Unsaved changes” | enabled | disabled, tooltip “Save is unavailable. ” plus the first field error |
| Running | the same status, plus “Read-only while running” | disabled if there are unsaved edits | disabled, tooltip “Stop the flow before saving changes.” |

Typed text counts as unsaved even before the `dirty` flag flips. Invalid numbers are kept on screen and are not written over the stored value.

Save is only in this footer. The workspace toolbar does not have a second Save. It keeps Add folder and Run (Stop while the flow is running). ⌘S is a window binding, so it saves the open draft even when a compact window is showing the canvas and the inspector is hidden. Unsaved work shows as the footer line “Unsaved changes” and as a small dot beside the flow name in the toolbar. The dot’s tooltip is “Unsaved changes — press ⌘S to save”. The selected flow in the sidebar shows the same dot.

Sections remember open or closed on the window (`inspector_open`). Advanced starts closed. Every other section starts open. The chevron tooltip is “Show &lt;section&gt; options” or “Hide &lt;section&gt; options”. Headers are small caps in the muted text colour, with a border in `theme.border`. Amber is only for a real warning (the running banner, and red/warn copy under a bad field).

### Sections

| Section | Starts | Selection | Contents |
|---|---|---|---|
| Basics | open | Flow | Name, status, connection count, Run / Stop, recent log. Hint to add a folder when the canvas is empty. |
| Schedule | open | Flow | Schedule switch, plain-English preview (`cron_label`), preset menu, custom cron and its error. |
| Basics | open | Node | Label, path, account, browse, resolved location. |
| Connections | open | Node | Incoming and outgoing links. |
| Basics | open | Edge | Direction (Push / Two-way / Re-sync) and the one-line explanation, From, To, browse, live progress, runtime error. |
| What to sync | open | Edge | Include, exclude, regex, min/max size, min/max age, max depth. |
| Safety | open | Edge | Dry run, max delete, delete excluded, immutable, backup suffix, keep extension, backup folder. |
| Advanced | **closed** | Edge | Performance, rare filters, comparison, and either delete timing (push) or two-way conflict options (`bi`, `bi-resync`). |
| Danger zone | open | Flow, node, edge | Delete flow / remove folder / remove connection. Still asks before deleting a flow. |
| Empty | — | Nothing selected | Short copy, how to select a folder or a connection, Add folder, Flow settings. |

Clicking empty canvas clears the selection (Nothing selected) instead of opening flow settings. Flow settings stay one click away: the empty-state button, or clicking the current flow in the rail. Opening a flow from the rail still lands on the flow, then follows a single edge or a live run, as before.

Switches and menus still apply immediately. Text still applies on Save, on Run, and now also when the selection changes, so leaving a field does not drop a valid edit. An invalid number is left as it was stored.

## Row layout

Toggle (bool): one horizontal row.

- Label column: `flex_1()`, `min_w(px(0.))`, `whitespace_normal()`, so the words wrap and the column cannot collapse to zero.
- Help under the label, muted `text_xs`, word-wrapped.
- Switch: `flex_shrink_0()`, tooltip says what turning it on does.

Text, number, list, and select: label on its own line, control full width underneath, then muted help, then the error.

Number controls keep the old placeholder inside the empty input only. They do not print that placeholder again. The unit sits on the label row (“files”, “MB/s”, “levels”). The help carries “0 means no limit” where that is true.

Validation, inline, under the control, in the error colour:

- Whole-number fields: empty is allowed (clears the flag). Otherwise a non-negative integer. Message: “Enter a whole number, or leave this blank.” / “Use 0 or a positive whole number.”
- `tps_limit`: empty or a non-negative finite number.
- Cron: the existing five-field check.
- Node path, edge From, and edge To: must not be blank.

Save and Run refuse while any of those fail. ⌘S hits the same check.

## Advanced (collapsed by default)

Performance: parallel transfers, bandwidth, multi-thread streams, buffer size, retries, low-level retries, max duration, check first, order by, pause between retries, request limit, connect timeout, I/O timeout.

Rare filters and caps: filter file, exclude-if-present, max transfer, max delete size.

Comparison: compare size only, skip newer files, ignore existing.

Push only: when to delete.

Two-way and re-sync only: if both sides changed, keep going after errors, lock wait, check access first, losing copy, conflict suffix.

Not advanced: include/exclude/regex/size/age/depth, and the Safety set (dry run, max delete, delete excluded, immutable, backup suffix, keep extension, backup folder).

## Before / after

| | Before | After |
|---|---|---|
| Width | 340px fixed | 320px, clamp 280–380; full width when compact |
| Scroll | `ScrollArea` around everything under the header | Header and footer pinned; `inspector-scroll` uses `overflow_y_scroll` |
| Draft status | Header “Draft until saved” | Footer “Unsaved changes” or “All changes saved”, with Discard and Save |
| Save control | Toolbar and the scrolling body | Footer only. Toolbar is Add folder and Run. ⌘S still saves |
| Toggle row | Label column `min_w(0)` only; glyph-per-line | Label `flex_1` + `min_w(0)` + word wrap; switch does not shrink |
| Max delete | Input plus a second “0” | Unit “files”, help “0 means no limit.” No echoed placeholder |
| Section chrome | Amber accent bar, rclone group names | Small caps, chevron, theme border. Amber only on a real warning |
| Density | ~40 fields open | What to sync and Safety open; Advanced closed and remembered |
| Errors | Bad numbers dropped on save | Red line under the field; Save disabled with a reason tooltip |
| Nothing selected | Blank, or the flow form | Empty state, Add folder, link to flow settings |
| Running | Header subtitle, inputs locked | Banner plus footer note; fields still locked; progress kept |
| Fields | 41 flags, flow, node, edge | Same flags and the same actions. None removed |

## Tests

Pure helpers cover section membership (including push vs `bi` / `bi-resync`), which section starts closed, number validation, the save-bar label, and that a numeric placeholder is not the help string.

A GPUI `VisualTestContext` layout test is not wired up: this crate has no window test harness, and this pass must not open the app. The toggle row is built from `toggle_row_style()` (`label_grows`, `label_min_width_zero`, `label_wraps_words`, `switch_does_not_shrink`). The test locks that spec, and the row builder applies it with `flex_1`, `min_w(px(0.))`, `whitespace_normal`, and `flex_shrink_0`.

`tip.rs` still scans every `src/*.rs` module, including the new inspector module, for a button, switch, select, or click with no tooltip.
