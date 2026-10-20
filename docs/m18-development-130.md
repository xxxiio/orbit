# Orbit Development 130 — Knowledge journal namespace integrity

## Priority decision
M18 U2 (canonical Markdown-backed knowledge and SQLite transaction coordination) remains the highest-priority unfinished work. Before enabling filesystem intents through authoritative commands, this iteration eliminates a concrete startup-recovery ambiguity which could misinterpret a valid creation intent as an update intent. No standalone release-hardening track is being advanced ahead of features.

## Defect and change
The filesystem intent journal names updates `update-{operation_id}.json`, but creation IDs were unrestricted. A creation with ID `update-foo` would therefore be written as `update-foo.json`. Startup recovery chooses the intent type by this filename prefix; it could not recover this create intent and would fail closed. Because the note may already have been published, this could also strand a journal record and prevent opening the host.

- Reject creation operation IDs beginning with `update-` before any file/journal mutation.
- Validate the same namespace rule when reading persisted creation intents, so corrupt or ambiguous records are not replayed under the wrong type.
- Added Rust regressions for early rejection without side effects and fail-closed recovery without touching a note.

## Limitations
- This change does not connect `KnowledgeOperations` to canonical knowledge commands or SQLite receipts.
- Existing canonical knowledge commands continue to persist Markdown bodies in SQL, which remains inconsistent with the target Markdown-authoritative design.
- The prefix-based intent format remains a transitional design; a typed, versioned journal record is preferable once the canonical command protocol is implemented.
- Executable Rust tests were not run: the Rust toolchain is unavailable.

## Validation
- Offline authority Python suite: 17 passed.
- Agent and automation worker Python suite: 12 passed.
- Python compileall: passed.
- Rust regression tests: added, not executed.
- Rust compile, cross-resource crash recovery, and client transport parity: unverified.

## Next priority and readiness
Next iteration must prioritize canonical Markdown/SQLite command acceptance, with durable intent + accepted receipt recovery, metadata-only SQL storage, and file-backed reads; then revisioned folder operations. No higher-priority unfinished category supersedes this M18 correction. Flutter stays in Atomic App. Internal testing is **not ready**: executable Rust smoke flows and data-loss-blocker checks are missing.
