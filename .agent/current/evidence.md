---
template: evidence-manifest
template_version: 1.0.0
template_digest: "2787d570257f4ffbf8add20f62aa21227a4aea1a6723583e8928b5e71e09f79b"
project_owned: true
record_contract: evidence-manifest
record_schema_version: 1
status: final
task_id: TASK-0007
---

# Evidence Manifest

## Task

TASK-0007 — M29 TUI product experience, interaction quality and maintainability.

## Acceptance evidence

| Criterion ID | Status | Evidence |
|---|---|---|
| AC-001: Selection and scroll restore | pass | `crates/orbit-tui/src/ui_state.rs` schema v1 + 18 state tests (corrupt/partial/newer files, view-wide id lookup, skipped restore); `tests/acceptance/tui/test_m29_selection.py` (3 passed) |
| AC-002: Focus and context | pass | `m28_navigation_tests`, `m29_layout_presentation_tests`; PTY Scenarios A/C/F |
| AC-003: Action bar/menu/palette | pass | `m29_layout_presentation_tests` compact-bar, disabled-reason, palette-ranking, MRU and refusal tests; captures `100x30-action-menu.txt`, `100x30-palette-query.txt` |
| AC-004: Quick edit and forms | pass | `m29_layout_presentation_tests` quick-edit/field-order/validation/multiline tests; PTY Scenario B |
| AC-005: Responsive layout and board | pass | `m29_layout_presentation_tests` five sizes, kanban windowing/ownership, table priority, detail clamp; PTY Scenario D at 5 sizes; captures `60x18-board.txt`, `160x45-board.txt` |
| AC-006: No-colour theme | pass | `theme.rs` glyph completeness/distinctness tests; capture `60x18-home.txt`; `NO_COLOR` render test |
| AC-007: Connected consolidation | pass | `m29_gate3_remediation_tests`, `m29_connected_task_detail_tests`, `app/tests.rs` outcome tests; PTY `test_m29_connected_recovery.py`; `test_m28_connected.py` console/inspector; captures `100x30-pending-inspector.txt`, `100x30-connected-console.txt`; `connected.rs` deleted |
| AC-008: Maintainability | pass | `main.rs` 196 lines; `input/`, `app/`, `render/` module trees; contract guards for single dispatch and authority boundaries |
| AC-009: Realistic workspace and performance | pass | `scripts/tui-fixture.py` (20 projects, 200 tasks, 1,017 activities, 6 attachments, 8 agent jobs, 5 pinned blueprint versions); `.agent/evidence/m29-perf-baseline.{md,json}` and `m29-perf-after.{md,json}`; `test_m29_fixture.py` |
| AC-010: PTY scenarios A–G and docs | pass | `python3 -m pytest -q tests/acceptance/tui` 54 passed/1 skipped; `ORBIT_TUI_FULL_FIXTURE=1 ...` 2 passed; docs rewritten; 21 captures |

### Performance record

- Cold start to first frame: 100→101 ms at 100x30 and 102→102 ms at 160x45 (flat within noise; the recorded 96 ms baseline did not reproduce).
- Project switch: 83 ms (100x30) and 88 ms (160x45), unchanged.
- Open task detail: 4→4 ms at 100x30; 8→5 ms at 160x45.
- Switch blueprint view: 21→21 ms at 100x30; 94→78 ms at 160x45.
- Search open plus first result: 60→60 ms at 100x30; 62→62 ms at 160x45.
- Scroll 20 rows: 105→104 ms at 100x30; 108→108 ms at 160x45.
- Save quick edit: 173→83 ms at 100x30; 173→87 ms at 160x45.
- Refresh project: 20→20 ms at 100x30; 26→26 ms at 160x45.

Unchanged operations are honest non-improvements: scrolling is dominated by
harness key pacing, project switching by key delays plus two required
resolutions, and cold start by process spawn.

## Quality gate evidence

| Gate | Status | Command or artefact | Result |
|---|---|---|---|
| Workspace tests | pass | `cargo test --workspace --offline` | 1,563 passed, 0 failed (29 binaries) |
| TUI tests | pass | `cargo test -p orbit-tui --offline` | 219 passed |
| Client tests | pass | `cargo test -p orbit-client --offline` | 348 passed |
| UI tests | pass | `cargo test -p orbit-ui --offline` | 28 passed |
| Lint | pass | `cargo clippy ... --all-targets -- -D warnings` | clean on 12 crates |
| Format | pass | `cargo fmt --all -- --check` | clean |
| Documentation tests | pass | `python3 -m unittest discover -s tests/documentation` | 39 passed |
| Architecture contracts | pass | `python3 -m pytest -q tests/contracts` | 43 passed |
| TUI acceptance | pass | `python3 -m pytest -q tests/acceptance/tui` | 54 passed, 1 skipped |
| Full fixture | pass | `ORBIT_TUI_FULL_FIXTURE=1 python3 -m pytest -q tests/acceptance/tui/test_m29_fixture.py` | 2 passed |
| Captures | pass | `python3 scripts/tui-captures.py` | 21 written, 0 skipped |
| M15 profile | pass | `./scripts/m15-full-automated.sh --profile full` | 45/47; only withheld release-workflow checks fail |

## Checks not run

- Live Herdr/model acceptance (authorization required; no token spend).
- Public release install/update (M27 external gate).
- Per-device ACL and server-side attachment GC (unchanged product follow-ups).

## Waivers

- None.

## Residual risk

- The withheld release workflow keeps two M15 checks red.
- PTY measurements are interaction-latency observations, not microbenchmarks.
- The connected console is the only surface for several advanced record
  families; discoverability is lower than a structured form.
