# Development 224 — Explicit plan review and approval

The authoritative Rust runtime now exposes a typed plan approval boundary.

- `review_task_plan_approval` does not mutate schedule state. It freezes the actor,
  command ID, expected global revision, validated plan command payload, and the
  concrete activity/schedule identities into a SHA-256 review fingerprint.
- `apply_approved_task_plan_accepted` rejects missing approval, changed identities,
  changed actors, changed command IDs, and changed expected revisions before
  delegating to the existing atomic/idempotent `apply_task_plan_accepted` path.
- The embedded `OrbitClient` supports a two-phase `review_task_plan` followed by
  `approve_task_plan(prepared, true)`. `PreparedTaskPlan` carries the validated
  command privately to prevent accidental modifications after review.
- A declined approval has no side effects. A stale revision fails in the
  authoritative command ledger; an application-level changed schedule is
  revalidated transactionally before any blocks are committed.

## Outstanding integration

The legacy `OrbitClient::apply_task_plan` still provides immediate application
and should be deprecated only after all existing callers move to the two-phase
interface. CLI, MCP, and HTTP approval adapters are not yet implemented. The
new `PlanReview` is a deterministic fingerprint, not a cryptographic identity
signature: authentication and permission enforcement remain transport duties.

The planning scenario comparison from Development 223 is a *capacity selection*,
not a timestamped `ValidatedPlanProposal`; it cannot be converted to calendar
reservations without explicit slot assignment and revalidation. Do not infer or
silently create schedule blocks from scenario comparisons.

Cargo compilation and executable Rust tests were unavailable in this workspace;
the new tests are source-only until M15 executable acceptance.

## Server-authoritative protocol

A device with valid Orbit credentials can issue `POST /v1/authoritative/plan-review`
with `{command_id, expected_revision, window_start_ms, window_end_ms, items}`,
where each item contains `task_id`, `starts_at_ms`, `ends_at_ms`,
`schedule_block_id`, and `activity_id`.

The server derives the review actor from the authenticated device header and
builds a validated, exact schedule command. It returns the immutable review
fingerprint and affected task/block identifiers without creating reservations.

After displaying the intended changes, the client can send
`POST /v1/authoritative/plan-apply-approved` with `{plan, approval}`, setting
`approval.approved=true` only after explicit user authorization. The server
reconstructs the same typed command, validates the review binding, and delegates
to the atomic authoritative schedule-plan command. Revising any item after review
invalidates the approval. Retrying a committed command uses the same command ID,
receipts, block identities, and payload, without creating duplicates.

The connected Rust client exposes `review_schedule_plan_online` and
`apply_approved_schedule_online`. Offline planning remains read-only: this path
never queues calendar reservations against stale accepted snapshots.

**Boundary:** Selection-only comparisons have no timestamps and therefore
cannot be executed directly; callers must supply a time-allocated, validated
schedule proposal. CLI and MCP commands for the approval workflow remain future
work. Authentication is checked by the server, not by a bearer-independent hash.

## Local-only CLI

Use `orbit task review-schedule --input plan.json --json` to obtain the exact
review. Construct a separate JSON document containing the unchanged `plan`
and the returned `approval.review`, setting `approval.approved=true` only after
a user reviews the exact scheduled slots. Submit with:

```sh
orbit task apply-reviewed-schedule --input approved.json --confirm --json
```

Both the request's `approved=true` and CLI `--confirm` are required. This is a
one-way authoritative write: use a unique command ID and stable block/activity
IDs, retain the exact approved file for idempotent retries, and never invent a
fresh command ID after an uncertain response. In connected mode use the
`ConnectedOrbitClient` HTTP approval API, not a fallback local DB.
