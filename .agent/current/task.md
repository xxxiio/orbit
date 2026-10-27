---
template: current-task
template_version: 2.0.0
template_digest: "179af91e65608fbfd07b28126ed729856e6524410956892a698b3f40e902e695"
project_owned: true
record_contract: current-task
record_schema_version: 1
id: TASK-0013
status: completed
milestone_id: MILESTONE-0032
---

# Task: M32 I4 — connected Workflow Capability execution

## Objective

Let a trusted Workflow Capability be requested from the browser and executed by a
separately enrolled executor through the authoritative server — with project
grants, revision fencing, durable request identity, a single execution claim,
idempotency, durable receipts and explicit reconciliation — while the browser and
the server both remain incapable of executing provider code.

## Current behaviour

Capability install, project grant, offer discovery, fenced request, single claim,
receipts and reconciliation exist for the **local** authority only. Connected
mode refuses capability execution by design, the server has no capability route,
`POST blueprints` cannot publish a manifest that declares `required_capabilities`
(it passes an empty installed set), the browser sheet states "this server exposes
no capability status" and offers no run control, and local execution writes the
provider effect, the request state and the receipt in three separate steps, which
can leave a settled request with no receipt.

## Desired behaviour

An operator can install a catalog capability and grant it to a project; a
capability-bearing workflow can be published and bound; the browser lists offered
actions with their availability code, supplies typed inputs, confirms an effectful
action, requests execution and then watches `pending → claimed → succeeded/failed`
with an immutable receipt, or settles an `uncertain` outcome explicitly. The
executor is an active enrolled device that announced the capability, claims the
request through the server, runs the provider in its own process and reports the
outcome. The server never loads provider code, a workflow definition never grants
execution, an uncertain outcome is never retried automatically, and a claim
survives restart.

## Scope

- Contract-only capability catalog split out of the trusted composition.
- Executor enrollment (announce + roster) and executor-only claim/result routes.
- Capability status, offers, request, show/list, reconcile, install/authorize/
  revoke routes with code-preserving refusals.
- Atomic request-state + receipt settlement on the connected path.
- Publication of a capability-bearing workflow against durable installs.
- A connected executor runner and connected read/request parity in the CLI.
- Browser capability panel: availability, typed inputs, confirmation, states,
  receipt, reconcile.
- Rust, CLI and browser acceptance with the runner as the executor.

## Out of scope

- Loading provider code or running it in `orbit-server` or the browser; a plugin
  marketplace; a machine registry beyond device enrollment/announcement.
- Changing local-authority capability behaviour or its tests.
- Remote agent execution (already covered by I3), dogfooding (I5), final
  acceptance and TUI redesign (I6).

## Dependencies

- M30 capability storage/runtime and the M25 blueprint authority gates.
- Device authentication and revocation (executor identity).
- The M32 I3 executor-enrollment precedent (`agent-executors`).

## Risks

- A server handler that claims and then executes would silently make the server the
  executor; the trust boundary is the primary risk of this iteration.
- Three-step local settlement can lose a receipt; the connected path must settle
  atomically.
- A claim reset on restart would duplicate effects; claims must persist.
- Accepting capabilities from a request body would let a workflow or a client grant
  itself execution.

## Acceptance criteria

- [x] AC-001: A catalog capability can be installed, granted to a project and
  published in a workflow, with unknown/unauthorized refusals.
- [x] AC-002: The browser lists offers with availability codes, submits typed
  inputs with confirmation, and shows pending/claimed/succeeded/failed/uncertain
  with an immutable receipt.
- [x] AC-003: An enrolled executor claims and executes outside the server; a second
  claim and a result from another executor are refused; claims survive restart.
- [x] AC-004: Reconciliation settles claimed/uncertain explicitly, refuses pending,
  and records the actor; the same identity replays without a duplicate effect.
- [x] AC-005: Rust, CLI, client and browser gates pass, and the full
  install → grant → publish → request → claim → execute → receipt → restart →
  retry cycle is proven against a real server.

## Validation

```sh
cargo fmt --all -- --check
cargo test -p orbit-runtime -p orbit-server -p orbit-storage -p orbit-blueprint -p orbit-cli --offline
cargo clippy -p orbit-runtime -p orbit-server -p orbit-storage -p orbit-cli --all-targets --offline -- -D warnings
cd web && node node_modules/typescript/bin/tsc --noEmit
cd web && node node_modules/vitest/vitest.mjs run
cd web && node scripts/check-terminology.mjs
cd web && node node_modules/vite/bin/vite.js build
cd web && node node_modules/@playwright/test/cli.js test
getdone-validate-project
```
