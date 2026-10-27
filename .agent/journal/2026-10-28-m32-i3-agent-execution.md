---
template: journal-entry
template_version: 2.1.0
template_digest: "20e8b70b95080dbe3110fe7ad70c6bd0480c2a911af959019213e78b3a1b7814"
project_owned: true
record_contract: journal-entry
record_schema_version: 1
status: recorded
date: "2026-10-28"
task_id: TASK-0012
---

# Journal Entry: 2026-10-28 — M32 I3 browser-authorized agent execution

## Context

Through I2 the browser could inspect agent jobs and review one it never started;
starting was CLI-only. The job lifecycle itself was already complete and tested.
I3 added the launch surface and the server facts a launch needs, without making an
external agent mandatory and without the server running anything on a browser's
request.

## Work completed

- `GET /v1/authoritative/agent-executors`: enrolled devices, active flag, last
  claim, non-terminal and preparing counts; three table passes merged in Rust, the
  authority's own identity excluded, empty list valid.
- `start` validates the executor against enrolled active devices **before** the
  job transaction (`unknown agent executor`, `inactive agent executor`).
- An allowlist of fixed storage refusal messages now reaches the wire for
  start/reserve/resume/commands/machine routes instead of one opaque
  `400 invalid agent job request`; unmapped errors keep the generic text.
- `AgentJobView.claim_state` and `next_action: await_executor`; the overview groups
  unclaimed queued/preparing jobs as **Awaiting an executor**.
- Optional launch objective, trimmed and capped, frozen into the brief, inherited
  by resume, appended to the Herdr prompt as an `Objective:` paragraph only when
  present (prompt byte-identical otherwise, never argv).
- Client: launch surface from the task drawer and the Agents page with a real
  executor roster, optional objective, agent kind, repository and worktree choice;
  a confirmation that states it queues work for an executor; one pinned
  `job_id`/`at_ms` per attempt; claim-state copy; resume removed on an ambiguous
  claim; a never-claimed executor warning in the panel, the picker and the
  confirmation.
- Acceptance: `web/tests/e2e/harness/agents.ts` (deterministic executor = the
  fixture's own device credential), a real `server.restart()`, and
  `agents-launch.spec.ts` (6). `agents.spec.ts` now proves foreign-job
  invisibility with a genuinely second enrolled device.

## Decisions made

- The fake executor is the enrolled device credential, not a new stub: the
  production machine fence is `job.machine_ref == authenticated device id`, so
  driving the real machine routes is the honest test.
- A roster cannot prove that no runner exists, so a never-claimed device is
  **flagged and explained**, not disabled or hidden. Refusing would be as
  dishonest as pretending the launch works.
- The objective stayed optional: requiring it would block launches the authority
  accepts; the form nudges and says the brief then carries the task alone.
- The authority's own seeded `orbit-server` identity is excluded from the roster
  (it is not an executor); revoked enrolled devices stay listed as inactive.

## Files or components changed

- `crates/orbit-runtime/src/agent_authority.rs` + `agent_authority/**`,
  `crates/orbit-storage/src/agent_jobs.rs`,
  `crates/orbit-runtime/src/herdr_runtime.rs`, `crates/orbit-server/src/main.rs`.
- `web/src/features/agents/**` (launch model, dialog, task section, executors
  panel, claim state, overview), `web/src/api/**` agent modules,
  `web/src/features/tasks/task-drawer/task-drawer.tsx`.
- `web/tests/{unit,e2e}/**` (new `harness/agents.ts`, `agents-launch.spec.ts`,
  `server.restart()`, credential-parameterised session helpers).
- `docs/web-app.md`.

## Validation performed

- `cargo test --workspace --offline` → 1,781 passed, 0 failed, 31 binaries.
- clippy `-D warnings` clean on runtime/server/storage/cli; `cargo fmt --check`
  clean.
- `tsc --noEmit` → 0; `vitest run` → 56 files / 342 tests; terminology 249 files;
  `vite build` → built.
- Playwright: 11 focused agent checks, 131 full, 4 opt-in skips, 0 failed.

## Problems encountered

- An unknown `machine_ref` used to create a `preparing` job nothing would claim;
  validation moved before the transaction.
- Every refusal was the same opaque 400, so the dialog could not quote the server;
  an allowlist mapping fixed it without leaking SQL.
- A preparing job with no claim reported `next_action: "none"`; it now reports
  `await_executor` and the overview shows it.
- The browser's own device is always an active roster row, so "no executor
  enrolled" is unreachable in this deployment and the picker could offer a device
  that never claimed; the confirmation now says exactly that.
- A false `(no claim yet)` suffix in the executor summary was removed along with
  the unit test that pinned it.

## Remaining work

- M32 I4–I6: connected Workflow Capability execution, extended dogfooding, final
  acceptance and the TUI specification.

## New TODOs

- None.

## Risks or blockers

- Roster liveness: a device that claimed before is not proof a runner is alive
  now. Stated in the confirmation; a runner heartbeat would be the honest fix and
  is out of I3 scope.
