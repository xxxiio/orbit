---
template: current-task
template_version: 2.0.0
template_digest: "179af91e65608fbfd07b28126ed729856e6524410956892a698b3f40e902e695"
project_owned: true
record_contract: current-task
record_schema_version: 1
id: TASK-0010
status: completed
milestone_id: MILESTONE-0032
---

# Task: M32 I1 — everyday project management

## Objective

Close the four most immediately frustrating gaps in the shipped web client:
project rename and status editing, one search across projects/tasks/knowledge,
saved task and board views, and honest task created/updated timestamps.

## Current behaviour

`project-commands` accepts only creation, so `ProjectSettings` prints "Not
supported by the API yet" for rename and shows status read-only. There is no
cross-entity search: the palette searches a static command list and each page
filters only what it already loaded. Filters live entirely in the URL and are
lost when the person navigates away. Task records carry no timestamps and no
projection exposes the recorded activity times.

## Desired behaviour

A project can be renamed and moved through legal status transitions in the
browser, fenced on the accepted revision, with the server's refusal quoted
verbatim and the new identity reflected everywhere without a reload. ⌘/Ctrl+K
searches projects, tasks and knowledge through one bounded server endpoint and
opens the correct destination. A person can save the current filter/sort/display
configuration, reopen it after a browser restart, rename and delete it, and no
workflow definition is modified by doing so. The task drawer shows authoritative
created/updated times, and `Unknown` where no history exists — never a
fabricated date.

## Scope

- `ProjectCommandAction::Update` (rename, status, no-op), project status edges
  read, project activity and ledger change, tests.
- `GET /v1/authoritative/search` with grouped bounded results over projects,
  tasks and the existing Markdown vault search.
- `GET/POST /v1/authoritative/saved-views[/commands]` with per-row version
  fencing, receipts and actor scoping — no authority revision movement.
- Task timestamps derived from recorded `activities`, exposed on task detail.
- Web: settings form, palette-backed search, saved-view controls, task metadata,
  unit tests.
- Browser acceptance for all four operations against an isolated server.

## Out of scope

- Knowledge folder/link work (I2), agent launch (I3), connected capability
  execution (I4), dogfooding (I5), final acceptance and TUI specification (I6).
- Any TUI change; local-only CLI behaviour changes; a second domain engine in
  TypeScript; new search-index infrastructure.

## Dependencies

- M31 web client, API layer, Playwright harness and session auth.
- Authoritative command engine (`PendingCommand`, receipts, revision fence).
- Existing `markdown_search` file-backed vault search.

## Risks

- Project identity must survive a rename: the id is the key for tasks, work
  items, milestone bindings and workflow bindings.
- A no-op update must be idempotent without consuming a revision, and must still
  fence on the accepted revision first.
- Saved views must never be mistaken for workflow-defined views; they must not
  touch the semantic ledger.
- Derived timestamps must degrade to `Unknown` rather than to "now".

## Acceptance criteria

- [x] AC-001: Project rename and status transition are accepted through the
  authoritative command with fencing, idempotent replay and domain validation,
  and persist across refresh.
- [x] AC-002: One search returns grouped, bounded, authorization-scoped results
  across projects, tasks and notes, and each result opens its destination.
- [x] AC-003: Saved views save/rename/delete/restore, survive a browser restart
  and never modify a pinned workflow.
- [x] AC-004: Task created/updated timestamps are authoritative and explicitly
  unknown for records without history.
- [x] AC-005: Rust, client typecheck, vitest, terminology, build and Playwright
  acceptance pass against a real server.

## Validation

```sh
cargo fmt --all -- --check
cargo test -p orbit-domain -p orbit-application -p orbit-runtime -p orbit-server --offline
cargo clippy -p orbit-domain -p orbit-application -p orbit-runtime -p orbit-server \
  --all-targets --offline -- -D warnings
cd web && node node_modules/typescript/bin/tsc --noEmit
cd web && node node_modules/vitest/vitest.mjs run
cd web && node scripts/check-terminology.mjs
cd web && node node_modules/vite/bin/vite.js build
cd web && node node_modules/@playwright/test/cli.js test
getdone-validate-project
```
