# Troubleshooting Orbit

Start with the read-only diagnostic:

```sh
orbit doctor
orbit doctor --strict
orbit status
```

`doctor` checks the executable, PATH resolution, configuration, database
integrity/schema, authority mode and optional tools. Without `--strict` it
reports and exits `0`; with `--strict`, errors exit `1`. It never prints token
values.

## Installation

### `orbit: command not found`

Add the install prefix to `PATH`:

```sh
export PATH="$HOME/.local/bin:$PATH"
command -v orbit
```

The installer deliberately does not edit shell startup files. Put the export in
your shell profile yourself.

### Checksum verification failed

Download the archive and its `.sha256` again from a trusted channel. Do not
bypass verification. Same-origin checksums detect corruption in transit; they do
not prove publisher authenticity.

### Unsupported target

Only `x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu`,
`x86_64-apple-darwin` and `aarch64-apple-darwin` are declared. Windows is
unsupported and there is no native Windows installer.

### Installer refuses to overwrite a binary

The installer only manages binaries recorded in its own manifest. Move or
uninstall the unrelated binary explicitly; do not force an overwrite.

### `orbit update` is missing or refuses to run

- If `orbit --help` does not list `update`, the installed build predates the
  command. Re-run the installer with `--version` to install a newer release.
- Confirm the installation has the M26 manifest. Update refuses an installation
  it cannot manage.
- Use `--check` to inspect without mutating.
- Downgrades require `--allow-downgrade`.

See [installation](../getting-started/installation.md) and
[upgrades and recovery](../releasing/upgrades-and-recovery.md).

## Initialization and configuration

### `unsupported configuration version`

The config was written by a newer build. Use a compatible binary or restore a
backup. Do not edit the version by hand.

### `invalid configuration ...; fix the file or run orbit init --force`

Correct the TOML or rewrite it deliberately with `--force`. Unknown keys fail
closed. A malformed config is not silently replaced with defaults.

### `configuration already exists; pass --force`

`orbit init` retained the existing file and refused conflicting flags. Re-run
without flags, or back up and use `--force`.

### `connected authority requires [connected] settings`

Initialize both fields:

```sh
orbit init --authority connected \
  --server https://orbit.example \
  --replica-database "$HOME/.orbit/replica.db"
```

Partial connected settings fail closed.

## Workspace and database

### Work seems to have disappeared

The command probably selected a different database:

```sh
orbit status
echo "$ORBIT_DB"
```

Check `--db`, `ORBIT_DB`, `[storage].database` and the default
`~/.orbit/orbit.db`. See
[switching projects and workspaces](switching-projects-and-workspaces.md).

### `database is locked` or integrity errors

Stop writers and use SQLite's backup/recovery procedure. Do not delete `-wal`
or `-shm` files from a live database. Run `orbit doctor` and
`sqlite3 DB "PRAGMA integrity_check;"`. See
[backup and recovery](backup-and-recovery.md).

### Where is my data?

- database: `~/.orbit/orbit.db` by default, or the configured path;
- vault: `<database>.knowledge/`;
- config: `~/.orbit/config.toml`;
- update backups: `~/.orbit/backups/`.

There is no dedicated backup subcommand. Use the documented SQLite and vault
procedure.

## Projects, tasks and planning

### Unknown ID

List entities and copy the stable ID:

```sh
orbit project list
orbit task list
```

IDs are not titles. `--project` is a filter/association, not a global switch.

### Task is blocked

```sh
orbit task ready --now-ms 1792480200000 --all
orbit task graph --now-ms 1792480200000 --all
```

Complete or unblock the prerequisite. Do not force status to skip the domain
rules.

### Empty `next` or `day-plan`

- Confirm `--now-ms` is Unix **milliseconds**.
- Confirm the task has no unfinished prerequisites.
- Supply capacity with `--available-minutes`.
- Check planning metadata with `orbit task planning TASK_ID --help`.

### No priority field or milestone commands

This build has no core task priority field. Rank with deadlines, readiness and
impact. There is no standalone milestone CRUD command; `task add --milestone`
only references an existing milestone. Model goals as tasks, criteria, notes or
blueprint items.

## Notes and folders

### `markdown publication failed`

Create the parent folder first:

```sh
orbit folder create docs
orbit note create docs/design.md --id note-design --body "# Design"
```

Also check that the path stays inside the vault and does not already exist.

### Note not found

Use the stable note ID, not the filename:

```sh
orbit note list
orbit note read NOTE_ID
```

### Stale note update

Another writer changed the file. Re-read it and retry with `--file`; do not
overwrite blindly.

## Agents

### Agent will not start

- Confirm the task exists and the repository is an absolute path.
- Confirm Herdr is configured (`orbit doctor`).
- Check whether an active job already exists for the task.
- Read `orbit agent start --help` for current flags.

### Launch result is uncertain

Unreachable does not mean terminated. Inspect the durable job and the
machine-side Herdr session. Do not relaunch automatically. Use
`resume-context`/`resume` when the predecessor is terminal.

### Agent finished but the task is still open

Expected. Review evidence and criteria, then run:

```sh
orbit task complete TASK_ID
```

Session or review approval never completes the task automatically.

## Connected mode

### Pending never becomes accepted

Pending is **not accepted** state. Check connectivity, credentials and
conflicts:

```sh
orbit sync replica --db REPLICA --server https://orbit.example pending
orbit sync replica --db REPLICA --server https://orbit.example receipt --command-id COMMAND_ID
```

Do not issue a new command ID after a timeout.

### Authentication or TLS failure

Device ID and token must be supplied together, and non-loopback endpoints
require TLS. Never print the token in logs or bug reports.

### Self-hosted server will not start

```sh
./scripts/setup-server.sh --check
```

`--init` creates a private environment file but does not start Docker; `--up`
starts a loopback-bound service. Configure TLS ingress for remote clients. Do
not paste the admin key into reports.

## MCP

A missing tool may mean the installed mode does not expose it. Call `tools/list`.
A denied mutation may mean `read_only` mode, a project restriction, a stale
revision, offline state or insufficient authority. See
[MCP integration](mcp-integration.md) and [MCP reference](../reference/mcp.md).

## Retired or unavailable features

The following are **not** working commands or features in this build:

- a knowledge command group (use `orbit note` and `orbit folder`);
- sync init/http/push/pull relay commands (use `orbit sync replica` and
  `orbit sync authority`);
- a project-switch command (select a workspace with `--db`/`ORBIT_DB`);
- milestone CRUD, task priority and a dedicated backup command;
- executable blueprint capability actions.

If a guide or older milestone note shows one of these, treat the current
`orbit --help` as authoritative.

## Preserve evidence for release testing

Run maintained checks in a disposable environment and retain logs with the
tested version and target. Never use production databases as fixtures. See
[platforms and components](../releasing/platforms-and-components.md).
