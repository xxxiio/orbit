# Developer guide: trusted capability integrations

This guide is for developers extending Orbit, not end users installing a blueprint. The safe default is a **declarative blueprint without executable capabilities**.

## Architecture boundaries

1. `orbit-blueprint` owns manifests, work-item kinds and action invocation planning.
2. `orbit-storage` persists authoritative work items, capability installations, project grants, action requests and receipts.
3. `orbit-runtime` coordinates authoritative host operations and trusted execution adapters.
4. `orbit-cli` / `orbit-mcp` expose user entry points; they must not implement a second capability engine. In this iteration the local CLI is the only supported programmatic surface; MCP has no capability tools.
5. Optional Herdr execution belongs behind registered ports and adapters. Server request handlers must not become generic shell execution endpoints.

The only path from an installed capability descriptor to an execution is the host-side
`TrustedComposition` in `crates/orbit-runtime/src/capability_composition.rs`. The
executor trait and registry live in
`crates/orbit-runtime/src/blueprint_execution.rs`. Durable installation, grant,
request and receipt tables live in
`crates/orbit-storage/src/capability_installs.rs` and
`crates/orbit-storage/src/blueprint_actions.rs`.

## Composition root

`TrustedComposition` is process-local state built by the host. It is not a global
registry, and it is never populated from a manifest:

```rust
let mut composition = TrustedComposition::empty();
composition.register(capability, Box::new(MyExecutor))?;
```

`TrustedComposition::with_builtin_reference()` is the bundled composition used by
the CLI. It registers `orbit.reference.work-item-note` with a deterministic,
I/O-free executor. It also registers the opt-in habit recurrence package
(`orbit.habit.recurrence`, implemented in `crates/orbit-habit-capability`), but
registration is not installation or authorization: habit streaks and project
habit consistency stay unavailable until the durable install and project grant
steps are performed. See the [habits guide](../guides/habits-routines-schedules.md)
for the user-facing sequence. `register` rejects:

- an empty or duplicate capability ID;
- an empty or duplicate action ID;
- an action whose `input_schema` is not a JSON object;
- an executor whose `capability_id()` does not match the capability ID;
- an executor that is already registered.

`TrustedCapability` carries the `CapabilityDescriptor` (`id`, `provider`,
`permission`, `enabled`), the advertised `CapabilityActionSpec` list, and a
`requires` set of other capability IDs that must be installed before this one is
usable. A descriptor can be registered but disabled: availability then reports
`capability_disabled` rather than `capability_missing`.

Each `CapabilityActionSpec` carries:

| Field | Meaning |
| --- | --- |
| `action_id` | Stable action name referenced by a manifest transition |
| `description` | Human-readable purpose |
| `input_schema` | JSON-schema subset used to validate merged parameters |
| `external_effect` | Whether the action reaches outside the workspace |

`is_destructive()` is true when `external_effect` is true or the schema contains
`"destructive": true`. The CLI requires `--confirm` for destructive actions. A
manifest never decides this: an action whose contract is unknown is treated as
effectful so a caller cannot skip confirmation by removing the composition.

### Input-schema subset

The current validator (`validate_capability_inputs` in
`capability_composition.rs`) is deliberately small:

```json
{
  "type": "object",
  "properties": {
    "note": { "type": "string", "minLength": 1 },
    "attempts": { "type": "integer" },
    "dry_run": { "type": "boolean" }
  },
  "required": ["note"]
}
```

- every name in `required` must be present;
- `string` must be a JSON string and honours `minLength` and `maxLength`
  (character count);
- `integer` must be a JSON integer and honours `minimum` and `maximum`;
- `boolean` must be a JSON boolean;
- unknown properties and unknown types are not enforced. This is not a general
  JSON Schema engine, so patterns, enums and nested schemas are ignored.

A transition action's `parameters` object supplies defaults. A request may send
any input the capability's `input_schema.properties` declares, even when the
manifest does not list it, plus any key the manifest `parameters` object names.
The storage boundary rejects an input key that neither layer declares, so a
caller can never invent a capability parameter.

## Lifecycle: installation, authorization, request, claim, execution, receipt

These are separate durable facts. None is implied by another.

| Phase | Operation | Effect |
| --- | --- | --- |
| Installation | `orbit blueprint capability install ID` | Copies the trusted composition's provider, permission, actions and requirements into a durable installation. An identical replay is idempotent; a changed contract is refused until an explicit uninstall. |
| Authorization | `orbit blueprint capability authorize ID --project PROJECT --confirm` | Creates one project-scoped grant. Requires an existing installation and an existing project. Installation alone grants nothing. |
| Offer | `orbit blueprint action offers ITEM` | Read-only projection of the transitions available from the item's current state, with availability computed from durable state. |
| Request | `orbit blueprint action request ITEM --action ID --revision N ...` | Re-checks the offer, revision, requirements, grant and inputs, then persists a `pending` request with a stable command ID. This is intent, not execution. |
| Claim | `orbit blueprint action execute COMMAND_ID` | The registry claims a `pending` request. The claim commits before the provider runs. |
| Execution | Provider `execute` | Receives the host, the durable request and the persisted `ActionInvocation`. |
| Receipt | Recorded by the host | Stores `outcome` (`succeeded`, `failed` or `uncertain`), the installation's `provider`, a detail object and `recorded_at_ms`. A replay of `execute` returns the receipt and never runs the provider again. |

Publication is also gated by installation. `orbit blueprint import` refuses a
manifest whose `required_capabilities` are not durably installed and enabled in
the current composition. An offer is never permission: every check is repeated
when the request is persisted.

The reference capability is the worked example. Its descriptor is provider
`orbit`, permission `mutate`, capability ID `orbit.reference.work-item-note`. It
advertises one action, `append_note`, with `external_effect: false`. Its executor
writes exactly one note row keyed by the action command ID, so a replay cannot
append a second note. The example manifest
`blueprints/examples/capability-reference.json` declares the capability in
`required_capabilities` and offers `append_note` on `open -> closed`:

```sh
orbit blueprint capability install orbit.reference.work-item-note
orbit blueprint capability authorize orbit.reference.work-item-note \
  --project PROJECT --confirm
orbit blueprint action offers ITEM
orbit blueprint action request ITEM --action append_note --revision 0 \
  --command cmd-note-1 --input '{"note":"first durable note"}'
orbit blueprint action execute cmd-note-1
orbit blueprint item ITEM
```

The durable effect is visible on the item (trimmed from `orbit blueprint item`):

```json
{
  "capability_notes": [
    {
      "command_id": "cmd-note-1",
      "item_id": "note-1",
      "note": "first durable note",
      "tag": null
    }
  ]
}
```

The full copy-pasteable sequence, including publishing the manifest in the
required order, is in [Build your own project workflow](../guides/custom-blueprints.md).

## Failure semantics

- `failed` is a recorded outcome. Orbit does not retry it automatically.
- `uncertain` is a recorded outcome for an ambiguous external effect. Retrying
  could duplicate the effect.
- `claimed` without a receipt means the process died between the durable claim
  and the recorded result. The effect may already have happened, so Orbit does
  not launch it again.
- Only `orbit blueprint action reconcile COMMAND_ID --succeed|--fail --confirm`
  resolves `claimed` or `uncertain`. `pending` is refused because it has not
  executed; `succeeded` and `failed` are immutable through reconcile.
- Reconcile emits `{"recovered_from": "claimed"|"uncertain", "receipt": {...}}`.
  The provider identity stays the durable installation's provider, not an
  operator-supplied string.
- A provider must propagate the command ID as an idempotency key when its
  integration supports one. The reference executor uses it as the unique key for
  the note row.

## What a provider must not do

- Do not register a provider from a manifest, a blueprint parameter or an HTTP
  request body. Registration happens only at trusted host composition.
- Do not treat parameters as a command line, shell fragment or executable path.
- Do not derive authority from an offer, an action description or an earlier
  availability check. Re-check the durable request, item, project and grant.
- Do not create direct SQLite connections. Use the authoritative host and
  application services passed to the composition.
- Do not report `Succeeded` when the external result is unknown. Return
  `Uncertain`; an operator resolves it explicitly.
- Do not retry after `uncertain` or a lost claim.
- Do not perform unbounded network, filesystem or interactive work while holding
  a write transaction.
- Do not keep global mutable provider state. The composition is constructed by
  the host process and passed explicitly.

## Implementing repository and execution adapters

Follow existing ports defined in `orbit-application` and adapters in
`orbit-storage` / `orbit-runtime`. Inject concrete dependencies at composition
roots. Do not create direct SQLite connections inside blueprint definitions, CLI
parsing, or MCP tool handlers.

## Connected and MCP status

Connected mode refuses capability administration and action commands with
`capability actions require the local authoritative workspace; connected parity
is tracked as M25 EC-004`. It does not queue a request that cannot execute.
There are no capability MCP tools in this iteration. The local
`blueprint.transition` MCP tool does use the project's durable grants, so a
capability-guarded transition works after a local authorization.

## Local developer checks

```sh
cargo fmt --all -- --check
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
```

For broader release testing, use `./scripts/m15-full-automated.sh --profile full`. The latter may require build tools or configured optional services; never report unexecuted checks as passed.

Related: [Repository layout](../contributing/repository-layout.md), [Testing](../contributing/testing.md), [Blueprints versus tasks](../guides/blueprints-vs-tasks.md).
