---
template: journal-entry
template_version: 2.1.0
template_digest: "20e8b70b95080dbe3110fe7ad70c6bd0480c2a911af959019213e78b3a1b7814"
project_owned: true
record_contract: journal-entry
record_schema_version: 1
status: recorded
date: "2026-10-25"
task_id: TASK-0010
---

# Journal Entry: 2026-10-25 — M31 I2: workflow-driven workspaces, planning and honest refusals

## Context

I1 shipped the authenticated shell and daily core. I2 had to make a realistic
multi-task project fully manageable in the browser and prove that workflows, not
hand-written screens, define a workspace. The I2 API layer (milestone commands,
graph/readiness/dependencies, day plan, schedule plan, review and fingerprint-fenced
apply) was already implemented and unit-covered.

## Work completed

- Component-type view registry (`features/workflows/view-registry.tsx`): all thirteen
  declared component types are keyed by type; nine render, four (habit tracker, metric
  history, knowledge refs, agent jobs) state plainly that the browser does not render
  them yet, and unknown types get a named refusal panel. `view-metadata.ts` derives the
  primary surface, badge, per-view action and declared filters structurally, so no
  surface special-cases a workflow or view id.
- Workspace page resolves declared views first, then reserved project tools
  (planning, settings), with explicit loading / unresolvable / missing states.
- Board with canonical status columns, keyboard traversal and an explicit "Move to…"
  menu; the server decides validity, a rejection keeps the card in place and shows the
  server's reason. Drag-and-drop intentionally omitted without keyboard parity.
- Milestone lifecycle (create, rename, status, delete) with refusals against the row and
  task counts; timeline lists milestone status and saved schedule blocks rather than
  inventing dates.
- Dependencies: prerequisites add/remove, blocked-by, unblocks count, depth, and the
  server's readiness reason quoted verbatim.
- Planning: authored capacity (no request until positive), server-authored day plan
  dispositions, explicit schedule window, read-only review, explicit approval, then an
  apply that echoes the reviewed fingerprint and command id; blocks survive reload and
  an identical replay reports `replayed`.
- Settings: immutable identity with an honest rename limitation, pinned workflow and
  version, declared views, newest published version, preview diff, confirmed change.
- Task list: URL-backed query/status/priority/sort/page/columns/selection, readiness and
  dependency columns, 50-row windows beyond 200 matches, bulk actions on the fixed fence.
- Gate 1 carry-overs closed: accepted task records are written into the caches (no stale
  payload window); the session refetches on a timer, focus and visibility.
- Server/data corrections: domain validation text now reaches the client while storage
  failures stay opaque; readiness reasons have a stable Display so planning explanations
  no longer leak Rust enum names; the bundled Development manifest copy was cleaned.

## Decisions made

- Board offers every non-current status and relies on the server's decision instead of a
  duplicated transition table; the improvement is that the refusal reason is now quoted.
- View chrome is metadata derived from the declared primary component, not per-id code.
- Milestone commands fence on the same global ledger revision the snapshot exposes
  (verified live: `expected_revision: 0` is refused 409 once any command has run).
- The planning apply path never mints a command id or fingerprint; it replays the review.
- Unimplemented component types are named refusals, not blank success-looking panels.

## Files or components changed

- `web/src/features/workflows/**` (registry, metadata, copy, views), `features/planning/**`,
  `features/projects/{project-workspace-page,project-settings,project-task-list,milestone-create-dialog}.tsx`,
  `features/tasks/{task-cache,task-list-state}.ts`, `hooks/use-session.ts`.
- `crates/orbit-server/src/main.rs` (owned error text, typed domain refusal, mapper tests),
  `crates/orbit-runtime/src/{store,task_dependencies,daily_planning,workload_insights}.rs`,
  `blueprints/development.json`.
- Tests: `web/tests/e2e/{workspaces,planning}.spec.ts`, `web/tests/unit/features/*`,
  `web/tests/fixture/seed.ts` (prefix fix).

## Validation performed

- `cargo test -p orbit-server` → 50; `-p orbit-runtime` → 545 + 2; `-p orbit-blueprint` → 22;
  `cargo fmt --check` and `clippy -D warnings` clean; `pytest tests/contracts tests/documentation` → 82.
- `tsc --noEmit` clean; Vitest 25 files / 91 tests; terminology 144 files, 0 findings; `vite build` OK.
- Playwright full suite → 34 passed (2 opt-in evidence skipped) against real per-worker servers,
  including board accepted/rejected moves with API read-back, milestone lifecycle and invalid edge,
  dependency/readiness, planning capacity/review/apply/reload, settings preview/change, five-workflow
  resolution.
- Live API probe: all I2 read endpoints return documented shapes; milestone fence verified; plan
  review→apply creates caller-named blocks and replays idempotently.

## Problems encountered

- The HTTP adapter flattened domain refusals to "invalid request", so the UI could not explain a
  rejected status change; fixed by carrying typed domain text while keeping storage opaque.
- Planning explanations embedded Rust `Debug` names; fixed at the runtime source (`ReadinessReason`
  Display) and covered by tests that reject CamelCase tokens.
- The fixture built task ids from the wrong project index; fixed and determinism re-verified.

## Remaining work

- I3: work-item surfaces, transitions, upgrade preview/confirm, honest action discoverability.
- I4: knowledge, attachments, agents. I5: dogfooding. I6: refinement, docs, TUI redesign spec.
- Research has no task-bearing view until work-item surfaces land.

## New TODOs

- None outside the milestone exit criteria.

## Risks or blockers

- `getdone-validate-project` cannot run in this environment (missing skill-pack manifest); records
  are written to the documented contracts but cannot be machine-validated here.
- Cross-surface concurrent commands still fence on the same revision (honest 409, self-healing).
