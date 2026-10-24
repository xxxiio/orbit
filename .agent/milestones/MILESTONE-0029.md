---
template: milestone-plan
template_version: 2.0.0
template_digest: "737b1e57a6469c8843d3ed28294d21fe5a61750b2222084a7eaac19644fa392f"
project_owned: true
record_contract: milestone-plan
record_schema_version: 1
id: MILESTONE-0029
status: completed
---

# Milestone: TUI product experience, interaction quality and maintainability

## Intended outcome

The already-functional Orbit TUI is a coherent daily-use terminal application:
working context survives navigation, view changes and restarts; important
actions are discoverable without memorised shortcuts; forms are safe and
efficient; boards, tables and details stay readable at every supported terminal
size; one shared shell serves local and connected authority; and the oversized
TUI modules are split along cohesive responsibility boundaries with measured
interaction improvements.

## Why this milestone matters

M28 delivered the product capabilities. Daily use then exposed interaction
defects rather than missing features — most concretely, the selected item was
persisted as `None`, so a restart lost the user's working context. The connected
legacy TUI remained a second presentation implementation behind
`ORBIT_TUI_LEGACY_CONNECTED`, and `main.rs`/`render.rs`/`app.rs` concentrated
unrelated responsibilities. M29 fixed these without reopening M28 scope, adding
domain models for visual concerns, or weakening authority boundaries.

## Scope

- Stable selected-item and scroll restoration by entity identity, with nearest
  valid fallback and corrupt-state tolerance.
- Per-component navigation persistence, focus behaviour, view switching and
  back-navigation that preserve the originating context.
- Contextual, permission-aware action bar and ranked command palette with
  disabled explanations and recent destinations.
- Form focus, ordering, save/cancel, validation feedback, input preservation,
  unsaved-change confirmation and quick-edit paths using existing commands.
- Responsive layout with component-aware minimums at 60x18 through 160x45.
- Kanban horizontal viewport, readable columns, predictable movement and real
  blueprint transition semantics; responsive tables, lists and detail panes.
- Refined restrained theme with `NO_COLOR` and limited-colour support.
- Shared-shell parity for connected pending-command inspection, conflict review
  and offline recovery, then removal of the legacy connected TUI.
- Cohesive module extraction and measured performance improvements on a
  deterministic 20-project/200-task/1,017-activity fixture.
- Targeted PTY acceptance scenarios A–G, documentation and evidence.

## Non-goals

- Rebuilding blueprint presentation, the Basic workflow, attachments,
  acceptance criteria, backend abstractions, quit handling or the PTY harness.
- New domain models, direct SQL in the TUI, or a second persistence model.
- A Flutter renderer, plugin marketplace or dynamic blueprint code execution.
- Asynchronous runtime or caching justified only by theoretical latency.
- Changing application-level authorization or connected command semantics.

## Deliverables

- `ui_state` schema carrying project, view, focus, component, selected id,
  index and scroll offsets with safe restore and fallback.
- One keyboard dispatch location, owned form/modal handling and separated
  render modules.
- Contextual action bar, ranked palette, quick edit and predictable forms.
- Responsive shell, Kanban and table sizing, refined theme.
- Shared connected shell with pending/conflict recovery and no legacy entry.
- Module layout `app/`, `input/`, `render/` with backend, local backend,
  terminal, theme, ui_state and connected operations at the crate root.
- Deterministic large fixture, recorded before/after measurements and PTY
  scenarios A–G.

## Dependencies

- MILESTONE-0028 (completed product capabilities and regression tests).
- Local `orbit-server` binary for connected recovery acceptance.

## Risks

- Mechanical module extraction could change privacy or behaviour — mitigated by
  verbatim moves and the full suite after each move; contract guards now enforce
  single dispatch and authority boundaries.
- Removing the legacy connected TUI could drop operations — mitigated by porting
  the record grammar, accepted record reads and planning analyses before
  deletion, with a real-server recovery scenario.
- Layout changes could regress narrow terminals — PTY checks at every supported
  size and positive settled-frame waits.
- Performance claims could outrun evidence — recorded baseline and after tables,
  with honest non-improvements documented.

## Ordered implementation sequence

1. I1 — Stable selected-item restoration and navigation context preservation.
2. I2 — Contextual UX, discoverable actions, ranked palette and forms.
3. I3 — Responsive layout, Kanban navigation, table/detail density, theme.
4. I4 — Connected shared-shell parity, pending/conflict management, legacy
   connected presentation removal.
5. I5 — Module extraction, realistic fixture, measured performance work.
6. I6 — PTY scenarios A–G, documentation, terminal captures, final evidence.

## Exit criteria

- [x] EC-001: Selected item, component, scroll and focus restore across view changes and restart; deleted/reordered items fall back safely; corrupt state does not block startup.
- [x] EC-002: Focus is unmistakable, selection stays visible while scrolling, modal open/close preserves focus, resize preserves navigation, and Esc recovers the prior context.
- [x] EC-003: A small contextual action bar exposes high-value actions with reasons for disabled entries; the command palette ranks fuzzily and is context-aware.
- [x] EC-004: Forms show field focus, consistent tab order, save/cancel, per-field validation, preserved input after errors, safe multiline editing and unsaved-change confirmation; quick edit avoids traversing advanced fields.
- [x] EC-005: Layout adapts at 60x18, 80x24, 100x30, 120x35 and 160x45 with component-aware minimums; Kanban scrolls horizontally, keeps selection visible and uses real transitions; tables and details prioritise title/status/priority.
- [x] EC-006: Theme uses restrained colour, clear headers, visible focus, readable selection and useful empty states, and degrades with `NO_COLOR`/limited colour.
- [x] EC-007: One shared shell handles local and connected operation; pending commands, conflicts and recovery are inspectable; the legacy flag and legacy connected presentation are gone with no capability loss.
- [x] EC-008: The event loop is simple, keyboard dispatch has one location, forms/modals have explicit owners, render layout is separate from domain actions, and M28 tests still pass.
- [x] EC-009: The 20-project/200-task fixture exercises switching, editing, search, scrolling and detail inspection with recorded startup/interaction measurements and no hangs.
- [x] EC-010: PTY scenarios A–G pass; documentation matches behaviour; no known blocking data-loss, authorization or everyday-use defect remains.

## Evidence

- EC-001: `crates/orbit-tui/src/ui_state.rs` schema v1 and 18 state tests; `tests/acceptance/tui/test_m29_selection.py` (3 passed).
- EC-002: navigation/focus tests; PTY Scenarios A/C/F; captures `100x30-tasks.txt`, `160x45-board.txt`.
- EC-003: compact-bar/menu/palette/disabled tests; captures `100x30-action-menu.txt`, `100x30-palette-query.txt`.
- EC-004: quick-edit, field-order, validation and multiline tests; PTY Scenario B (`test_m29_workflows.py`).
- EC-005: five-size layout, kanban windowing/ownership, table-priority and detail-clamp tests; PTY Scenario D; captures `60x18-board.txt`, `100x30-board.txt`, `160x45-board.txt`.
- EC-006: theme glyph completeness/distinctness tests; `60x18-home.txt` capture under `NO_COLOR`.
- EC-007: `connected.rs` deleted (1994 lines); `!` inspector, conflict review and console tests; PTY Scenario E (`test_m29_connected_recovery.py`); contract guards.
- EC-008: `main.rs` 196 lines, `input/dispatch.rs` single dispatch, `app/` and `render/` ownership; workspace 1,563 tests.
- EC-009: `scripts/tui-fixture.py` (20 projects, 200 tasks, 1,017 activities, 6 attachments, 8 agent jobs, 5 pinned blueprint versions); `.agent/evidence/m29-perf-baseline.*` and `m29-perf-after.*`.
- EC-010: TUI suite 54 passed/1 skipped; full fixture 2 passed; docs rewritten; 21 captures in `.agent/evidence/m29-captures/`; M15 45/47 with the withheld release workflow.

## Remaining work

- Per-device project ACLs (TODO-0013) and server-side attachment GC (TODO-0014)
  remain documented product follow-ups, unchanged by M29.
- Live Herdr/model acceptance remains opt-in and unverified in automation.
- The release workflow file remains withheld, so two M15 release-contract checks
  cannot pass (M27 external gate).
- Connected multi-command edits are not atomic by design; the pending inspector
  exposes per-command state.
- Advanced connected record families (schedule, routine, milestone, strategy,
  notes) are edited through the connected console rather than structured forms;
  read-only listings and writes are available.

## Next milestone

none
