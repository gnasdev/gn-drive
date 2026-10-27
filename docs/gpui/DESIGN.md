# GN Drive Workspace (addendum)

GN Drive reuses the shared chrome and tokens above with the amber accent (`#F0C674`, hover `#FFDC9A`, ink `#201503`, selected `#4A3913`). Surfaces: the flow rail (260px) lists flows with a status dot and schedule; the canvas draws nodes (170x56 at zoom 1) and orthogonal edges, with one edge card per operation showing action and live bytes and speed; the inspector (flow, node, edge) sits beside the canvas on wide windows and replaces it on narrow ones, with a Back arrow. Remotes and Settings use the shared 320px side column (full width below 720pt). Unlock is a centered 400px card.

- [ ] Every list (flows, remotes, history, logs, folders, providers) has loading, empty, and error states.
- [ ] Destructive actions (delete flow, node, edge or remote, clear history, uninstall service, remove password, apply update) open a confirmation card first.
- [ ] A running flow's inspector and canvas are read-only; Stop is the only edit.
- [ ] Escape closes the dialog or confirmation; the canvas refits when its size changes until the user pans or zooms.
- [ ] 1440x900 and 390x844 are both usable; no authorization starts without a click.
