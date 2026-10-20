# Development 250 — connected-mode machine monitoring

## Completed source scope

- The trusted machine worker now monitors journaled Herdr sessions after launching jobs. `--mode all` polls both queued launches and existing jobs; `--mode monitor` performs observation only; `--mode launch` preserves launch-only behavior.
- Runtime states are normalized to the existing authenticated `agent-machines/observe` API. `done`/`completed`/`idle` only indicate an idle/review-needed agent, never an accepted task.
- An unreachable or unrecognized runtime state reports `unreachable` rather than treating the session as absent, stopping jobs, or relaunching anything.
- Journals in uncertain preconfirmation phases require operator reconciliation. Launch claims are never renewed by the monitor. Per-journal errors do not stop other inspections.
- Monitoring obtains the current job revision and checks machine identity before reporting an observation.

## Validation

- Existing five machine-worker launch contract tests and six new monitoring contract tests pass under Python unittest.
- Rust build, authenticated server integration and live Herdr execution not verified in this environment (Rust and Herdr unavailable).
- This is source/isolated-worker acceptance, not an end-to-end production deployment claim.

## Progress

- M23 provisional source implementation progress: 66% -> 72% (+6 percentage points).
- M22 planning features: 100% source scope; M15 executable release acceptance pending.
- Internal-testing readiness: not established (no verified compiled build and live workflow).

## Remaining

- Durable reconciliation of unknown launches with verified Herdr sessions and operator decisions.
- Full connected-mode CLI command UX and authentication integration.
- Structured agent resumption, including handoff loading and branch/worktree reuse.
- Full compiled, authenticated multi-component and live Herdr acceptance checks.
