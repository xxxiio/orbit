# Search and navigation

Orbit has no interactive shell. Use list, search, status and planning commands
to find work and understand where you are.

## Search across entities

```sh
orbit search API
orbit search API --json
```

Search is a case-insensitive substring match against titles/names and stable
IDs. It covers projects, tasks, habits, notes and—only with an explicit
window—schedule blocks.

```sh
orbit search review --from-ms 1792393800000 --to-ms 1792655400000
```

Both `--from-ms` and `--to-ms` are required together. Times are Unix
milliseconds. Without a window, schedule blocks are excluded rather than
searched with an implicit "now".

Representative output:

```text
project     project-...  Prototype API  [active]
task        task-...     Design API     [done]
knowledge   note-design  design         [docs/design.md]
```

`knowledge` is the result kind for a Markdown note; it is not a command group.

## Inspect the workspace

```sh
orbit status
orbit status --json
```

`status` reports the resolved database path, schema version, integrity, SQLite
WAL/foreign-key/busy-timeout policy and entity counts. It never repairs data.
If results look wrong, run `status` before changing anything.

## Find the next task

```sh
orbit task next --now-ms 1792480200000
orbit task next --now-ms 1792480200000 --limit 10
```

`next` ranks actionable work deterministically by deadline and downstream
impact. An empty result means no task is actionable at that time, not that the
workspace is empty. Use `task ready --all` and `task graph --all` to see why.

## See the day

```sh
orbit today
orbit task day-plan --now-ms 1792480200000 --available-minutes 240
orbit task workload --now-ms 1792480200000
orbit task work-brief --now-ms 1792480200000 --ends-at-ms 1792566600000
```

- `today` shows planned, ready and in-progress work.
- `day-plan` fits tasks into the supplied capacity and explains exclusions.
- `workload` reports overdue work and dependency bottlenecks.
- `work-brief` summarizes accepted Today/Next work and project blockers.

These are read-only planning views. A plan is not a calendar reservation; use
the reviewed scheduling workflow for that.

## Drill into one task or project

```sh
orbit task planning-focus TASK_ID --now-ms 1792480200000 --ends-at-ms 1792566600000
orbit task planning-actions TASK_ID --now-ms 1792480200000 --ends-at-ms 1792566600000
orbit task completion-impact TASK_ID --now-ms 1792480200000
orbit project health PROJECT_ID
orbit project list --status active --query API
```

`planning-actions` offers a safe next-step menu; it does not submit mutations.
Resolve a selected action against a JSON file only when you intend to proceed.

## List and filter

```sh
orbit task list --status ready --project PROJECT_ID --query API
orbit task list --status review
orbit project list --status active --query prototype
orbit habit list --active --query run
orbit note list
orbit note catalog --kind decision
orbit folder tree
```

Filters compose. `task list` status values are the full task lifecycle; see
[projects and planning](projects-and-planning.md).

## Time conventions

Every `--*-ms` argument is Unix epoch **milliseconds**. Convert explicitly:

```sh
date +%s%3N
```

Do not pass seconds. Planning commands require an explicit evaluation time so
results remain reproducible and replay-safe offline.

## No navigation shortcuts

There is no project-switch command and no global active project. Select a
workspace with `--db`/`ORBIT_DB` and a project with `--project`. There is no
`cd` or interactive tree browser; `folder tree` prints a human-readable tree by
default, and `--json` selects structured output. `search` is the cross-entity
lookup.

## Common errors

- **No results** — confirm the workspace and spelling; search is a substring
  match, not fuzzy.
- **Missing schedule results** — supply both `--from-ms` and `--to-ms`.
- **Empty `next`** — check readiness at the same `--now-ms`.
- **Stale counts** — run `orbit status`; a different `--db` probably selected a
  different workspace.
- **JSON parse errors** — put `--json` before or after the subcommand as
  documented; do not mix human and JSON output.

Next: [projects and planning](projects-and-planning.md) or
[switching projects and workspaces](switching-projects-and-workspaces.md).
