# Orbit Development 128 — Recoverable Markdown replacement

## Priority rationale (before and after)
M18's highest-priority blocker remains the Markdown-authoritative knowledge cutover and consolidation of `SqliteStore`/`RelayStore`. This iteration addresses an essential prerequisite: replaying interrupted edits without silently replacing conflicting external content. No higher-priority unfinished feature is displaced; full canonical command/SQLite receipt integration remains next and was **not** completed here.

## Implementation
- Extended `KnowledgeOperations` with `update(operation_id, relative_path, expected, body)`.
- Durable `update-<id>.json` journal entry records expected and proposed bytes and hashes. The journal is temporary operational storage, not a note database.
- Recovery will roll forward an interrupted edit when the file matches the expected bytes, accept a previously published replacement, and refuse an unrelated external modification without dropping the intent.
- Legacy create journal records remain readable.
- Added Rust regression cases for interrupted update replay and conflicting external edits.

## Boundaries and known gaps
- Existing SQL-backed `authority_knowledge_command` and `authority_knowledge_update` are **not switched**; Markdown remains not yet authoritative for canonical knowledge commands.
- No SQLite cross-resource receipt/journal transaction; no promise of atomic acceptance across SQLite and filesystem.
- The compare-and-rename primitive does not eliminate races with independent external writers. Canonical operation serialization, filesystem interaction rules, recovery, and executable acceptance tests remain necessary.
- No Flutter work; this belongs to Atomic App after the Rust core is ready.

## Validation
- 17 Python offline synchronization tests: passed.
- 12 Python agent/automation tests (10 subtests): passed.
- Python syntax checks: passed.
- Rust regression tests: added but **not executed** (cargo/rustc unavailable).
- Rust compilation and end-to-end recovery smoke: not verified.

## Progress
M18 = 89% (unchanged); equal-weight overall = 89.5% (unchanged). Estimates only, not verified acceptance.

## Internal testing
**Not ready**: no verified Rust build or end-to-end canonical workflows; outstanding cross-resource consistency issues may cause data loss.

## Next step
Implement canonical knowledge create/update so Markdown is the content source of truth with an SQLite metadata/receipt protocol that can recover interrupted commits. Rework canonical reads/search/link indexing to reflect the accepted Markdown-backed state; retire SQL body ownership rather than maintain parallel stores.
