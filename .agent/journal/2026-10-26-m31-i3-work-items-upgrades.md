---
template: journal-entry
template_version: 2.1.0
template_digest: "20e8b70b95080dbe3110fe7ad70c6bd0480c2a911af959019213e78b3a1b7814"
project_owned: true
record_contract: journal-entry
record_schema_version: 1
status: recorded
date: "2026-10-26"
task_id: TASK-0010
---

# Journal Entry: 2026-10-26 — M31 I3: workflow work items, transitions, upgrades and actions

## Context

I2 made project workspaces workflow-driven for tasks. I3 had to make the workflow model
itself usable: work items declared by a manifest, transitions and guards from that
declaration, upgrade previews that a person can read and confirm, and Workflow Actions
shown honestly even though the connected server cannot execute capabilities.

## Work completed

- Registry: `source: work_items` now renders in `kanban`, `table`, `list` and `detail`.
  Columns are the active kind's declared states, cards use declared display fields,
  multi-kind boards offer a kind chooser, and declared `kind`/`state` filters, sorts and
  limits apply; unsupported keys are refused by name. `useWorkItems(projectId)` is read
  once at the workspace level and fed through `ViewRendererProps.workItems`.
- Create dialog: kind selection when the view does not pin one, typed controls for
  `text`/`boolean`/`integer`/`choice`, defaults, required validation, and one caller-owned
  attempt (command id + item id) per payload so an unchanged retry replays instead of
  duplicating.
- Work-item sheet (`?item=`): kind/state/revision, typed property editing, transitions
  offered strictly from the manifest with guards rendered as sentences (unsatisfied ones
  marked) and `required_capability` chips, accepted records written back to every cached
  copy, rejections in place quoting the server, uncertain stated as unknown with refresh.
- Upgrade: target is the newest published version of the same workflow id; the full
  server-computed diff (kinds, states, fields, transitions, actions, capabilities, views,
  warnings, blockers) is shown; apply requires the previewed fingerprint and an explicit
  version-named confirmation; the receipt shows previous → new, fingerprint and
  `already_applied`; new views appear and survive reload. Cross-id re-pin is labelled as
  not an upgrade.
- Actions: per state, declared actions with id, description, capability chip and typed
  parameters; no run control; copy states that execution requires the local Orbit
  authority and that this server does not execute capabilities.
- Gate 2 carry-forwards closed: `options` (`empty_text`, `caption`, `show_status`,
  `show_priority`, `column_width`, `show_dependencies`, `days`, `window`), `columns` on
  `list`, `limit` on tables/full task lists with truncation notes, and `options.unit`
  conversion with unknown units refused.

## Decisions made

- Transitions are offered from the declaration and the server remains the only validator;
  the UI never builds a transition table of its own.
- Work-item commands serialize per item and read `expected_revision` after the previous
  accepted record is written back, which removed the duplicate-submit class rather than
  hiding its symptom.
- A close only abandons a draft it has already tried to send: silent loss beats no dialog,
  so the sheet keeps a per-editor record of flushed drafts.
- Manifest copy is projected through `workflowText`, because `UI_ACTIONS` still contains a
  legacy identifier that a valid manifest could render.
- `useBindWorkflow` is deliberately absent: bind has no preview/fingerprint/blocker check,
  so a hook would be an unguarded second pin path beside the upgrade flow.
- Capability execution stays host-only; the browser tells the truth instead of offering a
  button that cannot work.

## Files or components changed

- `web/src/features/workflows/**` (new `declarations/*`, `views/work-item/*`, sheet,
  create dialog, upgrade preview), `web/src/api/accepted-write.ts`,
  `web/src/lib/{declarations,humanize}.ts`, `web/src/hooks/use-resource-drawer.ts`,
  `web/src/components/ui/{section,table}.tsx`, and the task drawer/board/table modules that
  now share them.
- Specs under `web/tests/unit/**` and `web/tests/e2e/{work-items,upgrades,workspaces-*}`.
- No Rust change in I3.

## Validation performed

- `cargo test -p orbit-server` 50; `-p orbit-runtime` 545 + 2 (unchanged Rust).
- `tsc --noEmit` clean (app and e2e configs); Vitest 33 files / 173 tests; terminology 177
  files, 0 findings; `vite build` OK.
- Playwright full suite 41 passed, 3 skipped (opt-in evidence), 1.8m — work-item
  create/properties/transition/refusal, blocked + changed-fingerprint + applied upgrades,
  action honesty, five-workflow registry sweep, board keyboard moves, task→milestone
  without a spurious 409.
- Import graph: no `components`→`features/app`, no `features`→`app`, no cross-feature deep
  imports, no cycles.

## Problems encountered

- Gate 3 found a sheet that could become uncloseable after clearing an optional integer, a
  double submit on edit-then-close, and a lost-response create retry reported as refused;
  all fixed with regression tests.
- A later re-review found the close flag could still discard a *fresh* draft and that
  declared descending sorts were inverted (direction applied twice); both fixed with tests
  proven to fail on the old code.
- The structure scan found a second renderer family forming; the pass split
  `view-metadata.ts` (872) and `work-item-views.tsx` (890), extracted the shared surface kit
  and one accepted-write path, and removed the dead sort helpers.

## Remaining work

- I4: knowledge, attachments, agents. I5: real dogfooding cycle. I6: refinement, docs,
  TUI redesign specification.
- Recorded I4 carry-forwards: generic guard refusal text from the server; no bundled
  manifest declares actions; create-dialog "refused but pinned" state; a bundled
  `descending: true` view for browser coverage; evidence README delta list; optional
  flushed-draft key hardening; `api/mutations.ts` still large; `project-settings.tsx`
  repeats the view-diff summary; `docs/web-app.md` not yet updated for I3.

## New TODOs

- None outside the milestone exit criteria.

## Risks or blockers

- `getdone-validate-project` still cannot run here (missing skill-pack manifest).
- Connected capability execution is absent by design; the UI states it rather than faking it.
