# M25 — Blueprint MCP management (Development 291)

The existing `orbit-mcp` tool surface now exposes `blueprint.list`, `blueprint.show`, `blueprint.binding`, `blueprint.item`, `blueprint.publish`, `blueprint.install`, `blueprint.bind`, `blueprint.create_item`, `blueprint.transition`, `blueprint.properties` and `blueprint.template`.

## Authority semantics

Embedded mode calls `AuthoritativeHost` directly; connected mode never posts a blueprint mutation straight at the authority. No new database or independent state machine is added. Both modes enforce the agent session’s project scope and developer/write permission. Workspace-wide registry reads and publication additionally require unrestricted workspace scope. Blueprint transitions do not grant external capability execution permission.

### Connected writes: one queue-first path (M30 I2 / M25 EC-004)

Every connected blueprint write resolves the same frozen `route` and wire `body` (unchanged payload construction and argument validation, including scope checks) and then calls one helper, `ConnectedMcp::blueprint_write`:

1. `queue_blueprint_transport(command_id, route, body)` first. The intention lands in the canonical, globally ordered replica outbox with `kind='blueprint'`, so an unreachable server cannot lose it. Repeating the same identity with the same body is a no-op; reusing it for a different body conflicts without touching accepted history.
2. Then the canonical `flush`. Only the same global flush that carries every other command family is used; no blueprint-specific transport exists.
3. The disposition is read back from that exact command identity (`command_state` / `accepted_receipt`), never from aggregate queue depth.

| State observed | `disposition` | Payload |
|---|---|---|
| outbox row `accepted` with a readable receipt | `accepted` | `receipt` (the authority’s immutable acceptance envelope) and `result` (the applied blueprint, binding, work item or template plan) |
| row still `pending`, state unobservable, or the flush failed/unreachable | `pending` | `command_id`, `state`, `reason`, `message`; never a server payload |
| row `conflict` (local identity reuse or an authority rejection) | `conflict` | `command_id`, `state`, `reason`, `message`; `receipt` is never fabricated |

Replaying an accepted identity returns the identical receipt and applies nothing new, because the server’s `orbit_blueprint_command_receipts` row is immutable and bound to `(command_id, device, route, canonical request)`. That same receipt is what makes lost-acknowledgement recovery safe: a client that queued and applied but never saw the response replays the frozen identity and receives the original result exactly once.

Reads (`blueprint.list`, `blueprint.show`, `blueprint.binding`, `blueprint.item`) stay direct GETs and never queue; they create no state. Local (`embedded`) mode is unchanged: it applies blueprint mutations synchronously through the host services and keeps no outbox.

## Example tool-call parameters

Connected writes accept an optional `command_id`; omit it and the response returns the generated identity. Reads take no command identity.

- `blueprint.list`: `{}` or `{"id":"development"}` (unrestricted workspace access)
- `blueprint.show`: `{"id":"development","version":1}`
- `blueprint.binding`: `{"project_id":"PROJECT-123"}`
- `blueprint.item`: `{"id":"FEATURE-001"}`
- `blueprint.publish`: `{"definition":{...},"command_id":"optional"}` (complete validated manifest)
- `blueprint.install`: `{"id":"development","command_id":"optional"}`
- `blueprint.bind`: `{"project_id":"PROJECT-123","id":"development","version":1,"command_id":"optional"}`
- `blueprint.create_item`: `{"project_id":"PROJECT-123","id":"FEATURE-001","kind":"feature","properties":{},"command_id":"optional"}`
- `blueprint.transition`: `{"id":"FEATURE-001","to":"review","expected_revision":0,"command_id":"optional"}`
- `blueprint.properties`: `{"id":"FEATURE-001","expected_revision":1,"changes":{"approved":true},"command_id":"optional"}`
- `blueprint.template`: `{"project_id":"PROJECT-123","template_id":"starter","namespace":"demo","command_id":"optional"}`

`blueprint.bind` is a first-binding operation: an already-bound project only accepts the identical binding, and any other binding or upgrade is still refused by the authority.

## Receipt and remaining-work status

Server-side blueprint command receipts are atomic and are exercised by the blueprint smoke test; offline replay/reconciliation is exercised by the CLI and by MCP `blueprint.*` writes. Remaining gaps:

- Capability action request/execute surfaces are not reachable from the CLI, MCP or HTTP; only the model/executor primitives exist.
- No blueprint upgrade path exists.

Trusted concrete capability adapters and final plugin composition remain unimplemented. Broad Rust compile and integration acceptance, along with resulting fixes, remain in M15.
