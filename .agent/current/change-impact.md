---
template: change-impact
template_version: 1.0.0
template_digest: "8618a859250c7346e16cb5e20990e6c6d496e6b238ab8f44824028eba527e134"
project_owned: true
record_contract: change-impact
record_schema_version: 1
status: current
task_id: TASK-0010
---

# Change Impact Declaration

## Task

TASK-0010 — M32 I1: project rename/status, unified search, saved views and task
timestamps.

## Impact classification

| Impact | Value | Reason | Activated gate |
|---|---|---|---|
| public_api | yes | `ProjectCommand` gains `action`/`status`; three new routes (`project-statuses`, `search`, `saved-views[/commands]`); task detail gains two timestamp fields. | Compatibility gate: existing create payloads unchanged, Rust command tests, client contract tests. |
| persisted_data | yes | Two new additive tables (`web_saved_views`, `web_saved_view_commands`) created through the existing schema-init path; no existing column changes. | Migration gate: open-an-existing-database test, saved-view durability after restart. |
| configuration | no | No new configuration keys or defaults. | none |
| dependencies | no | No new crate or npm dependency. | none |
| security_boundary | yes | Search runs server-side and is scoped by the authenticated device; saved views are actor-scoped and never readable across actors. | Security gate: actor isolation and authentication tests, Playwright authorization case. |
| concurrency | yes | Project update uses the existing admission fence; saved views use a per-row version fence with command receipts. | Concurrency gate: stale-revision, stale-version and duplicate-command tests. |
| performance_sensitive | yes | Search runs on every keystroke; it must be bounded (`limit + 1` per group, no COUNT) and cancellable client-side. | Performance gate: bounded query tests, debounce/abort in the client. |
| user_interface | yes | Project settings becomes an editing form; the palette becomes a search surface; task and board gain saved-view controls; the task drawer gains timestamps. | UI gate: unit tests, Playwright, keyboard/a11y and 390 px passes. |
| deployment | no | No installer or packaging change. | none |

## Assumptions and unknowns

- Saved views are personal presentation preferences stored by the authoritative
  server, not domain state: they must not move the authority revision or alter a
  workflow definition. This keeps one persistence model instead of adding
  browser storage that would not survive a profile change.
- Project completion criteria are not modelled in the authority SQL project row,
  so an `update` status move validates the transition edges and name rules, not
  success-criterion gating. This matches the existing authority create path and
  is recorded as a known limitation rather than simulated.
- Task timestamps are derived from recorded `activities`; a record with no
  recorded history reports `null` rather than an invented time.
- Search reuses the existing file-backed Markdown vault search; no new index is
  introduced in I1.

## Required outputs

- Authoritative project update command, project status edges read and activity
  and ledger coverage.
- Bounded grouped search endpoint and client surface.
- Saved-view storage, commands, receipts and actor scoping.
- Task timestamps in the task detail projection and the task drawer.
- Rust tests, client unit tests, Playwright acceptance and records.

## Test tier

- Tier 4 for the touched surface: `cargo test -p orbit-domain -p orbit-application
  -p orbit-runtime -p orbit-server`, clippy `-D warnings`, client typecheck,
  vitest, terminology guard, production build and the full Playwright suite
  against isolated real servers, plus `getdone-validate-project`. Release
  qualification remains M15/M27 and is not regenerated here.
