# M20 Development 201 — authority path consistency and closure audit

## Code delivered

- Unified single-command and multi-command admission constraints at the SQLite replica boundary. All CLI, TUI, and MCP remote submission paths reuse these checks.
- Reject noncanonical identifiers, embedded command envelopes, empty actions, and non-object actions before touching durable outbox state.
- Prevent transmission from a replica with a known-invalid audit result. The audit is read-only: it never silently repairs authoritative evidence.
- Added focused regression cases covering identical single/batch admission, entire-batch rollback on invalid later intent, successful audit, and detection of tampered action data.

## Architectural audit and remaining scope

- Local-only runtime retains its own authoritative SQLite schema.
- Connected mode retains accepted snapshots and one globally ordered durable outbox.
- Project, task, evidence, schedule, routine, milestone, strategy, Markdown, and folder operations are carried by the canonical connected outbox; server acceptance remains authoritative.
- CLI, TUI and desktop MCP expose connected-mode operations, with explicit pending/accepted distinctions.
- This source audit has **not** exhaustively proven the absence of all legacy production write paths. A comprehensive executable workflow matrix and Rust compilation are also unverified.

## Milestone decision

M20 implementation closure **not certified**. The previous 95% is a provisional estimate, not an acceptance calculation. Do not mark 100% until a definitive implementation inventory establishes that each remaining production mutation entrypoint uses the canonical authority boundary and its obsolete branches are retired. Broad executable release validation remains under M15; that does not excuse missing implementation evidence in M20.
