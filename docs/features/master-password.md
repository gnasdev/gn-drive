---
type: feature
title: "Master Password"
description: "Optional password that encrypts config at rest and gates the web session."
tags: ["feature", "security", "auth"]
timestamp: 2026-07-11T00:00:00Z
status: active
---

# Master Password

## Behavior

Users may configure a master password on first run (setup) or later. When enabled:

- `gn-drive.db` and `rclone.conf` are encrypted at rest as `.enc` while locked
- Web portal can start locked and unlock via SPA
- Service/CLI need password flag or env at process start

## Flows

1. **Setup** — `POST /auth/setup` with password ≥ 4 characters → session + data plane
2. **Unlock** — `POST /auth/unlock` → decrypt → `AfterUnlock` → session cookie
3. **Lock** — `POST /auth/lock` → `BeforeLock` → encrypt → clear session
4. **Change password** — authenticated rotation

## Failure modes

- Invalid password → 401; failed attempt counters increase
- Lockout after repeated failures → must wait `retry_after_secs`
- Data plane open failure after unlock rolls unlock back so retry is clean

## Critical requirements

### REQ-MP-1: Loopback only

Password protects local at-rest data; it is not multi-user auth over a network.

### REQ-MP-2: Transparent upgrade

`auth.json` and `.enc` formats remain compatible with prior Wails desktop data.

## Related

- [Auth module](/modules/auth.md)
- [API conventions](/shared/api-conventions.md)
