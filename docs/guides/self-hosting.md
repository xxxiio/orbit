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

## Backup and upgrade

Back up the server database consistently, including committed WAL data, and keep
credentials separately. Preserve client replicas and unresolved outboxes.
Restoring an older server snapshot can lose newer accepted history. See
[backup and recovery](backup-and-recovery.md) and
[upgrades and recovery](../releasing/upgrades-and-recovery.md).

## What this helper does not do

It does not enroll devices, install Herdr, configure a system service, establish
remote TLS or publish a release. Those steps depend on your deployment. Live TLS
ingress and browser deployment remain separate verification gates.

Next: [connected mode onboarding](../getting-started/connected-mode.md) or
[worker deployment](worker-deployment.md).
