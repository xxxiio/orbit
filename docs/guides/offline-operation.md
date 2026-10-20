# Local authority and connected offline operation

## Local-only mode

Local SQLite is authoritative. No server, device credential or synchronization
service is necessary. Select the workspace with `--db`, `ORBIT_DB`,
`[storage].database`, or the default `~/.orbit/orbit.db`.

Do not manually edit SQLite rows. Use the CLI or, where supported, the MCP
surface.

## Connected mode

The self-hosted server's SQLite is the canonical authority. Client SQLite is an
accepted-state replica plus a durable pending-command store. Configure the
server endpoint and replica database together:

```sh
orbit init --authority connected \
  --server https://orbit.example \
  --replica-database "$HOME/.orbit/replica.db"
```

Do not mix a local-authoritative database and a connected replica under one
path. In connected mode, ordinary local mutation commands are not a fallback;
use the replica workflow.

## What "pending" means

A client may enqueue an offline-capable mutation while disconnected. The request
preserves its command identity and remains **pending** until the server accepts
or rejects it.

Pending is **not accepted state**. It is not visible in accepted snapshots and
must not be reported as completed work or successful external execution. A lost
acknowledgment is not permission to generate a new command identity.

Inspect:

```sh
export ORBIT_SYNC_HTTP_TOKEN='<device-token>'
export ORBIT_AUTHORITY_DEVICE='<device-id>'

orbit sync replica --db "$HOME/.orbit/replica.db" \
  --server https://orbit.example status
orbit sync replica --db "$HOME/.orbit/replica.db" \
  --server https://orbit.example pending
orbit sync replica --db "$HOME/.orbit/replica.db" \
  --server https://orbit.example audit
orbit sync replica --db "$HOME/.orbit/replica.db" \
  --server https://orbit.example workspace-sync
```

`status` shows the last accepted revision and pending count. `audit` checks
queue ordering and receipt consistency without network access. `workspace-sync`
reports per-domain queue state and blockers.

Submit:

```sh
orbit sync replica --db REPLICA --server https://orbit.example flush
orbit sync replica --db REPLICA --server https://orbit.example reconnect
```

`flush` submits pending intentions in order, preserving retry identities.
`reconnect` replays, refreshes accepted state and mirrors Markdown notes. Neither
makes a rejected command successful.

Inspect a specific command or resolve a conflict only after review:

```sh
orbit sync replica --db REPLICA --server https://orbit.example \
  receipt --command-id COMMAND_ID
orbit sync replica --db REPLICA --server https://orbit.example \
  review-conflict-online --command-id COMMAND_ID
orbit sync replica --db REPLICA --server https://orbit.example \
  resolve-reviewed --review-file review.json --replacement-id REPLACEMENT_ID \
  --kind task --action-json '{"type":"..."}'
```

Conflict resolution is revision-bound. Do not overwrite accepted server history
silently or delete outbox data to make a conflict disappear.

## Direct authority access

`orbit sync authority` requires connectivity and returns a server receipt
immediately. It requires `--server`, `--device` and `--token`.

```sh
orbit sync authority \
  --server https://orbit.example \
  --device DEVICE_ID \
  --token "$ORBIT_SYNC_HTTP_TOKEN" \
  tasks
```

Use it for deliberate online reads and commands. It is not an offline fallback.

## External effects

Agent launch requests, continuation messages and other external actions are
never executed locally by a disconnected connected-mode client. The server
authorizes them and machine workers fence execution.

If delivery is uncertain, treat the state as requiring reconciliation:

- inspect the durable job/command and its receipt;
- check the machine-side Herdr session independently;
- resume or continue only through the supported command;
- never launch a second agent because the first acknowledgment was lost.

## Security boundaries

- Non-loopback server URLs must use TLS.
- Keep device tokens in the environment or a secret manager.
- Never commit credentials or paste them into URLs, logs or bug reports.
- A replica contains sensitive accepted data; protect it like the server.
- Pending commands and the Markdown vault must be backed up with the replica.

See [connected mode onboarding](../getting-started/connected-mode.md),
[backup and recovery](backup-and-recovery.md), and
[server API](../server-api.md).
