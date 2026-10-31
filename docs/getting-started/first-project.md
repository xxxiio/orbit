# Your first project

This walkthrough uses local-authoritative mode and the default database created
by `orbit init`. No server, Herdr or AI provider is involved.

```sh
orbit init
orbit project add "Prototype API"
```

Output:

```text
Orbit initialized
...
created project project-1792480192043-1054584-0: Prototype API [proposed]
```

The generated ID is stable. Copy it and substitute it for `PROJECT_ID` below;
IDs are not display names.

## Set the project mode and criteria

```sh
orbit project status PROJECT_ID active
orbit project configure PROJECT_ID \
  --mode standard \
  --objective "Ship a documented prototype API" \
  --signal tasks \
  --signal blockers \
  --criterion api=Public API documented
```

`configure` is a replacement operation for strategy fields: repeated
`--criterion` values define the required success criteria, and
`--optional-criterion` defines optional ones. `--signal` is repeatable. Use
`orbit project configure --help` for the current mode and signal enums.

Inspect the result:

```sh
orbit project list
orbit project health PROJECT_ID
```

`orbit project status PROJECT_ID STATUS` transitions the project. It always
requires both arguments; it is not a read command.

## Add tasks

```sh
orbit task add "Design the API" --project PROJECT_ID
orbit task add "Implement the API" --project PROJECT_ID
```

Output:

```text
created task task-1792480194574-1054630-0: Design API [inbox]
created task task-1792480194589-1054631-0: Implement API [inbox]
```

List and filter:

```sh
orbit task list --project PROJECT_ID
orbit task list --status inbox --query API
```

## Add a dependency

The first argument is the task that is blocked; the second is the prerequisite.

```sh
orbit task depends-on IMPLEMENT_TASK_ID DESIGN_TASK_ID
```

Inspect readiness at an explicit Unix-millisecond timestamp:

```sh
orbit task ready --now-ms 1792480200000 --all
orbit task graph --now-ms 1792480200000 --all
```

Both tasks start in `inbox`; the dependent task reports
`unfinished_prerequisites`. Planning is informational and never completes work
or launches processes.

## Capture acceptance criteria

Tasks now have first-class, ordered acceptance criteria with stable IDs and
per-criterion completion. In the TUI select the task and press `K`; over MCP use
`task.acceptance.add`, `task.acceptance.check` and the other explicit
operations. They work in local and connected mode; connected writes queue a
canonical command and become accepted after a flush. Checking every criterion
does not complete the task; completion remains explicit. See
[task details and attachments](../guides/task-details-and-attachments.md).

For longer prose, use a linked Markdown note and say which source is
authoritative.

**Option A — a linked Markdown note.** Create a note, then add a typed
reference to the task:

```sh
orbit note create acceptance.md --id note-acceptance --body "# API acceptance
- Typed JSON responses
- Documented error cases"
orbit note reference note-acceptance task:TASK_ID
orbit note reference-backlinks task:TASK_ID
```

If you want a nested path, create the folder first:

```sh
orbit folder create docs
orbit note create docs/api-acceptance.md --id note-api-acceptance --body "..."
```

**Option B — a project success criterion.** Set it with
`project configure --criterion key=description` and mark it satisfied only after
review:

```sh
orbit project criterion PROJECT_ID api satisfied
```

Acceptance criteria are not inferred from agent output or test commands.

## Work and complete

```sh
orbit task start DESIGN_TASK_ID
orbit task complete DESIGN_TASK_ID
orbit task list
```

Output:

```text
started task task-1792480194574-1054630-0: Design API [in_progress]
completed task task-1792480194574-1054630-0: Design API [done]
```

Completing the prerequisite makes the dependent task eligible for readiness,
subject to deadlines and status. Completing a task is always explicit; it is
not a side effect of closing an editor, ending a session or finishing an agent.

## Inspect the workspace

```sh
orbit search API
orbit today
orbit status
```

`search` matches titles and stable IDs across projects, tasks, habits, notes
and schedule blocks (schedule results require `--from-ms` and `--to-ms`).
`today` shows the actionable queue. `status` reports the database path, schema,
integrity, SQLite policy and entity counts.

## Limitations to know

- There is no core task priority field. Planning uses deadlines, readiness and
  downstream impact (`task planning`, `task next`, `task day-plan`).
- There is no standalone milestone create/list/status command. `task add
  --milestone` accepts a reference to an existing milestone, but this build
  does not provide milestone CRUD.
- There is no project-switch command and no global "active project". Select a
  project with `--project` on commands that support it, and select a workspace
  with `--db` or `ORBIT_DB`.
- Backups use `orbit backup create|list|inspect|verify|restore`; the Web GUI
  page is at **Settings → Backup & Restore**, and a running server applies the
  configured automatic schedule. See
  [backup and recovery](../guides/backup-and-recovery.md).

## Next

- [Projects and planning](../guides/projects-and-planning.md)
- [Notes and folders](../guides/notes-and-folders.md)
- [Blueprints versus tasks](../guides/blueprints-vs-tasks.md)
- [Agent workflow tutorial](../guides/agent-workflow-tutorial.md)
- [Switching projects and workspaces](../guides/switching-projects-and-workspaces.md)
