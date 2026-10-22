---
template: journal-entry
template_version: 2.1.0
template_digest: "20e8b70b95080dbe3110fe7ad70c6bd0480c2a911af959019213e78b3a1b7814"
project_owned: true
record_contract: journal-entry
record_schema_version: 1
status: recorded
date: "2026-10-22"
task_id: TASK-0005
---

# Journal Entry: 2026-10-22 — M28 usability recovery and Basic default workflow

## Context

The M28 project-centric shell, blueprint views and PTY harness were previously
accepted, but the product was not usable as an everyday project manager: tasks
had no description, priority, labels or acceptance criteria; attachments did not
exist; no `basic` workflow existed and new projects were never bound
automatically; MCP could only create a title and change status; multiline text
editing and persisted navigation were missing. This iteration closed those gaps
against the M28 usability-recovery brief.

## Work completed

- Added bundled `basic` v1 workflow (Overview, Tasks, Board, Timeline) with no
  specialized work items; relaxed blueprint validation to allow zero work items
  when views are declared.
- Bound Basic automatically for new projects in both authority creation paths
  (rich application creation and the minimal HTTP/MCP project command), with an
  explicit change-workflow and unpin operation; existing bindings are never
  replaced silently.
- Added typed task details (Markdown description, priority, labels) to the task
  aggregate without a JSON property bag, plus schema-20 columns and a labels
  table.
- Added ordered acceptance criteria as a separate aggregate with stable ids,
  completion timestamps, explicit add/edit/reorder/check/uncheck/remove and
  activity records.
- Added a first-class attachment subsystem: metadata repository, content-addressed
  filesystem object store, size limits, symlink rejection, atomic writes,
  integrity verification, dedup, association removal and GC.
- Extended the client and runtime with durable, retry-safe commands for details,
  criteria and attachments.
- Rebuilt the task experience in the TUI: multiline editor (Enter newline,
  Ctrl+S save), priority/labels, criteria modal, attachment modal with preview and
  explicit viewer handoff, activity history, workflow picker and action bar.
- Added persisted session state (active project, view, focus) and first-run
  guidance; removed the last user-facing "blueprint" labels and added an
  `orbit workflow` CLI alias.
- Added MCP tools for task detail/update, acceptance criteria, activity, project
  creation, workflow list/apply and attachment list/inspect/add/read/remove/
  associate, all routed through authority services with project scoping.
- Updated documentation (new task-details/attachments guide, backup contract,
  TUI keys, MCP reference) and added an external stdio MCP acceptance scenario.
- Added `orbit blueprint apply`/`unbind` and `--workflow` selection at
  server-side project creation so explicit workflow choice works in connected
  mode without replacing existing bindings silently.

## Decisions made

- Basic declares no work-item kinds; ordinary tasks remain the single core task
  model and workflows extend rather than replace it.
- Default binding is enforced inside the authority transaction, not per frontend.
- Attachment filenames are display metadata only; object identity is SHA-256 and
  objects are immutable, so retries and dedup cannot corrupt content.
- Materially unsupported connected-mode operations fail closed with actionable
  messages rather than silently queueing commands the server cannot apply.
- Checking all acceptance criteria never completes a task implicitly.

## Files or components changed

- Domain: `task.rs`, `acceptance.rs`, `attachment.rs`, `activity.rs`, `ids.rs`,
  `error.rs`.
- Application: `ports.rs`, `task_detail_commands.rs`, `attachment_commands.rs`.
- Storage: schema 20 migration, `repositories/{acceptance,attachments,entities}.rs`,
  `blueprints.rs` (replace/unpin), `row_mapping.rs`.
- Runtime: `attachment_store.rs`, `accepted_application.rs`,
  `application_services.rs`, `default_workflow.rs`, `store.rs`.
- Client: `contract.rs`, `task_details.rs`, `attachments.rs`, `unified/rich_client.rs`,
  `view_data.rs`.
- UI/TUI: task rows, action catalogue, detail/forms, new modals, action bar,
  session state, viewer handoff, help/terminology.
- MCP: `rich_tools.rs`, catalogs and schemas.
- Blueprint/CLI/docs/tests as listed in the evidence manifest.

## Validation performed

- Focused Rust: 1,238 passed, 0 failed.
- Workspace: `cargo test --workspace --offline`: 1,423 passed, 0 failed.
- Clippy `-D warnings` on 12 affected crates: clean.
- `cargo fmt --all -- --check`: clean.
- Documentation: 34 passed.
- Real terminal + MCP: `pytest -q tests/acceptance/tui tests/acceptance/mcp`:
  40 passed, including task details/criteria/attachment/session restart at
  60×18…160×45 and an external stdio MCP agent scenario.
- M15 full local profile: 40 passed, 2 failed. Both failures are the
  release-workflow contract because `.github/workflows/release.yml` is
  intentionally withheld (OAuth workflow scope); every product smoke passed.

## Problems encountered

- Existing PTY helpers assumed the old generated-default workspace and
  `q`-quits-help behaviour; updated them to the Basic default and safer help
  handling.
- The action bar made several old "wait until label disappears" predicates
  ambiguous; predicates now target form-specific text.
- Caret editing initially started at index 0, which broke rename-by-backspace;
  text/integer fields now place the caret at the end.
- Criteria/attachment reload initially ran while the modal was temporarily taken;
  list mutations now update modal state directly or reopen the modal by id.

## Remaining work

- Connected attachment upload/retrieval (scenario D) — recorded as NEXT-0009.
- Live Herdr Agent Development launch/review acceptance (scenario C).
- Optional connected parity for descriptions, criteria and attachments.
- M27 public release verification remains externally blocked.

## New TODOs

- TODO-0011: server-authoritative attachment upload and retrieval (ready).
- TODO-0012: live Herdr Agent Development acceptance (blocked on runtime).

## Risks or blockers

- Connected mode cannot yet accept attachments; it fails closed.
- Live agent acceptance remains unavailable without an authorized runtime.
- Public release verification remains an external M27 gate.
