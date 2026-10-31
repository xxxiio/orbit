# Optional self-hosted Orbit authority

Orbit works without any server. Connected deployment is an opt-in mode where
**server SQLite is canonical** and desktop clients keep an accepted snapshot plus
a durable pending-command outbox.

## Components

- `orbit-server`: the authenticated authoritative HTTP server.
- `orbit`: the CLI connected client.
- Optional workers under `deploy/server/` for approved proposals, automation and
  Herdr execution. These workers currently use Python; **Python is not a
  dependency of the local CLI**.
- A TLS reverse proxy for every non-loopback endpoint. Examples live under
  `deploy/server/`.

Install the server binary with:

```sh
./scripts/install.sh --components server
```

Then follow `deploy/server/README-TLS.md`, [the server API](../server-api.md) and
your deployment's enrollment procedure. Provision the database, credentials,
network policy, backups and TLS before connecting clients.

Never expose a plaintext non-loopback authority URL or put secrets in CLI
arguments, documentation samples or logs. Keep production databases out of test
fixtures.

## Compose helper

A noninteractive helper prepares the optional loopback-bound server:

```sh
./scripts/setup-server.sh --init   # generate private deploy/server/.env; starts nothing
./scripts/setup-server.sh --check  # verify Docker Compose and the environment
./scripts/setup-server.sh --up     # build/start using the existing Compose definition
./scripts/setup-server.sh --down   # stop, preserving the named data volume
```

`--init` never overwrites an existing environment file and does not print the
generated admin key. Use `--env-file /secure/path/server.env` for another
location. Docker Engine and Compose v2 are required only for check/up/down.

The sample Compose file binds port 8787 to `127.0.0.1`, not the public network.
Protect remote clients with TLS termination and separately managed credentials.
Treat database backups and volume recovery as operational requirements before
storing important data.

## Authority and client behavior

- Server SQLite is authoritative. A client replica is not.
- Pending commands are **not accepted** until the authority acknowledges them.
- `orbit sync replica` manages the offline cache and outbox;
  `orbit sync authority` performs connectivity-required commands.
- Agent launches are authorized externally and fenced by machine workers; the
  server does not run arbitrary shell commands.
- TLS is required for non-loopback endpoints. Loopback cleartext is for
  development only.
- `ORBIT_AUTH_MODE=local-trusted` is a browser-only exception for a loopback-only personal
  installation. It is accepted only with a literal loopback `ORBIT_SERVER_BIND`, refused on every
  non-loopback bind, and must not be combined with a reverse proxy. It does not change
  device-authentication trust for remotely reachable deployments; those keep device enrollment
  and the TLS requirement above.

## Backup and upgrade

The server workspace is the same three resources as a local one: the server
SQLite database, its Markdown vault and its attachment object store. Back it up
as one verifiable archive **on the server host**:

```sh
orbit backup create --db /var/lib/orbit/server.db --source-mode server \
  --label "pre-upgrade"
orbit backup list --output /var/lib/orbit/backups
orbit backup verify /var/lib/orbit/backups/NEWEST.orbitbak
```

Use `--output` (or the server's `ORBIT_BACKUP_DIR`) to keep archives on a
different physical device from the database. Credentials and enrollment
material are separate secrets and are never part of the archive. Client replica
snapshots are not a supported recovery point in this build: back up the server
authority, and keep each replica's accepted snapshot, unresolved outbox and
`attachment-staging/` directory with that replica.

A `--source-mode server` archive records `source.excluded_client_pending_data:
true` and a `client_pending_data` exclusion: unaccepted commands and staged
uploads on disconnected clients are not in the server store and are never
pretended to be backed up.

Server-side configuration:

| Setting | Default | Purpose |
| --- | --- | --- |
| `ORBIT_BACKUP_DIR` | `<server database parent>/backups` | Directory scanned for `.orbitbak` archives and the schedule/retention `policy.json`. |
| `ORBIT_BACKUP_RESTORE_ROOT` | `<server database parent>/restores` | Root under which isolated restores create new workspace copies. |
| `ORBIT_BACKUP_DOWNLOAD_MAX_BYTES` | 512 MiB | Ceiling for the in-memory archive download response; above it the API answers `413 Archive too large for a browser download`. Large archives should be copied from the server's backup folder. |

The replacement plan is the only administrator-credential backup route. It does
**not** replace the live database from the browser: it writes
`<db>.restore-pending.json` and returns the offline maintenance commands. Carry
out a replacement from the server shell with every writer stopped, after
verifying the archive; the replacement stages and verifies first and preserves
the previous workspace plus a recovery journal for rollback. Keep the preserved
copy until the restored workspace is accepted.

Automatic schedules and retention live in `policy.json` in the backup directory.
The CLI (`orbit backup schedule set|show`, `orbit backup retention apply`) and
the Web GUI edit the same file. The running server spawns the backup scheduler:
it re-reads `policy.json` every 15 minutes (first check about 30 seconds after
startup) and creates a verified `automatic-<schedule>` archive when a
`daily`/`weekly` backup is due, without duplicating a daily backup within the
same UTC day, then applies retention while protecting the new archive. A policy
change takes effect without a restart. `disabled` and `before-upgrade` never run
on a timer; `before-upgrade` is covered by `orbit update`, which creates a
complete verified recovery point and fails closed if it cannot. Cleanup runs only
after a verified creation and never removes the last verified recovery point or
a user-managed external copy.

A restore does not include pending commands or staged uploads held by
disconnected clients. Preserve each client replica and its outbox, and reconcile
through the supported convergence tools after an authority restore. An older
server snapshot can lose newer accepted history. See
[backup and recovery](backup-and-recovery.md) and
[upgrades and recovery](../releasing/upgrades-and-recovery.md).

## What this helper does not do

It does not enroll devices, install Herdr, configure a system service, establish
remote TLS or publish a release. Those steps depend on your deployment. Live TLS
ingress and browser deployment remain separate verification gates.

Next: [connected mode onboarding](../getting-started/connected-mode.md) or
[worker deployment](worker-deployment.md).
