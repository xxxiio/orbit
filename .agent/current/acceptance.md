---
template: acceptance-report
template_version: 2.0.0
template_digest: "cf2288fa6e58729b6f3b34306207c7168c34b100111c216cda2530187e055f83"
project_owned: true
record_contract: acceptance-report
record_schema_version: 1
status: passed
task_id: TASK-0004
---

# Current Acceptance Report

## Task

TASK-0004 — M28 project-centric TUI and blueprint UI composition.

## Gate results

| Gate ID | Gate | Status | Evidence |
|---|---|---|---|
| G-0001 | Blueprint view schema and validation | pass | `orbit-blueprint` 20 tests; legacy, duplicate, unknown, binding and bundled-view cases |
| G-0002 | Shared registry, resolution and forms | pass | `orbit-ui` 14 tests; schema drift, registration, sorting/limits, missing components, form/action contracts |
| G-0003 | Application view data and shared backend | pass | `orbit-client` 305 tests; local/connected project/workspace data, blueprint work items, pending/outbox separation |
| G-0004 | Project-centric shell/responsive rendering | pass | `orbit-tui` 125 tests; 60×18/80×24/100×30/120×35/160×45, readable Kanban window, table/list/detail sizing |
| G-0005 | Interactions and duplicate safety | pass | Project/task/work-item forms, status/dependency/blueprint actions, palette/search cancel, duplicate submission suppression |
| G-0006 | Authorization and architecture boundary | pass | Blueprint declarations do not grant actions; no SQL/process in TUI; all mutations through client/backend services |
| G-0007 | Workspace regression | pass | `cargo test --workspace --offline`: 1,350 passed, 0 failed |
| G-0008 | Format and lint | pass | `cargo fmt --all -- --check`; clippy `-D warnings` on affected crates |
| G-0009 | Documentation contract | pass | `python3 -m unittest discover -s tests/documentation`: 29 passed |
| G-0010 | M15 full local profile | pass | `./scripts/m15-full-automated.sh --profile full`: 42 passed, 0 failed, 0 not_run; `.agent/evidence/m28-m15-final.zip` |
| G-0011 | Local PTY acceptance | pass | 36 local PTY items: quit/confirm/restoration, help/history, project/task lifecycle + restart, blueprint bind/work-item edit/transition + restart, responsive sizes |
| G-0012 | Connected PTY acceptance | pass | Disposable server: pending project/task → `f` flush → server acceptance/refresh; earliest+estimate verified; Agents explicitly marks jobs not projected |
| G-0013 | Local/connected shared shell | pass | Same `App`/renderer over `backend.rs`; legacy connected surface opt-in `ORBIT_TUI_LEGACY_CONNECTED=1`; unsupported connected surfaces fail closed |

## Commands run

```text
cargo test -p orbit-blueprint -p orbit-ui -p orbit-client -p orbit-tui --offline
cargo test --workspace --offline
cargo clippy -p orbit-tui -p orbit-client -p orbit-ui --all-targets --offline -- -D warnings
cargo fmt --all -- --check
cargo build -p orbit-tui -p orbit-server --offline
python3 -m pytest -q tests/acceptance/tui
python3 -m unittest discover -s tests/documentation
./scripts/m15-full-automated.sh --profile full
```

## Checks not run

- Live Herdr/agent review/resume: inspectable jobs are supported, but review/resume remains disabled without an explicitly configured runtime or connected review service.
- Connected multi-command atomicity is best-effort; a later rejected intent can leave earlier intents queued (documented outbox/CLI recovery).
- Real public release installation/update (M27 external gate).

## Waivers

- None. Remaining connected atomicity and agent review/resume are documented limitations, not silently accepted defects.

## Residual risk

- Connected blueprint/work-item, habit, metric and note editing remain on the explicitly available legacy command surface.
- PTY evidence covers real local and connected project/task workflows, not an external agent runtime.
- Terminal-buffer/PTY tests cover representative emulators, not every terminal.
