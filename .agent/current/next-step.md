---
template: next-step
template_version: 3.0.0
template_digest: "b88835ce70ef09490ad55db0e910dc05ff721384d6ccad8d45eafedd7654aaf5"
project_owned: true
record_contract: next-step
record_schema_version: 1
id: NEXT-0017
status: completed
milestone_id: MILESTONE-0032
advances_exit_criteria: [EC-001, EC-002, EC-003, EC-004, EC-009, EC-011]
---

# Next Deterministic Step

## Objective

Land M32 iteration I3: an authorized agent job can be launched from an eligible
task in the browser with a real executor and an objective, watched through its
claim states, and reviewed — proven by a full launch → observe → handoff → review
cycle against a real server with a deterministic executor.

## Why this is next

Projects, tasks, search, views and knowledge are now complete enough to work in.
The remaining daily gap is agent work: the browser can review a job it did not
start, and the only way to start one is the CLI. Until launch exists, the Agents
surface is read-only, and I5 dogfooding cannot evaluate an agent-assisted
development cycle.

## Preconditions

- M32 I2 committed (`b571d97`); the agent lifecycle, owner scoping, machine
  fencing, handoffs, reviews and resume already ship and are tested.
- Frozen I3 contracts in `.slim/deepwork/m32-web-completeness/i3-plan.md`.

## Inputs

- `crates/orbit-runtime/src/agent_authority*`, `crates/orbit-storage/src/agent_jobs.rs`,
  `crates/orbit-runtime/src/herdr_runtime.rs`, `crates/orbit-server/src/main.rs`.
- `web/src/features/agents/**`, `web/src/api/**` agent modules.
- `.slim/deepwork/m32-web-completeness/i3-plan.md`.

## Ordered actions

1. Runtime lane: executor read, machine validation, preserved refusal text, claim
   state, `await_executor`, optional objective, tests.
2. Client lane: launch surface with executor and objective, pinned attempt, job and
   overview honesty, no-executor state, unit tests.
3. Browser acceptance lane: `harness/agents.ts` deterministic executor and
   `agents-launch.spec.ts` covering the full cycle, refusals and restart.
4. Run the gates, record evidence, update milestone criteria and journal, then
   start I4.

## Expected outputs

- A working launch flow with a real executor and an objective.
- A job page that distinguishes not-claimed, claimed, confirmed and ambiguous.
- Refusals in the server's words, and no job created for an invalid executor.
- Rust, unit and browser evidence for the whole cycle.

## Acceptance criteria

- [x] NS-001: An eligible task launches a job from the browser and the job page
  shows its state and claim state.
- [x] NS-002: An unknown or inactive executor is refused before a job exists, and
  an empty executor list is a named disabled state.
- [x] NS-003: Retry reuses one launch identity; an ambiguous claim offers no retry.
- [x] NS-004: A full launch → observe → handoff → review cycle runs deterministically
  in a browser spec and leaves the task status unchanged.
- [x] NS-005: Full Rust tests, clippy, typecheck, vitest, terminology, build and
  Playwright pass on the frozen tree.
- [x] NS-006: `getdone-validate-project` exits 0.

## Validation

```sh
cargo test -p orbit-runtime -p orbit-server -p orbit-storage --offline
cd web && node node_modules/typescript/bin/tsc --noEmit
cd web && node node_modules/vitest/vitest.mjs run
cd web && node scripts/check-terminology.mjs
cd web && node node_modules/vite/bin/vite.js build
cd web && node node_modules/@playwright/test/cli.js test
getdone-validate-project
```

## Stop conditions

- A launch would require the server to run a command or a model itself.
- An unknown executor could still create a job.
- The browser would have to call an executor-only route from product code.
- Approval could be presented as task completion.

## Out of scope

- Machine registry/leases, connected capability execution (I4), dogfooding (I5),
  final acceptance and TUI work (I6).
