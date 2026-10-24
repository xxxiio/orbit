---
template: milestone-plan
template_version: 2.0.0
template_digest: "737b1e57a6469c8843d3ed28294d21fe5a61750b2222084a7eaac19644fa392f"
project_owned: true
record_contract: milestone-plan
record_schema_version: 1
id: MILESTONE-0029
status: active
---

# Milestone: TUI product experience, interaction quality and maintainability

## Intended outcome

The already-functional Orbit TUI becomes an excellent daily-use terminal
application: working context survives navigation, view changes and restarts;
important actions are discoverable without memorised shortcuts; forms are safe
and efficient; boards, tables and details stay readable at every supported
terminal size; one shared shell serves local and connected authority; and the
oversized TUI modules are split along cohesive responsibility boundaries with
measured interaction latency.

## Why this milestone matters

M28 delivered the product capabilities: project-centric navigation, declarative
blueprint views, rich task editing, acceptance criteria, attachments, agent-job
inspection, search, palette and connected command durability. Daily use now
exposes interaction defects rather than missing features — most concretely, the
selected item is persisted as `None`, so a restart loses the user's working
context. The connected legacy TUI is still a second presentation implementation
behind `ORBIT_TUI_LEGACY_CONNECTED`, and `main.rs`/`render.rs`/`app.rs`
concentrate unrelated responsibilities. M29 fixes these without reopening M28
scope, adding domain models for visual concerns, or weakening authority
boundaries.

## Scope

- Stable selected-item and scroll restoration by entity identity, with nearest
  valid fallback for deleted or reordered items and corrupt-state tolerance.
- Per-component navigation persistence, focus behaviour, view switching and
  back-navigation that preserve the originating context.
- Contextual, permission-aware action bar and ranked command palette; disabled
  actions explain why; recent destinations.
- Form focus, ordering, save/cancel, validation feedback, data preservation,
  unsaved-change confirmation and quick-edit paths using existing commands.
- Responsive layout with component-aware minimums at 60x18 through 160x45.
- Kanban horizontal viewport, readable columns, predictable movement and real
  blueprint transition semantics; responsive tables, lists and detail panes.
- Refined restrained theme with `NO_COLOR` and limited-colour support.
- Shared-shell parity for connected pending-command inspection, conflict
  review and offline recovery, then removal of the legacy connected TUI.
- Cohesive module extraction and measured performance improvements on a
  deterministic 20-project/200-task/1,000-activity fixture.
- Targeted PTY acceptance scenarios A–G, documentation and evidence.

## Non-goals

- Rebuilding blueprint presentation, the Basic workflow, attachments,
  acceptance criteria, backend abstractions, quit handling or the PTY harness.
- New domain models, direct SQL in the TUI, or a second persistence model.
- A Flutter renderer, plugin marketplace or dynamic blueprint code execution.
- Asynchronous runtime, caching infrastructure or release machinery justified
  only by theoretical latency.
- Changing application-level authorization or connected command semantics.

## Deliverables

- `ui_state` schema carrying project, view, focus, component, selected id,
  index and scroll offsets with safe restore and fallback.
- One keyboard dispatch location, owned form/modal handling and separated
  render modules.
- Contextual action bar, ranked palette, quick edit and predictable forms.
- Responsive shell, Kanban and table sizing, refined theme.
- Shared connected shell with pending/conflict recovery and no legacy entry.
- Module layout `app/`, `input/`, `forms/`, `render/` with backend, terminal,
  theme and ui_state at the crate root.
- Deterministic large fixture, recorded before/after measurements and PTY
  scenarios A–G.

## Dependencies

- MILESTONE-0028 (completed product capabilities and regression tests).
- Local `orbit-server` binary for connected recovery acceptance.

## Risks

- Mechanical module extraction can change privacy or behaviour — mitigate with
  behaviour-preserving moves, `cargo test -p orbit-tui` after each move and an
  explorer structure scan at the gate.
- Removing the legacy connected TUI can drop operations — mitigate by porting
  each legacy section to the shared shell and verifying pending/conflict
  recovery before deletion.
- Layout changes can regress narrow terminals — mitigate with PTY checks at
  every supported size.
- Performance claims can outrun evidence — mitigate by recording fixture
  measurements before and after each change.

## Ordered implementation sequence

1. I1 — Stable selected-item restoration and navigation context preservation.
2. I2 — Contextual UX, discoverable actions, ranked palette and forms.
3. I3 — Responsive layout, Kanban navigation, table/detail density, theme.
4. I4 — Connected shared-shell parity, pending/conflict management, legacy
   connected presentation removal.
5. I5 — Module extraction, realistic fixture, measured performance work.
6. I6 — PTY scenarios A–G, documentation, terminal captures, final evidence.

## Exit criteria

- [ ] EC-001: Selected item, component, scroll and focus restore across view changes and restart; deleted/reordered items fall back safely; corrupt state does not block startup.
- [ ] EC-002: Focus is unmistakable, selection stays visible while scrolling, modal open/close preserves focus, resize preserves navigation, and Esc recovers the prior context.
- [ ] EC-003: A small contextual action bar exposes high-value actions with reasons for disabled entries; the command palette ranks fuzzily and is context-aware.
- [ ] EC-004: Forms show field focus, consistent tab order, save/cancel, per-field validation, preserved input after errors, safe multiline editing and unsaved-change confirmation; quick edit avoids traversing advanced fields.
- [ ] EC-005: Layout adapts at 60x18, 80x24, 100x30, 120x35 and 160x45 with component-aware minimums; Kanban scrolls horizontally, keeps selection visible and uses real transitions; tables and details prioritise title/status/priority.
- [ ] EC-006: Theme uses restrained colour, clear headers, visible focus, readable selection and useful empty states, and degrades with `NO_COLOR`/limited colour.
- [ ] EC-007: One shared shell handles local and connected operation; pending commands, conflicts and recovery are inspectable; the legacy flag and legacy connected presentation are gone with no capability loss.
- [ ] EC-008: The event loop is simple, keyboard dispatch has one location, forms/modals have explicit owners, render layout is separate from domain actions, and M28 tests still pass.
- [ ] EC-009: The 20-project/200-task fixture exercises switching, editing, search, scrolling and detail inspection with recorded startup/interaction measurements and no hangs.
- [ ] EC-010: PTY scenarios A–G pass; documentation matches behaviour; no known blocking data-loss, authorization or everyday-use defect remains.

## Evidence

- EC-001: `crates/orbit-tui/src/ui_state.rs` tests, PTY Scenario A.
- EC-002: navigation/focus tests, PTY Scenarios A/C/F.
- EC-003: palette/action tests, PTY footer assertions.
- EC-004: form tests, PTY Scenario B.
- EC-005: layout/kanban tests at five sizes, PTY Scenario D.
- EC-006: theme tests with `NO_COLOR` set.
- EC-007: connected tests, PTY Scenario E.
- EC-008: module layout, `cargo test -p orbit-tui`, clippy/fmt.
- EC-009: fixture generator, measurement report in GetDone evidence.
- EC-010: full PTY suite, docs, terminal captures, milestone acceptance record.

## Remaining work

- All exit criteria are open at milestone start.

## Next milestone

none
