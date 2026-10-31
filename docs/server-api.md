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

`ORBIT_AUTH_MODE` selects the browser policy:

- Unset or `authenticated` (the default): device authentication. A browser exchanges an enrolled
  device ID and token once for an HttpOnly, SameSite=Strict session cookie.
- `local-trusted`: an opt-in mode for a loopback-only personal installation. There is no Device
  ID, device token, login screen or session cookie.

Local-trusted refuses to start unless `ORBIT_SERVER_BIND` is a literal loopback address
(`127.0.0.0/8` or `[::1]`). `0.0.0.0`, `[::]`, LAN addresses, IPv4-mapped IPv6 such as
`[::ffff:127.0.0.1]`, `localhost`, and any other non-loopback value are rejected before the
database is opened. In this mode browser calls need no bearer credential or cookie, but browser
mutations still require the `x-orbit-csrf: 1` header, cross-site requests are rejected, and
forwarded headers (`X-Forwarded-For`, `X-Real-IP`) never grant access. The server does not treat
the request source address as authorization.

Device requests use an enrolled device identity and token together:

```text
Authorization: Bearer <device-token>
x-orbit-device-id: <device-id>
```

Missing either half is an error. Administrator operations use the deployment's
`ORBIT_SERVER_ADMIN_KEY` in both modes. That key is required in both modes, guards the
administrator endpoints only (device enrollment, revocation and rotation, admin proposals,
automations), and is never sent to the browser. Capability install/authorize/revoke and executor
enrollment are a separate privilege exercised by an enrolled *device* bearer token, not by the
admin key. In local-trusted mode device enrollment returns `403 administrator credential required`
without the admin key, and
capability install/authorize/revoke and executor enrollment return `403` because the browser
principal is not an enrolled device. Tokens are stored as hashes where
applicable and must never be placed in URLs, committed files, logs or bug reports.

`GET /v1/web/session` reports the browser policy and session state:

```json
{"mode":"authenticated","device_id":"browser-laptop","expires_at_ms":1767225600000}
```

`mode` is `authenticated` or `local-trusted`. In authenticated mode `device_id` is the signed-in
device and `expires_at_ms` is the session expiry. In local-trusted mode there is no enrolled
device: `device_id` is the reserved `local-trusted` principal and `expires_at_ms` is `null`:

```json
{"mode":"local-trusted","device_id":"local-trusted","expires_at_ms":null}
```

`POST /v1/web/session` exchanges device credentials for a session cookie in authenticated mode.
In local-trusted mode it returns `409`, because there is no session to create.

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
