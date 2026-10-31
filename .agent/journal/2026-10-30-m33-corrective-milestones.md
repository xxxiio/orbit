---
template: journal-entry
template_version: 2.1.0
template_digest: "20e8b70b95080dbe3110fe7ad70c6bd0480c2a911af959019213e78b3a1b7814"
project_owned: true
record_contract: journal-entry
record_schema_version: 1
status: recorded
date: "2026-10-30"
task_id: TASK-0015
---

# Journal Entry: 2026-10-30 — M33 corrected to Milestones, unified Tasks and a compact Board

## Context

The first M33 pass treated Epics as a second project work-grouping entity beside
Milestones, with its own authority family, routes, columns, filters and roadmap
lanes. A human visual/product review rejected that model: Orbit already has a
canonical Milestone aggregate, and Jira was a UX reference, not a domain
specification. The Epic design was superseded while its typography and
authentication work was retained. This entry records the corrected phase. M33
remains active: automated web gates are clean, but real-browser acceptance and
human approval have not happened.

## Work completed

- Removed Epic as an authoritative and user-facing concept. Milestone is now the only
  project work-grouping unit.
- Extended Milestone with description, status, optional priority/owner, optional start
  and target dates, associated Tasks through `tasks.milestone_id`, associated Workflow
  Work Items through `milestone_work_items`, derived progress, blockers, shared-stream
  activity, safe delete/move guards and the command family `create`, `update`,
  `change_status`, `set_target`, `move_project`, `assign_task`, `unassign_task`,
  `assign_work_item`, `unassign_work_item` and `delete`.
- Added the one-time store-open conversion: deterministic milestone ids, `--epic`
  and `--epic-<hash>` collision suffixes capped at 128 characters, existing task
  milestone assignments win, existing Work Item assignments win, cancelled is
  preserved, completed is downgraded only by the copied-work rule, activity is
  remapped, legacy tables are dropped and no permanent compatibility layer remains.
- Added rich atomic task creation: title, project, description, milestone, initial
  status, priority, deadline, estimate, acceptance criteria and dependencies in one
  accepted command with one receipt. Quick capture remains for Inbox and the keyboard;
  retry replays the receipt.
- Unified project navigation to Overview, Tasks, Roadmap, Milestones and workflow
  extras. One Tasks tab presents List, Board and Backlog through
  `?view=list|board|backlog`; legacy `/list`, `/board` and `/backlog` links redirect
  with their filters. Shared URL filters cover search, status, priority, milestone,
  sort/direction, grouping, columns, selection and the open task.
- Made the Board compact by default with persisted Compact/Comfortable density,
  priority/deadline/blocked cues, on-demand inspector and authoritative move menu.
- Rebuilt the Roadmap as one Milestone hierarchy: duration bars when start and target
  exist, checkpoints when only a target exists, child task windows, progress, blocked
  work, dependency highlighting, week/month/quarter and an Unscheduled section. No
  Epic lanes remain.
- Rewrote the affected web guide sections, replaced the Epic TUI addendum with the
  corrected Milestone/unified-Tasks/compact-Board/full-create/roadmap model, and
  stated that no TUI implementation happened in this phase.

## Decisions made

- **Milestone is the grouping unit.** There is no second grouping noun, lifecycle or
  authority family. A task has at most one milestone, Work Items have their own
  membership, and both are server-validated within one project.
- **Progress is server-derived.** Cancelled tasks are excluded; `done` is complete and
  `blocked` is counted; Work Items are complete in `done`, `completed` or `closed`.
  Clients render the number instead of recomputing it from a partial row set.
- **Conversion preserves identity and existing choices.** A task keeps its identity and
  an existing milestone assignment always wins. Completed status is not carried blindly:
  it survives only when copied work is finished, becomes `planned` when nothing was
  copied, and `in_progress` when copied work is unfinished.
- **The conversion is one-way and temporary by design.** It runs once when the store
  opens, then the legacy tables are dropped. There is no compatibility layer to
  maintain after the pre-release window.
- **The Board stays calm.** Compact is the default, details are on demand, and status
  movement remains an explicit authoritative command rather than drag-and-drop.

## Files or components changed

- Rust: `crates/orbit-runtime/src/authority_milestones.rs`, `store.rs`,
  `crates/orbit-server` routes and tests.
- Web: `web/src/features/milestones/**`, `features/tasks/{tasks-workspace,task-board,
  create-task-dialog,task-list-state}.tsx`, `features/roadmap/**`,
  `features/projects/project-workspace-page.tsx`, API adapters/mutations and unit tests.
- Docs: `docs/web-app.md`, `docs/guides/tui-redesign-spec.md`.
- Records: `.agent/current/{acceptance,evidence,next-step}.md` and this journal entry.

## Validation performed

- `cargo test --workspace --offline`: 31 test binaries, 1862 passed, 0 failed.
- `cargo clippy -p orbit-runtime -p orbit-server -p orbit-mcp --offline --all-targets -- -D warnings`: clean; `cargo fmt --all -- --check`: clean.
- `cd web && node node_modules/typescript/bin/tsc --noEmit`: exit 0.
- `cd web && node node_modules/vitest/vitest.mjs run`: 72 files, 458 tests passed.
- `cd web && node scripts/check-terminology.mjs`: clean, 273 files.
- `cd web && node node_modules/vite/bin/vite.js build`: clean production build.
- Real-browser Chromium: 150 passed, 5 env-gated skips, 0 failed on the final full
  suite; the affected navigation, roadmap, milestones, tasks-workspace, a11y,
  responsive and planning suites were also re-run after the last UI change.
- Legacy conversion on a copy of the real dogfooding database: 2 Epics became 2
  Milestones, all 142 Tasks kept their identity, 41 Tasks carry a milestone, the Epic
  tables are gone, and a server restart left revision 889 with the same counts.
- 28 captures in `web/evidence/m33-corrective/` at 1440×900 and 390×844, light and
  dark, with no page or console errors.

## Problems encountered

- The first design added a second grouping entity rather than extending Milestones.
  Human review rejected it; the corrective work removes it instead of keeping a
  compatibility layer.
- Canonicalising Milestones initially dropped the workflow's Schedule panel. The full
  suite caught it through the planning specs; the section now keeps non-milestone
  panels below the milestone cards, and the affected specs pass.
- Oracle gate 1 found six material Rust issues (legacy retry serialization, missing
  semantic fields, canonical bounds, Work Item mutation, empty-completed conversion,
  MCP unassign scope); all were fixed and re-reviewed as resolved.
- Oracle gate 2 found nine web issues (search spaces, declared filters/columns, board
  readiness/limit, create bounds, uncertain discard, milestone entry points,
  membership freshness, board focus, sort reporting); all were remediated with unit
  coverage.

These are findings recorded in the corrective work, not inferred results. Oracle gate 1
reviewed the Rust data model and dispatched six material fixes.

| Defect | Fix |
|---|---|
| Legacy retry serialization was not preserved correctly. | Replayed receipts for converted legacy commands serialize the stored result. |
| Rich create did not emit semantic fields for criteria and prerequisites. | The create transaction emits a field mutation for each created field, criterion and prerequisite edge. |
| Priority and estimate accepted values outside the canonical bounds. | Enforce the task priority enum and the canonical estimate cap on create. |
| Work Item membership mutation was missing from the milestone write path. | Emit the Work Item membership mutation with the accepted command. |
| A completed empty conversion produced the wrong milestone status. | A completed source with no copied work converts to `planned`; unfinished copied work converts to `in_progress`. |
| MCP unassign scope was too broad. | Scope unassign to the milestone being changed. |

## Remaining work

- Human visual and usability review of the running app and
  `web/evidence/m33-corrective/` (EC-010). M33 remains active until that review is
  recorded.

## New TODOs

- Record the human review outcome before changing the M33 milestone status.
- Re-run the affected gates after any review-driven change.

## Risks or blockers

- Human approval is still pending; M33 must stay active.
- The Development workflow keeps a declared mixed Board because it also owns a Work
  Item board; it is the documented workflow exception, not a second task model.
- Screenshots cannot judge comfort, trust or visual taste.
