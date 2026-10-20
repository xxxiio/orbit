---
template: milestone-plan
template_version: 2.0.0
template_digest: "737b1e57a6469c8843d3ed28294d21fe5a61750b2222084a7eaac19644fa392f"
project_owned: true
record_contract: milestone-plan
record_schema_version: 1
id: MILESTONE-0022
status: completed
---

# Milestone: Planning and agent-facing workflow foundations

## Intended outcome

Users and agents can plan tasks, inspect dependencies and readiness, produce daily/portfolio plans and handoffs, and review approved plans through one authoritative application model.

## Why this milestone matters

Planning is the shared language between human project control and agent execution. Later agent jobs and blueprint workflows depend on stable planning briefs, revisions and approval boundaries.

## Scope

- Planner crate, dependency/readiness analysis and task graph operations.
- Daily and portfolio planning, capacity, scenarios and approved-plan application.
- Planning briefs, focus, handoffs, navigation and review surfaces.
- CLI, TUI, MCP and server planning adapters.

## Non-goals

- Autonomous multi-agent orchestration.
- A visual planning UI outside the Rust repository.

## Deliverables

- `crates/orbit-planner` and planning modules in `orbit-runtime`.
- Planning CLI/MCP/server surfaces.
- Source-contract tests for planning and portfolio behavior.

## Dependencies

- MILESTONE-0024 consolidated application/runtime boundaries.

## Risks

- Planning behavior is broad; source-contract tests alone do not prove every workflow. Mitigated by local CLI planning/schedule acceptance and the M15 gate.

## Ordered implementation sequence

1. Deliver planner and dependency/readiness primitives.
2. Deliver daily/portfolio planning and scenario approval.
3. Deliver planning briefs, focus and handoff surfaces.
4. Wire CLI/TUI/MCP/server adapters.
5. Compile and run maintained contract tests.

## Exit criteria

- [x] EC-001: Planning and portfolio modules compile in the workspace.
- [x] EC-002: Planner and planning contract tests pass.
- [x] EC-003: Local CLI planning metadata and schedule workflows pass with restart persistence.
- [x] EC-004: Planning brief and approval rules are enforced by the shared runtime rather than a transport.

## Evidence

- EC-001: `cargo check --workspace --all-targets --locked` passed during the M15 gate.
- EC-002: `cargo test --workspace --locked` passed; planning contract tests are in `tests/contracts/`.
- EC-003: `deploy/server/smoke-local-authority.py` passes in the 37-check M15 profile.
- EC-004: `crates/orbit-runtime/src/planning_*` and `task_commands.rs` source-contract checks pass.

## Remaining work

- Add dedicated executable portfolio/scenario/approval coverage and connected planning-path acceptance; this is M15 expansion, not missing M22 source.

## Next milestone

MILESTONE-0023
