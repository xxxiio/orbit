---
template: milestone-plan
template_version: 2.0.0
template_digest: "737b1e57a6469c8843d3ed28294d21fe5a61750b2222084a7eaac19644fa392f"
project_owned: true
record_contract: milestone-plan
record_schema_version: 1
id: MILESTONE-0033
status: active
---

# Milestone: Professional Web UX, Epic-Based Planning & Jira-Inspired Project Management

## Intended outcome

Orbit's browser client becomes a professional, issue-centric project-management
surface: one application font system, a corrected first-visit sign-in experience,
a compact project workspace with canonical List, Board, Backlog and Roadmap
sections, first-class Epics that group Tasks and Workflow Work Items without
becoming Milestones, milestone target dates, a usable dependency-aware roadmap,
and real-browser evidence at desktop and phone sizes. The TUI is not redesigned;
it receives a specification derived from the patterns the browser proved.

## Why this milestone matters

M31 and M32 shipped substantial function, but the product still looked and read
like a marketing dashboard, the project workspace was a set of workflow panels
rather than a place to run work, and there was no way to express a body of
related work. That left ordinary project management split between task rows,
milestones and ad-hoc filters. M33 is the highest-priority unfinished user-facing
work because the TUI redesign depends on a validated interaction model; building
that model in the terminal first would encode guesses.

## Scope

- Inter-based typography, neutral application palette, restrained spacing and
  removal of decorative serif/display type and excessive uppercase tracking.
- First-visit vs expired/revoked session distinction with an explicit browser
  marker that stores no credential material.
- Canonical project sections: Overview, List, Board, Backlog, Roadmap, Milestones,
  with workflow-declared extra views preserved.
- Epic domain model in the authoritative Rust ledger: create/update/status,
  task and Work Item membership, progress, activity, safe delete.
- Milestone target dates as checkpoint instants, editable from the milestone list
  and roadmap.
- Splittable roadmap: week/month/quarter, epics, task windows, milestone markers,
  dependencies, unscheduled work, current-date line.
- List grouping by epic/status/priority, epic column, URL-backed state.
- Shared inspector Epic field for tasks; Epic field for Workflow Work Items.
- Real-browser dogfooding, screenshots at desktop/laptop/phone in light and dark,
  and TUI specification addendum.

## Non-goals

- Rebuilding the web application, a second TypeScript domain engine, or a generic
  UI framework.
- Jira branding or the full Scrum model; mandatory sprints; Story Points.
- Renaming internal `blueprint` crates, routes or database fields.
- A TUI redesign or any TUI feature change.
- Reopening M31/M32; M27 public release work; Flutter; cloud services; mandatory
  server for local CLI/TUI use.

## Deliverables

- `web/src/styles/{tokens,globals}.css`, `web/tailwind.config.ts`, Inter font.
- Auth distinction in `web/src/api/session-history.ts`, `session-store.ts`,
  `protected-route.tsx`, with unit and Playwright coverage.
- `crates/orbit-runtime/src/authority_epics.rs`, route handlers, milestone target
  command, Rust tests.
- `web/src/features/epics/**`, `roadmap/**`, `backlog/**`, canonical project
  sections and List grouping.
- `web/tests/e2e/{epics,roadmap,project-workspace-nav}.spec.ts`, updated suite.
- `web/evidence/m33/**` real screenshots, `docs/web-app.md` and
  `docs/guides/tui-redesign-spec.md` updates.

## Dependencies

- MILESTONE-0031 and MILESTONE-0032 are complete.
- The authoritative command ledger, task/milestone services and the Playwright harness.
- No optional service, Python or Herdr process is required for the delivered UI.

## Risks

- Epic membership could be mistaken for a task field; it is a separate fenced command family and the UI reads the epic snapshot.
- A child-derived epic bar could be mistaken for an authored schedule; it is dashed/labelled derived and undated items stay unscheduled.
- The browser cannot distinguish expiry from revocation at the HTTP level; both are explained as "session ended" rather than inventing a reason.
- Screenshots cannot judge comfort or taste; EC-008 stays open until a human reviews.

## Ordered implementation sequence

1. Typography/tokens, Inter, auth first-visit distinction and tests.
2. Canonical project sections, List grouping, Backlog and the shared inspector field.
3. Authoritative Epic family, milestone target dates and Rust tests.
4. Roadmap, epic editor, epic filter and Work Item field.
5. Browser acceptance, captures, documentation and the TUI addendum.

## Exit criteria

- [x] EC-001: Decorative serif/display typography is gone; the product uses one
  sans-serif application face at a restrained scale.
- [x] EC-002: A first-time visitor never sees an expired-session warning; a real
  expiry or revocation is explained; sign-out stays quiet.
- [x] EC-003: Ordinary projects expose Overview, List, Board, Backlog, Roadmap and
  Milestones; workflow extras keep their own names and routes.
- [x] EC-004: Epics are authoritative, project-scoped, revision-fenced, can group
  Tasks and Work Items, report progress and survive restart.
- [x] EC-005: Milestones remain checkpoints with one optional target date, not
  durations or epic children.
- [x] EC-006: The roadmap renders epics, task windows, milestone markers,
  dependency indicators and unscheduled work at three time scales.
- [x] EC-007: Real-browser acceptance and captures at 1440×900, 1280×800 and
  390×844 in light and dark pass without page or console errors.
- [ ] EC-008: A human evaluates the built app for visual and usability quality.
- [x] EC-009: The full Rust workspace gate is green after the M33 changes.

## Evidence

- `.slim/deepwork/m33-web-ux/progress.md` and `acceptance.md`.
- `web/evidence/m33/` screenshots and README.
- `.agent/current/acceptance.md` and `.slim/deepwork/m33-web-ux/acceptance.md`.
- Playwright: 141 passed / 5 skipped chromium, 6 passed mobile auth.
- Rust: 1,848 passed / 0 failed across 31 test binaries; fmt/clippy on touched crates clean.
- Unit: 63 files / 410 tests.

## Remaining work

- EC-008: a human visual and usability review of the built app and `web/evidence/m33/`.
- Optional follow-ups after that review: per-edge status explanations and drag-to-reschedule with conflict handling.

## Next milestone

None. M33 is the current milestone; the next milestone is chosen after the human review.
