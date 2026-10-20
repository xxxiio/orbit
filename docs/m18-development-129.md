# Orbit Development 129 — Markdown vault organization operations

## Priority decision
M18 U2 remains the highest-priority unfinished work. Canonical Markdown/SQLite command coordination is still unfinished. This iteration contributes two concrete filesystem operations needed to preserve real folder organization, but **does not** claim to complete the cutover or make these operations authoritative application commands.

## Implementation
- `MarkdownVault::move_markdown(from, to)` moves a note across existing folders without replacing an existing destination and without modifying Markdown content.
- `MarkdownVault::move_folder(from, to)` moves/renames a real folder including its nested Markdown files; it rejects moves into itself or descendants and rejects existing destinations.
- Both APIs reject traversal, hidden paths (including the operations journal), symlinks traversed through existing components, and invalid source types.
- Added a regression test for moving a note, destination collision, moving a folder, and retaining the original document bytes and visible tree.

## Critical limitations
- These are filesystem-level primitives only. The canonical `knowledge_command` and `knowledge_update` still store Markdown bodies in SQL. Neither primitive updates SQLite paths or semantic receipts; they must not be presented as accepted canonical commands.
- The note move consists of `hard_link`, directory sync, then source unlink. An interruption may leave two names; a future durable command journal must reconcile before a command is acknowledged.
- Folder rename is filesystem atomic only on an appropriate local filesystem; cross-resource SQLite coordination, external filesystem races, stable note identity, idempotency and server/replica synchronization remain unfinished.
- On platforms/filesystems with different hard-link or directory-sync support, executable validation is required.

## Validation
- `python scripts/test-authority-offline.py`: 17 tests passed.
- Python compileall: passed.
- Source assertions: passed.
- Rust tests: written, not executed (toolchain unavailable).
- Rust compilation: not run.
- End-to-end canonical Markdown-authoritative cutover and transport parity: not verified.

## Next priority
Prioritize integrating recoverable Markdown writes and SQLite metadata/receipts in one canonical command lifecycle, then adopt note/folder move operations as revisioned canonical commands with path-index reconciliation. Remove SQL-authoritative body storage rather than preserving duplicate ownership. Flutter remains deferred to Atomic App.

## Readiness
Internal testing is not established: there is no successful Rust build and smoke flow, and cross-resource data-loss cases are not yet resolved.
