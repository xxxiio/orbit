# M20 iteration 184 — cohort task-planning session (incomplete scheduling milestone)

## Scope delivered

`orbit_client::unified::{TaskPlanningSession, PlannedTaskWindow}` provide a typed, multi-task planning session, with `AuthorityConnection::plan_tasks` executing the planned steps through existing authoritative task commands. The entire manifest is validated before the first side effect; retry-stable command identities are generated from a caller-supplied prefix, and every step uses an incrementing expected revision. Partial execution returns `WorkflowFailure` with the accepted or queued prefix.

Preflight validates identity, number of tasks, duplicate task IDs, positive reasonable duration, nonnegative timestamps, ordered time windows, and enough time between earliest start and deadline for the given estimate. Validation of requested windows is deliberately **not** a promise of resource availability or conflict-free scheduling.

Four new Rust acceptance scenarios cover invalid-batch no-write, duplicate/impossible-window rejection, embedded authoritative persistence and duplicate receipts, and remote accepted/pending separation with retry deduplication.

## Remaining before planning and scheduling is complete

This is **not** yet an end-to-end calendar scheduling workflow. Scheduled block creation/rescheduling/deletion, overlap policy, schedule snapshot projection, and server-authoritative transport of schedule commands remain pending. Rust compilation, integration testing and multi-client scenarios must also be run.

## Five-iteration completion budget

- 184: multi-task task-planning session, with preflight validation and authority integration — partial delivery, not a milestone closure.
- 185: implement schedule block create/update/delete as canonical server transactions with semantic receipts and stable read projections.
- 186: wire schedule, routines, and their accepted remote command transports plus replica read paths.
- 187: complete project strategy/milestone authoritative writes and wire shared CLI/TUI/MCP service surfaces.
- 188: build, execute end-to-end smoke and two-client reconciliation scenarios, fix failures, remove conflicting legacy write paths; close M20 **only if** acceptance passes.

These are targets rather than guaranteed completion dates. A milestone percentage must not increase solely because wrappers or uncompiled tests were added.
