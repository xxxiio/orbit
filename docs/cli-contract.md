# Orbit CLI contract

The `orbit` CLI is a headless client of Orbit application services. It does not write SQLite directly.

## Workspace selection

Resolution order:
1. `--db <path>`
2. `ORBIT_DB`
3. `$HOME/.orbit/orbit.db` (or `%USERPROFILE%\\.orbit\\orbit.db`)
4. `./orbit.db` when no home directory is available

## Output contract

Human-readable output is the default. `--json` switches successful responses and runtime errors to structured JSON.

Examples:

```bash
orbit task add "Renew passport"
orbit task list
orbit task list --status in-progress
orbit task start <task-id>
orbit task complete <task-id>
orbit project add "Orbit Core"
orbit project list
orbit today
orbit task list --json
```

Success JSON has a stable top-level `type`. Runtime error JSON has:

```json
{
  "type": "error",
  "code": "application_error",
  "message": "...",
  "recoverable": true
}
```

The CLI intentionally respects domain transitions. `task start` may internally promote an Inbox task through Ready before persisting In Progress; `task complete` does not silently skip invalid lifecycle states.

`orbit today` is provisional in M3: it returns planned, ready, and in-progress tasks. Calendar/capacity-backed day planning is introduced in M4 without breaking the command/output surface.

## Habit commands

```bash
orbit habit add "Exercise"
orbit habit add "Run" --weekly-target 3
orbit habit list
orbit habit pause <habit-id>
orbit habit resume <habit-id>
```

Habits remain first-class entities rather than recurring tasks. `--weekly-target` must satisfy the domain cadence invariant (1..=7); omitting it creates a daily habit.

## Schedule commands

```bash
orbit schedule add "Deep work" --start-ms 1770000000000 --duration-minutes 90
orbit schedule add "Dentist" --start-ms 1770000000000 --duration-minutes 60 --fixed
orbit schedule add "Run" --start-ms 1770000000000 --duration-minutes 45 --habit <habit-id>
orbit schedule add "Focus" --start-ms 1770000000000 --duration-minutes 60 --task <task-id>
orbit schedule list --from-ms 1769990000000 --to-ms 1770100000000
orbit schedule reschedule <schedule-id> --start-ms 1770200000000
```

Schedule blocks default to movable unless `--fixed` is provided. Task and habit targets are mutually exclusive and must already exist. Rescheduling a fixed block is rejected by the domain; the CLI does not bypass that invariant.

## Lifecycle editing and search

The CLI exposes domain-driven lifecycle changes rather than writing status fields directly:

```bash
orbit task status <task-id> blocked
orbit task status <task-id> review
orbit project status <project-id> active
orbit project status <project-id> on-hold
```

Invalid transitions are rejected by the domain model and returned through the same human/JSON error contract.

List commands support local filtering without changing repository semantics:

```bash
orbit task list --status ready --project <project-id> --query passport
orbit project list --status active --query orbit
orbit habit list --active --query run
orbit schedule list --from-ms 1770000000000 --to-ms 1770100000000 --query focus
```

Cross-entity search is case-insensitive over stable ids and display titles/names:

```bash
orbit search orbit
orbit search run --from-ms 1770000000000 --to-ms 1770100000000
orbit search orbit --json
```

Schedule blocks are included in cross-entity search only when an explicit `--from-ms/--to-ms` window is supplied. JSON search results use one stable tagged shape with `kind` plus `item`.

## Workspace diagnostics

`orbit status` is the headless workspace health surface. It reports:

- resolved database path;
- current schema version;
- read-only integrity health and any detected issues;
- effective SQLite WAL, foreign-key, and busy-timeout policy;
- current project/task/habit counts.

`orbit status --json` uses the stable `workspace_status` response type. Diagnostics never repair or mutate corrupted state.

## Process exit codes

Orbit reserves stable runtime exit classes for scripts and agents:

| Code | Meaning |
| ---: | --- |
| `0` | success |
| `2` | usage or domain-input error |
| `3` | application/use-case failure |
| `4` | local I/O or repository/storage failure |
| `70` | internal serialization failure |

Clap argument/parser failures also use the conventional exit code `2`. JSON mode changes error formatting, not exit semantics.

## CLI acceptance contract

The M3 acceptance workflow uses one real temporary SQLite workspace and must prove:

1. `task add --json` creates an Inbox task;
2. `task list --query ... --json` can discover it;
3. `task start --json` transitions it through the application/domain rules to `in_progress`;
4. `search ... --json` returns a typed task result;
5. `status --json` reports a healthy workspace and the expected entity count.

These scenarios are present as executable Rust tests but remain validation-blocked until the Rust toolchain is available.


## Task planning metadata

```bash
orbit task planning <task-id> --estimate-minutes 45 --earliest-start-ms 100 --deadline-ms 500
```

The command replaces the task's planning metadata atomically and records an `Updated` activity. Omitting a field clears that field. Task JSON includes `estimated_minutes`, `earliest_start_ms`, and `deadline_ms`.

## Project health / progress evidence

```bash
orbit project health <project-id>
orbit project health <project-id> --lookback-minutes 10080
orbit project health <project-id> --json
```

The command is deterministic and evidence-first. It returns the canonical current project rollup,
per-milestone rollups, blocked task/milestone counts, recent activity evidence, and an explainable
health projection (`on_track`, `needs_attention`, or `at_risk`) with explicit reason codes.

Current canonical task progress is status-derived unless a stored normalized progress source is
available: completed tasks are 100%, unfinished tasks default to 0%, cancelled work is excluded,
and estimate minutes are used as weights when available. The JSON response exposes
`status_derived_progress=true` so clients do not mistake this fallback for richer measured progress.

The command never invents planned progress, effort actuals, project target dates, progress history,
or forecasts. Those fields participate in the health engine only when explicit evidence is available.

## Habit occurrence history

Habits remain first-class entities rather than recurring tasks. A completion is persisted as a
`HabitOccurrence` and mirrored into the semantic activity journal atomically.

```bash
orbit habit check <habit-id>
orbit habit check <habit-id> --duration-minutes 30
orbit habit check <habit-id> --at-ms 1770000000000 --duration-minutes 30
orbit habit history <habit-id>
orbit habit history <habit-id> --json
```

`habit check` rejects paused habits. `habit history` returns occurrence ids, timestamps, and optional
time-spent evidence in deterministic chronological order. Streak/day/week calculations do not assume
a timezone: callers supply explicit period boundaries to the application/domain API.

## Routine commands

```text
orbit routine add "Morning"
orbit routine list
orbit routine add-habit <routine-id> <habit-id>
orbit routine remove-habit <routine-id> <habit-id>
```

Routine JSON returns ordered `habit_ids`. Routine membership is organizational/planning metadata only; it does not pause, resume, complete, or otherwise mutate the referenced habits.

## Structured knowledge

```text
orbit knowledge add decision "Use Rust" --body "Rust is the embedded/headless core."
orbit knowledge list --kind decision --status active
orbit knowledge show <knowledge-id>
orbit knowledge update <knowledge-id> --title "..." --body "..."
orbit knowledge status <knowledge-id> resolved
orbit knowledge link <knowledge-id> about project:<project-id>
orbit knowledge unlink <knowledge-id> about project:<project-id>
orbit knowledge export <knowledge-id> ./decision.json
orbit knowledge import ./decision.json
```

Knowledge targets use typed `kind:id` references (`project`, `milestone`, `task`, `habit`, `knowledge`). The same commands support `--json` output. Knowledge export/import is explicit and local; no hosted Orbit service is required.

### Knowledge retrieval

```text
orbit knowledge search "rust" --kind decision --status active
orbit knowledge search "sync" --target project:<project-id>
orbit knowledge backlinks project:<project-id>
orbit knowledge context task:<task-id> --limit 20
orbit knowledge inbox
orbit search "rust"
```

Global `orbit search` now includes structured knowledge results. Backlinks and context use the same typed `kind:id` target grammar as knowledge links.
