---
type: module
title: "Auth"
description: "Master password with Argon2id, AES-GCM config encryption, and unlock rate limits."
tags: ["module", "auth", "security"]
timestamp: 2026-07-11T00:00:00Z
status: active
compliance: current-state
---

# Auth

Package: `internal/auth`.

## Responsibility

Optional master password, at-rest encryption of SQLite DB and `rclone.conf`, unlock rate limiting, app settings persistence in `auth.json`.

## Crypto

| Piece | Detail |
|-------|--------|
| Password hash | Argon2id `m=65536,t=3,p=4`, 32-byte salt/key |
| File encryption | AES-256-GCM; key derived from password |
| On-disk enc | 12-byte nonce prefix + ciphertext (`.enc`) |

## Rate limit

- Soft delay after 3 failures
- Hard lockout after 10 failures for 5 minutes
- State persisted in `auth.json`

## Public errors

`ErrNotSetup`, `ErrAlreadyUnlocked`, `ErrNotUnlocked`, `ErrInvalidPassword`, `ErrLocked`, `ErrAlreadySetup`.

## Business rules

- Setup requires password length ≥ 4 (enforced at API)
- Lock encrypts sensitive files and clears in-memory key
- Unlock decrypts and sets unlocked flag; API then opens data plane in portal mode
- Wire format matches former Wails desktop for transparent upgrade

## Related

- [Master password feature](/features/master-password.md)
- [App module](/modules/app.md)
