# M20 Development 185 — Schedule lifecycle delivery (partial acceptance)

This release adds an embedded authoritative scheduling lifecycle: collision-aware creation, collision detection with half-open interval semantics, movable-block rescheduling, fixed-block protection, persisted activity and revision receipts, and a restart/persistence regression test. Domain schedule timestamps now reject overflow on construction and rescheduling.

## Scope and limitations

The existing rich-state authoritative `create_schedule_block_accepted` and `reschedule_block_accepted` transactions are used rather than introducing independent SQL writes. The collision query is a preflight check and is **not a transaction-level cross-client uniqueness guarantee**. Legacy `create_schedule_block` still permits intentional overlaps; the new `create_schedule_block_without_conflict` explicitly opts in to no-overlap behavior. No authoritative schedule deletion or connected offline/remote schedule transport has been implemented in this iteration. Do not characterize this as the completed M20 scheduling feature.

## Acceptance status

Tests added: embedded create / conflict / move / fixed-block rejection / reopen, domain overflow rejection. **Rust was not compiled or executed** because rustc/cargo are unavailable. Acceptance is blocked pending Rust CI or a local Rust toolchain and remote transport plus deletion functionality. M20 progress remains at 77% provisional.
