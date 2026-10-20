# M20 implementation consolidation — Development 202

## Architectural correction
The local CLI had a second, dead `SqliteStore`-direct habit mutation implementation alongside the actively dispatched `AuthoritativeHost`-based implementation. This version deletes the dead branch, its direct-storage import, its local clock adapter, and unused application-service imports. The CLI's existing command dispatch remains on `AuthoritativeHost` for all local-only mutations; connected sync continues through `AuthorityReplica`.

## Scope and status
This is a real reduction in duplicated business operations, not M20 implementation closure. Remaining active client and legacy mutation entrypoints must still be inventoried before declaring M20 complete. Broad Rust compilation and release acceptance remain within M15.
