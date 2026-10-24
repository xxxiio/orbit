---
template: change-impact
template_version: 1.0.0
template_digest: "8618a859250c7346e16cb5e20990e6c6d496e6b238ab8f44824028eba527e134"
project_owned: true
record_contract: change-impact
record_schema_version: 1
status: current
task_id: TASK-0007
---

# Change Impact Declaration

## Task

TASK-0007 — M29 TUI product experience, interaction quality and maintainability.

## Impact classification

| Impact | Value | Reason | Activated gate |
|---|---|---|---|
| public_api | yes | `AuthorityConnection` gains thin queue/notes/snapshot passthroughs; TUI state gains persisted navigation fields and new modal types. No domain or server wire change. | compatibility: workspace compile, client tests, contract tests |
| persisted_data | yes | `tui-state.json` schema v1 adds component, selected id/index, scroll and detail scroll; older files load with defaults, newer versions are ignored. No authoritative entity schema change. | migration: state round-trip, corrupt/partial/newer-file tests and PTY Scenario A |
| configuration | yes | `ORBIT_TUI_LEGACY_CONNECTED` is removed; `ORBIT_TUI_STATE` remains for isolated state; `ORBIT_TUI_FULL_FIXTURE` opts into the 20/200 fixture. | compatibility: configuration and contract guards |
| dependencies | no | No new crates; Ratatui/Crossterm versions unchanged. | compatibility: locked offline build and clippy |
| security_boundary | yes | The shared shell must never open an embedded authority or local store in connected mode; connected writes remain client-queued and pending; conflict replacement fails closed. | security: authority-boundary and deployment-boundary contract tests |
| concurrency | yes | Pending flush still runs ordered through the replica; the inspector and console never treat queued work as accepted and report per-command outcomes. | concurrency: connected recovery PTY scenario, outcome tests |
| performance_sensitive | yes | Detail-pane backend reads were removed per frame and title-only quick edits avoid a whole-workspace reload; both are measured before/after. | performance: perf probe and cache-invalidation regression tests |
| user_interface | yes | Navigation/selection persistence, ranked actions, palette ranking, quick edit, forms, responsive layout, Kanban, theme, pending/conflict inspector and connected console. | ui: Rust buffer tests and PTY scenarios A–G |
| deployment | yes | One shell for local and connected operation; the legacy connected presentation and its event loop are deleted. | deployment: connected server PTY scenario, contract tests, M15 profile |

## Assumptions and unknowns

- The authority remains single-tenant; no per-device ACL was added
  (TODO-0013).
- The connected console preserves the record command grammar for families with
  no structured form; it is a deliberate capability-preserving surface, not a
  second shell.
- Interaction measurements are PTY wall-clock medians and are compared
  baseline-to-after on the same machine; they are not microbenchmarks.
- The release workflow file remains withheld; two M15 checks stay red for that
  documented reason.

## Required outputs

- Durable, id-based navigation restoration with safe fallback.
- Contextual action bar, ranked palette, quick edit and predictable forms.
- Responsive layout, windowed Kanban, readable tables/detail and no-colour theme.
- Shared-shell pending/conflict inspection and connected console with the legacy
  presentation removed.
- Cohesive module extraction, measured performance improvements and a realistic
  fixture.
- PTY Scenarios A–G, documentation, captures and updated records.

## Test tier

- Tier 3: user-facing interaction, persisted navigation, connected authority
  semantics and layout across terminal sizes. Workspace tests, clippy, fmt,
  documentation, architecture contracts, real-PTY scenarios A–G, the full
  fixture probe and the M15 product profile were run.
