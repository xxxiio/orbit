# Development 288 — M25 capability executor composition

Baseline: 287. Added application-runtime `BlueprintCapabilityExecutor` and
`BlueprintExecutorRegistry` so the already persisted, explicitly authorized
workflow-action requests can be dispatched to trusted injected providers.
Durable claim occurs before any external effects; unknown providers leave the
request pending, and uncertain results are never automatically retried.

Implemented scope: provider registration, dispatch fencing, typed outcomes,
receipt persistence, command-envelope checks, focused regression tests and
capability-execution architecture documentation. This does not yet provide
public CLI/MCP routes or concrete plugin runtimes.

M25 provisional source progress: 58% → 64% (+6 percentage points).
M15 remains 5%; broad executable acceptance deferred to M15.
