# Orbit M20 — Development 196: transactional connected-workflow staging

## User-visible outcome

The connected TUI batch confirmation previously queued individual intentions one
by one. A late failure could leave an arbitrary partially queued prefix even
before any request was sent to the authority server. Reviewed batches now enter
the local SQLite replica as a single transaction: either all previously unseen
commands are staged, or none of them are. Duplicate requests with identical
stable identities are idempotent. Conflicting reused IDs fail and roll back.

This is a deliberately *local-only atomicity* guarantee. When commands are
flushed to the server, acceptance is ordered and may have a completed prefix.
Remote execution is **not** an atomic distributed transaction.

## Contracts

- Batch length must be 1..=100.
- Every command ID must be unique within the batch, nonempty, and at most 128 bytes.
- Actions must be objects and cannot provide internal `command_id` or
  `expected_revision` fields.
- Preexisting command IDs must have exactly the same command family and JSON
  action; otherwise the entire new batch is rejected.
- Newly queued steps receive contiguous outbox ordinals and remain pending.
- Accepted snapshots are never modified by staging a batch.
- The TUI reports accepted-vs-pending status without implying server acceptance.

## Regression coverage authored

Added four Rust behavioral scenarios: durable idempotent staging, late collision
rollback, duplicate/invalid late-step preflight, and retaining ordered entries
when a prefix is already queued. Rust compilation and broader acceptance remain
on the M15 track per project priorities.

## Milestone assessment

M20 remains **95% provisional**. This closes a concrete connected TUI staging
gap but does not prove that all remaining legacy paths are retired. Do not
count this as verified milestone completion or internal-test readiness.
