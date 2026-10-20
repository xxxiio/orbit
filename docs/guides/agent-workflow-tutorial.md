# Tutorial: from a task to an accepted agent handoff

This is a user workflow, not a milestone report. Coding agents are optional:
projects, tasks, notes and blueprints work without Herdr. Real local execution
requires a configured Herdr runtime and an agent provider.

The ten steps below separate **job state**, **session state**, **evidence**,
**review** and **Orbit task completion**. They are not interchangeable.

## 1. Prepare the repository and workspace

Use a dedicated or disposable Git repository while learning, never your main
worktree. Confirm it is clean and that you can recover it:

```sh
cd /absolute/path/to/disposable/repo
git status --short
git rev-parse --show-toplevel
```

Use a dedicated Orbit database if you do not want agent jobs in your usual
workspace:

```sh
orbit --db "$HOME/.orbit/agent-demo.db" init
export ORBIT_DB="$HOME/.orbit/agent-demo.db"
```

Do not put credentials, tokens or private data in the repository or in task
titles.

## 2. Configure Herdr if real execution is needed

```sh
orbit doctor
orbit agent start --help
```

`doctor` reports whether Herdr is discoverable. A missing Herdr does not affect
local project/task work; it means `agent start` cannot execute a real session.
Configure the runtime and provider according to your environment before
continuing. Orbit does not install Herdr for you.

## 3. Create the task and record acceptance criteria

```sh
orbit project add "Agent demo"
orbit project status PROJECT_ID active
orbit task add "Add request validation" --project PROJECT_ID
```

This build has no task acceptance-criteria field. Use a linked note:

```sh
orbit note create acceptance.md --id note-agent-acceptance --body "# Acceptance
- Invalid payloads return a typed error
- Existing valid requests are unchanged
- Tests are recorded in the handoff"
orbit note reference note-agent-acceptance task:TASK_ID
```

Or a project criterion:

```sh
orbit project configure PROJECT_ID --mode standard \
  --criterion validation=Validation behavior is reviewed
```

Tell the agent which approach is authoritative. Do not accept a summary as a
substitute for the criteria.

## 4. Start the agent job

```sh
orbit agent start TASK_ID \
  --repository /absolute/path/to/disposable/repo \
  --agent codex \
  --machine local \
  --worktree
```

- `--repository` is required.
- `--agent` defaults to the configured provider or `codex`.
- `--machine` defaults to `local`.
- `--worktree` asks Herdr to create an isolated Git worktree first.

`start` is an external side effect. It records a job, launch claim and context.
If the result is unknown, **reconcile**; do not start a second job to "fix" it.

## 5. Monitor progress

```sh
orbit agent overview
orbit agent attention
orbit agent status JOB_ID
```

`overview` is a portfolio catch-up with handoffs and review decisions.
`attention` lists jobs that need a human. `status` reads persisted state without
contacting the runtime.

A job can be queued, running, waiting, terminated or in review. Session state
describes the external runtime; it is not task state.

## 6. Inspect the session and capture evidence

```sh
orbit agent inspect JOB_ID --revision REVISION --at-ms AT_MS
orbit agent capture JOB_ID --max-commits 20
orbit agent handoffs JOB_ID
```

`inspect` reads a previously launched session and never launches another.
`capture` writes read-only Git evidence into a durable handoff. It does not
prove tests passed and does not accept the work.

An agent may also submit a structured handoff:

```sh
orbit agent handoff JOB_ID --file handoff.json
```

The JSON must include `summary`, `validation` and `next_steps`. Review the
actual commits, changed files and validation output, not just the summary.

## 7. Review the handoff

```sh
orbit agent handoffs JOB_ID
orbit agent review-status JOB_ID
orbit agent review JOB_ID \
  --handoff-id HANDOFF_ID \
  --reviewer reviewer-id \
  --decision approved \
  --reason "Acceptance criteria independently checked"
```

The review decision is recorded on the job. `approved` does not complete the
Orbit task. The command response explicitly reports `task_completed: false`.

## 8. Request changes

```sh
orbit agent review JOB_ID \
  --handoff-id HANDOFF_ID \
  --reviewer reviewer-id \
  --decision changes_requested \
  --reason "Return a typed error for malformed payloads"

orbit agent continue JOB_ID --message "Address the failing acceptance check"
```

`changes_requested` is a review decision, not a task status. `continue` sends a
message to an **existing confirmed session** and never starts a second agent.
Delivery can be ambiguous; do not retry blindly. If the session is terminal,
resume from the durable handoff instead.

## 9. Resume unfinished work

```sh
orbit agent resume-context JOB_ID
orbit agent resume JOB_ID
```

`resume-context` builds a resume brief without launching anything. `resume`
starts a successor job from a terminal predecessor and its persisted handoff.
Use `orbit agent sweep` to reconcile claimed local sessions and capture
reviewable evidence across jobs.

Never create two active jobs for the same task to work around an uncertain
session. Reconcile the existing job first.

## 10. Complete the Orbit task explicitly

After the evidence and acceptance criteria are satisfied:

```sh
orbit task complete TASK_ID
orbit project criterion PROJECT_ID validation satisfied
orbit project health PROJECT_ID
```

This is the only step that changes task state. A finished agent session, an
`approved` review or a clean handoff does **not** run `task complete`.

## Why session completion is not task completion

| Concept | What it means | What it does not mean |
| --- | --- | --- |
| Agent session | Herdr process ended or is idle | Work satisfies the task |
| Job state | Orbit's durable record of the attempt | Task is accepted |
| Handoff | Persisted summary/validation/next steps | Independent proof |
| Review | Human decision on a handoff | Task completion |
| Task status | Orbit's accepted work state | Agent process outcome |

The separation prevents an agent from silently declaring success and prevents a
lost or ambiguous session from closing work that was never reviewed.

## Connected mode notes

In connected mode, supported mutations may queue while offline. A pending
command is **not accepted** state. Agent launch requests are never executed
locally merely because they were queued; machine workers fence execution.
Inspect with `orbit agent pending` and reconcile through the authority. See
[offline operation](offline-operation.md).

## Common failures

- **`start` fails** — check task ID, repository path, Herdr availability and
  whether an active job already exists.
- **Unknown launch result** — inspect the job and Herdr session; do not relaunch.
- **No handoff** — capture Git evidence or have the agent submit the required
  JSON.
- **Review conflicts** — a different decision is already recorded for the same
  handoff; inspect `review-status`.
- **Task still open after approval** — expected; run `task complete`.

Next: [agent review and resumption](agent-review-and-resumption.md),
[optional machine workers](worker-deployment.md), or
[troubleshooting](troubleshooting.md).
