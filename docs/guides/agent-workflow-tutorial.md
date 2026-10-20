# Tutorial: From a development task to an accepted agent handoff

This is a *user workflow*, not a development milestone report. A coding agent
is optional; projects, tasks, and blueprints work without Herdr.

## 1. Set up local Orbit

Install `orbit` using [installation](../getting-started/installation.md).
Confirm the version's supported command syntax using `orbit --help`,
`orbit project --help`, and `orbit task --help`. Select a separate local database
for experiments, not an existing work database.

## 2. Create a project and choose its workflow

Create the project using `orbit project` and note its returned project ID.
Discover the built-in development blueprint and inspect it:

```sh
orbit blueprint bundled
orbit blueprint install development
orbit blueprint list
```

Bind the version installed by your command (replace the placeholder ID):

```sh
orbit blueprint bind PROJECT_ID development 1
orbit blueprint binding PROJECT_ID
```

Blueprint-defined work items and ordinary Orbit tasks are **not interchangeable**.
The agent launch command below operates on an Orbit task ID. Define the task
and its acceptance criteria with `orbit task --help` before delegating.

## 3. Launch explicitly (only if Herdr is configured)

Read `orbit agent start --help`. Enroll/configure the local Herdr runtime and
choose an expendable test repository first. Do not point a smoke run at your
main worktree or use a privileged account.

```sh
orbit agent start TASK_ID --agent codex --machine local \
  --repository /absolute/path/to/disposable/repo --worktree
```

Orbit records a job, launch claim, and context. Herdr owns the session. An
unknown launch result **must be reconciled**, not blindly retried.

## 4. Monitor, inspect, and review

```sh
orbit agent overview
orbit agent attention
orbit agent status JOB_ID
orbit agent handoffs JOB_ID
```

Review the actual Git commits, changed files, and validation reports. An
`idle` or `done` session is **not** proof that the task's acceptance criteria
are satisfied. Use `orbit agent review --help` to record the explicit review
outcome; task completion is a separate operation.

## 5. Continue work from a durable handoff

```sh
orbit agent resume-context JOB_ID
orbit agent resume JOB_ID
```

Resume creates a successor execution from persisted context when eligible;
`orbit agent continue` sends a message to an existing confirmed session.
Never create two active jobs for the same task to work around an uncertain
session status.

## Connected mode notes

Connected clients queue supported authoritative mutations while offline.
Pending commands are **not** accepted state and external actions are never
executed locally merely because they were queued. Use `orbit agent pending`
and the authority reconciliation workflow for pending commands. Only enrolled
trusted machine workers invoke Herdr, as described in
[worker deployment](worker-deployment.md).

For full command signatures, use the installed version's `--help` rather than
assuming example IDs or arguments are literally valid in all releases.
