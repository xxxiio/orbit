# M21 — Development 206: Accepted Markdown mirror synchronization

## Why this work is next

The connected replica can cache individual read-by-ID notes, but this does not
provide dependable offline discovery or deletion reconciliation. M21 requires
an explicit full inventory → validated download → atomic publication path. This
is a higher priority than standalone durability or release-acceptance work.

## Implementation

- The host exposes an authenticated `/v1/authoritative/markdown-inventory`
  endpoint based on the physical knowledge vault's stable-ID discovery.
- Connected clients download the authoritative identity set and then resolve
  every body by stable ID; changes in identity/path/title between requests
  reject the refresh instead of partially publishing it.
- After every download succeeds, the client checks document frontmatter,
  ID uniqueness, path uniqueness, and Markdown paths, then replaces the entire
  accepted Markdown cache in one SQLite transaction.
- Moves and deletions are reflected by whole-set replacement. Any download or
  validation failure leaves the previous accepted mirror unchanged.
- The pending outbox is never modified by mirror publication. Offline search
  and inventory continue to use only previously accepted Markdown bodies.
- `AuthorityConnection::sync_markdown_mirror()` exposes the operation to
  connected clients. Local-only clients do not need a replica mirror.
- `orbit sync replica ... sync-markdown` performs online synchronization.
  `search-markdown` and `cached-markdown` operate entirely offline.
- The connected TUI exposes `note.sync`, `note.cached`, and offline fallback
  for `note.search` when the HTTP transport is absent.

## Boundaries

This is a *full-refresh* protocol, not a delta subscription. It does not claim
an atomic server filesystem snapshot across concurrent remote writes. If
writers change the vault during refresh, identity drift is rejected where
observed; a subsequent refresh converges. Live server execution remains the
only authority for pending edits.

## Essential checks and release acceptance

Five focused tests cover replacement/removals, invalid-document rollback,
identity/path collisions, pending-write isolation, and reopening SQLite.
These Rust tests are included in source but not executed in this environment.
Broad compilation and release acceptance remain assigned to M15.

## Internal-testing readiness

Not established: executable Rust workflows and absence of known blocking data
loss are required before declaring readiness. This is a source implementation
increment, not verified release acceptance.
