# M25 Development 292 — Blueprint command queuing

> **Historical checkpoint.** This record describes the state at Development 292.
> Server-side atomic blueprint command receipts and ordinary replay/reconciliation
> were implemented afterward and are exercised by the blueprint smoke. The current
> remaining M25 gaps are capability installation/action execution, connected MCP
> outbox parity, blueprint upgrade, habit-recurrence extraction and a lost-ack E2E
> test. See `docs/roadmap.md`.

Source change: add blueprint-specific durable mutation envelopes to canonical connected-mode replica, expose `orbit blueprint queue`, `pending`, and `flush`, and extend global flush/auditing to recognize blueprint routes. No local SQLite authority fallback.

Important remaining work: server-side atomic idempotency receipts and replay/reconciliation, required before offline blueprint commands can be treated as fully accepted functionality. This implementation is partial and does not close the M25 offline-command acceptance criterion.

M25 prior reported completion: 82%. No completion percentage is advanced in this checkpoint absent server-side acceptance semantics. Rust compilation and broad integration validation are M15.
