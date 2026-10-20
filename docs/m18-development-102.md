# Orbit 102 — Native embedded connected-mode facade

## Why this work takes priority

M18 already had an SQLite-backed native authority replica and a connected TUI,
but the embedded/Flutter-facing `orbit-client` interface still presented only
local-authoritative `OrbitClient`. That made it too easy for future GUI bindings
to write directly to local domain storage while ostensibly connected.

## Implementation

`orbit_client::connected::ConnectedOrbitClient` provides a separate, explicit
embedded boundary for server-authoritative mode. `open(replica_db, server_url)`
pins the replica to one server and **does not open `SqliteStore`**. Its
`workspace()` presents the six server-accepted collections and revision from
one cached snapshot, plus an independently calculated pending command count.
It is safe when offline or before the first server snapshot: the collections
are empty and the revision is `None`, not implicitly revision zero.

GUI consumers can queue projects, project-scoped tasks, task transitions,
habits, occurrences, metric targets and metric observations using the server's
existing command action shapes and a caller-generated stable command ID.
`unresolved()` makes conflicts and pending work inspectable; `replace_conflict()`
requires a new command ID and an explicitly reviewed action. `flush()` and
`refresh()` require a separately supplied `AuthorityHttp` with a device token.
Refresh retains native ancestry verification. The type deliberately does not
expose local-only planning, knowledge, or agent mutations as connected writes.

The facade uses simple serializable `ConnectedWorkspace` data so generated
Flutter/desktop FFI adapters can consume one read model without knowing the
replica's SQL schema. **This does not itself generate Dart bindings or integrate
actual Flutter screens.** Neither a device token nor pending edits are included
in `ConnectedWorkspace`.

## Verification and limitations

Added three native source tests: cached accepted state is distinct from all
seven supported queued operations, accepted payloads must contain all six
collections, and a replica cannot switch servers. Rust compilation, FFI
binding generation, end-to-end two-device testing and interactive Flutter
acceptance are still unexecuted.

Next: wire the new embedded facade into the actual Flutter/desktop bridge and
screens, migrate browser/automation writes to canonical services, implement
safe history import and retire built-in Git synchronization, then finish M15
runtime/release checks.
