---
template: milestone-plan
template_version: 2.0.0
template_digest: "737b1e57a6469c8843d3ed28294d21fe5a61750b2222084a7eaac19644fa392f"
project_owned: true
record_contract: milestone-plan
record_schema_version: 1
id: MILESTONE-0032
status: active
---

# Milestone: Web Product Completeness, Agent Workflows & Extended Dogfooding

## Intended outcome

The M31 web client becomes a daily-use project-management product: project
identity is editable, one search finds projects, tasks and knowledge, saved
views reopen with their filters, knowledge folders and note links are managed,
an authorized agent job can be launched and reviewed from the browser, and a
trusted Workflow Capability can be executed through the authoritative server.
An extended Orbit Development usage cycle produces the defects and fixes, and
the TUI redesign specification is updated from what the browser actually
proved.

## Why this milestone matters

M31 shipped a real browser product but left the ordinary editing loops
incomplete: there is no way to rename a project, change its status, find a task
across projects, keep a filter, add a folder, link two notes, or start an agent
job without the CLI. Those are the operations a person performs every day, so
each one is a workaround in the middle of a workflow. The same is true of
connected Workflow Capability execution, which the local TUI can do and the
server explicitly refuses.

M32 is therefore the highest-priority unfinished product work: it finishes the
operations M31 could only inspect, and it uses the result for real development
management so the TUI redesign that follows is based on proven browser
workflows instead of speculation. The TUI stays unchanged in M32.

## Scope

- Project rename, project status transitions, project metadata editing, with
  revision fencing and idempotent receipts in the authoritative command path.
- One server-backed search across projects, tasks and knowledge with grouped,
  navigable, bounded results.
- Saved views as personal presentation preferences, separate from workflow
  definitions, surviving browser restart.
- Authoritative task created/updated timestamps, with explicit unknown state
  for records that have no recorded history.
- Knowledge folders (create/move/rename/browse), document move/rename,
  document links with back-references and broken-link indication, and
  project-contextual knowledge.
- Browser-authorized agent job launch, monitoring, evidence, handoff and
  review, using the existing job orchestration services.
- Connected Workflow Capability execution: trusted registration, install,
  project grant, offer discovery, typed request, single claim, receipt,
  idempotent retry and explicit reconciliation through the server.
- Extended browser dogfooding on a persistent Orbit Development workspace with
  fixes in the layer that owns each defect.
- Reproducible clean frontend installation, final real-browser acceptance, and
  the updated `docs/guides/tui-redesign-spec.md`.

## Non-goals

- Rebuilding the M31 web application, or a second TypeScript domain engine.
- Any TUI redesign or TUI feature change (blocking regressions only).
- Reopening MILESTONE-0022 through MILESTONE-0031 or M27 release publication.
- Flutter, arbitrary executable workflow plugins, a public plugin marketplace,
  cloud AI, mandatory Herdr, or multi-tenant infrastructure.
- A server dependency in local-only CLI/TUI operation.
- New search-index infrastructure, new content storage models, or direct SQL in
  HTTP handlers.
- M15 evidence regeneration; release qualification remains M15/M27 work.

## Deliverables

- Rust command/query surface for project update, project status edges, unified
  search, task timestamps and saved views, with tests.
- React features for project settings editing, global search, saved views,
  task metadata, knowledge folders/links and the agent launch flow.
- Connected capability execution route plus client wiring and reconciliation.
- Playwright acceptance against a real isolated `orbit-server` for every new
  operation, including conflicts, restart persistence and authorization.
- Extended dogfooding evidence on a persistent isolated workspace.
- Updated `docs/web-app.md`, `docs/guides/tui-redesign-spec.md` and the
  documentation index.
- Durable records: milestone, iteration evidence and journal entries.

## Dependencies

- MILESTONE-0031 web client, Playwright harness, session auth, terminology
  guard and API contract documentation (reused as-is).
- MILESTONE-0024 authoritative host, command receipts, revision fencing and
  the accepted projection.
- MILESTONE-0025 workflow registry, work items, capabilities, installs and
  project grants.
- MILESTONE-0030 opt-in capability package model and the trusted provider
  registry.
- Existing agent job orchestration, machine fencing and handoff/review services.

## Risks

- A presentation feature (saved views, search) could leak into the domain
  revision or workflow definitions; it must stay in its own store and must
  never mutate a pinned workflow.
- Project rename is an identity-adjacent write: the id must stay stable, no
  relationship may be orphaned, and the ledger change must be fenced.
- Deriving task timestamps from recorded activity is only honest if records
  without history report unknown instead of a fabricated time.
- A browser capability/agent launch could become an execution bypass; the
  server must persist an authorized, fenced intent and never run provider code
  itself.
- Dogfooding can degrade into scripted demos; the record must label automated
  interaction as automated and state what still needs a human operator.
- Local-only CLI/TUI must keep working without a server; every new command
  needs a local path or an explicit local refusal.

## Ordered implementation sequence

1. I1 — everyday project management: project rename/status, unified search,
   saved views, task timestamps.
2. I2 — knowledge workflows: folders, document move/rename, note links and
   back-references, project-contextual knowledge.
3. I3 — browser agent execution: launch UX, authorized intent, monitoring,
   evidence, handoff, review, no-executor state.
4. I4 — connected Workflow Capability execution: server composition path,
   grants, typed inputs, claims, receipts, reconciliation, parity.
5. I5 — extended dogfooding on a persistent Orbit Development workspace and
   fixes for the highest-impact findings.
6. I6 — final acceptance, reproducible installation and TUI redesign
   specification handoff.

## Exit criteria

- [x] EC-001: A project can be renamed and its status changed in the browser
  through authoritative commands, with stable identity, revision fencing,
  idempotent retry, server-side validation and persistence across refresh.
- [x] EC-002: One search finds projects, tasks and knowledge from any screen,
  through a server endpoint with bounded, authorization-aware results, and each
  result opens the correct destination.
- [x] EC-003: Saved task/board views can be saved, renamed, deleted and
  restored, persist across a browser restart, and never modify a pinned
  workflow definition.
- [x] EC-004: Task created/updated timestamps come from authoritative recorded
  history, are displayed in a readable form with full precision available, and
  report unknown for records without history.
- [x] EC-005: Knowledge folders and documents can be created, moved, renamed
  and browsed; documents can be linked with navigable back-references and
  broken-link indication; knowledge is reachable from project context.
- [x] EC-006: An authorized agent job can be launched from an eligible task in
  the browser, monitored, its evidence and handoff reviewed, and approved or
  rejected, with restart persistence, authorization failures surfaced and no
  automatic task completion.
- [x] EC-007: A trusted Workflow Capability provider can be registered,
  installed, granted, discovered, requested with typed inputs and executed
  through the authoritative server, with durable receipts, retry-safe
  idempotency and explicit uncertain-outcome reconciliation.
- [ ] EC-008: Orbit Development work is managed through the browser across
  repeated sessions on a persistent workspace, with recorded defects, friction
  and layer-correct fixes plus regression coverage.
- [ ] EC-009: Every new operation has real-browser acceptance against an
  isolated authoritative server, including conflicts, restarts, authorization
  and empty/error states.
- [ ] EC-010: A clean frontend installation is reproducible in a supported
  environment (install, build, Rust build, launch, browser suite).
- [ ] EC-011: User-visible vocabulary uses Workflow throughout, enforced by the
  existing terminology check, and `docs/web-app.md` matches shipped behaviour.
- [ ] EC-012: `docs/guides/tui-redesign-spec.md` reflects proven M31/M32
  browser usage and classifies each pattern as must-match, adapt,
  not-appropriate-in-terminal or requiring shared Rust services. No TUI
  redesign is implemented.

## Evidence

- EC-001: `POST /v1/authoritative/project-commands` with `action:"update"`
  (`crates/orbit-runtime/src/store.rs`, `crates/orbit-domain/src/project.rs`
  `rename`/`restore`/`ProjectStatus::parse`), `GET project-statuses`, 8 runtime
  command tests and 5 server HTTP tests; `web/tests/e2e/project-settings.spec.ts`
  (6 checks: rename across surfaces + stable id, status persistence, server edges
  and an illegal move, stale revision, cancel, blank name).
- EC-002: `crates/orbit-runtime/src/search.rs` (7 tests: grouping/order/
  subtitles, wildcard escaping, truncation without counts, limit clamp, project
  filter, empty/oversized query) and `GET /v1/authoritative/search`;
  `web/tests/e2e/search.spec.ts` (5 checks: three kinds, destinations, empty
  query, keyboard, unauthenticated refusal, stale target).
- EC-003: `crates/orbit-runtime/src/saved_views.rs` (9 tests: create/rename/
  delete, stale version, actor isolation, receipt replay, ordering, and the
  authority revision provably unchanged);
  `web/tests/e2e/saved-views.spec.ts` (6 checks including a second browser
  context after a sign-in and an unchanged workflow binding + ledger).
- EC-004: `AuthorityStore::task_timestamps` derived from `activities`,
  `TaskDetailResponse.created_at_ms`/`updated_at_ms`, runtime known/unknown/
  advanced tests; `web/tests/e2e/task-metadata.spec.ts` (2 checks) plus the
  `Unknown` branch unit test.
- Gate evidence: `.agent/evidence/m32-i1-acceptance.md` — `cargo test
  --workspace --offline` green, clippy `-D warnings` clean across the touched
  crates, fmt clean, client `tsc` clean, vitest 46 files / 249 tests, terminology
  230 files, production build, Playwright 106 passed / 4 skipped / 0 failed.
- EC-005: `crates/orbit-runtime/src/references.rs` (shared typed-reference
  grammar, 19 tests), `markdown-reference-commands` / `markdown-references` /
  `project-knowledge` routes, and the folder-command journal fix that stops a
  refused folder operation from blocking every later write (6 regression tests).
  Browser: `knowledge-folders.spec.ts` (8), `knowledge-links.spec.ts` (4),
  `project-knowledge.spec.ts` (7) — every claim is read back from the server as
  well as asserted on screen. Folder create/rename/move, note create/rename/move,
  link and unlink with back-references and unresolved targets, project/task
  references with broken-target display, the project knowledge panel and the task
  Notes section all ship; note/folder deletion and work-item references are stated
  as absent.
- EC-006: `GET agent-executors` with executor validation at start, preserved
  refusal text, `AgentJobView.claim_state`, `next_action: await_executor`, and an
  optional objective frozen into the brief (`crates/orbit-runtime/src/
  agent_authority.rs`, `crates/orbit-storage/src/agent_jobs.rs`, `crates/orbit-
  runtime/src/herdr_runtime.rs`); client launch surface with a real executor
  roster, never-claimed warning, pinned launch identity and claim-state honesty.
  `web/tests/e2e/agents-launch.spec.ts` (6) proves launch → claim → launch-result
  → observe → handoff → review, the objective in the frozen brief, the task status
  unchanged in the record and on screen, an unknown executor refused with no job
  row, one `job_id` across a retry, awaiting-executor visibility, and persistence
  across a real server restart. `agents.spec.ts` now proves a foreign job is
  invisible (403) for a genuinely second device.
- EC-007: A contract-only capability catalog (`crates/orbit-runtime/src/
  capability_contracts.rs`), durable executor enrollment, the capability routes
  (status, offers, request, claim, result, show/list, reconcile, install/authorize/
  revoke) in `capability_server.rs` and `orbit-server/src/main.rs`, one-savepoint
  state+receipt+**declared-effect** settlement, the C25 publication fix against
  durable installs, and `orbit capability serve --once` as the only process that
  runs provider code. `grep` finds no `TrustedComposition`, executor registry or
  `provider.execute` in `orbit-server`; a runtime test asserts the provider effect
  never happens in-process. `web/tests/e2e/capability-execution.spec.ts` (5) drives
  the real CLI executor through install → publish → item → authorize → announce →
  offer → typed request → claim → execute → receipt → durable effect note →
  restart → idle, plus uninstalled/unauthorized/invalid-input/stale-revision/
  forged-body refusals, single claim, replay, and reconcile of `uncertain` and
  `claimed`.
- EC-008 and EC-010..EC-012 stay open until their iteration lands.

## Remaining work

- I2 (knowledge folders, document move/rename, note links, project context), I3
  (browser agent execution), I4 (connected capability execution), I5 (extended
  dogfooding) and I6 (final acceptance, clean install, TUI specification) remain.

## Next milestone

The TUI redesign using `docs/guides/tui-redesign-spec.md`, derived from M31/M32
browser usage.
