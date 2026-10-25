# Orbit CLI contract

`orbit` is a headless client of Orbit application services. The CLI does not
write SQLite tables directly. This page describes stable output and authority
contracts; see the [CLI reference](reference/cli.md) for the complete command
inventory and the [configuration reference](reference/configuration.md) for
deployment resolution.

Historical milestone notes may describe retired commands. The installed
`orbit --help` is authoritative for the build you are running.

## Workspace and deployment selection

Database precedence in local mode:

1. `--db <path>`
2. `ORBIT_DB`
3. `[storage].database` in `~/.orbit/config.toml`
4. `~/.orbit/orbit.db`

Configuration file precedence:

1. `--config <path>`
2. `ORBIT_CONFIG`
3. `~/.orbit/config.toml`

In connected mode, `--db`, `ORBIT_AUTHORITY_DB`, `ORBIT_DB` and
`[connected].replica_database` select the replica. A complete server URL and
replica are required; partial settings fail closed. A connected deployment never
falls back to a local-authoritative database.

Each database has a Markdown vault at `<database>.knowledge/`.

## Output contract

Human-readable output is the default. `--json` switches successful responses and
runtime errors to structured JSON.

```sh
orbit task add "Renew passport"
orbit task list
orbit task list --status in-progress
orbit task start TASK_ID
orbit task complete TASK_ID
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

The CLI respects domain transitions. `task start` may promote an inbox task to
in progress through the normal lifecycle; `task complete` does not silently skip
invalid states.

## Task and project lifecycle

```sh
orbit task status TASK_ID blocked
orbit task status TASK_ID review
orbit project status PROJECT_ID active
orbit project status PROJECT_ID on-hold
```

Invalid transitions are rejected by the domain model and returned through the
same human/JSON error contract. `task start` and `task complete` are convenience
transitions; `task status` accepts the full status enum.

There is no core task priority field and no standalone milestone CRUD command.
`task add --milestone` references an existing milestone only.

## List filtering and search

```sh
orbit task list --status ready --project PROJECT_ID --query passport
orbit project list --status active --query orbit
orbit habit list --active --query run
orbit schedule list --from-ms 1770000000000 --to-ms 1770100000000 --query focus
```

Cross-entity search is a case-insensitive match over stable IDs and display
titles/names:

```sh
orbit search orbit
orbit search run --from-ms 1770000000000 --to-ms 1770100000000
orbit search orbit --json
```

Schedule blocks are included in cross-entity search only when an explicit
`--from-ms`/`--to-ms` window is supplied. JSON search results use one stable
tagged shape with `kind` plus `item`.

## Workspace diagnostics

`orbit status` is the headless workspace health surface. It reports:

- the resolved database path;
- the current schema version;
- read-only integrity health and detected issues;
- effective SQLite WAL, foreign-key and busy-timeout policy;
- current project/task/habit and other entity counts.

`orbit status --json` uses the stable `workspace_status` response type.
Diagnostics never repair or mutate corrupted state.

`orbit doctor` is the installation diagnostic: executable, PATH resolution,
configuration, database integrity, authority and optional tools. It is
read-only and never prints tokens.

## Exit codes

Orbit reserves stable runtime exit classes for scripts and agents:

| Code | Meaning |
| ---: | --- |
| `0` | success |
| `2` | usage or domain-input error |
| `3` | application/use-case failure |
| `4` | local I/O or repository/storage failure |
| `70` | internal serialization failure |

Clap argument/parser failures use exit code `2`. JSON mode changes error
formatting, not exit semantics.

## Task planning metadata

```sh
orbit task planning TASK_ID \
  --estimate-minutes 45 \
  --earliest-start-ms 100 \
  --deadline-ms 500
```

The command replaces planning metadata atomically and records an activity.
Omitting a field clears it. Times are Unix milliseconds.

## Project health and evidence

```sh
orbit project health PROJECT_ID
orbit project health PROJECT_ID --lookback-minutes 10080
orbit project health PROJECT_ID --json
```

Health is deterministic and evidence-first. It returns the current project
rollup, blocked counts, recent activity and an explainable health state with
reason codes. It does not invent forecasts, effort actuals or progress history.

Metric observations are explicit time-stamped values:

```sh
orbit project metric target METRIC_ID BASELINE TARGET at-most
orbit project metric observe METRIC_ID VALUE --observed-at-ms 1770000000000
orbit project evidence bind PROJECT_ID metric METRIC_ID
```

## Habit occurrences and routines

```sh
orbit habit check HABIT_ID
orbit habit check HABIT_ID --duration-minutes 30
orbit habit check HABIT_ID --at-ms 1770000000000 --duration-minutes 30
orbit habit history HABIT_ID
```

`habit check` rejects paused habits. History returns occurrence IDs, timestamps
and optional time-spent evidence in chronological order. Automatic recurrence
and streak extraction are not implemented in this build.

Routines are organizational metadata:

```sh
orbit routine add "Morning"
orbit routine list
orbit routine add-habit ROUTINE_ID HABIT_ID
orbit routine remove-habit ROUTINE_ID HABIT_ID
```

Routine membership does not pause, resume, complete or schedule a habit.

## Markdown notes and folders

The Markdown vault is file-authoritative:

```sh
orbit note create acceptance.md --id note-acceptance --body "# Acceptance"
orbit note read note-acceptance
orbit note update note-acceptance --file revised.md
orbit note kind note-acceptance decision
orbit note status note-acceptance resolved
orbit note reference note-acceptance task:TASK_ID
orbit note reference-backlinks task:TASK_ID
orbit note catalog --kind decision --status active
orbit note search "acceptance"
orbit note export-dir ./notes-export
orbit folder tree
orbit folder create docs
orbit folder move docs archive/docs
```

There is no knowledge command group in this build. The note and folder commands
replace the retired structured-knowledge interface. A linked note or a project
success criterion is the supported way to record task acceptance criteria.

## Trusted capability actions

Capability administration and action commands are local-authority only in this
iteration. Connected mode refuses the whole `capability` and `action` groups
with `capability actions require the local authoritative workspace; connected
parity is tracked as M25 EC-004`; it does not queue an unexecutable request.

Installation, authorization, request and execution are separate facts and must
happen in that order:

```sh
orbit blueprint capability install CAPABILITY_ID
orbit blueprint import MANIFEST.json
orbit blueprint apply PROJECT_ID BLUEPRINT_ID
orbit blueprint capability authorize CAPABILITY_ID --project PROJECT_ID --confirm
orbit blueprint action offers ITEM_ID --json
orbit blueprint action request ITEM_ID --action ACTION_ID --revision N \
  --command COMMAND_ID --input '{"note":"..."}' --json
orbit blueprint action execute COMMAND_ID --json
```

Publication is gated by durable installation: importing a manifest whose
`required_capabilities` are unavailable fails with
`unavailable capability: <id>`. Installation alone grants no project authority;
`authorize` creates one project grant and requires `--confirm`. An offer is
advisory and is re-checked when the request is persisted.

Stable JSON shapes:

- offer availability is `{"state":"available"}` or
  `{"state":"unavailable","code":"...","reason":"..."}` with codes
  `capability_missing`, `capability_disabled`, `capability_not_installed`,
  `executor_missing`, `requirement_missing` and `not_authorized`;
- a request carries `state: "pending"`, a stable `command_id`, the item revision,
  the action/capability and `plan_json`;
- a receipt carries `outcome` (`succeeded`, `failed` or `uncertain`), `provider`,
  `command_id`, `item_id`, `action_id`, `detail` and `recorded_at_ms`;
- `show` returns the request plus `receipt` (or `null`);
- `reconcile` returns `{"recovered_from":"claimed"|"uncertain","receipt":{...}}`.

A request is intent, not execution. `execute` claims once and never re-dispatches
a recorded receipt. `failed` and `uncertain` outcomes are not retried
automatically; `claimed` without a receipt and `uncertain` are resolvable only
with `orbit blueprint action reconcile COMMAND_ID --succeed|--fail --confirm`.
A `pending` request cannot be reconciled.

Exit behaviour follows the normal classes: unavailable actions, missing
`--confirm` and the connected refusal exit `2`; invalid inputs, revision
conflicts and unreconcilable action states exit `4`. `--json` changes formatting,
not exit semantics.

## Workflow version upgrades

Publishing or installing a newer manifest never rebinds a project, and
`orbit blueprint apply` keeps its original meaning (replace the whole workflow,
no compatibility check). A version change on an existing project uses a separate
preview/apply pair:

```sh
orbit blueprint upgrade-preview PROJECT_ID --id BLUEPRINT_ID --version N --json
orbit blueprint upgrade PROJECT_ID --id BLUEPRINT_ID --version N \
  --fingerprint HEX --confirm --json
```

Stable JSON shapes:

- the preview result carries `schema` (`orbit.blueprint-upgrade-preview.v1`),
  `project_id`, `project_revision`, `current` (`null` when unbound), `target`,
  `kinds`, `views`, `warnings`, `blockers` and `fingerprint`;
- every warning/blocker is `{code, message, subjects}` where `subjects` is the
  list of live work-item ids or schema identifiers the finding was computed
  from. Blocker codes are `kind_removed_with_live_items`,
  `state_removed_with_live_items`, `field_removed_with_values`,
  `field_type_incompatible`, `field_choice_value_unsupported`,
  `field_required_without_default`, `field_now_required_missing_value`,
  `property_not_in_target_schema`, `required_capability_unavailable` and
  `target_validation_failed`;
- `upgrade` echoes the approved report and adds `binding`, `previous` (or
  `null`) and `already_applied`;
- `views` is `{added, removed, changed}` and never blocks an upgrade;
- `warnings` includes `transition_removed`, `action_removed`,
  `field_type_widened`, `field_choice_options_trimmed` and
  `target_version_not_newer` for changes that keep existing items valid.

The fingerprint is a hex SHA-256 over the canonical preview payload: current
binding, target id/version, both manifest digests, the accepted revision (the
command-ledger tip) and every live work item's id, kind, state, revision and
properties. It is recomputed inside the same savepoint as the binding write, so
a preview taken before another accepted change is refused, not applied.

Refusals write nothing: `--confirm` is mandatory (missing it exits `2`, a usage
error), and a blocker, unpublished target or fingerprint mismatch exits `4` with
the reason on stderr as `repository_error`. Replaying the identical approved
upgrade returns the same binding with `already_applied: true`; when the project is
already pinned to the target there is no approved change left, so the command is
a no-op success and does not compare the fingerprint. Upgrade commands are
local-authoritative only in this build; connected mode refuses them rather than
queueing a change whose preview it cannot verify.

## External effects

Agent launch, continuation and other external actions are not safe to retry
blindly. In local mode, `agent start` requires an existing task and repository
and a configured Herdr runtime. In connected mode, a queued launch is not proof
that an agent started; machine workers fence execution. A finished session or an
approved review does not complete the Orbit task.

## CLI acceptance contract

Compiled acceptance workflows use one temporary SQLite workspace and prove:

1. `task add --json` creates an inbox task;
2. `task list --query ... --json` discovers it;
3. `task start --json` transitions it through domain rules;
4. `search ... --json` returns a typed result;
5. `status --json` reports a healthy workspace and expected counts.

Source-bound documentation tests are not a substitute for compiled acceptance.
