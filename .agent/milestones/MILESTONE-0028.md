---
template: milestone-plan
template_version: 2.0.0
template_digest: "737b1e57a6469c8843d3ed28294d21fe5a61750b2222084a7eaac19644fa392f"
project_owned: true
record_contract: milestone-plan
record_schema_version: 1
id: MILESTONE-0028
status: completed
---

# Milestone: Project-centric TUI and blueprint UI composition

## Intended outcome

A user can navigate Orbit by project, see the capabilities each project's
blueprint configures, work with typed tasks and work items, inspect details and
agent execution, and use the same authorized application services as the CLI.
Blueprints declare views; a trusted component registry renders them.

## Why this milestone matters

The current local TUI still exposes six hardcoded feature tabs. Project
capabilities such as habits, planning, knowledge and agent jobs appear as
unrelated global screens, and an installed blueprint cannot control how its
project is presented. Orbit's blueprint framework is meaningful for ordinary
users only when a pinned blueprint can produce a usable workspace without Rust
changes. M27's remaining release verification is an external publication gate;
M28 is the highest-priority unfinished product work that can be completed
locally and materially improves every project.

## Scope

- Framework-neutral blueprint presentation schema (`views`, layouts,
  components, bindings, contextual actions).
- Shared `orbit-ui` crate: data contracts, closed source bindings, trusted
  component registry, resolved view/component/detail models, action catalogue.
- Application-facing view data in `orbit-client` using existing queries and
  authoritative commands.
- Project-centric TUI shell: sidebar, workspace, details, responsive layout,
  theme, focus, keyboard, command palette, search, modals and feedback.
- Reusable built-in components: text, list, table, metric, detail, kanban, task
  list, timeline, habit tracker, metric history, knowledge references, agent
  jobs and milestone progress.
- Blueprint-defined views, validation, legacy generated defaults and explicit
  version pinning.
- Bundled v2 views for development, research, learning and habits.
- User and developer documentation plus focused schema, rendering, interaction
  and architecture tests.

## Non-goals

- Flutter implementation or a second persistence model.
- Dynamic third-party code loading, scripting, a plugin marketplace or a hosted
  blueprint registry.
- New cloud synchronization, Herdr requirements or an agent review service that
  does not exist.
- Rewriting the connected command surface in this milestone.
- Broad M15 release acceptance, signing, TLS or cross-platform certification.

## Deliverables

- `crates/orbit-blueprint/src/presentation.rs`.
- `crates/orbit-ui` with `data`, `binder`, `registry`, `resolve`, `actions` and
  `model` modules.
- `crates/orbit-client/src/view_data.rs` and read-only blueprint/work-item/agent
  projections.
- New `orbit-tui` modules `app`, `render`, `theme` plus the project-centric
  shell and tests.
- Bundled `blueprints/{development,research,learning,habits}.json` version 2
  with views.
- `docs/guides/tui-workspace.md`, `docs/guides/blueprint-views.md`,
  `docs/reference/ui-views.md`, `docs/developers/ui-components.md` and updated
  index/quickstart/custom blueprint/TUI contract pages.

## Dependencies

- MILESTONE-0015 executable Linux baseline.
- MILESTONE-0022 planning brief/focus/impact contracts.
- MILESTONE-0023 agent job read models.
- MILESTONE-0025 I1/I2 blueprint registry, binding, typed work items and
  transitions (implemented surfaces only; the deferred M25 capability/upgrade
  criteria are not dependencies of M28).

## Risks

- Renderer could bypass application services; prevented by typed client DTOs,
  source-shape tests and review of every mutation path.
- Blueprint metadata could be mistaken for authority; the action catalogue is
  derived from the selection and the authority revalidates.
- Legacy blueprints could become unusable; generated defaults are covered by
  client and TUI tests.
- New bundled versions could silently change pinned projects; binding remains
  explicit and versioned.
- Narrow terminals could hide critical data; multi-component views collapse to
  a focused component and details open as a modal.
- Repeated keypresses or refreshes could duplicate mutations; identical
  submissions are suppressed and every command is typed/revalidated.

## Ordered implementation sequence

1. Shared presentation schema and project-centric shell contracts.
2. Trusted component registry, bindings and typed resolved models.
3. Application-facing project/workspace data and blueprint work-item access.
4. Interactive workspace: navigation, details, actions, forms, palette, search.
5. Domain components and bundled blueprint views.
6. Responsive polish, docs, focused tests and record reconciliation.

## Exit criteria

- [x] EC-001: A Ratatui-free blueprint presentation schema declares views,
  layouts, component instances, sources, filters, sorting, limits and action
  hints; duplicate/unknown/invalid definitions fail validation and legacy
  manifests remain valid.
- [x] EC-002: The local TUI shell replaces fixed feature tabs with Home, Inbox,
  Projects, Agents and project navigation, plus sidebar/workspace/details
  regions and wide/medium/narrow layouts.
- [x] EC-003: Application-facing view data resolves real projects, tasks,
  readiness, blueprint work items, milestones, knowledge, habits, metrics,
  schedules and agent jobs through existing client/application services.
- [x] EC-004: A trusted component registry provides typed descriptors and
  built-in text, list, table, metric, detail, kanban, task list, timeline,
  habit tracker, metric history, knowledge reference, agent job and milestone
  progress renderers with empty/loading/error/stale/missing states.
- [x] EC-005: Blueprints resolve named views with project-scoped bindings, and
  projects without views receive functional generated defaults; bundled
  development, research, learning and habits manifests demonstrate the model
  without changing pinned projects.
- [x] EC-006: Contextual actions, forms and the command palette share one
  catalogue and route every mutation through existing authorized client calls;
  pending, accepted, rejected, stale and unavailable states are visible and
  duplicate submissions are suppressed.
- [x] EC-007: Focused schema, binding, rendering, interaction and architecture
  tests pass, including terminal-buffer checks for wide/narrow layouts,
  missing components, stale/error states and project switching.
- [x] EC-008: User and developer documentation describe implemented behavior,
  and GetDone records actual progress and remaining limitations.

## Evidence

- EC-001: `cargo test -p orbit-blueprint` 20 tests pass; `presentation.rs`
  validation tests and bundled YAML/JSON round-trip.
- EC-002: `crates/orbit-tui/src/{main,app,backend,render,theme,terminal}.rs`; one
  App/renderer over local and connected backends; TestBackend and PTY tests for
  wide/narrow layout, focus, tabs, project switching, quit and restoration.
- EC-003: `crates/orbit-client/src/view_data.rs`, `orbit-tui/src/backend.rs`;
  local/connected project/workspace resolution, pending/accepted separation and
  connected flush verified against a real server.
- EC-004: `crates/orbit-ui` 14 tests; `orbit-tui` 125 tests including overview,
  readable Kanban window, table/list sizing, detail scrolling, missing/loading/
  stale/error rendering and common terminal sizes.
- EC-005: `blueprints/*.json` version 2 with views; generated fallback test;
  blueprint binding/work-item transition test.
- EC-006: structured forms and contextual actions; duplicate-submission tests;
  action-authority tests; palette/search tests; no-SQL/no-process source boundary;
  real PTY lifecycle and blueprint-workflow persistence.
- EC-007: `cargo test --workspace --offline` 1,350 passed, 0 failed; clippy
  `-D warnings` on affected crates; fmt clean.
- EC-008: This milestone, task/status/acceptance/evidence records, roadmap,
  journal and documentation pages.

#### Usability recovery (2026-10-22)

The original M28 delivery was a working project-centric shell but not an
everyday project manager. Recovery added:

- Bundled `basic` v1 workflow (ordinary tasks; Overview/Tasks/Board/Timeline)
  and authority-owned default binding for every new project on both creation
  paths, with explicit change/unpin.
- Task description (Markdown), priority, labels and ordered acceptance criteria
  with stable ids and completion timestamps.
- A content-addressed attachment subsystem (metadata + managed objects) with
  size limits, symlink rejection, deduplication, GC and explicit viewer handoff.
- TUI editing, criteria/attachment/activity modals, action bar, multiline
  editor, session restore, first-run guidance and workflow terminology.
- MCP task detail/acceptance/attachment/project/workflow tools; no implicit agent
  launch.
- 1,238 focused tests, 1,423 workspace tests, 40 real PTY/MCP acceptance items,
  clippy/fmt clean, 34 documentation tests.

## Remaining work

- Connected attachment upload/retrieval is not implemented; all connected
  attachment operations fail closed with an explicit message (next step
  NEXT-0009). Connected task descriptions/criteria have the same limitation.
- Connected project/task workflows share the shell and preserve pending/accepted
  separation. Connected multi-command edits are not atomic: a later rejected
  intent can leave earlier intents queued (documented recovery via outbox/CLI).
- Connected blueprint/work-item, habits, metrics and notes remain on the
  explicitly available legacy connected command surface.
- Agent execution is inspectable; review/resume and live Herdr launch stay
  disabled without an explicitly configured runtime or connected review service.
- Local loading is synchronous; no screenshots are claimed.
- Broad public release acceptance remains assigned to M15/M27.

## Next milestone

none
