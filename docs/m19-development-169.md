# Orbit 169 — M19 SQL knowledge contract retirement

## Scope

This source iteration removes inactive SQL knowledge repository implementations, legacy row decoders, FTS helper, obsolete SQL knowledge storage tests, and the last knowledge repository requirements from the canonical application `UnitOfWork` contract. These were remnants of the former SQL-backed knowledge source of truth, which was removed from the fresh schema in Orbit 168.

## Runtime boundary

* `UnitOfWork` now requires only authoritative non-knowledge repositories and activity/outbox transactions.
* `SqliteStore` no longer implements legacy `KnowledgeRepository`, `KnowledgeResourceRepository`, or `KnowledgeResourceLocatorRepository`.
* The historical SQL knowledge use cases are kept explicitly isolated behind the deprecated `LegacyKnowledgeUnitOfWork` compatibility trait; `SqliteStore` no longer implements that combined contract, so they cannot use the active runtime store.
* Unreferenced SQL knowledge row decoders, relation loader, FTS helper and SQL-knowledge-specific tests that expected the retired schema have been removed.
* Fresh workspaces continue to reject databases containing legacy SQL knowledge tables without deleting existing data.

## What is not yet done

The old `orbit-application` knowledge command, query and retrieval module exports and some stale client/CLI types still exist as historical API definitions. They should be removed or replaced with file-backed contracts in a subsequent coordinated change. Do **not** label M19 100% completed until the public application API no longer offers competing knowledge workflows. Compiler and full end-to-end acceptance have been deferred to M15 by the user. Source-level checks cannot prove Rust compilation.

## Verification

* `scripts/legacy-contracts/test-m19-application-boundary.py`: passed.
* `scripts/legacy-contracts/test-m19-sql-retirement.py`: passed.
* Other six M19 source test scripts: passed.
* `scripts/test-authority-offline.py`: 17 passed.
* Python worker tests: 12 passed, 10 subtests.
* Rust compilation: unavailable; deferred to M15.

## Progress

M19: provisional implementation estimate 99%; acceptance not established. M18: 99%; M20: 65%; M21: 65%; M22: 10%; M15: 8%. This iteration removes core architectural dead paths and prevents the active application transaction from depending on retired SQL knowledge repositories. Full API retirement remains open.

Measured diff is in `docs/orbit169-diff.json` (Orbit 168 baseline, excluding the diff report and this new development note).
