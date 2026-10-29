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

# Task: M33 I1–I4 — professional UX, project workspace, epics and roadmap

## Objective

Replace Orbit's decorative web typography, fix the false expired-session warning,
build a coherent issue-centric project workspace, introduce authoritative Epics
that group Tasks and Workflow Work Items, give Milestones target dates, and prove
the result with real-browser tests and captures.

## Current behaviour

The browser used Fraunces/Instrument Sans and editorial headings, a warm paper
palette and oversized panels. A missing first-visit session was reported as an
expiry because `ProtectedRoute` could not distinguish "never authenticated" from
"session ended". Projects were workflow-panel collections with no List, Backlog
or Roadmap sections and no Epic concept; milestones had status and name only.

## Desired behaviour

A neutral, compact application UI in Inter; a clean first visit and honest expiry
and revocation copy; canonical List, Board, Backlog, Roadmap and Milestones
sections that preserve workflow-declared extras; Epics as a first-class
authoritative grouping with progress, activity and same-project enforcement;
milestone target dates as checkpoints; a roadmap that shows scheduled work and
never invents dates.

## Scope

- Typography/design tokens, Inter, neutral light/dark palette.
- Auth marker and redirect-state fix, unit and browser tests.
- Canonical project navigation, List grouping/epic column, Backlog, Roadmap.
- Rust Epic command/snapshot family, task and Work Item membership, progress and
  activity; milestone `set_target`.
- Epic dialogs, epic filter, shared task-drawer field, Work Item sheet field.
- E2E coverage, evidence captures and documentation.

## Out of scope

- TUI redesign, Jira branding, mandatory sprints, Story Points, public release.
- A second frontend domain engine or client-side hierarchy calculation.

## Dependencies

- M31/M32 web client, authoritative command ledger, task/milestone services,
  Playwright harness.

## Risks

- Epic membership could be mistaken for a task field; it is a separate fenced
  command family and the UI reads the epic snapshot.
- Derived epic bars could be mistaken for authored schedules; they are dashed and
  labelled derived, and undated items stay unscheduled.
- The browser cannot distinguish expiry from revocation at the HTTP level; both
  are explained as "session ended" rather than inventing a reason.

## Acceptance criteria

- [x] AC-001: No Fraunces/Instrument Sans imports or decorative serif headings remain.
- [x] AC-002: First visit, sign-out, expiry and revocation each render the correct
  login state; no credential is stored in browser storage.
- [x] AC-003: A project exposes one canonical Overview/List/Board/Backlog/Roadmap/
  Milestones set plus its own extra views; old `/tasks` and `/timeline` links resolve.
- [x] AC-004: Epic create/edit/status/assign/unassign/delete/progress/activity work
  through Rust authority commands and survive restart.
- [x] AC-005: Cross-project epic assignment is refused and leaves no partial state.
- [x] AC-006: Milestone target dates can be set and cleared; milestones are not
  epics and have no duration.
- [x] AC-007: Roadmap supports week/month/quarter, current date, dependencies,
  unscheduled work and click-through to task/epic/milestone.
- [x] AC-008: Full chromium suite 141 passed / 5 skipped; mobile auth 6 passed.
- [x] AC-009: Unit suite 63 files / 410 tests; terminology and typecheck pass.
- [ ] AC-010: A human has reviewed the built UI and screenshots.

## Validation

```sh
cd web
node node_modules/typescript/bin/tsc --noEmit
node node_modules/vitest/vitest.mjs run
node node_modules/vite/bin/vite.js build
node scripts/check-terminology.mjs
LD_LIBRARY_PATH=/tmp/pwlibs/root/usr/lib/x86_64-linux-gnu \
  node node_modules/@playwright/test/cli.js test --project=chromium
LD_LIBRARY_PATH=/tmp/pwlibs/root/usr/lib/x86_64-linux-gnu \
  node node_modules/@playwright/test/cli.js test tests/e2e/auth.spec.ts --project=mobile-chromium
cd ..
cargo test -p orbit-runtime authority_epics authority_milestones --offline
```
