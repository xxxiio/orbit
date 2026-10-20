# Orbit Development 120 — Authenticated canonical knowledge HTTP API

## Priority decision
M18 U2 remains the highest priority: one authoritative application and persistence implementation across local and server deployments. The canonical knowledge read/create/update services already existed in the host, but were inaccessible through server HTTP. This iteration exposes those exact host operations, not a parallel SQL implementation. The rich local SqliteStore is still separate; this is an incremental M18 integration step, not full consolidation.

## Changes
- `GET /v1/authoritative/knowledge`: returns `{revision,items}` using the canonical host.
- `POST /v1/authoritative/knowledge-commands`: accepts `KnowledgeCommand`, returns `KnowledgeResult`.
- `POST /v1/authoritative/knowledge-updates`: accepts `KnowledgeUpdateCommand`, returns `KnowledgeResult`.
- Every handler authenticates an enrolled device via the existing `device` guard, and uses the same `AuthoritativeHost` instance shared by the other authoritative routes.
- Canonical stale revision, immutable command-id receipt, and validation errors use the server's existing error mapping. No direct SQL in the HTTP handlers.

## Validation and limitations
Python offline regression suite, Python syntax checks, source wiring assertions, and archive integrity are checked separately. Rust compiler is unavailable in this environment; HTTP handlers were not compiled, and end-to-end embedded/HTTP/MCP parity is not claimed. HTTP and MCP are not yet feature-equivalent for canonical knowledge. Remote offline client knowledge outbox, knowledge metadata/relations/resources/search, and the old feature-rich local store have not been unified.

## Next highest-priority work
Unify the actual knowledge repository and application services (not just endpoints), preserving metadata, relations, resources, and search; extend transport adapters after canonical service parity. Flutter stays with Atomic App. Internal testing is not ready until real Rust build + core executable smoke + no known blocking data-loss issue.

## Priority assessment at completion
U2 remains unfinished and higher priority than standalone hardening, release acceptance, public distribution, or Flutter screens.
