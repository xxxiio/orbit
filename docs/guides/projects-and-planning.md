# Projects, tasks, dependencies and planning

This guide describes the local-authoritative CLI. Examples assume a workspace
selected with `--db` or `ORBIT_DB`; see [switching projects and
workspaces](switching-projects-and-workspaces.md).

```sh
orbit init
orbit project add "Example project"
orbit project list
```

Copy the generated project ID. Substitute `PROJECT_ID`, `TASK_ID` and other
placeholders; IDs are stable identifiers, not display names.

## Project lifecycle and mode

```sh
orbit project status PROJECT_ID active
orbit project list --status active
orbit project configure PROJECT_ID \
  --mode metric-target \
  --objective "Reduce build time" \
  --signal metric-value \
  --signal tasks \
  --criterion p95=Build p95 under four minutes
```

Project statuses are `proposed`, `active`, `on-hold`, `completed`, `cancelled`
and `archived`. Mode is one of `standard`, `learning`, `habit-driven`,
`metric-target`, `maintenance` or `custom`.

`configure` replaces the strategy fields supplied by the command. Required
criteria are repeatable `--criterion key=description`; optional criteria use
`--optional-criterion`. Mark a criterion only after independently checking it:

```sh
orbit project criterion PROJECT_ID p95 satisfied
```

Statuses for a criterion are `pending`, `satisfied` and `waived`.

There is no global "active project" and no project-switch command. Commands
that support project scoping accept `--project PROJECT_ID`.

## Task lifecycle

Tasks are created in `inbox` and move through the domain lifecycle:

| Status | Meaning |
| --- | --- |
| `inbox` | Captured, not yet planned |
| `planned` | Has planning metadata or an explicit plan |
| `ready` | Actionable now |
| `in-progress` | Actively being worked |
| `waiting` | Waiting on an external condition |
| `blocked` | Blocked by a prerequisite or explicit block |
| `review` | Work complete enough for review |
| `deferred` | Intentionally postponed |
| `done` | Completed |
| `cancelled` | Abandoned |

Use commands rather than editing rows:

```sh
orbit task add "Design the API" --project PROJECT_ID
orbit task start TASK_ID
orbit task status TASK_ID review
orbit task complete TASK_ID
orbit task list --status review --project PROJECT_ID --query API
```

`task start` and `task complete` are convenience transitions. `task status`
accepts the status enum shown by `--help`. Invalid transitions are rejected by
the domain layer, not silently forced.

### Planning metadata

```sh
orbit task planning TASK_ID \
  --estimate-minutes 90 \
  --earliest-start-ms 1792480200000 \
  --deadline-ms 1792566600000
```

All times are Unix milliseconds. Omitting a field clears it. Planning metadata
makes prioritization and capacity calculations meaningful; it does not change
task status.

### Priority and milestones are limited

This build has no core task priority field. Rank work with deadlines,
earliest-start times, readiness and downstream impact.

There is no standalone milestone create/list/status command. `task add
--milestone MILESTONE_ID` accepts a reference to an existing milestone and
requires `--project`, but milestone CRUD is not exposed by this CLI. Do not
invent milestone commands; model milestone-level goals as tasks, project
criteria, blueprint items or notes.

## Dependencies and readiness

The dependent task is the first argument; the prerequisite is the second:

```sh
orbit task depends-on IMPLEMENT_TASK_ID DESIGN_TASK_ID
orbit task unblock IMPLEMENT_TASK_ID DESIGN_TASK_ID
```

Inspect without changing accepted state:

```sh
orbit task ready --now-ms 1792480200000 --all
orbit task graph --now-ms 1792480200000 --all
orbit task completion-impact DESIGN_TASK_ID --now-ms 1792480200000
orbit task preview-dependency IMPLEMENT_TASK_ID DESIGN_TASK_ID --remove --now-ms 1792480200000
```

- `ready` explains actionable versus blocked work at an explicit time.
- `graph` shows direct and transitive prerequisites, roots, leaves and
  downstream impact.
- `completion-impact` previews what becomes actionable if one task completes.
- `preview-dependency` simulates an add/remove. It is read-only;
  `depends-on`/`unblock` perform the mutation.

Cycles are rejected by the domain layer. A task with unfinished prerequisites
is not actionable even if its status is `ready`.

## Planning and capacity

```sh
orbit task next --now-ms 1792480200000 --limit 10
orbit task day-plan --now-ms 1792480200000 --available-minutes 240
orbit task workload --now-ms 1792480200000 --horizon-ms 604800000
orbit task work-brief --now-ms 1792480200000 --ends-at-ms 1792566600000
orbit today
```

`next` ranks actionable work deterministically by deadline and downstream
impact. `day-plan` respects available minutes, estimates and optional
`--include-unsized`/`--default-minutes`; it never writes a reservation. Use
`--project` to scope a plan.

Advanced planning (`portfolio-actions`, `compare-scenarios`,
`review-scenario`, `schedule-plan`, `review-schedule`,
`apply-reviewed-schedule`, `planning-actions`) is read-only until an explicit
reviewed apply. In particular, only `apply-reviewed-schedule` creates calendar
reservations, and it requires a reviewed request file plus `--confirm`. A
suggestion or preview is not approval.

## Project health and evidence

```sh
orbit project health PROJECT_ID
orbit project health PROJECT_ID --lookback-minutes 10080
```

Health is deterministic and evidence-first. It reports task progress, mode
progress, bound evidence, missing signals, blocked counts and recent activity.
It does not invent forecasts or measured effort.

Bind optional habit or metric evidence:

```sh
orbit project metric target build_minutes 18 10 at-most
orbit project metric observe build_minutes 14 --observed-at-ms 1792480200000
orbit project evidence bind PROJECT_ID metric build_minutes
orbit project evidence list PROJECT_ID
orbit project evidence report PROJECT_ID \
  --start-ms 1792393800000 \
  --end-ms 1792480200000 \
  --as-of-ms 1792480200000
```

Metric values are explicit observations, not test results or a substitute for
review. Habit evidence is bound similarly with `--kind habit`.

## Scheduling

```sh
orbit schedule add "Deep work" --start-ms 1792480200000 --duration-minutes 90
orbit schedule add "Design review" --start-ms 1792566600000 --duration-minutes 60 --task TASK_ID
orbit schedule list --from-ms 1792393800000 --to-ms 1792566600000
orbit schedule reschedule SCHEDULE_ID --start-ms 1792570200000
```

Blocks default to movable; `--fixed` makes them immutable to rescheduling.
Task and habit targets must already exist and are mutually exclusive. Times
are Unix milliseconds.

## Common errors

- **Unknown ID** — list projects/tasks and copy the stable ID.
- **Blocked work** — inspect `task ready` and `task graph`; do not force a
  status transition.
- **Wrong database** — run `orbit status` and check `--db`/`ORBIT_DB`.
- **Empty plan** — verify `--now-ms`, planning metadata and available minutes.
- **Criterion not satisfied** — set it explicitly after review; health does not
  infer it from task completion.

Next: [habits, routines and schedules](habits-routines-schedules.md),
[backup and recovery](backup-and-recovery.md), or
[development projects](development-projects.md).
