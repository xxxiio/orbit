# Habits, routines and schedules

Habits, routines and schedule blocks are separate from tasks. A habit records
occurrences; a routine groups habits; a schedule block reserves time. None of
them completes a task automatically.

## Habits

```sh
orbit habit add "Morning review"
orbit habit add "Run" --weekly-target 3
orbit habit list
```

`--weekly-target` accepts 1 through 7. Omit it for a daily habit. Output:

```text
created habit habit-1792480200199-1054746-0: Run [3/week; active]
```

Record an occurrence:

```sh
orbit habit check HABIT_ID
orbit habit check HABIT_ID --at-ms 1792480200000 --duration-minutes 30
orbit habit history HABIT_ID
```

`check` rejects a paused habit. Timestamps are Unix milliseconds; duration is
optional time-spent evidence. History returns occurrence IDs, timestamps and
recorded duration in chronological order.

Pause and resume without deleting history:

```sh
orbit habit pause HABIT_ID
orbit habit resume HABIT_ID
orbit habit list --paused
orbit habit list --active --query run
```

### Recurrence and streaks are an opt-in capability

Habit **storage** stays in core: adding, listing, pausing/resuming and checking
habits, `orbit habit history`, the TUI habit tracker counts, and last-occurrence
projections all work without any extra capability. They read the same
`habits`/`habit_occurrences` rows as before.

**Recurrence and streak evaluation** — per-period completions, daily/weekly
targets, current and longest streak, totals, and the habit consistency shown in
project evidence — is *not* computed by core. It is provided by the optional
trusted capability `orbit.habit.recurrence`, action `evaluate_periods`. Until
that capability is installed (and, for project evidence, authorized for the
project), the host returns an explicit unavailable result instead of silently
computing a streak:

```json
{
  "type": "project_evidence_report",
  "project_id": "project-1792889943866-342239-0",
  "progress_bp": null,
  "habits": [
    {
      "id": "habit-1792889943970-342261-0",
      "available": false,
      "fulfilled": 0,
      "evaluated": 0,
      "progress_bp": null
    }
  ],
  "metrics": []
}
```

There is no "every Monday" calendar expansion in this build. Callers still
supply explicit period boundaries; the capability evaluates occurrences inside
those half-open windows.

#### Install and authorize it

Use an isolated database so the commands are reproducible:

```sh
export ORBIT_DB=/tmp/orbit-habit-capability.db
orbit project add "Habit docs" --json
orbit habit add "Morning review" --json
orbit habit check HABIT_ID --at-ms 1000 --duration-minutes 20 --json
orbit project evidence bind PROJECT_ID habit HABIT_ID --json
```

Before installation, `capability list` and the evidence report explain why no
streak exists:

```sh
orbit blueprint capability list --project PROJECT_ID --json
```

```json
{
  "type": "authority",
  "result": [
    {
      "actions": ["evaluate_periods"],
      "authorized_projects": [],
      "available": false,
      "capability_id": "orbit.habit.recurrence",
      "executor_registered": true,
      "installed": false,
      "permission": "read",
      "provider": "orbit",
      "requires": [],
      "unavailable_reason": "capability_not_installed: orbit.habit.recurrence is not installed"
    }
  ]
}
```

(The list also contains the independently registered reference capability; the
snippet is trimmed to the habit row.)

Installation records the contract; it does not grant project authority. The next
step installs and then authorizes exactly one project:

```sh
orbit blueprint capability install orbit.habit.recurrence --json
orbit blueprint capability authorize orbit.habit.recurrence \
  --project PROJECT_ID --confirm --json
```

After installation but before authorization, the reason is explicit:

```json
"available": false,
"installed": true,
"unavailable_reason": "not_authorized: orbit.habit.recurrence is not authorized for the requested project"
```

After authorization the same report computes the streak:

```sh
orbit project evidence report PROJECT_ID \
  --start-ms 0 --end-ms 2000 --as-of-ms 2000 --json
```

```json
{
  "type": "project_evidence_report",
  "project_id": "project-1792889943866-342239-0",
  "progress_bp": 10000,
  "habits": [
    {
      "id": "habit-1792889943970-342261-0",
      "available": true,
      "fulfilled": 1,
      "evaluated": 1,
      "progress_bp": 10000
    }
  ],
  "metrics": []
}
```

Uninstall removes the contract and every grant; revoke removes only one project's
grant:

```sh
orbit blueprint capability revoke orbit.habit.recurrence --project PROJECT_ID
orbit blueprint capability uninstall orbit.habit.recurrence
```

#### Where the gate lives

The host/projection seam is `ApplicationQueries::project_evidence_report` and
`ApplicationQueries::mode_aware_project_health` in
`crates/orbit-runtime/src/application_services.rs`. They check the durable
installation and the project grant, then pass a `HabitRecurrenceStatus` into
`orbit-application`. `habit_streak_summary` accepts the same status and returns
`HabitStreakSummaryOutcome::Unavailable { capability_id, reason }` without
reading occurrences when the capability is unavailable. The algorithm itself
lives in the `orbit-habit-capability` package; `orbit-domain` keeps only
`HabitOccurrence`/`HabitPeriod` value types and validation.

## Routines

Routines are ordered collections of existing habits:

```sh
orbit routine add "Weekday reset"
orbit routine list
orbit routine add-habit ROUTINE_ID HABIT_ID
orbit routine remove-habit ROUTINE_ID HABIT_ID
```

Routine membership is organizational metadata. Adding a habit to a routine
does not pause, resume, complete or schedule it.

## Schedule blocks

```sh
orbit schedule add "Deep work" --start-ms 1792480200000 --duration-minutes 90
orbit schedule add "Dentist" --start-ms 1792570200000 --duration-minutes 60 --fixed
orbit schedule add "Run" --start-ms 1792581000000 --duration-minutes 45 --habit HABIT_ID
orbit schedule add "Design review" --start-ms 1792584600000 --duration-minutes 60 --task TASK_ID
```

- `--start-ms` and `--duration-minutes` are required.
- Blocks are movable by default; `--fixed` makes them immutable to planner or
  user rescheduling. `--movable` is the explicit default.
- `--task` and `--habit` are mutually exclusive and must reference existing
  entities.

List a time window (both bounds required):

```sh
orbit schedule list --from-ms 1792393800000 --to-ms 1792655400000
orbit schedule list --from-ms 1792393800000 --to-ms 1792655400000 --query review
```

Reschedule:

```sh
orbit schedule reschedule SCHEDULE_ID --start-ms 1792573800000
```

Rescheduling a fixed block is rejected. `orbit search` includes schedule blocks
only when an explicit `--from-ms`/`--to-ms` window is supplied.

## Using habits as project evidence

Bind a habit to a project to include consistency in health reporting:

```sh
orbit project evidence bind PROJECT_ID habit HABIT_ID
orbit project evidence report PROJECT_ID \
  --start-ms 1792393800000 \
  --end-ms 1792655400000 \
  --as-of-ms 1792655400000
```

Evidence is evaluated in an explicit window. Habit consistency is not a test
pass or a measure of task completion. Recurrence is opt-in: bind the habit and
install/authorize `orbit.habit.recurrence` as shown above, otherwise the report
returns the habit as unavailable with no streak. See
[projects and planning](projects-and-planning.md).

## Common errors

- **`--weekly-target` out of range** — use 1–7.
- **Checking a paused habit** — resume it first.
- **Unknown task/habit on a schedule block** — create the entity first.
- **Missing `--from-ms`/`--to-ms`** — schedule listing requires both.
- **Fixed block will not move** — create a new movable block or change the plan
  deliberately; do not edit SQLite rows.
- **Wrong workspace** — run `orbit status` and check `--db`/`ORBIT_DB`.

Next: [notes and folders](notes-and-folders.md) or
[search and navigation](search-and-navigation.md).
