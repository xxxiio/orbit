---
template: acceptance-report
template_version: 2.0.0
template_digest: "cf2288fa6e58729b6f3b34306207c7168c34b100111c216cda2530187e055f83"
project_owned: true
record_contract: acceptance-report
record_schema_version: 1
status: passed
task_id: TASK-0007
---

# Current Acceptance Report

## Task

TASK-0007 — M29 TUI product experience, interaction quality and maintainability.

## Gate results

| Gate ID | Gate | Status | Evidence |
|---|---|---|---|
| G-0001 | Navigation and selection restoration | pass | `ui_state` schema v1 persists project/view/focus/component/stable id/index/scroll; restore resolves the id across the view, falls back to a clamped index, preserves a skipped restore and ignores corrupt/newer files. 18 state tests + PTY Scenario A (3 tests). |
| G-0002 | Contextual actions and palette | pass | Compact bar shows three context-ranked actions with keys that resolve to the action they advertise; grouped menu with disabled reasons; palette prefix/word/fuzzy ranking, 5-entry destination MRU, context boost, disabled refusal. 13 Rust tests + PTY Scenario F. |
| G-0003 | Quick edit and forms | pass | `E` edits title/priority through existing commands; task forms lead with title/priority/description/labels; range/timestamp errors focus and mark the field; multiline caret window; discard confirmation preserved. 9 Rust tests + PTY Scenario B. |
| G-0004 | Responsive layout and board | pass | Component minimums, content-weighted allocation and single-component rail at 60x18/80x24/100x30/120x35/160x45; Kanban header/rule/card, horizontal windowing and ownership-aware selection; priority table columns; clamped detail scroll. 8 layout tests + PTY Scenario D (5 sizes). |
| G-0005 | Theme and no-colour legibility | pass | Restrained palette; selection/focus carry `▸`, fill and border weight; complete distinct status glyphs for task/project/agent/workflow states; `NO_COLOR`/`TERM=dumb` tested. 3 theme tests. |
| G-0006 | Shared connected shell and legacy removal | pass | `!` pending inspector, conflict review with fail-closed replacement, connected command console covering the record grammar/batch/resolve/sync/notes/planning and accepted routine/strategy/milestone/schedule reads. `connected.rs` (1994 lines) and `ORBIT_TUI_LEGACY_CONNECTED` deleted. Real-server PTY Scenario E. |
| G-0007 | Maintainability extraction | pass | `main.rs` 196 lines (wiring, `run`, one event loop, test declarations); keyboard dispatch only in `input/dispatch.rs`; `app/{state,navigation,selection,actions,forms,connected}`; `render/{layout,shell,components,modals}`; local construction isolated in `local_backend.rs`. Contract tests enforce the boundaries. |
| G-0008 | Measured performance | pass | 20/200/1,017 fixture + PTY probe recorded baseline and after tables; detail model cached (render performs zero backend reads for the docked pane); title-only quick edit avoids a whole-workspace reload. Quick edit 173→83 ms (100x30), 173→87 ms (160x45); detail open 7.6→5.3 ms and board view switch 93.5→77.6 ms at 160x45; other operations unchanged within noise. |
| G-0009 | PTY scenarios A–G | pass | `tests/acceptance/tui`: 54 passed, 1 skipped; full fixture opt-in 2 passed. Scenario A (`test_m29_selection.py`), B/C/D/F (`test_m29_workflows.py`), E (`test_m29_connected_recovery.py`), G (`test_m29_fixture.py` + `perf_probe.py`). |
| G-0010 | Quality gates | pass | workspace 1,563 passed; orbit-client 348; documentation 39; contracts 43; clippy `-D warnings` clean; fmt clean. M15 profile 45/47 with the two withheld release-workflow checks. |
| G-0011 | Documentation and captures | pass | User guide, TUI contract, developer component guide and acceptance README rewritten to shipped behaviour; `scripts/tui-captures.py` produced 21 real screen captures at 60x18/100x30/160x45. |

## Commands run

```text
cargo test --workspace --offline                          # 1,563 passed, 0 failed (29 binaries)
cargo clippy --offline -p orbit-domain -p orbit-application -p orbit-storage \
  -p orbit-runtime -p orbit-sync -p orbit-blueprint -p orbit-client -p orbit-ui \
  -p orbit-tui -p orbit-mcp -p orbit-cli -p orbit-server --all-targets -- -D warnings
cargo fmt --all -- --check
python3 -m unittest discover -s tests/documentation       # 39 passed
python3 -m pytest -q tests/contracts                      # 43 passed
python3 -m pytest -q tests/acceptance/tui                 # 54 passed, 1 skipped
ORBIT_TUI_FULL_FIXTURE=1 python3 -m pytest -q tests/acceptance/tui/test_m29_fixture.py  # 2 passed
python3 scripts/tui-fixture.py --out /tmp/opencode/m29-fixture --json   # 20/200/1,017
python3 tests/acceptance/tui/perf_probe.py --workspace /tmp/opencode/m29-fixture --repeat 5
python3 scripts/tui-captures.py                           # 21 written
./scripts/m15-full-automated.sh --profile full            # 45 passed, 2 failed
```

### M15 profile observation

47 checks: 45 passed. The two failures are `release-workflow-contract` and
`python-contract-tests`; both read `.github/workflows/release.yml`, which is
intentionally withheld pending the OAuth workflow-scope fix recorded in M27/M28.
`python-contract-tests` reports 103 passed and 12 errors, all in
`tests/packaging/test_release_workflow.py`. Every product, server, blueprint,
offline-authority, connected and agent check passed. Evidence:
`orbit-m15-evidence-20261024T123449Z.zip`.

## Checks not run

- Live Herdr/model agent acceptance (requires authorization and token spend;
  the scripted Scenario C path still passes).
- Per-device project ACLs: the authority remains single-tenant (TODO-0013).
- Server-side attachment garbage collection (no endpoint; TODO-0014).
- Public release installation/update (M27 external gate).

## Waivers

- None. The withheld release-workflow checks and the single-tenant limitation
  are documented limitations, not waived requirements.

## Residual risk

- Connected multi-command edits are not atomic; the pending inspector and
  console report per-command state.
- The connected console exposes the legacy command grammar for record families
  that have no structured form yet; it is one modal in the shared shell but less
  discoverable than a purpose-built form.
- Conflict replacement requires a user-supplied replacement command; the UI
  fails closed but cannot synthesise a valid replacement.
- Interaction measurements are PTY wall-clock medians, not a microbenchmark;
  process spawn and terminal reconstruction dominate cold start and key-paced
  scrolling.
