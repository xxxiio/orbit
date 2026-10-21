---
template: current-task
template_version: 2.0.0
template_digest: "179af91e65608fbfd07b28126ed729856e6524410956892a698b3f40e902e695"
project_owned: true
record_contract: current-task
record_schema_version: 1
id: TASK-0004
status: completed
milestone_id: MILESTONE-0028
---

# Task: M28 project-centric TUI and blueprint UI composition

## Objective

Replace the hardcoded local TUI feature tabs with a project-centric workspace
whose views are declared by pinned project blueprints and rendered through a
trusted, framework-neutral component registry backed by real Orbit application
data and authorized commands.

## Current behaviour

The local TUI exposes Today, Inbox, Projects, Knowledge, Habits/Plan and Agent as
fixed tabs. Project capabilities are separate global screens, blueprints cannot
control presentation, and there is no shared view contract for another client.
The connected TUI is a separate command-centric surface. All mutations already
travel through `OrbitClient`/application services.

## Desired behaviour

A user opens Orbit and navigates Home/Today, Inbox, Projects and Agents, then
opens a project and works inside blueprint-defined or generated default views.
Tables, lists, metrics, boards, timelines, habits, knowledge and agent jobs are
reusable registered components. Contextual actions and the command palette use
one catalogue, and every mutation remains authorized, revision-checked,
duplicate-safe and clearly reported. Legacy blueprints open without migration.

## Scope

- Blueprint `views` schema, validation, layouts, component bindings and action
  hints.
- New `orbit-ui` crate for data contracts, registry, resolution, detail/action
  models.
- `orbit-client` project/workspace view data and blueprint work-item reads.
- TUI shell, responsive layout, theme, focus, input, modals, palette and search.
- Built-in components and bundled v2 development/research/learning/habits views.
- Focused tests and user/developer documentation.

## Out of scope

- Flutter implementation, dynamic third-party code loading, plugin marketplace.
- A second persistence model or TUI-specific business rules.
- Cloud sync, mandatory Herdr, TLS/signing, cross-platform certification.
- Rewriting the connected command surface.
- Broad M15 release acceptance.

## Dependencies

- MILESTONE-0015 Linux executable baseline.
- MILESTONE-0022 planning contracts.
- MILESTONE-0023 agent read models.
- MILESTONE-0025 I1/I2 blueprint registry, work items and transitions (implemented surfaces only).

## Risks

- UI bypass of authority: prevented by typed client calls and architecture tests.
- Legacy blueprint breakage: generated defaults and compatibility tests.
- Silent version changes: immutable versions and explicit binding.
- Narrow layout information loss: focused component mode and detail modal.
- Duplicate mutations: submission guard plus authoritative revalidation.
- Overclaiming connected/release readiness: records separate local M28 scope from
  M27/M15 external gates.

## Acceptance criteria

- [x] AC-001: Shared view schema validates views, layouts, components, sources,
  bindings and actions; legacy manifests remain valid.
- [x] AC-002: The fixed-tab shell is replaced by project-centric navigation and
  responsive sidebar/workspace/details regions.
- [x] AC-003: Views resolve real application data for projects, tasks, work
  items, milestones, knowledge, habits, metrics, schedule and agent jobs.
- [x] AC-004: Registered components cover text, list, table, metric, detail,
  kanban, task list, timeline, habit tracker, metric history, knowledge refs,
  agent jobs and milestone progress.
- [x] AC-005: Blueprints declare multiple named views; legacy projects receive
  generated defaults; bundled v2 manifests demonstrate the model.
- [x] AC-006: Contextual actions, forms and command palette mutate only through
  authorized client calls and show pending/accepted/rejected/stale states.
- [x] AC-007: Focused tests cover schema, rendering, interactions and
  architecture; workspace Rust tests pass.
- [x] AC-008: User and developer documentation match implemented behavior.

## Validation

```sh
cargo build -p orbit-tui -p orbit-cli -p orbit-server --offline
cargo test -p orbit-blueprint -p orbit-ui -p orbit-client -p orbit-tui --offline
cargo test --workspace --offline
cargo clippy -p orbit-tui -p orbit-client -p orbit-ui --all-targets --offline -- -D warnings
cargo fmt --all -- --check
python3 -m pytest -q tests/acceptance/tui
python3 -m unittest discover -s tests/documentation
./scripts/m15-full-automated.sh --profile full
```
