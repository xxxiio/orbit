# Orbit Development 165 — Fail-closed filesystem journal recovery

## Scope and rationale

The next M19 implementation requirement is coordinated filesystem consistency. The preexisting `KnowledgeOperations::recover` loop discovered errors one-by-one while replaying prior operations, so a corrupt later intent could leave some earlier files published before startup rejected the journal. That violates the intended fail-closed behavior for a batch of preexisting filesystem recovery intents.

## Implementation

- Preflight **all** `.orbit-operations` entries before publishing any content. Detect malformed JSON, unexpected file types, symlinked journal records, mismatched content digests, invalid paths, and wrong filename/operation-ID associations.
- Reject multiple simultaneously pending intents for the same Markdown path. Their deterministic filename sorting cannot be treated as an authoritative revision ordering.
- Verify each target's current state: a create target must be absent or already contain exactly the desired bytes; an update target must match either its expected predecessor or its already-published desired bytes.
- If preflight fails, preserve every intent and every note, so an operator can diagnose or correct conflicts. After successful preflight, replay operations and `fsync` the journal after removing each accepted intent. This preserves restart idempotency if replay is interrupted.
- Add three Rust regression tests: corrupt later intent, divergent external edit, and ambiguous duplicate intent path. A Python source guard verifies the preflight boundary remains in place.

## Limitations

This covers the filesystem intent journal. It does not make multi-file import atomic or replace the SQL-based durable command log with a single filesystem transaction. A power failure *during* replay can still leave a prefix of successfully published operations; that prefix remains recoverable. The separate SQLite Markdown/folder command reconciliation remains responsible for authoritative receipts and revisions. Rust tests and end-to-end crash injection were not executed because the Rust toolchain is unavailable and broader validation has been assigned to M15.

## Next milestone

Orbit 166 should finish remaining M19 implementation reconciliation: remove obsolete knowledge-domain dependencies where safe, document the authoritative user-facing feature surface, and hand off integration verification to M15. Do not claim verified M19 completion without a Rust build.

## Attachment publication and asset integrity

The CLI attachment publisher previously opened the final content-addressed filename with `create_new`, exposing partially written bytes to concurrent readers before the write and fsync finished. It now writes and syncs a private staging file, atomically creates a hard link under the immutable SHA-256 filename, and syncs the asset directory. It cannot overwrite an existing blob; collisions fail closed, and identical retries verify the existing bytes. Staging files are deliberately excluded from note asset discovery. An interruption before linking a note can leave an orphan, which the existing asset audit can detect without deleting data.

Asset import and export now verify their complete source asset sets before starting to publish outputs. This avoids copy-prefix side effects from a corrupt late input; a crash during publication is still a resumable operation, not an atomic multi-file transaction. An additional Rust test covers immutable publish/retry and failure to publish mismatched content.
