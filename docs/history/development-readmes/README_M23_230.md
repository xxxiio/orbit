# Orbit Development 230 — M23-I1 partial foundation

Why now: M22 planning supplies task readiness; M23 needs persistent authority-owned job identity, reservations and lifecycle **before** spawning external agents. M15 remains the outstanding release-acceptance gate and should not be displaced.

Implemented: domain job state and transition rules; SQLite v14 schema, durable job and event ledger, active-task unique reservation, idempotency key validation and optimistic concurrency transitions, plus unit tests. No task is marked complete when its agent job finishes.

Validation: Python SQLite schema/constraint smoke PASS. Rust build/tests NOT RUN (`cargo` unavailable). Application accepted commands, task revision verification, remote replicas, Herdr runtime, monitoring, structured handoffs and UI NOT IMPLEMENTED. No claims of runnable acceptance.

Milestone status (measured against milestone acceptance; percentage estimates are provisional): M22 source scope 100%, executable acceptance pending M15; M15 remains pending (percentage not re-estimated); M23 8%, I1 40% of its 20-point milestone share, with authority integration outstanding. This development snapshot is a **partial iteration**, below the preferred completed-iteration 5-point acceptance threshold; do not count it as a closed iteration until integration/validation is finished.

Next: complete the server/local authoritative launch command with revision-verified ready-task capture, accepted command receipt, connected outbox projection, and tests for stale, blocked, duplicate, disconnected launches; then run Rust workspace checks before I1 closure.
