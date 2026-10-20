# Orbit 147 — Host-owned CLI task/project workflows

## Integrated change
- CLI task commands now use `AuthoritativeHost` application services and queries: create, list, start, complete, set status, and update planning. Activity-history behavior remains in the original application use cases.
- CLI project create/list/status also use the same host-owned services and queries.
- Advanced project modes, evidence operations, and other CLI command families retain explicitly transitional repository access pending typed host operations. This is not full M18 completion.
- Added `scripts/legacy-contracts/test-m18-cli-task-project-boundary.py` to prevent basic task/project command regressions, and included it in the acceptance script.

## Validation
- 17 offline synchronization Python tests passed.
- 12 worker tests and 10 subtests passed.
- M18 Python source-contract checks passed.
- Rust toolchain absent: Rust compilation and executable parity tests not performed.

## Completion criteria remaining
- Remove remaining CLI transitional storage access; unify application-command revision/receipt semantics; preserve rich project/planning/habit/metric operations.
- Rust build and executed smoke workflows required before M18/internal testing readiness.
