---
type: module
title: "Auth Requirements"
description: "Critical security requirements for master password and sessions."
tags: ["requirements", "auth"]
timestamp: 2026-07-11T00:00:00Z
status: active
---

# Auth Requirements

Phạm vi: master password, encryption, HTTP session. Liên quan: [auth module](../auth.md).

## Critical Requirements

### REQ-AUTH-1: At-rest encryption when locked

- Acceptance criteria: with password set and locked, DB and rclone conf are not left as plaintext readable configs
- Applies to: Lock path, portal BeforeLock
- Failure mode: locking UI while leaving plaintext secrets on disk when `.enc` path is expected

### REQ-AUTH-2: Session required when setup

- Acceptance criteria: after password setup, protected `/api/v1/*` requires valid `gn-drive-session` cookie while unlocked
- Applies to: auth middleware
- Failure mode: data-plane APIs usable with unlock crypto but without session mint

### REQ-AUTH-3: Unlock rate limit

- Acceptance criteria: repeated failed unlocks eventually lock out for a cooldown period
- Applies to: auth.Service unlock
- Failure mode: unlimited online guessing against local API
