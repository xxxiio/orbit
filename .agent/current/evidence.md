---
template: evidence-manifest
template_version: 1.0.0
template_digest: "2787d570257f4ffbf8add20f62aa21227a4aea1a6723583e8928b5e71e09f79b"
project_owned: true
record_contract: evidence-manifest
record_schema_version: 1
status: draft
task_id: TASK-0002
---

# Evidence Manifest

## Task

TASK-0002 — expose M25-I3 trusted capability installation and explicit action request/execute.

## Acceptance evidence

| Criterion ID | Status | Evidence |
|---|---|---|
| AC-001: Trusted-grant capability publication | not-run | Pending implementation |
| AC-002: Persisted action request with stable command ID | not-run | Pending implementation |
| AC-003: Deterministic claim/duplicate/refusal/ambiguity coverage | not-run | Pending implementation |
| AC-004: No automatic task completion | not-run | Pending implementation |
| AC-005: Full M15 regression | not-run | Baseline evidence is `.agent/evidence/m15-final.zip`; rerun required after the change |

## Quality gate evidence

| Gate | Status | Command or artefact | Result |
|---|---|---|---|
| Baseline M15 (pre-change) | pass | `.agent/evidence/m15-final.zip` | 38 passed, 0 failed, 0 not_run |
| TASK-0002 implementation gates | not-run | Pending | Not yet executed |

## Checks not run

- All TASK-0002 implementation and regression gates are pending.

## Waivers

- None.

## Residual risk

- Capability-requiring blueprints fail closed until this task is complete.
