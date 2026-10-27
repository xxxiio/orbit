---
template: current-task
template_version: 2.0.0
template_digest: "179af91e65608fbfd07b28126ed729856e6524410956892a698b3f40e902e695"
project_owned: true
record_contract: current-task
record_schema_version: 1
id: TASK-0012
status: completed
milestone_id: MILESTONE-0032
---

# Task: M32 I3 — browser-authorized agent execution

## Objective

Let a person start an authorized agent job for a task from the browser, watch it,
read its evidence, review its handoff, and get back to the task — without making
an external agent mandatory and without the server running anything on the
browser's behalf.

## Current behaviour

The browser can inspect jobs, overview, attention, handoffs, review and resume
context, and can already submit a review or resume a finished job. It cannot start
one: `startAgentJob` exists in the API layer with no caller, there is no executor
read, no objective field, and a job whose executor never claims it is invisible —
`overview.next_action` reports `none` for `queued`/`preparing`, so a stuck launch
looks like "nothing to do". Every start refusal is also collapsed into one opaque
400, and an unknown `machine_ref` silently creates a job nothing will claim.

## Desired behaviour

An eligible task offers **Start agent job**: the person picks a real enrolled
executor, writes an objective, confirms that this queues work for an executor,
and lands on the job page showing `preparing` and whether anyone has claimed it.
No executor configured is a named disabled state, not a failure. The job page
distinguishes not-claimed, claimed, confirmed and ambiguous, refuses to offer a
retry on an ambiguous launch, and never claims that approval completes the task.
Refusals quote the server.

## Scope

- `GET agent-executors` (owner-scoped, honest empty list) and start validation of
  an unknown or inactive executor.
- Preserved refusal text for start/reserve/resume and the machine routes.
- `claim_state` on the job view and `next_action: await_executor`.
- Optional launch objective, stored in the brief, inherited by resume, threaded
  into the Herdr prompt only when present.
- Client launch surface, executor state, pinned attempt, job/overview honesty,
  unit tests.
- Playwright acceptance with a deterministic in-process fake executor.

## Out of scope

- A machine registry, leases or grants that let one device dispatch to a device it
  does not authenticate as; a real model/Herdr run in the browser path.
- Changing the state machine, owner scoping, machine fencing, review/resume
  semantics or the "no task writes" invariant.
- Connected capability execution (I4), dogfooding (I5), final acceptance (I6),
  any TUI change.

## Dependencies

- Existing agent storage/runtime lifecycle, owner and machine principals.
- The e2e fixture's machine driver recipe (`machine_ref == device id`).

## Risks

- An unknown executor that silently creates a `preparing` job is indistinguishable
  from a slow one; validation must happen before the job transaction.
- Preserving storage refusal text must not leak SQL or paths.
- The objective must not enter shell argv.
- The browser must never call `agent-machines/*` from product code, and an
  ambiguous claim must never be relaunched.

## Acceptance criteria

- [x] AC-001: An eligible task can launch an authorized job from the browser with
  a real executor and an objective, and the resulting job is visible with its
  state and claim state.
- [x] AC-002: No executor configured is a named disabled state; an unknown or
  inactive executor is refused before any job exists.
- [x] AC-003: Refusals are the server's own words, and a retry reuses the same
  launch identity.
- [x] AC-004: The job page distinguishes claim states, never offers a retry on an
  ambiguous launch, and a completed job never marks the task done.
- [x] AC-005: Rust, client and Playwright gates pass against a real server,
  including a full launch → observe → handoff → review cycle with a deterministic
  fake executor.

## Validation

```sh
cargo fmt --all -- --check
cargo test -p orbit-runtime -p orbit-server -p orbit-storage --offline
cargo clippy -p orbit-runtime -p orbit-server -p orbit-storage --all-targets --offline -- -D warnings
cd web && node node_modules/typescript/bin/tsc --noEmit
cd web && node node_modules/vitest/vitest.mjs run
cd web && node scripts/check-terminology.mjs
cd web && node node_modules/vite/bin/vite.js build
cd web && node node_modules/@playwright/test/cli.js test
getdone-validate-project
```
