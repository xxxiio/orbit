# M20 Development 200 — authority database mode isolation

## Implemented

- The connected SQLite replica refuses a local authoritative workspace database before creating replica tables.
- The local authoritative runtime refuses a connected replica database before initializing domain schema.
- The connected replica recognizes authority identity, immutable history, authoritative receipts, domain ledger, project, and task tables as local-authority markers.
- The embedded authority recognizes replica snapshot, outbox, settings, and accepted Markdown cache markers.
- New regression cases verify rejection does not create the other mode's schema.
- Removed stale server output claiming that the active runtime is a relay and updated the legacy error wording.

Both deployment modes remain independent. This change is essential but does not by itself prove all M20 implementation requirements are complete. Full Rust compilation and release tests remain deferred to M15 by project policy; the source-level changes have not been compiled.

## Remaining closure

Complete a full source audit of client and server mutation paths and retire any incompatible legacy production entrypoints. Do not treat structural tests or the line count as acceptance evidence.
