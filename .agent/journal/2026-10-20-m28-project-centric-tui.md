---
template: journal-entry
template_version: 2.1.0
template_digest: "20e8b70b95080dbe3110fe7ad70c6bd0480c2a911af959019213e78b3a1b7814"
project_owned: true
record_contract: journal-entry
record_schema_version: 1
status: recorded
date: "2026-10-20"
task_id: TASK-0004
---

# Journal Entry: 2026-10-20 — M28 project-centric TUI and blueprint UI composition

## Context

M28 was selected over M27 continuation because M27's only remaining gate is an
external, authorized GitHub Release verification. The local TUI still exposed
six hardcoded feature tabs and blueprints could not control presentation, so
project capabilities remained unrelated global screens. M28 is the highest
priority unfinished product work that can be completed and validated locally.

The existing working tree already contained uncommitted M27 distribution work.
It was preserved; M28 changes were added on top rather than reverting or
rewriting unrelated files.

## Work completed

- M28 definition-of-done progress: approximately 92% (0% before this task);
  eight source-level exit criteria and the full local profile are complete.

- Added a Ratatui-free blueprint presentation schema with `views`, layouts,
  component instances, sources, filters, sorting, limits, action hints and
  publication-time validation. Legacy manifests without `views` remain valid.
- Created `orbit-ui` with renderer-neutral data contracts, a closed source
  binder, a trusted component registry, resolved view/component/detail models,
  generated defaults and one action catalogue.
- Added `orbit-client` project/workspace view data over existing application
  queries: tasks/readiness, blueprint work items and transitions, milestones,
  knowledge, habits/occurrences, metrics, schedules and agent jobs.
- Added storage/runtime/client read projections for project blueprint work
  items; no schema change and no second persistence model.
- Replaced the local fixed-tab TUI with Home/Today, Inbox, Projects, Agents and
  project views. Added sidebar/workspace/details regions, wide/medium/narrow
  layouts, focus, selection, scrolling, theme tokens, monochrome fallbacks,
  action menus, prompts, confirmations, command palette, search, help and
  status feedback.
- Implemented 13 built-in component renderers: text, list, table, metric,
  detail, kanban, task list, timeline, habit tracker, metric history, knowledge
  references, agent jobs and milestone progress.
- Updated bundled development/research/learning/habits manifests to version 2
  with differentiated views. Older pinned versions are not rebound or
  reinterpreted.
- Added user and developer documentation: terminal workspace guide, blueprint
  views guide, UI view reference and component/renderer development guide;
  updated the docs index, quickstart, custom blueprint guide, TUI contract and
  root README.
- Created M28 milestones/task/records/evidence and reconciled roadmap, status,
  acceptance, change impact, evidence and next-step records.

## Decisions made

- Blueprint view/action declarations are presentation metadata, never grants.
  All mutations are typed client calls and the authority revalidates.
- Project views resolve only the selected project; Home/Inbox/Agents are
  host-generated explicit workspace aggregations.
- Habits and knowledge remain global domain records; views reference them
  without claiming project ownership.
- Legacy blueprints get generated default views instead of migration.
- Bundled versions advance to 2 so existing v1 bindings keep their definition.
- M28 delivers the local renderer; connected TUI adoption is explicitly
  remaining work rather than an overclaimed completion.
- No dynamic plugins, SQL, scripts or shell commands are accepted from a
  manifest. Registration is trusted compiled-in Rust.

## Files or components changed

- `crates/orbit-blueprint/src/presentation.rs`, `lib.rs`, `bundled.rs`.
- `crates/orbit-ui/` (new crate: data, binder, registry, resolve, actions, model).
- `crates/orbit-client/src/view_data.rs`, `Cargo.toml`, `lib.rs`.
- `crates/orbit-storage/src/blueprint_work_items.rs`.
- `crates/orbit-runtime/src/blueprint_authority.rs`.
- `crates/orbit-tui/src/{main,app,render,theme}.rs`, `Cargo.toml`.
- `blueprints/{development,research,learning,habits}.json`.
- `docs/guides/{tui-workspace,blueprint-views}.md`,
  `docs/reference/ui-views.md`, `docs/developers/ui-components.md`,
  `docs/README.md`, `docs/tui-contract.md`, `docs/getting-started/quickstart.md`,
  `docs/guides/custom-blueprints.md`, `README.md`.
- `deploy/server/smoke-blueprint-workflows.py`, storage receipt test and
  blueprint registry test updated for bundled version 2.
- `.agent/` M28 milestone/task/current/status/acceptance/evidence/roadmap and
  evidence files.

## Validation performed

- `cargo test -p orbit-blueprint -p orbit-ui -p orbit-client -p orbit-tui --offline`:
  420 tests passed, 0 failed.
- `cargo test --workspace --offline`: 1,306 tests passed, 0 failed.
- `cargo clippy` on blueprint/UI/client/TUI/storage/runtime with
  `--all-targets -- -D warnings`: clean.
- `cargo fmt --all -- --check`: clean.
- `python3 -m unittest discover -s tests/documentation`: 29 passed.
- `./scripts/m15-full-automated.sh --profile full`: 42 passed, 0 failed,
  0 not-run; `.agent/evidence/m28-m15-final.zip`.
- Real pseudo-terminal launch at 140x40 rendered the Home shell and exited
  cleanly with `q`.

## Problems encountered

- The first Python rewrite of bundled JSON used invalid keyword-argument syntax;
  rewrote it with explicit dictionaries and verified all manifests.
- The TUI kanban selection initially used row-sorted order while the body used
  column order, so the highlighted card could differ from the detail selection;
  fixed by reordering selection to the visual order.
- The storage blueprint receipt test and M25 smoke script assumed bundled
  development v1; changed them to use the installed version dynamically.
- A self-referential source-shape test detected its own forbidden string; the
  test now inspects only the renderer/app/theme modules.

## Remaining work

- Run the full executable M15 profile and reconcile results.
- Decide/implement connected TUI adoption of the shared renderer.
- Local agent review remains inspect-only until an accepted local review surface
  exists.
- Loading is represented but local loading is synchronous.
- No screenshots are claimed; terminal-buffer tests and the guide describe
  verified behavior.
- Full M15 release acceptance and M27 public release verification remain
  separate gates.

## New TODOs

- TODO: connected TUI adapter to `orbit-ui` resolved views and action catalogue.
- TODO: local agent handoff review surface when an accepted client API exists.
- TODO: optionally add component `span`/nested layout for richer Home layout.

## Risks or blockers

- Connected/local renderer parity is not implemented yet.
- Agent review is inspect-only.
- M27 real-release installation remains blocked on maintainer authorization.
- Terminal-buffer tests do not cover every emulator.
EOF