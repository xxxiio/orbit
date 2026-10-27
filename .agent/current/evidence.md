---
template: evidence-manifest
template_version: 1.0.0
template_digest: "2787d570257f4ffbf8add20f62aa21227a4aea1a6723583e8928b5e71e09f79b"
project_owned: true
record_contract: evidence-manifest
record_schema_version: 1
status: final
task_id: TASK-0012
---

# Evidence Manifest

## Task

TASK-0012 — M32 I3: browser-authorized agent execution.

## Acceptance evidence

| Criterion ID | Status | Evidence |
|---|---|---|
| AC-001: Launch with executor and objective | pass | `agent_authority.rs` + `agent_jobs.rs` + `herdr_runtime.rs`; client `features/agents/launch/**`, `api/{endpoints,adapters,mutations,queries}/agents`; `agents-launch.spec.ts` full cycle; `web/tests/unit/features/{agent-launch-model,launch-agent-job-dialog,task-agent-job-section,agents-overview-page}.spec.*`. |
| AC-002: Executor states and refusals | pass | `GET agent-executors`, start validation, allowlisted refusal text; browser proofs for unknown and revoked executors and the never-claimed warning. |
| AC-003: Server words and launch identity | pass | `AGENT_REFUSALS` allowlist; `agents-launch.spec.ts` pinned-retry spec (one job row) and server-quoted refusal in the dialog. |
| AC-004: Claim-state honesty | pass | `claim_state` on `AgentJobView`, `await_executor` next action, four client claim states, no retry on ambiguous; task status asserted unchanged in the record and on screen. |
| AC-005: Quality gates | pass | See the gate table below. |

## Quality gate evidence

| Gate | Status | Command or artefact | Result |
|---|---|---|---|
| Workspace tests | pass | `cargo test --workspace --offline` | 1,781 passed, 0 failed, 31 binaries |
| Lints | pass | `cargo clippy … --all-targets -- -D warnings` | clean |
| Formatting | pass | `cargo fmt --all -- --check` | clean |
| Client typecheck | pass | `tsc --noEmit` | exit 0 |
| Client unit tests | pass | `vitest run` | 56 files, 342 tests |
| Terminology guard | pass | `check-terminology.mjs` | 249 files, clean |
| Production build | pass | `vite build` | built |
| Browser acceptance (I3) | pass | `playwright test agents-launch agents` | 11 passed |
| Browser acceptance (full) | pass | `playwright test` | 131 passed, 4 skipped, 0 failed |
| Acceptance record | pass | `.agent/evidence/m32-i3-acceptance.md` | written from the runs above |

## Checks not run

- M15 full release profile and M27 publication verification.
- Live Herdr/model acceptance (the acceptance executor is deterministic, not a model).

## Waivers

- None.

## Residual risk

- Enrolled-but-dormant executors look available; the confirmation states the roster
  cannot show liveness.
- The empty-roster branch is unreachable for a signed-in browser here.
