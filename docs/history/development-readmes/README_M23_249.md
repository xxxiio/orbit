# Development 249 — M23 connected-mode trusted machine worker

## Delivered

- Opt-in Python machine-side worker consumes the authenticated M23 machine inbox.
- Retrieves an owner-scoped, immutable authority brief and checks machine identity.
- Fences each job with an fsynced, atomic, local journal **before claim**.
- Sends a machine-bound launch claim and invokes allowlisted Herdr agent kinds only
  for newly accepted claims. Supports optional isolated worktrees.
- Reports confirmed session references or ambiguous execution outcomes; never
  automatically relaunches after transport loss, crash, or Herdr uncertainty.
- Handles inbox entries independently and prints JSON machine-poll outcomes.
- Adds five Python contract tests including lost-ack protection and worktree routing.

## Validation

`python -m unittest discover -s deploy/server -p test_agent_machine_worker.py -v`

5 tests passed in Development 249. Python syntax compilation passed. No Rust
build or Herdr binary is available; executable cross-service acceptance is not
verified. In particular, agent status polling/reconciliation after launch is
still missing. Internal-testing readiness is **not established**.

## Progress accounting

M23 provisional implementation-scope estimate: 60% -> 66% (+6 percentage
points). The gain covers the previously absent connected-machine polling,
claim, runtime launch and reporting vertical slice. This is NOT 66% executable
acceptance. M22 remains 100% source scope, M15 executable acceptance pending.
