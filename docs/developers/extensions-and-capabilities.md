# Developer guide: trusted capability integrations

This guide is for developers extending Orbit, not end users installing a blueprint. The safe default is a **declarative blueprint without executable capabilities**.

## Architecture boundaries

1. `orbit-blueprint` owns manifests, work-item kinds and action invocation planning.
2. `orbit-storage` persists authoritative work items and action requests.
3. `orbit-runtime` coordinates authoritative host operations and trusted execution adapters.
4. `orbit-cli` / `orbit-mcp` expose user entry points; they should not implement a second capability engine.
5. Optional Herdr execution belongs behind registered ports and adapters. Server request handlers must not become generic shell execution endpoints.

Look at `crates/orbit-runtime/src/blueprint_execution.rs` for `BlueprintCapabilityExecutor`, `BlueprintExecutorRegistry`, and the existing local-agent adapter. Examine the trait in the source before implementing a provider; this guide intentionally does not reproduce an unstable Rust signature.

## Add a capability safely

- Define a stable capability ID and a clear schema for invocation parameters.
- Register its provider **explicitly** in the trusted composition root; installing or importing a blueprint cannot register arbitrary code.
- Validate repository, task, project, and machine references against accepted Orbit state.
- Authorize the action and persist a unique command ID before attempting external effects.
- Claim durably; distinguish `succeeded`, `failed`, and `uncertain` instead of interpreting timeouts as retryable failures.
- Test idempotent replay, forbidden references, mismatched project/task association, and ambiguous launch recovery.
- Keep Herdr and other external adapters optional. Core Orbit must remain usable without them.

## Implementing repository and execution adapters

Follow existing ports defined in `orbit-application` and adapters in `orbit-storage` / `orbit-runtime`. Inject concrete dependencies at composition roots. Do not create direct SQLite connections inside blueprint definitions, CLI parsing, or MCP tool handlers.

## Local developer checks

```sh
cargo fmt --all -- --check
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
```

For broader release testing, use `./scripts/m15-full-automated.sh --profile full`. The latter may require build tools or configured optional services; never report unexecuted checks as passed.

Related: [Repository layout](../contributing/repository-layout.md), [Testing](../contributing/testing.md), [Blueprints versus tasks](../guides/blueprints-vs-tasks.md).
