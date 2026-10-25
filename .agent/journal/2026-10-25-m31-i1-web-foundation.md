---
template: journal-entry
template_version: 2.1.0
template_digest: "20e8b70b95080dbe3110fe7ad70c6bd0480c2a911af959019213e78b3a1b7814"
project_owned: true
record_contract: journal-entry
record_schema_version: 1
status: recorded
date: "2026-10-25"
task_id: TASK-0010
---

# Journal Entry: 2026-10-25 — M31 I1: self-hosted web application foundation and daily core

## Context

Baseline `7333036` (M30 complete) shipped a mature local TUI and a connected
server whose only browser surface was the administrative `console.html`. M31
makes the browser the product: a self-hosted reference client for the same
authoritative domain, used to dogfood Orbit and to specify the next TUI
redesign. I1 delivers the foundation and everyday core against a real server.

## Work completed

- Browser authentication: `POST/GET/DELETE /v1/web/session` exchange an
  operator-issued device credential for an HttpOnly `SameSite=Strict` cookie;
  session tokens are stored only as SHA-256 hashes with sliding (12h) and
  absolute (7d) expiry, per-device login throttling (5 failures / 5 min),
  revocation-driven invalidation, and CSRF enforcement (`x-orbit-csrf: 1` plus
  `Sec-Fetch-Site` rejection) for cookie-authenticated mutations only.
- SPA served by the Rust server at `/` from `web/dist` (`ORBIT_WEB_DIST`), with
  a JSON 404 for API-shaped paths, traversal-safe file resolution, immutable
  asset caching, strict CSP without inline script; `/console` unchanged.
- React 18 + TypeScript strict + Vite + React Router + TanStack Query + Radix +
  Tailwind application: design tokens, light/dark/system themes, responsive
  two-region shell with an off-canvas navigation drawer, accessible primitives,
  command palette, error boundary and skeletons.
- Typed API layer (`web/src/api/**`) owning wire contracts, per-area endpoint
  modules, adapters that translate internal `blueprint` identifiers into
  Workflow vocabulary, error classification, query-key factory, and mutations
  with explicit `idle/pending/accepted/rejected/uncertain` disposition.
- Daily core screens: Home (server-derived readiness, overdue, due today,
  blocked with reasons, recently opened projects), Inbox (quick capture,
  triage, filters, selection, bulk actions), Projects (index + creation with
  workflow choice), project workspace with workflow-declared view tabs
  (Overview/Tasks implemented; other declared views say so), and a task drawer
  with status, priority, labels, Markdown description, acceptance criteria,
  dependencies and activity, deep-linkable via `?task=`.
- Real-browser acceptance: per-worker `orbit-server` harness with an isolated
  database, a deterministic 6-project/122-task API-seeded fixture, and seven
  Playwright specs.
- Documentation: `docs/web-app.md` user guide, milestone and status records,
  terminology enforcement script.

## Decisions made

- Cookie sessions are delegated device credentials; the existing `device()`
  guard accepts either a Bearer token or the cookie and re-checks device
  liveness through `AuthoritativeHost`, so `DELETE /v1/devices/{id}` also kills
  live browser sessions. No new authorization model was invented.
- Static serving is hand-written instead of adding `tower-http`, keeping the
  dependency surface and CSP control explicit; API paths can never fall back to
  `index.html`.
- Workflow upgrade routes recompute the preview when `fingerprint` is omitted so
  an apply can never target a version the caller was not shown.
- The browser never runs optimistic mutations; `expected_revision` is read from
  the query cache at mutate time, and every rejection renders the server answer.
- Client-side filtering over the full snapshot is accepted for I1 (the API has
  no `/tasks` query support) and recorded as a scaling risk.

## Files or components changed

- `crates/orbit-server/src/{session.rs,web.rs,main.rs,blueprints.rs}`,
  `crates/orbit-server/Cargo.toml`, `crates/orbit-server/src/remote_mcp.rs`.
- `crates/orbit-runtime/src/{store.rs,lib.rs}` (`device_active`).
- `web/**` (app, api, features, styles, scripts, tests, evidence).
- `docs/web-app.md`, `docs/README.md`, `README.md` (web app entry).
- `.agent/milestones/MILESTONE-0031.md`, `.agent/status/project-status.md`.

## Validation performed

- `cargo test -p orbit-server --offline` → 38 passed; `cargo test -p
  orbit-runtime --offline` → 543 + 2 passed; `cargo fmt --check` and
  `cargo clippy -p orbit-server -p orbit-runtime --all-targets -- -D warnings`
  clean.
- Live curl smoke against a real server: login 201 + cookie, session 200,
  snapshot 200, `/` 200 text/html, `/projects/abc` SPA 200, `/v1/nope` JSON 404,
  CSRF-less mutation 403, project create 200 with automatic Basic v1 binding.
- `tsc --noEmit` clean; 32 Vitest tests pass; `check-terminology.mjs` clean;
  `vite build` succeeds.
- `node node_modules/@playwright/test/cli.js test` → 20/20 passed (chromium 16,
  mobile-chromium 4) against real per-worker servers; covers auth incl. expiry,
  project lifecycle and duplicate-name handling, task capture/edit/status/reload,
  rejected transition, stale-revision conflict, empty workspace, terminology.

## Problems encountered

- The dev image's `npm` is the Windows interop build and fails on WSL paths;
  repository scripts are run through Linux `node node_modules/<tool>`. Recorded
  for future lanes rather than "fixed" in the repo.
- The first Playwright run failed four checks; all were product defects
  (Home "Show all" key mismatch, silent session-expiry bounce, non-keyboard row
  titles, empty-state slot fragility) and the Vitest glob also collected e2e
  specs. All were fixed with unit regression tests and a fully green suite.
- The structure scan found 34 placement/duplication findings (duplicated list
  controllers, layout importing features, a 1479-line task drawer); remediation
  landed before I2 so the workflow view registry can slot in cleanly.

## Remaining work

- I2: board/timeline views, milestones, dependencies, readiness, day/schedule
  planning with explicit approval, project settings, saved list filters.
- I3: view registry, work items, transitions, workflow apply/change, upgrade
  preview/apply, capability action discoverability.
- I4: knowledge, attachments, agents. I5: dogfooding cycle. I6: refinement,
  documentation completeness, TUI redesign specification.

## New TODOs

- None recorded outside the milestone exit criteria.

## Risks or blockers

- Client-side filtering over the snapshot will not scale past a few hundred
  tasks; server-side query support is required before M31 completion.
- `expected_revision` freshness depends on cache invalidation discipline;
  covered by unit tests but worth re-checking as mutations grow.
