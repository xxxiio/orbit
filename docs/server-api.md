# Orbit server API

The optional `orbit-server` is an authenticated, single-user authority for
connected Orbit deployments. In connected mode **server SQLite is canonical**;
client SQLite is an accepted-state replica plus a durable outbox.

Status: source-implemented. Linux x86_64 loopback connected workflows are the
only runtime-verified scope in this repository. TLS ingress, multi-device
production operation and remote machine execution remain separate gates. This
page does not claim that a public server deployment has been certified.

## Authority model

- Accepted project, task, evidence, schedule, routine, blueprint and Markdown
  state belongs to the server.
- Clients submit commands with a stable command identity and expected revision.
- A request that is merely queued on a client is **not accepted** state.
- Conflicts are reviewed against the accepted server history; the server does
  not silently take last-write-wins.
- External effects (agent launch, continuation) are authorized by the authority
  and fenced by machine workers. The server does not run arbitrary shell
  commands.

## Authentication

Device requests use an enrolled device identity and token together:

```text
Authorization: Bearer <device-token>
x-orbit-device-id: <device-id>
```

Missing either half is an error. Administrator operations use the deployment's
admin credential. Tokens are stored as hashes where applicable and must never be
placed in URLs, committed files, logs or bug reports.

Use TLS for every non-loopback endpoint. Loopback cleartext HTTP is acceptable
only for local development.

## Endpoints

The exact route set depends on the server version. The maintained client
surface is:

| Area | Routes / behavior |
| --- | --- |
| Liveness | `GET /health` is an unauthenticated liveness check, not a readiness or sync proof |
| Devices | Enrollment, revocation and credential rotation under `/v1/devices` |
| Authoritative tasks | `GET /v1/authoritative/tasks`, `POST /v1/authoritative/task-commands` |
| Authoritative state | Snapshot/read routes for the accepted domain families used by `sync replica` |
| Domain commands | Typed lifecycle commands with command ID and expected revision |
| MCP | Optional JSON-RPC endpoint for configured MCP clients |
| Automations | Optional administrator-owned schedules and run history |
| Console | Optional read-only/administrative browser page on supported deployments |

A mutating request is idempotent by command identity. Reusing an identity with
different content is rejected. Expected-revision mismatches require a refresh
and review rather than a blind retry.

## Client commands

Connected clients should use the maintained CLI:

```sh
orbit sync replica --db REPLICA --server https://orbit.example status
orbit sync replica --db REPLICA --server https://orbit.example snapshot
orbit sync replica --db REPLICA --server https://orbit.example pending
orbit sync replica --db REPLICA --server https://orbit.example flush
orbit sync replica --db REPLICA --server https://orbit.example reconnect
```

Connectivity-required commands:

```sh
orbit sync authority --server https://orbit.example \
  --device DEVICE_ID --token "$ORBIT_SYNC_HTTP_TOKEN" tasks
orbit sync authority --server https://orbit.example \
  --device DEVICE_ID --token "$ORBIT_SYNC_HTTP_TOKEN" snapshot
```

See the [CLI reference](reference/cli.md) for subcommands and flags, and
[connected mode](getting-started/connected-mode.md) for onboarding.

## Backup and restore

Back up the server database consistently, including committed WAL data, and
store credentials separately. A server backup does not include pending edits on
disconnected clients. Do not restore an older snapshot over newer accepted
history without an explicit disaster-recovery plan.

Clients must preserve replicas and unresolved outboxes. Use reviewed
convergence/conflict tools; never delete outbox data to make a warning disappear.
See [backup and recovery](guides/backup-and-recovery.md) and
[upgrades and recovery](releasing/upgrades-and-recovery.md).

## Retired interfaces

Early Orbit releases used an append-only `/v1/events` relay and a retired
initialization/HTTP push-pull sync workflow. Those commands and that relay are
not part of the current CLI or the supported connected contract. They are
preserved only in historical milestone notes. Use `orbit sync replica` and
`orbit sync authority` instead.

Likewise, the old `docs/server-authoritative-migration.md` design note is not a
current document. Current authority behavior is described here and in the
guides linked above.

## Related

- [Connected mode](getting-started/connected-mode.md)
- [Self-hosting](guides/self-hosting.md)
- [Offline operation](guides/offline-operation.md)
- [Worker deployment](guides/worker-deployment.md)
- [MCP reference](reference/mcp.md)
