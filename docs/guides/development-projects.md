# Development projects

A development project uses core tasks for delivery and, optionally, the
`development` blueprint for typed work items. The agent workflow is optional;
every step below works without Herdr.

## 1. Create and configure

```sh
orbit init
orbit project add "Payments service"
orbit project status PROJECT_ID active
orbit project configure PROJECT_ID \
  --mode standard \
  --objective "Ship a reliable payments service" \
  --signal tasks \
  --signal blockers \
  --criterion build=CI build is green \
  --criterion api=Public API is documented
```

Standard mode's defaults also include milestones and forecast, but this build
has no milestone CRUD. Track delivery through tasks and criteria.

## 2. Define work

```sh
orbit task add "Design the payment intent API" --project PROJECT_ID
orbit task add "Implement idempotent capture" --project PROJECT_ID
orbit task add "Add integration tests" --project PROJECT_ID
```

Record estimates and deadlines so planning is meaningful:

```sh
orbit task planning DESIGN_TASK_ID \
  --estimate-minutes 120 \
  --deadline-ms 1792566600000
orbit task planning CAPTURE_TASK_ID \
  --estimate-minutes 240 \
  --deadline-ms 1792653000000
```

## 3. Model dependencies

```sh
orbit task depends-on CAPTURE_TASK_ID DESIGN_TASK_ID
orbit task depends-on TESTS_TASK_ID CAPTURE_TASK_ID
orbit task ready --now-ms 1792480200000 --all
orbit task graph --now-ms 1792480200000 --all
```

The first task is blocked by the second. Use `preview-dependency` before a
change if you want to see the effect without mutating state.

## 4. Capture acceptance criteria

There is no task acceptance-criteria field. Choose and document one approach:

**Linked note**

```sh
orbit note create acceptance.md --id note-payments-acceptance --body "# Acceptance
- Capture is idempotent under retry
- Decline paths return documented errors
- Integration tests run in CI"
orbit note reference note-payments-acceptance task:CAPTURE_TASK_ID
```

**Project criterion**

```sh
orbit project criterion PROJECT_ID build satisfied
```

Do not mark either satisfied until the evidence has been reviewed.

## 5. Work the queue

```sh
orbit today
orbit task next --now-ms 1792480200000
orbit task day-plan --now-ms 1792480200000 --available-minutes 300
orbit task start DESIGN_TASK_ID
orbit task complete DESIGN_TASK_ID
```

Completion is explicit. A completed prerequisite can make the dependent task
actionable, but the dependent task still needs its own status transition.

## 6. Optional development blueprint

```sh
orbit blueprint bundled
orbit blueprint install development
orbit blueprint list
orbit blueprint bind PROJECT_ID development 1
orbit blueprint create-item PROJECT_ID feature feature --properties '{"priority":"high"}'
```

The bundled development blueprint defines a `feature` work item with a
`priority` choice field and states `backlog`, `ready`, `implementing`, `review`
and `done`. That is a **blueprint custom field**, not a core task priority.
Blueprint items and Orbit tasks are different entities; associate delivery
work with a task ID before delegating it to an agent.

See [blueprints versus tasks](blueprints-vs-tasks.md).

## 7. Delegate to an agent (optional)

Only after a task exists and a disposable or dedicated repository is ready:

```sh
orbit agent start TASK_ID \
  --repository /absolute/path/to/repo \
  --agent codex \
  --machine local \
  --worktree
```

Monitor and review:

```sh
orbit agent overview
orbit agent attention
orbit agent status JOB_ID
orbit agent handoffs JOB_ID
orbit agent review JOB_ID \
  --handoff-id HANDOFF_ID \
  --reviewer reviewer-id \
  --decision approved \
  --reason "Acceptance criteria independently checked"
```

An agent session ending is not task acceptance. Follow the
[agent workflow tutorial](agent-workflow-tutorial.md) and
[review guide](agent-review-and-resumption.md).

## 8. Release checklist for the project

- All required criteria are satisfied or explicitly waived.
- Tests and validation were inspected, not inferred from a summary.
- Dependency graph has no unfinished blockers for the release scope.
- Notes and vault are backed up with the database.
- Remaining work is recorded as tasks, not chat history.

## Common failures

- **Task is blocked** — inspect `task graph`, then complete the prerequisite.
- **Agent job will not start** — confirm Herdr, repository path and task ID;
  use `orbit doctor` and `orbit agent start --help`.
- **Agent says done but task is open** — expected; review evidence and run
  `orbit task complete`.
- **Blueprint item is not a task** — create or link an Orbit task explicitly.
- **Wrong workspace** — check `orbit status` and the `--db` selector.

Next: [agent workflow tutorial](agent-workflow-tutorial.md) or
[backup and recovery](backup-and-recovery.md).
