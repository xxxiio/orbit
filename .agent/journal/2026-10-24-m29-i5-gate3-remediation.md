---
template: journal-entry
template_version: 2.1.0
template_digest: "20e8b70b95080dbe3110fe7ad70c6bd0480c2a911af959019213e78b3a1b7814"
project_owned: true
record_contract: journal-entry
record_schema_version: 1
status: draft
date: "2026-10-24"
task_id: null
---

# Journal Entry: 2026-10-24 — Oracle Gate 3 remediation for the M29 I5 shared shell

## Context

Oracle Gate 3 blocked M29 I5. The split of `app.rs`/`render.rs`/`main.rs` had left
four source-contract test modules pointing at the deleted `crates/orbit-tui/src/app.rs`,
and the shared shell had six honesty gaps around the detail cache, the `!`
inspector flush, accepted collection reads, connected `None` outcomes, the
pending-count read path and the narrow conflict review. This pass fixes exactly
those findings in one bounded change; the shared-shell design and the
pending-never-accepted rule are unchanged.

## Work completed

- Repaired the four stale contract modules to assert against the real owners
  (`backend.rs`, `app/*`, `input/*`, `render/*`, `connected_ops.rs`,
  `connected_actions.rs`, `crates/orbit-client/src/unified/planning_queries.rs`)
  and restored the strict authority, planning-navigation and planning-handoff
  assertions. Added the `handle_key`/`handle_modal` single-owner guard and the
  `ORBIT_TUI_LEGACY_CONNECTED`-is-gone guard.
- Moved local backend construction into `src/local_backend.rs` so no connected
  file, and no `app/`, `input/` or `render/` module, can reach `OrbitClient::open`
  or an embedded authority.
- Routed criterion toggle/reorder/remove and attachment remove/import through
  `App` methods that invalidate the detail cache (and refresh the pending count
  for attachments).
- `pending_flush_report` now refreshes accepted state and invalidates the detail
  cache on a non-empty flush, re-lists pending rows, and separates
  "N accepted by the server" from "M still pending/rejected". Staged uploads
  keep the status on `Pending`; the `f` flush action does the same.
- Added read-only console listings `routine.list`, `strategy.list`,
  `milestone.list`, `schedule.list` over the existing typed records
  (`AuthorityConnection::routines/strategies/milestones/schedule`), rendered in
  `planning_ui` with explicit accepted-state labels, empty states, help entries
  and suggestions.
- `describe_mutation_outcome` now fails closed as pending for a connected call
  with no recorded disposition, while local success is unchanged.
- Cached the pending count on `App` (queue, flush, refresh, inspector refresh,
  palette open, attachment stage/remove) and made the header read the cache.
- The conflict review shows the server rejection reason and scrolls with
  Up/Down/PgUp/PgDn (letters still edit the replacement).
- Console suggestions are filtered to command-shaped lines; help prose can no
  longer be filled into the prompt.
- Fixed stale statements in `.agent/status/project-status.md`,
  `docs/developers/ui-components.md`, `.agent/evidence/m29-perf-after.md`, and
  labelled the small M29 fixture session a launch smoke while the opt-in 20/200
  session asserts the measured shape.

## Decisions made

- The blocker asked for no `OrbitClient::open(` in `src/backend.rs`. Rather than
  weaken the guard, local construction moved to a local-only module, so the
  string scan is literally true and the structural boundary is clearer.
- Accepted collection listings decode the authority's own records
  (`RoutineRecord`, `StrategyRecord`, `MilestoneRecord`, `ScheduleRecord`) via
  `serde_json`; no new domain model was introduced and a row this binary cannot
  decode fails closed instead of being dropped.
- The pending-count read counter is test-only and thread-local, so concurrently
  running tests cannot observe each other's reads.
- Conflict review scrolling uses arrow/page keys only: `j`/`k` must keep
  typing into the replacement command.

## Files or components changed

- `crates/orbit-tui/src/backend.rs`, `src/local_backend.rs` (new), `src/main.rs`.
- `crates/orbit-tui/src/app/{mod,actions,connected,forms,navigation,tests}.rs`.
- `crates/orbit-tui/src/input/modal.rs`, `src/render/{shell,modals}.rs`.
- `crates/orbit-tui/src/{connected_ops,connected_actions,planning_ui}.rs`.
- `crates/orbit-tui/src/tests/m29_gate3_remediation_tests.rs` (new),
  `m29_connected_task_detail_tests.rs`, `m29_perf_render_tests.rs`.
- `tests/contracts/test_{authority_entrypoints,deployment_boundaries,planning_handoff_contract,planning_navigation_contract}.py`.
- `tests/acceptance/tui/{test_m29_fixture.py,README.md}`, plus documentation and
  `.agent` status/evidence corrections.

## Validation performed

- `cargo test -p orbit-tui --offline` — 203 passed, 0 failed.
- `cargo test -p orbit-client --offline` — 348 passed, 0 failed.
- `cargo clippy -p orbit-tui -p orbit-client --all-targets --offline -- -D warnings` — clean.
- `cargo fmt --all -- --check` — exit 0.
- `python3 -m pytest -q tests/contracts` — 39 passed.
- `python3 -m pytest -q tests/documentation` — 39 passed.
- `bash scripts/check-m15-python-acceptance.sh` — 99 passed, 12 errors; every
  error is `tests/packaging/test_release_workflow.py` reading the withheld
  `.github/workflows/release.yml` (tracked as `_.github/...`). Reproduced on a
  pristine `HEAD` worktree, so it is environmental, not caused by this change.
- `cargo build -p orbit-tui -p orbit-cli -p orbit-server --offline` — finished.
- `python3 -m pytest -q tests/acceptance/tui` — 54 passed, 1 skipped.
- `ORBIT_TUI_FULL_FIXTURE=1 python3 -m pytest -q tests/acceptance/tui/test_m29_fixture.py`
  — 2 passed, including the asserted 20-project/200-task measured shape.

## Problems encountered

- `pending_flush_report` originally refreshed accepted state on every flush,
  including an offline failure; that double-reported an error and slowed the
  failing path, so the refresh is now conditional on a flush that was not an
  error while the outbox was non-empty.
- The first narrow-conflict test only checked that the reason line existed; at
  60x18 the line wraps and the server text lands below the fold, so the test now
  scrolls until the reason text is actually on screen.

## Remaining work

- None for this finding set.

## New TODOs

- None.

## Risks or blockers

- The release-workflow packaging errors need the withheld
  `.github/workflows/release.yml` restored by a maintainer; they are unrelated to
  the TUI shell.
