# M20 Development 179 — typed domain workflows

## Source baseline and scope

Development 178 was not present among the accessible source archives in this session. This implementation is based on the latest accessible complete source archive, Development 177. The previous conversation describes Development 178 adding workflows, but those source changes could not be inspected or reconciled. This iteration independently implements typed workflows on the inspected 177 baseline and should not be described as a verified incremental change from 178.

## Why this work precedes further hardening

M20's remaining primary blocker is transporting useful daily workflow writes through one embedded/remote authority interface. Ordered typed workflows directly support project/task/evidence user operations in both modes, without broadening the legacy persistence surface. Rich planning, schedule, routine, strategy, and milestone mutations remain unfinished and higher-priority than optional standalone hardening.

## Work delivered

- `DomainWorkflowStep` covers existing project, task and habit/metric evidence command families.
- `AuthorityConnection::execute_workflow` executes sequentially and preserves each embedded accepted receipt or remote pending result.
- Preflight rejects duplicate or blank IDs in one workflow before side effects; network-independent remote outbox continues to persist intents.
- `WorkflowFailure` exposes failing index, completed dispositions, and underlying error, documenting non-atomic semantics.
- Four Rust unit tests cover preflight, embedded order, remote outbox isolation, and accepted-prefix failure.

## Validation and readiness

Python source guards: five passed. Server-side Python tests: twelve passed (plus ten subtests). `cargo` and `rustc` are unavailable; newly added Rust tests were not executed. Two-client behavior and rich-domain writes are not verified. Not ready to declare internal testing.

## Progress

M20: **73% provisional implementation estimate**, unchanged from the previously reported 178 estimate because the 178 source archive could not be reconciled. Acceptance percentage remains unverified.

## Next priority

Bring the actual 178 changes into a unified baseline when available, then deliver rich domain write operations and server dispatch for schedules, routines, project strategies and milestones. Flutter remains out of scope and will be developed in Atomic App. Reassess internal readiness only after a runnable Rust build and smoke workflows, without known blocking data loss.
