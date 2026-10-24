---
template: journal-entry
template_version: 2.1.0
template_digest: "20e8b70b95080dbe3110fe7ad70c6bd0480c2a911af959019213e78b3a1b7814"
project_owned: true
record_contract: journal-entry
record_schema_version: 1
status: recorded
date: "2026-10-24"
task_id: TASK-0007
---

# Journal Entry: 2026-10-24 — M29 connected consolidation, maintainability and acceptance

## Context

After I1–I3 the interface was coherent, but the connected legacy TUI was still a
second presentation behind `ORBIT_TUI_LEGACY_CONNECTED`, `main.rs` (6,999
lines) and `render.rs` (3,435 lines) concentrated unrelated code, and the
20-project interaction path had never been measured.

## Work completed

- Shared-shell connected surfaces: `!` pending inspector with per-command state
  and flush/report, conflict review with fail-closed replacement by a validated
  replacement command, and a connected command console covering the legacy
  grammar (project/task/habit/metric/schedule/routine/milestone/strategy/note,
  `batch`, `resolve`, `sync.*`, `note.*`, `plan.*`) plus accepted
  routine/strategy/milestone/schedule reads.
- Deleted `connected.rs` (1,994 lines), the legacy entry point and dead
  planning helpers; kept the command grammar and typed client APIs.
- Extracted `app/{state,navigation,selection,actions,forms,connected}`,
  `input/{dispatch,modal,text}`, `render/{layout,shell,components,modals}` and
  moved 18 test modules to `src/tests/`. `main.rs` is 186 lines with one loop and
  one dispatch entry point.
- Added `local_backend.rs` to isolate local construction, contract guards for
  authority boundaries/single dispatch/legacy-flag absence, and a detail-model
  cache with targeted invalidation.
- Built `scripts/tui-fixture.py` (20 projects, 200 tasks, 1,017 activities, 6
  attachments, 8 agent jobs, 5 pinned blueprint versions), a PTY perf probe,
  scenario A–G tests, a capture driver and 21 screen captures.
- Rewrote the user guide, TUI contract, developer component guide and acceptance
  README to match shipped behaviour.

## Decisions made

- The console is one modal in the shared shell, not a second shell: capability
  parity without duplicating the event loop, rendering or focus model.
- Pending is never accepted: unknown connected outcomes fail closed as pending;
  flush reports accepted vs still-pending/rejected per command.
- Local construction lives in `local_backend.rs`; connected modules are checked
  for embedded-authority constructors by contract tests.
- Performance work is limited to measured hot spots: detail reads per frame and
  whole-workspace reloads on title-only edits. PTY-dominated operations were
  left alone and documented.

## Files or components changed

- `crates/orbit-tui/src/{main.rs,backend.rs,local_backend.rs,connected_ops.rs,connected_actions.rs,planning_ui.rs}`
  and the `app/`, `input/`, `render/`, `tests/` trees.
- `crates/orbit-client/src/unified.rs` (thin queue/notes/snapshot passthroughs).
- `tests/acceptance/tui/` (scenarios A–G, settle helper, fixture smoke), `tests/contracts/`,
  `scripts/tui-fixture.py`, `scripts/tui-captures.py`, `docs/`, `.agent/evidence/`.

## Validation performed

- `cargo test --workspace --offline` 1,552 passed; `orbit-tui` 208;
  `orbit-client` 348; `orbit-ui` 28.
- clippy `-D warnings` on 12 crates; fmt clean; documentation 39; contracts 43.
- PTY suite 54 passed/1 skipped; full fixture opt-in 2 passed; scenarios A–G.
- M15 profile 45/47 (release-workflow contract withheld).
- Performance: quick edit 173→83 ms (100x30), 173→87 ms (160x45); detail open
  7.6→5.3 ms and board view switch 93.5→77.6 ms at 160x45; others within noise.
- `python3 scripts/tui-captures.py` 21 captures at 60x18/100x30/160x45.

## Problems encountered

- Oracle gates blocked twice: broken/weak contract tests after the extraction,
  lost accepted routine/strategy reads, stale detail cache after criteria and
  attachment mutations, stale accepted shell after inspector flush, and
  under-invalidated pending count. All were fixed with regression tests.
- PTY flakiness came from absence-only waits sampling torn frames; fixed with a
  settled-frame helper and positive surface predicates.
- `getdone validate` warnings are project-owned digest drift accumulated by prior
  milestones; validation passes.

## Remaining work

- Per-device ACLs (TODO-0013), server attachment GC (TODO-0014) and live Herdr
  acceptance remain outside M29.
- The withheld release workflow keeps two M15 checks red.

## New TODOs

- None added.

## Risks or blockers

- The connected console is the only surface for several advanced record
  families; discoverability is lower than a structured form.
- Conflict replacement needs a user-supplied valid command; the UI fails closed
  but cannot synthesise one.
- Measurements are interaction observations, not microbenchmarks.

After the readiness gates, the final tree carries 1,563 workspace tests and 219
`orbit-tui` tests (the added readiness/guard tests), `main.rs` at 196 lines, and
regenerated captures. PTY remains 54 passed/1 skipped; contracts 43;
documentation 39; M15 45/47 for the withheld release workflow. The detail model
is only seeded while the docked pane can be painted.

Final close-out note: after the readiness gates the tree carries 1,563 workspace tests and 219 `orbit-tui` tests (the added readiness/guard tests), `main.rs` at 196 lines, and regenerated captures. PTY remains 54 passed/1 skipped; contracts 43; documentation 39; M15 45/47 for the withheld release workflow. The detail model is only seeded while the docked pane can be painted.
