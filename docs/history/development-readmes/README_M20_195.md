# Orbit M20 — Development 195: cross-domain connected MCP workflows

## Implementation scope

The unified `DomainWorkflowStep` representation previously handled only project,
task and evidence commands. It now covers all seven canonical domain command
families: project, task, evidence, scheduling, routines, milestones, and
strategies. `AuthorityConnection::execute_workflow` preserves stable command
identities, rejects duplicates before side effects, and exposes precisely which
prefix was accepted or queued if a later operation fails.

The connected desktop MCP now exposes `authority.workflow` for ordered,
explicitly non-atomic cross-domain operations. It first deserializes and checks
**every** requested command against the agent's project allowlist, permission
to mutate, and unscoped execution access. Nothing is queued when a later step
is malformed, unauthorized or shares a command ID. Actual runtime failures
still return their completed prefix and failed step index; there is deliberately
no false claim of atomicity. The operation is capped at 100 steps.

`authority.resolve` supports explicit review and replacement of conflicted
remote commands. The MCP adapter checks the replacement action's type and
scope first, then delegates to the canonical outbox, which guarantees the
original was conflicted, the replacement family matches, command IDs are
unique, and the original remains retained as superseded audit data.

Neither operation mutates accepted remote projections. Connected requests
remain pending until accepted by the self-hosted authority.

## MCP examples

Workflow:

```json
{"steps":[
  {"family":"project","command":{"command_id":"create-project-1","expected_revision":0,"project_id":"demo","name":"Demo"}},
  {"family":"task","command":{"command_id":"create-task-1","expected_revision":0,"action":{"type":"create","task_id":"demo-task","title":"First task","project_id":"demo"}}}
]}
```

Send this argument to `authority.workflow`. A response with two `pending`
dispositions means both are durably queued, **not** accepted. Connected agents
should `authority.flush` and then `authority.refresh` before relying on the
accepted projection.

Conflict replacement, after reviewing the accepted state and the unresolved
command list:

```json
{"rejected_id":"old-command-id","replacement_id":"new-command-id",
 "family":"task","action":{"type":"rename","task_id":"demo-task","title":"Updated"}}
```

Send this to `authority.resolve`. It will reject replacement of a non-conflicted
command, cross-family substitution, out-of-scope actions, and reused IDs.

## Testing and acceptance boundary

Focused Rust test cases were authored for ordered cross-domain queueing,
preflight failures, permissions, conflict substitution, and tool discovery.
Compilation and broad executable acceptance remain assigned to M15.

M20 remains open until residual implementation paths have been reconciled.
Internal testing is **not** declared ready merely from source changes.
