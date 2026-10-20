# Development 251 — Resumption across agent sessions

## Scope

- `orbit agent resume-context JOB_ID` reads a durable context bundle consisting of the original execution brief, most recent persisted handoff, last review, runtime session reference, worktree policy and source job metadata. Does not require Herdr and does not change authority state.
- `orbit agent resume JOB_ID` in fully local authoritative mode creates a fresh job for the same task, only when the predecessor is terminal and has a handoff. It captures that handoff into the new job's launch brief **before preparation and dispatch**, and reuses the previous agent, machine, repository and worktree setting. The actual Herdr prompt includes the stored handoff context.
- The connected-mode authenticated API supports owner-scoped `GET /v1/authoritative/agent-jobs/resume-context?id=...` for a durable handoff view. Connected execution resumption is not yet implemented.
- The captured handoff is a source record, not a guarantee that claimed tests or commits were verified. The agent must review evidence and acceptance criteria before closing a task.
- The predecessor must be terminal. No takeover of a running or uncertain session; this prevents creating another concurrent owner. A resume launch with an ambiguous response must be reconciled, not retried.
- The new brief is frozen while queued. Context cannot be changed after preparation. Repeated matching attachment is idempotent, conflicting attachment is rejected.

## Scope limitation

The user-facing `orbit agent resume` makes a **new** local session. Resuming in an existing Herdr session, remote execution from the CLI, asynchronous monitoring, and full end-to-end Rust acceptance remain outstanding. This change does not close any tasks automatically.

## Validation

- `python -m unittest discover -s deploy/server -p 'test_agent_machine*.py' -q`: 11 passed (worker regression suite).
- Source delimiter checks passed on all five edited Rust modules.
- Added two Rust storage tests for durable context, idempotency, active predecessor and missing handoff; **not executed** because Rust/Cargo are unavailable.
- No claim of a compiled build or live Herdr smoke test.

## Milestone accounting

M23 previously reported at 72% provisional implementation scope. This package adds new durable local resumption and connected context retrieval; provisional source-scope planning estimate 77%, pending compiled acceptance. M15 remains pending; M22 remains source-scope complete. Internal testing readiness not yet established.
