---
template: current-task
template_version: 2.0.0
template_digest: "179af91e65608fbfd07b28126ed729856e6524410956892a698b3f40e902e695"
project_owned: true
record_contract: current-task
record_schema_version: 1
id: TASK-0015
status: active
milestone_id: MILESTONE-0033
---

# Task: M33 corrective — Milestone unification, task creation UX, unified Tasks workspace

## Objective

Correct the rejected Epic-based M33 design: make Milestone the only project
work-grouping entity, remove Epic, let users configure a task fully before creating
it, place List/Board/Backlog under one Tasks tab, make the Board compact and
readable, and rebuild the Roadmap around Milestones. Keep the retained typography
and authentication work.

## Current behaviour

The first M33 pass added a second grouping entity (Epic) beside Milestones, a
single-field task create dialog, three top-level task presentations, crowded board
cards and an Epic-based roadmap. A human review rejected that product model. The
corrected tree has one Milestone aggregate, one accepted rich-create command, one
Tasks tab with URL views, compact board cards and a Milestone hierarchy on the
roadmap. A human visual/usability review has not yet been recorded.

## Desired behaviour

Milestones group Tasks and supported Workflow Work Items with rich optional fields,
derived progress, activity and safe guards. Existing Epic data converts once,
deterministically, without losing task identity or an existing milestone assignment.
A task can be created with every supported property in one accepted operation, while
quick capture stays lightweight. List, Board and Backlog share the Tasks tab and its
filters. The Board defaults to compact cards and opens details on demand. The Roadmap
shows Milestone duration bars, checkpoints, child work and unscheduled work without
inventing dates. Rust remains authoritative; no TUI feature changes.

## Scope

- Rich Milestone authority, Task and Work Item membership, progress, activity,
  guards, one-time Epic conversion and Epic removal.
- Atomic rich task creation with criteria and dependencies; quick capture retained.
- One Tasks tab with URL-backed List/Board/Backlog and shared filters.
- Compact Board density, priority/deadline/blocked cues and authoritative moves.
- Milestone dialogs/list, task and Work Item Milestone fields.
- Milestone Roadmap with duration/checkpoint/unscheduled semantics.
- Tests, documentation, real-browser acceptance, captures and human handoff.

## Out of scope

- TUI implementation; Jira branding; mandatory sprints; Story Points; public release.
- A second grouping entity, a permanent Epic compatibility layer, or a second
  frontend domain engine.
- Reopening M31/M32 areas M33 did not touch.

## Dependencies

- M31/M32 web client, authoritative command ledger, task/milestone services and the
  Playwright harness.
- A legacy database copy for the conversion check and a browser-libraries bundle for
  Playwright Chromium.

## Risks

- Conversion could overwrite an existing task milestone; the existing assignment
  wins and the result is recorded.
- Rich create could partially fail; it is one transaction with one receipt.
- Canonicalising Milestones could strand workflow panels; non-milestone panels stay
  reachable below the cards.
- Screenshots cannot judge comfort or taste; human approval stays open.

## Acceptance criteria

- [x] AC-001: Epic is not a domain, route, dialog, filter, column or roadmap concept.
- [x] AC-002: One Milestone family supports rich fields, Task and Work Item
  membership, derived progress, activity and safe guards; conversion preserves
  identity and restart state.
- [x] AC-003: A fully configured task is created in one accepted operation; quick
  capture remains; retry is idempotent.
- [x] AC-004: One Tasks tab presents List, Board and Backlog with URL state and
  shared filters; legacy links redirect.
- [x] AC-005: The Board is compact by default, persists density, opens details on
  demand and keeps authoritative moves.
- [x] AC-006: The Roadmap is one Milestone hierarchy with duration, checkpoint and
  unscheduled shapes.
- [x] AC-007: Rust workspace, clippy, fmt, web typecheck, unit, terminology and build
  gates pass.
- [x] AC-008: Full Chromium suite passes (150 passed, 5 env-gated skips, 0 failed).
- [x] AC-009: 28 corrective captures exist at desktop/phone in light and dark.
- [ ] AC-010: A human has reviewed the running app and corrective captures.

## Validation

```sh
cargo test --workspace --offline
cargo clippy -p orbit-runtime -p orbit-server -p orbit-mcp --all-targets --offline -- -D warnings
cargo fmt --all -- --check
cd web
node node_modules/typescript/bin/tsc --noEmit
node node_modules/vitest/vitest.mjs run
node node_modules/vite/bin/vite.js build
node scripts/check-terminology.mjs
LD_LIBRARY_PATH=/tmp/pwlibs/root/usr/lib/x86_64-linux-gnu \
  node node_modules/@playwright/test/cli.js test --project=chromium
```
