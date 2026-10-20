# Development 225: Scenario → concrete slots → explicit approval

## Feature contract

A user compares portfolio alternatives using `orbit task compare-scenarios`, then
chooses the baseline or named alternative. `orbit task review-scenario --input
scenario-execution.json` deterministically packs **every selected task** into
accepted free calendar windows. This command returns `plan` and `review` in a
single read-only response. The response contains real task identifiers, concrete
start/end timestamps, stable calendar-block/activity identities and the server
revision.

The caller must visibly inspect `review.scheduled_slots` before constructing an
`ApprovedScheduleRequest { plan, approval: { review, approved: true } }`. Execution
is only through the existing `orbit task apply-reviewed-schedule --confirm` or
`/v1/authoritative/plan-apply-approved` routes. A comparison or review does
not reserve time. A review is **never** sufficient authorization to execute.

The server exposes `POST /v1/authoritative/scenario-execution-review`, bound to
the authenticated device; the connected native client exposes
`review_scenario_execution_online`. The server reconstructs the comparison,
re-evaluates accepted schedule windows, validates task and schedule revisions,
then delegates the exact proposal to the existing validated approval gate.

The allocator rejects infeasible pinned work, duplicate IDs, empty plans,
invalid windows, missing selected tasks and tasks that cannot fit an
uninterrupted slot. Nothing is silently omitted from an approved scenario.

## Intentional boundaries

- The client supplies stable identities for idempotent approved retries.
- No implicit approval from MCP or background agents; an agent can use the
  existing explicit approval API only with independently granted permission.
- Global accepted revision must match at review; authoritative application
  checks it again when execution begins.
- No new calendar writing subsystem, and no Git/JSONL sync subsystem.
- Rust compilation and executable acceptance belong to M15; source checks
  and tests here do not establish production-readiness.

## Local CLI example

1. Export `ScenarioCompareRequest` to `compare.json` and evaluate it with
   `orbit task compare-scenarios --input compare.json --json`.
2. Create a `ScenarioExecutionRequest` including the exact `comparison` input,
   the selected `scenario_name`, a bounded time window, unique `command_id`
   and `identity_prefix`, for example:

```json
{
  "comparison": {
    "baseline": {
      "now_ms": 0,
      "available_minutes": 90,
      "max_tasks": 2,
      "project_id": null,
      "include_unassigned": true,
      "include_unsized": false,
      "default_minutes": null
    },
    "expected_revision": 3,
    "alternatives": []
  },
  "scenario_name": "baseline",
  "starts_at_ms": 0,
  "ends_at_ms": 7200000,
  "command_id": "user-plan-001",
  "identity_prefix": "user-plan-001"
}
```

3. Run `orbit task review-scenario --input scenario.json --json`. Present the
   returned `review.scheduled_slots` to the user; save the returned `plan` and
   `review`. The accepted revision must not advance between review and apply.
4. Only on independent user approval, construct `ApprovedScheduleRequest` and
   submit `orbit task apply-reviewed-schedule --input approved.json --confirm`.
   Refusal is represented by `approved: false`; refusing never writes.

## Agent/MCP contracts

- `planning.review_scenario` accepts `{ "request": ScenarioExecutionRequest }`
  and returns a read-only review; it requires full calendar visibility.
- `planning.apply_approved_scenario` accepts `{ "request": ApprovedScheduleRequest,
  "confirm": true }`, requires a write-enabled session and full workspace
  authorization, and rejects unapproved, stale or tampered schedules.
- Both use the same authoritative host/HTTP validation; neither has an offline
  fallback or bypasses the accepted-revision check.

## Review and safety cases

Regression coverage includes stable allocation, non-overlapping windows,
unsplittable task refusal, duplicate identities, stale revisions, absent named
alternatives, explicit denial, authoritative idempotent retries, and rejection
of concurrent edits. No source-level test substitutes for runnable acceptance.

## Comparing calendar-feasible alternatives

`orbit task review-scenario-options --input scenario.json --json` evaluates the
baseline and every named what-if option over the same accepted task revision and
schedule window. Feasible options return exact plans and reviews; infeasible
options contain an explicit rejection reason. Rejection of one option does not
silently exclude other options. The corresponding online endpoint is
`POST /v1/authoritative/scenario-options-review`, and the MCP tool is
`planning.review_scenario_options`. The caller must still independently choose
**one** option and explicitly approve its exact slots.

## Connected TUI commands

`plan.review --hours 8 --minutes 120 --pin task-123` constructs a time-window
review of the named what-if alternative. `--baseline` selects the unchanged
baseline. It requires a current accepted replica and an online authenticated
server; the returned review is displayed with task and block IDs and concrete
start/end timestamps. The command does not write.

`plan.approve --confirm` applies the most recently reviewed plan. The TUI
consumes the stored review on submission; network errors and stale revisions
require a new review rather than blindly retrying. In particular, `plan.approve`
without `--confirm` is rejected. The TUI never claims an offline schedule
approval, never queues a planning write, and never treats a preview as consent.

## Risks for M15 executable acceptance

The new Rust tests, including live-host SQLite protocol tests and embedded MCP
approval tests, are source-only until Cargo compilation and execution. M15 must
exercise the generated CLI, TUI, MCP, and server over actual authenticated
HTTP. It must verify that failed/competing approvals cannot create partial
calendar blocks, that retries retain immutable receipts, and that concurrent
schedule changes force a new review. The design is implemented at source level,
not established as runtime-correct in this environment.
