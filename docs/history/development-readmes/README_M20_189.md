# M20 development 189: Project strategy authority integration

## Why this work takes priority

The canonical authoritative runtime already supports schedules, routines and
milestones, but project strategies still use a separate application write path.
This is a direct functional gap for connected users: remote clients cannot
configure project objectives and success criteria through the authority server.
The selected work completes the strategy write/read transport before working on
standalone integrity hardening or release preparation.

## Implemented workflow

- Revision-fenced `StrategyCommand` with configured project modes, objectives,
  progress signals and an ordered set of success criteria.
- Criterion satisfaction/waiver transitions without resetting the rest of the
  project strategy.
- Atomic SQL updates and canonical semantic history, in the same transaction
  as command receipts.
- Full project-strategy snapshots for local clients and server-connected
  replicas, with pending changes explicitly separated from accepted state.
- Remote outbox dispatch for strategy commands using immutable command IDs.
- Receipt identity verification on connected clients.
- Integration into `AuthorityConnection`, the embedded runtime, server routes,
  and authoritative accepted snapshots.
- Rust regression tests for the lifecycle, revision fencing, idempotence,
  malformed requests, bounded criteria and signals, and remote pending state.

## Invariants

1. An existing project is required. Invalid references cannot create orphan
   strategies or revision events.
2. Command IDs are immutable: retries return the committed receipt but reusing
   an ID for different contents is rejected.
3. The authoritative project fields and ordered criterion records are updated
   in the same SQLite `IMMEDIATE` transaction as the immutable revision receipt.
4. Remote intents enter the durable outbox but never become accepted reads
   until server acceptance and snapshot refresh.
5. A strategy is not a new peer-to-peer authority. It is a revisioned update
   to a project, using the existing project semantic entity kind.
6. Greenfield outbox upgrades intentionally fail closed on incompatible old
   replicas; no legacy migration path is added.

## Acceptance and remaining risks

Rust toolchain is unavailable in this execution environment. Rust code and
new tests have not been compilation-verified. Server / two-client integration
still needs an executable smoke run. CLI, TUI and MCP user commands need
separate audit and integration before M20 closure. This is a source-level
implementation milestone, **not** final acceptance.

## Next M20 priority

Run the Rust workspace build, resolve compiler and behavior failures, exercise
real server/replica command dispatch, and then complete the CLI/TUI/MCP
integration. Treat M20 as unfinished until executable tests pass.

## Internal test readiness

Not verified: there is no successful Rust build or executable end-to-end smoke
result and therefore no reliable absence-of-data-loss determination. Flutter
integration remains deferred to the separate Atomic App project.
