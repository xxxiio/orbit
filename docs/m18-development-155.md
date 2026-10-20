# Orbit Core Development 155 — Shared rich command acceptance

## Scope

This is an M18 unified runtime iteration based on Orbit Core 154. It does not close
M18: there is no Rust toolchain in the execution environment, and additional
rich workflow methods still write without canonical acceptance envelopes.

## Production implementation

1. `SqliteStore::transact` uses a SQLite savepoint, permitting nested rich
   application use cases inside an outer `BEGIN IMMEDIATE` transaction without
   prematurely committing authority history. Local-only use cases continue to
   commit when their outermost savepoint is released.
2. `AuthoritativeHost::transact_application` coordinates batches of related
   rich operations and rolls back the entire batch on any application error.
3. `PendingCommand` now accepts a borrowed SQLite connection, so canonical
   admission and history publication can surround calls to the mutable rich
   application service on the exact same database connection.
4. `AuthoritativeHost::execute_application_command` applies revision checks,
   immutable retry receipts, a rich use case, a semantic event, replay, and the
   final commit within one transaction. This low-level primitive is `pub(crate)`
   so callers cannot publish arbitrary semantics inconsistent with their writes.
5. The typed accepted-command methods are `create_project_accepted`,
   `transition_project_accepted`, `create_task_accepted`,
   `start_task_accepted`, `transition_task_accepted`,
   `create_habit_accepted`, and `create_routine_accepted`.
6. Project and task transitions read the most recent semantic field version
   from committed history and supply that as the optimistic mutation base.
7. The CLI now uses typed accepted command methods for project/task create
   and lifecycle transitions and for habit/routine creation. Embedded projects,
   tasks, habits and routines still share a single `SqliteStore` owner.

## Verification implemented

- SQLite transactional Rust tests cover standalone commit, rollback, nested
  operations, preserving the outer transaction, and isolating failed savepoints.
- Host Rust tests cover multi-operation batch commit/rollback and canonical
  receipt acceptance coupled to project/activity persistence.
- Typed rich-command Rust tests cover mixed project/task/habit/routine command
  ordering, retry safety, stale revision rejection, command-ID collisions,
  invalid domain transitions, field-versioned repeated status changes, and
  interaction with the existing minimal canonical commands.
- Source regression guard `scripts/legacy-contracts/test-m18-transaction-bridge.py` checks the
  command/transaction boundary and the CLI adoption of typed commands.
- All available non-binary M18 Python regression scripts, the 17 offline
  synchronization tests, and the Python worker tests passed in this iteration.

## Acceptance boundaries — still open

- Other rich writes (project strategy, planning, metric observations/targets,
  habit occurrences, routine membership, scheduling, evidence updates) still
  have standalone application-service entry points. They must be integrated
  into the same accepted command contract or explicitly distinguished as
  local-only actions with appropriate revision behavior.
- Knowledge and folder durability belong to M19; HTTP/MCP transport parity
  belongs to M20 and remote offline convergence to M21.
- The canonical projector still uses the semantic envelope abstraction inherited
  from the old peer-sync model. A complete M18 cutover must verify its semantics
  for all rich fields and prevent divergence after unaccepted local mutations.
- No `cargo`, `rustc`, or `rustfmt` executable is available in this environment.
  These Rust changes have **not been compiled or executed**. Do not claim a
  successful Rust build, passing Rust tests, internal testing readiness, or M18
  acceptance from source-level checks.

## Progress interpretation

M18 provisional source implementation estimate: 92% (previous 88%). The
increase represents a new canonical rich application transaction seam and typed
CLI lifecycle adoption. Verified runtime acceptance remains **unestablished**.
M19: 75%; M20: 65%; M21: 65%; M22: 10% — unchanged estimates.

## Priority decision

M18 remains the highest-priority unfinished architectural milestone. The next
iteration should integrate the remaining rich mutation families into accepted
commands, eliminate residual bypasses, and execute the Rust acceptance suite as
soon as a toolchain can be provisioned. Do not substitute more source-only
checks for runnable acceptance or split remaining domains into tiny releases.
