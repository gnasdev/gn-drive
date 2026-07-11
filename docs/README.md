---
type: index
title: "GN Drive Docs"
description: "Entry point for the GN Drive knowledge base."
tags: ["docs"]
timestamp: 2026-07-11T00:00:00Z
status: active
---

# GN Drive Docs

Repository knowledge base for current product behavior, architecture, module boundaries, and shared contracts.

## Structure

| Path | Role |
|------|------|
| [overview.md](./overview.md) | Product scope, runtime, guardrails |
| [_index.md](./_index.md) | Navigation and dependency map |
| [_sync.md](./_sync.md) | Docs/code sync snapshot |
| [architecture/](./architecture/) | System architecture |
| [modules/](./modules/) | Package boundaries and APIs |
| [features/](./features/) | Shipped user-facing behavior |
| [shared/](./shared/) | Models, glossary, API conventions |
| [development/](./development/) | Dev conventions (see also root [DEVELOPER.md](../DEVELOPER.md)) |
| [specs/planning/](./specs/planning/) | Plans (pre/during implementation) |

## Rules

- Describe **current** behavior only. History belongs in root `CHANGELOG.md`.
- Prefer short statements, real relative links, and OKF frontmatter (`type` required).
- When code changes behavior, update the smallest docs set and refresh `_sync.md`.
