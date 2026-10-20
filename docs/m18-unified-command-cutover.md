# M18 unified client command contract — consolidation worktree

## Delivered in this worktree

The public `AuthorityConnection` operations for Markdown edits and folder create/move now have the **same asynchronous acceptance semantics** as project/task/evidence operations:

- `Embedded`: invoke the shared `AuthoritativeHost` and return an accepted disposition after durable local acceptance.
- `Remote`: persist the command in the **single global SQLite outbox**, return `Pending`, then use `flush()` to submit in ordinal order and process receipts.
- Calls with an already-configured remote HTTP transport also queue instead of bypassing prior pending work, preventing out-of-order file operations.
- File-command methods previously had a separate synchronous HTTP branch which bypassed outbox ordering; that branch is removed.
- Existing `queue_*` method names delegate to those same operations, so they cannot develop separate business semantics.
- Local Markdown acceptance uses `AcceptedMarkdown { revision }`, since the Markdown receipt does not record whether it was an idempotent replay. Do not invent a duplicate flag.
- Remote domain command flush now checks the accepted revision and receipt shape before changing outbox state from `pending` to `accepted`.

## Still incomplete — do not claim M18 acceptance

- `RelayStore` continues to contain earlier relay/event and canonical projection machinery. A single owned `SqliteStore` exists but legacy service semantics are not fully removed.
- Full rich-feature local/remote service parity has not been verified. Client API refactoring is *not* equivalent to consolidating the entire application layer.
- Rust compilation, live HTTP integration, and crash/recovery acceptance have not been executed in this environment (no cargo/rustc).
- Existing cache and outbox code is not a substitute for an executable offline server synchronization acceptance suite.

## Minimum executable validation before completion

Build and test the Rust workspace, run embedded/server parity for projects/tasks/habits/metrics/Markdown and folder operations, simulate lost receipts and conflicting edits while disconnected, test SQLite/Markdown crash recovery, and verify that planning/project-mode workflows remain available through both connection types. Do not mark M18 complete until these pass and no blocking data-loss defect remains.
