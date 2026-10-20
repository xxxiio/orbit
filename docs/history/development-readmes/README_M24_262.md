# Orbit Development 262 — M24 application authority boundary

Base: Development 261. M24 I1 implementation continues.

- Added `AgentObservationAuthority` port and normalized monitoring observations
  to `orbit-application/src/agent_runtime.rs`.
- Implemented the port in `orbit-storage/src/agent_observation_port.rs`.
- Refactored the runtime's monitor to avoid direct `SqliteStore` and
  persistence-observation dependencies; the host still composes the concrete adapter.
- Added in-memory monitoring contract tests for stale revisions, missing claim,
  reviewable idle observation, and unreachable sessions.
- Updated the M24 architecture roadmap and its dependency-direction notes.

M24 progress: 10% -> 16% (+6 percentage points, implementation scope).
M23: 100% source scope (unchanged). M15: acceptance pending (unchanged).
Compilation, integration validation and fixes found during validation stay in M15.
