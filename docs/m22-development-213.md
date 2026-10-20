# Development 213 — M22 Intelligent Task Planning & Agent-Friendly Workflows

## Scope and why this comes before M15

After M21 source completion, the product roadmap explicitly prioritizes usable daily
workflows inspired by Beads Rust over standalone release acceptance. M22 has been
repurposed from duplicated integrated-acceptance work to deterministic planning.
M15 retains broad Rust compilation, integration, packaging and internal-testing
acceptance. This iteration delivers the dependency/readiness foundation.

## Implemented

- Task dependencies persist in the **authoritative SQLite store** as task-to-task
  prerequisite edges with a composite key, foreign keys, and reverse index.
- Dependency add/remove uses the *existing* canonical task command API, its
  global expected revision, immutable command receipt, and atomic semantic event.
  It is not a second command engine. A cyclic or missing edge never commits.
- Transitive cycle detection walks the graph recursively within the same SQLite
  transaction as the edge mutation.
- Every accepted mutation publishes a sorted complete `prerequisites` semantic
  field for that task, preserving state-replay and conflict diagnostics.
- The server's accepted snapshot includes `task_dependencies` so connected
  replicas can evaluate the same graph **offline** from last accepted data;
  pending outbox commands do not alter accepted readiness.
- An explicit-time readiness evaluator rejects corrupt graphs, differentiates
  unfinished dependencies, deferred/waiting/planned workflow states, scheduled
  future tasks, overdue tasks, and completed/cancelled tasks. Overdue work
  remains actionable.
- Graph analysis computes transitive prerequisites, transitive dependents,
  topological order, critical prerequisite depth, and potential downstream
  impacts, with deterministic sorted results and fail-closed corruption checks.
- `next_actions` ranks actionable tasks by overdue deadline, unfinished
  downstream reach, upcoming deadline, and estimated duration, with human
  explanations. This is a transparent heuristic, not automatic execution.
- Exposed the same logic through the shared Rust connection, embedded client,
  HTTP GET `/v1/authoritative/task-readiness`, `/task-graph`,
  `/task-dependencies`, and `/next-actions` endpoints; local CLI `task
  depends-on`, `task unblock`, `task ready`, `task graph`, `task next`.

## Contract examples

```text
orbit task depends-on implement-auth database-schema
orbit task ready --now-ms 1800000000000
orbit task graph --now-ms 1800000000000
orbit task next --now-ms 1800000000000 --limit 20
orbit task unblock implement-auth database-schema
```

Both local-only authority and self-hosted server paths remain supported. Remote
queueing uses existing canonical `TaskAction` serialization; disconnected users
must still refresh accepted snapshots to observe accepted dependency changes.

## Limitations / next milestones

- Only task-to-task dependencies: project/milestone/habit relations remain
  outside this scoped model.
- Readiness is computed from accepted workflow status and explicit evaluation
  time; no hidden LLM/agent mutation or background scheduler.
- Rank is transparent and heuristic; personalization and policy configuration
  are outside this iteration.
- Existing `today` views have not been replaced with the next-action engine;
  future M22 iterations should integrate them coherently.
- Existing remote MCP task command transport can submit these typed task
  actions; dedicated discoverable dependency and next-action MCP tools are
  planned for a later M22 iteration.
- This artifact has not been built or runtime-tested with Cargo in the current
  environment. Focused Rust tests were added, but cannot be claimed as passed.

## Provisional milestone

M22 0% -> 20% source implementation (task dependency and deterministic planning
foundation). M15 remains deferred and internal-testing readiness unverified.
The original M22 acceptance scope is consolidated into M15 in the roadmap;
no separate duplicated acceptance milestone is planned.
