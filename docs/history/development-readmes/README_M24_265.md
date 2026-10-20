# Orbit Development 265 — M24 client boundaries

Baseline: Development 264. M24 source-implementation progress: **38%** (previously 31%, +7 percentage points). M23 remains 100% source scope; M15 release acceptance is pending.

## Code changes

- Split multi-step workflow manifest types into `crates/orbit-client/src/unified/workflow.rs`.
- Split local/connected planning projections into `crates/orbit-client/src/unified/planning_queries.rs`.
- Kept existing public type paths, `AuthorityConnection` method names, command IDs, explicit revisions and local/connected semantics.
- No new binaries, database owners, migrations, HTTP endpoints, or business rules.

Scope accounting: 239 lines of workflow construction and 418 lines of planning projection logic extracted from the 2,691-line unified module. The parent facade now has about 2,038 lines, making the two responsibilities independently navigable. Files were moved as Rust items without rewriting their bodies.

## Handoff

Next: decompose `orbit-client/src/authority.rs` by coherent replica outbox/mirror/synchronization responsibilities, then examine the storage facade. Preserve internal client state invariants; do not create a second accepted-state implementation.

Broad Rust compilation, end-to-end integration validation, and fixes discovered during release acceptance remain in M15. No internal-testing-ready claim is made.
