# M18 integrated worktree — NOT M18-complete

This is a **single unnumbered worktree** based on Orbit 142. It is deliberately
not packaged as a sequence of micro-releases. It brings together multiple
connected knowledge and remote-client capabilities.

## Integrated source changes

1. Canonical `MarkdownCommand::Move {note_id, from, to, expected}` uses the
   existing authoritative durable SQLite intent, Markdown publication, global
   revision, metadata-index reconciliation, and idempotent receipt lifecycle.
   The accepted note ID is stable despite its folder-path change.
2. Crash replay handles the gap between hard-link publication and source unlink.
   When both paths exist, only hard links to the **same inode** can be safely
   reconciled on Unix. Conflicting external modifications fail closed. On
   non-Unix hosts this ambiguous case remains a known recovery limitation.
3. Creation, edits, and moves validate note IDs, relative paths, and Markdown
   frontmatter before queuing unreplayable intents. Acceptance verifies exact
   committed Markdown bytes and the expected destination path.
4. Remote offline operations now share **one globally ordered SQLite outbox**
   across project, task, evidence, and Markdown commands. Conflicts stop the
   entire stream; no separate Markdown queue can skip older commands.
   The retired SQL-backed knowledge command kinds and their HTTP endpoints
   are removed from the client-facing queue. Pre-cutover replica schemas fail
   closed instead of applying unrequested migration compatibility logic.
5. Authenticated server reads can be explicitly copied into a persistent
   accepted-Markdown cache. `cached_markdown_by_id()` is explicitly stale-capable;
   `refresh_markdown_by_id()` fetches and stores a fresh authoritative copy.
   Pending edits do not appear in the accepted-data cache.
6. Added Rust regression scenarios for accepted moves, crash replay, globally
   ordered domain/knowledge commands, and persisted accepted-data caching.
7. Added `scripts/legacy-contracts/m18-acceptance.sh` and a Linux CI workflow to execute
   workspace compilation/tests and Python source-contract checks when a Rust
   toolchain is available.

## Validation evidence

- Python offline synchronization suite: 17 passed.
- Markdown API cutover/source contract checks: passed.
- Retired canonical SQL-body source guard: passed.
- Python syntax checks: passed.
- Rust compilation: **NOT RUN** (no `rustc`/`cargo` available).
- New Rust regression tests: **NOT RUN**.

## Milestone blockers — no false completion claim

The older feature-rich local `SqliteStore`, including CLI/TUI/local-MCP access,
remains independent of `AuthoritativeHost`. Those services must be consolidated
without losing project-mode, planning, milestone, habit, and knowledge features.
The filesystem-based folder tree still needs complete revisioned folder CRUD
and stable folder identities. MCP parity, all-platform behavior, and integrated
external-file race/crash safety are not verified. The worktree is NOT ready for
M18 acceptance or internal testing.

Flutter remains deferred to the separate Atomic App project.
