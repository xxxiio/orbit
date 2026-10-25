# M25 — Trusted blueprint capability execution

## Responsibility and safety boundary

A blueprint describes **offers**, never executable permissions. Users explicitly
request an offered action through `AuthoritativeHost`, which checks the pinned
blueprint, work-item revision, registered capability catalog and trusted grants
before persisting an immutable request in SQLite.

`BlueprintExecutorRegistry` (in `orbit-runtime`) is constructed by a trusted
host/worker. The blueprint manifest cannot register a provider or name a shell
command. The registry dispatches only **pending, already-persisted** requests;
missing providers leave requests pending. A claim is committed before the
provider is called. Only `succeeded`, `failed`, or `uncertain` results are
recorded. `claimed` and `uncertain` requests **must not** be automatically
retried: after a process crash, the external effect may have happened.

A provider implements `BlueprintCapabilityExecutor`, receives typed
`ActionInvocation` data, and should propagate the stable `command_id` as an
idempotency key when the underlying integration supports it. The provider must
avoid interpreting blueprint parameters as an executable command line.

## Example composition (illustrative)

```rust,ignore
let mut registry = BlueprintExecutorRegistry::default();
registry.register(Box::new(approved_provider))?;
let receipt = registry.execute_claimed(&mut host, "requested-command-id")?;
```

## Current surface and remaining integration

The registry is an **opt-in composition API**, not a background worker. Actual
Herdr and other capability providers must be registered by trusted deployment
composition and obey their own permissions. Do not call `execute_claimed`
directly from an untrusted HTTP request.

The local CLI now exposes the composition path:
`orbit blueprint capability list|install|uninstall|authorize|revoke` and
`orbit blueprint action offers|request|execute|show|list|reconcile`. The
`TrustedComposition` used by the CLI registers the deterministic
`orbit.reference.work-item-note` capability, whose executor writes one durable
note row keyed by the action command ID. Installation, project authorization,
request, claim, execution and receipt stay separate; publication of a manifest
that requires a capability is gated by durable installation.

Connected transports and MCP intentionally expose no capability
administration or action-execution surface in this iteration. Connected mode
refuses the CLI groups with `M25 EC-004`; MCP has no capability tools. Do not
add a transport that can dispatch a capability without the same durable
installation, grant, request and receipt contract.

Executable Rust/workspace and deployment acceptance remain under M15.

## Trusted Orbit agent launch adapter (Development 296)

`LocalAgentLaunchCapability<R>` registers the existing `agent.launch` capability
through the generic executor registry. It requires a trusted `AgentRuntime`
instance and an explicit allowlist of absolute repository paths. A blueprint
manifest cannot add repositories to the allowlist, install runtimes, or invoke
commands without a persisted, authorized action request.

The invocation uses `task_id`, `repository`, `agent` (`codex` or `opencode`),
and optional boolean `worktree`. The task must be a **core Orbit task** in the
same project as the blueprint work item. This is intentionally explicit:
blueprint work items are not interchangeable with core tasks.

The action command ID becomes the agent job ID. The workflow action is claimed
before the runtime is invoked; uncertain failures remain fenced for operator
reconciliation, never automatically relaunched. The adapter is opt-in at host
composition: no capability is installed merely because the blueprint declares
`agent.launch`. A remote-machine adapter requires a separately authorized
machine executor and is not represented by this local adapter.

Release-acceptance compilation, integration tests and live Herdr exercises are
owned by M15.
