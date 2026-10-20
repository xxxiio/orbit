# Troubleshooting Orbit

## `orbit: command not found`

Check the install prefix (`~/.local` by default), then confirm `~/.local/bin` is on `PATH`. Installation deliberately does not edit shell startup files.

## Installer refuses to overwrite an executable

The installer only manages binaries listed in its own installation manifest. Move or uninstall the preexisting binary explicitly; never force an overwrite of an unrelated executable.

## Missing `cargo` or Rust dependencies

Build from source with a Rust toolchain, or install trusted prebuilt binaries with `./scripts/install.sh --from-dir /path/to/bin`. Orbit's ordinary local CLI does not require Python, Herdr, or a server.

## Blueprint command says pending or blocked

In connected mode, changes are not canonical until the authoritative server accepts them. Inspect `orbit blueprint pending`, verify server connectivity and device configuration, then use `orbit blueprint flush`. Earlier conflicts may block newer commands in the same ordered outbox.

## Agent is unreachable or its launch is uncertain

Unreachable does not mean terminated. Do not relaunch automatically. Inspect the durable Orbit job and the machine-side Herdr session, reconcile any ambiguous claim, then resume through the supported command. A completed agent session does not mark its Orbit task complete.

## Self-hosted server will not start

Run `./scripts/setup-server.sh --check` to validate the optional Compose deployment. `--init` creates a private `.env` file but does not start Docker; `--up` starts a loopback-bound service. For remote clients, configure a TLS ingress. Do not paste the server admin key into bug reports or logs.

## Preserve evidence for M15

Run `./scripts/m15-full-automated.sh --profile full` in a disposable test environment and retain the evidence archive with the tested Git commit ID. Never use production SQLite databases as acceptance fixtures.

## Local installation diagnosis

Run `./scripts/orbit-env.sh --doctor` with the same `--prefix` used for installation. It prints the resolved Orbit executable and database location, identifies a missing installation manifest or executable, and distinguishes missing optional tools from a broken local installation. It does not modify a database or contact the server. For database corruption, use the M15 SQLite validation/recovery procedures rather than treating a successful doctor check as proof of database integrity.
