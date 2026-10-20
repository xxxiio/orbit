# Local quickstart: your first 15 minutes

This tutorial uses a local SQLite workspace and needs no server, Herdr, MCP
client, Python or AI provider. It is the fastest way to confirm that the
installed binary works.

## 1. Install, initialize and check

Install with the one-command installer:

```sh
curl -fsSL https://github.com/xxxiio/orbit/releases/latest/download/install.sh | sh
export PATH="$HOME/.local/bin:$PATH"
```

Then initialize the default workspace and run the read-only diagnostic:

```sh
orbit init
orbit doctor
```

`orbit init` creates `~/.orbit/config.toml`, `~/.orbit/orbit.db` and the
Markdown vault. `orbit doctor` should end with `Status: ok`. Optional tools such
as Herdr or `orbit-mcp` are reported as informational findings; they are not
required.

If you prefer an isolated workspace for the tutorial, select one explicitly:

```sh
mkdir -p "$HOME/.orbit/tutorial"
orbit --db "$HOME/.orbit/tutorial/quickstart.db" init
export ORBIT_DB="$HOME/.orbit/tutorial/quickstart.db"
```

Use the same `--db`/`ORBIT_DB` on **every** command in this tutorial. A command
run against a different database will appear to lose your work. `--json` emits
machine-readable output and may appear before the subcommand.

## 1a. Open the terminal workspace (optional)

The installed TUI binary reads the same workspace:

```sh
orbit-tui
```

Use the sidebar to open **Home / Today**, **Inbox**, **Projects** or **Agents**.
Select a project and press `Enter` to open its blueprint views. `?` shows the
complete keyboard map, `:` opens the command palette and `/` searches. Press
`q` from Home to leave; from a project it returns Home first. On a narrow
terminal, `Tab` switches between the sidebar and workspace. See
[the terminal workspace guide](../guides/tui-workspace.md) for the full
navigation and troubleshooting reference.

## 2. Create a project

```sh
orbit project add "Prototype API"
```

```text
created project project-1792480192043-1054584-0: Prototype API [proposed]
```

Substitute the returned ID for `PROJECT_ID` in later commands.

```sh
orbit project status PROJECT_ID active
orbit project configure PROJECT_ID \
  --mode standard \
  --objective "Ship a documented prototype" \
  --criterion api=Public API documented
```

## 3. Add tasks and a dependency

```sh
orbit task add "Design the API" --project PROJECT_ID
orbit task add "Implement the API" --project PROJECT_ID
orbit task list --project PROJECT_ID
```

```text
task-...-0  inbox  Design API     project=PROJECT_ID
task-...-1  inbox  Implement API  project=PROJECT_ID
```

The dependent task is the first argument to `depends-on`; the prerequisite is
the second:

```sh
orbit task depends-on IMPLEMENT_TASK_ID DESIGN_TASK_ID
orbit task ready --now-ms 1792480200000 --all
orbit task graph --now-ms 1792480200000 --all
```

`--now-ms` is Unix **milliseconds**, not seconds. Readiness explains what is
actionable; it does not change state or launch anything.

## 4. Record acceptance criteria

There is no task acceptance-criteria field. Use a linked note or a project
criterion, and state which approach your project uses:

```sh
orbit note create acceptance.md --id note-acceptance --body "# Acceptance
- Typed JSON responses
- Documented errors"
orbit note reference note-acceptance task:DESIGN_TASK_ID
orbit note reference-backlinks task:DESIGN_TASK_ID
```

Or:

```sh
orbit project criterion PROJECT_ID api satisfied
```

Do not mark a criterion satisfied before reviewing the work.

## 5. Work, finish and inspect

```sh
orbit task start DESIGN_TASK_ID
orbit task complete DESIGN_TASK_ID
orbit search API
orbit today
orbit status
```

`task complete` is explicit. A closed editor, an ended agent session or a
successful command in another tool does **not** complete the task. The
dependent task becomes eligible only after its prerequisite is done and its own
readiness conditions are met.

## 6. Optional blueprint

A blueprint is a versioned workflow definition, not a replacement for core
tasks:

```sh
orbit blueprint bundled
orbit blueprint install development
orbit blueprint list
orbit blueprint bind PROJECT_ID development 1
orbit blueprint binding PROJECT_ID
```

A project stays pinned to the installed version until explicitly rebound.
Blueprint work items and Orbit tasks are different entities.

## 7. Where to go next

- [Your first project](first-project.md) — the same workflow with more detail.
- [Initialization](initialization.md) — layout, config and precedence.
- [Projects and planning](../guides/projects-and-planning.md) — modes, criteria and planning.
- [Notes and folders](../guides/notes-and-folders.md) — the Markdown vault.
- [Development projects](../guides/development-projects.md) — software delivery.
- [Agent workflow tutorial](../guides/agent-workflow-tutorial.md) — optional Herdr execution.
- [Connected mode](connected-mode.md) — only when you need a server.

Do not mix local-authoritative and connected-replica databases. When in doubt,
run `orbit doctor` and `orbit status` before changing state.
