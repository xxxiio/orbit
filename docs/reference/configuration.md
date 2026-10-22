# Configuration reference

Orbit reads one optional versioned TOML file and resolves a deployment from it,
the environment and CLI overrides. A missing file means built-in defaults. A
file that exists but is invalid **fails closed**; Orbit never silently falls
back to a different database.

## File location

| Priority | Source |
| ---: | --- |
| 1 | `--config PATH` |
| 2 | `ORBIT_CONFIG` |
| 3 | `~/.orbit/config.toml` |

`orbit init` writes the file atomically. `doctor` reads it without modifying it.

A custom config location changes only where the file is read from. Runtime
directories (`logs`, `state`, `backups`) and the default database remain under
`~/.orbit` when a home directory is available. Relative paths **inside** the
file resolve against the config file's directory, not the process working
directory.

## Schema

```toml
version = 1

[storage]
# database = "~/custom/orbit.db"

[mode]
authority = "local"                 # or "connected"

[connected]
# server = "https://orbit.example"
# replica_database = "~/.orbit/replica.db"

[agent]
# default_provider = "codex"
```

### `version` (required)

Integer. This build supports only `1`. Any other value is an error.

### `[storage]`

| Key | Type | Default | Meaning |
| --- | --- | --- | --- |
| `database` | string | omitted → `~/.orbit/orbit.db` | Local-authoritative database path |

Paths may be absolute, relative to the config directory, or start with `~/`.
Empty paths are rejected. In connected mode this key is not the replica;
`connected.replica_database` selects the replica.

### `[mode]`

| Key | Type | Default | Values |
| --- | --- | --- | --- |
| `authority` | string | `local` | `local`, `connected` |

`connected` requires a complete `[connected]` section. There is no partial
connected configuration.

### `[connected]`

| Key | Type | Required when | Meaning |
| --- | --- | --- | --- |
| `server` | string | `authority = "connected"` | Authenticated server base URL |
| `replica_database` | string | `authority = "connected"` | Local accepted-state replica and outbox |

`server` must be a valid URL. Non-loopback endpoints must use TLS in operation.
`replica_database` follows the same path rules as `storage.database`.

### `[agent]`

| Key | Type | Default | Meaning |
| --- | --- | --- | --- |
| `default_provider` | string | omitted → `codex` at launch | Default agent provider identifier |

The provider must be non-empty and contain no control characters. It does not
install Herdr or the provider.

Unknown keys are rejected. This is intentional: a typo must not silently change
the deployment.

## Database selection precedence

Local mode, highest priority first:

1. `--db PATH`
2. `ORBIT_DB`
3. `[storage].database`
4. `~/.orbit/orbit.db`

Connected mode, highest priority first:

1. `--db PATH` (used as the replica path)
2. `ORBIT_AUTHORITY_DB`
3. `ORBIT_DB`
4. `[connected].replica_database`

A connected deployment can also be selected entirely by environment variables
when the config authority is local:

- `ORBIT_AUTHORITY_DB` plus `ORBIT_AUTHORITY_SERVER` (or
  `ORBIT_AUTHORITY_URL`) select a replica and server.
- Supplying only one of the pair fails with a missing-server or
  missing-replica error.
- `ORBIT_AUTHORITY_SERVER` and `ORBIT_AUTHORITY_URL` must agree if both are
  set.
- A replica path identical to the local authority database is rejected.

There is no local-to-connected fallback. A connected client that cannot reach
its authority reports a connection failure; it does not write to a local
database instead.

## Environment variables

| Variable | Used by | Meaning |
| --- | --- | --- |
| `ORBIT_DB` | CLI | Local database or connected replica override |
| `ORBIT_CONFIG` | CLI | Configuration file path |
| `ORBIT_AUTHORITY_DB` | CLI/deployment | Connected replica path |
| `ORBIT_AUTHORITY_SERVER` | CLI/deployment | Connected server URL |
| `ORBIT_AUTHORITY_URL` | CLI/deployment | Alternate server URL; must agree with `ORBIT_AUTHORITY_SERVER` |
| `ORBIT_SYNC_HTTP_TOKEN` | `orbit sync` | Device token; never commit |
| `ORBIT_AUTHORITY_DEVICE` | `orbit sync` | Enrolled device identity |
| `ORBIT_MCP_MODE` | `orbit-mcp` | `read_only` or `developer` |
| `ORBIT_MCP_PROJECTS` | `orbit-mcp` | Allowed project IDs |
| `ORBIT_DEVICE_ID` / `ORBIT_DEVICE_TOKEN` | `orbit-mcp` connected | Connected device credentials |
| `ORBIT_ATTACHMENT_MAX_BYTES` | CLI/TUI/MCP local | Maximum attachment size in bytes; default 64 MiB |
| `ORBIT_VIEWER` | `orbit-tui` | External viewer command for attachment open |
| `ORBIT_VERSION` | `install.sh` | Requested install version |
| `ORBIT_INSTALL_PREFIX` | `install.sh` | Install prefix |
| `ORBIT_TARGET` | `install.sh` tests | Target override; not for normal use |

`HOME` (or `USERPROFILE`) determines `~` expansion and the default layout.
Empty environment values are errors where they would otherwise silently change
the target.

## Effective paths

| Path | Default | Notes |
| --- | --- | --- |
| Config | `~/.orbit/config.toml` | Override with `--config`/`ORBIT_CONFIG` |
| Database | `~/.orbit/orbit.db` | Local authority, or configured path |
| Markdown vault | `<database>.knowledge/` | Must be backed up with the database |
| Attachment objects | `<database directory>/attachments/objects/` | Content-addressed; back up with the database |
| Logs | `~/.orbit/logs/` | Created by `orbit init` |
| State | `~/.orbit/state/` | Created by `orbit init` |
| Backups | `~/.orbit/backups/` | Used by `orbit update` |

`orbit init` creates layout directories with user-only permissions on Unix and
refuses symlinked layout directories.

## Project modes

Project mode is stored per project, not in `config.toml`. Configure it with
`orbit project configure PROJECT_ID --mode MODE`. Default progress signals:

| Mode | Default signals |
| --- | --- |
| `standard` | milestones, tasks, blockers, forecast |
| `learning` | milestones, learning evidence, habit consistency, time invested |
| `habit-driven` | habit consistency, time invested |
| `metric-target` | metric value, forecast |
| `maintenance` | metric value, habit consistency, blockers |
| `custom` | milestones unless `--signal` overrides |

Explicit `--signal` values replace the defaults. See
[projects and planning](../guides/projects-and-planning.md).

## Failure behavior

| Condition | Result |
| --- | --- |
| File absent | Built-in local defaults |
| Invalid TOML | Error; no fallback |
| Unknown key | Error; no fallback |
| Unsupported `version` | Error naming the supported version |
| Empty path/provider | Error |
| `authority = "connected"` without `[connected]` | Error |
| Missing server or replica in connected resolution | Error |
| Server/replica conflict | Error |
| No home directory and no explicit config | Error for setup/diagnostic commands |

Errors are reported through the normal human or `--json` output and exit with a
usage/domain-input class. `orbit doctor` reports configuration validity without
changing it.

## Examples

Local project database:

```toml
version = 1

[storage]
database = "~/work/orbit-work.db"

[mode]
authority = "local"

[agent]
default_provider = "codex"
```

Connected client:

```toml
version = 1

[mode]
authority = "connected"

[connected]
server = "https://orbit.example"
replica_database = "~/.orbit/replica.db"
```

Generate these with `orbit init` rather than editing by hand when possible. See
[initialization](../getting-started/initialization.md) and the
[CLI reference](cli.md).
