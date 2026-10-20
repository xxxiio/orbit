# Orbit 145 — M18 Unified Application Runtime checkpoint

## Decision and scope
M18 in the revised M18–M22 roadmap is **shared application runtime and authoritative SQLite ownership**, not completion of every remote transport, Markdown feature or final release acceptance. M19 handles full Markdown/folder functionality, M20 transport parity, M21 offline replication, and M22 integrated acceptance. Essential checks belong to every iteration.

## Implementation
* Removed `AuthoritativeHost::application_store_mut`, the public mutable repository escape hatch.
* Migrated the existing local MCP mutation handlers to host-owned `ApplicationServices` for evidence binding, metrics, tasks, project status, project strategy, success criteria, milestones, and habits. Authorization/scope checks remain inside the MCP adapter; business mutations delegate to the shared service.
* Extended `ApplicationServices` with the existing milestone transition use case, using the same host-owned SqliteStore and activity transaction convention.
* CLI's legacy handlers remain temporarily in use through an explicitly named, scoped `ApplicationServices::with_legacy_cli` bridge. This is **not** a completed typed CLI application-service migration. Local CLI retains legacy SQL-backed knowledge command functionality; M19/M20 must retire it.
* Updated the composition guard to reject reintroduction of `application_store_mut` and check that 10 MCP write-operation classes call the shared services. This is source-only evidence.

## Results
* Python worker tests: 12 passed, 10 subtests passed.
* Offline synchronization tests: 17 passed.
* Application-service, host-composition, entity-schema, project/task, legacy relay, Markdown and folder-contract source checks: passed.
* Rust: **not compiled** (`cargo` and `rustc` unavailable).
* Verified embedded/server rich workflow parity: **not established**.

## M18 acceptance criteria (no fabricated percentage)
| Criterion | Evidence | Status |
|---|---|---|
| One SqliteStore owner per authoritative host | source guards | source evidenced, not runtime verified |
| No direct mutation of application store from MCP | source guard | source evidenced, not runtime verified |
| Shared rich application services | 17 typed service methods including milestone; local client and MCP call them | source evidenced, not runtime verified |
| No remaining ad hoc client repository mutations | CLI scoped legacy bridge and other read-only repository access remain | incomplete |
| No duplicate/obsolete authoritative ledger semantics | relay history machinery still exists | incomplete |
| Build / executable core acceptance | no Rust toolchain available | blocked |

## Next priority
Finish M18 without broadening it to M19–M22: remove the CLI legacy write bridge with typed shared services; eliminate overlapping application write/receipt semantics and retired peer-relay dependencies; execute Rust tests in an environment with a toolchain. Do not mark complete until that work and acceptance are satisfied.

Flutter/Dart remains delegated to Atomic App, outside Orbit M18.
