# Orbit unified-runtime roadmap (historical plan)

> **Superseded for M20 completion status.** This file documents historical
> decisions and in-progress steps from earlier developments. The implemented
> runtime boundary and M20 implementation-closure inventory are recorded in
> [M20 implementation closure](m20-implementation-closure.md). Broader
> compilation and executable acceptance remain in M15.


## Architecture contract

One host-side `OrbitRuntime` owns the authoritative SQLite connection and executes
application/domain services. It has no local-vs-server mode switch and does not
select a network transport. A local embedded host and a remote self-hosted host
initialize the **same** runtime with a database path. Different transport and
security adapters call this runtime; clients do not open authoritative SQLite.

- Local UI/CLI/TUI: embedded typed requests or optional local IPC/loopback API.
- Remote UI/CLI/TUI/browser: HTTPS requests; offline capable native cache/outbox.
- Local AI: MCP stdio (default) or optional loopback with suitable permissions.
- Remote AI: authenticated HTTP MCP.
- Domain rules, knowledge/planning, commands and repositories: one implementation.
- SQLite: authoritative on the machine hosting the runtime, never a remote DB.
- Remote offline cache/outbox: non-authoritative and owned by client sync.
- Flutter/Dart UI: later, in Atomic App, not a release blocker for Orbit.
- No legacy Git, relay compatibility, migration tools or database importers.

## Revised sequence and acceptance gates

### U1: Host/runtime foundation — **started in 112**
Establish `orbit-runtime` as the owner of local authoritative SQLite and the
composition point for shared `orbit-application` use cases. Bridge existing
embedded client initialization through runtime temporarily. Add runtime-level
persistence and domain tests. Gate: compile and run runtime/embedded smoke.

### U2: Eliminate parallel server domain implementation — **highest priority next**
Move canonical server project/task/habit/metric operations behind runtime-owned
application services and transaction-aware repositories. Preserve revision,
idempotency, semantic history and snapshot continuity as runtime infrastructure;
do not duplicate domain business rules in REST/MCP handlers. Prefer clean schema
initialization; no migrations for previous development snapshots. Gate: identical
business operations yield equivalent domain state in local and remote hosts.

### U3: Adapter unification — **after U2**
Introduce request/response transport boundaries around the runtime. REST and MCP
call the same application service entry points. Local MCP works via stdio, and
remote MCP via authenticated HTTP. CLI/TUI operate locally or remotely via the
same client-facing capability contract. Gate: test parity for create/read/update
through embedded, REST and MCP adapters.

### U4: Offline and agent/automation integration — **after U3**
Retain remote client's SQLite accepted cache, outbox and explicit conflict
reconciliation; all queued intents are accepted only by host runtime.
Both local and remotely coordinated agents/automation use the same services.
Gate: two-client convergence, interruption/retry/conflict and receipt tests.

### U5: M15 executable acceptance — **after integration**
Run cargo build/test/clippy, package dev/server binaries, exercise local and
remote CLI/TUI plus both MCP transports, browser, agent/automation smoke and
data consistency. Require runnable build, successful core smoke workflows and no
known blocking data-loss issue before internal-testing-ready declaration.

## Development 112 implementation boundary

This first slice adds the host-side runtime crate and routes legacy embedded
client SQLite opening through it. The embedded client's existing methods still
hold a transitional `SqliteStore`; they are NOT yet transport-only, and the
server still uses its legacy-named independent authoritative store. Do not
claim runtime unification or feature parity until U2/U3 are implemented.

## Progress accounting

M18 scope has expanded from server-authoritative connectivity to *one unified
host runtime and common client adapters*. Previous 99% estimate is invalid
under this stronger architectural contract. Rebaseline M18 at 75% source
implementation pending U2-U4; M15 remains 8% until executable acceptance.
M7 is historical, retired work, not a supported Git feature.

## Iteration 113 — host-side authority extraction

The existing authoritative SQLite transaction engine, semantic projection and project/task/habit/metric command contracts now belong to `orbit-runtime`. The server executable imports these as modules, instead of defining its own persistence crate-local implementation. MCP and REST use the same `RelayStore` instance and transaction path. This is a structural extraction, **not yet** a replacement of the local `SqliteStore` with the canonical authority store.

Next: migrate embedded local commands to the authoritative runtime repository without losing the richer existing local planning/knowledge behavior; remove the transitional `into_legacy_store`; then unify embedded and remote request adapters. New installs only; no data migration or Git support. Flutter is a later Atomic App task.

## Development 114: server host composition

The server now constructs `orbit_runtime::AuthoritativeHost`, not a direct
`RelayStore`. HTTP and MCP canonical reads/writes call explicit host methods
that delegate to the existing revision-checked, transactional authoritative
implementation. Device administration/backup remain reachable through a
transitional management bridge until runtime ports are fully extracted.

**Not yet unified:** `OrbitRuntime` (embedded) still owns `SqliteStore`, while
`AuthoritativeHost` owns the canonical authority schema; they do not yet use
identical repositories or offer feature parity. U2 therefore remains open.
The following step must consolidate the schemas and application use cases,
not introduce more superficial adapter wrappers. No Flutter work is required.

## Development 115 — embedded canonical host transport

Added `orbit-client::EmbeddedAuthorityClient` as an in-process adapter to the
**same** `AuthoritativeHost` and canonical SQLite schema used by remote REST/MCP.
It exposes canonical projects, tasks, habit/metric evidence, snapshots and
history-prefix reads, with revision-checked and idempotent commands. Its trusted
local actor identity is bound once at construction. No HTTP or local replica
is needed for this embedded path. The facade does not open SQL directly; its
owned host does.

This is an integration stepping stone, **not** U2 completion: the richer old
`OrbitClient` still uses `orbit-storage::SqliteStore` and offers planning,
knowledge and other operations not yet moved to the canonical host. The next
iteration must port those application services onto the canonical persistence
boundary and delete the transitional local store usage. Avoid declaring parity
until both embedded and remote adapters exercise the same supported features.

Validation: two new Rust source tests target canonical project/task commands,
idempotency, stale revision handling and history-prefix parity. Cargo and rustc
are not available in the authoring environment, so these tests are unexecuted.

## Development 116 — authority boundary tightening

Removed the public `Deref<Target = RelayStore>` escape hatch from `AuthoritativeHost`.
The HTTP server now calls explicit runtime methods for device authentication, enrollment,
revocation, token rotation, backup, workspace snapshots and canonical commands.
This keeps database ownership inside the host runtime. These changes do **not**
solve the larger U2 issue: the feature-rich local `SqliteStore` remains distinct
from the canonical authoritative persistence implementation. The next feature
iteration must address that duplication directly, without adding more facades.

## Development 117 — canonical client routing and authenticated remote transport

`orbit-client::unified::AuthorityConnection` now presents the same supported
project/task/evidence command contracts for both canonical host connections:

- `AuthorityConnection::embedded(path, actor)` executes commands immediately in
  `AuthoritativeHost` through `EmbeddedAuthorityClient`. Receipts are accepted.
- `AuthorityConnection::remote(replica_db, server, identity)` retains accepted
  snapshots and durable pending intentions in the replica. It submits through
  HTTPS (or loopback HTTP) using a registered device ID **and** bearer token.
  Pending receipts are never represented as accepted writes. `refresh` and
  `flush` are explicit operations.
- The interface is Rust-only: Atomic App will consume it later through FFI.
  There is no Flutter/Dart source or Flutter-specific dependency in Orbit.
- CLI and TUI native remote transport now supply the server-required
  `x-orbit-device-id` header. Offline reads and queueing need no credentials.

**Architecture gate still open:** `OrbitRuntime`/`SqliteStore` is distinct from
`AuthoritativeHost`/canonical SQLite. The new connection API gives the future
Atomic App both in-process and HTTP routes but does NOT solve complete feature
parity, planning/knowledge persistence, or local stdio MCP. Those remain U2/U3
priorities; avoid hiding this behind client interface terminology.

Validation is source-level only. The new Rust tests check accepted-vs-pending
semantics and reject absent or malformed remote device IDs. Cargo is not
available in the current environment; compiled parity and live HTTP tests
remain mandatory before internal-testing readiness.
