> ARCHIVED DESIGN RECORD — not an implementation contract. Superseded by `architecture.md`. The old relay/Git synchronization model was retired; do not reintroduce it.

# Orbit optional self-hosted architecture (decision 2026-10-10)

**Status:** architecture accepted; M16 now in source implementation. M14 planned feature source is 97% with full acceptance deferred to M15 by explicit product decision. M16 has a first authenticated semantic relay slice; M17 remains planned. See `server-api.md` for what works and what remains.

## Deployment modes

1. **Standalone (default):** existing CLI/TUI/embedded desktop/mobile Rust runtime with local SQLite and optional Git remote. Full offline read/write without an account.
2. **Self-hosted (opt-in):** exactly the same application/domain commands run behind `orbit-server`, with authenticated API/sync and optional background workers. Device clients keep local databases and write while offline; they reconcile later.
3. **Managed host (future, not committed):** potential deployment variant of the same server, only after separate privacy, operations and trust review.

## Topology

```text
Desktop / TUI / Flutter mobile              Web/PWA (later, M17)
          | local commands                      | authenticated HTTPS
          v                                     v
  embedded Orbit Rust runtime  <----sync----> optional orbit-server
          |                                     | API/auth/job scheduler
          v                                     v
     local SQLite                       server storage + outbox
          |                                     |
          +---- Git-compatible event history --+ 
                              (transport adapters)

Local/headless MCP -> application ports    Remote MCP -> scoped server application ports
Optional local/cloud BYOK AI provider      Optional explicitly configured AI worker
```

## Package boundaries and ports

- `orbit-domain` contains canonical entities, transitions, validation and completion gates; it has no server dependencies.
- `orbit-application` coordinates commands, queries, permissions, outbox/event emission and idempotent application; client and server compose the same services.
- Storage repository ports isolate embedded SQLite from server persistence; start server with SQLite and a single process, with migrations/backup before scale changes. Never expose SQLite over the network.
- A transport-neutral sync protocol carries immutable events, causal identity/sequence and semantic resolution records. Existing Git storage and replay remain valid adapters; server does not invent a second incompatible history.
- `orbit-server` (M16) supplies authenticated HTTP API, enrollment/revocation, per-device cursor and sync, TLS deployment guidance, basic admin/health/backup tools, and Docker Compose packaging. Limit initial ownership to one user/workspace.
- `orbit-jobs` and remote agent endpoints (M17) are independently enabled. Persist schedules and leased jobs with idempotency keys; audit all side effects; use capability-scoped tokens and explicit approval policies for actions.
- Web UI (M17) calls the same API; it must not own separate business invariants. It may be online-first initially; native clients must retain offline capabilities.

## Synchronization and conflict invariants

- Device is writable offline. The server must not become an obligatory source for basic reads/writes.
- Preserve per-device event identity, monotonic sequence reservations, immutable resolution artifacts, and deterministic replay. Sync is at-least-once and idempotent, not claimed exactly-once.
- Clients reconcile conflicts using existing semantic merge/resolution domain rules. Never silently overwrite one device's user choice.
- Define transport migration/dual-transport behavior explicitly before implementation; do not assume Git and HTTP streams can be naively merged.
- No server process should write directly to a client's local database; clients apply vetted event envelopes through application services.

## Threat boundaries

- Server may process decrypted content for automation: opt-in server hosting is not automatically content-blind or end-to-end encrypted. Document what reaches the server and each selected AI provider.
- Protect internet-exposed endpoints with TLS, authenticated device registration, token rotation/revocation, rate limits and least-privilege agent capabilities. Private networking is a deployment option, not a substitute for application authorization.
- Keep API keys and provider credentials out of sync data and Git history; store secrets using dedicated secret mechanisms.
- No mandatory central accounts, automatic cloud export, compulsory AI services or telemetry.

## Milestone gates

**M14 before M16:** project/habit/metric features source-complete enough to move forward; full executable M14 acceptance explicitly deferred to M15. Do not mark M14 fully accepted.

**M16 acceptance:** single-user Compose deployment, authenticated enrollment/revocation, two locally writable devices syncing through server, tested offline conflict/replay compatibility with Git-resolution semantics, restart and backup/restore smoke. Source-only implementation is not closure.

**M17 acceptance:** persistent scheduled job executes once in logical-effect terms across retry/restart, scoped remote agent/API with explicit approvals/audits, authenticated web view/update through shared services, and native offline regression checks.

**M15 after M16/M17:** cross-platform integration, migration, performance, resilience, privacy, deployment and release gates. Its ID stays M15 for history stability.
