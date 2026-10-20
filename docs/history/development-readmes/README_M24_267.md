# Orbit Development 267 — Storage facade modularization

Baseline: Development 266. M24 implementation estimate **45% → 52% (+7 pp)**.

- Reduced `orbit-storage/src/lib.rs` from 3,252 lines to a 103-line stable facade.
- Moved SQLite connection, migration, and schema lifecycle into `store_core.rs`.
- Isolated application transaction/savepoint semantics in `unit_of_work.rs`.
- Moved project criteria loading into `project_criteria.rs`.
- Moved SQLite row mapping/codecs to `row_mapping.rs` and SQL/filesystem helpers to `sqlite_helpers.rs`.
- Relocated existing storage test suites to `storage_tests.rs` without modifying test logic.
- Retained `SqliteStore`, schema version 19, SQLite transaction behavior, and public exports.

This is source-level restructuring, not release acceptance. Broad Rust compilation and acceptance fixes belong to M15. The next M24 priority is finishing I2 storage/client modularization, then I3 offline-command consolidation. M22 and M23 remain 100% reported feature-source scope; M15 remains pending. Internal testing has not been established.
