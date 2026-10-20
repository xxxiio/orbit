# Orbit 073 — architecture and maintainability refactor

## Scope and rationale

The first priority is separating storage, CLI input contracts, and embedded DTOs into stable modules **without changing the domain, sync wire format, SQLite schema, or public commands**. These three areas are high-change surfaces and their former single-file organization made reviews and ownership difficult.

## Implemented

- `orbit-storage::repositories::{entities,resources,sync}` implements existing application repository ports; `SqliteStore` remains the public adapter, migration owner, and `UnitOfWork` implementation. There is no new storage abstraction, transaction scope, or schema change.
- `orbit-cli::commands` owns the clap command model. The crate root re-exports all existing public command types, retaining CLI consumers' import paths.
- `orbit-client::contract` owns existing DTOs, error types and embedded contract types. The crate root re-exports public contracts; `OrbitClient` stays the public facade.
- No new dependency-injection framework, ORM, event bus, state machine, or new crate. Existing repository, unit-of-work, outbox, and facade patterns remain the intended boundaries.

## Known technical debt and deliberately deferred work

- CLI `run_*` handlers and output rendering still form a long module. Extracting them safely into further modules requires executable typechecking to catch private helper visibility and trait import changes.
- `orbit-client` still contains large facade implementation blocks. Distinct AI and planning service facades are candidates after executable contract tests are available.
- `orbit-server::main` still combines route assembly, request authentication, and handlers. Its separate automation, proposal, relay, and MCP stores have independent locks. Do not unify them into an ill-defined transaction without first specifying atomicity across stores.
- Agent proposal state machine and remote-worker crash recovery require integration acceptance; no new exactly-once guarantees are claimed.

## Compatibility and acceptance requirements for M15

1. `cargo fmt --all -- --check`, `cargo check --workspace --all-targets`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`.
2. Verify previously exposed Rust types from `orbit_cli`, `orbit_client`, and `orbit_storage` remain importable via representative downstream compile tests.
3. Run SQLite migrations and legacy backfill against a copied v11/v12 database; verify sync outbox/replay and knowledge lookup results match 072.
4. Run CLI, embedded client, MCP, TUI, two-device Git/HTTP, server restore, and approved-agent workflow smoke tests.
5. Review cross-store server transaction boundaries and pending mutation lease reconciliation for data-loss/duplicate-action defects.

No full Rust compile or smoke pass was executed in the authoring environment. Static file/module checks are not substitutes.
