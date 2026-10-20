# Orbit authority and optional execution module ownership

Status: source architecture following M24 Development 274. The current Rust application and Herdr workflow still require M15 executable acceptance.

## Authoritative boundaries

- `orbit-domain`: durable concepts, identities, and lifecycle invariants.
- `orbit-application`: use cases and repository/runtime ports. Launching, observing, and messaging are separate optional capabilities.
- `orbit-storage`: SQLite schema, transactions, row mapping and repository adapters.
- `orbit-runtime`: in-process composition and one `AuthoritativeHost` per authority database.
- `orbit-server`: authenticated transport with device/machine scope; no arbitrary shell execution.
- `orbit-client`: local or connected configuration, replicas, canonical SQLite pending-command outbox.
- CLI, TUI, MCP: entry-point adapters; the sole public CLI remains `orbit`.

## Agent authority modules

The public API remains `orbit_runtime::agent_authority`:

| Implementation module | Responsibility |
| --- | --- |
| `agent_authority.rs` | Request and response DTOs, validation helpers, immutable resume context, transactional start/resume submission |
| `agent_authority/machine_operations.rs` | Machine-scoped inbox, brief retrieval, claims, launch/status/handoff reporting, session-message delivery |
| `agent_authority/local_operations.rs` | Local-only launch/inspection and evidence/review facade with optional runtime ports |
| `agent_authority/owner_operations.rs` | Device-owned reservations, command acceptance, handoffs, review, overview and attention queries |

The three child modules add inherent methods to the existing `AuthoritativeHost` type; callers do not need to import new public types or select a second authority. Device authentication is a transport concern, while business ownership must still be checked inside the authority.

## Two different durable queues

The canonical **client** outbox stores accepted-or-pending domain and agent intentions. Agent starts are not executed while disconnected; command IDs and authoritative receipts guard idempotency. The **machine-local** execution journal independently fences irreversible external side effects after server acceptance, preventing speculative relaunches on ambiguous results. These must not be merged.

## Historical surfaces

Old M20/M21 milestone READMEs, relay-era documentation, and references in regression tests are historical artifacts, not supported peer-relay or Git synchronization modes. The current local and connected architectures are described in `docs/architecture.md` and `docs/self-hosted-architecture.md`. Avoid removing live compatibility interfaces without establishing that there are no active callers; M15 will test actual runtime integration.

## M15 boundary

M24 covers source structure and intentional interfaces; M15 owns broad Rust compilation, end-to-end HTTP and Herdr acceptance, and fixes uncovered during validation. No source-level restructuring alone proves internal-testing readiness.
