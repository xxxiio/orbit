---
template: milestone-plan
template_version: 2.0.0
template_digest: "737b1e57a6469c8843d3ed28294d21fe5a61750b2222084a7eaac19644fa392f"
project_owned: true
record_contract: milestone-plan
record_schema_version: 1
id: MILESTONE-0025
status: active
---

# Milestone: Configurable Project Blueprints

## Intended outcome

Users can define, install, pin and operate versioned project blueprints with typed work items, guarded transitions, templates and explicitly authorized capabilities in local and connected modes.

## Why this milestone matters

Blueprints are Orbit's extension model without per-project Rust code or a mandatory plugin marketplace. They must reuse the authoritative command, revision and receipt boundaries established by M24.

## Scope

- I1: versioned model, registry, binding, import/export and validation.
- I2: typed properties, states, transitions, guards, templates and event history.
- I3: trusted capability catalog, action intents, executor composition and explicit authorization.
- I4: bundled packs and opt-in domain extraction.
- I5: CLI/MCP/server surfaces, offline outbox parity, receipts and upgrade policy.

## Non-goals

- Arbitrary untrusted plugin execution or embedded scripting.
- A visual blueprint designer in this repository.
- A hosted blueprint registry or marketplace.

## Deliverables

- `crates/orbit-blueprint` model/validation/workflow code.
- Storage, runtime, client, CLI, MCP and server blueprint surfaces.
- Bundled blueprint manifests.
- Local and connected blueprint acceptance smoke.
- Blueprint reference and authoring documentation.

## Dependencies

- MILESTONE-0024 authoritative host and command receipts.

## Risks

- Capability execution is security-sensitive; any public path must fail closed without an explicit host grant.
- Blueprint version upgrades can silently remap user work; upgrades must remain explicit and previewable.
- Connected MCP writes bypassing the outbox would break offline/receipt guarantees.

## Ordered implementation sequence

1. Deliver I1 model, registry and binding.
2. Deliver I2 typed lifecycle, guards and templates.
3. Make I3 capability installation and explicit action request/execute reachable end to end.
4. Add connected MCP outbox/receipt parity and a lost-ack replay test.
5. Add an explicit blueprint upgrade/preview path.
6. Extract habit recurrence into an opt-in capability package.
7. Re-run local and connected blueprint acceptance.

## Exit criteria

- [x] EC-001: I1 manifests, registry, binding, import/export and validation are implemented and exercised.
- [x] EC-002: I2 typed properties, transitions, guards and templates are implemented and exercised.
- [ ] EC-003: A trusted capability can be installed through a public composition surface and an offered action can be explicitly requested, claimed, executed and reconciled end to end.
- [ ] EC-004: Connected MCP blueprint writes use the canonical outbox and command receipts, including a lost-ack replay test.
- [ ] EC-005: An explicit, previewable blueprint version upgrade path exists.
- [ ] EC-006: Habit recurrence/streak behavior is owned by an opt-in capability package rather than core.
- [x] EC-007: Local and connected blueprint smoke passes for install, binding, typed properties, guards, templates, offline replay, receipts and restart.

## Evidence

- EC-001: `cargo test -p orbit-blueprint --locked`; `deploy/server/smoke-blueprint-workflows.py`.
- EC-002: same blueprint smoke; `crates/orbit-storage/src/blueprint_template_tests.rs`.
- EC-003: Unmet. Executor primitives exist in `crates/orbit-runtime/src/blueprint_execution.rs`, but publish paths pass an empty installed-capability set and no CLI/MCP/HTTP action route exists.
- EC-004: Unmet. `crates/orbit-mcp/src/blueprint_tools.rs` posts directly to blueprint endpoints rather than the outbox.
- EC-005: Unmet. Binding is first-binding-wins and no upgrade operation exists.
- EC-006: Unmet. Habit recurrence remains in `crates/orbit-domain/src/habit_tracking.rs`.
- EC-007: `.agent/evidence/m15-final.zip` contains the passing `blueprint-workflow-smoke` check.

## Remaining work

- EC-003, EC-004, EC-005 and EC-006.
- Then re-run the full M15 profile and update this record with new evidence.

## Next milestone

MILESTONE-0026
