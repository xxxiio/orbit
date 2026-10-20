# M18 integrated consolidation checkpoint — 2026-10-13

## Purpose and scope

This is a single integrated **development worktree**, not a claim that M18 is complete. It follows the folder-command checkpoint without issuing separate package versions for each change.

### Implemented together

- Local MCP knowledge operations now invoke the same physical Markdown-backed `AuthoritativeHost` as embedded Rust and authenticated HTTP, including read, search, relations, command, folder creation and folder move. Access requires an explicit unscoped-knowledge grant; mutation additionally requires developer policy.
- Revisioned, replayable empty-folder creation: SQLite intents and idempotent receipts; restart replay before pending note creation; real filesystem folders appear in the app tree. Parent-directory synchronization after mkdir is enabled on Unix.
- Remote offline folder creation and move commands enter the same globally ordered, durable outbox as other commands, retaining conflicts and not confusing queued commands with accepted ones.
- File-backed knowledge create, update and status changes are wired into the legacy `OrbitClient` facade using the canonical host while preserving stable note IDs and user Markdown frontmatter; no new writes into legacy SQLite knowledge-body rows through these facade methods.
- Accepted Markdown commands append activity records in the shared SQLite database transaction, so activity cursors can observe canonical knowledge changes.
- Rust regression scenarios were added for knowledge round-trip, idempotency, explicit policy, folder creation, offline queueing, restart replay and ordering. **They have not been executed.**

### Actual architecture limitations — M18 NOT COMPLETE

1. `OrbitClient` and local MCP still use the feature-rich `SqliteStore` for projects/tasks/habits/planning, while canonical `AuthoritativeHost` still has separate `authority_*` tables for its corresponding domains. Sharing a SQLite file is not application-service consolidation.
2. Direct CLI knowledge subcommands and some advanced resource/term-level knowledge workflows still access legacy knowledge repositories. Canonical Markdown notes do not yet preserve full resource functionality. An upgrade of existing SQL-only knowledge databases is intentionally unsupported; fresh initialization is accepted.
3. Stable folder IDs, authoritative folder deletion, complete offline folder-tree cache, and all client/MCP/CLI/TUI feature parity are unfinished.
4. Full crash/fault injection and executable embedded/HTTP/remote end-to-end verification are unavailable without a working Rust toolchain. Unexpected edits by an external Markdown writer remain a concurrency risk.
5. Neither Rust compilation nor tests have run in this environment. Any Rust source introduced in this worktree is **unverified**, despite passing structural checks.

## Checks performed

- `python scripts/test-authority-offline.py`: 17 tests passed.
- `python -m pytest -q deploy/server/test_agent_worker.py deploy/server/test_automation_worker.py`: 12 tests, 10 subtests passed.
- `python scripts/legacy-contracts/test-m18-markdown-cutover.py`, `python scripts/legacy-contracts/test-m18-no-sql-knowledge.py`, `python scripts/legacy-contracts/test-m18-folder-contract.py`: source checks passed.
- Python compilation and consolidated static/wiring assertions passed.
- `cargo check --workspace --all-targets --locked`, `cargo test --workspace --locked`: **not run**; `cargo`/`rustc` missing.

## Acceptance definition

M18 requires the one authoritative application service per domain, no competing knowledge-content store, both local-authoritative and server-authoritative workflows, real Markdown and folder hierarchy with recoverable writes, consistent embedded/HTTP/MCP/CLI/TUI and offline semantics, successful executable Rust builds and core smoke flows, and no known blocking data-loss defect. Flutter UI remains deferred to Atomic App.

## Progress reporting

The previously quoted 89% M18 and 89.5% overall are **historical, subjective source-development estimates**, not an independently recomputed score. Verified M18 acceptance is *not established*. This worktree represents meaningful integrated source changes (MCP knowledge cutover, folder create, remote offline folder queue, local facade bridge and shared activities), but does not meet the full M18 definition above.
