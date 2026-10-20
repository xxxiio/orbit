# Orbit Development 263 — M24 application authority context consolidation

## Why this work now

M24-I1 remains the highest-priority incomplete M24 category. Agent launch and observation coordinators were decoupled from `SqliteStore` in Developments 261–262, but each port still exposed multiple independent reads. That meant application orchestration could combine different snapshots of the same authoritative job. This iteration completes a cohesive contract-level refactor before proceeding to CLI/client decomposition (M24-I2), generic offline command consolidation (M24-I3), or worker cleanup (M24-I4).

## Changes

- `orbit-application::agent_runtime`: added `AgentLaunchContext` and `AgentMonitoringContext` application-level snapshot types; replaced independent authority getters with one context read per operation.
- `orbit-storage`: added SQLite read-transaction-backed context adapters that retrieve the job and its persisted launch brief/worktree policy or monitoring claim as one consistent snapshot. No database schema or public CLI changes.
- `orbit-runtime`: dispatch and monitoring now consume the immutable context obtained from those ports, not multiple independently fetched pieces.
- In-memory port implementations in dispatch and monitoring unit tests updated to the new contracts; launch fencing, stale-revision checking and task/job separation remain unchanged.

## Architectural invariants

- External side effects are still fenced by a durable claim written before the Herdr call.
- A snapshot is *not* an executor authorization or a substitute for the revision and claim-token CAS on writes.
- Local SQLite transactions ensure internally consistent context at read time; remote machine execution continues using the authenticated server machine protocol.
- A monitoring observation never creates a new job or completes an Orbit task.
- Both local-only authoritative and self-hosted server-authoritative modes remain supported, with no mandatory Herdr dependency for ordinary tasks.

## Scope and accounting

M24 provisional implementation progress: 16% -> 23% (+7 percentage points). This covers one cohesive application/adapter/runtime contract change across three crates. M24-I1 remains open; next is to consolidate command handling through application services across CLI/MCP/HTTP before starting M24-I2.

M22: 100% source scope; M23: 100% source scope; M15: pending release acceptance. Broad Rust compilation, integration tests, live Herdr smoke tests and fixes resulting from acceptance are allocated to M15, not M24.

## Internal-testing readiness

Not established: runnable build and end-to-end core smoke workflows remain unverified under M15. This iteration makes no claim of runtime acceptance.
