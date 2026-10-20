# Projects, tasks, dependencies, and planning

This guide describes the local-authoritative CLI. All examples use an explicitly selected SQLite workspace, so experimentation cannot accidentally change your usual database.

```sh
mkdir -p "$HOME/.local/share/orbit"
export ORBIT_DB="$HOME/.local/share/orbit/demo.db"
orbit status
orbit project add "Example project"
orbit --json project list
```

The global `--json` switch may appear before the subcommand: `orbit --json project list`. Copy the generated project ID from the result; in the following commands substitute that ID for `PROJECT_ID`.

```sh
orbit task add "Design the API" --project PROJECT_ID
orbit task add "Implement the API" --project PROJECT_ID
orbit task list --project PROJECT_ID
```

Copy generated task IDs from `orbit task list`. The dependent task is the **first** argument to `depends-on`; the prerequisite is the second.

```sh
orbit task depends-on IMPLEMENT_TASK_ID DESIGN_TASK_ID
orbit task list --project PROJECT_ID
orbit task next --now-ms 1792454400000
orbit task day-plan --help
orbit task workload --now-ms 1792454400000
```

`--now-ms` values are Unix epoch **milliseconds**. The fixed timestamp above illustrates reproducible planning; substitute your own evaluation time for everyday use. Many planning operations require an explicit time precisely to avoid ambiguous results and make offline replays deterministic.

To inspect work before changing anything:

```sh
orbit task ready --now-ms 1792454400000 --all
orbit task graph --now-ms 1792454400000
orbit task completion-impact DESIGN_TASK_ID --now-ms 1792454400000
orbit task preview-dependency IMPLEMENT_TASK_ID DESIGN_TASK_ID --remove --now-ms 1792454400000
```

The `preview-dependency` command is read-only. In contrast, `orbit task unblock IMPLEMENT_TASK_ID DESIGN_TASK_ID` actually removes the dependency. `orbit task complete DESIGN_TASK_ID` changes task state. Confirm task IDs and readiness before running mutations.

## Scheduling and reviewed changes

Planning inspection and calendar mutation are deliberately separate. Start with `orbit task schedule-plan --help`, `orbit task review-schedule --help`, and `orbit task apply-reviewed-schedule --help`. The last operation requires a reviewed request file **and** `--confirm`; neither a suggestion nor a preview creates a reservation. The supplied JSON must match the application's typed request contract. Do not fabricate approval tokens.

## Project status and progress evidence

`orbit project status PROJECT_ID STATUS` changes project status; use `orbit project status --help` to see enum choices. Use `orbit project evidence --help` and `orbit project metric --help` for optional habit/metric evidence linked to a project. Metric observations include explicit event timestamps; reported progress should not be confused with a test-pass result.

## Common errors

- **Unknown IDs:** list the project or tasks first; generated IDs are not display names.
- **Blocked work:** inspect `task graph` and prerequisites rather than forcing readiness.
- **Different database:** check `ORBIT_DB` / `--db`, then `orbit status`.
- **Unexpected plan:** supply an explicit, correct `--now-ms` and inspect available capacity.

Related: [First project](../getting-started/first-project.md), [Agent workflow](agent-workflow.md), [Offline operation](offline-operation.md).
