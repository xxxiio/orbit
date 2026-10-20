# Orbit 142 — retire the SQL-authoritative canonical knowledge model

This is an **architectural removal**, not M18 completion.

- Removed `authority_knowledge_items` table creation and all four `RelayStore::authority_knowledge*` legacy methods.
- Removed the matching `AuthoritativeHost` methods and obsolete SQL-body knowledge command contracts and unit tests.
- Removed the legacy SQL-body relationship module; canonical file-backed relationships remain served by `KnowledgeService`.
- Opening a database with an existing `authority_knowledge_items` table now fails closed as `MigrationConflict`. This is intentionally breaking under Orbit's greenfield policy. No migration/import or silent deletion is attempted.
- Added source contract guard and Rust tests for fresh schema and rejecting a retired body table.

## Validation

- Python worker tests: 12 passed.
- Offline authority regression tests: 17 passed.
- Source API cutover and SQL-retirement guards: passed.
- Rust toolchain missing; Rust compilation and Rust regression execution **not performed**.

## Significant unfinished M18 work

The separate `orbit-storage::SqliteStore` remains authoritative for the feature-rich older `OrbitClient`, CLI/TUI and local stdio MCP flows, and differs from `AuthoritativeHost`. Eliminating that duplicate implementation requires moving its planning/knowledge/advanced project application workflows behind common services. File-backed folder commands and MCP/client parity are also incomplete. Therefore M18 and internal-testing acceptance cannot be declared complete.

Flutter remains owned by Atomic App.
