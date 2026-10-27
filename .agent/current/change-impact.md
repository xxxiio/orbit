---
template: change-impact
template_version: 1.0.0
template_digest: "8618a859250c7346e16cb5e20990e6c6d496e6b238ab8f44824028eba527e134"
project_owned: true
record_contract: change-impact
record_schema_version: 1
status: current
task_id: TASK-0013
---

# Change Impact Declaration

## Task

TASK-0013 — M32 I4: connected Workflow Capability execution.

## Impact classification

| Impact | Value | Reason | Activated gate |
|---|---|---|---|
| public_api | yes | A new capability HTTP surface: status, offers, request, show/list, claim, result, reconcile, install/authorize/revoke, executor announce/roster; `POST blueprints` now publishes against durable installs. | Compatibility gate: local capability behaviour and its tests must be unchanged; server HTTP tests. |
| persisted_data | yes | One additive table `orbit_capability_executors` and claim ownership on the connected request path; no change to existing columns. | Migration gate: open/reopen and restart tests. |
| configuration | no | No new keys. | none |
| dependencies | no | No new crate or npm dependency. | none |
| security_boundary | yes | The server must never load or run provider code; a browser session must not act as an executor or an operator; grants are the only execution authority; ambiguous outcomes are never retried. | Security gate: trust-boundary greps and tests, executor credential rules, grant checks. |
| concurrency | yes | Single execution claim, revision fencing, durable request identity, idempotent retry and atomic state+receipt settlement. | Concurrency gate: claim/replay/result/reconcile tests, restart. |
| performance_sensitive | no | Single-user server; capability reads are durable-row reads. | none |
| user_interface | yes | Capability panel with availability codes, typed input form, confirmation, states, receipt and reconcile. | UI gate: unit tests, Playwright, keyboard and 390 px. |
| deployment | no | No packaging change. | none |

## Assumptions and unknowns

- Executor identity reuses enrolled devices plus an explicit capability
  announcement; the server is never an executor (`AUTHORITY_IDENTITY` is excluded).
- The contract-only catalog is sufficient to validate inputs and availability; no
  provider code is needed to offer or request an action.
- The reference capability is the acceptance provider because it needs no external
  service.
- The local three-step settlement is left intact for the local path; the connected
  path settles state and receipt atomically.

## Required outputs

- Contract catalog, executor enrollment, capability routes with codes, atomic
  settlement, publication fix.
- Connected executor runner and connected read/request parity.
- Browser capability panel and reconcile.
- Rust, CLI and Playwright acceptance proving the trust boundary and restart
  durability.

## Test tier

- Tier 4 for the touched surface: runtime/server/storage/blueprint/CLI tests,
  clippy `-D warnings`, client typecheck and vitest, terminology guard, production
  build, the full Playwright suite against isolated real servers with the runner as
  the executor, and `getdone-validate-project`. Release qualification remains
  M15/M27.
