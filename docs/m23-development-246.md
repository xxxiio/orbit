# Development 246 — worktree-specific evidence integrity

Baseline: Development 245, M23 source-scope estimate 43%.

## Implemented

- AuthoritativeHost exposes `local_agent_evidence_location(job_id)`, returning the canonical repository selector, optional launch-derived worktree branch, and rejecting remote-machine captures locally.
- Manual `orbit agent capture` and automatic `inspect`/`sweep` evidence now share a branch-specific Git resolver.
- Worktree discovery uses `git worktree list --porcelain`, requires exactly one branch match, and rechecks the actual branch after capture. No silent fallback to the main checkout if the worktree cannot be resolved.
- Evidence records include the actual resolved worktree path and explicitly disclaim unproven agent attribution.
- Added Rust regression tests for absent branch and missing repository.

## Independent checks

- Git executable 2.47.3 available; a real Git fixture demonstrated isolated worktree changes are invisible to the main checkout.
- Rust cargo/rustc absent: source compilation and Rust test execution NOT verified.
- Herdr binary absent: real session execution NOT verified.
- No full start-to-review acceptance or connected remote evidence implementation.

## Progress

M23 provisional implementation scope: 43% -> 48% (+5 points), attributed to closing the documented incorrect-worktree-evidence acceptance gap across both capture pathways, including fail-closed remote behavior. This is NOT compiled acceptance. M22 remains 100% source scope; M15 executable acceptance remains pending.

## Outstanding

Connected-mode executor authorization and evidence collection, actual Rust build, Herdr smoke, reliable agent-only change attribution, review validation outcomes, task resumption and M23 integration acceptance.

Internal-testing readiness: not established.
