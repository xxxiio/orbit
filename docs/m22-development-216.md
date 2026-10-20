# Development 216 — Accepted Today/Next planning brief

## Problem

The existing day/schedule planning services return individual estimates and slot
proposals, while the Today surface lacks an authoritative explanation of blocked,
future, or unsized work. Clients can display task records without knowing whether
those tasks are executable. M22 needs one consistent, deterministic read model.

## Delivered

- `planning_brief::build_work_brief`: a pure planning projection from one accepted
  task/dependency/schedule revision, suitable for disconnected clients.
- Explicit `scheduled`, `ready`, `blocked`, `deferred`, `completed` dispositions.
- One representation per accepted task, stable across incoming SQLite ordering.
- Proposed start/end times, blocking task IDs, downstream tasks, project and
  milestone IDs, recommended rank, deadline, and human-readable explanations.
- Project-level counts and top-level capacity/count summaries.
- Milestone-level counts, proposed minutes, overdue task IDs, and the most
  common direct dependency bottlenecks, without mixing projects.
- `AuthoritativeHost::work_brief` checks that task, readiness, and schedule
  revisions agree before building a proposal; mismatches fail closed.
- `AuthorityConnection::work_brief` uses only the last accepted remote snapshot;
  durable pending offline commands are never projected as completed or ready.
- `GET /v1/authoritative/work-brief` and `orbit task work-brief` expose identical
  Rust business logic without creating reservations or mutating any task.
- Eight focused work-brief tests for mixed dispositions, capacity, project filters,
  deterministic ordering, invalid inputs, bounded proposals, and milestone bottlenecks.
- MCP `planning.work_brief` read tool for embedded and connected agents.
  It requires unrestricted execution read permission because blocker IDs can
  reveal cross-project identities; constrained agents fail closed.

## Safe hypothetical completion impact

- `task_impact::preview_completion` computes the immediate dependency impact of
  hypothetically completing one accepted task without applying any mutation.
- Reports directly affected tasks, which tasks become actionable, remaining
  prerequisites, per-task explanation, and a deterministic next-action order.
- Validates the accepted graph and refuses cancelled/missing/duplicate targets.
- `AuthoritativeHost::preview_task_completion` and
  `AuthorityConnection::preview_task_completion` share one evaluator. Remote
  evaluation uses only the accepted snapshot, not pending offline intentions.
- Available at `GET /v1/authoritative/completion-impact`, `orbit task
  completion-impact`, and MCP `planning.completion_impact`.
- Both new MCP tools fail closed for restricted project scope to prevent
  cross-project dependency disclosure.
- Seven tests exercise immediate unlock, partial blockers, delayed work,
  already-complete targets, cancelled targets, corrupt cycles and limits.

## Usage

`orbit task work-brief --now-ms 1800000000000 --ends-at-ms 1800028800000 --limit 15`

`orbit task completion-impact task-a --now-ms 1800000000000 --limit 10`

## Remaining M22 feature work

Integrate the accepted work brief into the terminal Today display and broaden
agent-facing capability discovery; the shared read model and scoped MCP contract
are now available for client consumption. The old `orbit today` command has
not been replaced here; avoiding an incompatible contract change is intentional.

## Validation

Source integration assertions passed. Cargo and rustc are unavailable; no Rust
compilation, executable tests, HTTP smoke, or integrated TUI acceptance was run.
M22 remains a provisional source-implementation estimate, not release readiness.
