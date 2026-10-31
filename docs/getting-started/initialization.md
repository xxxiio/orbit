# Initializing Orbit

`orbit init` creates the local layout, writes a versioned configuration file,
and initializes the SQLite database through the same storage API used by every
other command. It never prompts and never requires a server.

```sh
orbit init
```

Representative output:

```text
Orbit initialized
Config: /home/you/.orbit/config.toml (created)
Authority: local
Database: /home/you/.orbit/orbit.db (created)
Logs: /home/you/.orbit/logs
State: /home/you/.orbit/state
Backups: /home/you/.orbit/backups
Next: orbit task add "My first task"
```

## What gets created

| Path | Purpose |
| --- | --- |
| `~/.orbit/config.toml` | Versioned TOML deployment configuration |
| `~/.orbit/orbit.db` | Authoritative local SQLite database (default) |
| `~/.orbit/orbit.knowledge/` | File-authoritative Markdown vault created by storage |
| `~/.orbit/logs/` | Reserved for local logs |
| `~/.orbit/state/` | Reserved for local state |
| `~/.orbit/backups/` | Default workspace-backup destination and pre-update backups |

Directories are created with user-only permissions on Unix. Init refuses to
follow a symlinked layout directory and refuses a path that exists as a regular
file.

## Configuration file

`orbit init` writes a minimal versioned file:

```toml
version = 1

[mode]
authority = "local"
```

The complete supported schema is:

```toml
version = 1

[storage]
# database = "~/custom/orbit.db"   # optional; omit for ~/.orbit/orbit.db

[mode]
authority = "local"                # or "connected"

[connected]
# server = "https://orbit.example" # required in connected mode
# replica_database = "~/.orbit/replica.db"

[agent]
# default_provider = "codex"
```

Unknown keys are rejected. A file that exists but is invalid fails closed
instead of silently falling back to defaults. An unsupported `version` fails
with an explicit error. Relative paths inside the file resolve against the
directory containing that file; `~/` expands to the current user's home.

See the [configuration reference](../reference/configuration.md) for the full
contract.

## Precedence

Database selection, highest priority first:

1. `--db PATH`
2. `ORBIT_DB`
3. `[storage].database`
4. `~/.orbit/orbit.db`

Configuration file selection:

1. `--config PATH`
2. `ORBIT_CONFIG`
3. `~/.orbit/config.toml`

A custom configuration file location moves only the file. Runtime directories
(`logs`, `state`, `backups`) and the default database stay under `~/.orbit`
when a home directory is available.

## Initialize a connected client

Connected mode needs the server URL and a replica database path. Both are
required; incomplete or partial connected settings fail closed.

```sh
orbit init --authority connected \
  --server https://orbit.example \
  --replica-database "$HOME/.orbit/replica.db"
```

The output includes `Authority: connected` and `Server: https://orbit.example`.
Initialization does not enroll the device, obtain a token or contact the
server. Do that through your deployment's enrollment process, then inspect the
replica with `orbit sync replica status`. See
[connected mode onboarding](connected-mode.md).

A client replica is not a local-authoritative workspace. Never point a
connected replica and a local database at the same file.

## Choose a database at init time

```sh
orbit init --database "$HOME/work/orbit-work.db"
```

The path is recorded in `[storage].database`. Alternatively pass `--db` for a
one-off initialization; the override is used for this run but is not recorded
unless `--database` is supplied. In connected mode the analogous flag is
`--replica-database`.

## Modes

Project modes are configured per project, not globally:

```sh
orbit project configure PROJECT_ID \
  --mode learning \
  --objective "Understand distributed consensus" \
  --signal learning-evidence \
  --criterion notes=Decision notes reviewed
```

| Mode | Typical use | Default progress signals |
| --- | --- | --- |
| `standard` | General execution | milestones, tasks, blockers, forecast |
| `learning` | Study and skill acquisition | milestones, learning evidence, habit consistency, time invested |
| `habit-driven` | Consistency-based work | habit consistency, time invested |
| `metric-target` | Numeric targets | metric value, forecast |
| `maintenance` | Ongoing upkeep | metric value, habit consistency, blockers |
| `custom` | Explicit signal selection | milestones unless `--signal` overrides |

`--signal` is repeatable and accepts `milestones`, `tasks`, `blockers`,
`forecast`, `habit-consistency`, `time-invested`, `learning-evidence`,
`metric-value` and `knowledge-evidence`. Success criteria use repeatable
`--criterion key=description`; optional criteria use
`--optional-criterion key=description`.

## Check the installation

```sh
orbit doctor
```

Doctor is read-only. It reports:

- the running executable and its version;
- whether `orbit` on `PATH` resolves to that executable;
- configuration path, version and validity;
- effective database path, read-only integrity and schema version;
- authority mode, connected server and replica path;
- optional Herdr, Python and sibling binaries.

It never prints token values. Without `--strict`, findings are reported and the
command exits `0`. With `--strict`, any error-severity finding exits `1`, which
is suitable for scripts.

Representative output:

```text
OK: /home/you/.local/bin/orbit (orbit 0.1.0)
OK: /home/you/.orbit/config.toml (version 1)
OK: /home/you/.orbit/orbit.db (schema 19, 46 tables)
OK: SQLite integrity check passed
OK: local authority: no remote server required
INFO: herdr: herdr 0.9.5
INFO: python3: Python 3.12.9
INFO: orbit-mcp: not found next to orbit
INFO: orbit-tui: not found next to orbit
INFO: orbit-server: not found next to orbit
Status: ok
```

Optional tools are informational, not errors. A missing `orbit-mcp` does not
break local CLI use.

## Re-running init on an existing installation

`orbit init` is idempotent:

- An existing configuration is parsed and **retained**.
- The existing database is opened in place, never moved or replaced.
- Conflicting flags (`--database`, `--authority`, `--server`,
  `--replica-database`, `--agent-provider`) are refused without `--force` so
  user intent is not silently ignored.
- `--force` rewrites the configuration atomically. It still does not delete
  data.

```sh
orbit init --force --authority local
```

Back up the database, the Markdown vault and the attachment object store before
changing authority mode or database paths. See
[backup and recovery](../guides/backup-and-recovery.md).

## Common initialization failures

- **Permission denied** — the parent of the chosen database or config path is
  not writable by your user.
- **Unsupported configuration version** — the file was written by a newer
  build; use a compatible binary or restore a backup.
- **Unknown configuration key** — correct the TOML; unknown fields fail closed.
- **Connected initialization refused** — `--server` and
  `--replica-database` must both be valid.
- **Conflicting flags on an existing config** — re-run without flags, or pass
  `--force` after backing up.

Next: [your first project](first-project.md).
