# M15 automated runner coverage (verified 2026-10-20)

Run from the repository root: `./scripts/m15-full-automated.sh --profile full`.

The runner creates a disposable evidence ZIP with per-check status, logs, and a
machine-readable manifest; it does not install dependencies or use a personal
Orbit database. The current full profile has **37 checks**; adding opt-in
`--live-herdr` makes **38**.

## Current executable acceptance checks (full profile)

- Toolchain and metadata: `python-version`, `rust-toolchain`, `cargo-version`, `cargo-metadata`.
- Rust quality gates: `cargo-format`, `cargo-check`, `cargo-test`, `cargo-clippy`, `cargo-build`.
- Documentation and packaging: `user-documentation-contract`; installer,
  transaction, manifest-safety, release-archive, release-installation,
  worker-install and server-setup contracts; `real-release-archive-smoke` builds,
  verifies, installs, exercises and uninstalls a real CLI archive.
- Python suites: `python-worker-tests` (agent/automation workers, 46 tests),
  `python-contract-tests` (maintained Python suite, 51 tests).
- M25 blueprint source checks: `m25-blueprint-contracts`, `m25-blueprint-crate-tests`, `m25-blueprint-cli-tests`, `m25-blueprint-cli-help`, `m25-validate-{development,learning,habits,research}`.
- Local CLI persistence: `local-cli-persistence-smoke` runs 59 compiled-CLI
  workflows across separate processes, including project/task/habit/routine/
  schedule lifecycle, rejected writes, SQLite integrity and restart persistence.
- Local-authoritative workflow: `local-authority-smoke` drives the real CLI through project/task lifecycle, dependencies, planning metadata, schedule blocks, rejection of an invalid transition, SQLite integrity, and restart persistence in a disposable workspace.
- Connected canonical authority: `server-task-smoke` and `server-domain-smoke` (two devices, MCP/REST parity, stale-revision rejection, approved worker mutation).
- Offline synchronization: `offline-authority-smoke` (durable queue while disconnected, ordered replay, no local acceptance, idempotent retry, stale conflict, reviewed resolution, device ownership, restart).
- Server durability: `server-durability-smoke` (two-device convergence, offline reconnect, token rotation, revocation, SQLite backup/restore, fail-closed rewind handling, restart). This replaces the retired append-only relay smoke.
- Blueprint workflows: `blueprint-workflow-smoke` (bundled install, immutable version pinning, JSON/YAML authoring, typed properties, declarative guards, templates, capability refusal, connected offline replay, receipts, device ownership, restart).
- Agent execution contract: `agent-execution-contract` runs `orbit-runtime`'s deterministic fake-runtime test: reservation, single claimed launch, observation, handoff, review, resume, ambiguous-launch fencing, and no task completion from agent-session state.

## Optional or explicitly blocked gates

- `--require-herdr` checks only that a Herdr CLI exists.
- `--live-herdr` is the explicit opt-in for live Herdr acceptance: it runs `deploy/server/smoke-herdr-live.py --confirm-live`, which creates a disposable Git repository and Orbit workspace, launches a real `opencode` agent through `orbit agent start`, prompts it, observes the real session, captures read-only Git evidence, records a handoff and review (asserting the Orbit task is not completed), and closes the Herdr workspace it created. It never touches a user repository, workspace, or pre-existing Herdr session. Live verification on 2026-10-19 passed with Herdr 0.9.5 in this environment; it remains opt-in because it starts a real coding agent.
- Packaging is verified by the packaging contracts plus a real release-archive
  build/verify/install/uninstall smoke. The public executable is `orbit`; the
  package crate remains `orbit-cli`. Signing and authenticated publishing are not
  covered.
- Production TLS ingress, browser deployment, distribution signing, cross-platform acceptance, and destructive migration/upgrade drills remain separate release gates and are not executed by this runner.
- `deploy/server/smoke-two-clients.py` targets the retired append-only relay architecture and is **not** part of acceptance; use `server-durability-smoke` and `offline-authority-smoke` instead. `deploy/server/sync-loop.sh` is the current optional replica polling helper and is not itself an acceptance check.

If Rust is absent, compiled checks explicitly show `not_run`; a passing source inventory must not be reported as a full M25 or M15 acceptance pass.
