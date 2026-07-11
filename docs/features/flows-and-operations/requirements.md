---
type: feature
title: "Flows and Operations Requirements"
description: "Critical requirements for sequential flow execution and product actions."
tags: ["requirements", "flows"]
timestamp: 2026-07-11T00:00:00Z
status: active
---

# Flows and Operations Requirements

Phạm vi: flow CRUD, execute/stop, operation action surface. Liên quan: [overview](../flows-and-operations.md).

## Critical Requirements

### REQ-FLOW-1: Product actions only

- Acceptance criteria: API and UI accept only `push` | `bi` | `bi-resync` for flow operations; legacy `pull` normalizes to `push`
- Applies to: store normalize, API create/update, workspace forms
- Failure mode: offering pull as a first-class flow action in the product UI

### REQ-FLOW-2: Sequential execution

- Acceptance criteria: operations run in sort order; one failure fails the flow; concurrent execute on same flow is rejected
- Applies to: `flowengine`, `POST /flows/{id}/execute`
- Failure mode: parallel ops within one flow or silent skip after failure treated as success

### REQ-FLOW-3: Terminal status retained

- Acceptance criteria: after a run ends, status remains completed/failed/cancelled long enough for UI poll (not immediately idle-only)
- Applies to: `flowengine.Status`, frontend run status panel
- Failure mode: failed runs reported as completed because WaitTask returned nil

### REQ-FLOW-4: Fixed path slots

- Acceptance criteria: source_remote/path and target_remote/path do not swap in storage when action changes
- Applies to: Operation model, save paths
- Failure mode: mutating stored direction by swapping endpoints
