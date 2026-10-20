# M21 — Development 210: Byte-verified incremental Markdown mirroring

## Why this was the highest-priority unfinished M21 capability

M21 had explicit reconnect and conflict-handling code, but its Markdown mirror
could publish a non-coherent collection if files changed during download. An
inventory based only on note identity/path/title could miss same-path body
updates. Every reconnect also downloaded every document, even if the replica
already held verified identical bytes. These behaviors directly affect the
core connected-client offline workflow; they outrank standalone hardening or
new wrappers. No higher-priority M21 feature precedes synchronizing accepted
Markdown bytes safely and making cache completeness observable.

## Implementation

- New server `/v1/authoritative/markdown-manifest` endpoint contains each note's
  stable metadata and SHA-256 of its current body. The old inventory endpoint is
  preserved for lightweight listing.
- Client fetches a manifest, reuses locally stored accepted bodies only when
  metadata and actual SHA-256 match, downloads changed files, verifies downloaded
  SHA-256, then fetches a second manifest. Only equal manifests can publish.
- A single SQLite transaction replaces the accepted mirror and its provenance.
  Failure leaves the previous accepted mirror intact. Deleted notes are removed
  only after the final successful manifest comparison.
- Individually cached server reads clear the full-mirror provenance in the
  same transaction. Offline cached search remains available without asserting
  that the cache contains the whole vault.
- `MarkdownMirrorSyncReport` exposes transferred/reused/total counts and the
  digest. `MarkdownMirrorStatus` distinguishes partial cache, direct manual
  replacement, and manifest-verified full cache; surfaced through the shared
  client, connected client, CLI and convergence status.
- Extensive focused Rust tests cover changed bodies, renamed/moved/deleted
  files, invalid digests, duplicate paths, interrupted publication, cache reuse,
  SQLite reopen/corruption, divergent independent devices, and pending-state
  isolation. The existing network convergence simulations were updated to the
  two-manifest contract.

## Limits and boundaries

Two matching manifest reads are not a distributed filesystem lock; an A→B→A
mutation between reads can escape detection. Convergence remains point-in-time.
The source revision in mirror metadata refers to the accepted domain snapshot
available to the client, not a new global Markdown filesystem transaction ID.
No pending command is ever projected as accepted Markdown.

## M21 progress

Previous source-implementation estimate: 85%. Current: 90%, reflecting a
complete manifest-verified, incremental offline Markdown mirror that integrates
with reconnect and offline cache status. This is not compiled/tested acceptance.
Broad Rust build and multi-client release acceptance remain assigned to M15.

## Essential source check

The existing M21 reviewed-batch source guard and focused manifest integration
assertions were run successfully. Added Rust regression tests have not been
executed because Cargo is unavailable in the environment.

## Remaining M21 implementation

Complete consistent cross-domain synchronization status and final connected
client workflows, while avoiding further standalone validation work. Flutter
remains deferred to Atomic App.

## Priority check at finish

The selected work closes an actual missing accepted-state synchronization
workflow, rather than a convenience wrapper. Core reconnect/rollback is already
present; the next unfinished M21 priority is complete connected convergence
reporting and residual client integration.
