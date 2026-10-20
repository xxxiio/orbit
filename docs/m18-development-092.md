# M18 development 092 — Accepted offline view for authoritative projects/tasks

The opt-in Python authority client now maintains a local SQLite cache of **server-accepted** project and task snapshots, separate from its pending command outbox. `refresh --server` fetches both authoritative collections and atomically replaces the cache only when both report the same server revision. `sync --server` processes durable pending commands before attempting the refresh; a failed refresh never loses an accepted receipt or overwrites the prior cache.

Commands:

```sh
python3 scripts/orbit-authority-offline.py refresh --server https://orbit.example.com
python3 scripts/orbit-authority-offline.py cached-projects
python3 scripts/orbit-authority-offline.py cached-tasks
python3 scripts/orbit-authority-offline.py status
```

`ORBIT_SYNC_HTTP_TOKEN` is required for online commands. Cached views are readable offline and include revision and fetch timestamp. Pending changes are **not** incorporated into the server-accepted cache. The queue database is pinned to the first configured server URL; use a separate `--db` for another authority. This limits accidental submission of durable pending commands to the wrong server.

**Remaining boundaries:** no habit/metric authority, no integrated Rust client replica, no incremental authoritative state stream, no automatic reconciliation of conflicts or offline edits, no multi-device cross-server migration, no browser/agent cutover, no Git retirement. URL scoping is a provisional safeguard, not cryptographic server identity. The two collection GETs are only revision-consistent when equal; the server still needs a single snapshot endpoint for a more efficient, strongly consistent fetch.

Checks: 9 focused Python unit tests passed. Rust/Cargo acceptance and live server integration not executed here. M18 estimated source progress: 65%; other milestones unchanged.
