# Orbit Development 241 — host-owned one-command agent start

## Scope

`orbit agent start TASK_ID --agent codex --machine local --repository /absolute/path` is now parsed by the public `orbit` CLI. The invocation uses `AuthoritativeHost` to load the task and its project, reserves a job, records its immutable brief, prepares it, obtains a durable launch claim and invokes the injected `AgentRuntime` (Herdr by default). Its response reports the job ID, confirmation or reconciliation status. Task dependency and status checks run in the store's existing authority transactions.

All preexisting local CLI agent operations (launch, inspect, status, attention) now use host-owned methods rather than creating a parallel `SqliteStore` connection. Connected mode still explicitly refuses local commands; connected execution and worktree selection are NOT implemented.

## Safety

This is a single local-authority workflow, not a universal executor permission. The CLI accepts only codex/opencode and absolute repositories; the Herdr runtime is optional and failures after invocation are treated as ambiguous, never retried blindly. Tasks are never automatically closed. Job identities are generated for every CLI start; multiple invocations against an active task are rejected at reservation, not silently attached to an existing job. Failed pre-dispatch reservations can require manual inspection.

## Validation / progress

Source command-registration, boundary, and archive inspections performed. Rust compilation and live Herdr integration could not be executed (cargo, rustc, Herdr not installed). No executable acceptance is claimed. M23 remains at the last provisional 26% pending verification rather than assigning artificial completion percentage. M15 remains pending. Internal testing is not ready.
