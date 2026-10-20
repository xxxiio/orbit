# Switching projects and workspaces

Orbit has two independent selectors:

- a **workspace** is one SQLite database plus its Markdown vault;
- a **project** is an entity inside that workspace.

There is no global "active project" and no project-switch command. This is
deliberate: commands are explicit about which workspace and project they touch.

## Select a workspace

Highest priority first:

1. `--db PATH`
2. `ORBIT_DB`
3. `[storage].database` in the configuration file
4. `~/.orbit/orbit.db`

```sh
orbit --db "$HOME/work/orbit-work.db" status
ORBIT_DB="$HOME/work/orbit-work.db" orbit project list
```

Use the same selector on every command in a workflow. A common failure is
creating a project with `--db work.db` and then listing tasks against the
default database, which looks like data loss.

Each database has its own vault at `<database>.knowledge/`. Moving a database
without its vault separates notes from their references.

## Select a project

Pass `--project` to commands that support it:

```sh
orbit task add "Write the migration" --project PROJECT_ID
orbit task list --project PROJECT_ID
orbit task day-plan --now-ms 1792480200000 --available-minutes 180 --project PROJECT_ID
```

Use `orbit project list` to discover IDs. `--project` is a filter/association,
not a global switch; a subsequent command without it is not scoped to the same
project.

Some commands take a project ID as a positional argument instead:

```sh
orbit project health PROJECT_ID
orbit project criterion PROJECT_ID api satisfied
orbit blueprint binding PROJECT_ID
```

## Select a configuration file

```sh
orbit --config "$HOME/.config/orbit/work.toml" doctor
ORBIT_CONFIG="$HOME/.config/orbit/work.toml" orbit project list
```

A custom config file location does not relocate the default database, logs or
state; those remain under `~/.orbit` when a home directory exists. Relative
paths inside the config resolve against the config file's directory. See the
[configuration reference](../reference/configuration.md).

## Multiple local workspaces

Using explicit paths is the simplest way to separate personal, work and test
data:

```sh
orbit --db "$HOME/.orbit/personal.db" project add "Personal"
orbit --db "$HOME/.orbit/work.db" project add "Work"
orbit --db "$HOME/.orbit/work.db" project list
```

Do not point two commands at the same database through different symlinked
paths and assume they are separate. Use real paths and check `orbit status`.

A disposable workspace is also the right way to try a new version:

```sh
orbit --db /tmp/orbit-smoke.db init
orbit --db /tmp/orbit-smoke.db project add "Smoke"
```

## Blueprint bindings are per project

Blueprint versions are installed in the workspace and pinned per project:

```sh
orbit blueprint bundled
orbit blueprint install development
orbit blueprint bind PROJECT_ID development 1
orbit blueprint binding PROJECT_ID
```

Publishing a newer version does not rebind existing projects. Check the binding
before creating or transitioning blueprint items.

## Connected replicas are not local workspaces

A connected client uses a replica database bound to one server:

```sh
orbit init --authority connected \
  --server https://orbit.example \
  --replica-database "$HOME/.orbit/replica.db"
```

Do not reuse a local-authoritative database as a replica or mix both modes in
one workflow. The replica contains accepted snapshots and pending commands, not
locally authoritative state. See [connected mode](../getting-started/connected-mode.md).

## Checklist when work "disappears"

1. Run `orbit status` and note the database path.
2. Check whether the command used `--db`, `ORBIT_DB` or the default.
3. Check `ORBIT_CONFIG` and the `[storage].database` setting.
4. List projects and tasks in that exact workspace.
5. Only then investigate IDs or filters.

## Related

- [Projects and planning](projects-and-planning.md)
- [Search and navigation](search-and-navigation.md)
- [Backup and recovery](backup-and-recovery.md)
- [Configuration reference](../reference/configuration.md)
