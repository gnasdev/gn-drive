---
type: shared
title: "Glossary"
description: "Shared product and engineering terms for GN Drive."
tags: ["glossary", "shared"]
timestamp: 2026-07-11T00:00:00Z
status: active
---

# Glossary

| Term | Meaning |
|------|---------|
| **Portal mode** | Web `run` path that can start while locked and unlock via SPA |
| **Data plane** | Open store + rclone client attached to engines |
| **Flow** | Named container of sequential operations; primary workspace unit |
| **Operation** | One source remote/path → target remote/path + action + sync_config |
| **Profile** | Named sync option bag (from/to + flags) for CLI/API one-shot |
| **Remote** | rclone remote entry in `rclone.conf` |
| **Board** | DAG of nodes (endpoints) and edges (syncs); CLI-oriented |
| **Action** | Sync direction/mode: product surface `push` \| `bi` \| `bi-resync`; CLI also `pull`, `dry-run` |
| **Bisync** | rclone bidirectional sync; `bi` incremental, `bi-resync` baseline with `--resync` |
| **SyncConfig** | JSON option overlay on operations / edges (mirrors profile flags) |
| **Session** | In-memory token in cookie `gn-drive-session` after unlock/setup |
| **Lockout** | Temporary auth block after failed unlock attempts |
| **SSE** | Server-Sent Events stream for live progress |
| **Instance lock** | flock ensuring one portal process per config dir |
| **Service mode** | OS-managed background process; password at start, no browser |
