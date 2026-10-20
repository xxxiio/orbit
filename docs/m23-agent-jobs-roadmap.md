# M23 — Agent-aware project execution (implemented; historical roadmap)

Status: **historical planning record.** M23 source is implemented. The local
opencode + Herdr 0.9.5 execution path is live-verified; the remote
machine-worker path remains verified only against contract doubles. M15
executable acceptance for release remains an independent gate. The text below
is the Development 229 proposal plus dated checkpoints and is not a statement of
current implementation gaps beyond the notes in this header.

## Current baseline and priority

- M18–M21 feature/source scopes were addressed; M20 implementation closure at Development 204 and M21 source closure at Development 212. M22 Development 229 closes its source-feature scope for deterministic task planning, portfolio selection and read-only action handoffs.
- M15 owns workspace compilation, integration tests, runnable packages, local/remote smoke tests and data-integrity release acceptance. Finish M15 before declaring M22/Orbit accepted. M23 planning can begin now; do not allow its work to silently defer M15.
- Keep Orbit's local SQLite authority and optional server-authoritative connected mode. A disconnected replica cannot approve, launch through host authority, or close a task based on its pending intentions.
- Extend `AuthoritativeHost`, accepted command/receipt paths, `orbit-mcp` and CLI/TUI; do not add a second SQLite authority or business rules to a worker, MCP handler, or Herdr adapter. Flutter belongs to Atomic App.
- Existing planning handoffs (`planning_handoff`) are read-only *navigation*, not persistent agent handoffs. Existing MCP agent origin/permission contracts are useful, but are not an execution runtime.

## Roles and core invariants

- Orbit owns project/task state, immutable approved task launch context, durable AgentJob ledger, reservations/leases, structured reports, reviewer decisions and activity history.
- Herdr owns process/pane/workspace/terminal runtime, remote routing and reconnection. Codex/OpenCode/custom agents own implementation. Git owns commits/diffs/branches; record verifiable references and bounded summaries rather than duplicating Git history.
- Two independent state machines: **job lifecycle** (queued, preparing, launching, running, waiting_input, blocked, completed, failed, interrupted, unreachable) and **task review** (existing task status plus pending_review/review outcomes). `completed` is a recorded runtime/job outcome, not proof of task acceptance. Only an explicit accepted task command may mark a task complete.
- Distinguish `unreachable` (connectivity observation) from terminal job outcomes, and `idle`/`done`/`blocked` Herdr observations from Orbit job outcomes. Preserve `last_seen_at`, `last_observed_state`, `observation_source`, and freshness independently. Unknown state must not become success.
- Use durable idempotency key per requested launch; authority-serialized active job reservation per task and explicit replace/parallel policy. Record preparing intent before side effects. Launch attempts must be recoverable after process crash; inspect Herdr before retrying; never blindly relaunch on ambiguous timeout.
- Agent reports are untrusted proposals/evidence. Attribute reports to a job-scoped credential/identity with narrowly scoped task/project access, validate allowed transitions and command revisions, retain audit events. Never grant Git/OS capability merely because MCP tool access was allowed.
- Local launch requires a local trusted executor; connected launch requires a machine-side **opt-in executor/bridge** with a registered identity and accepted server command. The server must not assume it can execute directly on a disconnected client, nor require cloud or SSH credentials in Orbit's canonical data.
- Do not capture secrets or full terminal logs by default. Limit context to explicit project/task/repository paths and consented information. Bind repository identity and validate worktree base/ref/path; don't treat prompt instructions as shell commands.

## Proposed Rust boundaries

`AgentJob` (domain): job_id, task_id, project_id, launch_revision, request_id, agent_kind, runtime_kind, machine_ref, repository_ref, workspace_ref, branch, worktree_ref, lifecycle, created_at, updated_at, last_seen_at, launch_attempt, completion_evidence_ref, report_ref, review_state.

`AgentLaunchRequest`: task_id, agent_kind, machine_ref, repository_ref, worktree_policy, explicit user-approved runtime permissions, optional instruction; binds an immutable, revision-stamped `TaskExecutionBrief` (goal, criteria, relevant dependencies, non-secret references).

`AgentRuntime` port (in an integration crate, e.g. `orbit-agent-runtime`): `capabilities`, `prepare_workspace`, `start`, `inspect`, `list`, `send`, `reconnect`, and `stop` with typed results and capability/error enums. Runtime identity must include `machine_ref + server/session + workspace/pane + optional agent_session`, not just a name. Make stopping or sending explicit user actions. Avoid assuming session resume is supported for every agent.

`AgentJobRepository`/`AgentJobService`: authoritative job event transitions, idempotency, leases/ownership, report receipt, evidence association, reviewer decisions, and reconciliation. Job events belong to canonical history; projections serve CLI/TUI/MCP/HTTP and accepted snapshots. Offline updates remain pending commands until accepted.

`GitEvidenceReader`: read-only adapter for commit OIDs, status, changed paths and command validation results; verify repo/branch provenance and keep collection bounded. `TaskHandoff` (persistent structured result) is distinct from M22's read-only `PlanningHandoff`.

`HerdrRuntime`: optional adapter using Herdr JSON CLI/socket interface, initially CLI subprocess. Implement machine-specific command routing, worktree/workspace creation, pane acquisition, `agent start`, `agent prompt`, `agent get/list`, and backend error mapping. Use CLI JSON parsing (never brittle screen scraping). Version/capability probe and explicit failures for absent Herdr, offline machine and incompatible API. Do not treat `prompt` acknowledgment as successful execution.

## Cohesive implementation iterations

Each iteration below is a **target of at least five M23 percentage points**, preferably substantially larger. Percentages are provisional allocations to acceptance outcomes, never credits for line counts; target ~1,000 meaningful LOC only when warranted. Implement features and essential tests in the same iteration. These are the original Development 229 allocations; all listed iterations were subsequently delivered in source, so percentage claims in the dated checkpoints below are historical snapshots, not the current state.

| Iteration | Target share | Delivery and acceptance gate |
| --- | ---: | --- |
| M23-I1 Authority and task/job contract | 20% | Persist AgentJob lifecycle/events, typed launch briefs, job/task distinction, idempotent requests, single-active reservation policy and accepted local/remote command projection. Test conflicting launches, stale revisions, offline pending, replay and task-not-auto-complete. |
| M23-I2 Herdr launch vertical slice | 20% | Opt-in local Herdr adapter and task Start action; agent choice, workspace/worktree prep, stored machine+pane identity, Codex/OpenCode launches, context prompting. Test real Herdr CLI on supported platform (mock contract tests alone insufficient), unsupported agent and ambiguous launch retry. |
| M23-I3 Connected execution and monitoring | 20% | Registered opt-in machine executor, remote Herdr routing, per-machine polling/reconciliation, dashboard projection for job states, attention queue and stale/reachable UI. Test two-machine disconnect/reconnect, unknown states, recoverable crash and no phantom completion. |
| M23-I4 Agent report/evidence/review | 20% | Scoped CLI/MCP tools for progress/blockers/decisions/handoffs, Git evidence capture and immutable reporting, review/accept/reject/retry flow. Test permission boundaries, forged job IDs, report revisions, evidence provenance, job-finished/task-open invariant. |
| M23-I5 Catch-up and resume | 20% | Deterministic cross-project catch-up (since last viewed), attention-ranked next actions, reusable structured context packets, Resume with Agent in live/new pane; optional LLM summary adapter only. Test restart, interrupted sessions, portable handoffs, redaction, no-LLM operation and end-to-end UX loop. |

Completion of M23 requires compiled tests and executable end-to-end acceptance of `Select Task -> Start -> Monitor -> Review -> Resume/Complete` in local and optional self-hosted modes, without Herdr installed for ordinary Orbit workflows. This M23 feature gate does **not** replace M15 release acceptance.

## Deferred

Autonomous multi-agent dispatch/orchestration, scheduling, automatic merges, Hermes integration, GitHub Issues/Beads synchronization, cloud execution, and automatic criterion judgment. These are future opt-in milestones, not prerequisites for useful M23.

## Herdr integration evidence (checked 2026-10-18)

Herdr CLI reference: https://herdr.dev/docs/cli-reference/ ; agent integrations: https://herdr.dev/docs/integrations/ . `--machine` routing is explicit per request; `workspace create`, `worktree create`, `agent start` and `agent prompt/list/get` exist. `agent start` requires a free shell pane, and `agent prompt` without `--wait` confirms input submission only. Herdr `idle`/`done` are ready-for-input states; `blocked` means a detected interaction prompt. Implement against tested versions, not assumed future guarantees.

## M23-I1 implementation checkpoint (Development 230)

**Source progress: M23 8% (provisional; executable acceptance pending).** This is a partial I1 deliverable, not I1 closure. The code adds domain `AgentJob`/`AgentJobState` and SQLite schema v14, reservation, event ledger, CAS lifecycle updates and baseline tests. It does **not yet** connect reservations to the authoritative accepted-command/replica projection, validate task readiness and task revision at authority, dispatch Herdr, or implement application `AgentJobRepository` and `AgentJobService` ports. Thus the target I1 20% is not earned yet. M15 acceptance remains pending and M22 remains source-complete, not runtime-accepted.

Checks performed: SQLite DDL initialization and partial unique active-task constraint via Python `sqlite3`, source export check. Rust `cargo test` could not be run because cargo is unavailable in the execution environment. Before building against existing schema-v13 databases, execute real Rust migration tests; do not use this snapshot as release-ready. The next priority is to finish the I1 accepted-authority command path (not commence Herdr side effects).

## M23-I1 checkpoint (Development 231)

**Provisional progress: M23 13%, I1 65%.** This checkpoint adds within-transaction task existence/project/status checks, exclusive durable per-job launch claims, ambiguous launch result records, confirmed runtime session binding, and project-scoped job listing. Reservation, launch claim and lifecycle remain distinct so an interrupted external invocation does not trigger a blind repeat. New unit tests cover missing/blocked tasks, exclusive claims and ambiguous-then-confirmed results.

Important incomplete work: accepted-command/replica projection, task dependency readiness and immutable launch context, task revision binding, host authorization, executable Rust validation, and real Herdr commands. The 13% value is a source-evidence estimate, **not an accepted I1 or production-readiness claim**. M15 executable acceptance remains pending; no further milestone progress is assigned to it. The next iteration must deliver >=5 M23 percentage points through acceptance criteria, not only code quantity.

## M23-I1 checkpoint (Development 232)

**Provisional source progress: M23 18% (+5 percentage points from 231); I1 90%.** Adds an immutable task brief stored atomically with job reservation, and command-ID-based accepted receipt persistence with revision-checked prepare/interrupt/mark-failed transitions. Replayed command IDs are idempotent; reusing the ID with changed arguments fails. Updated SQLite schema to v15 with a v14→v15 upgrade, plus new source-level tests. Job transitions do not complete the linked task. The task's title/status snapshot is deliberately minimal and is not a validated set of acceptance criteria.

**Validation:** SQLite schema DDL and JSON-valid checks passed via Python sqlite3. Cargo/rustc unavailable: no Rust compilation, tests, or connected-mode acceptance performed. This checkpoint does not yet integrate `accept_agent_job_command` with Orbit's external accepted-command/replica command router or authorize users/machines; the method is an authority-local primitive only. Dependency-aware task readiness, real task revision tracking, approved acceptance-criteria snapshots, host authorization and live Herdr dispatch remain incomplete. I1 is not accepted or closed. Avoid double-counting these source-progress percentages as runtime-acceptance progress. M15 remains pending, M22 source-scope 100%.
