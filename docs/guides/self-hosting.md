# Optional self-hosted Orbit authority

Orbit works without any server. Connected deployment is an opt-in mode where **server SQLite is canonical** and desktop/mobile SQLite databases retain accepted snapshots and pending commands.

## Components

- `orbit-server`: Rust authoritative HTTP server.
- `orbit`: CLI connected client.
- Optional `deploy/server/` worker processes for approved proposals, automation and Herdr execution. These workers currently use Python; **Python is not a dependency of the local CLI**.
- TLS reverse proxy for non-loopback endpoints; deployment examples live in `deploy/server/`.

Install server binary via `./scripts/install.sh --components server` and follow `deploy/server/README-TLS.md` and `docs/server-api.md` for configuration. Provision server database, credentials, networking, backups and TLS before connecting clients.

Never expose a plaintext non-loopback authority URL or place secrets in CLI arguments, documentation samples, or test logs. Keep production databases out of M15 test fixtures.

A prebuilt server installation does not automatically deploy/configure a service. Container builds and platform-specific service management need their own deployment acceptance under M15.

## Compose deployment helper

A noninteractive helper prepares the optional loopback-bound server without downloading code or touching a production database:

```sh
./scripts/setup-server.sh --init   # generate private deploy/server/.env; no service started
./scripts/setup-server.sh --check  # verify Docker Compose and environment
./scripts/setup-server.sh --up     # build/start using existing Compose definition
./scripts/setup-server.sh --down   # stop, preserving the named data volume
```

`--init` never overwrites an existing environment file. The generated admin key is not printed. For an alternative location use `--env-file /secure/path/server.env`. Docker Engine + Compose v2 are required only for check/up/down, not for local Orbit.

The sample Compose file binds port 8787 to **127.0.0.1**, not the public network. Protect remote clients with TLS termination and separately managed credentials; see `deploy/server/README-TLS.md`. Treat database backups and volume recovery as operational requirements before storing important data.

The helper does not automatically enroll devices, install Herdr, configure a system service, or establish remote TLS; these steps depend on your deployment.
