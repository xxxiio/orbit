# Orbit Development 237 — M23 authoritative job ownership boundary

## Delivered

- Schema 16: `agent_job_owners` records the authenticated reserving device.
- Job and owner are inserted in **one SQLite transaction**, including immutable execution brief and job event.
- Idempotent reservation replays require the same device as well as the same job payload.
- All exposed HTTP job mutations and point reads now require the authenticated owner, and attention lists are scoped to that owner.
- Existing client-device authentication remains in effect; no agent execution side effects were introduced.
- Added source unit coverage for owner binding and replay/foreign-device rejection.

## Validation and limitations

SQLite schema was independently parsed and initialized. Rust compile/unit tests cannot be run without `cargo`/`rustc`; code remains unverified. Server device ownership is intentionally **not** a complete machine-executor authorization model, and it does not integrate the generic connected-mode offline outbox or client replica projection. In particular, authorization is per device, not team/project role; richer sharing requires deliberate authorization policy. No Herdr adapter has been started.

## Progress accounting

M23 remains **21%** provisional implementation progress; M23-I1 remains incomplete until accepted-command and connected/offline semantics are integrated and Rust executable acceptance passes. M22 retains 100% source scope. M15 release acceptance remains pending. Development 237 is a checkpoint, **not a completed 5-point milestone iteration**.

## Next highest priority

Connect job operations to the existing canonical accepted-command and semantic projection model, then provide executor-scoped grants and Herdr integration. Avoid duplicating the authority source of truth.
