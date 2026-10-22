# MCP tools and authority reference

`orbit-mcp` is an **optional** executable. Its tools differ between local-authoritative and connected replica operation. Always call the MCP `tools/list` endpoint to discover the installed build's actual tool names and JSON schemas; this document is an orientation, not a substitute for negotiated schemas.

## Permission and identity

The runtime reads `ORBIT_MCP_AGENT_ID` (default `external-agent`), `ORBIT_MCP_SESSION_ID` (default `default`), `ORBIT_MCP_MODE` (`read_only` by default; `developer` permits scoped writes), and `ORBIT_MCP_PROJECTS` (comma-separated allowed project IDs). An empty project list means unrestricted *project selection*, not authorization for unscoped external execution. `ORBIT_MCP_ALLOW_UNSCOPED_EXECUTION` and `ORBIT_MCP_ALLOW_UNSCOPED_KNOWLEDGE` separately control those exceptions. Grant least privilege to each agent/session.

Connected mode uses `ORBIT_DEVICE_ID` and `ORBIT_DEVICE_TOKEN` together for authenticated authority access; missing one of the pair is an error. A connected local SQLite file is a **replica/outbox**, not canonical state. Pending commands are not accepted server mutations until accepted by the authority. Do not share tokens or database files with untrusted extensions.

## Tools registered in MCP source

The following names are extracted from the explicit Rust registrations in `crates/orbit-mcp/src/main.rs`. Dynamically registered `orbit_runtime::capabilities::mcp_tool(...)` entries are supplied by the capabilities registry and must still be discovered at runtime.

### Connected registry (explicit registrations)

- `agent.handoff_submit`
- `agent.inspect`
- `agent.job_context`
- `authority.conflict_review`
- `authority.converge`
- `authority.convergence`
- `authority.flush`
- `authority.observe_convergence`
- `authority.observe_sync_health`
- `authority.observe_workspace_sync`
- `authority.refresh`
- `authority.resolve`
- `authority.resolve_reviewed`
- `authority.resolve_reviewed_batch`
- `authority.review_online`
- `authority.status`
- `authority.sync_health`
- `authority.unresolved`
- `authority.workflow`
- `authority.workspace_sync`
- `blueprint.bind`
- `blueprint.binding`
- `blueprint.create_item`
- `blueprint.install`
- `blueprint.item`
- `blueprint.list`
- `blueprint.properties`
- `blueprint.publish`
- `blueprint.show`
- `blueprint.template`
- `blueprint.transition`
- `evidence.command`
- `habit_occurrences.list`
- `habits.list`
- `metric_observations.list`
- `metrics.list`
- `milestone.command`
- `milestones.list`
- `planning.apply_approved_scenario`
- `planning.capabilities`
- `planning.compare_scenarios`
- `planning.completion_impact`
- `planning.day`
- `planning.group_actions`
- `planning.group_focus`
- `planning.handoff_group`
- `planning.handoff_task`
- `planning.portfolio_actions`
- `planning.portfolio_capacity`
- `planning.resolve_group_action`
- `planning.resolve_task_action`
- `planning.review_scenario`
- `planning.review_scenario_options`
- `planning.schedule`
- `planning.task_actions`
- `planning.task_focus`
- `planning.work_brief`
- `planning.workload`
- `project.command`
- `projects.list`
- `routine.command`
- `routines.list`
- `schedule.command`
- `schedule.list`
- `strategies.list`
- `strategy.command`
- `task.command`
- `task.dependencies.add`
- `task.dependencies.preview`
- `task.dependencies.remove`
- `task.graph`
- `task.next`
- `task.readiness`
- `tasks.list`

### Local registry (explicit registrations)

- `agent.handoff_submit`
- `agent.inspect`
- `agent.job_context`
- `attachment.add`
- `attachment.associate`
- `attachment.inspect`
- `attachment.list`
- `attachment.read`
- `attachment.remove`
- `blueprint.bind`
- `blueprint.binding`
- `blueprint.create_item`
- `blueprint.install`
- `blueprint.item`
- `blueprint.list`
- `blueprint.properties`
- `blueprint.publish`
- `blueprint.show`
- `blueprint.template`
- `blueprint.transition`
- `habit.set_active`
- `habits.list`
- `knowledge.command`
- `knowledge.folder_create`
- `knowledge.folder_move`
- `knowledge.read`
- `knowledge.relations`
- `knowledge.search`
- `knowledge.tree`
- `milestone.set_status`
- `milestones.list`
- `planning.apply_approved_scenario`
- `planning.capabilities`
- `planning.compare_scenarios`
- `planning.completion_impact`
- `planning.day`
- `planning.group_actions`
- `planning.group_focus`
- `planning.handoff_group`
- `planning.handoff_task`
- `planning.portfolio_actions`
- `planning.portfolio_capacity`
- `planning.resolve_group_action`
- `planning.resolve_task_action`
- `planning.review_scenario`
- `planning.review_scenario_options`
- `planning.schedule`
- `planning.task_actions`
- `planning.task_focus`
- `planning.work_brief`
- `planning.workload`
- `priorities.list`
- `project.configure_strategy`
- `project.create`
- `project.evidence_bind`
- `project.evidence_list`
- `project.evidence_report`
- `project.evidence_unbind`
- `project.health`
- `project.metric_observe`
- `project.metric_set_target`
- `project.set_criterion_status`
- `project.set_status`
- `projects.list`
- `task.create`
- `task.get`
- `task.update`
- `task.activity`
- `task.acceptance.add`
- `task.acceptance.check`
- `task.acceptance.list`
- `task.acceptance.remove`
- `task.acceptance.reorder`
- `task.acceptance.update`
- `task.dependencies.add`
- `task.dependencies.preview`
- `task.dependencies.remove`
- `task.graph`
- `task.next`
- `task.readiness`
- `task.set_status`
- `tasks.list`
- `workflow.apply`
- `workflow.list`

## Workflow and rich-task tooling

Orbit calls project manifests **workflows** in the TUI and in prose. The
`blueprint.*` tools and the `orbit blueprint` command group keep their names for
compatibility. The local registry adds explicit aliases and rich operations:

| Tool | Purpose |
|---|---|
| `workflow.list` | List bundled workflows and, with `project_id`, the bound workflow |
| `workflow.apply` | Bind a project to a workflow version (alias of blueprint binding) |
| `project.create` | Create a project through the canonical authority; Basic is bound automatically |
| `task.get` / `task.update` | Read or replace description, priority and labels |
| `task.acceptance.list` | Ordered criteria with stable IDs and completion timestamps |
| `task.acceptance.add` / `update` / `check` / `reorder` / `remove` | Explicit ordered-criteria operations |
| `task.activity` | Durable semantic activity for one task, newest first |
| `attachment.list` / `inspect` / `add` / `read` / `remove` / `associate` | Managed content-addressed attachments |

`task.get`, `task.update`, task acceptance, `task.activity`, workflow binding
and all attachment tools are **local-authoritative only** in this build. They
are not advertised in the connected registry: the accepted server snapshot does
not project these fields, and the server does not accept attachment uploads.
Calling them against a remote connection returns a typed `Unavailable` failure
instead of a false success. `project.create` under a scoped policy still
requires an explicit project ID inside `ORBIT_MCP_PROJECTS`.

No MCP operation in this group launches an agent job or a Herdr session.
Checking or completing acceptance criteria never completes the task implicitly.

## Typical workflows

**Read first:** call `agent.inspect` to inspect effective mode and scope; list accepted project/task state, then inspect the relevant workflow or task before requesting mutation.

**Connected writes:** submit canonical commands using the developer permission mode, then inspect `authority.status` / `authority.unresolved`. Use `authority.flush` and `authority.refresh` when online; use reviewed conflict resolution rather than silently overwriting accepted server history.

**Blueprints and workflows:** inspect `blueprint.list` and `blueprint.show`, or
use `workflow.list`, select an immutable version, then `blueprint.bind` (or
`workflow.apply`) only for an authorized project. `blueprint.create_item`,
`blueprint.transition`, and `blueprint.properties` are user-directed mutations;
declaring an action does **not** grant arbitrary shell or agent access. Basic is
the default binding for newly created projects.

**Rich local tasks:** use `task.get` and `task.update` for description, priority
and labels; `task.acceptance.*` for ordered criteria; `task.activity` for
history; and `attachment.*` for managed files. These are local-workspace tools
in this build and fail closed in connected mode.

**Agent operations:** job context and handoff submission are job-scoped. A reported agent success does not automatically mark the Orbit task complete.

## Diagnosing failures

A missing name may indicate that the installed MCP mode does not expose the tool. A denied mutation may indicate `read_only` mode, project restrictions, stale revision, offline state or insufficient authority. Check `agent.inspect`, `authority.sync_health` and `authority.unresolved` where available. Treat uncertain external launches as reconciliation work, not permission to retry blindly.

See [MCP integration](../guides/mcp-integration.md), [offline operation](../guides/offline-operation.md), and [trusted extension authoring](../developers/extensions-and-capabilities.md).
