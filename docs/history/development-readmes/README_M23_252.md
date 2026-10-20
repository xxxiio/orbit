# Development 252 — Authenticated connected agent commands and resume

## Implementation

- `orbit agent status`, `handoffs`, `handoff`, `review-status`, `review`, `attention`, `overview`, and `resume-context` now use device-authenticated server APIs in connected mode, instead of opening a standalone SQLite authority.
- `orbit agent resume JOB_ID` in connected mode creates a queued successor via an authenticated authority command. The server does not invoke Herdr; the device-bound machine worker handles the launch.
- The successor copies the former agent's task, project, machine, repository and runtime; an immutable previous handoff is incorporated before preparation, and worktree policy is copied. Terminal predecessor and durable handoff are mandatory.
- Remote CLI calls require `ORBIT_AUTHORITY_DEVICE` and `ORBIT_SYNC_HTTP_TOKEN`, reject insecure non-loopback HTTP, reject redirects, and never fall back to local storage.
- Unsupported connected actions fail explicitly. In particular `agent start` is not yet connected end-to-end, and `agent resume` queues a new session rather than attaching to the old session.

## Checks and known gaps

- Python machine-worker suite: 11 tests pass.
- Rust compilation and native integration tests not executed: neither cargo nor rustc is available in this environment.
- No live Herdr integration or connected authenticated HTTP smoke test executed.
- Server command currently composes several SQLite operations under the server's exclusive host mutex rather than a single storage transaction. A mid-command failure can leave a reserved successor; fix transactional composition and deterministic retry semantics before production use.
- The original API's device-scoped ownership remains in place; richer cross-device role delegation and disconnected/offline command queueing remain outstanding.

## Milestone progress

- M23 previously reported: 77% provisional source scope.
- Connected resume, authenticated CLI operations and server queueing constitute a candidate +5-7 percentage-point scope increment. Executable acceptance is outstanding, so do not treat this as a proven release acceptance increment.
- M22 planning: 100% reported source scope. M15 executable acceptance: pending.

## Internal testing

Not ready: needs a successful Rust build, core smoke tests and no blocking data-loss issues.
