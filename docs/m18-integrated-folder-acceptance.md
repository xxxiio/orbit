# M18 integrated development checkpoint — 2026-10-13

## Scope delivered in this checkpoint

- Added durable SQLite intent/receipt handling for physical Markdown-vault folder renames.
- Added replay on host startup, including interrupted renames already visible on disk.
- Reconciles descendant Markdown note paths in the same SQLite transaction as accepted folder command receipts.
- Rejects duplicate command IDs with different payloads, folder traversal, symlink destinations, conflicting targets, and interleaving with pending Markdown commands.
- Uses the *same host command* through embedded Rust and authenticated self-hosted HTTP; a unified Rust client exposes both, and remote offline invocation fails explicitly rather than claiming acceptance.
- Added Rust regression tests (not executed here) and dependency-free source contract checks.

## Acceptance limitations

This does **not** complete M18. The legacy `OrbitRuntime` / feature-rich `SqliteStore` and its CLI/TUI/MCP users are not consolidated with `AuthoritativeHost`. Folder creation/deletion and remote offline folder outbox remain incomplete. A folder operation interrupted after an external filesystem intervention fails closed; arbitrary external writers cannot be atomically coordinated with SQLite. Rust compiler and Cargo are unavailable, so even the new folder path is uncompiled and untested in Rust.

## Checks executed

- `python scripts/test-authority-offline.py`: 17 passing tests.
- `python -m pytest -q`: 12 tests and 10 subtests passing.
- `python scripts/legacy-contracts/test-m18-markdown-cutover.py`: passed.
- `python scripts/legacy-contracts/test-m18-no-sql-knowledge.py`: passed.
- `python scripts/legacy-contracts/test-m18-folder-contract.py`: passed (structural check only).
- `python -m compileall -q scripts`: passed.
- Rust compile and executable runtime parity: NOT RUN.

## Highest-priority follow-through

1. Consolidate the actual application repositories and workflows behind `AuthoritativeHost`, preserving project modes, planning, progress, habits, metrics and knowledge.
2. Migrate remaining local MCP/CLI/TUI paths to this shared runtime, and complete folder commands and offline client parity.
3. Compile and execute the Rust workspace, fix failures and verify crash-recovery/fail-closed behavior before any M18 completion claim.
4. Flutter remains deferred to Atomic App and is not an M18 blocker.

## Progress reporting

The old 89% M18 and 89.5% roadmap figures are **historical, subjective source-progress estimates**. They are not verified completion, and cannot meaningfully be incremented based on this work alone. Current verified M18 acceptance: **not established**. Internal-testing readiness: **not established**.
