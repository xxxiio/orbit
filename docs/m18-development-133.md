# Orbit Development 133 — Derived SQLite Markdown identity index

## Priority rationale
M18 U2 remains the highest-priority unfinished work: convergence on filesystem-authoritative Markdown with one canonical Knowledge Service. The major remaining barrier is coordinated, recoverable SQLite acceptance and filesystem publication. This iteration adds only a replaceable metadata projection, not another body store or parallel authority. No higher-priority incomplete M18 work should be displaced by UI or independent adapter extensions. Flutter remains deferred to Atomic App.

## Implementation
- Added `markdown_note_index` (stable note ID, physical path, derived title only) to the canonical SQLite schema.
- Added transactional `RelayStore::reconcile_markdown_note_index` that validates all IDs and paths before changing any rows and atomically replaces the derived projection.
- Added `AuthoritativeHost::reconcile_markdown_knowledge_index` that first completes Markdown-vault discovery, rejecting duplicate or missing `orbit_id`, before updating SQLite.
- Added `AuthoritativeHost::indexed_markdown_knowledge` for inspecting the previously reconciled projection. It is explicitly non-authoritative and can be stale until reconciliation.
- New Rust regression test covers moving a Markdown file, index reconciliation, duplicate identity rejection without clobbering previous index, and absence of body column.

## Important limitations
- This is not the canonical SQL/Markdown write-path cutover. Existing SQL-backed knowledge commands and their legacy body table still exist.
- No durable SQLite operation intent, accepted command receipt, cross-resource recovery, or revisioned folder moves were added.
- Reconciliation is explicit, not automatically triggered on every read or external filesystem edit. A crash during discovery/reconciliation leaves a stale but intact metadata projection; files remain authoritative.
- The same local legacy versus host persistence split remains unresolved.

## Validation
- Rust compilation/tests were not executable because cargo and rustc are unavailable.
- Python offline tests and static source checks are recorded separately for this iteration.
- Internal-testing readiness remains **not established**: runnable Rust build, core smoke tests, and no known blocking data-loss issue are required.

## Next priority
Implement the integrated canonical knowledge write/acceptance protocol (SQLite durable intent + filesystem publication + SQLite receipt), recovery/replay and file-backed read cutover before more transport/UI work. Verify on a runnable Rust toolchain before treating source progress as acceptance.
