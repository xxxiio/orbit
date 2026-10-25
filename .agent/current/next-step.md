---
template: next-step
template_version: 3.0.0
template_digest: "b88835ce70ef09490ad55db0e910dc05ff721384d6ccad8d45eafedd7654aaf5"
project_owned: true
record_contract: next-step
record_schema_version: 1
id: NEXT-0013
status: active
milestone_id: MILESTONE-0031
advances_exit_criteria: [EC-002, EC-003, EC-004]
---

# Next Deterministic Step

## Objective

Render the workflow-declared project views and the planning surface through the
already-implemented I2 API layer: board and timeline from the pinned workflow,
milestone management, dependency and readiness inspection, authored-capacity day
planning, schedule plan review followed by fingerprint-fenced apply, project
settings, and shareable list filters — each proven in real-browser acceptance.

## Why this is next

I1 delivered the authenticated shell and daily core, and I2's API surface
(milestone commands, graph/dependencies/work-brief/day-plan/schedule-plan/
workload/portfolio reads, plan review plus apply) is implemented and unit
covered. The remaining I2 work is presentation and interaction on top of that
authority; doing it before I3 keeps the workflow view registry from being built
against a half-populated workspace.

## Preconditions

- MILESTONE-0031 I1 committed (`bf2c4bd`, `87b4710`) with a green Playwright
  suite against a real server.
- I2 API layer: `web/src/api/endpoints/{milestones,planning}.ts` and the
  milestone/planning hooks, with `expected_revision` read at mutate time and
  plan apply replaying the reviewed fingerprint and command id.
- Structure remediation so `features/**` no longer duplicates list controllers
  and `components/**` no longer imports features.

## Inputs

- `web/src/api/**`, `web/src/hooks/**`, `web/src/features/**`, `web/src/app/**`.
- `.slim/deepwork/m31-web-app/api-contract.md` §6–§8 for exact query shapes.
- `web/tests/e2e/**` harness, fixture and specs; `web/evidence/i1/**`.

## Ordered actions

1. Board view from the pinned workflow's `kanban`/task views with server-validated
   moves, keyboard traversal and honest rejection feedback.
2. Milestones and timeline: status lifecycle, task counts, no invented dates.
3. Dependency and readiness inspection quoting the server's `reason` verbatim,
   plus graph depth and unblock counts.
4. Planning: authored capacity input, day plan dispositions, schedule window,
   plan review, explicit approval, apply receipt that survives reload.
5. Project settings: pinned workflow and available views, workflow change with
   confirmation, honest note about unsupported rename, destructive actions only
   where the API supports them.
6. Task-list upgrades: column choice, URL-backed filter/sort state, bulk actions,
   local windowing beyond 200 rows.
7. Extend Playwright with I2 specs; keep every assertion strict.
8. Update docs and records; capture evidence.

## Expected outputs

- Five workflow types producing appropriate functional workspaces through one
  view dispatch seam, with unavailable views named explicitly.
- Planning that is server-authored end to end, with no client scheduling math.
- Real-browser acceptance for board, milestones, dependencies, planning, settings
  and filters.
- Documentation matching the shipped I2 behaviour.

## Acceptance criteria

- [ ] NS-001: A board move is accepted or rejected by the server, and a rejected
  move never relocates the card while showing the server's reason.
- [ ] NS-002: Milestones can be created, renamed and transitioned, with invalid
  edges surfaced as server rejections.
- [ ] NS-003: Readiness and blockers display server `reason` values, and adding
  or removing a prerequisite changes them without a reload.
- [ ] NS-004: Day planning requires an authored capacity and shows server
  dispositions; schedule plan review cannot be skipped before apply, and the
  applied plan survives a reload.
- [ ] NS-005: Project settings show the pinned workflow and version; changing the
  workflow updates the available views; unsupported edits are stated, not faked.
- [ ] NS-006: Filter/sort/selection state is URL-shareable and survives a reload.
- [ ] NS-007: `tsc`, unit tests, terminology check, build and the full Playwright
  suite pass; documentation and records reflect only verified behaviour.

## Validation

```sh
cargo test -p orbit-server --offline
cargo test -p orbit-runtime --offline
cd web && node node_modules/typescript/bin/tsc --noEmit
cd web && node node_modules/vitest/vitest.mjs run
cd web && node scripts/check-terminology.mjs
cd web && node node_modules/vite/bin/vite.js build
cd web && node node_modules/@playwright/test/cli.js test
```

## Stop conditions

- A planning or board interaction would require client-side scheduling or
  transition rules to work.
- A server response cannot be shown honestly (rejection must be visible).
- A criterion cannot be evidenced in a real browser.

## Out of scope

- I3 workflow registry implementation, work items and upgrades (next iteration).
- Knowledge, attachments and agent surfaces.
- TUI changes.
