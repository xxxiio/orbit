# Development 236 — M23 authenticated agent-job HTTP boundary

Source-level integration checkpoint; **not a completed or accepted iteration**.

## Changes

- Add `orbit_runtime::agent_authority` host-owned API for agent job reservations, idempotent state commands, readback, and attention lists.
- Device-authenticated HTTP endpoints:
  - `POST /v1/authoritative/agent-jobs/reserve`
  - `POST /v1/authoritative/agent-jobs/command`
  - `GET /v1/authoritative/agent-jobs?id=...`
  - `GET /v1/authoritative/agent-jobs/attention`
- Explicitly reject malformed identifiers and negative revisions/timestamps.
- Preserve authority-owned SQLite reservation and command transactions; no agent process is launched.
- Define a stable serializable job response independent from transport-specific runtime state.
- Add three host-boundary source tests.

## Limitations / next priority

This is **not** yet an integrated connected-client outbox command family. Device authentication
exists on the server routes, but accepted global revision/snapshot projection and offline-replica
semantics require dedicated integration work. Do not enqueue `agent_job` into the existing
outbox until server/replica revisions and conflict handling are defined and accepted.
The reservation route is an online-only authoritative operation in this checkpoint.
Herdr launch and remote executor authorization are not included. The server still has no
permission to run shell commands; the device identity only authorizes job record operations.

Rust toolchain is unavailable in the work environment. Source/tests were added, but
no claim of successful compile or Rust test execution is made. This checkpoint does
not satisfy the five-point *completed iteration* rule. Milestone progress remains
M23 21% provisional, M23-I1 incomplete; M15 executable acceptance pending.
