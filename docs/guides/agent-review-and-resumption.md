# Agent results: inspect, review, continue and resume

Orbit separates **job state**, **agent session state**, **handoff evidence**,
**review** and **Orbit task completion**. Agent operations require an existing
Orbit task and, for real execution, a configured optional Herdr runtime.

## Start and observe

```sh
orbit agent start TASK_ID \
  --repository /absolute/path/to/disposable/repo \
  --agent codex \
  --machine local \
  --worktree

orbit agent overview
orbit agent attention
orbit agent status JOB_ID
```

`start` is an external side effect. Read the durable job state and reconcile the
Herdr session before retrying anything. Do not create a second active job for
the same task.

## Capture evidence

```sh
orbit agent handoffs JOB_ID
orbit agent capture JOB_ID --max-commits 20
orbit agent inspect JOB_ID --revision REVISION --at-ms AT_MS
```

`capture` records read-only Git evidence into a durable handoff and does **not**
prove tests passed. `inspect` never launches a second session. Use the job
revision from `status`; times are Unix milliseconds.

A submitted handoff must include structured `summary`, `validation` and
`next_steps` fields:

```sh
orbit agent handoff JOB_ID --file handoff.json
```

## Review

```sh
orbit agent review-status JOB_ID
orbit agent review JOB_ID \
  --handoff-id HANDOFF_ID \
  --reviewer reviewer-id \
  --decision approved \
  --reason "Acceptance criteria independently checked"
```

The supported decisions are `approved` and `changes_requested`. A review is
recorded once per job; a conflicting decision is rejected. Inspect the actual
changes and validation logs before deciding.

Review does **not** close the Orbit task. The response reports
`task_completed: false`, and the task must be completed explicitly after its own
acceptance requirements are satisfied.

```sh
orbit task complete TASK_ID
```

Tasks have first-class acceptance criteria (local-authoritative in this build):
edit them in the TUI with `K` or over MCP with `task.acceptance.*`. They are a
separate ordered aggregate; checking every criterion never completes the task.
For longer arguments you can also use a linked note or a project success
criterion, and state which one is authoritative. See
[task details and attachments](task-details-and-attachments.md) and
[notes and folders](notes-and-folders.md).

## Request changes

```sh
orbit agent review JOB_ID \
  --handoff-id HANDOFF_ID \
  --reviewer reviewer-id \
  --decision changes_requested \
  --reason "Return a typed error for malformed payloads"

orbit agent continue JOB_ID --message "Address the failing acceptance check"
```

`continue` sends to an existing confirmed session. Delivery errors are
ambiguous; inspect the session instead of retrying blindly.

## Continue versus resume

- `orbit agent continue JOB_ID --message "..."` sends to an **existing
  confirmed session**. It never starts a second agent.
- `orbit agent resume-context JOB_ID` reads a durable resume brief without
  launching.
- `orbit agent resume JOB_ID` starts a successor job when the predecessor is
  terminal and its handoff satisfies the resume contract.
- `orbit agent sweep` reconciles claimed sessions and captures reviewable
  evidence across jobs.

For connected clients, commands may be queued until authoritative
acknowledgment. A locally queued launch is **not** proof that an agent started.
See [offline operation](offline-operation.md).

## Recovery checklist

1. Identify the same `JOB_ID`; inspect persisted status, revision and handoffs.
2. Check runtime observations from Herdr. Distinguish an unreachable machine
   from a terminated job.
3. Inspect the Git worktree and commits independently of the agent summary.
4. Review only the saved handoff that corresponds to the actual job.
5. Continue a live session or resume a terminal predecessor as appropriate.
6. Complete the task only after evidence and criteria are accepted.

## Common failures

- **`review requires a persisted handoff`** — capture or submit a handoff first.
- **Conflicting decision** — a review is already recorded; read
  `review-status` instead of overwriting it.
- **`inspect` rejected** — supply the current revision and a valid timestamp.
- **`resume` refused** — the predecessor is not terminal or lacks a usable
  handoff; inspect `resume-context`.
- **Agent finished but task is open** — expected; run `task complete`.

Related: [Agent workflow tutorial](agent-workflow-tutorial.md),
[Optional machine workers](worker-deployment.md),
[Troubleshooting](troubleshooting.md).
