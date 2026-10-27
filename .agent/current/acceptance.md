---
template: acceptance-report
template_version: 2.0.0
template_digest: "cf2288fa6e58729b6f3b34306207c7168c34b100111c216cda2530187e055f83"
project_owned: true
record_contract: acceptance-report
record_schema_version: 1
status: passed
task_id: TASK-0012
---

# Current Acceptance Report

## Task

TASK-0012 — M32 I3: browser-authorized agent execution.

## Gate results

| Gate ID | Gate | Status | Evidence |
|---|---|---|---|
| G-0001 | Launch from an eligible task with a real executor and objective | pass | `crates/orbit-runtime/src/agent_authority.rs` (executor validation, objective in the brief, claim state, `await_executor`), `crates/orbit-storage/src/agent_jobs.rs` (roster, claim-state read, objective in brief_json), `crates/orbit-runtime/src/herdr_runtime.rs` (objective paragraph only when present); client launch dialog and task-drawer section. `agents-launch.spec.ts` 6/6 proves the cycle, the frozen objective and the unchanged task. |
| G-0002 | Executor read, unknown/inactive refusal, no-executor state | pass | `GET agent-executors` (three passes, no N+1) plus start validation; browser proofs for an unknown executor (400, no row) and a revoked roster (both triggers disabled, nothing posted); the never-claimed warning is proven in the fresh-install shape. |
| G-0003 | Preserved refusal text and pinned launch identity | pass | 40+ allowlisted storage messages now reach the wire with fixed text; `agents-launch.spec.ts` proves a retry reuses one `job_id` (exactly one row) and the dialog quotes the server. |
| G-0004 | Claim-state honesty and no task completion from a job | pass | `claim_state` on the view and four client states; resume removed on an ambiguous claim; the cycle asserts the task status unchanged in the record and in the drawer header, and the review shows **Task not completed**. |
| G-0005 | Browser acceptance and quality gates | pass | Full gates below. |

## Commands run

```text
cargo fmt --all -- --check                                   # clean
cargo test --workspace --offline                             # 1,781 passed, 0 failed, 31 binaries, exit 0
cargo test -p orbit-runtime -p orbit-server -p orbit-storage --offline       # 610 / 59 / 111 (+2)
cargo clippy -p orbit-runtime -p orbit-server -p orbit-storage -p orbit-cli --all-targets --offline -- -D warnings   # clean
cd web && node node_modules/typescript/bin/tsc --noEmit      # exit 0
cd web && node node_modules/vitest/vitest.mjs run            # 56 files, 342 tests passed
cd web && node scripts/check-terminology.mjs                 # 249 files, clean
cd web && node node_modules/vite/bin/vite.js build           # built
cd web && playwright test agents-launch agents               # 11 passed
cd web && node node_modules/@playwright/test/cli.js test     # 131 passed, 4 skipped, 0 failed
```

## Checks not run

- M15 full release profile and M27 publication verification (release
  qualification remains M15/M27; deliberately not regenerated).
- Live Herdr/model acceptance (unchanged external requirement). The executor in
  acceptance is the deterministic enrolled device, not a model.

## Waivers

- None.

## Residual risk

- The roster lists enrolled devices rather than runners: a device that claimed
  before is not proof a runner is alive now. The confirmation says so; launching is
  not disabled because the roster cannot prove absence.
- The "no executor enrolled" branch is unreachable for a signed-in browser in this
  deployment (the session device is always an active roster row); it stays as a
  defensive branch.
- The objective is optional; the form states that the brief then carries the task
  alone.
- `claimed_jobs` counts non-terminal jobs, not "running now".
