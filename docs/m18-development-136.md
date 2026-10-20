# Orbit Development 136 — Coordinated Markdown command acceptance (host-side)

## Why this work comes first
The highest-priority unfinished M18 category is U2: consolidate authoritative knowledge persistence rather than adding filesystem helpers, transport facades, or release hardening. The prior `stage_markdown_*` calls updated files and reconciled a disposable index but did not create accepted receipts. This iteration closes the host-side create/edit acceptance and replay gap. No higher-priority unfinished category should precede it.

## Delivered
- Added `markdown_commands.rs` with a distinct durable SQLite command log (`pending`/`accepted`), version table, immutable command identity validation, payload comparison, and accepted receipts.
- `AuthoritativeHost::execute_markdown_command` coordinates durable intent, file publication, index reconciliation, revision and receipt acceptance. Markdown bodies are not stored in the new SQLite command projection, except temporarily as **pending command payloads needed to recover interrupted publication**. The accepted command log retains the payload for idempotency, so it currently still contains a copy of submitted Markdown text. This is operational history, *not* the authoritative content source and requires later retention/privacy policy work.
- `AuthoritativeHost::open` resumes pending operations before opening the runtime to new commands. Already-published matching file content is recognized during replay; divergent external edits fail closed. A failed or ambiguous recovery prevents the host from opening.
- New regression tests cover create/edit, stale or conflicting command IDs, repeatable accepted receipts, reopen, and crash after publication before SQLite acceptance.
- SQLite revisions and the derived note index are committed in the same transaction as each accepted receipt.

## Limits — do not infer cutover or release readiness
- This is a new **host-side canonical file-backed command path**, not a replacement of the old `knowledge_command` and `knowledge_update` public paths. Legacy SQL-body operations remain present, so a unified content store is not yet achieved.
- The existing journal and Markdown publication safeguards protect many interruptions, but filesystem/SQLite concurrency, external writers, safe rollback and all crash points have **not** been fully tested. A pending command may deliberately block startup if the file no longer matches expected data.
- Existing SQL metadata still includes `authority_knowledge_items.body` in legacy code. The new `markdown_command_log.payload` also retains command contents; this is not a purely metadata-only SQLite schema.
- Host call-site is not yet exposed via embedded/HTTP/MCP unified clients. Folder commands, offline replicas and removal of old SQL-body authority remain unfinished.
- Rust compiler unavailable; Rust code is **not compiled**, and new executable Rust tests are written but **not run**.

## Validation
- Offline Python tests: 17 passed.
- Agent/automation worker Python tests: 12 passed.
- Python syntax: passed.
- Targeted source contract assertions: passed (6 checks).
- Rust compilation: not run (toolchain unavailable).
- Rust regression tests: written, not executed.
- Live host/HTTP/MCP parity: not verified.
- No internal-testing ready claim: no compiled build, complete smoke flows, or proven absence of blocking data loss.

## Next substantial iteration
Replace the legacy SQL-body canonical knowledge endpoints and local adapter with this coherent file-backed service, add verified folder moves/identity preservation, and execute full crash/reopen and embedded/HTTP parity tests using a Rust toolchain. Reassess M18 completion based on executed acceptance, not added source artifacts. Flutter remains deferred to Atomic App.

## M18 and project estimate
M18 89% (unchanged), overall 89.5% (unchanged): provisional source-progress figures, not verified acceptance.
