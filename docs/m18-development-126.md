# Orbit Development 126 — conflict-aware Markdown replacement

## Priority rationale
M18's highest-priority unfinished work remains canonical Markdown ownership and convergence with the rich local repository. M14 core priorities are preserved; no higher-priority unfinished feature category displaces this currently selected M18 work. This iteration adds a filesystem update primitive necessary for a later recoverable canonical command cutover; it does not complete that cutover.

## Changes
- Added `MarkdownVault::replace_markdown(relative, expected, content)` to replace an existing `.md` file through a synchronized staging file and same-directory rename.
- Rejects wrong old content with `VaultError::Conflict`, invalid/symlink paths, and non-Markdown files.
- Rechecks expected content immediately before rename; stages are hidden from navigation and removed on error.
- Added regression tests covering successful replacement, stale content, traversal, no stale staging files and directory navigation.

## Constraints and known gaps
- The comparison plus rename is **not race-free** against external filesystem editors; callers must serialize vault operations and reconcile concurrent external modifications.
- A directory sync error after rename can return an error even though content changed; domain command retry needs durable receipts and recovery.
- This is NOT an SQLite + filesystem transaction; no canonical knowledge SQL-backed command was switched over.
- SQL-backed knowledge creation, update, search, and semantic history remain in place, and OrbitRuntime/SqliteStore remains distinct from AuthoritativeHost/RelayStore.
- Rust compilation and Rust tests were NOT run because cargo/rustc are unavailable. No executable host parity was verified.

## Checks
- `python scripts/test-authority-offline.py`: 17 tests passed.
- `python -m compileall -q scripts`: passed.
- Source assertions: replacement implementation and test present.
- Archive integrity: verified after packaging.

## Next priority
Design and implement a recovery-safe, single authoritative Markdown command protocol and metadata-only SQLite projection, with files as the authoritative bodies. Explicitly handle pending operation journal, revision receipts, restart replay, and external edits. Then remove legacy SQL body ownership and converge the old local feature-rich application services. Do not add compatibility/migration paths for a greenfield application. Flutter UI stays in Atomic App.

## Readiness and progress
M18 remains 89%, overall 89.5%, provisional source implementation estimates. Internal testing is **not ready**: successful Rust build, core executable smoke runs, and a no-known-blocking-data-loss assessment are absent. This work remains highest-priority within M18 at completion; no higher-priority unfinished category was promoted ahead of it.
