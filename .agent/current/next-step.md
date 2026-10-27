---
template: next-step
template_version: 3.0.0
template_digest: "b88835ce70ef09490ad55db0e910dc05ff721384d6ccad8d45eafedd7654aaf5"
project_owned: true
record_contract: next-step
record_schema_version: 1
id: NEXT-0018
status: completed
milestone_id: MILESTONE-0032
advances_exit_criteria: [EC-001, EC-002, EC-003, EC-004, EC-009, EC-011]
---

# Next Deterministic Step

## Objective

Land M32 iteration I4: connected trusted Workflow Capability execution — install,
grant, publish, offer, request, executor claim, execution, durable receipt,
reconciliation and restart durability — proven against a real server with a
separately enrolled executor process.

## Why this is next

I1–I3 made projects, knowledge and agent work operable in the browser. Capability
execution is the last operation the local authority can do and the connected
server explicitly refuses, which is the remaining functional difference between the
two supported modes. Closing it removes the last CLI-only workflow and completes
the operation surface before dogfooding (I5) and acceptance (I6).

## Preconditions

- M32 I3 committed (`0cbb79b`); local capability execution, storage and tests ship
  and must not change.
- Frozen I4 contracts in `.slim/deepwork/m32-web-completeness/i4-plan.md`.

## Inputs

- `crates/orbit-runtime/src/{capability_composition.rs,blueprint_execution.rs}`,
  `crates/orbit-storage/src/{capability_installs.rs,blueprint_actions.rs}`,
  `crates/orbit-server/src/{main.rs,blueprints.rs}`.
- `crates/orbit-cli/src/{blueprint_commands.rs,connected_blueprint.rs}`.
- `web/src/features/workflows/work-item-sheet.tsx` and the agent modules for
  pattern reuse.
- `.slim/deepwork/m32-web-completeness/i4-plan.md`.

## Ordered actions

1. Server lane: contract-only catalog, executor enrollment, status/offers/request/
   claim/result/reconcile/install/authorize/revoke, atomic settlement, publication
   fix, tests.
2. Runner lane: `orbit capability serve --once` and connected read/request parity.
3. Client lane: capability panel with availability, typed inputs, confirmation,
   states, receipt and reconcile.
4. Acceptance lane: Playwright with the CLI runner proving the full cycle,
   refusals and restart durability.
5. Run the gates, record evidence, update milestone criteria and journal, then
   start I5.

## Expected outputs

- A capability-bearing workflow installed, granted and executable through the
  server by an enrolled executor.
- Durable receipts, a single claim, idempotent retry and explicit reconciliation.
- Proof the server and the browser never execute provider code.

## Acceptance criteria

- [x] NS-001: The catalog and executor enrollment work without any provider code
  in the server.
- [x] NS-002: Install/authorize/publish/offer/request/claim/result/reconcile all
  enforce their fences and codes.
- [x] NS-003: The connected runner executes only what it claimed and reports one
  outcome.
- [x] NS-004: The browser completes the request → receipt flow and can reconcile
  an uncertain outcome.
- [x] NS-005: Full Rust, CLI, client and Playwright gates pass on the frozen tree.
- [x] NS-006: `getdone-validate-project` exits 0.

## Validation

```sh
cargo test -p orbit-runtime -p orbit-server -p orbit-storage -p orbit-blueprint -p orbit-cli --offline
cd web && node node_modules/typescript/bin/tsc --noEmit
cd web && node node_modules/vitest/vitest.mjs run
cd web && node node_modules/@playwright/test/cli.js test
getdone-validate-project
```

## Stop conditions

- Provider code would have to run in the server or the browser.
- A workflow definition or a request body could grant execution.
- A claim reset or an automatic retry could duplicate an effect.
- A receipt could be lost while the request is settled.

## Out of scope

- Dogfooding (I5), final acceptance and the TUI specification (I6), any TUI change.
