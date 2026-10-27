# GN Drive GPUI UX review

Mock engine only (`GN_DRIVE_MOCK=1`, password `drive1234`). No real rclone and no OAuth. The sign-in dialog in these shots uses `https://accounts.example.invalid` and was not submitted.

Captures are Retina (scale 2). Requested sizes were 1440×900, 600×800, and 390×844. The window accepted 600×800. The display clamped the other two to 1440×816 and 390×816 points, so those files are 2880×1632 and 780×1632 pixels. 600×800 is 1200×1600 pixels.

Before shots: `~/.agent-scratch/omp/gn/gnd-gpui/shots/before-*.png` (and `before-m*.png` for the narrow window).
After shots: `~/.agent-scratch/grok/gn/gnd-gpui/shots/`.

Long fixtures live in the mock: flow name, folder, file name, cloud account, and a failed copy whose engine text starts `rclone: failed to copy …`. The sentence on screen is “Failed to copy …”. The raw line is the tooltip / Details line, not a second banner.

## Unlock

| | |
|---|---|
| Before | `before-01-unlock.png`, `before-m01-unlock.png` |
| Issue | The lockout sentence was one line and ran past the card (high). Owned by the unlock pass; this pass did not edit `unlock.rs`. |
| Fix | The unlock card wraps its body, path, and footer. |
| After | `after-1440x900-unlock.png`, `after-600x800-unlock.png`, `after-390x844-unlock.png` |

## Workspace and the long flow

| | |
|---|---|
| Before | `before-02-workspace.png`, `before-03-flow0.png`, `before-03-flow1.png`, `before-m02-list.png`, `before-m03-canvas.png` |
| Issue | Flow names, node titles, and paths could paint past the rail and the node card (high). A failed sync was easy to miss, and the activity card said “No activity yet” because unlock cleared the seeded runtime (high). |
| Fix | Rail names and node titles truncate, with the full name and path in the tooltip. Paths use a middle ellipsis. The file card wraps the failure and ellipsizes each file name. Unlock restores the seeded failed archive instead of wiping it. Status chips are sentence case (Failed, Waiting, Stopped). |
| After | `after-1440x900-workspace.png` (Nightly Photos), `after-1440x900-long-flow.png`, `after-600x800-long-flow.png`, `after-390x844-workspace.png`, `after-390x844-long-flow.png` |

At 390 the header title ellipsizes (`Archive of the family photo library from the ol…`). The file card stays inside the canvas. The red failure wraps on `/` and `_`. File rows read `…lename_0001.heic` / Failed and `…long-name.heic` / Waiting.

The same failure sentence is also in the inspector progress block. It is one sentence, not a second conflicting error. The canvas card is what you see when the inspector is closed. The inspector is what you see when a narrow window replaces the canvas.

## Node and connection

| | |
|---|---|
| Before | `before-04-edge.png`, `before-05-node.png`, `before-m04-edge-inspector.png` |
| Issue | Node subtitles and the connection route ran out of the card (high). Browse stayed available while a flow was running (medium, UC-131). |
| Fix | Node cards are 170×56, title and path truncate, tooltip is the heading plus the full path. The edge route uses one-line ellipsis. Browse is disabled while running, with the tooltip “Stop the flow before choosing a different folder.” From and To stay editable fields, so they show the start of the path inside the field rather than a middle ellipsis. |
| After | `after-1440x900-node.png`, `after-1440x900-edge.png`, `after-1440x900-inspector.png`, `after-600x800-inspector.png`, `after-390x844-inspector.png` |

The inspector failure box wraps. File names in the progress list ellipsize. “Push” / “Two-way” / “Re-sync” are the action labels.

## Cloud accounts

| | |
|---|---|
| Before | `before-06-remotes.png`, `before-m06-remotes.png` |
| Issue | Long account names and descriptions ran out of the row (high). The kind chip showed the raw id `drive` (medium). |
| Fix | Name and description ellipsize in a `min_w_0` column, with the full string in the tooltip. Kind chips use a short name (Google Drive, SFTP, Dropbox). The sign-in note wraps. |
| After | `after-1440x900-remotes.png`, `after-600x800-remotes.png`, `after-390x844-remotes.png` |

At 390 the long account reads `family-arch…` and `Google Drive account us…`. The note wraps inside the column.

## Settings

| | |
|---|---|
| Before | `before-07-settings-general.png`, `before-07-settings-service.png`, `before-07-settings-history.png`, `before-07-settings-logs.png`, `before-07-settings-about.png`, `before-m07-settings.png` |
| Issue | Paths and the health error were single lines (high). History showed `push` and `failed`. Doctor rows showed `database` (medium). |
| Fix | Value rows and the health error wrap. Path rows middle-ellipsize and tooltip the full path. History actions use Push / Two-way, states use Failed / Completed / Stopped, and the error is one wrapped sentence. Doctor labels are sentence case (Database, Auth) and chips say Passed / Info. Service chips say Service and User, not `service` and `user`. Error log lines use the same sentence, with the raw line in the tooltip. |
| After | `after-1440x900-settings-general.png`, `after-1440x900-settings-service.png`, `after-1440x900-settings-history.png`, `after-1440x900-settings-logs.png`, `after-1440x900-settings-about.png`, plus service, history, and logs at 600 and 390 (`after-600x800-settings-*.png`, `after-390x844-settings-*.png`) |

The log list sits under the health checks in the side column, so a short window shows the checks first. The error line uses the same wrap as history.

## Dialogs

| | |
|---|---|
| Before | `before-08-dialog-new-flow.png`, `before-08-dialog-add-remote.png`, `before-08-dialog-oauth.png`, `before-08-dialog-browse.png`, `before-08-dialog-change-password.png`, `before-08-dialog-confirm-lock.png`, `before-m08-add-remote.png`, `before-m09-confirm.png` |
| Issue | The sign-in URL and the four buttons painted past the card (high). On a 390-point window the dialog subtitle was one line and was cut off at the right edge (high). Closing the demo sign-in dialog toasted `oauth_session_not_found` (medium). |
| Fix | The long URL wraps inside its box, tooltip is the raw URL. The buttons are a wrapping row in the body, so Cancel, Copy link, Open in browser, and Complete (mock) stay inside the card. Header subtitles are one short line. The longer instruction is a wrapped note in the body. The demo session is not sent to `remotes.oauth.cancel`. That code, if it does arrive, reads “That sign-in session is no longer there.” with `Details: oauth_session_not_found`. Confirmations name the flow and wrap. Browse ellipsizes the long file name. |
| After | `after-1440x900-dialog-oauth.png`, `after-1440x900-dialog-browse.png`, `after-1440x900-dialog-add-remote.png`, `after-1440x900-dialog-new-flow.png`, `after-1440x900-dialog-password.png`, `after-1440x900-dialog-confirm.png`, `after-1440x900-dialog-delete-flow.png`, and the 600 and 390 oauth, browse, confirm, and delete shots |

Delete at 390 wraps both the title and the body of “Archive of the family photo library from the old apartment and the cabin trips” inside the card. Cancel and Delete flow stay inside the card.

## What is still partial

Parity was **151 done / 7 partial / 0 missing / 46 n/a** after this review. GN-33 (2026-10-24) closed UC-17, UC-27, UC-57, and UC-144, so it is now **155 done / 3 partial**: UC-15 and UC-202 (short en/vi table) and UC-172 (no stdout/stderr log source). See `docs/gn-drive-parity.md`.
