---
template: journal-entry
template_version: 2.1.0
template_digest: "20e8b70b95080dbe3110fe7ad70c6bd0480c2a911af959019213e78b3a1b7814"
project_owned: true
record_contract: journal-entry
record_schema_version: 1
status: recorded
date: "2026-10-25"
task_id: TASK-0009
---

# Journal Entry: 2026-10-25 — Navigation rail fix and the remaining M25 criteria

## Context

M30 I1 had shipped the two-region workspace and trusted capability execution, but
the navigation rail did not reliably drive the workspace: on the Projects screen
`Enter` always opened the workspace's selected project row, and activating a
top-level rail destination dropped focus into the workspace, so moving the rail
afterwards did nothing. The user reported the right pane appearing stuck. M25
EC-004 (connected MCP outbox parity), EC-005 (previewable upgrades) and EC-006
(habit recurrence ownership) were still open.

## Work completed

- Fixed the rail: `Enter` activates the focused rail row on every screen;
  activating Home/Inbox/Agents/Projects keeps the rail focused so `j`/`k`
  continues to drive the right pane; opening a project still hands the keyboard
  to the workspace. Four regression tests and a TUI-guide correction.
- Connected MCP blueprint writes now queue the frozen route/body through
  `queue_blueprint_transport` before attempting the canonical flush and report
  `accepted`/`pending`/`conflict` from the outbox row, with immutable receipts,
  offline replay and lost-ack recovery; no direct connected write `.post(` remains.
- Added previewable blueprint upgrades: a typed diff of kinds/states/fields/
  transitions/actions/capabilities/views with warning/blocker classification and
  a SHA-256 fingerprint over the diff plus live item state; `apply` requires
  `--confirm` and the exact fingerprint, recomputes in one savepoint, refuses
  blockers/mismatch, preserves items and is idempotent. Publish/install never
  rebind. CLI `blueprint upgrade-preview|upgrade`; connected mode refuses.
- Moved habit recurrence/streak evaluation into the opt-in
  `orbit-habit-capability` package (`orbit.habit.recurrence`), registered in the
  trusted composition but not installed/authorized by default; core domain keeps
  only occurrence/period types, and application/project-evidence reads return an
  explicit unavailable status until installed (and granted for project evidence).

## Decisions made

- The rail keeps focus for top-level destinations (browsing) but project
  activation moves into the workspace, matching the documented M29 contract and
  keeping the workspace immediately usable.
- Connected writes are queued first and the disposition is read back from that
  exact outbox row; an unobservable outcome is `pending`, never `accepted`.
- Upgrades are fail-closed: removed kinds/states with live items, incompatible
  field-type changes and missing required capabilities are blockers, not silent
  remaps; an already-pinned target is an idempotent success.
- Habit recurrence is opt-in and honest: without the capability the projection
  reports unavailable rather than falling back to core math; existing habit
  counts and last-occurrence data are untouched.

## Files or components changed

- `crates/orbit-tui/src/{input/dispatch.rs,app/navigation.rs}`, new
  `tests/m30_sidebar_navigation_tests.rs`, `docs/guides/tui-workspace.md`.
- `crates/orbit-mcp/src/{blueprint_tools.rs,main.rs}`,
  `crates/orbit-client/src/{unified.rs,connected.rs}`,
  `tests/acceptance/mcp/test_m30_blueprint_outbox.py`, MCP reference docs.
- `crates/orbit-runtime/src/blueprint_upgrade.rs` (+ tests),
  `crates/orbit-cli/src/{blueprint_commands.rs,connected_blueprint.rs}`,
  `tests/acceptance/local_cli_workflows.py`, upgrade docs.
- New `crates/orbit-habit-capability/`, `crates/orbit-runtime/src/habit_capability.rs`;
  `crates/orbit-domain/src/habit_tracking.rs`, `crates/orbit-application/src/{habit_occurrence_queries.rs,project_evidence_queries.rs,project_health_queries.rs}`,
  `crates/orbit-progress/src/lib.rs`, habits guide.

## Validation performed

- `cargo fmt --all -- --check` clean; clippy `-D warnings` clean across the 14
  product crates including the new package.
- `cargo test --workspace --offline` — 1,658 passed, 0 failed (31 binaries).
- `cargo test -p orbit-runtime blueprint_upgrade` — 13 passed; `orbit-mcp` 76;
  `orbit-cli` 128; `orbit-habit-capability` 5; `orbit-application` 17;
  `orbit-domain` 56; `orbit-tui` 250; `orbit-client` 348.
- `python3 -m pytest -q tests/acceptance/tui tests/acceptance/mcp tests/contracts tests/documentation`
  plus the capability workflow — 151 passed, 1 skipped after the capability-list
  assertion was updated for the new opt-in package.
- `python3 tests/acceptance/local_cli_workflows.py --binary target/debug/orbit`
  — 60 passed, including preview → refusal → apply → restart persistence.
- `getdone-validate-project` — passed.

## Problems encountered

- The capability acceptance test assumed the capability list contained exactly
  one row; the newly composed (but uninstalled) habit capability made it two. The
  test now selects the reference capability by id and additionally asserts the
  habit capability is present and unavailable, which is a stronger check.
- Concurrent lanes briefly produced transient compile failures across shared
  crates; all reported runs are from the converged tree.

## Remaining work

- Connected capability execution and MCP capability tools remain follow-ups.
- Per-device project ACLs, server attachment GC and live Herdr acceptance remain
  documented follow-ups.
- M27 public release verification remains externally blocked.

## New TODOs

- TODO-0015: connected capability execution parity (execute through the server
  outbox once a transport exists).
- TODO-0016: expose capability status/state in a dedicated TUI screen when a
  second provider exists.

## Risks or blockers

- Habit streaks/evidence are unavailable until the capability is installed and
  granted; this is intended and documented.
- Upgrade blockers intentionally refuse incompatible live data rather than
  remapping it.
- M27 release work is externally blocked.
