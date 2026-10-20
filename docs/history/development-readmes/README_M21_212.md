# Development 212 — M21 source-implementation closure

## Contract and boundaries

Connected clients keep two independent state families: accepted domain state and a
Markdown mirror (both read-only locally), and a durable pending command outbox.
No connected client can transform a queued intention into accepted state. The
self-hosted authoritative Rust runtime alone accepts commands and issues receipts.

The completed M21 source scope is:

1. Durable, globally ordered offline queuing across all ten supported command
   families, including rejection/receipt retention.
2. Reconnect, replay, accepted-history prefix verification, and point-in-time
   server fingerprint comparison; explicitly distinguish partial outcomes.
3. Snapshot-bound reviewed conflict resolution with stale-review detection,
   transactional local staging, and globally ordered remote application.
4. Offline Markdown search and incremental full-mirror transfer: hash-verified
   reuse, changed/deleted note reconciliation, and transactional publication.
5. CLI, desktop MCP, and connected TUI interfaces for reconciliation, mirror
   synchronization, and cross-domain health/queue reports.
6. Source-revision-bound mirror provenance and on-read persisted-content
   verification added in this development. The mirror is rejected if a concurrent
   authority revision advanced during transfer or if cached bytes changed
   after publication without updating the manifest.

## Development 212 correction

Previously, a verified Markdown manifest was linked to the replica's *local*
revision even when the transfer never compared that revision with the current
server. In addition, the persisted completeness label could remain true after
SQLite cache corruption if the row count was unchanged.

When an accepted domain snapshot exists, synchronization now compares the
server's revision and history fingerprint before and after inventory/body
transfer; mismatch fails closed without publishing new cache data. Persisted
verified mirrors now recalculate their manifest from cached bodies and metadata
when their status is read. The common convergence/health paths also require
mirror source revision to agree with accepted/live revision.

An uninitialized note-only cache remains supported as a limited offline read
cache, but it cannot establish a joined domain/Markdown convergence checkpoint.

## Meaning of source completion

M21 implementation contracts are represented in the source, but broad Rust
compilation, end-to-end multi-client testing, build packaging and release
acceptance remain deferred to M15. This is **not** executable acceptance or
verified readiness for internal testing. Essential Python source checks alone
do not establish compile-time correctness.

The server can advance immediately after the final comparison. As designed,
all convergence statements remain point-in-time observations and do not imply
atomic distributed transactions or a live subscription.
