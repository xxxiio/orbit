---
template: journal-entry
template_version: 2.1.0
template_digest: "20e8b70b95080dbe3110fe7ad70c6bd0480c2a911af959019213e78b3a1b7814"
project_owned: true
record_contract: journal-entry
record_schema_version: 1
status: recorded
date: "2026-10-22"
task_id: TASK-0004
---

# Journal Entry: 2026-10-22 — M28 functional TUI recovery

## Context

The previous M28 claim was architectural only. Real terminal use exposed traps:
`q` did not quit from projects, Ctrl+Q was swallowed by text inputs, terminal
cleanup was not exception-safe, layouts collapsed or squeezed Kanban columns,
and project/task/work-item workflows still required CLI or encoded prompt strings.
M28 was reopened and verified with a real PTY harness, not renderer snapshots.

## Work completed

- Central quit path: q, Ctrl+Q, Ctrl+C; dirty-form/palette/search confirmation;
  Esc navigation history; RAII `TerminalSession` for raw mode/alternate
  screen/cursor; non-zero error exit.
- Per-view/component selection and scroll restore, including item-ID restore;
  always-visible quit footer; responsive breakpoints and content-aware heights.
- Structured forms: project create/settings, task create/edit/status/planning/
  scope/dependencies, work-item schema forms/edit/transition, blueprint install/
  bind, metric/evidence forms. Validation-first mutations and duplicate guard.
- Shared local/connected `App`/renderer over `orbit-tui/src/backend.rs`; connected
  project/task intents stay pending until `f` flush/server acceptance; connected
  refresh/pending/offline state is explicit; unsupported surfaces fail closed.
- Readable Kanban window, table width fixes, list ellipsis, detail scrolling; agent
  job inspection with handoff/review state; review/resume disabled with reason.
- Maintained stdlib PTY harness and connected disposable-server test.

## Decisions made

- One shell is authoritative for both modes; the legacy connected command palette
  remains opt-in via `ORBIT_TUI_LEGACY_CONNECTED=1` for unimplemented parity.
- Forms validate all inputs before mutating; follow-up connected intents are queued
  rather than dropped; connected multi-command edits remain non-atomic and are
  documented.
- Blueprint/action declarations never grant authority; connected writes remain
  outbox intents until server acceptance.
- Agent completion never completes a task; review/resume requires an authorized
  runtime/service.

## Files or components changed

- `crates/orbit-tui/src/{main,app,backend,render,theme,terminal,connected}.rs`
- `crates/orbit-client/src/{lib,view_data,contract,client_facade_tests}.rs`
- `crates/orbit-ui/src/{actions,binder,data,model,resolve,snapshot}.rs`
- `crates/orbit-blueprint/src/presentation.rs`
- `tests/acceptance/tui/*`, docs guides/reference/contract pages, `.agent` M28 records.

## Validation performed

- `cargo test --workspace --offline`: 1,350 passed.
- `orbit-tui` 125, `orbit-ui` 14, `orbit-client` 305.
- `python3 -m pytest -q tests/acceptance/tui`: 37 passed (36 local + real connected server).
- `./scripts/m15-full-automated.sh --profile full`: 42 passed, 0 failed, 0 not-run.
- clippy affected crates `-D warnings`, fmt, 29 documentation tests clean.
- `getdone-validate-project`: passed.

## Problems encountered

- The connected default initially showed accepted-looking status after queueing;
  fixed with pending disposition, header/banner, and early return before opening
  pending projects.
- Connected refresh used the cached snapshot; `refresh_accepted` now pulls before
  planning/view rebuild.
- Agent inspection action was previously permanently disabled; now inspect is
  enabled and review/resume is separately disabled with an explicit reason.
- Test harness screen reconstruction needed escape-sequence buffering; maintained
  PTY tests now use DB/server assertions for state truth.

## Remaining work

- Connected multi-command atomicity is best-effort; inspect/recover the outbox if
  a later intent is rejected.
- Connected blueprint/work-item, habit, metric and note editing remains on the
  legacy command surface.
- Agent review/resume needs an explicitly configured runtime or connected review
  service.
- Public release verification remains blocked under M27.

## New TODOs

- TODO-0008 remains open for agent handoff review/resume when an authorized local
  runtime or connected review service is configured.
- Connected blueprint/work-item/habit/metric/note parity and atomic multi-command
  admission remain follow-ups.

## Risks or blockers

- M27 external release authorization remains blocked.
- No known blocking local data-loss or authorization defect.
