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
- `task.acceptance.add`
- `task.acceptance.check`
- `task.acceptance.list`
- `task.acceptance.remove`
- `task.acceptance.reorder`
- `task.acceptance.update`
- `task.activity`
- `task.command`
- `task.get`
- `task.update`
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
compatibility. Both registries add explicit aliases and rich operations:

| Tool | Purpose |
|---|---|
| `workflow.list` | List bundled workflows and, with `project_id`, the bound workflow |
| `workflow.apply` | Bind a project to a workflow version (alias of blueprint binding) |
| `project.create` | Create a project through the canonical authority; Basic is bound automatically |
| `task.get` / `task.update` | Read or replace description, priority and labels |
| `task.acceptance.list` | Ordered criteria with stable IDs and completion timestamps |
| `task.acceptance.add` / `update` / `check` / `reorder` / `remove` | Explicit ordered-criteria operations |
| `task.activity` | Durable semantic activity for one task, newest first |
| `attachment.list` / `inspect` / `add` / `read` / `remove` / `associate` | Managed content-addressed attachments (local registry only) |

**Connected rich task tools are advertised.** `task.get`, `task.update`,
`task.acceptance.*` and `task.activity` appear in the connected registry.

`task.get` is snapshot-first: description, priority and labels come from the
last accepted replica snapshot, so a connected client can load accepted detail
values offline. If the snapshot does not contain the task, the client falls back
to the live `GET /v1/authoritative/tasks/{id}` route. Ordered acceptance criteria
and activity are read from their dedicated authenticated per-task routes and
require a server connection. Connected mutations queue a canonical command and
return the last accepted payload together with explicit state markers:

| Marker | Meaning |
|---|---|
| `accepted` | `true` only when the server committed this exact command. It is `false` for both `pending` and `conflict`. |
| `pending` | `true` when the command is durable outbox intent that still needs a flush. It is `false` when the command is accepted or conflicted. |
| `disposition` | Exactly `accepted`, `pending` or `conflict`; a queued command is never labelled accepted. |
| `server_state` | The last accepted projection: description, priority and labels for `task.update`, or the accepted criteria list for `task.acceptance.*`. It never contains the queued value, even when returned alongside `disposition: pending`. |
| `message` | Present for `pending` and `conflict`; it names the queued command and the recovery step. |

When a mutation is queued, `disposition` is `pending`, `accepted` is `false`,
`pending` is `true` and the message names `authority.flush` as the recovery step.
The returned task or criteria list still shows the accepted state, which may not
include the queued change yet. If the authority permanently rejects the command,
the same response uses `disposition: conflict` with `accepted: false` and
`pending: false`; the message includes the command ID and reason and points at
`authority.unresolved` / `authority.conflict_review`. Treat that as
reconciliation work rather than retrying blindly: inspect the outbox, then
remove or replace the staged intent explicitly. Checking criteria never changes
task status.

**Connected blueprint writes are durable intents, not direct posts.** In
connected mode, `blueprint.publish`, `blueprint.install`, `blueprint.bind`,
`blueprint.create_item`, `blueprint.transition`, `blueprint.properties` and
`blueprint.template` have exactly one transport: each freezes its route and wire
payload into the canonical, globally ordered replica outbox first, then attempts
the same flush that carries every other command family. There is no direct write
route, so an unreachable or offline authority cannot lose the intention.
`blueprint.list`, `blueprint.show`, `blueprint.binding` and `blueprint.item`
remain direct reads and never queue.

| Field | Meaning |
|---|---|
| `command_id` | The stable identity of this write. Supplied as an optional string argument, otherwise generated and returned. Reuse it to replay the same write; reusing it for a different body conflicts. |
| `disposition` | Exactly `accepted`, `pending` or `conflict`. |
| `accepted` / `pending` | Booleans that mirror `disposition`; both are `false` for `conflict`. |
| `receipt` / `result` | Present only when accepted. `receipt` is the authority's immutable acceptance envelope (command ID plus response) read back from the outbox; `result` is the applied blueprint, binding, work item or template plan. A replay returns the identical receipt and never repeats the mutation. |
| `reason` / `state` | Present when pending or conflicted: the outbox state and the reviewable reason. |

While the authority is unreachable the response is `pending` with the command ID
and no server payload. Repeating the identical call after reconnecting (or
`authority.flush`) submits the same frozen intention and reports `accepted` once
the server acknowledges it. A pending acknowledgement that was actually applied
on the server is replayed from the same identity, so the retry returns the
original receipt instead of a second mutation.

**Attachment tools remain local-only.** The connected registry deliberately
does not advertise `attachment.*`: the connected CLI and TUI own staging,
upload, cross-client retrieval and online-only removal, while connected MCP
would have no honest attachment surface in this build. Use
`orbit attachment add|list|inspect|export|remove|flush` against a connected
replica instead. Attachment tools in the local registry continue to work
against a local workspace.

**Capability actions have no MCP tools in this iteration.** Capability
installation, project authorization, action requests, execution and
reconciliation are exposed only by the local `orbit blueprint capability` and
`orbit blueprint action` commands. The MCP registries deliberately advertise no
capability administration or action-execution tool: there is no hypothetical
name to discover, and adding one without an executor and grant contract would
misrepresent authority. Use the CLI against the local authoritative workspace.
Connected mode refuses those CLI groups explicitly; it does not queue a request
that cannot execute.

The local `blueprint.transition` tool does resolve the project's durable
capability grants, so a transition guarded by a capability works after that
capability has been installed and authorized for the project through the CLI.
The tool still takes only the item, target state and expected revision; grants
come from durable state, not from the tool call. See the
[trusted capability developer guide](../developers/extensions-and-capabilities.md).

`workflow.apply` and workflow binding remain local-authoritative; connected
mode reports that clearly instead of queueing an unsupported mutation.
`project.create` under a scoped policy still requires an explicit project ID
inside `ORBIT_MCP_PROJECTS`.

No MCP operation in this group launches an agent job or a Herdr session.
Checking or completing acceptance criteria never completes the task implicitly,
and no success response implies an agent launch.

## Typical workflows

**Read first:** call `agent.inspect` to inspect effective mode and scope; list accepted project/task state, then inspect the relevant workflow or task before requesting mutation.

**Connected writes:** submit canonical commands using the developer permission mode, then inspect `authority.status` / `authority.unresolved`. Use `authority.flush` and `authority.refresh` when online; use reviewed conflict resolution rather than silently overwriting accepted server history.

**Blueprints and workflows:** inspect `blueprint.list` and `blueprint.show`, or
use `workflow.list`, select an immutable version, then `blueprint.bind` (or
`workflow.apply`) only for an authorized project. `blueprint.create_item`,
`blueprint.transition`, and `blueprint.properties` are user-directed mutations;
declaring an action does **not** grant arbitrary shell or agent access. Basic is
the default binding for newly created projects. For capability actions, use the
local CLI: MCP has no install, authorize, request, execute or reconcile tools.

**Rich tasks:** use `task.get` and `task.update` for description, priority and
labels; `task.acceptance.*` for ordered criteria; and `task.activity` for
history. These work locally and in connected mode. In connected mode,
description, priority and labels load from the accepted snapshot (with a live
task-detail fallback), criteria and activity are read from the server routes,
and writes return `accepted`/`pending`/`disposition` markers until the outbox is
flushed. Attachment tools remain local-only; use the connected CLI or TUI for
connected attachments.

**Agent operations:** job context and handoff submission are job-scoped. A reported agent success does not automatically mark the Orbit task complete.

## Diagnosing failures

A missing name may indicate that the installed MCP mode does not expose the tool. A denied mutation may indicate `read_only` mode, project restrictions, stale revision, offline state or insufficient authority. Check `agent.inspect`, `authority.sync_health` and `authority.unresolved` where available. Treat uncertain external launches as reconciliation work, not permission to retry blindly.

See [MCP integration](../guides/mcp-integration.md), [offline operation](../guides/offline-operation.md), and [trusted extension authoring](../developers/extensions-and-capabilities.md).
