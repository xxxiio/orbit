# M25 — Blueprint MCP management (Development 291)

The existing `orbit-mcp` tool surface now exposes `blueprint.list`, `blueprint.show`, `blueprint.binding`, `blueprint.item`, `blueprint.publish`, `blueprint.install`, `blueprint.bind`, `blueprint.create_item`, `blueprint.transition`, `blueprint.properties` and `blueprint.template`.

## Authority semantics

Embedded mode calls `AuthoritativeHost` directly; connected mode calls the authenticated blueprint HTTP endpoints. No new database or independent state machine is added. Both modes enforce the agent session’s project scope and developer/write permission. Workspace-wide registry reads and publication additionally require unrestricted workspace scope. Existing work items are retrieved before mutations to enforce project scope. Blueprint transitions do not grant external capability execution permission. Connected mode requires an online authority rather than offline staging.

## Example tool-call parameters

- `blueprint.list`: `{}` or `{"id":"development"}` (unrestricted workspace access)
- `blueprint.show`: `{"id":"development","version":1}`
- `blueprint.binding`: `{"project_id":"PROJECT-123"}`
- `blueprint.item`: `{"id":"FEATURE-001"}`
- `blueprint.publish`: `{"definition":{...}}` (complete validated manifest)
- `blueprint.install`: `{"id":"development"}`
- `blueprint.bind`: `{"project_id":"PROJECT-123","id":"development","version":1}`
- `blueprint.create_item`: `{"project_id":"PROJECT-123","id":"FEATURE-001","kind":"feature","properties":{}}`
- `blueprint.transition`: `{"id":"FEATURE-001","to":"review","expected_revision":0}`
- `blueprint.properties`: `{"id":"FEATURE-001","expected_revision":1,"changes":{"approved":true}}`
- `blueprint.template`: `{"project_id":"PROJECT-123","template_id":"starter","namespace":"demo"}`

## Receipt and remaining-work status

Server-side blueprint command receipts are atomic and are exercised by the blueprint smoke test; ordinary offline replay/reconciliation is implemented and exercised there too. The remaining gaps are specific:

- Connected MCP writes bypass the canonical outbox and its receipts.
- Lost-acknowledgement blueprint replay has no end-to-end test.
- Capability action request/execute surfaces are not reachable from the CLI, MCP or HTTP; only the model/executor primitives exist.
- No blueprint upgrade path exists.

Trusted concrete capability adapters and final plugin composition remain unimplemented. Broad Rust compile and integration acceptance, along with resulting fixes, remain in M15.
