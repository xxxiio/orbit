# Orbit Development 255 — Automated connected machine evidence handoff

## Priority and milestone decision
The active milestone is M23, last estimated at 87% implemented source scope. The most valuable outstanding user journey is connecting the machine's finished/idle agent session to durable review evidence in server-authoritative Orbit, without the user reconstructing terminal output or running a manual Git capture. This is prioritized over expanding independent hardening or introducing agent orchestration. M15 remains the distinct executable release acceptance gate. No higher-priority unfinished feature slice is displaced.

## Cohesive feature acceptance slice (provisional +5 pp to M23)
- Added a machine-authenticated `/v1/authoritative/agent-machines/handoff` POST endpoint. It verifies the assigned machine, durable launch claim token, and the server-bound Herdr session before allowing an append-only handoff. It does not grant review or task-completion permissions.
- Added machine-side deterministic Git evidence capture when a monitored session becomes idle, including HEAD, branch, worktree path, recent commits, and uncommitted file status. Tests and authorship remain explicitly unverified, never inferred from session state.
- Exact isolated-worktree lookup uses the branch derived from the persisted claim; missing or ambiguous worktrees fail closed instead of reading another checkout. Non-isolated checkouts are explicitly treated as shared.
- The complete handoff request is written durably to the existing machine journal before HTTP submission; the same immutable body and timestamp are replayed after uncertain HTTP acknowledgments. Idempotent server handoff storage handles replay safely.
- The existing `orbit agent handoffs`, `review`, `attention`, and `overview` data path can display resulting evidence without a separate public CLI.

## Verification
Run: `cd deploy/server && python -m unittest discover -p 'test_agent_machine*.py' -q`
Result: 23 Python tests passed, including five real-Git/durable-evidence tests; `python -m py_compile deploy/server/agent-machine-worker.py` passed.
The Rust authority handler and new route are wired in source but **cannot be compiler-verified here** (no Cargo/Rustc); actual authenticated HTTP and Herdr remain untested. This is not production or internal-testing acceptance.

## Progress
M22: 100% source scope unchanged. M23: 87% → **92% provisional source implementation** (+5 pp, concrete automated server-backed review evidence slice). M15: acceptance pending, unchanged. Remaining 8 pp cannot be claimed without Rust integration, live/fixture server acceptance and completion of safe review/resume flow. Percent is not a passing-test ratio.

## Internal testing readiness
Not established: no runnable verified Rust build, authenticated server smoke test or live Herdr run. The implementation is designed to fail closed on ambiguous worktree or session ownership; real deployments require testing Herdr's output contract.
