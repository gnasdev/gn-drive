---
type: module
title: "CLI"
description: "cobra subcommands for run, service, sync, board, profile, remote, and tooling."
tags: ["module", "cli"]
timestamp: 2026-07-11T00:00:00Z
status: active
compliance: current-state
---

# CLI

Package: `cmd/gn-drive`.

## Subcommands

| Command | Purpose |
|---------|---------|
| `run` | Portal or `--service`; port, browser, password flags |
| `service` | install/uninstall/start/stop/status/restart |
| `sync` | One-shot `pull\|push\|bi\|bi-resync\|dry-run` with `--profile` |
| `board` | Execute board DAG by id/name |
| `profile` | list / add / delete |
| `remote` | list / add / test / delete |
| `self-update` | Apply GitHub release update |
| `version` | Version + commit (ldflags) |
| `doctor` | Environment diagnostics (`--data` lists config files) |
| `completion` | Shell completion scripts |

## Run flags

- `--port` (default 53241)
- `--no-browser`
- `--dev` (debug-oriented logging; still portal)
- `--service`
- `--password` / `GN_DRIVE_PASSWORD` for non-interactive unlock

## Related

- [App](/modules/app.md)
- [README usage](../../README.md)
