# Development 253 — connected agent start and atomic resume

- `orbit agent start TASK --agent codex --machine DEVICE --repository PATH --worktree` now uses the authenticated connected authority when Orbit is configured in connected mode. The project is resolved from the task by the authority: no extra project argument or environment variable is needed.
- The server start endpoint `/v1/authoritative/agent-jobs/start` queues a prepared job *atomically* with immutable brief, ownership, worktree policy, command receipt and job events. Herdr is invoked only by the trusted machine worker.
- Connected resume uses one storage transaction for the successor, frozen latest handoff, worktree option, accepted prepare command and inbox visibility. If the task has changed, or a duplicate active job exists, no partial successor is committed.
- Repeated start/resume IDs return the same committed job only when owner and payload match; changed payloads are rejected.
- Limitations: Rust compilation, HTTP+Herdr integration and cross-machine smoke testing remain unverified. Do not claim internal-testing readiness.
