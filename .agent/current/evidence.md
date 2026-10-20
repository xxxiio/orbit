---
template: evidence-manifest
template_version: 1.0.0
template_digest: "2787d570257f4ffbf8add20f62aa21227a4aea1a6723583e8928b5e71e09f79b"
project_owned: true
record_contract: evidence-manifest
record_schema_version: 1
status: incomplete
task_id: TASK-0004
---

# Evidence Manifest

## Task

TASK-0004 — M28 project-centric TUI and blueprint UI composition.

## Acceptance evidence

| Criterion ID | Status | Evidence |
|---|---|---|
| AC-001: Shared view schema and validation | pass | `crates/orbit-blueprint/src/presentation.rs`; 5 presentation tests plus existing blueprint suite (20 total) |
| AC-002: Project-centric shell | pass | `crates/orbit-tui/src/{main,app,render,theme}.rs`; sidebar/workspace/details and responsive tests |
| AC-003: Real application data | pass | `crates/orbit-client/src/view_data.rs`; project/workspace snapshots, readiness, work items, milestones, knowledge, habits, metrics, schedule, agent jobs |
| AC-004: Registered components | pass | `crates/orbit-ui/src/registry.rs`, `resolve.rs`, `model.rs`; 13 component types and renderer bodies |
| AC-005: Blueprint views and legacy defaults | pass | bundled v2 JSON, generated default views, client fallback test, pinned work-item transition test |
| AC-006: Contextual actions and feedback | pass | `orbit-ui/src/actions.rs`, TUI action menu/palette, duplicate-submission test, accepted/rejected/stale/unavailable states |
| AC-007: Tests | pass | `.agent/evidence/m28-focused-tests.txt`, `.agent/evidence/m28-workspace-tests.txt`; 1,306 workspace tests |
| AC-008: Documentation | pass | TUI/blueprint/UI docs; `tests/documentation` 29 tests |

## Quality gate evidence

| Gate | Status | Command or artefact | Result |
|---|---|---|---|
| Focused M28 tests | pass | `cargo test -p orbit-blueprint -p orbit-ui -p orbit-client -p orbit-tui --offline` | 420 tests across focused crates; 0 failed |
| Workspace regression | pass | `cargo test --workspace --offline` | 1,306 passed, 0 failed |
| Clippy | pass | affected crates, `--all-targets -- -D warnings` | clean |
| Format | pass | `cargo fmt --all -- --check` | clean |
| Documentation | pass | `python3 -m unittest discover -s tests/documentation` | 29 passed |
| Interactive terminal smoke | pass | `.agent/evidence/m28-terminal-smoke.txt` | 140x40 and 60x18; generated fallback and development@2 views |
| Full M15 local profile | pass | `./scripts/m15-full-automated.sh --profile full` (2026-10-20T111949Z) | 42 passed, 0 failed, 0 not_run; `.agent/evidence/m28-m15-final.zip` |
| Connected read-only project views | pass | `connected.rs` `V` over `orbit_ui::project_from_snapshot`; 2 connected tests | accepted snapshot only; pending excluded |
| Full connected interactive parity | not-run | connected mutation surface remains command-centric | future work |

## Checks not run

- Real connected-server parity for the shared renderer.
- Real public release installation/update.

## Waivers

- None.

## Residual risk

- Connected TUI has not adopted the shared renderer.
- Local agent review is inspect-only because no accepted review surface is
  exposed.
- Screenshots are not claimed; terminal-buffer tests and the TUI guide describe
  verified behavior.
