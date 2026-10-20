# Agent workflow: delegate, monitor, review, resume

## Requirements

Orbit core works without Herdr. To execute a coding job, configure the optional Herdr runtime and a supported agent (Codex/OpenCode or an installed custom adapter) on the intended execution machine. Prefer an isolated Git worktree for concurrent code edits.

## Workflow

1. **Prepare the task.** Record repository context, acceptance criteria and prerequisites in Orbit. Blocked tasks should not launch.
2. **Launch intentionally.** `orbit agent start --help` describes the launch options. Use an authorized repository path and select an execution machine and worktree policy.
3. **Monitor.** `orbit agent overview` provides durable job state, while `orbit agent attention` shows sessions needing input/review. Offline or unreachable is not equivalent to failed or terminated.
4. **Capture evidence.** Inspect the job and its structured handoffs. Review Git commits, changes and validation outcomes; a Git diff alone is not evidence that tests passed.
5. **Review.** Explicitly approve or request changes. Approval of an agent job does not automatically complete the project task.
6. **Resume.** `orbit agent resume --help` creates a new job from saved context when eligible, while `orbit agent continue --help` messages a confirmed existing session. Do not assume that an uncertain launch is safe to retry.

## Machine authority boundary

The self-hosted Orbit server accepts jobs and tracks durable state; it **never becomes a general terminal manager**. A trusted machine worker claims authorized launches, invokes Herdr and reports observations. The machine worker's external-effect journal is distinct from the client's pending-command outbox. Never delete a journal that contains ambiguous launches without reconciling actual Herdr sessions.

## Deterministic catch-up

Orbit stores jobs, handoffs, reviews and task state independent of terminal transcripts. `orbit agent overview` can build a catch-up summary without any LLM. Treat suggested next actions as recommendations requiring normal authorization.
