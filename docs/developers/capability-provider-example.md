# Trusted capability provider: implementation checklist

This guide demonstrates the **composition pattern**, not a copy-paste executable plugin API. Check the exact trait signature in `crates/orbit-runtime/src/blueprint_execution.rs` against the installed version before coding.

## End-to-end extension path

1. Define a declarative blueprint action with a stable capability ID and narrowly typed fields. Validation must reject missing references and unrecognized values.
2. Implement `BlueprintCapabilityExecutor` in trusted Rust application code, outside blueprint JSON/YAML. The provider should implement its advertised `capability_id()` and the runtime's current `execute(...)` contract.
3. Wire that provider explicitly into `BlueprintExecutorRegistry` at the trusted composition root. Blueprint installation must **never** implicitly load code or register providers.
4. Before calling a side-effectful port, validate the persisted, authorized `ActionInvocation` against the task/project/repository association and agent permissions.
5. Persist/claim the action through `AuthoritativeHost` before performing external effects. Respect the action state machine: `pending` → `claimed` → `succeeded` / `failed` / `uncertain`. An uncertain outcome is not safe to automatically retry.
6. Return the recorded result to the requesting user. An external coding-agent finishing its run is not a request to complete the underlying Orbit task.

## Suggested verification scenarios

- Missing provider: action remains non-executed and reports unsupported capability.
- Invalid project/task association: forbidden with no external effect.
- Replayed command ID: no second agent launch.
- Timeout after dispatch: classify as uncertain and reconcile before retry.
- External runtime unavailable: core local project/task/blueprint workflows continue.
- Connected client: the authority, not the local replica, accepts the command.

## Where to read the implementation

- `crates/orbit-blueprint/src/` — declarative manifests/invocations
- `crates/orbit-storage/src/blueprint_actions.rs` — durable action record
- `crates/orbit-runtime/src/blueprint_execution.rs` — registry and trusted adapter
- `crates/orbit-runtime/src/blueprint_authority.rs` — authoritative operations

See [MCP permissions](../reference/mcp.md) for agent scopes. Do not create new public executables, direct shell-invocation endpoints, or alternate execution engines.
