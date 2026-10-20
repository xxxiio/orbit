# Orbit development 154 — authority-ledger cutover and fail-closed consistency

Baseline: Orbit 153. Milestone: **M18 — Unified Application Runtime** (not complete).

## Implemented

- Renamed the runtime's durable command-history tables to `authority_history`, `authority_identity`, and `authority_projection`, and the projection revision column to `authority_revision`. Renamed the Rust backing types `AuthorityStore`, `AuthorityError`, and `AuthorityEvent` across the server, client, command engine, and runtime. This is a greenfield breaking schema change, not a migration.
- New `AuthorityStore::open()` explicitly rejects a database containing old `relay_events`, `relay_identity`, or `accepted_projection` tables rather than silently initializing parallel authority history. A fresh SQLite initialization is required.
- Hardened transactional command admission: accepted command receipt count **must exactly equal** history tip; history row count must equal tip; and for nonempty history, the durable accepted projection must exist at the same revision. Refuse subsequent writes if any invariant is violated. No automatic repair occurs.
- Added executable Rust regression cases for missing receipts, mismatched history, missing/stale projection, new schema presence, and fail-closed rejection of a legacy database.
- Updated M18 source guards to use authority-owned nomenclature and preserve checks for the one-owner transactional boundary.

## Validation performed

- All `scripts/test-m18-*.py` source guards except the executable CLI scenario runner succeeded.
- `scripts/test-authority-offline.py`: 17 Python tests passed.
- `python3 -m pytest -q deploy/server/test_agent_worker.py deploy/server/test_automation_worker.py`: 12 tests plus 10 subtests passed.
- `scripts/legacy-contracts/m18-acceptance.sh` correctly reported **BLOCKED** due to unavailable cargo/rustc. Rust compilation, Rust unit tests, and CLI binary scenarios were **not executed**. No runtime acceptance is claimed.

## Important limitations / remaining M18 requirements

1. Rich `ApplicationServices` operations mutate the shared application tables but do not all participate in `PendingCommand`'s canonical receipt/history path. This means there are still distinct command semantics for canonical remote operations versus richer embedded operations. Finishing these semantics is the **highest-priority next change**.
2. The retained semantic replay uses `orbit_sync` wire envelopes internally and contains test-only old peer relay cases. Renaming the tables is not the same as removing all legacy semantics or code. Eliminate remaining obsolete logic without changing the accepted history contract accidentally.
3. Verify Rust build, unit tests, CLI smoke tests, and persistence/restart behavior with cargo available before declaring M18 accepted or suitable for internal testing.
4. M19 Markdown vault, M20 transport parity, M21 remote cache/outbox, and M22 integrated acceptance are separate roadmap milestones. Flutter remains deferred to Atomic App.

## Percentage accounting

The prior 85% M18 estimate is **provisional**. This iteration advances durable ledger isolation and corruption rejection, but until the rich mutation path and executable Rust validation are resolved it would be misleading to claim M18 is complete. Existing M19–M22 planning estimates are unchanged: 75%, 65%, 65%, and 10%, respectively.

## Changed-line measurement

See `docs/orbit154-diff.json` for the complete file-by-file line diff against Orbit 153, including both production and validation changes. Repeated identifier replacements contribute to line churn; numbers do not imply an equal number of distinct functional changes.
