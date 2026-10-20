# Orbit Development 132 — Markdown identity and host-owned knowledge service

## Priority rationale
M18 U2 remains highest priority. Canonical knowledge writes still store Markdown bodies in SQLite; the new filesystem service is not an accepted command implementation. This iteration establishes stable file-owned identities and a domain-level discovery boundary before the irreversible cutover. No higher-priority unfinished M18 category precedes knowledge storage consolidation. Flutter remains outside this project.

## Implementation
- New `knowledge_service::KnowledgeService` using existing `KnowledgeOperations` and physical `MarkdownVault`.
- Stable `orbit_id` frontmatter field, independently retained across file moves.
- Folder-tree discovery returns stable ID, physical path, and derived title; rejects missing, invalid, or duplicated identities instead of arbitrarily selecting a file.
- `stage_create` uses the existing recoverable filesystem publication path but deliberately does not claim a SQLite-accepted command.
- `AuthoritativeHost::markdown_knowledge_index` exposes this domain read path to host-side Rust callers.
- Rust unit coverage for identity stability, collisions, interrupted-move duplicate paths, and missing identity.

## Not delivered
- Existing SQL-backed knowledge create/update/list/search remain active; this is not yet a Markdown-authoritative cutover.
- File journal and SQLite acceptance/receipts are not coordinated; staged creation must not be interpreted as acceptance.
- No SQLite path index, revisioned move/folder commands, HTTP/MCP parity, or browser/offline Markdown cache.
- External editors can modify vault files; robust concurrent change reconciliation is not yet implemented.

## Validation
- Offline authority Python suite: 17 tests passed.
- Python syntax checks: passed.
- Static source assertions: passed.
- Rust compilation/tests: not run; Rust toolchain unavailable.
- Cross-resource crash tests: not run.

## Next priority
Implement a single transaction-aware canonical knowledge service with durable SQLite intents and idempotent receipts, published Markdown content, and startup recovery, then switch and remove SQL-body authoritative methods. File-based IDs must be the stable source and filesystem paths should remain an index. Acknowledge commands only once durable accepted state is recoverable.

## Internal testing
NOT READY. No compiled Rust build or executable core smoke flows, and cross-resource data loss remains unverified.
