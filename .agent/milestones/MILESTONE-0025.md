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
- [x] EC-003: A trusted capability can be installed through a public composition surface and an offered action can be explicitly requested, claimed, executed and reconciled end to end.
- [x] EC-004: Connected MCP blueprint writes use the canonical outbox and command receipts, including a lost-ack replay test.
- [x] EC-005: An explicit, previewable blueprint version upgrade path exists.
- [x] EC-006: Habit recurrence/streak behavior is owned by an opt-in capability package rather than core.
- [x] EC-007: Local and connected blueprint smoke passes for install, binding, typed properties, guards, templates, offline replay, receipts and restart.

## Evidence

- EC-001: `cargo test -p orbit-blueprint --locked`; `deploy/server/smoke-blueprint-workflows.py`.
- EC-002: same blueprint smoke; `crates/orbit-storage/src/blueprint_template_tests.rs`.
- EC-003: Met through M30 I1. `crates/orbit-runtime/src/capability_composition.rs` (`TrustedComposition`, offers/request/execute/receipt/reconcile), `crates/orbit-storage/src/capability_installs.rs` (installation, project grants, receipts, reference notes), CLI `orbit blueprint capability|action …` and the example manifest `blueprints/examples/capability-reference.json`. Evidence: `cargo test -p orbit-runtime -p orbit-storage` (527 + 105 passed), `python3 -m pytest -q tests/acceptance/test_m30_capability_workflow.py` (1 passed: create project → install → publish → bind → offer → refuse `not_authorized` → authorize → request → execute → receipt → restart → replay without a second note → connected refusal), TUI capability actions in `crates/orbit-tui/src/app/capability.rs` with PTY coverage. Explicit reconciliation of `claimed`/`uncertain` records a settlement without re-execution.
- EC-004: Met through M30 I2. `crates/orbit-mcp/src/blueprint_tools.rs` now queues every connected blueprint write through `AuthorityConnection::queue_blueprint_transport` before attempting the canonical flush, and reports `accepted`/`pending`/`conflict` from the outbox row; no `.post(` remains in the blueprint tools. Evidence: `cargo test -p orbit-mcp` (76 passed, incl. offline → pending → accepted replay, conflicting id, lost-ack replay with the original receipt), `python3 -m pytest -q tests/acceptance/mcp` (4 passed against the real server/MCP, `orbit_blueprint_command_receipts` count proofs).
- EC-005: Met through M30 I3. `crates/orbit-runtime/src/blueprint_upgrade.rs` adds `preview_blueprint_upgrade` (typed diff of kinds/states/fields/transitions/actions/capabilities/views, warning/blocker classification, SHA-256 fingerprint over the diff plus the live item state) and `apply_blueprint_upgrade` (requires `--confirm` plus the exact fingerprint, recomputes inside one savepoint, refuses blockers/mismatch, preserves items, idempotent). CLI `blueprint upgrade-preview|upgrade`; publish/install/apply never rebind. Evidence: `cargo test -p orbit-runtime blueprint_upgrade` (13 passed), `cargo test -p orbit-cli` (128 passed), `python3 tests/acceptance/local_cli_workflows.py` (60 passed incl. preview → refusals → apply → restart).
- EC-006: Met through M30 I4. The recurrence/streak algorithm now lives in the opt-in `crates/orbit-habit-capability` package (`orbit.habit.recurrence`, action `evaluate_periods`); `orbit-domain` keeps only occurrence/period value types and no longer exports streak computation. The runtime registers the capability in `TrustedComposition` but does not install or authorize it; application/project-evidence queries return an explicit `Unavailable` status unless it is installed (and granted for project-scoped reads), while habit counts/last-occurrence projections keep working. Evidence: `cargo test -p orbit-habit-capability -p orbit-domain -p orbit-application -p orbit-runtime` (5/56/17/542 passed), gating tests, docs `docs/guides/habits-routines-schedules.md` with executed commands.
- EC-007: `.agent/evidence/m15-final.zip` contains the passing `blueprint-workflow-smoke` check.

## Remaining work

- All exit criteria are satisfied (EC-001..EC-007). Re-run the full M15 profile before release qualification and attach the new evidence zip; no M25 criterion is outstanding.

## Next milestone

MILESTONE-0026
