# Development 248 — authenticated machine execution protocol

## Feature scope

This increment adds an authority-owned API contract that lets an authenticated machine device pull assigned agent work and report execution without granting the Orbit server any shell-execution capability.

- `GET /v1/authoritative/agent-machines/inbox`: list prepared, unclaimed jobs targeted at the authenticated device ID; no jobs for other devices are returned.
- `GET /v1/authoritative/agent-machines/brief?id=...`: retrieve the immutable task brief and worktree option only for jobs targeted at that machine.
- `POST /v1/authoritative/agent-machines/claim`: compare-and-swap from prepared to launching and durably claim a single launch token prior to all Herdr side effects.
- `POST /v1/authoritative/agent-machines/launch-result`: record confirmed runtime session identity or an ambiguous launch result using the saved claim token.
- `POST /v1/authoritative/agent-machines/observe`: reconcile the runtime state by saved token and expected revision. `idle` is explicitly not task acceptance.

Each request is authenticated with the existing Orbit device credentials, and machine selection uses the authenticated device ID, **not** a `machine` body parameter. Set job `machine_ref` to the executor's registered device ID when reserving a connected job. This is a deliberate departure from local CLI's `machine_ref=local` convention. No executor binary or public CLI distinct from `orbit` has been introduced.

## Safe machine dispatcher contract

The trusted machine component must first receive a configured device credential and register the same ID as `machine_ref`. It polls the inbox, downloads the brief, generates a unique claim token, claims the job, and only if `claimed=true` invokes `HerdrRuntime` on that machine. On an ambiguous launch result, it reports an ambiguous result; it must NOT re-launch. Monitoring uses `observe`, never a second claim. For an existing claim, `claimed=false` means there is no authorization to launch again. No server endpoint runs a shell command.

A lost reply after a successful claim means the executor must reconcile rather than retry the side effect. A device credential is trusted machine authority for jobs assigned to its ID; machine registration/allowlisting and credential lifecycle remain explicit deployment concerns. Owner-device job commands retain their separate API and task-authority rules. Neither runtime status nor report approval updates task completion.

## Current limitations / acceptance evidence

This is a server API protocol implementation, not a deployed machine poller. The trusted machine polling client, offline outbox integration, deployment provisioning, and a compiled client-server smoke test remain incomplete. Rust `cargo` and live Herdr are unavailable in this environment. Independent SQLite tests verified that only targeted machines see their unclaimed jobs and that a stored claim removes them from the inbox. The API compilation and runtime authentication tests are not yet executed.

## Milestone accounting

Previously reported M23 54%. Source implementation estimate after this increment: **60% (+6 percentage points)**, credited for the machine-bound inbox, immutable brief delivery, atomic claim, launch result, and observation endpoints as a single coherent protocol. This is **not** executable acceptance. M22 stays at 100% source scope; M15 release acceptance remains pending. Internal-testing readiness: not established.
