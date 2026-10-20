# Orbit 171 — M20 unified accepted-state reads

## Scope

This is the first M20 source implementation pass following the M19 source handoff. It fixes the embedded transport's import of the runtime's private store module and exposes a coherent accepted-state read surface through `AuthorityConnection` in both authoritative embedded and remote replica modes.

The shared read surface includes project/task/habit/metric collections, point lookups, habit occurrences and metric observations, accepted snapshot metadata, and authority-history prefix checks. Remote reads use the last accepted (validated) replica snapshot: pending commands remain out of the projection. Embedded reads come from the authoritative host. Disconnected remote clients without a previously accepted snapshot return `None` for snapshot metadata and empty collections. Cached tip history fingerprints are available offline; other historical-prefix proofs require credentials and a live server.

The connected workspace now includes `history_fingerprint` alongside revision, so downstream clients can track which accepted history they are displaying. Added Rust behavioral tests for local/remote accepted-read parity and for the isolation of pending writes.

## Implementation limitations and next M20 priority

This is **not** remote rich-workflow parity. The remaining M20 implementation is remote command contracts for planning, routines, schedules, project strategy, milestones and other rich operations, plus CLI/TUI/MCP convergence. Their source implementation is the next priority; do not interpret this change as acceptance completion.

Full Rust compilation/integration and release validation remain deferred to M15 as requested. Source guards and existing Python tests are not substitutes for Rust acceptance.
