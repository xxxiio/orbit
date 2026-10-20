# Agent results: inspect, review, continue, and resume

Orbit separates **job state**, **agent session state**, **handoff evidence**, **review**, and **Orbit task completion**. They are not interchangeable. Agent operations require an existing Orbit task and, for actual execution, a configured optional Herdr runtime.

## Start and observe

```sh
orbit agent start TASK_ID --agent codex --machine local --repository /absolute/path/to/git/repo --worktree
orbit agent overview
orbit agent attention
orbit agent status JOB_ID
```

Use a disposable or dedicated Git repository while learning. A `start` operation is an external side effect: do not repeat it to resolve uncertainty. Read the durable job state and reconcile the Herdr session first.

## Capture and review

`orbit agent inspect` takes job revision and timestamp arguments. Check `orbit agent inspect --help` for the current signature, and read the recorded job before supplying its revision.

```sh
orbit agent handoffs JOB_ID
orbit agent capture JOB_ID --max-commits 20
orbit agent handoffs JOB_ID
orbit agent review-status JOB_ID
```

`capture` records Git evidence but **does not prove tests passed**. A handoff submitted by an agent must include structured `summary`, `validation`, and `next_steps` fields. A reviewer should inspect the actual changes and validation logs, then submit an explicit decision:

```sh
orbit agent review JOB_ID --handoff-id HANDOFF_ID --reviewer reviewer-id --decision approved --reason "Acceptance criteria independently checked"
```

The `--decision` may also request changes according to the server's supported review contract; verify allowed values with CLI help before use. Review does **not** close the Orbit task. The task must be completed explicitly once its own acceptance requirements are satisfied.

## Continue versus resume

- `orbit agent continue JOB_ID --message "Address the failing acceptance check"` sends a message to an **existing confirmed session**. Delivery may be ambiguous; do not retry blindly.
- `orbit agent resume-context JOB_ID` reads the saved context without starting an agent.
- `orbit agent resume JOB_ID` starts a successor job when the predecessor and handoff satisfy the resume contract.

For connected clients, commands may be queued until authoritative acknowledgment. A locally queued launch is **not** proof that an agent has started. See [Offline operation](offline-operation.md).

## Recovery checklist

1. Identify the same `JOB_ID` and inspect persisted status, revision and handoffs.
2. Check for runtime observations from Herdr; distinguish an unreachable machine from a terminated job.
3. Inspect Git worktree and commit evidence independently of any agent summary.
4. Review only the saved handoff that corresponds to the actual job.
5. Continue a live session or resume a terminal predecessor as appropriate; never duplicate an uncertain launch.

Related: [Agent workflow tutorial](agent-workflow-tutorial.md), [Optional machine worker](worker-deployment.md), [Troubleshooting](troubleshooting.md).
