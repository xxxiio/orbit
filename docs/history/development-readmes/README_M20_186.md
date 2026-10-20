# Orbit M20 — Development 186: authoritative scheduling lifecycle

This iteration replaces the isolated embedded-only schedule preflight wrappers
with a canonical schedule command family. Unlike prior iterations, it implements
schedule state changes, read projections, remote acceptance routing, durable
outbox queuing and regression scenarios together.

## Scope implemented

The `ScheduleCommand` wire contract contains a stable command identity, expected
accepted revision and typed action. Actions include create, reschedule, resize,
rename, retarget and delete. These write to the existing canonical
`schedule_blocks` table using the same command engine that commits task and
project changes. The accepted revision, command receipt, semantic ledger entry,
replay projection and schedule record live in one SQLite transaction.

Scheduling uses half-open intervals. A new or modified block may meet the end
of another block exactly, but may not overlap. Conflict detection occurs while
an immediate SQLite transaction is held. The transaction rolls back on a
collision, missing target, invalid duration, stale revision or duplicate ID.
Semantic edits resolve their predecessor field versions from immutable history
so successive renames, reschedules and retargeting updates do not incorrectly
conflict in replay. Fixed blocks cannot be moved or resized. Targeted blocks must reference an
existing task or habit. Delete removes the live schedule row, while the command
receipt and semantic event remain in immutable history.

The self-hosted server provides `GET /v1/authoritative/schedule`,
`GET /v1/authoritative/schedule-conflicts`, and
`POST /v1/authoritative/schedule-commands`. Full authoritative snapshots now
include `schedule`. Connected replicas validate and cache that collection
separately from pending outbox intentions. A `schedule` command shares the
existing globally ordered queue, immutable command IDs, optimistic revision
fencing and explicit conflict recovery. Embedded and remote clients expose the
same `AuthorityConnection::schedule_command` interface; accepted reads use
`AuthorityConnection::schedule`.

## Completion boundaries

This work does **not** claim that calendar scheduling is completely accepted:

- Rust compiler and Cargo are unavailable in this runtime, so tests are added
  but cannot be compiled or executed here.
- Schedule authority integration with CLI, TUI and MCP remains to be verified
  at executable acceptance level.
- The full M20 milestone still includes routines and project strategy/milestone
  command transports, as well as the two-client smoke workflows.
- Older connected replicas intentionally fail the new required schedule
  snapshot schema and should be freshly initialized (greenfield Orbit policy).

## Executable verification required

Run these commands in an environment with a Rust toolchain:

```sh
cargo fmt --all -- --check
cargo test -p orbit-runtime authority_schedule
cargo test -p orbit-client
cargo test -p orbit-server
cargo test --workspace
```

Additionally verify: use a local and a server-backed client to create two
adjacent blocks, reject an overlap, move a movable block, reject a fixed-block
move, resize a block, retarget to an accepted task, and delete a block. Confirm
that a retried command does not create a second event and that rejected
commands remain unresolved rather than changing accepted read state.

## Progress accounting

This iteration deliberately addresses a substantial missing domain capability.
The changes comprise a new runtime authority module, integration with the
server and unified client interfaces, and real Rust regression cases. No
milestone percentage increase is claimed until compilation and behavioral
verification. The pre-existing 77% M20 estimate remains provisional.
