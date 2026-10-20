# M21 / Development 205 — Offline accepted Markdown discovery

This iteration extends the connected SQLite replica's *accepted-document cache*
with offline text search and inventory operations. Both operations are available
through `ConnectedOrbitClient` and never perform a network request.

- Only notes previously fetched from the server with `refresh_markdown` can
  appear in cache search results. The result set is not a complete vault index.
- Unacknowledged note edits, folder commands, and other outbox intentions are
  never materialized into this cache.
- The cached documents may be stale. No background synchronization or
  automatic freshness assertion is implied.
- Search matches literal user terms in cached bodies or metadata (including
  title and path); `%`, `_`, and `!` are escaped for SQLite LIKE.
- Results are limited, deterministic by note ID, and fail closed on mismatched
  persisted note metadata.
- Tests cover offline-only reads, literal matching, restart durability,
  pending isolation, invalid requests, and metadata corruption.

M21 remains in progress. Full connected-client offline convergence, reconnect,
conflict reconciliation, and two-client acceptance are not claimed by this
source implementation. Broad executable release acceptance remains assigned
to M15. Flutter stays in the separate Atomic App project.
