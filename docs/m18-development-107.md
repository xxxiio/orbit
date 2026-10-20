# Development 107: greenfield connected-mode cleanup

The authoritative server is the sole supported connected-mode write implementation. There are **no existing installations** requiring import/migration. This development snapshot intentionally drops backward compatibility with Orbit 106 and earlier. Start the server with a fresh SQLite database.

### Deleted

- Server's domain-preview SQLite `DomainCommandStore`, its routes, and migration-status endpoint. Shared task/project command DTOs and status validator remain for canonical domain operations.
- Dead legacy append/strict-commit/event-pull and semantic-view HTTP handlers and their route registrations. Internal semantic events remain necessary for canonical revisions, accepted projections and history-prefix verification.
- Domain-preview import/preflight utility and its tests.
- CLI `sync init`, `sync status`, `sync pull`, `sync push`, `sync http`, and the old HTTP/Git transport adapter. CLI exposes only native `sync authority` and `sync replica` connected-mode commands.
- Prior obsolete migration docs.

### Not yet deleted

- `orbit-git`/`orbit-sync` legacy infrastructure is still referenced by the **local-only TUI** and older tests. `orbit-sync` also supplies event types used by canonical server history, so its removal requires splitting semantic types from the Git transport.
- Server `relay_events` internal table and selected helper methods still need to be renamed/pruned in a later cleanup. Do not remove the semantic append-only history itself.
- Existing worker and proposal endpoints still need a full canonical-domain operational cutover.

### Verification

Python syntax checks and archive integrity were run. Rust compile/test and end-to-end server smoke are **not verified** due to unavailable Cargo. No claim of fully completed M18 or internal-testing readiness.
