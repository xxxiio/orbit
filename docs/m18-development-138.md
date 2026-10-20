# Orbit Development 138 — Durable offline Markdown write queue

## Selection and priority
M18's highest-priority unfinished work is the Markdown-authoritative storage cutover and removal of the SQL-body implementation. This iteration advances a key connected-mode integration blocker: remote clients could use the new file-backed command path only synchronously, with no offline retry. It adds a persisted Markdown command queue reusing the server's idempotent acceptance endpoint instead of adding a parallel content store. It **does not** complete the legacy cutover; that remains higher priority than unrelated reliability or release work.

## Delivered
- `AuthorityReplica::queue_markdown`, `markdown_unresolved`, and `flush_markdown` persist complete Markdown command payloads, preserve command identity, and retry with the same command ID after uncertain network outcomes.
- Accepted state changes only after server receipt validation (command ID, note ID, positive revision); transient errors keep commands pending.
- HTTP conflicts remain durable barriers, including the returned reason, until the user explicitly handles the conflict. There is no implicit conflict rebase.
- `ConnectedOrbitClient` and `AuthorityConnection` expose `queue_markdown_command`, `markdown_unresolved`, and `flush_markdown`. Embedded mode accepts synchronously; remote mode reports `Pending` rather than inventing acceptance.
- Remote workspace pending counts include outstanding Markdown commands without projecting them into accepted data.
- Added Rust tests for persistence across a replica reopen, idempotent queueing, conflicting command IDs, and no premature accepted snapshot.

## Remaining limits
- SQL-authoritative legacy knowledge APIs and queue are still present; this is **not** the cutover. Old `knowledge()` and `knowledge_command()` methods still follow the legacy path.
- Markdown queue and earlier domain queue have separate flush methods and orderings. Never use this as a global cross-domain transaction guarantee.
- Offline Markdown document cache, revisioned folder operations, and complete MCP parity remain unfinished.
- Pending commands store Markdown content in the replica outbox for durable retry. This is intentional local operational state, not authoritative note storage.
- Rust toolchain not available; no Rust compilation, executable integration, or crash-injection acceptance has been performed.

## Validation
- Python offline authority tests: 17 passed.
- Python server agent and automation worker tests: 12 passed.
- Python syntax checks: passed.
- Rust regression test added but **not run**.
- Full native embedded/server and offline recovery tests: **unverified**.

## Progress and readiness
M18 retains the prior provisional 89% source-implementation estimate and overall roadmap retains 89.5%; neither is a verified acceptance percentage. This iteration resolves a specific offline transport blocker but not the primary legacy storage cutover. **Internal testing is not ready**: Rust build, integrated core workflows, and no known blocking data-loss issue must be demonstrated first. Flutter remains deferred to Atomic App.

## Highest-priority next work
Complete the unified knowledge storage cutover: retire the SQL-body command/read and outbox routes, rebase knowledge read/search/relations on accepted Markdown-backed state, align MCP and offline client APIs, then run actual Rust acceptance tests including SQLite/file crash points. No higher-priority feature category takes precedence over this M18 work.
