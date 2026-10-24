---
template: current-task
template_version: 2.0.0
template_digest: "179af91e65608fbfd07b28126ed729856e6524410956892a698b3f40e902e695"
project_owned: true
record_contract: current-task
record_schema_version: 1
id: TASK-0007
status: completed
milestone_id: MILESTONE-0029
---

# Task: M29 TUI product experience, interaction quality and maintainability

## Objective

Turn the already-functional Orbit TUI into a coherent daily-use terminal
application: restore the working context, make actions discoverable, make forms
safe and efficient, keep boards/tables/details readable at every supported
terminal size, serve local and connected authority from one shell, and split the
oversized modules with measured interaction improvements.

## Current behaviour

M28 completed project-centric navigation, declarative blueprint views, rich task
editing, acceptance criteria, attachments, agent-job inspection, contextual
actions, the command palette, search and connected command durability. Daily use
still exposed interaction defects: `ui_state` persisted `selected_id` as `None`;
`main.rs` (148 KB) and `render.rs` (78 KB) concentrated unrelated
responsibilities; the legacy connected TUI remained reachable through
`ORBIT_TUI_LEGACY_CONNECTED`; the action bar repeated keys and omitted status; the
palette had no ranking, recents or disabled guard; forms buried priority behind
planning fields and reported validation only as a banner; the shell used fixed
sidebar/detail widths and boxed Kanban columns; and the 20-project interaction
path was unmeasured.

## Desired behaviour

A restart or view switch returns to the same project, view, component, selected
entity and scroll position, with safe fallback when the entity or view is gone.
The focused region, selected item and available high-value actions are obvious.
Quick edits avoid advanced fields; forms preserve input and point at the failing
field. Layout adapts from 60x18 to 160x45 with component-aware minimums and a
horizontally windowed board. One shell handles local and connected operation;
pending commands and conflicts are inspectable, recoverable and never presented
as accepted; the legacy connected presentation is gone without losing the
command capability. The event loop, keyboard dispatch, form/modal ownership and
render layout are separated, and measured interaction latency improves.

## Scope

- Durable navigation state: project/view/focus/component, stable selected id,
  index and scroll offsets, with id-to-row resolution, clamping, corrupt-file
  tolerance and a restore-skipped marker.
- Ranked compact action bar, grouped action menu with disabled reasons,
  context-aware hotkeys, ranked palette with MRU destinations and disabled
  refusal, `E` quick edit, reordered task forms, field-level validation and
  multiline caret presentation.
- Component-aware responsive layout, Kanban header/rule/card presentation with
  windowing and ownership-aware selection, table column priority, clamped detail
  scroll, colour-independent theme with complete status glyphs.
- Shared-shell connected pending inspector (`!`), conflict review with
  fail-closed replacement, connected console covering the legacy record grammar,
  accepted routine/strategy/milestone/schedule reads; deletion of the legacy
  connected TUI and event loop.
- Module extraction into `app/`, `input/`, `render/`, `connected_ops.rs`,
  `local_backend.rs`; detail-model cache; title-only quick-edit refresh.
- Deterministic 20-project/200-task/1,017-activity fixture, PTY perf probe,
  before/after measurements, PTY scenarios A–G, documentation and captures.

## Out of scope

- Rebuilding M28 blueprint presentation, Basic workflow, attachments, criteria,
  backend abstractions, quit handling or the PTY harness.
- Domain models for visual concerns; direct SQL in the TUI; asynchronous runtime.
- Flutter renderer, plugin marketplace or dynamic blueprint code execution.
- Per-device project ACLs and server-side attachment GC (unchanged follow-ups).
- Public release verification (M27 external gate).

## Dependencies

- MILESTONE-0028 completed product capabilities and regression tests.
- Local `orbit-server` binary for connected recovery acceptance.
- Existing `orbit-client` typed connected command/record APIs.

## Risks

- Module extraction could change privacy or ordering semantics; mitigated by
  verbatim moves and the full M28/M29 test suite after each move.
- Removing the legacy connected TUI could drop operations; mitigated by porting
  the record grammar, accepted reads and planning analyses before deletion.
- Cache optimisations could show stale accepted data; mitigated by invalidating
  at navigation/refresh/mutation chokepoints with regression tests.
- Layout changes could break narrow terminals; mitigated by PTY checks at every
  supported size.
- Performance claims could outrun evidence; mitigated by recorded before/after
  fixtures and honest treatment of PTY-dominated operations.

## Acceptance criteria

- [x] AC-001: Selected item, component, scroll and focus restore across view
  changes and restart; deleted/reordered items fall back safely; corrupt or
  newer-version state does not block startup.
- [x] AC-002: Focus, selection and context are preserved across modal open/close,
  resize, view switches and Esc; the focused region and selected row are
  unmistakable.
- [x] AC-003: The compact bar exposes at most three context-ranked actions with
  keys that actually run them; the menu and palette explain disabled actions;
  the palette ranks deterministically, remembers destinations and refuses
  disabled execution.
- [x] AC-004: `E` quick edit changes title and priority through existing
  commands; full task forms lead with everyday fields, keep input on error, mark
  and focus the failing field, and confirm unsaved changes.
- [x] AC-005: Layout adapts at 60x18, 80x24, 100x30, 120x35 and 160x45;
  Kanban windows columns, keeps the selected card visible and uses real
  workflow transitions; tables prioritise title/status/priority.
- [x] AC-006: The theme stays legible without colour; status glyphs are complete
  and distinct; empty states and action hints are accurate.
- [x] AC-007: One shell serves local and connected operation; `!` inspects
  pending commands, conflicts are reviewable and replaceable fail-closed, the
  console covers the legacy grammar and accepted record reads, and the legacy
  entry point/presentation is deleted.
- [x] AC-008: `main.rs` is a simple event loop, keyboard dispatch has one
  location, forms/modals/render have explicit owners, and prior tests pass.
- [x] AC-009: The 20/200/1,017 fixture exercises switching, editing, search,
  scrolling and detail inspection with recorded measurements; quick edit dropped
  from ~173 ms to ~83 ms and render performs no backend reads for the detail pane.
- [x] AC-010: PTY Scenarios A–G pass, documentation matches behaviour, captures
  exist, and no known blocking data-loss/authorization/everyday-use defect
  remains.

## Validation

```sh
cargo test --workspace --offline
cargo clippy --offline -p orbit-domain -p orbit-application -p orbit-storage \
  -p orbit-runtime -p orbit-sync -p orbit-blueprint -p orbit-client -p orbit-ui \
  -p orbit-tui -p orbit-mcp -p orbit-cli -p orbit-server --all-targets -- -D warnings
cargo fmt --all -- --check
python3 -m unittest discover -s tests/documentation
python3 -m pytest -q tests/contracts
python3 -m pytest -q tests/acceptance/tui
ORBIT_TUI_FULL_FIXTURE=1 python3 -m pytest -q tests/acceptance/tui/test_m29_fixture.py
python3 scripts/tui-fixture.py --out /tmp/opencode/m29-fixture --json
python3 tests/acceptance/tui/perf_probe.py --workspace /tmp/opencode/m29-fixture --repeat 5
python3 scripts/tui-captures.py
```
