---
template: milestone-plan
template_version: 2.0.0
template_digest: "737b1e57a6469c8843d3ed28294d21fe5a61750b2222084a7eaac19644fa392f"
project_owned: true
record_contract: milestone-plan
record_schema_version: 1
id: MILESTONE-0031
status: completed
---

# Milestone: Self-hosted web application as Orbit's reference product experience

## Intended outcome

A polished self-hosted browser application, served by the existing
`orbit-server` and backed by the same authoritative domain, lets a person sign
in, manage projects and tasks, plan work, apply and upgrade workflows, browse
knowledge, handle attachments and inspect agent jobs — and is then used to run
Orbit's own development cycle, producing the evidence-based specification for
the next TUI redesign.

## Why this milestone matters

M22–M30 built real domain and application capability, all reached through the
CLI and TUI. Two problems follow. First, the only browser surface
(`crates/orbit-server/static/console.html`) is an administrative/reporting page,
so the connected server has no product interface. Second, the workflow model
(M25) is not discoverable anywhere a person can see it: views, work items,
transitions, upgrades and capability actions exist but are hidden behind
terminal conventions.

The web app is therefore both the internal dogfooding environment and the
reference experience that a future TUI redesign and a separate Flutter client
will follow. The official user-facing vocabulary is **Workflow**; internal
`blueprint` identifiers stay for compatibility.

## Scope

- React + TypeScript (strict) + Vite + React Router + TanStack Query + Radix
  primitives + Tailwind, in `web/`, with a typed API layer that owns the
  wire contracts and translates internal names.
- Browser authentication: device credential exchanged once for an HttpOnly,
  `SameSite=Strict` cookie session with sliding/absolute expiry, server-side
  revocation, CSRF defence and a login throttle. No admin rights in the UI.
- Same-origin deployment behind `orbit-server` static serving with SPA
  fallback; `/console` preserved; Vite dev server proxy for local work.
- Daily core: Home, Inbox (quick capture + triage), Projects, project
  workspace driven by the assigned workflow's declared views, task drawer.
- Project workspaces: board, timeline, milestones, dependencies, readiness,
  planning preview/approval/apply, project settings and workflow binding.
- Workflow experience: available workflows, apply/change, pinned version,
  work-item create/edit/transition, upgrade preview and confirmed upgrade,
  capability/action discoverability with honest availability.
- Knowledge (Markdown), attachments (upload/list/preview/download/remove),
  agent job/handoff/review inspection.
- Real-browser acceptance (Playwright) against an isolated authoritative
  server, a deterministic multi-project/100+ task fixture, a dogfooding cycle
  on an Orbit Development project, and a TUI redesign specification derived
  from that usage.
- User documentation that matches the shipped product.

## Non-goals

- Reopening M22–M30 or recreating the M25 registry, work-item engine,
  transitions, capability authorization or versioning.
- A TUI redesign in this milestone (blocking data-loss/regression fixes only).
- Renaming Rust crates, types, database columns or
  `/v1/authoritative/blueprints/*` routes.
- Offline browser editing, a second business-logic layer in the browser, or
  authoritative scheduling/readiness logic in TypeScript.
- Hosted cloud dependency, third-party identity provider, unrestricted CORS,
  or exposing operator/admin privileges through the product UI.
- M27 public release verification and Flutter client implementation.
- Arbitrary executable workflow plugins or JavaScript-based extensions.

## Deliverables

- `web/` application (app shell, api layer, features, styles, tests) and
  committed browser evidence under `web/evidence/`.
- `crates/orbit-server/src/{session.rs,web.rs}` plus session, CSRF and static
  serving wiring; two thin workflow upgrade routes and per-project work-item
  listing.
- Playwright harness with an isolated real server per worker and a reusable
  seed fixture.
- `docs/web-app.md` and updated documentation index; terminology enforcement
  script in `web/scripts/check-terminology.mjs`.
- A dogfooding issue log and the derived TUI redesign specification.

## Dependencies

- MILESTONE-0024 authoritative host, command receipts and revision fencing.
- MILESTONE-0025 workflow registry, work items, transitions, capabilities and
  versioning (implementation complete; reused as-is).
- MILESTONE-0028/0029/0030 project-centric TUI, interaction quality and the
  two-region workspace (must stay functional and stable).
- M27 release verification remains externally constrained and is not a
  dependency of implementation work here.

## Risks

- A browser client can drift into a second business-logic layer; every rule
  must stay server-authoritative and the client may only present results.
- Session/cookie auth is security-sensitive: no bearer token in web storage,
  no token in URLs, CSRF-resistant mutations, revocation that actually
  invalidates live sessions, and no admin surface in the product UI.
- Optimistic UI can show false acceptance; mutations must display pending,
  accepted, rejected and uncertain states honestly.
- Terminology regression (`Blueprint` leaking back into the interface) must be
  caught automatically rather than by review.
- Dogfooding can stall if the app is only demoed; the milestone requires a
  real planning-to-completion cycle with persisted evidence.
- Local-only CLI/TUI workflows must keep working; the server must never become
  mandatory for local usage.

## Ordered implementation sequence

1. I1 — foundation: server session auth/CSRF/static serving, app shell, login,
   Home, Inbox, Projects, task create/edit/status/drawer, initial Playwright.
2. I2 — project workspaces and planning: board, timeline, milestones,
   dependencies, readiness, day/schedule planning, project settings, filters.
3. I3 — workflow-driven experience: view registry, work-item editing and
   transitions, workflow apply/change, upgrade preview/apply, actions.
4. I4 — knowledge, attachments and agents.
5. I5 — real dogfooding on an Orbit Development project, defect fixes.
6. I6 — product refinement, accessibility/responsive/performance work,
   documentation, TUI redesign specification.

## Exit criteria

- [x] EC-001: A person signs in from a browser with an operator-issued device credential, receives an HttpOnly cookie session, survives refresh, is bounced to login with an explanation on expiry, and cannot use the product UI to obtain administrator privileges.
- [x] EC-002: Projects and tasks are fully manageable through the browser (create, edit, priority, labels, Markdown description, acceptance criteria, dependencies, attachments, status changes with server rejection surfaced) with persisted state after refresh and no CLI or direct-database fallback.
- [x] EC-003: Home and Inbox are functional daily surfaces whose readiness, blockers, due/overdue and planning data all come from Rust services.
- [x] EC-004: Basic, Development, Research, Learning and Habits workflows produce appropriate functional project workspaces through one component-type view registry, with work-item creation, typed fields, valid transitions, invalid-transition explanations, workflow application/change and upgrade preview plus fingerprint-confirmed upgrade. Declared Workflow Actions are listed with their capability requirements and an explicit statement that the connected server does not execute capabilities.
- [x] EC-005: No user-visible "Blueprint" text exists anywhere in the product, enforced by an automated check; internal identifiers remain unchanged.
- [x] EC-006: Knowledge documents, attachments and agent jobs/handoffs can be handled from the browser within existing authorization, size and integrity limits.
- [x] EC-007: Playwright acceptance runs against a real isolated `orbit-server` and covers authentication, project/task lifecycle, inbox triage, workflow selection/views/transitions, planning, knowledge, attachments, agent inspection, conflict rejection, session expiry, refresh and restart.
- [x] EC-008: An Orbit Development project has been actively managed through the web app for at least one planning-to-completion cycle, with a dogfooding issue log, fixes in the correct layer and regression tests.
- [x] EC-009: User documentation matches the shipped product, uses Workflow terminology, and every documented command was executed.
- [x] EC-010: A TUI redesign specification classifies each proven web interaction pattern as must-match, adapt, not-appropriate or shared-domain/API change, derived from real usage rather than assumptions.

## Evidence

- EC-001: `crates/orbit-server/src/session.rs` + `main.rs` session/CSRF handlers and the loopback HTTP tests in `crates/orbit-server`; live curl smoke (login 201 + cookie, session 200, CSRF-less mutation 403); Playwright `tests/e2e/auth.spec.ts` (wrong token, valid login + reload, sign-out, expiry message) — 20/20 suite green at commit `87b4710`.
- EC-005: `web/scripts/check-terminology.mjs` scanning 106 files with zero findings; Playwright `terminology.spec.ts` asserting rendered routes and document titles.
- EC-002/003 partial: Home, Inbox, Projects, project workspaces and the task drawer are implemented and
covered by the browser suite; attachments remain for I4.
- EC-004: I3 (commits `05f1e74..1f806b1`, correctness `c3bbf56`, structure `e03caa8`, final `3eca6c3`) —
work items render through the registry for all five workflows, transitions and guards come from the
manifest, upgrades require the previewed fingerprint and an explicit confirmation, and actions are
listed honestly with no run control. 41 Playwright checks and `web/evidence/i3/` prove it.
- Gate 3 closed at `3eca6c3` (three review attempts; the last two holds were a sheet lifecycle that
could discard a draft, a double submit, a lost-create retry, and inverted descending sorts).
- EC-003 partial: Home shows server readiness, blockers, overdue and due-today; Inbox
triages against the same snapshot. Day/schedule planning is surfaced in the project
planning tool rather than Home, which I5 dogfooding will judge.
- EC-004 partial: Basic, Development, Research, Learning and Habits workspaces resolve
through one component-type registry (nine renderers, four named refusals); work items,
transitions and upgrade confirmation remain I3.
- I2 evidence: commits `78ef2d6` (registry/board/milestones/planning/settings) and
`686eeda` (quoted domain refusals, humanized planning reasons, manifest copy); 34
Playwright checks and `web/evidence/i2/` from the deterministic fixture.
- EC-002: projects, tasks (title/description/priority/labels), acceptance criteria, dependencies, milestones, planning fields and attachments are all browser-managed with server read-back; `tasks.spec.ts`, `planning-estimates.spec.ts`, `attachments.spec.ts`, `conflict.spec.ts`.
- EC-003: `home.spec.ts` (server readiness/overdue/blocked, honest empty state) and `tasks.spec.ts` (Inbox capture/triage); planning data is surfaced in the project planning tool.
- EC-005: `check-terminology.mjs` over 217 files with zero findings plus `terminology.spec.ts` asserting rendered routes and titles.
- EC-006: `knowledge.spec.ts`, `attachments*.spec.ts`, `agents.spec.ts` against a real server, including the agent 403 path and approval leaving the task untouched.
- EC-007: 87 Playwright checks (4 opt-in evidence captures skipped) across auth, projects, tasks, board, milestones, dependencies, planning, knowledge, attachments, agents, conflicts, expiry, restart and the quality specs (a11y/keyboard/responsive).
- EC-008: `web/tests/e2e/i5/dogfood.mjs` + `web/evidence/i5/` (run on `987bcf1`, evidence `dfcfb8a`; dist/binary hashes, 30 captures, DB+WAL) and `.slim/deepwork/m31-web-app/i5-issues.md`.
- EC-009: `docs/web-app.md` rewritten for the shipped product and `docs/guides/web-quickstart.md` added; every documented command executed; `tests/documentation` + `tests/contracts` 82 passed.
- EC-010: `docs/guides/tui-redesign-spec.md` (788 lines; 7/3/1/1 classification) derived from the I5 run, with the five shared-domain follow-ups and A1-A14 acceptance checks.
- Implementation records: commits `bf2c4bd`, `87b4710`, `78ef2d6`, `686eeda`, `05f1e74..3eca6c3`, `87119af..292e898`, `972a09d`, `3f4101f`, `0ff8af9`, `987bcf1`, `dfcfb8a`, `27d7edc`, `077b4a9`; `.slim/deepwork/m31-web-app/{progress.md,api-contract.md,gate-1..5.md,structure-scan*.md,i5-issues.md}`.

## Remaining work

- None. M31 is complete; the recorded follow-ups (project rename/status, task
  timestamps, folder creation, note links, saved views, agent start, capability execution,
  the v3 creation default and the `store.rs` fallback distinction) are post-milestone work.

## Next milestone

None — M31 is complete. Post-milestone work: the TUI redesign using
`docs/guides/tui-redesign-spec.md`, the externally constrained MILESTONE-0027 release
verification, and the recorded follow-ups.
