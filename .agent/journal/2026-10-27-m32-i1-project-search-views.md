---
template: journal-entry
template_version: 2.1.0
template_digest: "20e8b70b95080dbe3110fe7ad70c6bd0480c2a911af959019213e78b3a1b7814"
project_owned: true
record_contract: journal-entry
record_schema_version: 1
status: recorded
date: "2026-10-27"
task_id: TASK-0010
---

# Journal Entry: 2026-10-27 — M32 I1 project identity, search, saved views, timestamps

## Context

M31 shipped the browser client but left four operations reachable only from the
CLI: project rename/status, cross-entity search, saved filters and absolute task
timestamps. M32 opens with those because each sits in the middle of a daily
workflow. Baseline `d1e916a`; contracts frozen in
`.slim/deepwork/m32-web-completeness/i1-plan.md` before any code moved.

## Work completed

- Authoritative project `update`: rename and status through
  `Project::rename`/`Project::transition_to`, same receipt and revision fence as
  creation, project activity row, ledger change containing only changed fields,
  and a no-op that writes nothing and still fences first.
- `GET /v1/authoritative/project-statuses` derived from the domain edges.
- `GET /v1/authoritative/search` with three ordered groups, escaped `LIKE`,
  `limit + 1` truncation and no COUNT queries; notes reuse the file-backed vault
  search instead of a new index.
- Saved views: separate tables and command receipts outside the semantic ledger,
  per-row version fencing, device-scoped rows, soft delete.
- Task `created_at_ms`/`updated_at_ms` derived from recorded `activities`, nullable.
- Client: editable project identity form with server-offered status edges, one
  palette-backed search (debounced, aborted, keyboard-navigable), saved-view
  controls on the task list and board, Created/Updated in the task drawer.
- Documentation updated (`docs/web-app.md`) for search, identity editing, saved
  views, task metadata and the "Not available yet" list.

## Decisions made

- The C1 payload is `ProjectCommandRequest`, a superset converted `From<
  ProjectCommand>`; the create JSON is byte-identical (asserted). Adding fields to
  `ProjectCommand` would have forced edits to 17 struct literals including
  production TUI code for no wire benefit. Recorded in
  `.slim/deepwork/m32-web-completeness/api-contract.md`.
- Saved views live on the server, not in `localStorage`: they survive a browser or
  profile change, stay coherent with the authoritative deployment, and never
  modify a workflow definition.
- Timestamps are derived from recorded history instead of adding nullable columns
  and backfilling nothing; records without history report `Unknown`.
- The fence runs before domain/existence checks, so stale probes answer
  `stale server revision`. The spec asserts that ordering rather than the domain
  message.

## Files or components changed

- `crates/orbit-domain/src/project.rs`, `crates/orbit-runtime/src/{domain_commands,
  store,lib,search,saved_views}.rs` (+ `search_tests`, `saved_views_tests`,
  `project_command_tests`), `crates/orbit-server/src/main.rs`.
- `crates/orbit-client/src/rich_client_tests.rs` (pre-existing stale literal).
- `web/src/{api,app,components,domain,features,hooks}/**` for the four operations.
- `web/tests/{unit,e2e}/**` including the four new specs.
- `docs/web-app.md`, `web/tests/e2e/README.md`.

## Validation performed

- `cargo test --workspace --offline` → 1,730 passed, 0 failed, 31 binaries (exit 0).
- `cargo clippy -p orbit-domain -p orbit-application -p orbit-runtime
  -p orbit-server -p orbit-client --all-targets --offline -- -D warnings` → clean.
- `cargo fmt --all -- --check` → clean.
- `tsc --noEmit` → 0; `vitest run` → 46 files / 249 tests; `check-terminology.mjs`
  → 230 files clean; `vite build` → built.
- Playwright: 19 focused, 106 full, 4 skipped (opt-in captures), 0 failed.
- Hand-run wire check on a disposable server: create → update → replay →
  stale fence → status edges → search → task timestamps → saved views.

## Problems encountered

- The Playwright harness spawned a stale `target/debug/orbit-server` because
  `cargo test` does not rebuild the server binary; rebuilt before the browser runs
  and recorded the ordering in the progress log.
- A saved-view stale version answers 400, so the shared mutation frame never
  refreshed the list; the three saved-view mutations now invalidate on rejection.
- `orbit-client`'s workflow-binding round-trip asserted a bundled version literal
  and had been failing before M32; reproduced on a pristine worktree and fixed by
  reading the bundled descriptor.

## Remaining work

- M32 I2–I6: knowledge workflows, browser agent execution, connected capability
  execution, extended dogfooding, final acceptance and the TUI specification.

## New TODOs

- None.

## Risks or blockers

- Project completion-criteria gating is still not modelled in the authority SQL
  project row; a status move validates edges and name rules only.
