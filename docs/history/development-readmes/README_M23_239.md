> **Historical checkpoint only.** Not current CLI or deployment instructions. See the [root README](../../../README.md) and [current architecture](../../architecture.md).

# Development 239: local agent monitoring vertical slice

## What changed

- Added `agent_monitor::reconcile_job`, bridging Herdr's runtime observation to authority-owned SQLite CAS transitions.
- Expanded `orbit-agent-executor` with explicit `launch`, `inspect`, and `attention` operations. The original positional launch invocation is retained.
- Monitoring uses the persisted launch token and never creates a second launch. Terminal jobs and stale revisions are rejected.
- Herdr `idle` and `done` map to an Orbit job waiting for attention, **not** to completed tasks.
- Transport errors map to unreachable rather than assumed terminated sessions.

## Local operator workflow

Preconditions: the job has already been reserved and prepared in the authoritative *local* SQLite database, and Herdr is installed on the machine doing execution.

```sh
orbit-agent-executor --local-authority ./orbit.sqlite launch <job-id> <revision> <claim-token> <unix-ms>
orbit-agent-executor --local-authority ./orbit.sqlite inspect <job-id> <latest-revision> <unix-ms>
orbit-agent-executor --local-authority ./orbit.sqlite attention
```

Do not use this executor with a connected client replica. Connected-mode machine authorization, a fully automated Start with Agent user action, and end-to-end Herdr smoke tests are still outstanding.

## Verification / progress convention

Source/structural inspection completed; `cargo`, `rustc` and `herdr` are not installed in the provided environment, so Rust compilation, executable integration, and Herdr contract validation have **not** passed. M23 progress measures implementation scope only, not accepted runtime readiness.
