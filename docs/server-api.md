# Orbit Server v0: opt-in single-user semantic artifact relay

> **Historical architecture notice (M24/M15).** The append-only `/v1/events`
> relay and the `orbit sync init|http|push|pull` commands described in the
> early sections of this document are **retired**. Current connected clients use
> the authenticated canonical authority under `/v1/authoritative/*` through
> `orbit sync authority` and the durable replica outbox (`orbit sync replica`).
> Acceptance is covered by `smoke-authoritative-tasks.py`,
> `smoke-authoritative-domains.py`, `smoke-offline-authority.py`,
> `smoke-server-durability.py`, and `smoke-blueprint-workflows.py`. The relay
> sections remain only as historical design context.

**Status: source implemented, not runtime-accepted.** M16 remains open. This first server binary is an authenticated, persistent transport for the existing `orbit-sync` versioned change and conflict-resolution wires. It intentionally does **not** implement separate project, habit, metric, or agent business rules. It does not replace Git history, materialize a canonical domain database, automatically sync local clients, or provide remote MCP. Those tasks remain M16/M17 work.

## Local/Compose startup

From repository root:

```sh
ORBIT_SERVER_ADMIN_KEY="$(openssl rand -hex 32)" # store this safely; reuse across restarts
export ORBIT_SERVER_ADMIN_KEY
cargo run -p orbit-server
# or: docker compose -f deploy/server/compose.yaml up --build -d
```

The default direct listener is **127.0.0.1:8787**. Compose binds loopback on the **host** even though the container listens on all its interfaces. To access from elsewhere, use a trusted private network or terminate HTTPS with an authenticating/secure reverse proxy. Do not expose this proof-of-foundation relay directly on a public HTTP port. Preserve the admin key and database securely; rotating the admin key is done by updating the process configuration.

## Device enrollment and revocation

Devices have independent secrets (minimum 32 characters). Create a device secret locally and enroll it using the admin key; treat both keys as secrets. Device IDs must match the authored semantic change/resolution identity.

```sh
export DEVICE_TOKEN="$(openssl rand -hex 32)"
curl -fsS -X POST http://127.0.0.1:8787/v1/devices \
  -H "Authorization: Bearer $ORBIT_SERVER_ADMIN_KEY" \
  -H 'Content-Type: application/json' \
  -d "{\"device_id\":\"my-desktop\",\"token\":\"$DEVICE_TOKEN\"}"

curl -i -X DELETE http://127.0.0.1:8787/v1/devices/my-desktop \
  -H "Authorization: Bearer $ORBIT_SERVER_ADMIN_KEY"
```

Device token hashes are stored in SQLite, not plaintext tokens. Revocation stops subsequent authenticated reads and writes by that device; it does not remove previously published immutable history. Reenrollment after revocation is deliberately not supported by this initial API.

## Artifact push/pull

Both local semantic changes and semantic conflict resolutions use their **exact existing versioned JSON strings** (`ChangeEnvelope::to_wire_json` and `ConflictResolution::to_wire_json`). There is no new incompatible event format.

`POST /v1/events`: authenticated per-device bearer token and `x-orbit-device-id` header; JSON `{ "kind": "change" | "resolution", "wire": "<serialized versioned Orbit artifact>" }`. Returns `{ "cursor": integer, "duplicate": boolean }`. A second identical upload is idempotent; different content for an existing `(device_id, sequence)` is rejected with HTTP 409. The source device in the wire must equal the authenticated device. Artifacts are never mutated by the relay.

`GET /v1/events?after=0&limit=100`: same authentication headers; returns `{ "events": [{ "cursor", "device_id", "sequence", "kind", "wire" }], "next_cursor": integer }`. The cursor is a server-side monotonically increasing offset, **not** the source device's semantic sequence. Store the returned `next_cursor` only **after** the local application/replica has durably processed or durably staged *all* returned events. On failure, repeat the pull (duplicates must be idempotent). Limit must be 1–1000.

`GET /health` is an unauthenticated liveness response and must not be interpreted as a readiness or end-to-end sync check.

## Compatibility rules

- Relay upload is **not** proof the device's outbox is fully integrated; delete/ack the local outbox only under a defined client transport adapter lifecycle.
- Source artifacts from Git are content-identical; do not generate new source sequences for HTTP relay transfers. Conversely, **do not** add the HTTP cursor into Git semantic identities.
- `change` and `resolution` artifacts share a `(device, sequence)` namespace, consistent with Orbit's reserved source sequence semantics.
- Arrival order can differ from device source-sequence order. Clients must apply the existing `orbit-sync` dependency/sequence rules, not take last-write-wins on the server. Conflicts are not resolved on the server.
- When using Git and HTTP for the same device, dual transport requires de-duplication by original immutable semantic identity; an adapter to enforce this is **not yet implemented**.
- This relay does **not** store app-level authentication scopes, user accounts, model provider secrets, or AI prompts; authenticated single-user devices currently have shared access to the relay history.
- Server data is not end-to-end encrypted against its owner/host: treat it as sensitive private content.

## Remaining M16 acceptance items

Implement a local client HTTP transport bridge to the application's pending sync outbox and import queue (for both changes and resolutions); multi-device offline/reconnection and dual-Git compatibility tests; backup/restore operator commands and smoke; auth lifecycle improvements, TLS setup documentation, basic operational readiness; real `cargo build`, `cargo test`, and running Compose smoke. Leave full historical M14/release-wide validation for M15 per user direction, but do focused M16 checks as features land.

## Local CLI HTTP bridge (development 047)

After `orbit sync init --device <enrolled-device-id>` (or existing Git sync initialization), enroll that same device ID on the server and run:

```sh
export ORBIT_SYNC_HTTP_TOKEN='<device bearer secret>'
orbit sync http --server http://127.0.0.1:8787
```

Use HTTPS outside loopback. The CLI operates on the same immutable `changes/` and `resolutions/` history as Git. It exports local SQLite outbox records to this history first; uploads all artifacts authored by the current device to the relay (server deduplicates); then stages new remote artifacts before advancing its per-server cursor and rematerializes the existing local replica through Orbit's semantic merge rules. This allows periodic/manual offline reconnection, not continuous or background synchronization. Do not run concurrent HTTP sync operations against the same local root yet. Keep local Git history as well as server data backed up.

`sync http` returns the normal `SyncPull` report. The report counts the materialized local semantic history, not just the HTTP download batch. CLI secrets can be supplied through `ORBIT_SYNC_HTTP_TOKEN`, avoiding embedding credentials in shell history. No public plaintext HTTP remote is allowed.

## Development 048: operator backup, offline restore, and periodic client sync

The optional relay now accepts `orbit-server backup /backups/orbit-relay.db` to create a **consistent SQLite snapshot** via `VACUUM INTO`, including committed WAL data. Destinations must not already exist. Take backups to secure storage independent from the server volume. Admin and device secrets are **not** included in a database snapshot unless your secret manager backs them up separately.

To restore, stop the server process and set `ORBIT_SERVER_DB` to the desired **absent** target database. Run `orbit-server restore /backups/orbit-relay.db`. Restore validates SQLite integrity and required relay tables, refuses overwrite, and refuses SQLite sidecars. Move aside the old database *and its WAL/SHM files safely while stopped* before restore; never restore onto a live database. On a restored older snapshot, clients may hold cursors newer than the restored server; clear/reinitialize the **local per-server cursor** in the `.orbit-local` sync workspace only after assessing replay/recovery, or prefer a new server URL identity. Restoring a server backup alone cannot recreate events published after the backup: clients must republish their immutable local history.

An opt-in sequential polling loop is provided at `deploy/server/sync-loop.sh`. Set `ORBIT_SYNC_SERVER`, `ORBIT_SYNC_HTTP_TOKEN`, and optionally `ORBIT_SYNC_INTERVAL_SECONDS` (minimum 30, default 300); run `deploy/server/sync-loop.sh --root /path/to/sync`. The loop calls the same CLI sync code without spawning concurrent polls. Use a supervisor if always-on operation is desired. Do not run multiple loops against the same local root. Failure retries retain the local outbox and persisted cursor. This is a **polling helper**, not a native background daemon or multi-device end-to-end acceptance proof.

### HTTP client process coordination (development 049)

The local HTTP adapter now holds an operating-system advisory lock over a complete relay exchange. A second exchange against the same local root/server/device fails instead of racing the cursor. The `.lock` file persists after exit by design; do not delete it while a sync is running. Cursors are separated by server URL **and device ID**; legacy URL-only cursor files are ignored, causing safe idempotent replay on upgrade. This does not lock unrelated Git CLI operations or provide transactional merge across simultaneously edited Git worktrees. Those compatibility checks remain pending.

## Development 050: relay generation and restore resilience

`GET /v1/events` now includes `generation` (persistent relay installation
identity) and `tip` (latest event cursor), along with `events` and `next_cursor`.
The CLI checkpoints a JSON `{generation,cursor}` pair per server/device. On an
identity change or cursor rollback, it replays from cursor zero and stages
records idempotently using the same Git-compatible immutable history. Numeric
checkpoints from older versions automatically replay from zero. This protects
against silently retaining an invalid cursor following replacement or restore.
A *same-generation* database backup restored before newer published events
cannot recover those absent events without originating clients republishing
artifacts; keep all device histories backed up. For restore, the relay generation
is retained as part of its database backup; if the new tip is below the client
checkpoint, the client detects the rollback.

## Development 051: isolated two-device integration scenario

Run after building the workspace: `python3 deploy/server/smoke-two-clients.py`.
The script starts a temporary loopback `orbit-server`, enrolls two ephemeral
identities, initializes two independent local SQLite/Git-compatible workspaces,
creates task changes on both devices (including a disconnected edit), checks
propagation, restarts the relay, and checks repeated exchange idempotency.
It does not contact an existing deployment or modify your personal databases.
The script is an **unexecuted acceptance harness** until binaries are available;
do not infer that dual-device runtime behavior has been validated from its presence.

Remaining M16 gaps: conflict-resolution and true Git-remote interoperability
scenarios, automated backup/restore smoke, HTTPS deployment verification,
and full runtime acceptance in M15.

## Development 052: disposable restore scenario and HTTPS option

The retired relay smoke `deploy/server/smoke-two-clients.py` is no longer part of acceptance. Its replacement, `deploy/server/smoke-server-durability.py`, exercises canonical two-device convergence, token rotation, revocation, backup/restore, fail-closed rewind handling, and restart durability against temporary databases. Opt-in HTTPS ingress is in `deploy/server/compose.tls.yaml`, `Caddyfile`, and `README-TLS.md`. Use private networking where practical and protect credentials. Live TLS ingress and browser deployment remain separate release gates.

### Development 055: device credential rotation

An administrator can rotate a currently active device credential without changing
its device identity or immutable semantic sequence namespace:

```sh
curl -i -X POST http://127.0.0.1:8787/v1/devices/my-desktop/token \
  -H "Authorization: Bearer $ORBIT_SERVER_ADMIN_KEY" \
  -H 'Content-Type: application/json' \
  -d '{"token":"REPLACE_WITH_NEW_RANDOM_SECRET_AT_LEAST_32_CHARACTERS"}'
```

`204 No Content` means the device's hash was replaced atomically. The old
credential stops authenticating immediately; clients must update their local
`ORBIT_SYNC_HTTP_TOKEN` independently. Unknown/revoked devices receive 404,
malformed or short credentials receive 400, and the admin bearer token is
required. This endpoint does not revive revoked devices, regenerate source
sequences, or delete historical events. Rotate over TLS/private-network ingress,
not public plaintext HTTP. A secret replacement cannot be retrieved later.

This source implementation is not runtime-accepted; deployment and auth smoke
remain deferred to M15 as requested.

## M17 opt-in durable automation dispatch (development 057)

M17 introduces a **single-user, server-owned scheduler and durable leased run queue**.
Jobs have a stable ASCII ID, an allowlisted action (`project-health-refresh`,
`daily-briefing`, `habit-summary`), an interval from 60 seconds to 365 days,
and an enabled flag. The scheduler ticks at 15-second intervals. Every due
interval produces at most one durable run per job per tick, even after restart;
missed intervals are coalesced rather than causing a catch-up storm.

Only the configured **administrator token** may create/update/list/disable jobs
or inspect runs:

- `POST /v1/automations` with `{ "id":"morning", "action":"daily-briefing", "interval_seconds":86400, "enabled":true }`
- `GET /v1/automations`
- `DELETE /v1/automations/{id}` (disables a schedule; preserves run history)
- `GET /v1/automation-runs` (last 100 durable runs)

Only **enrolled, nonrevoked devices** may acquire and settle runs; requests
must contain the same device token and `x-orbit-device-id` as event sync:

- `POST /v1/automation-runs/claim` → JSON run object or `null` if none due
- `POST /v1/automation-runs/{id}/settle` with `{ "succeeded":true }` → 204

Leases expire after 300 seconds; an expired lease can be claimed by another
worker. Completion requires the *same worker identity* during its live lease.
The run's numeric `id` is the durable **idempotency key** for the eventual
executor; because a worker can crash after performing an action but before
settlement, exactly-once **execution is not guaranteed**. Failed runs remain
recorded for audit; retries require an explicit future retry policy rather
than silently executing an unbounded number of times.

**Critical scope boundary:** This release implements scheduling and authenticated
**dispatch**, not application-level execution, remote MCP, browser access or
provider-specific AI. No user task/project content is sent to a cloud service,
no arbitrary shell commands can be defined in a job, and no untrusted jobs are
executed by the server. A later M17 worker adapter must enforce per-action
scopes, approvals and output/audit semantics before such actions can execute.
The server is still optional, and no local client workflow depends on it.

The automation tables share `ORBIT_SERVER_DB` with the relay. Existing server
backup/restore commands therefore capture committed automation state too.

## Development 059 — authenticated remote observability MCP + web console

`POST /mcp` implements JSON-RPC 2.0 (`initialize`, `ping`, `tools/list`, `tools/call`). Canonical MCP tools are `orbit_get_workspace`, `orbit_list_projects`, `orbit_list_tasks`, `orbit_get_evidence`, `orbit_create_project`, `orbit_task_command`, and `orbit_evidence_command`; automation schedules and run-history are also readable. Mutating tools require an enrolled device bearer token and matching `x-orbit-device-id`. Admin-only credentials permit read operations, **not** domain mutations. MCP commands use the same authoritative SQLite transaction and domain validation as REST, with `command_id` and `expected_revision` required. Tool-level errors return `isError=true`; conflicting intentions require an explicit refresh and review. Browser `Origin` requests are forbidden. Full streamable-HTTP session interoperability remains subject to runtime acceptance tests.

Example (administrator):

```sh
curl -fsS -X POST http://127.0.0.1:8787/mcp \
  -H "Authorization: Bearer $ORBIT_SERVER_ADMIN_KEY" \
  -H 'Content-Type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}'
```

`GET /console` serves an optional no-build browser administration page for listing schedules and recent report runs, creating/updating allowlisted schedules and disabling schedules. It uses the preexisting administrator-only REST endpoints. The user enters an administrator credential into the page; it remains in JavaScript memory only and is not stored in cookies or localStorage. **Never use the console on an untrusted host, browser extension environment or plaintext public network.** Prefer localhost/private network or HTTPS. Run JSON results may contain sensitive local report data. Browser console is an *automation operations page*, not a full Orbit projects/tasks/habits web client.

Permissions for remote **mutating** AI agents, explicit review/approval records and canonical project browser application services remain M17 work. Rust runtime compatibility and live MCP clients/console acceptance remain deferred to M15 by user request.

### Browser report dashboard (development 062)

`GET /console` provides read-only sections for the most recently successful
`daily-briefing`, `project-health-refresh`, and `habit-summary` automation
outputs. The browser retrieves those via the already authenticated
`GET /v1/automation-runs` endpoint after an administrator supplies a token.
The server does not expose a second domain-data store or live database access.
No report is fabricated when a worker has not produced one. The console uses
`textContent` and constructed DOM nodes for untrusted report data; no HTML
interpolation is used. Outputs are bounded and may be stale. Do not expose
this personal-data console without HTTPS or a private authenticated network.

## Development 063: administrator console task-status submission

`POST /v1/admin/agent-proposals` requires the administrator bearer token and accepts the same proposal document as the device-scoped `POST /v1/agent-proposals`. The server attributes it to `admin-console` and **always persists it as pending**. The administrator must separately approve it via the existing decision API before an enrolled worker can claim it. The console's task-status form submits only `task-update` with `{"status":"done"}`-style payloads. The server rejects extra keys and unsupported statuses for all `task-update` proposals, even for enrolled devices. The worker itself still enforces the same payload grammar. Browsing and proposing do not directly mutate client databases; an opt-in worker with a local database is required. Leased tasks that lose acknowledgement are held for manual reconciliation; do not assume exactly-once semantics.


### Development 064 — approved project task creation

The optional browser console can now submit a project-scoped task creation proposal. The server validates a strictly bounded `{"title": ...}` payload, preserves the existing administrator approval boundary, and makes approved work available to the opt-in local agent worker. The worker uses the durable server proposal ID to select a stable local task ID (`remote-proposal-<id>`), queries for an already-applied task, and only treats an exact title/project match as successful reconciliation. The CLI `task add` command now accepts `--id` for this purpose; interactive users may omit it. This is scoped to a *single configured relay and local worker database*; do not merge multiple unrelated relays using the same local task namespace. Task creation writes through the existing Orbit application command and semantic outbox. No shell execution is permitted.

Habit and metric mutations remain proposal-only, and browser data is still report snapshots rather than a live canonical UI. A crash after local commit but before server acknowledgement leaves a leased proposal for operator reconciliation; replaying it manually can safely recognize the same task. End-to-end Rust execution, browser tests, and other release acceptance are deferred to M15 by product decision.


### Development 065 — approved habit and metric execution (source pending Rust acceptance)

The optional browser can propose habit completions (explicit `at_ms`) and metric observations (explicit `at_ms` and signed 64-bit value), which remain pending until administrator approval. The enrolled worker runs fixed local Orbit commands. Habit occurrences use stable proposal-specific IDs and reconcile against history before writing; metric observations reuse the transactional command that treats identical timestamp/value retries as no-ops and rejects conflicting timestamps. Unknown or malformed payloads fail closed. A worker crash after commit before settlement still requires operator reconciliation because leases are not retried automatically. Full server acceptance remains deferred to M15.

## Server-owned semantic projection (development 082)

`GET /v1/state` requires the same enrolled-device bearer credential and `x-orbit-device-id` as event pull. It exposes a deterministic semantic snapshot and explicit `complete`, `pending_events`, and `unresolved_conflicts` indicators. This endpoint is read-only; it does **not** make `/v1/events` a server-validated command API. See `docs/server-authoritative-migration.md`.

### Orbit 087: transactional authoritative task path (migration preview)

`GET /v1/authoritative/tasks` and `POST /v1/authoritative/task-commands` now use one server SQLite transaction for task state, command idempotency, semantic event emission, revision assignment, and accepted projection. The command body is the existing `TaskCommand` shape from the domain preview API; authentication uses an enrolled device token. Legacy relay and domain-preview writes are fenced once this mode has begun. Starting this mode on populated legacy relay history is refused until an explicit import/cutover exists.

**Not a complete authoritative server:** projects, habits, metrics, local-device pending commands, browser writes, worker mutation rerouting, and legacy migration are not yet wired to this path. `migration-status.unified_authority_ready` remains false. Clients must not switch automatically. Runtime Rust tests are outstanding.
