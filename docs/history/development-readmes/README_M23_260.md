# Development 260 — M23 offline agent intentions

## Source implementation

The connected `orbit agent` CLI now exposes explicit durable pending commands:

```sh
orbit agent queue-start TASK_ID --agent codex --machine devbox --repository /repo --worktree
orbit agent queue-continue JOB_ID --message 'Continue with review comments'
orbit agent queue-resume JOB_ID
orbit agent queue-handoff JOB_ID --file handoff.json
orbit agent queue-review JOB_ID --handoff-id HANDOFF_ID --decision approved
orbit agent pending
orbit agent flush
```

Commands are recorded before any network action; `flush` sends them FIFO to the
existing authenticated server API. Original job, prompt, handoff, and review
parameters, including their IDs and timestamps, are replayed unchanged.
A timeout, HTTP error, or lost acknowledgment keeps the original command
pending rather than creating a replacement job or prompt. A blocked command
stops the ordered flush until an operator resolves the underlying problem.
An intentional queue operation does not contact Herdr or the Orbit server.

The outbox is a separate local command-intention file, not an alternate
canonical Orbit database. It is bound to an exact authority URL and enrolled
device; normal connected agent commands still require an online authority.
`ORBIT_AGENT_OUTBOX_PATH` optionally overrides its location; otherwise it
resides beside the configured `--db` path. Credentials are never persisted.
A separate advisory lock and atomic rename protect concurrent CLI writers.

## Source-completion boundary

M23's deferred source feature (offline agent commands) is now represented by
an explicit connected-mode queue. Live connection verification, Rust builds,
server integration acceptance, Herdr smoke tests, and fixes found in those
checks belong to M15. Essential code-level checks are not release acceptance.

## Progress

M22 implementation 100%; M23 feature source 100% (previously reported 97%);
M15 release acceptance remains pending. The closing 3 points are the
remaining feature work and are **not** a new five-point iteration.
