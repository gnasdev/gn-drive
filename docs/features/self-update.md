---
type: feature
title: "Self-Update"
description: "Update the running binary from GitHub Releases."
tags: ["feature", "release"]
timestamp: 2026-07-11T00:00:00Z
status: active
---

# Self-Update

## Behavior

- CLI: `gn-drive self-update`
- API/UI: `POST /api/v1/self-update` from Settings

Downloads the matching platform artifact from GitHub Releases and swaps the binary atomically when possible. Version string comes from build ldflags (`Version` / `Commit`).

## Related

- [CLI](/modules/cli.md)
- Release workflow: `.github/workflows/release.yml`
