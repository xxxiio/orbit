---
template: current-task
template_version: 2.0.0
template_digest: "179af91e65608fbfd07b28126ed729856e6524410956892a698b3f40e902e695"
project_owned: true
record_contract: current-task
record_schema_version: 1
id: TASK-0002
status: active
milestone_id: MILESTONE-0025
---

# Current Task

## Objective

Make the existing trusted blueprint capability path reachable end to end and prove it with a deterministic fake-runtime acceptance test.

## Current behaviour

Blueprint I1/I2 and normal command receipts work. Capability-requiring blueprints cannot be published through any public path because the CLI, server and MCP publish calls pass an empty installed-capability set. `BlueprintExecutorRegistry`, `LocalAgentLaunchCapability` and `AuthoritativeHost::request_project_work_item_action` exist but have no public callers. There is no CLI/MCP/HTTP action request or status surface.

## Desired behaviour

A trusted host configuration can supply installed capabilities and grants; a user can request an offered action against a blueprint work item with a stable command ID; the action intent is persisted and claimed exactly once; duplicate requests reconcile; missing/unauthorized capabilities fail closed; an uncertain outcome remains fenced and is never relaunched; the agent action does not complete the Orbit task.

## Scope

- Host composition input for installed capabilities and explicit grants.
- One explicit local CLI action request/status surface.
- Publish path that accepts the trusted installed-capability set.
- Deterministic fake-runtime acceptance covering success, duplicate, refusal and ambiguity.
- Documentation update for the new surface.

## Out of scope

- Connected server action routing and MCP action parity (next M25-I5 step).
- Real live Herdr execution as a gate; the existing live smoke remains separate and opt-in.
- Blueprint version upgrades and habit recurrence extraction.
- A plugin marketplace or untrusted plugin loader.

## Dependencies

- MILESTONE-0025 I1/I2 model, storage and command receipt paths.
- Existing `crates/orbit-runtime/src/blueprint_execution.rs` executor primitives.
- `crates/orbit-runtime/src/agent_authority.rs` local launch adapter.

## Risks

- A capability path that launches a real process before authorization would be unsafe; every test must use a fake runtime and fail-closed assertions.
- Ambiguous launch outcomes must never be retried blindly; the journal/uncertain state must be durable.
- A public surface could be mistaken for permission to run arbitrary capabilities; grants must come from trusted host configuration, not manifests.

## Acceptance criteria

- [ ] AC-001: A blueprint requiring `agent.launch` can be published when and only when the trusted host supplies that capability and grant.
- [ ] AC-002: A CLI action request persists a stable command ID and returns a pending/claimed status without launching an unregistered capability.
- [ ] AC-003: A deterministic fake runtime proves single claim/execution, duplicate-command reconciliation, unauthorized refusal and ambiguous-outcome fencing.
- [ ] AC-004: The action path never completes the associated Orbit task.
- [ ] AC-005: The full M15 profile, 1,235 Rust tests, fmt and Clippy still pass.

## Validation

```bash
cargo fmt --all -- --check
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
./scripts/m15-full-automated.sh --profile full
```
