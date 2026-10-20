# M20 — Unified authoritative runtime: implementation closure

**State:** implementation scope complete in Development 204. **Not release-accepted.**
Rust compilation, runnable multi-client smoke and broad release acceptance are
M15 obligations; these have **not** been run in this environment. In particular,
this document must not be used to claim that Orbit is ready for internal testing.

## Exact architecture contract

There are exactly two supported deployment modes:

1. **Local-only:** an embedded `AuthoritativeHost` owns authoritative SQLite,
   revision/receipt handling, Markdown vault operations and domain services.
   No server is installed or required.
2. **Connected:** the optional self-hosted server owns `AuthoritativeHost`
   and canonical SQLite. Native clients own `AuthorityReplica` (accepted
   snapshots + pending outbox only) and send operations to the server. Offline
   changes never mutate the accepted projection. The browser uses server APIs.

The host, not the caller's transport, enforces project/task/evidence/schedule/
                                                              routine/milestone/strategy business rules.
Markdown/folder commands use the host-owned physical vault plus durable
command receipts. No Git synchronization or peer relay is supported.

## Source-based audit of supported entrypoints

| Executable | Standalone/local owner | Connected owner | Business writes |
| --- | --- | --- | --- |
| orbit-cli | `AuthoritativeHost` | `sync replica` -> `AuthorityReplica` | host directly or outbox only |
| orbit-tui | `OrbitClient` -> `AuthoritativeHost` | `connected` -> `AuthorityConnection` -> replica | host directly or outbox only |
| orbit-mcp | `Server` -> `AuthoritativeHost` | `ConnectedMcp` -> `AuthorityConnection` | host directly or outbox only |
| orbit-server | `AuthoritativeHost` | N/A, it **is** the authority | authoritative transaction APIs |
| `orbit-client` embedded facade | `AuthoritativeHost` | N/A | host methods |
| `orbit-client` connected facade | N/A | `AuthorityReplica` | queued intentions only |

The server may have ancillary automation/proposal SQLite connections for
server-owned queues; those are not client-side competing authority runtimes.
They are confined to the server and do not own core domain command execution.

## Final source changes

- One shared `orbit_client::deployment` resolver for CLI, TUI and desktop MCP.
  Partial or conflicting connected configurations are rejected instead of
  silently opening the local authoritative database.
- Connected CLI no longer falls back to local `AuthoritativeHost` for commands
  that are not `sync replica`; the user is directed to the connected command
  transport. Standalone CLI remains fully supported.
- Desktop MCP connected and local backends are separate production modules.
  There is one process-level selection; connected MCP cannot instantiate
  a host by accident within its adapter module.
- Existing authority/replica SQLite type checks remain intact on both sides.
- Existing shared durable outbox, history receipts, conflict-resolution and
  accepted-only queries are reused; no new conflicting implementation introduced.

## In-scope command coverage

| Family | Canonical host | Server route | Connected replica | Embedded client |
|---|---|---|---|---|
| Project/task | Yes | Yes | Yes | Yes |
| Habit/metric evidence | Yes | Yes | Yes | Yes |
| Schedule | Yes | Yes | Yes | Yes |
| Routine | Yes | Yes | Yes | Yes |
| Milestone | Yes | Yes | Yes | Yes |
| Project strategy | Yes | Yes | Yes | Yes |
| Markdown/folders | Yes | Yes | Yes | Yes |

## Deferred to M15, not hidden within M20

- Rust `cargo build`, `cargo test`, clippy and release packaging.
- Runnable local CLI/TUI/MCP and server smoke checks.
- Connected two-client offline/refresh/retry/rejection and conflict drills.
- Runtime data-loss assessment and internal testing authorization.

**Implementation closure** and **acceptance** are separate. Do not re-open
M20 for superficial adapters, historical migration tooling, or new features.
If compilation later identifies an actual implementation defect, fix it as
part of M15 acceptance rather than inventing an unfinished M20 feature.
