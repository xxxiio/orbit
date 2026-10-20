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

### What is deferred

Automatic recurrence expansion and streak extraction are **not implemented** in
this build. There is no "every Monday" recurrence command and no streak field
in habit history. Use a weekly target plus explicit `check` calls and `history`
for consistency tracking. Do not infer a streak from consecutive rows unless
your own reporting defines the period boundaries.

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
pass or a measure of task completion. See
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
