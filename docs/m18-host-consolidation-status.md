# M18 consolidated native-host composition (source stage)

## Delivered in this worktree

- Removed the obsolete `OrbitRuntime` wrapper and its `into_legacy_store` escape hatch. The supported host is `AuthoritativeHost`.
- Moved ownership of the complete existing `SqliteStore` application repository into `AuthoritativeHost`, alongside the canonical authority command ledger and Markdown vault.
- `OrbitClient` no longer opens or owns `SqliteStore` independently: it uses one host for existing planning, project, habit and task use cases and for Markdown knowledge commands.
- CLI, local MCP and TUI paths that formerly opened their own `SqliteStore` now open `AuthoritativeHost` and borrow its application repository.
- Added a repeatable adapter-composition regression guard and included it in the M18 acceptance script.

## IMPORTANT: not runtime consolidation acceptance

**There are still two persistent data models within the same host.** `application: SqliteStore` and `authority: RelayStore` open distinct connections to the same database file, and the older feature-rich project/task/habit commands do not all use canonical revisions and receipts. This refactor consolidates *ownership and opening*, not the complete repository/transaction semantics. CLI knowledge commands and TUI term references still use the old knowledge-domain repositories; do not interpret host ownership as knowledge API parity. Remote replicas, authenticated HTTP and local/remote MCP require real executable parity testing.

## Verification status

- Python offline replica regression suite: 17 tests passed.
- Python worker tests: 12 passed (plus 10 subtests).
- Markdown/folder source-contract guards: passed.
- Native-host composition source guard: passed.
- Rust compile/test: **not executed** (cargo/rustc absent).
- Full M18 acceptance and internal-testing readiness: **not established**.

## Highest-priority remaining work

1. Unify project/task/habit/metric business state, transaction boundaries, revision receipts and projections (currently two repository implementations in the same host).
2. Replace remaining legacy knowledge/term/resource CLI and TUI use cases with file-authoritative service calls without losing required capabilities.
3. Verify parity and interruption recovery in embedded, HTTP and MCP modes and run the complete Rust workspace acceptance suite.

Flutter remains in Atomic App, outside M18.
