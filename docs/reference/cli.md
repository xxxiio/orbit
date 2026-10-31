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
| `orbit attachment` | Attach, list, inspect, export, remove and flush file attachments |
| `orbit backup` | Create, list, inspect, verify and restore authoritative workspace backups |
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
the M26 manifest and creates a **complete, verified workspace recovery point**
(the whole `.orbitbak` — database, vault, referenced attachments and normalized
non-secret configuration — under `~/.orbit/backups/`) before replacing binaries.
If the recovery point cannot be created, verified, inspected or deep-verified,
the update is refused and no binaries are replaced; with no workspace database
(a fresh install) the step is skipped. Update preserves the prior binary/data on
failure and applies the stored retention policy after a verified recovery
point, always protecting the new one. See
[upgrades and recovery](../releasing/upgrades-and-recovery.md).

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
| `orbit blueprint upgrade-preview` | `PROJECT --id ID --version N` (read-only version-upgrade report with blockers and a fingerprint) |
| `orbit blueprint upgrade` | `PROJECT --id ID --version N --fingerprint HEX --confirm` (apply exactly the approved preview) |
| `orbit blueprint unbind` | `PROJECT` (remove the binding; generated default views apply) |
| `orbit blueprint binding` | `PROJECT` |
| `orbit blueprint create-item` | `PROJECT ID KIND [--properties JSON]` |
| `orbit blueprint item` | `ID` |
| `orbit blueprint transition` | `ID STATE --revision N` |
| `orbit blueprint set-properties` | `ID --revision N --file FILE` |
| `orbit blueprint apply-template` | `PROJECT TEMPLATE NAMESPACE` |
| `orbit blueprint capability list` | `[--project ID]` |
| `orbit blueprint capability install` | `CAPABILITY_ID` |
| `orbit blueprint capability uninstall` | `CAPABILITY_ID` (also removes its project grants) |
| `orbit blueprint capability authorize` | `CAPABILITY_ID --project ID --confirm` |
| `orbit blueprint capability revoke` | `CAPABILITY_ID --project ID` |
| `orbit blueprint action offers` | `ITEM_ID` |
| `orbit blueprint action request` | `ITEM_ID --action ID --revision N [--command ID] [--input JSON] [--confirm]` |
| `orbit blueprint action execute` | `COMMAND_ID` |
| `orbit blueprint action show` | `COMMAND_ID` |
| `orbit blueprint action list` | `[--item ID] [--state STATE] [--limit N]` |
| `orbit blueprint action reconcile` | `COMMAND_ID --succeed\|--fail --confirm` |

`--offline` and `--require-online` are group-level options for connected
behavior. `workflow` is accepted as a user-facing alias for `blueprint`, for
example `orbit workflow apply PROJECT basic`. A workflow cannot authorize its
own capabilities; the local CLI exposes explicit capability administration and
action commands. Every new project is already bound to `basic` (version 1) by
the authority, so use `blueprint apply` to change it and `blueprint bind` only
for an unbound project. Binding a new version does not migrate existing work
items. Changing or unpinning a workflow is local-authoritative only in this
build; connected mode reports that clearly instead of queueing an unsupported
mutation.

### Workflow version upgrades

A published version is immutable and a binding is pinned, so moving an existing
project to a newer version is an explicit operation. `upgrade-preview` is a pure
read; `upgrade` writes only after its own fences pass.

```sh
# Read-only: current/target bindings, per-kind delta, views, warnings, blockers,
# accepted revision and the preview fingerprint.
orbit blueprint upgrade-preview PROJECT_ID --id ID --version N --json

# Apply exactly the approved preview. --confirm pins and the fingerprint is
# recomputed inside the same savepoint that writes the binding.
orbit blueprint upgrade PROJECT_ID --id ID --version N \
  --fingerprint HEX --confirm --json
```

The preview result exposes `schema`, `project_id`, `project_revision`,
`current` (`null` when the project has no binding), `target`, `kinds`, `views`,
`warnings`, `blockers` and `fingerprint`. Each notice is
`{code, message, subjects}`; `subjects` names the live item ids or schema
identifiers the finding came from.

`upgrade` succeeds only when `--confirm` is present, the recomputed preview has
no blockers, and the recomputed `fingerprint` equals `--fingerprint`. It echoes
the approved report and adds `binding`, `previous` and `already_applied`.
Repeating the same approved upgrade is idempotent (`already_applied: true`,
binding unchanged). Once the project is already pinned to the target version the
command is a no-op success and does not compare the fingerprint, because there is
no approved change left to apply. Nothing is rebound by `blueprint import`,
`install` or `apply`, and other projects are never touched.

Exit codes: missing `--confirm` is a usage error (`2`); a blocker, an
unpublished target or a fingerprint mismatch is an authority conflict (`4`) with
the reason on stderr and no binding written. `upgrade-preview` exits `0` even
when it reports blockers, because the report is the success case. Both commands
are local-authoritative only in this build; connected mode refuses them instead
of queueing a change it cannot verify. See
[Build your own project workflow](../guides/custom-blueprints.md#6-updating-existing-projects)
for a runnable two-version example and the full warning/blocker code list.

### Capability administration and actions

These commands are local-authority only. Connected mode refuses them with
`capability actions require the local authoritative workspace; connected parity
is tracked as M25 EC-004` and does not queue a request. There are no capability
MCP tools in this iteration.

Installation records the trusted composition's contract; it grants no
permission. Authorization is a separate project-scoped grant:

```sh
orbit blueprint capability install orbit.reference.work-item-note
orbit blueprint capability list --json
orbit blueprint capability authorize orbit.reference.work-item-note \
  --project PROJECT_ID --confirm
orbit blueprint capability list --project PROJECT_ID --json
orbit blueprint capability revoke orbit.reference.work-item-note --project PROJECT_ID
orbit blueprint capability uninstall orbit.reference.work-item-note
```

`capability list` rows contain `capability_id`, `provider`, `permission`,
`installed`, `executor_registered`, `actions`, `requires`,
`authorized_projects` and `available`. `install` and `uninstall` return the
installation or `{"capability_id": ..., "removed": bool}`. `authorize` returns
the grant (`capability_id`, `project_id`, `granted_by`, `granted_at_ms`) and
requires `--confirm`. `revoke` returns
`{"capability_id": ..., "project_id": ..., "revoked": bool}`.

Action commands are a lifecycle, not one command with a retry flag:

```sh
orbit blueprint action offers ITEM_ID --json
orbit blueprint action request ITEM_ID --action append_note --revision 0 \
  --command cmd-note-1 --input '{"note":"first durable note"}' --json
orbit blueprint action show cmd-note-1 --json
orbit blueprint action execute cmd-note-1 --json
orbit blueprint action list --item ITEM_ID --state succeeded --limit 10 --json
orbit blueprint action reconcile cmd-note-1 --succeed --confirm --json
```

- `offers` returns each action with `action_id`, `capability`, `description`,
  `from`, `to`, `input_schema`, `declared_parameters`, `external_effect`,
  `requires` and `availability`. Availability is either
  `{"state":"available"}` or `{"state":"unavailable","code":...,"reason":...}`;
  codes are `capability_missing`, `capability_disabled`,
  `capability_not_installed`, `executor_missing`, `requirement_missing` and
  `not_authorized`.
- `request` re-checks everything and writes `state: "pending"` with the stable
  `command_id`, `item_id`, `item_revision`, `action_id`, `capability` and
  `plan_json`. If `--command` is omitted, the CLI generates one. Inputs must be
  keys declared in the transition action's `parameters`. `--confirm` is
  required for effectful or destructive actions.
- `execute` claims the pending request once, dispatches the trusted provider and
  returns the receipt: `command_id`, `item_id`, `action_id`, `capability`,
  `provider`, `outcome`, `detail`, `recorded_at_ms`. Replaying `execute` returns
  the same receipt without dispatching again.
- `show` returns `{"request": {...}, "receipt": {...}|null}`.
- `list` returns request rows and accepts an item, state and limit filter.
- `reconcile` resolves only `claimed`-without-receipt or `uncertain` requests,
  requires `--confirm` and one of `--succeed`/`--fail`, and returns
  `{"recovered_from":"claimed"|"uncertain","receipt":{...}}`. A `pending`
  request is refused; `succeeded` and `failed` receipts are not rewritten.

Refusals use the normal error contract: an unavailable offer, a missing
`--confirm`, an unknown action or the connected-mode refusal exits `2`; an
invalid persisted input, a revision mismatch or an unreconcilable state exits
`4`.

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

## Attachment commands

```text
orbit attachment add OWNER_KIND OWNER_ID PATH [--description TEXT] [--json]
orbit attachment list OWNER_KIND OWNER_ID [--json]
orbit attachment inspect ID [--json]
orbit attachment export ID DESTINATION [--json]
orbit attachment remove ID [--json]
orbit attachment flush [--json]
```

`OWNER_KIND` is one of `task`, `project`, `work_item`, `knowledge` or
`agent_job`. Content is immutable and content-addressed; `export` never
overwrites an existing destination.

In a local deployment every import is accepted synchronously. In a connected
deployment the CLI stages bytes in the replica, attempts one upload, and reports
acceptance only after a validated server receipt:

| Command | JSON shape |
| --- | --- |
| `add` | `{"accepted": bool, "pending": bool, "attachment": {...}}` |
| `list` | `{"accepted": [...], "staged": [{"attachment": {...}, "state": "pending\|conflict", "last_error": string\|null}]}` |
| `inspect` | `{"pending": bool, "verified_size_bytes": number, "attachment": {...}}` |
| `export` | `{"pending": bool, "destination": path, "attachment": {...}}` |
| `remove` | `{"removed": true, "pending": bool, "pending_discarded": bool, "content_removed": bool, "attachment_id": id}` |
| `flush` | `{"pending_before": n, "uploads_pending_before": n, "pending_after": n, "uploads_pending_after": n, "outcome": {...}}` |

`accepted` and `pending` are mutually exclusive for `add`. A pending upload is
never included in `accepted` or reported as an accepted attachment: `list`
keeps it in `staged`, and the human output marks it `[pending]`.

`flush` is connected-only. It flushes the ordered command outbox and then the
staged attachment uploads, printing the pending counts before and after; queued
commands and uploads are idempotent across retries. `outcome.status` is `empty`
(nothing queued), `flushed` (all accepted) or `blocked` (a command or upload
needs attention, including `blocked_command`/`reason`). A blocked or conflicted
upload is never counted as accepted. Local mode returns a clear usage error
instead of pretending there is an outbox.

Connected mode requires `ORBIT_SYNC_HTTP_TOKEN` and `ORBIT_AUTHORITY_DEVICE`
for server-backed operations. Without credentials, staged uploads remain
listable locally, but accepted reads, export of accepted content, and accepted
removal fail explicitly. Removing an accepted attachment is online-only in this
build; removing a staged upload discards it locally without contacting the
server. Server-side content garbage collection is not exposed to clients.

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
| `orbit agent complete` | `JOB_ID [--summary TEXT]`; terminalizes the job record only, never the task |
| `orbit agent interrupt` | `JOB_ID [--reason TEXT]`; interrupts the job record only, the external session may keep running |
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

`orbit agent complete` and `orbit agent interrupt` change the **Orbit job
record only**. They do not complete the Orbit task, delete handoffs or erase a
recorded review. Completion requires a non-terminal job and records the optional
`--summary`; interrupt records the optional `--reason` and requires a
non-terminal job. Neither command stops the external Herdr session: the runtime
has no stop verb, so the session may keep running, and the JSON result reports
`task_completed: false` and `runtime_stopped: false`. Close or reconcile a live
session out of band. A terminal job is refused with an actionable error instead
of being completed twice; use `resume` to create a successor from a terminal
predecessor with a durable handoff.

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

## Backup commands

`orbit backup` manages complete, verifiable workspace archives covering the
authoritative SQLite snapshot, the Markdown vault, the referenced attachment
objects and normalized non-secret configuration. The archive format is specified
in [the backup format reference](backup-format.md); the operator procedure is in
[backup and recovery](../guides/backup-and-recovery.md).

```text
orbit backup create [--output DIR] [--label TEXT]
                    [--source-mode local|server|replica] [--no-config]
orbit backup list [--output DIR] [--limit N]
orbit backup inspect FILE
orbit backup verify FILE [--structural-only]
orbit backup restore FILE [--dry-run]
                         [--to DIR | --replace --confirm REPLACE|DATABASE_PATH]
                         [--fast-preflight]
orbit backup schedule show|set --mode disabled|daily|weekly|before-upgrade
                         [--max-retained N] [--min-retention-days N]
                         [--max-total-bytes N]
orbit backup retention apply [--dry-run]
```

| Subcommand | Important flags and behaviour |
| --- | --- |
| `create` | Writes one staged, verified archive and publishes it atomically. Default destination is the configured Orbit backups directory (`~/.orbit/backups`); a home-less process with `--db` uses `<workspace>/backups`. `--output` selects another directory, `--label` names the backup, `--source-mode local\|server` declares the authority (`replica` is reserved and rejected in this build), `--no-config` omits the configuration projection. Never mutates the source workspace. A server-mode manifest records `source.excluded_client_pending_data: true` and a `client_pending_data` exclusion. |
| `list` | Lists archives newest first from `--output` (or the default directory), with `--limit N`. Shows creation time, format version, size, verification state and label. |
| `inspect` | Prints the manifest summary and component inventory without verifying the archive. |
| `verify` | Re-checks structure, manifest, per-file hashes, SQLite integrity, schema compatibility, referenced attachments and vault consistency. `--structural-only` skips the deep SQLite extraction check. |
| `restore` | `--dry-run` prints the plan and changes nothing; `--to DIR` restores into an isolated workspace; `--replace --confirm REPLACE` (or the exact database path) is the maintenance replacement path and refuses a live/active workspace. `--fast-preflight` skips deep verification during preflight. A connected client cannot replace a server authority. |
| `schedule` | Shows or updates the automatic-backup policy in `policy.json`: `--mode disabled\|daily\|weekly\|before-upgrade`, `--max-retained N`, `--min-retention-days N`, `--max-total-bytes N`. The long-running server re-reads the file every 15 minutes and creates verified automatic backups when due; the change takes effect without a restart. `disabled` and `before-upgrade` never run on a timer; `before-upgrade` is exercised by `orbit update`. |
| `retention` | Applies the stored retention policy now; `--dry-run` reports what it would remove without deleting. Cleanup runs only after a verified creation and protects the newest verified recovery point. |


Client replica snapshots are not supported as a recovery point in this build.
`orbit backup create --source-mode replica` exits nonzero with an actionable
message; back up the server workspace on the server host with
`--source-mode server`, and keep each replica's own pending outbox and staging
directory with that replica.

Verification failure and a refused restore preflight exit `1` with a rendered
diagnostic report. Backup, verification and recovery failures exit `4`
(`backup_error`); argument errors exit `2`. In JSON mode a failed create still
emits the tagged error object, and a failed verify prints the verification
report before the nonzero exit.

A successful create-and-verify also writes `<archive>.verify.json` beside the
archive; it records the verification result but is never transferred proof for a
copy that moved between machines. The archive and its sidecar should be kept
together.

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
| `1` | A rendered diagnostic failure: failed backup verification, failed restore preflight, or `doctor --strict` |
| `2` | Usage or domain-input error |
| `3` | Application/use-case failure |
| `4` | Local I/O, storage, backup or recovery failure (`backup_error`) |
| `70` | Internal serialization failure |

## Not available in this build

- No CLI subcommand for task descriptions, priority, labels or acceptance
  criteria; use the TUI or MCP tools. The domain and the connected authority
  store and queue these values. Attachments have the `orbit attachment` group
  described above.
- No standalone milestone create/list/status command.
- No project-switch command and no global active project.
- No connected-mode or MCP capability install/authorize/request/execute surface;
  capability actions are local CLI only in this iteration.
- No knowledge command group; notes and folders replace it.

See [configuration](configuration.md), [release assets](release-assets.md) and
the [documentation index](../README.md).
