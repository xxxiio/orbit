# Orbit 168 — M19 SQL knowledge authority retirement

## Implementation

- SQLite schema v13 no longer creates the retired `knowledge_items`, `knowledge_relations`, `knowledge_resources`, `knowledge_resource_local_locators`, or `knowledge_fts` structures in new workspaces. This is a **greenfield breaking change**, not a data migration.
- Opening an existing database carrying legacy SQL knowledge objects now rejects it before migration, rather than silently dropping possibly important content. A fresh database and explicit Markdown import are required. Existing files are not rewritten on rejection.
- Legacy storage repository interfaces remain implemented only to satisfy application trait bounds, but all SQL knowledge/resource reads and writes now fail with an explicit retired-storage error. No parallel SQL content authority is created.
- Semantic inbound materialization for legacy knowledge and resource envelopes returns **deferred** instead of materializing into SQL or acknowledging field versions. File-backed knowledge replication remains M21 scope.
- The TUI term-navigation read path now resolves physical Markdown documents via `AuthoritativeHost::markdown_term_locations`; the SQL-backed query endpoint has been removed.
- A small Rust regression module covers fresh schema shape, legacy adapter rejection, and failure without content loss when a legacy database is opened. A Python source-contract guard accompanies it.

## Known boundaries and remaining implementation

The application crate still exports legacy knowledge use cases and repository traits for existing type composition and obsolete unit tests. These are not supported as persistence-backed runtime paths. Old Rust tests that expect the SQL-backed knowledge model, FTS, or inbound SQL replay must be replaced with Markdown-backed tests; Rust compilation and full acceptance remain unverified and assigned to M15 per the roadmap. The new term-reference implementation reports lowercased text matches and is not yet equivalent to the old token-boundary-aware Unicode matcher; refine in M19 follow-up if needed. Note/file publishing, import, export, and asset storage remain as implemented in prior iterations.

## Verification performed

- Seven M19 Python source/regression scripts: passed.
- M18 SQL-knowledge and Markdown-cutover source guards: passed.
- Offline authority Python regression suite: passed (17 cases).
- Python agent and automation workers: 12 passed, 10 subtests.
- Rust compilation and new Rust tests: **not run** (no Rust toolchain; M15).

The source-level greenfield schema contract is a deliberate breaking change and should be compiled and exercised before production use.
