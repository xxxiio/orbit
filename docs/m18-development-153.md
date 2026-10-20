# Orbit 153 — shared canonical command acceptance

## Architectural objective

All three production authoritative project, task and evidence command families now execute the same transaction-level admission and publication flow. This is a substantive component of M18's *unified command handling, revisions and receipts* criterion, not another CLI adapter migration.

## Changes

- `crates/orbit-runtime/src/command_engine.rs`: shared private `PendingCommand` admission and commit boundary. One global command-ID receipt lookup; immutable actor and payload conflict semantics; revision fencing; fail-closed validation against legacy histories; server semantic event generation; immutable result receipt; canonical replay; complete/unconflicted projection publication. Domain rows and these records share one SQLite immediate transaction.
- Project, task and evidence authoritative commands now delegate common admission and publication to the shared engine; domain-specific validation/mutations remain with the appropriate domain commands.
- Five Rust behavioral regressions exercise mixed-domain sequence ordering, cross-type receipt collisions, retries after unrelated writes, rollback after invalid/stale commands, and actor/payload immutability.
- Six source-contract regression checks prevent the three domain handlers from silently forking command semantics again.

## Validation

- Source contract: 6 passed.
- Existing runtime-boundary contracts: 22 passed.
- Python offline authority tests: 17 passed.
- Python worker tests: 12 passed; 10 additional subtests passed.
- Existing M18 application-services, host, single-store, unified-entity and retired-relay source checks passed.
- **Rust compilation and the new Rust behavioral tests NOT RUN** (no cargo/rustc installed). These cannot be reported as accepted.

## Remaining M18 objectives

1. Retire legacy relay/semantic append/projection paths from production runtime and remove or replace relay-era synchronization coupling without breaking canonical history needed by connected clients.
2. Audit and close remaining mutable repository entrypoints and command bypasses; ensure rich workflows have the intended canonical semantics rather than sharing only a SQLite handle.
3. Compile entire workspace; execute Rust tests and 59 CLI acceptance scenarios; fix the defects they reveal. Do not call M18 complete without this evidence.

## Scope boundaries

Markdown/folder recovery belongs to M19, full transport parity M20, client replication M21 and full release acceptance M22. The current milestone still includes core executable smoke workflows.
