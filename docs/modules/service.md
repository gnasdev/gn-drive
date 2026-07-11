---
type: module
title: "Service"
description: "OS service install/start/stop and health heartbeat writer."
tags: ["module", "service"]
timestamp: 2026-07-11T00:00:00Z
status: active
compliance: current-state
---

# Service

Package: `internal/service`.

## Responsibility

Platform managers for opt-in background install:

| OS | Mechanism |
|----|-----------|
| Linux | systemd user unit (default) or system (`--system`) |
| macOS | launchd LaunchAgent |
| Windows | SCM |

Also writes `service.health` JSON periodically when running with health writer attached.

## CLI

`gn-drive service [install|uninstall|start|stop|status|restart]`.

## Related

- [Service mode feature](/features/service-mode.md)
