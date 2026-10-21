---
template: project-status
template_version: 2.0.0
template_digest: "bae979b223043d02aa7be0cacebed8dd05241d5cc9f0d330796a7c919fbbe019"
project_owned: true
record_contract: project-status
record_schema_version: 1
status: current
---

# Project Status

## Current state

Orbit has a functionally accepted project-centric TUI. One shell and renderer
serve local and connected deployments through a shared backend, with structured
project/task/work-item forms, predictable quit/back/navigation, RAII terminal
restoration, readable responsive layouts, connected pending/accepted separation
and real PTY acceptance. M28 is complete; M27's real public-release verification
remains blocked on external authorization.

## Current milestone

MILESTONE-0028 — M28 Project-Centric TUI and Blueprint UI Composition, completed.
All eight source-level exit criteria and the functional acceptance scenarios are
met, with documented residual limitations.

## Recently completed

- Critical interaction recovery: centralized q/Ctrl+Q/Ctrl+C quit, unsaved-input
  confirmation, Esc history, per-view/component/id selection, RAII terminal guard.
- Structured forms for project/task/work-item/blueprint/metric/evidence workflows,
  validation, persistence and restart tests.
- Shared local/connected shell over `orbit-tui/src/backend.rs`; connected project
  and task intents remain pending until `f` flush/server acceptance.
- Responsive content-aware layout, readable Kanban window, table/list sizing,
  detail scrolling, forced `Ctrl+Q` footer.
- Agent job inspection with handoff/review state; review/resume disabled without
  an authorized runtime.
- Maintained stdlib PTY harness: 37 scenarios, disposable local databases and a
  real connected server.

## In progress

- None. Connected multi-command atomicity and agent review/resume are documented
  follow-ups, not active work.

## Blocked

- M27 real GitHub Release installation/update verification: the local account is
  read-only, no release is published, and publication needs maintainer approval.

## Key decisions

- `App`/renderer is authority-agnostic; local and connected differences live in
  `backend.rs`, never in view rendering.
- Connected mutations keep the accepted-vs-pending distinction and use the
  durable outbox; unsupported connected edit surfaces fail closed with a reason.
- The original connected command palette remains opt-in through
  `ORBIT_TUI_LEGACY_CONNECTED=1` while connected blueprint/note/habit parity is
  completed.
- Legacy blueprints receive generated views; no migration.
- Blueprint declarations are presentation metadata, not authority.
- Agent job completion never completes an Orbit task.

## Project health

- Build: `cargo check --workspace --all-targets --offline` pass.
- Tests: `cargo test --workspace --offline` 1,350 passed, 0 failed.
- Focused: `orbit-tui` 125, `orbit-ui` 14, `orbit-client` 305.
- PTY: `python3 -m pytest -q tests/acceptance/tui` 37 passed (local + connected).
- Format/lint: fmt clean; clippy `-D warnings` on affected crates.
- Documentation: 29 tests pass.
- Acceptance: `./scripts/m15-full-automated.sh --profile full` 42 passed, 0 failed,
  0 not-run; `.agent/evidence/m28-m15-final.zip`.
- Data integrity: no schema change; storage/regression suites pass.
- Interactive evidence: `.agent/evidence/m28-terminal-smoke.txt` and PTY evidence
  under `target/tui-evidence/`.

## Risks

- Connected multi-command edits are non-atomic; a later rejection can leave earlier
  intents queued. The UI never presents them as accepted and documents recovery.
- Connected blueprint/work-item/habit/metric/note editing remains on the legacy
  command surface.
- Agent review/resume requires an explicitly configured runtime or connected
  review service.
- Public release remains unverified; M27 is blocked externally.
- Terminal tests cover representative emulators, not every terminal.

## Remaining milestones

1. MILESTONE-0027 — blocked on authorized external release verification.
2. MILESTONE-0025 — deferred (EC-003 through EC-006).

## Next deterministic step

NEXT-0008 (blocked) — authorized real GitHub Release publication and installation
verification for M27.
