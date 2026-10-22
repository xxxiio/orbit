---
template: current-task
template_version: 2.0.0
template_digest: "179af91e65608fbfd07b28126ed729856e6524410956892a698b3f40e902e695"
project_owned: true
record_contract: current-task
record_schema_version: 1
id: TASK-0005
status: completed
milestone_id: MILESTONE-0028
---

# Task: M28 usability recovery and core workflow completion

## Objective

Turn the project-centric shell into a usable everyday project manager: Basic as
the default offline workflow, rich task details with ordered acceptance
criteria, first-class local attachments, discoverable non-Vim navigation,
workflow terminology, and MCP parity for external agents acting on Basic tasks.

## Current behaviour

The project-centric TUI, blueprint views, forms, palette, search, history and PTY
harness existed. Task identity/planning existed, but there was no description,
priority, labels or acceptance criteria; no attachment capability anywhere; no
`basic` workflow and no automatic binding; persisted UI state was absent;
multiline editing was impossible; MCP could only create a task with a title and
change status; user-facing labels said "Blueprint".

## Desired behaviour

A new user can create a Basic project and fully manage a task in the TUI without
CLI workarounds or Vim knowledge, and an independent external agent can perform
the same task workflow through MCP without Orbit launching or managing an agent.

## Scope

- `basic` bundled workflow (v1) and authority-owned default binding.
- Task description (Markdown), priority, labels, ordered acceptance criteria.
- Content-addressed local attachment store with metadata, limits, GC and TUI/MCP
  operations.
- TUI detail/editing, multiline editor, criteria/attachments/activity modals,
  action bar, session restore, onboarding, workflow picker and terminology.
- MCP task detail/acceptance/activity/attachment/project/workflow tools.
- Focused Rust tests, PTY acceptance, docs and GetDone records.

## Out of scope

- Connected-mode attachment upload/synchronisation (server endpoints).
- Live Herdr agent launch/review acceptance.
- Public release verification (M27 external gate).
- Renaming Rust crates, tables or MCP `blueprint.*` identifiers.

## Dependencies

- MILESTONE-0015 Linux executable baseline.
- MILESTONE-0022 planning contracts and MILESTONE-0023 agent read models.
- MILESTONE-0025 blueprint binding/work-item surfaces (implemented surfaces only).
- Existing local authority ledger, application services and PTY harness.

## Risks

- Schema migration: additive columns/tables with defaults; legacy workspace PTY
  coverage.
- Attachment safety: content-addressed paths, symlink rejection, size limits and
  no content execution.
- False connected success: unsupported operations fail closed with clear errors.
- Default workflow churn: Basic binds only when a project has no binding and is
  never applied silently to existing projects.

## Acceptance criteria

- [x] AC-001: `basic` v1 is a validated bundled workflow with ordinary task views
  (Overview, Tasks, Board, Timeline) and no specialized work-item requirement.
- [x] AC-002: Every new project is bound to Basic by the shared authority path
  (rich and minimal creation), offline; existing bindings are never replaced
  silently; an explicit change/unpin operation exists.
- [x] AC-003: Tasks support Markdown description, priority, labels and ordered
  acceptance criteria with stable ids, completion timestamps and explicit
  add/edit/reorder/check/uncheck/remove; checking all never completes the task.
- [x] AC-004: Attachments are first-class for tasks/projects/work items with
  content-addressed storage, metadata, size limits, symlink rejection, integrity
  verification, dedup, association removal and unreferenced-object GC.
- [x] AC-005: The TUI can edit details in a multiline editor, manage criteria,
  attach/list/preview/export/remove files, inspect activity, change workflows and
  restore the active project/view after restart.
- [x] AC-006: Navigation needs no Vim knowledge: arrows/Enter/Esc/Tab/Space plus
  `/`, `?`, Ctrl+P; an action bar exposes executable actions with real keys.
- [x] AC-007: User-facing terminology says workflow; `orbit workflow` is accepted
  as a CLI alias while `orbit blueprint` and `blueprint.*` remain compatible.
- [x] AC-008: MCP provides task detail/update, acceptance criteria, activity,
  project creation (Basic default), workflow list/apply, and authorized
  attachment metadata/content tools; no operation launches an agent job.
- [x] AC-009: Focused Rust tests, PTY acceptance and an external MCP stdio
  scenario pass; workspace tests, clippy, fmt and documentation checks pass.
- [x] AC-010: Records state verified progress and remaining limitations without
  claiming connected attachments or live agent acceptance.

## Validation

```sh
cargo test --offline -p orbit-domain -p orbit-application -p orbit-storage \
  -p orbit-runtime -p orbit-blueprint -p orbit-client -p orbit-ui -p orbit-tui \
  -p orbit-mcp -p orbit-sync
cargo test --workspace --offline
cargo clippy --offline -p orbit-domain -p orbit-application -p orbit-storage \
  -p orbit-runtime -p orbit-sync -p orbit-blueprint -p orbit-client -p orbit-ui \
  -p orbit-tui -p orbit-mcp -p orbit-cli -p orbit-server --all-targets -- -D warnings
cargo fmt --all -- --check
python3 -m unittest discover -s tests/documentation
python3 -m pytest -q tests/acceptance/tui tests/acceptance/mcp
./scripts/m15-full-automated.sh --profile full
```

### Evidence

- `.agent/evidence/m28-usability-focused-tests.txt`
- `.agent/evidence/m28-usability-workspace-tests.txt`
- `.agent/evidence/m28-usability-clippy.log`
- `.agent/evidence/m28-usability-pty.txt`
- `target/tui-evidence/` real PTY transcripts.
- `.agent/evidence/m28-usability-m15.zip` (40/42; two release-workflow contract
  checks blocked by the intentionally withheld workflow file).
