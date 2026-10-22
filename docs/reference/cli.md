# CLI reference

`orbit` is the single public primary CLI. It is local-first; a connected
deployment uses a replica and the `sync` group. Run `orbit --help`, `orbit
<group> --help` and `orbit <group> <subcommand> --help` for the exact interface
of the installed build.

This reference describes the current compiled surface. Empty cells mean there
are no group-specific flags beyond the global options.

## Global options

| Option | Purpose |
| --- | --- |
| `--db PATH` | Select the local SQLite workspace |
| `--config PATH` | Select the configuration file (default `~/.orbit/config.toml`; env `ORBIT_CONFIG`) |
| `--json` | Emit a stable JSON response instead of human-readable text |
| `-h, --help` | Show help |
| `-V, --version` | Show the binary version |

Global options may appear before or after the subcommand where clap permits it.
`--json` changes formatting, not exit semantics.

## Top-level commands

| Command | Purpose |
| --- | --- |
| `orbit init` | Create the home layout, configuration and database |
| `orbit doctor` | Read-only installation and workspace diagnostic |
| `orbit update` | Check for or install a CLI update |
| `orbit capabilities` | Discover deterministic planning capability contracts |
| `orbit project` | Projects, strategy, criteria, health and evidence |
| `orbit task` | Tasks, dependencies and planning |
| `orbit blueprint` | Versioned project workflows (compatibility command group) and typed work items |
| `orbit habit` | Habits and occurrences |
| `orbit routine` | Ordered groups of habits |
| `orbit note` | Markdown notes in the file-authoritative vault |
| `orbit folder` | Physical folder navigation and mutation |
| `orbit schedule` | Schedule blocks |
| `orbit agent` | Optional agent jobs, handoffs and reviews |
| `orbit sync` | Connected authority and offline replica |
| `orbit search` | Cross-entity search |
| `orbit status` | Workspace diagnostics and entity counts |
| `orbit today` | Current actionable queue |

## Setup and diagnostics

### `orbit init`

```text
orbit init [--database PATH] [--db PATH] [--config PATH] [--force]
           [--authority local|connected] [--server URL]
           [--replica-database PATH] [--agent-provider NAME]
           [--non-interactive] [--json]
```

Idempotent. Retains an existing configuration and database unless `--force`
rewrites the configuration. Conflicting flags on an existing config are refused
without `--force`. Connected mode requires both `--server` and a replica path.

### `orbit doctor`

```text
orbit doctor [--db PATH] [--config PATH] [--strict] [--json]
```

Read-only. Reports executable/version, PATH resolution, config validity,
effective database path, read-only integrity/schema, authority mode, connected
server/replica and optional tools. `--strict` exits `1` when any error-severity
finding is present. Token values are never printed.

### `orbit update`

```text
orbit update [--check] [--version VERSION] [--channel stable|prerelease]
             [--prefix DIR] [--allow-downgrade] [--force]
```

| Flag | Effect |
| --- | --- |
| `--check` | Report availability without mutating anything |
| `--version VERSION` | Install a specific version |
| `--channel stable\|prerelease` | Select the update channel; stable by default |
| `--prefix DIR` | Update an installation at a non-default prefix |
| `--allow-downgrade` | Permit an older version |
| `--force` | Reinstall even when already current |

Checksums are verified before replacement. Update refuses installations without
the M26 manifest, backs up the SQLite database under `~/.orbit/backups/`, and
preserves the prior binary/data on failure.

### `orbit capabilities`

```text
orbit capabilities [--id ID] [--accepted-only] [--json]
```

## Project commands

| Command | Purpose / important flags |
| --- | --- |
| `orbit project add` | Create a project: `NAME` |
| `orbit project list` | Filter: `--status`, `--query` |
| `orbit project status` | Transition: `ID STATUS` (`proposed`, `active`, `on-hold`, `completed`, `cancelled`, `archived`) |
| `orbit project configure` | `ID --mode MODE [--objective TEXT] [--signal SIGNAL]... [--criterion key=text]... [--optional-criterion key=text]...` |
| `orbit project criterion` | `ID KEY STATUS` (`pending`, `satisfied`, `waived`) |
| `orbit project health` | `ID [--lookback-minutes N]` |
| `orbit project evidence` | Nested: `list`, `bind`, `unbind`, `report` |
| `orbit project metric` | Nested: `target`, `observe` |

`project configure` modes: `standard`, `learning`, `habit-driven`,
`metric-target`, `maintenance`, `custom`. Signals: `milestones`, `tasks`,
`blockers`, `forecast`, `habit-consistency`, `time-invested`,
`learning-evidence`, `metric-value`, `knowledge-evidence`.

Nested evidence/metric forms:

```text
orbit project evidence list PROJECT_ID
orbit project evidence bind PROJECT_ID habit|metric EVIDENCE_ID
orbit project evidence unbind PROJECT_ID habit|metric EVIDENCE_ID
orbit project evidence report PROJECT_ID --start-ms N --end-ms N --as-of-ms N
orbit project metric target METRIC_ID BASELINE TARGET at-least|at-most
orbit project metric observe METRIC_ID VALUE --observed-at-ms N
```

## Task commands

| Command | Purpose / important flags |
| --- | --- |
| `orbit task add` | `TITLE [--project ID] [--milestone ID] [--id ID]` |
| `orbit task list` | `[--status STATUS] [--project ID] [--query TEXT]` |
| `orbit task start` | `ID` |
| `orbit task complete` | `ID` |
| `orbit task status` | `ID STATUS` |
| `orbit task planning` | `ID [--estimate-minutes N] [--earliest-start-ms N] [--deadline-ms N]` |
| `orbit task depends-on` | `ID PREREQUISITE` |
| `orbit task unblock` | `ID PREREQUISITE` |
| `orbit task ready` | `--now-ms N [--all]` |
| `orbit task graph` | `--now-ms N [--all]` |
| `orbit task project-dependencies` | `--now-ms N` |
| `orbit task portfolio-actions` | `--now-ms N` |
| `orbit task portfolio-capacity` | `--now-ms N --available-minutes N` |
| `orbit task compare-scenarios` | `--input FILE` |
| `orbit task review-scenario` | `--input FILE` |
| `orbit task review-scenario-options` | `--input FILE` |
| `orbit task review-schedule` | `--input FILE` |
| `orbit task apply-reviewed-schedule` | `--input FILE --confirm` |
| `orbit task project-handoffs` | `--now-ms N` |
| `orbit task next` | `--now-ms N [--limit N]` |
| `orbit task day-plan` | `--now-ms N --available-minutes N [--project ID] [--include-unsized] [--default-minutes N] [--max-tasks N]` |
| `orbit task schedule-plan` | `--starts-at-ms N --ends-at-ms N` |
| `orbit task workload` | `--now-ms N [--horizon-ms N]` |
| `orbit task work-brief` | `--now-ms N --ends-at-ms N [--project ID] [--limit N] [--default-minutes N]` |
| `orbit task planning-focus` | `ID --now-ms N --ends-at-ms N` |
| `orbit task planning-actions` | `ID --now-ms N --ends-at-ms N` |
| `orbit task resolve-planning-action` | `ID --now-ms N --ends-at-ms N --selection ID` |
| `orbit task handoff-planning-action` | `ID --now-ms N --ends-at-ms N --selection ID` |
| `orbit task handoff-planning-group-action` | `PROJECT --now-ms N --ends-at-ms N --selection ID` |
| `orbit task resolve-planning-group-action` | `PROJECT --now-ms N --ends-at-ms N --selection ID` |
| `orbit task planning-group-actions` | `PROJECT --now-ms N --ends-at-ms N` |
| `orbit task planning-group` | `PROJECT --now-ms N --ends-at-ms N` |
| `orbit task completion-impact` | `ID --now-ms N [--limit N]` |
| `orbit task preview-dependency` | `ID PREREQUISITE --now-ms N [--remove]` |

Task statuses: `inbox`, `planned`, `ready`, `in-progress`, `waiting`,
`blocked`, `review`, `deferred`, `done`, `cancelled`.

Planning-review commands that take `--input FILE` are read-only until
`apply-reviewed-schedule` writes. That command also needs `--confirm` and a
file with `approved=true`; a preview is not approval.

## Blueprint (workflow) commands

Orbit calls these manifests **workflows** in the TUI and in prose. The compiled
command group, crate names and persisted identifiers keep the `blueprint` name
for compatibility; the commands below are unchanged. Every new project is bound
to the bundled **Basic** workflow (version 1) automatically. The other bundled
workflows are Development, Research, Learning and Habits (version 2).

| Command | Purpose / important flags |
| --- | --- |
| `orbit blueprint pending` | List pending/conflicted blueprint mutations |
| `orbit blueprint flush` | Replay the canonical outbox |
| `orbit blueprint bundled` | List bundled packages |
| `orbit blueprint list` | List installed versions |
| `orbit blueprint show` | `ID VERSION` |
| `orbit blueprint install` | `ID` |
| `orbit blueprint validate` | `FILE` |
| `orbit blueprint import` | `FILE` (JSON or YAML) |
| `orbit blueprint export` | `ID VERSION [--file DEST]` |
| `orbit blueprint bind` | `PROJECT ID VERSION` (fails if the project is already bound) |
| `orbit blueprint apply` | `PROJECT ID [--version N]` (replace the current workflow; omitted version uses the bundled/latest published revision) |
| `orbit blueprint unbind` | `PROJECT` (remove the binding; generated default views apply) |
| `orbit blueprint binding` | `PROJECT` |
| `orbit blueprint create-item` | `PROJECT ID KIND [--properties JSON]` |
| `orbit blueprint item` | `ID` |
| `orbit blueprint transition` | `ID STATE --revision N` |
| `orbit blueprint set-properties` | `ID --revision N --file FILE` |
| `orbit blueprint apply-template` | `PROJECT TEMPLATE NAMESPACE` |

`--offline` and `--require-online` are group-level options for connected
behavior. `workflow` is accepted as a user-facing alias for `blueprint`, for
example `orbit workflow apply PROJECT basic`. A workflow cannot authorize its
own capabilities; action execution is not exposed in this build. Every new
project is already bound to `basic` (version 1) by the authority, so use
`blueprint apply` to change it and `blueprint bind` only for an unbound
project. Binding a new version does not migrate existing work items. Changing
or unpinning a workflow is local-authoritative only in this build; connected
mode reports that clearly instead of queueing an unsupported mutation.

## Habit, routine, note, folder and schedule commands

| Command | Purpose / important flags |
| --- | --- |
| `orbit habit add` | `NAME [--weekly-target 1..7]` |
| `orbit habit list` | `[--active] [--paused] [--query TEXT]` |
| `orbit habit pause` | `ID` |
| `orbit habit resume` | `ID` |
| `orbit habit check` | `ID [--at-ms N] [--duration-minutes N] [--occurrence-id ID]` |
| `orbit habit history` | `ID` |
| `orbit routine add` | `NAME` |
| `orbit routine list` | |
| `orbit routine add-habit` | `ROUTINE_ID HABIT_ID` |
| `orbit routine remove-habit` | `ROUTINE_ID HABIT_ID` |
| `orbit note create` | `PATH --id ID [--body TEXT\|--file FILE]` |
| `orbit note read` | `ID` |
| `orbit note update` | `ID --file FILE [--expected-file FILE]` |
| `orbit note move` | `ID PATH` |
| `orbit note kind` | `ID KIND` |
| `orbit note status` | `ID STATUS` |
| `orbit note link` | `ID TARGET` |
| `orbit note unlink` | `ID TARGET` |
| `orbit note reference` | `ID project:ID\|task:ID\|habit:ID` |
| `orbit note unreference` | `ID TARGET` |
| `orbit note reference-backlinks` | `TARGET` |
| `orbit note list` | |
| `orbit note catalog` | `[--kind KIND] [--status STATUS]` |
| `orbit note search` | `QUERY` |
| `orbit note import-dir` | `SOURCE [--into PATH]` |
| `orbit note export-dir` | `DESTINATION` |
| `orbit note relations` | `ID` |
| `orbit note path` | `PATH` |
| `orbit note asset add` | `NOTE_ID FILE [--label TEXT]` |
| `orbit note asset list` | `NOTE_ID` |
| `orbit note asset verify` | `NOTE_ID` |
| `orbit note asset unlink` | `NOTE_ID ASSET` |
| `orbit folder tree` | |
| `orbit folder create` | `PATH` |
| `orbit folder move` | `FROM TO` |
| `orbit schedule add` | `TITLE --start-ms N --duration-minutes N [--fixed\|--movable] [--task ID\|--habit ID]` |
| `orbit schedule list` | `--from-ms N --to-ms N [--query TEXT]` |
| `orbit schedule reschedule` | `ID --start-ms N` |

Note kinds and statuses are enumerated by `orbit note kind --help` and `orbit
note status --help`. Schedule times and habit occurrence times are Unix
milliseconds.

## Agent commands

| Command | Purpose / important flags |
| --- | --- |
| `orbit agent start` | `TASK_ID --repository PATH [--agent NAME] [--machine NAME] [--worktree]` |
| `orbit agent launch` | `JOB_ID --revision N --claim-token TOKEN --at-ms N` |
| `orbit agent inspect` | `JOB_ID --revision N --at-ms N` |
| `orbit agent sweep` | `[--max-commits N]` |
| `orbit agent queue-start` | `TASK_ID --repository PATH [--agent NAME] [--machine NAME] [--worktree]` |
| `orbit agent queue-resume` | Queue a successor job |
| `orbit agent queue-continue` | Queue a message to an existing session |
| `orbit agent queue-handoff` | Queue a durable handoff |
| `orbit agent queue-review` | Queue a review decision; never completes the task |
| `orbit agent pending` | Inspect pending agent commands |
| `orbit agent flush` | Submit queued commands to the authority |
| `orbit agent status` | `JOB_ID` |
| `orbit agent handoff` | `JOB_ID --file FILE` |
| `orbit agent continue` | `JOB_ID --message TEXT` |
| `orbit agent handoffs` | `JOB_ID` |
| `orbit agent resume-context` | `JOB_ID` |
| `orbit agent resume` | `JOB_ID` |
| `orbit agent capture` | `JOB_ID [--max-commits 1..100]` |
| `orbit agent review` | `JOB_ID --handoff-id ID --reviewer ID [--decision approved\|changes_requested] [--reason TEXT]` |
| `orbit agent review-status` | `JOB_ID` |
| `orbit agent overview` | |
| `orbit agent attention` | |

Agent execution requires Herdr and a configured provider. `start` and
`continue` are external side effects; reconcile uncertain results instead of
retrying. Review never completes an Orbit task.

## Sync commands

| Command | Purpose / important flags |
| --- | --- |
| `orbit sync replica` | Connected offline cache and outbox; requires `--db`, `--server`, and `--device`/`--token` for authenticated operations |
| `orbit sync authority` | Direct online authority commands; requires `--server`, `--device`, `--token` |

Replica subcommands include `status`, `snapshot`, `refresh`, `reconnect`,
`pending`, `flush`, `queue`, `receipt`, `audit`, `workspace-sync`,
`sync-markdown`, `search-markdown`, `cached-markdown`, `converge`,
`observe-convergence`, `review-conflict-online`, `review-conflict`,
`resolve-reviewed`, `resolve-reviewed-batch`, `convergence` and `accepted`.

Authority subcommands include `snapshot`, `domain-command`, `domain-read`,
`tasks`, `projects`, `project-create`, `create` and `status`.
`project-create` accepts `--workflow ID` to pin a workflow atomically at
creation; omitting it binds the Basic default.

Pending commands are not accepted state. See
[connected mode](../getting-started/connected-mode.md).

## Search, status and today

| Command | Purpose / important flags |
| --- | --- |
| `orbit search` | `QUERY [--from-ms N --to-ms N]` |
| `orbit status` | Workspace path, schema, integrity and counts |
| `orbit today` | Planned, ready and in-progress queue |

`search` requires both time bounds when schedule blocks should be included.

## Output and exit codes

Human-readable output is the default. `--json` emits stable tagged structures.
Runtime errors in JSON mode carry a `type` and `message`; exit classes are:

| Code | Meaning |
| ---: | --- |
| `0` | Success |
| `2` | Usage or domain-input error |
| `3` | Application/use-case failure |
| `4` | Local I/O or storage failure |
| `70` | Internal serialization failure |

## Not available in this build

- No CLI subcommand for task descriptions, priority, labels, acceptance
  criteria or attachments; use the TUI or MCP tools. The domain stores these
  values and they remain local-authoritative.
- No standalone milestone create/list/status command.
- No project-switch command and no global active project.
- No dedicated backup subcommand.
- No executable workflow/blueprint capability actions.
- No knowledge command group; notes and folders replace it.

See [configuration](configuration.md), [release assets](release-assets.md) and
the [documentation index](../README.md).
