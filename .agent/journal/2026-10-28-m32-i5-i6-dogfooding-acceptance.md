---
template: journal-entry
template_version: 2.1.0
template_digest: "20e8b70b95080dbe3110fe7ad70c6bd0480c2a911af959019213e78b3a1b7814"
project_owned: true
record_contract: journal-entry
record_schema_version: 1
status: recorded
date: "2026-10-28"
task_id: TASK-0014
---

# Journal Entry: 2026-10-28 — M32 I5–I6: dogfooding, acceptance, installation, TUI handoff

## Context

I1–I4 completed the operation surface. M32's remaining obligations were evidence of
use, a clean-install proof, and a TUI specification derived from the larger product.

## Work completed

- Nine automated interactive dogfooding sessions against one persistent workspace
  (run 9 resumed run 8), 23 steps each, with captures, captions, step JSON and a
  trace. Seven defects recorded.
- Fixed the blocker: plan review refused the plan the server itself proposed.
  `orbit_planner::deadline_bound_at` is now the single rule used by the planner's
  proposer and validator, `schedule_planning`, `portfolio_schedule` and
  `planning_approval`; overdue work is planned with an explanation, and a still-ahead
  deadline that cannot be met is reported as unplaced with the task instead of
  proposed and refused. The same session's plan step now passes.
- Fixed silent loss of an acceptance-criterion draft, a settled capability request
  still offered as available, and unreadable planning-failure copy.
- Full browser acceptance and workspace gates green.
- Clean-install evidence: a lockfile-only directory installs with the documented
  workaround for this host's Windows npm shim, typechecks, builds, and produces a
  `dist` byte-identical to the checkout build; the supported loop was run end to end.
- TUI specification rewritten from proven M31/M32 usage (18 patterns, named shared
  APIs, A1–A23), with three stale documentation claims corrected.

## Decisions made

- The dogfooding artefact is labelled **automated interactive dogfooding**, with the
  human-only judgements listed; a scripted run is not presented as human use.
- The past-deadline rule: an overdue deadline no longer constrains a block end,
  because every block ends after the window opens; the slot says why. Rejecting the
  plan was the alternative and it made the workflow impossible.
- The clean-install limitation is recorded, not worked around in the tooling: the
  tooling must not be changed to accommodate one host's broken npm.
- Low-severity dogfooding findings are recorded rather than fixed under time
  pressure, with their symptoms and layers named.

## Files or components changed

- `crates/orbit-planner/src/lib.rs`,
  `crates/orbit-runtime/src/{schedule_planning,portfolio_schedule,planning_approval}.rs`.
- `web/src/features/tasks/task-drawer/criteria-section.tsx`,
  `web/src/features/workflows/{capability-model,capability-panel}.tsx`,
  `web/src/features/planning/{planning-utils.ts,project-planning.tsx}`.
- `web/tests/unit/**`, `web/tests/e2e/dogfooding/**`.
- `docs/guides/tui-redesign-spec.md`, `docs/web-app.md`, `docs/README.md`,
  `web/README.md`.
- `.slim/deepwork/m32-web-completeness/{i5-dogfood/**,i6-install.md}`.

## Validation performed

- `cargo test --workspace --offline` → 1,844 passed, 0 failed, 31 binaries; clippy
  `-D warnings` clean; fmt clean.
- `tsc` 0; vitest 60 files / 393 tests; terminology 256 files; `vite build` built.
- Playwright 136 passed / 5 skipped / 0 failed; dogfooding session 1 passed with the
  plan step now ok; `tests/documentation` 39 passed.

## Problems encountered

- The blocker took five reproduced sessions to characterise before the shared rule
  was obvious; the fix is one predicate, not a special case.
- A documentation lane requested access outside the repository
  (`/home/xliu/orbit-tui/src`, which does not exist in this product). It was refused,
  the session was cancelled, no repository change had been made, and it was
  re-dispatched with an explicit in-repo path rule.
- Plain `npm ci` does not work on this host; the workaround and its byte-identical
  output are recorded rather than hidden.

## Remaining work

- M32 is complete. Open low-severity findings: D-04, D-05, D-06/06b.
- The next milestone is the TUI redesign from `docs/guides/tui-redesign-spec.md`,
  not yet defined in the roadmap.

## New TODOs

- None.

## Risks or blockers

- The clean-install proof depends on a host workaround; a native-Linux-npm machine
  should run plain `npm ci` but that was not verifiable here.
