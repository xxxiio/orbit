# Connected mode onboarding

Connected Orbit requires a separately installed and authenticated self-hosted
server. **The server's SQLite is authoritative.** Client SQLite is an
accepted-state replica plus a durable pending-command outbox. An offline
mutation is **not accepted** merely because it was queued.

Connected mode is optional. Local mode needs no server and no device
credentials.

## 1. Deploy and enroll

1. Install the optional server component. Follow
   [self-hosting](../guides/self-hosting.md) and the deployment notes under
   `deploy/server/`.
2. Configure TLS for every non-loopback endpoint. Cleartext HTTP is acceptable
   only on loopback for development.
3. Enroll a device through the deployment's administrative enrollment path.
   Device identity and token are a pair; missing one is an error.
4. Keep the token in the environment or a service manager. Never commit it,
   paste it into a URL, or include it in a bug report.

## 2. Initialize the client

```sh
orbit init --authority connected \
  --server https://orbit.example \
  --replica-database "$HOME/.orbit/replica.db"
```

Initialization writes the complete `[connected]` section. Incomplete settings
fail closed. It does not contact the server or enroll the device.

`config.toml` then contains:

```toml
version = 1

[mode]
authority = "connected"

[connected]
server = "https://orbit.example"
replica_database = "~/.orbit/replica.db"
```

## 3. Inspect and refresh the replica

Replica commands are nested under `orbit sync replica` and require `--db` and
`--server`. Authenticated operations also require `--device` and `--token`, or
the environment variables `ORBIT_AUTHORITY_DEVICE` and
`ORBIT_SYNC_HTTP_TOKEN`.

```sh
export ORBIT_SYNC_HTTP_TOKEN='<device-token>'
export ORBIT_AUTHORITY_DEVICE='<device-id>'

orbit sync replica \
  --db "$HOME/.orbit/replica.db" \
  --server https://orbit.example \
  status
```

Useful read-only commands:

```sh
orbit sync replica --db REPLICA --server https://orbit.example snapshot
orbit sync replica --db REPLICA --server https://orbit.example refresh
orbit sync replica --db REPLICA --server https://orbit.example pending
orbit sync replica --db REPLICA --server https://orbit.example audit
orbit sync replica --db REPLICA --server https://orbit.example workspace-sync
```

`reconnect` replays pending operations, refreshes accepted state and mirrors
Markdown notes. It does not turn a rejected operation into an accepted one.

```sh
orbit sync replica --db REPLICA --server https://orbit.example reconnect
```

Representative status fields include the last accepted revision and the number
of pending commands. A pending command is durable, but it is **not** accepted
state and must not be treated as success.

## 4. Queue and flush work

The replica accepts validated JSON actions:

```sh
orbit sync replica --db REPLICA --server https://orbit.example \
  queue --command-id COMMAND_ID --kind task --action-json '{"type":"..."}'
```

Use the command contract for your server version; do not fabricate action
payloads. Later:

```sh
orbit sync replica --db REPLICA --server https://orbit.example flush
orbit sync replica --db REPLICA --server https://orbit.example pending
```

Pending commands preserve their original identity. A timeout does not
authorize issuing a replacement ID. Inspect receipts and conflicts instead:

```sh
orbit sync replica --db REPLICA --server https://orbit.example \
  receipt --command-id COMMAND_ID
orbit sync replica --db REPLICA --server https://orbit.example \
  review-conflict-online --command-id COMMAND_ID
```

Conflict review is a two-step, revision-bound workflow. Resolve only after
reviewing the exact accepted history; never overwrite accepted server history
silently.

## 5. Direct authority commands

For connectivity-required operations, `orbit sync authority` talks to the
server directly and returns a receipt. It requires `--server`, `--device` and
`--token`.

```sh
orbit sync authority \
  --server https://orbit.example \
  --device DEVICE_ID \
  --token "$ORBIT_SYNC_HTTP_TOKEN" \
  tasks
```

Available subcommands include `snapshot`, `domain-command`, `domain-read`,
`tasks`, `projects`, `project-create`, `create` and `status`. These are not a
substitute for the replica queue when offline.

## 6. Failure and offline expectations

- Pending requests remain pending until acknowledged.
- Failed or conflicting requests require user attention; they are not silently
  retried forever.
- Running `reconnect` refreshes accepted snapshots but does not make rejected
  actions successful.
- Agent launch and continuation are privileged external effects. A disconnected
  client never executes them merely because they were queued. Machine workers
  fence actual execution.
- Replica and local-authoritative databases must not share a file or be mixed
  in one workflow.

## Common failures

- **`connected authority requires [connected] settings`** — run `orbit init
  --authority connected` with both server and replica database.
- **Authentication error** — confirm device ID and token are supplied together
  and are still valid.
- **TLS error** — non-loopback servers must present a trusted certificate.
- **Unexpected stale data** — run `snapshot` and `pending`; the replica shows
  the last accepted revision, not an optimistic local edit.
- **Conflict** — review the server-observed history before resolving.

Read [offline operation](../guides/offline-operation.md) for the authority
model and [server API](../server-api.md) for endpoint contracts. Start with
[local quickstart](quickstart.md) if you do not need a server.
