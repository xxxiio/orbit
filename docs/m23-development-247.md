# Orbit Development 247 — Connected-mode handoffs, review and catch-up

## Scope

Extends the pre-existing authenticated, device-owner-scoped connected-mode agent-job API with durable handoff submission/retrieval, immutable review decisions, and deterministic cross-project overview.

### API

- `GET /v1/authoritative/agent-jobs/handoffs?id=JOB_ID`
- `POST /v1/authoritative/agent-jobs/handoffs` body `{ "job_id":"...", "handoff_id":"...", "at_ms":0, "payload":{"summary":"...", "validation":{}, "next_steps":[]} }`
- `GET /v1/authoritative/agent-jobs/review?id=JOB_ID`
- `POST /v1/authoritative/agent-jobs/review` body `{ "job_id":"...", "handoff_id":"...", "decision":"approved", "reason":"...", "at_ms":0 }`
- `GET /v1/authoritative/agent-jobs/overview`

All routes use existing device authentication. Individual job access checks stored device ownership; overview filters entries to owned jobs. The reviewer identity is the authenticated device, not a caller-provided string. Review never changes task status. Handoff submission remains idempotent by its supplied ID and immutable thereafter. No shell commands run on the server.

## Boundaries and caveats

This is a connected-mode *reporting and review* vertical slice, not remote agent execution. It does not yet include executor registration, leases, machine-side authentication, or remote job dispatch. It also does not project these data through the offline outbox/replica protocol. Consequently, connected clients must use the authority while online for these operations. Device owner scoping remains a coarse policy and is not multi-user RBAC.

## Validation

Static route and ownership wiring checks passed. `cargo`, `rustc`, and Herdr are unavailable; Rust compilation, integration tests, and live end-to-end execution remain unverified. M23 progress estimates are provisional until executable acceptance.

## Milestone tracking

M23 provisional implementation 48% -> 54% (+6 pp), based on connected-mode durable handoff/review/catch-up feature coverage. M22 source scope 100%; M15 release acceptance pending. This is not an executable acceptance claim. Internal testing not ready.
