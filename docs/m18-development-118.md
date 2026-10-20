# Orbit Development 118 — Canonical knowledge persistence slice

## Priority and decision
M18 U2 (consolidation of application services and SQLite ownership) remains
higher priority than further client facades, packaging, or hardening. A full
schema/domain merger remains unfinished. This iteration starts transferring
knowledge persistence capability onto the canonical host-owned store, rather
than implementing an alternative knowledge store in a transport adapter.

## Scope
- New `authority_knowledge_items` canonical SQLite table.
- `KnowledgeCommand`, `KnowledgeRecord`, `KnowledgeResult` are typed Rust contracts.
- New `AuthoritativeHost::knowledge` and `knowledge_command` call the canonical
  store; knowledge creation participates in authority revision history,
  `authority_commands` idempotency receipts, semantic event log, accepted
  replay projection, and a single immediate SQLite transaction.
- Duplicate command ID with different actor or payload rejects; stale
  revision rejects; malformed and excessively large commands reject.
- Added a Rust regression test (not executed in this environment).

## Explicit remaining issues
- Feature-rich `OrbitRuntime` / `SqliteStore` and canonical host store are still
  distinct implementations. Knowledge capability is only a partial port.
- Read endpoint and commands are not yet wired through HTTP/MCP/unified client.
- Knowledge metadata, relations, resources, updates/deletes, search, scheduling,
  and other existing local-domain services need unified application interfaces.
- No Rust compilation or runtime parity check has been performed.
- Internal testing is not ready: runnable Rust build, accepted smoke workflows,
  and a no-known-data-loss-blocker assessment are mandatory.
- Flutter stays in Atomic App, not M18 acceptance.

## Iteration completion priority check
U2 is still the highest-priority unfinished category; next work should
consolidate the existing local repository implementations rather than expand
one-off canonical schemas without a shared application-service boundary.
