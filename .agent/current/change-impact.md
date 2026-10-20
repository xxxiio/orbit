---
template: change-impact
template_version: 1.0.0
template_digest: "8618a859250c7346e16cb5e20990e6c6d496e6b238ab8f44824028eba527e134"
project_owned: true
record_contract: change-impact
record_schema_version: 1
status: current
task_id: TASK-0004
---

# Change Impact Declaration

## Task

TASK-0004 — M28 project-centric TUI and blueprint UI composition.

## Impact classification

| Impact | Value | Reason | Activated gate |
|---|---|---|---|
| public_api | yes | Adds blueprint `views`, a new `orbit-ui` crate, client view-data/blueprint/agent-job reads, and new client blueprint methods. | Rust workspace compile, compatibility review and tests |
| persisted_data | no | No schema change or second store; reads use existing tables. Blueprint manifests gain optional presentation metadata only. | migration/regression tests still run |
| configuration | no | Deployment/config resolution is unchanged. | none |
| dependencies | yes | Adds `orbit-ui` and `orbit-blueprint` dependencies to client/TUI; no new external crates. | dependency review, locked/offline build and clippy |
| security_boundary | yes | Blueprint views must not grant authority, execute code or bypass authorization; mutations still use client/application services with revision checks and capability sets. | security action-authority, no-code, no-SQL/no-process and duplicate-submission tests |
| concurrency | no | TUI remains single-threaded; the submission guard is in-process only and the authority remains authoritative. | duplicate/retry tests |
| performance_sensitive | yes | Views load bounded project snapshots once per refresh; no per-frame database reads or unbounded queries. | performance bounded filters/limits and client resolution tests |
| user_interface | yes | Replaces fixed tabs with a project-centric shell, details, modals, palette, search and responsive layout. | Ratatui buffer, interaction, focus and narrow-layout tests |
| deployment | no | Local and connected deployment selection is unchanged; M28 does not publish a release. | M15 regression only |

## Assumptions and unknowns

- The local TUI is the M28 delivery target; the connected command surface remains
  on its existing adapter in this milestone.
- Knowledge and habits are global domain records; project views may reference
  them without claiming project ownership.
- Local agent review has no accepted client surface; the TUI inspects jobs and
  explains why review is disabled.
- Loading is synchronous in the local client; the contract supports a future
  asynchronous renderer.

## Required outputs

- Blueprint presentation schema and validation tests.
- Shared `orbit-ui` registry/resolution/action tests.
- Client view-data and blueprint work-item tests.
- Project-centric TUI, responsive renderer and interaction tests.
- Bundled v2 views and documentation.
- Workspace test/clippy/fmt evidence.

## Test tier

- Tier 3: user-facing UI, public view contracts and authorization-sensitive
  actions. Focused tests plus the full Rust workspace test suite and clippy are
  required; broad executable M15 acceptance remains with M15.
