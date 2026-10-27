---
template: change-impact
template_version: 1.0.0
template_digest: "8618a859250c7346e16cb5e20990e6c6d496e6b238ab8f44824028eba527e134"
project_owned: true
record_contract: change-impact
record_schema_version: 1
status: current
task_id: TASK-0012
---

# Change Impact Declaration

## Task

TASK-0012 — M32 I3: browser-authorized agent execution.

## Impact classification

| Impact | Value | Reason | Activated gate |
|---|---|---|---|
| public_api | yes | New `GET agent-executors`; `AgentJobView.claim_state`; `next_action: await_executor`; optional `objective` on start; preserved refusal text where one opaque 400 used to be. | Compatibility gate: existing start payloads (fixture, CLI) must still work; server HTTP tests. |
| persisted_data | yes | The objective is stored in the existing `agent_job_briefs.brief_json`; no new table or column. | Migration gate: reopen/replay tests; the brief JSON is additive. |
| configuration | no | No new keys. | none |
| dependencies | no | No new crate or npm dependency. | none |
| security_boundary | yes | Launch authorization: executor validation, owner vs. machine principals, no browser access to executor routes, objective never in shell argv, ambiguity never relaunched. | Security gate: unknown/inactive executor refusal, ownership and fencing tests unchanged, no task writes. |
| concurrency | yes | One active job per task, idempotent `job_id`, revision fences on claim/observe/command, immutable review. | Concurrency gate: replay and CAS tests. |
| performance_sensitive | no | Executor read is one pass per table on a single-user server. | none |
| user_interface | yes | Launch surface, executor state, objective, claim-state copy, overview attention, no-executor state. | UI gate: unit tests, Playwright, keyboard and 390 px. |
| deployment | no | No packaging change. | none |

## Assumptions and unknowns

- The executor is a real enrolled device credential; the browser starts, a machine
  claims. The Playwright fake executor is the same device id, which is exactly how
  the production fence works.
- No machine registry exists and this iteration does not add one: the executor read
  derives availability from enrolled devices and their claims.
- The objective is threaded into the Herdr prompt only; it is never used to build
  shell arguments.
- `queued`/`preparing` without a claim is "waiting for an executor", not a person's
  attention item, but it must not read as "nothing to do".

## Required outputs

- Executor read, machine validation, preserved refusals, claim state, objective.
- Browser launch surface, executor/no-executor state, job and overview honesty.
- Deterministic fake-executor harness and browser acceptance.
- Documentation and records.

## Test tier

- Tier 4 for the touched surface: runtime/server/storage tests, clippy
  `-D warnings`, client typecheck and vitest, terminology guard, production build,
  the full Playwright suite against isolated real servers, and
  `getdone-validate-project`. Release qualification remains M15/M27.
