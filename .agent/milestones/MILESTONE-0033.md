---
template: milestone-plan
template_version: 2.0.0
template_digest: "737b1e57a6469c8843d3ed28294d21fe5a61750b2222084a7eaac19644fa392f"
project_owned: true
record_contract: milestone-plan
record_schema_version: 1
id: MILESTONE-0033
status: completed
---

# Milestone: Professional Web UX, Milestone-Based Planning & Unified Project Work

## Intended outcome

Orbit's browser client becomes a professional project workspace with one
grouping concept: **Milestone**. Users can create a fully configured task in one
step, work with List, Board and Backlog as three presentations of one Tasks tab,
read a calm compact board, and plan milestones and their child work on a Roadmap.
Rust authority, revision fencing, receipts and persistence remain authoritative.
The TUI is not redesigned in this corrective phase; it receives an updated
specification only.

### Correction notice (human review, 2026-10-30)

The first M33 implementation introduced **Epic** as a second project work-grouping
entity alongside Milestones. A human visual/product review rejected that model:
Orbit already has Milestones, and Jira is a UX reference, not a domain
specification. The Epic family (`authority_epics`, `/epics` routes, `features/epics`,
Epic columns/filters/roadmap bars) is **superseded** and has been absorbed into the
canonical Milestone aggregate.

The successful I1 typography and authentication work is retained. The earlier
Epic-based TUI specification text is removed/corrected. The previously checked
criteria EC-003/EC-004/EC-006 of the Epic design are **not** accepted as proof of
the corrected design; the corrected criteria below replace them. M33 stays active.

## Why this milestone matters

M31/M32 made the browser capable but not professional. M33's first pass added
Jira-style Epics, duplicating Milestones and making the product model harder.
Correcting that is the highest-value user-facing work and the prerequisite for
any TUI redesign derived from the browser.

## Scope

- Remove Epic as an authoritative or user-facing concept. One-time deterministic
  conversion of any local Epic data into Milestones; no permanent compatibility
  layer. Preserve task, Work Item and Milestone identity.
- Extend the canonical Milestone: description, status, optional priority/owner,
  optional start and target dates, Tasks and Workflow Work Items, derived
  progress, blockers, activity, safe delete/move guards.
- Full task creation in one accepted operation: title, project, description,
  milestone, initial status, priority, deadline, estimate, acceptance criteria,
  dependencies. Keep lightweight quick capture for Inbox/keyboard.
- One top-level **Tasks** entry with a URL-backed List | Board | Backlog
  switcher and shared filters; legacy list/board/backlog links redirect.
- Compact default board cards with a locally persisted Compact/Comfortable
  density; details on demand in the existing inspector.
- Milestone-based Roadmap: duration bars when start+target exist, checkpoint
  markers when only a target exists, unscheduled otherwise; child task windows,
  progress, blocked work, dependency highlighting, week/month/quarter.
- Retain typography, authentication, project sections, workflow-declared extra
  views and the authoritative command ledger.
- Add an explicit, opt-in local-trusted browser mode for a personal loopback
  installation. It is refused on non-loopback binds, keeps every privileged
  surface separately authenticated, and never becomes a default.
- Tests, documentation, real-browser acceptance and human-review handoff.

## Non-goals

- A second grouping entity, Epic/Milestone distinction, Jira branding or Scrum
  sprints/story points.
- TUI implementation or any TUI feature change in this phase.
- Frontend-only pseudo-state for membership or progress.
- Permanent Epic compatibility infrastructure before public release.
- Reopening M31/M32; M27 release work; Flutter; cloud dependencies.

## Deliverables

- `crates/orbit-runtime/src/authority_milestones.rs` (rich aggregate, membership,
  progress, activity), Epic removal and the open-migration in `store.rs`,
  rich `TaskAction::Create`, server route updates, Rust tests.
- `web/src/api/**` rich milestone and create contracts; `features/milestones/**`;
  `features/tasks/**` create dialog and unified workspace; `features/backlog/**`;
  `features/roadmap/**`; project navigation; board density.
- Updated unit and Playwright suites, terminology check, `.agent` records.
- `crates/orbit-server/src/auth.rs` authentication policy and principal;
  `ORBIT_AUTH_MODE` startup validation; local-trusted session/CSRF/Host guards;
  `crates/orbit-server/tests/local_trusted_startup.rs`.
- `web/evidence/m33-corrective/**` light/dark desktop/phone captures and a
  restart-persistence proof.
- `web/evidence/m33-auth/**` local-trusted workspace, sidebar and Roadmap captures
  plus the scenario A/B/C results.
- `docs/web-app.md`, `docs/guides/local-web-quickstart.md`,
  `docs/guides/tui-redesign-spec.md`, this milestone record.

## Dependencies

- MILESTONE-0031 and MILESTONE-0032 are complete.
- Existing authority ledger, task/milestone services, Playwright harness.
- No optional service or cloud dependency for the delivered UI.

## Risks

- A migration that overwrites an existing `tasks.milestone_id` would silently
  move user work; the converter must let an existing assignment win.
- Rich create that issues several commands could partially fail; it must be one
  revision-fenced operation with one receipt.
- A merged Tasks workspace could hide workflow-defined Work Item views; only the
  conventional task List/Board/Backlog presentations are consolidated.
- Compact cards could hide blocked/priority cues; those remain visible without
  opening the inspector.
- Screenshots cannot judge comfort or taste; the human gate stays open.

## Ordered implementation sequence

1. Milestone authority consolidation, Work Item membership, Epic conversion and
   removal, rich atomic task create (Rust).
2. Additive web API for rich milestones and create.
3. Unified Tasks workspace, compact board, full create dialog, milestone detail
   and milestone Roadmap (parallel UI lanes).
4. Remove remaining Epic API/UI/tests; reconcile navigation and terminology.
5. Browser acceptance, persistence restart, captures, documentation and human
   review handoff.

## Exit criteria

- [x] EC-001: The product uses one sans-serif application face at a restrained
  scale (retained from the first M33 pass).
- [x] EC-002: First visit shows no expiry warning; expiry/revocation is explained
  and sign-out is quiet (retained).
- [x] EC-003: Epic is not a create/edit/filter/column/roadmap concept anywhere;
  no Epic route, dialog, API client or authority family remains.
- [x] EC-004: One canonical Milestone family supports description, status,
  optional priority/owner/start/target, Tasks and supported Work Items, derived
  progress, blockers, activity and safe delete/move; membership survives restart
  and rejects cross-project links atomically.
- [x] EC-005: A full task create sets title, project, description, milestone,
  status, priority, deadline, estimate, acceptance criteria and dependencies in
  one accepted operation; quick capture still works; retry is idempotent.
- [x] EC-006: Exactly one top-level Tasks entry presents List, Board and Backlog
  with URL-backed switching and shared filters; legacy links redirect.
- [x] EC-007: The Board is compact by default, readable at 100+ tasks, shows
  title/priority/deadline/blocked at a glance, persists density, and keeps
  authoritative movement and inspector access.
- [x] EC-008: The Roadmap shows a single Milestone hierarchy with duration bars,
  checkpoint markers, unscheduled work, child task windows, progress, blocked
  work and dependency highlighting; no Epic lanes exist.
- [x] EC-009: Real-browser acceptance passes at 1440×900 and 390×844 in light and
  dark, including create → filter → switch → edit → status → roadmap → restart.
- [x] EC-010: A human reviewed the running app and representative captures on
  2026-10-31 and accepted the corrected build for **internal dogfooding**. This is
  internal-use acceptance only; it is not public-release certification (see
  `.agent/journal/2026-10-31-m33-internal-acceptance.md`).
- [x] EC-011: A personal local installation can be started with the explicit,
  opt-in `ORBIT_AUTH_MODE=local-trusted`. Startup is refused on every
  non-loopback bind; the workspace opens with no Device ID, device token or
  session cookie; projects, tasks, Milestones, Workflows and agent operations
  keep working. Administrator, operator, executor, machine-transport and MCP
  surfaces stay separately authenticated, browser mutations keep a CSRF proof,
  non-loopback `Host` values are refused, and switching the server back to
  authenticated mode demands normal sign-in without a stale trusted UI.

## Evidence

- `.slim/deepwork/m33-web-ux/progress.md`, `corrective-plan.md`, `acceptance.md`.
- `.slim/deepwork/ses_e7076bbabffejijzQEfVyIuLrj.md` (authentication iteration).
- `web/evidence/m33-corrective/` captures and README.
- `web/evidence/m33-auth/` local-trusted captures, provenance and A/B/C results.
- `.agent/current/acceptance.md`.
- `docs/web-app.md`, `docs/guides/local-web-quickstart.md` and
  `docs/guides/tui-redesign-spec.md`.

## Remaining work

- None. Every exit criterion is met and M33 is complete. The human acceptance is
  explicitly internal-dogfooding acceptance, not public-release certification.
  Minor UX follow-ups are preserved as TODO-0015, TODO-0016 and TODO-0017; they
  do not reopen the accepted typography, Milestone, Epic-removal, unified Tasks,
  compact Board or authentication work.

## Next milestone

MILESTONE-0034
