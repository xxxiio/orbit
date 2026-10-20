# Orbit Development 124 — Markdown directory hierarchy foundation

## Priority rationale
The highest-priority unfinished M18 work remains replacement of SQL-backed knowledge bodies and convergence of the two authoritative repositories. The prior implementation in M118–M123 stored knowledge bodies in SQLite despite the requirement that Markdown files be the source of truth. The next safe incremental step is to establish the filesystem vault and folder navigation contract **before** redirecting writes. This should not be interpreted as completed consolidation.

## Implemented
- New `orbit_runtime::knowledge_files::MarkdownVault` filesystem module.
- `KnowledgeNode` folder/note tree, where folders are first-class, including empty folders, and notes are `.md` files.
- Lexically validated relative paths, no traversal, no symlink-following for scans/reads, hidden entries omitted, folder-first deterministic ordering.
- Directory creation primitive and direct Markdown-file read primitive, with Rust unit tests.
- Host-owned knowledge vault next to authoritative SQLite (`database.knowledge/` for `database.sqlite`), with ephemeral vault for in-memory SQLite tests.
- Authenticated `GET /v1/authoritative/knowledge-tree` endpoint.
- Unified native `AuthorityConnection::knowledge_tree()` for embedded and remote modes; remote requires server connectivity.

## Deliberately incomplete
- The **existing** `authority_knowledge_items` SQL table still contains the authoritative body and legacy commands still write it. Markdown files are NOT yet the source of truth for those old APIs.
- The new file vault does not have a durable cross-file/SQLite command journal, revisioned folder mutations, conflict management, local replica cache, or migration logic. Do not route existing note writes to files until crash-recovery behavior is established.
- `create_folder` is a filesystem primitive, not yet exposed as a revisioned public command.
- Flutter UI belongs to Atomic App; this iteration exposes the tree contract, not Dart or Flutter screens.
- Root directory policy and backup of SQLite+Markdown need explicit coordinated design.

## Validation
- Python offline tests: 17 passed.
- Python compileall: passed.
- Rust unit tests authored but not run; no Rust toolchain.
- No live embedded/HTTP/MCP parity verification.

## Next priority
Implement the real Markdown authoritative write protocol with crash recovery and path metadata index; replace SQL body ownership (greenfield-breaking changes are acceptable). Add revisioned folder create/rename/move/delete commands and ensure note paths update consistently. Preserve folder hierarchy in local replicas and the Atomic App navigation contract. Eliminate legacy persistence duplication rather than adding permanent side channels.

Internal-testing readiness: not established; compiled build, core executable workflows and no blocking data-loss issue are required. Flutter remains deferred to Atomic App.
