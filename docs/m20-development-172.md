# Orbit 172 — M20 accepted snapshot integrity

## Priority selection

M20's immediate architecture objective remains parity through one accepted-state contract. Before expanding rich planning command surfaces, the already implemented remote accepted-state reader must avoid silently replacing an accepted projection without a revision change. This is a correctness requirement for every higher-level remote reader. It takes precedence over further release hardening or Flutter work; broad M15 acceptance remains deferred.

## Delivered

- The remote replica's accepted snapshot reader now checks that the persisted revision and fingerprint columns agree with the serialized projection, and that all six canonical collections are arrays. Inconsistent stored state fails closed rather than being surfaced as accepted state.
- Applying a snapshot at the **same** revision and fingerprint now requires an identical accepted JSON projection, irrespective of JSON object key ordering. A contradictory projection is rejected before a database transaction, preserving the previous accepted snapshot.
- Added two Rust regression tests for the immutable same-revision projection and corrupted persisted metadata, plus a dedicated source guard.

## Validation and remaining work

Executed Python source guards for M20 accepted reads, domain outbox and snapshot integrity, and the deploy/server Python tests. These **do not** validate Rust compilation or runtime behavior: Rust/Cargo are unavailable in this execution environment. The included Rust regression tests remain unexecuted.

Next M20 feature work: extend one canonical host/domain command contract to planning, routines, schedules, project strategy and milestones, and make the CLI/TUI/MCP adapters consume it in both embedded and remote modes. Resolve the remaining legacy local SQL ownership duplication, then run compiled integration and live two-client acceptance before calling Orbit internally testable. Flutter remains a separate Atomic App project. M15 and overall milestone percentages have not been changed based on source-only evidence.
