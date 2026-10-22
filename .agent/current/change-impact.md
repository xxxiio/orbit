---
template: change-impact
template_version: 1.0.0
template_digest: "8618a859250c7346e16cb5e20990e6c6d496e6b238ab8f44824028eba527e134"
project_owned: true
record_contract: change-impact
record_schema_version: 1
status: current
task_id: TASK-0005
---

# Change Impact Declaration

## Task

TASK-0005 — M28 usability recovery and core workflow completion.

## Impact classification

| Impact | Value | Reason | Activated gate |
|---|---|---|---|
| public_api | yes | New client methods/DTOs for task details, acceptance criteria, attachments and workflows; new MCP tools; `orbit workflow` alias; blueprint validation now allows zero work items when views exist. | workspace compile, public-contract tests and compatibility review |
| persisted_data | yes | Schema 20 adds task description/priority, `task_labels`, `task_acceptance_criteria`, `attachment_objects` and `attachments`. Additive migration; existing rows default to empty details. | storage migration tests, legacy workspace PTY |
| configuration | yes | `ORBIT_ATTACHMENT_MAX_BYTES`, `ORBIT_VIEWER`, `ORBIT_TUI_STATE` added; defaults preserve existing behaviour. | configuration/documentation tests |
| dependencies | yes | `orbit-tui` adds `serde` (existing workspace dependency) for session state; no new external crates otherwise. | dependency review, locked offline build and clippy |
| security_boundary | yes | Attachment paths are opaque and content-addressed; filenames are display-only; symlinks rejected; size limits enforced; content is never executed; MCP attachment access is explicit and project-scoped; no MCP operation launches an agent. | security path/scope and no-code-execution tests | attachment safety tests, MCP scope/read-only tests, no-process TUI boundary test |
| concurrency | no | Commands remain serialized by the authority ledger; attachment import writes immutable objects before metadata. | duplicate-command and retry tests |
| performance_sensitive | yes | Task list attaches labels in one query; attachments are streamed only when explicitly read; no per-frame storage access added. | performance-sensitive bounded-query tests | focused test suites |
| user_interface | yes | Task detail editing, multiline editor, criteria/attachment/activity modals, action bar, onboarding banner, session restore, workflow picker and terminology changes. | Ratatui buffer tests and real-PTY scenarios |
| deployment | yes | New projects are bound to Basic on both authority creation paths; connected attachment/detail operations fail closed. | deployment-mode acceptance tests | local and connected acceptance tests |

## Assumptions and unknowns

- The local workspace is the authoritative attachment source of truth in this
  build; connected upload is intentionally unavailable.
- Acceptance-criteria completion is informational and never authorizes task
  completion.
- Agent Development remains an optional workflow; no Herdr runtime is required
  for Basic or MCP usage.

## Required outputs

- Additive schema migration and repository adapters.
- Application commands with activity and duplicate safety.
- Client and MCP surfaces with typed errors.
- TUI experience and PTY evidence.
- Documentation, backup guidance and GetDone records.

## Test tier

- Tier 3: user-facing UI, persisted data, security-sensitive file handling and
  authorization-sensitive MCP tools. Focused suites, the full Rust workspace,
  clippy, documentation tests and real-terminal acceptance were run.
