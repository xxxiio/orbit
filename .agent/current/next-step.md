---
template: next-step
template_version: 3.0.0
template_digest: "b88835ce70ef09490ad55db0e910dc05ff721384d6ccad8d45eafedd7654aaf5"
project_owned: true
record_contract: next-step
record_schema_version: 1
id: NEXT-0001
status: ready
milestone_id: MILESTONE-0025
advances_exit_criteria: [EC-003]
---

# Next Deterministic Step

## Objective

Expose trusted capability installation and an explicit local action request/execute path, then prove the complete claim/duplicate/refusal/ambiguity cycle with a deterministic fake runtime.

## Why this is next

M25 is the active milestone and EC-003 is its largest unreachable block. The model, storage, executor and agent adapter already exist; the missing work is host composition and a public request surface, not a new framework or milestone. No higher-priority unfinished work precedes it.

## Preconditions

- `.agent/evidence/m15-final.zip` records a passing 38/38 local acceptance baseline.
- Blueprint I1/I2 and command receipts are implemented.
- `crates/orbit-runtime/src/blueprint_execution.rs` provides a registry and local `agent.launch` adapter.
- All capability acceptance uses disposable directories and a fake runtime.

## Inputs

- `crates/orbit-blueprint/src/capabilities.rs`
- `crates/orbit-blueprint/src/actions.rs`
- `crates/orbit-storage/src/blueprint_actions.rs`
- `crates/orbit-runtime/src/blueprint_execution.rs`
- `crates/orbit-runtime/src/agent_authority.rs`
- `crates/orbit-cli/src/blueprint_commands.rs`
- `deploy/server/smoke-blueprint-workflows.py`
- `docs/m25-capability-executor.md`, `docs/m25-action-authority.md`

## Ordered actions

1. Add a trusted host-composition input for installed capabilities and explicit grants; default to empty.
2. Route blueprint publication through that set instead of an unconditional empty set.
3. Add one local CLI action request/status command that persists an action intent with the caller's command ID and expected revision.
4. Compose the local `agent.launch` adapter only when the trusted configuration supplies the capability and grant; otherwise fail closed.
5. Add a deterministic fake-runtime acceptance test covering success, duplicate command, unauthorized/missing capability and ambiguous outcome.
6. Assert the associated Orbit task remains incomplete after every action outcome.
7. Update the capability/action documentation to match the new surface.
8. Run focused tests, then the full canonical gate.

## Expected outputs

- Host composition accepts installed capabilities/grants.
- CLI action request/status surface.
- Deterministic fake-runtime acceptance test.
- Updated capability documentation.
- Fresh M15 evidence after the change.

## Acceptance criteria

- [ ] NS-001: Capability-requiring blueprint publication succeeds only with a trusted grant.
- [ ] NS-002: An action request is persisted and claimed exactly once; a duplicate command reconciles without a second execution.
- [ ] NS-003: Missing or unauthorized capability fails closed without a launch.
- [ ] NS-004: Ambiguous outcome remains fenced and is not relaunched.
- [ ] NS-005: The Orbit task is not completed by any action outcome.
- [ ] NS-006: The full M15 profile and Rust gates pass.

## Validation

```bash
cargo fmt --all -- --check
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
./scripts/m15-full-automated.sh --profile full
```

## Stop conditions

- Stop if the existing executor cannot be composed without introducing a new plugin loader or arbitrary command execution.
- Stop if a proposed surface would let a blueprint manifest grant itself capabilities.
- Stop if an ambiguous launch would be retried without a durable fence.
- Stop if the change requires live Herdr credentials for the deterministic gate.

## Out of scope

- Connected server action routing and MCP action parity.
- Blueprint upgrade/preview.
- Habit recurrence extraction.
- Live Herdr as an acceptance prerequisite.
- New milestone scope or a plugin marketplace.
