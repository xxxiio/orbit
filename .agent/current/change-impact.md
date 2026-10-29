---
template: change-impact
template_version: 1.0.0
template_digest: "8618a859250c7346e16cb5e20990e6c6d496e6b238ab8f44824028eba527e134"
project_owned: true
record_contract: change-impact
record_schema_version: 1
status: current
task_id: TASK-0015
---

# Change Impact Declaration

## Task

TASK-0014 — M32 I5–I6: dogfooding, final acceptance, installation and TUI handoff.

## Impact classification

| Impact | Value | Reason | Activated gate |
|---|---|---|---|
| public_api | no | No new route or contract; the fixes change planner/approval behaviour and client copy within existing contracts. | Compatibility gate: workspace and browser suites. |
| persisted_data | no | No schema change. | none |
| configuration | no | No new keys; the dogfooding session is opt-in with `ORBIT_DOGFOOD=1`. | none |
| dependencies | no | No new crate or npm dependency. | none |
| security_boundary | no | No authorization or credential change. | none |
| concurrency | no | No new concurrency path; the shared deadline rule is pure. | none |
| performance_sensitive | no | The session recorded no interaction above 1 s on loopback; nothing was optimised without a measurement. | none |
| user_interface | yes | Planning failure copy, the recorded capability request, and the acceptance-criterion draft fix. | UI gate: unit tests, browser specs, 390 px and keyboard suites. |
| deployment | yes | Clean-install evidence and a documented host limitation; no tooling change. | Deployment gate: lockfile-only install plus the documented loop, and the host limitation recorded. |

## Assumptions and unknowns

- The dogfooding session is automated and labelled as such; human judgement items are
  listed rather than simulated.
- The clean-install deviation (`--ignore-scripts --os=linux --cpu=x64` plus one
  `chmod`) is a host workaround, not a tooling change; a native-Linux-npm machine is
  expected to run plain `npm ci` but that was not verifiable here.

## Required outputs

- Dogfooding record, defect table, fixes with regression tests, and a re-run proving
  the blocker is gone.
- Full acceptance numbers, clean-install evidence, and the TUI specification.

## Test tier

- Tier 4: workspace tests, clippy `-D warnings`, client typecheck and vitest,
  terminology and documentation suites, the full Playwright suite, the opt-in
  dogfooding session, and `getdone-validate-project`.
