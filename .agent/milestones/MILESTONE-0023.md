---
template: milestone-plan
template_version: 2.0.0
template_digest: "737b1e57a6469c8843d3ed28294d21fe5a61750b2222084a7eaac19644fa392f"
project_owned: true
record_contract: milestone-plan
record_schema_version: 1
id: MILESTONE-0023
status: completed
---

# Milestone: Agent-aware execution and local Herdr integration

## Intended outcome

Orbit can authorize, launch, observe, hand off and review an optional coding-agent job without letting agent completion silently complete an Orbit task.

## Why this milestone matters

Agent assistance must remain explicit, auditable and fenced. The local Herdr path is the first real integration that proves the authority, journal and review boundaries.

## Scope

- Agent job domain and storage.
- Authority-owned launch/reservation, monitoring, evidence, handoff and review.
- Optional Herdr adapter and local `opencode` path.
- Machine-worker Python contracts and transport.
- Connected agent outbox and server routes.

## Non-goals

- Autonomous multi-agent orchestration.
- Requiring Herdr, Python or an AI provider for ordinary Orbit use.
- Remote-machine execution as a local-testing prerequisite.

## Deliverables

- Agent modules in `orbit-domain`, `orbit-application`, `orbit-storage`, `orbit-runtime`, `orbit-client` and `orbit-server`.
- `deploy/server/agent-machine-worker.py` and worker tests.
- Deterministic agent-execution contract and opt-in live Herdr smoke.

## Dependencies

- MILESTONE-0022 planning briefs and handoffs.

## Risks

- Live agents are non-deterministic and may require credentials; mitigated by deterministic contracts plus an opt-in disposable live smoke.
- Remote worker behavior is only contract-tested; mitigated by keeping remote execution out of the local readiness claim.

## Ordered implementation sequence

1. Deliver agent job identities, states and durable storage.
2. Deliver authority-owned launch/reservation and process journal fencing.
3. Deliver observation, evidence, handoff and review without task auto-completion.
4. Deliver the optional Herdr adapter and local worker path.
5. Deliver connected routes and machine-worker contracts.
6. Run deterministic contract and live local Herdr acceptance.

## Exit criteria

- [x] EC-001: Agent job and authority modules compile and pass workspace tests.
- [x] EC-002: Deterministic fake-runtime contract proves single launch, fencing, handoff, review and no task auto-completion.
- [x] EC-003: Live local Herdr 0.9.5 launches `opencode`, monitors the session, captures read-only Git evidence, records a handoff and review, and leaves the Orbit task open.
- [x] EC-004: Python worker contract tests pass.

## Evidence

- EC-001: `cargo test --workspace --locked` passed, including `agent_execution_contract`.
- EC-002: `crates/orbit-runtime/tests/agent_execution_contract.rs` passes in the M15 profile.
- EC-003: `.agent/evidence/herdr-live.log`.
- EC-004: `.agent/evidence/m15-final.zip` contains the 46-test worker suite result.

## Remaining work

- Remote machine-worker full cycle against the real Rust server, live `codex`, live resume/continue and a Herdr version matrix remain unverified; these are M15 deployment-acceptance work.

## Next milestone

MILESTONE-0024
