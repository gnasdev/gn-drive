# Runtime edge telemetry

- Backend `sync:*` events and runtime snapshots are the source of truth for edge telemetry. Keep `profile_id` in the `flowId:operationId` form so `FlowCanvas` can route a snapshot to its operation edge.
- During pre-sync, file rows are commonly `pending`. They must remain visible in the edge card, but must not render as static edge dots. Edge dots require a live or terminal file state, and an active file row must display its percentage.
- A selected edge with an active operation opens its file card automatically. A card opened this way must remain open across pan and zoom; only an explicit outside click or another user action may dismiss it.
- Any change to runtime event handling must cover: live WebSocket event, reload/runtime snapshot, pending-file rendering, and active-operation edge routing. Add a targeted regression test for each affected boundary.

## Branching

- Work only on `main`. Do not create new branches.
- Commit directly on `main` and push to `origin/main`.
- Exception: Firstmate delivery workers may use one short-lived `fm/<task>` branch per task to open a pull request into `main`; that branch is deleted as soon as it is merged.
