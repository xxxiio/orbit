# M20 Development 198 — Shared atomic remote workflow staging

## Implemented

The canonical `AuthorityConnection::execute_workflow()` now stages every remote
cross-domain workflow in **one SQLite transaction** using the same durable
`AuthorityReplica::queue_batch()` employed by the TUI. Previously it queued
commands one by one, risking unexpected partial local staging when a later
command ID conflicted. This fixes the divergence between connected TUI, desktop
MCP (`authority.workflow`), and other unified-client workflow consumers.

`ConnectedOrbitClient::queue_workflow()` exposes the existing replica-level
atomic batch operation to the common interface. The typed workflow is serialized
by domain family into the same action format as individual queue methods:
project fields are unwrapped; task/evidence/schedule/routine/milestone/strategy
serialize the action only, with the command envelope supplied during flushing.

The entire batch is checked for duplicate IDs and conflicts with previously
queued intentions before commit. Stable-id retries remain idempotent. Accepted
projections do not include pending staged actions. Remote **execution remains
ordered and non-atomic**: once submitted, the server may accept a prefix before
rejecting a later step. Local atomic staging does not imply atomic server execution.

Regression scenarios added for mixed project/task batch serialization, whole-batch
rollback on a late existing-ID collision, repeated retry idempotency, and duplicate
ID preflight. They are not compiled/executed in this implementation iteration.

## Limitations and handoff

M20 must **not** be reported 100% on this source change alone. The remaining
implementation audit must establish that no required connected-mode client
mutation paths bypass the authoritative runtime and that all required CLI/TUI/MCP
commands are present. Broad Rust compilation and release testing are owned by M15;
no runtime verification is claimed here.
