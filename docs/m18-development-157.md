# Orbit Development 157 — Atomic Multi-Block Plan Acceptance

## M18 scope and highest priority

The last unaccepted local client mutations were task/habit plan applications. These apply multiple schedule blocks and activities in one operation. Leaving them outside the authority ledger risked durable writes without globally ordered receipts, defeating the milestone's unified write contract. M19 knowledge and M20 HTTP parity remain deliberately deferred; this is a higher-priority M18 correction.

## Implementation

- Added a `PlanApplication` semantic entity type to distinguish **one accepted plan operation** from **one schedule block**. The event fields `kind` and `schedule_block_ids` include all allocated schedule-block IDs; the event does not falsely claim to represent an individual schedule block.
- Added `AuthoritativeHost::apply_task_plan_accepted` and `::apply_habit_plan_accepted` using the common `execute_application_command` admission/receipt/revision/projection transaction. Plan application revalidates inside its SQLite savepoint and returns the IDs written.
- Compares the resulting schedule-block ID set and count to the planned identities *before accepting the receipt*. An error rolls back the entire plan and history.
- Embedded client task and habit plan application now calls the accepted path, looks up results from the host query service, and no longer directly invokes the plain application service.
- Reduced plain plan application service methods to `pub(crate)` to prevent adapters from bypassing acceptance.
- Removed two extra copies of `ApplicationQueries::schedule_block` and `ApplicationQueries::routine`. The duplicate definitions were an existing Rust compile-time failure in Orbit 156.
- Extended Rust client workflow test to assert exactly one additional global accepted revision on plan application, surviving a process restart. Added Python source-contract checks and updated an outdated guard that expected plan operations to *remain* unaccepted.

## Validation and remaining gaps

The available Python M18 source-level checks, offline tests, and worker tests pass. Rust and Cargo are absent, so no `cargo check`, `cargo test`, or executable CLI smoke acceptance was possible; M18 must not be declared complete. Remaining M18 work includes removing transitional `with_legacy_cli` repository access (its knowledge commands belong to M19 cutover), auditing all rich-service direct writes and finally obtaining a working Rust toolchain to diagnose potential type/borrow errors and run end-to-end smoke tests. M19–M22 scope and Flutter deferral are unchanged.

## Internal testing readiness

Not established: an executable Rust build and smoke acceptance are still missing. There is no reliable evidence that the changed Rust workspace compiles, let alone that it has no blocking data-loss errors.
