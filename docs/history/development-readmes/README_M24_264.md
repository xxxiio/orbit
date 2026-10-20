# Orbit Development 264 — M24 command adapter modularization

## Priority and rationale
M24-I1 application authority ports are established through Development 263. Remaining I1 work is interface-specific operation normalization; the highest-leverage next cohesive source change is to decouple the oversized CLI transport entrypoint from its capability handlers while preserving public `orbit` commands and host-owned authority mutations. This completes I1's source boundary cleanup and begins M24-I2 module decomposition. M15 continues to own broad build and runtime acceptance.

## Changes
- Moved local agent start/inspect/sweep/handoff/review/resume/continue routing to `orbit-cli/src/agent_commands.rs` without introducing a new binary or opening a parallel SQLite connection.
- Moved task dependency, task lifecycle and task planning handlers into `task_commands.rs`, preserving `AuthoritativeHost` accepted-command calls.
- Extracted project, habit/routine and schedule capability handlers into distinct modules; retained their existing command arguments and response formatting.
- Kept `lib.rs` as shared parser/deployment routing/response representation and CLI entrypoint. Connected mode still routes agent calls to the authenticated connected adapter; the local path still owns one `AuthoritativeHost`.
- Preserved existing in-crate tests' task helper lookup with a test-only module import.
- No database schema, domain command, endpoint, command-line surface, or user-visible behavior changes intended.

## Implementation scope progress
M24: 23% -> 31% (+8 percentage points provisional source implementation). I1 boundary cleanup is closed for the scoped agent/command dispatch adapter and I2 begins; further modularization of client and persistence remains. M22/M23: 100% reported source implementation. M15: pending acceptance.

## Internal testing
Not established. Broad compile/integration and resulting defect correction remain M15. Essential source-level registration and boundary checks were performed for this refactor, not runtime acceptance.

## Next priority
Decompose the oversized `orbit-client` connected authority and `orbit-storage` capability-specific files by stable responsibilities; then compare the existing general offline command queue against M23's file-backed agent outbox before pursuing I3.
