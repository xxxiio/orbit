---
template: acceptance-report
template_version: 2.0.0
template_digest: "cf2288fa6e58729b6f3b34306207c7168c34b100111c216cda2530187e055f83"
project_owned: true
record_contract: acceptance-report
record_schema_version: 1
status: incomplete
task_id: TASK-0004
---

# Current Acceptance Report

## Task

TASK-0004 — M28 project-centric TUI and blueprint UI composition.

## Gate results

| Gate ID | Gate | Status | Evidence |
|---|---|---|---|
| G-0001 | Blueprint view schema and validation | pass | `orbit-blueprint` 20 tests; legacy, duplicate, unknown, binding and bundled-view cases |
| G-0002 | Shared registry and resolution | pass | `orbit-ui` 13 tests; schema drift, registration, sources, sorting/limits, missing component, generated defaults |
| G-0003 | Application view data | pass | `orbit-client` 305 tests including project/workspace view data and blueprint work-item scoping |
| G-0004 | Project-centric shell and responsive rendering | pass | `orbit-tui` 82 tests; wide/narrow buffers, sidebar/workspace/details, focus, tables, boards, metrics |
| G-0005 | Interactions and duplicate safety | pass | Project switching, palette/search cancel, work-item transition, duplicate habit test, quit/back behavior |
| G-0006 | Authorization and architecture boundary | pass | Blueprint declarations do not grant actions; TUI has no SQL/process use; all mutations are `OrbitClient` calls |
| G-0007 | Workspace regression | pass | `cargo test --workspace --offline`: 1,306 passed, 0 failed |
| G-0008 | Format and lint | pass | `cargo fmt --all -- --check`; clippy `-D warnings` on affected crates |
| G-0009 | Documentation contract | pass | `python3 -m unittest discover -s tests/documentation`: 29 passed |
| G-0010 | M15 full local executable profile | pass | `./scripts/m15-full-automated.sh --profile full` (2026-10-20T111949Z): 42 passed, 0 failed, 0 not-run; `.agent/evidence/m28-m15-final.zip` |
| G-0011 | Interactive wide/narrow terminal smoke | pass | `.agent/evidence/m28-terminal-smoke.txt`; project views, generated fallback, help, view switch and quit verified |
| G-0012 | Connected read-only project views | pass | `V` resolves accepted replica data through `orbit-ui` generated views; 2 connected tests; pending commands remain separate |
| G-0013 | Full connected project-centric shell parity | not-run | Connected mutations remain command intents; interactive shell parity is future work (TODO-0007) |

## Commands run

```text
cargo test -p orbit-blueprint -p orbit-ui -p orbit-client -p orbit-tui --offline
cargo test --workspace --offline
cargo clippy -p orbit-blueprint -p orbit-ui -p orbit-client -p orbit-tui -p orbit-storage -p orbit-runtime --all-targets --offline -- -D warnings
cargo fmt --all -- --check
python3 -m unittest discover -s tests/documentation
```

## Checks not run

- Full interactive connected-mode shell parity; connected mode has read-only
  project views but its mutation surface remains command-centric.
- Live agent handoff review from the local TUI (no accepted local review surface
  is exposed).
- Real public release installation/update (M27 external gate).

## Waivers

- None for M28 source-level acceptance. Connected renderer adoption and local
  agent review are explicit remaining work, not waived defects.

## Residual risk

- Shared contracts are suitable for connected/Flutter clients, but only the
  local TUI renderer consumes them today.
- Knowledge/habit views reference global domain records; documentation states
  this rather than claiming per-project ownership.
- Terminal-buffer tests cover representative layouts, not every terminal
  emulator.
