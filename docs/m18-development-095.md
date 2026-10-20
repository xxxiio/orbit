# Orbit 095 — M18 evidence command integration

## Delivered

- `authority_evidence` extends the server's canonical transaction model with
  four allowlisted operations: habit creation and occurrence recording; metric
  target creation and timestamped observation recording.
- Domain validation uses `orbit-domain::Habit` / `HabitCadence` for habit
  creation and rejects unknown references, contradictory metric direction,
  duplicate observation timestamps, stale revision, or mismatched idempotent
  retries.
- Every successful command commits relational records, one immutable semantic
  change, a server revision, an idempotency receipt, and accepted projection
  **atomically**. Project and task writes share the same sequence and receipts.
- `GET /v1/authoritative/evidence` returns revision, fingerprint and canonical
  evidence collections. Server's shared connection lock protects reads from
  concurrent commands.
- The opt-in Python offline command queue submits evidence operations while
  retaining stable identities, revision and conflict-stop behavior.

## Checks

- Python offline command tests: 14 passed.
- Python syntax: passed.
- Rust compilation, Clippy, and new server Rust tests: **not run** (toolchain
  unavailable in this environment).
- Live two-client authoritative evidence acceptance: **not run**.

## Gaps / M18 exit blockers

Native Rust desktop/mobile replica reconciliation, full server-authoritative
application services beyond the covered operations, browser and worker cutover,
lossless import of existing client and relay data, and built-in Git retirement
are still outstanding. Target updates and modification/deletion are deliberately
not exposed without a version-aware semantic update policy. Normal local-only
Orbit mode remains unchanged. Full acceptance is deferred to M15.
