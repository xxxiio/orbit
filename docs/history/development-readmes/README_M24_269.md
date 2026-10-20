# Orbit Development 269 — Canonical Pending Commands

Source iteration: **M24 58% → 66% (+8 points)**.

- Agent offline intents join the connected replica's authoritative SQLite
  command outbox, sharing global FIFO ordering with projects, tasks and files.
- `orbit agent queue-*`, `pending`, and `flush` route through the existing
  `AuthorityReplica` implementation. `flush` never bypasses older domain work.
- The isolated JSON agent outbox has been retired (greenfield cutover).
- Server-authoritative acceptance and external Herdr execution remain separate.
- Essential code-level checks are included; full Rust build/integration and
  fixes uncovered by them belong to M15.

No CLI executable or new SQLite authority is introduced. See
`docs/m24-offline-command-consolidation.md` for cutover limitations.
