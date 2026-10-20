# Orbit Development 122 — Canonical knowledge search

## Priority rationale
M18 U2 remains the highest-priority unfinished work. Rather than introduce a new repository or client facade, this iteration adds an actual knowledge retrieval capability to the existing canonical SQLite read model, shared by embedded and remote clients. Full repository consolidation remains incomplete.

## Implementation
- `RelayStore::authority_knowledge_search` searches canonical knowledge titles and bodies with SQL wildcard escaping, stable ordering, bounded result counts and accepted revision.
- `AuthoritativeHost::knowledge_search` exposes the host-owned query.
- Embedded and unified clients call the same host capability; remote clients use an authenticated HTTP POST endpoint, without maintaining another SQLite knowledge index.
- Offline remote knowledge search explicitly fails rather than returning misleading results.
- Regression test checks wildcard literals, bounds, latest revision, and read-after-update behavior.

## Limitations
No semantic ranking, FTS, relations, metadata, resources or offline cached search. Legacy local `SqliteStore` remains separate. Rust tests must run in a Rust-enabled environment; source checks alone do not validate compilation. Flutter remains out of scope.

## Next priority
Consolidate existing knowledge metadata/relations/resources and rich application services into the canonical repository. No higher-priority unfinished category displaces M18 U2 in this iteration. Internal testing is not ready without compiled runtime smoke tests and a data-loss blocker review.
