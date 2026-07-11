---
type: feature
title: "Service Mode"
description: "Opt-in OS background service for headless portal process."
tags: ["feature", "service"]
timestamp: 2026-07-11T00:00:00Z
status: active
---

# Service Mode

## Behavior

Users install an OS service only if they want background operation:

```bash
gn-drive service install
gn-drive service start
```

The service process is `gn-drive run --service` (no browser). Master password must be provided at start (`--password` or `GN_DRIVE_PASSWORD`) because there is no unlock UI.

## Platforms

- Linux: systemd user (default) or system unit
- macOS: launchd LaunchAgent
- Windows: SCM

## Health

When enabled, a health writer updates `service.health` in the config directory on a short interval for external monitors/`doctor`.

## Related

- [Service module](/modules/service.md)
- [CLI](/modules/cli.md)
