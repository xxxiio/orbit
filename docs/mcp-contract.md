# Orbit MCP contract

Orbit MCP is a desktop/headless adapter. It is never a storage API and mobile clients do not compile/register it.

## Trust boundary

Agent identity, session identity, capability mode, project allowlists, and access to unscoped personal data come from Orbit server composition (`ORBIT_MCP_*`), not from self-reported MCP client metadata.

Default composition is read-only, no unscoped execution data, and no unscoped knowledge. Developer mode must be explicitly selected.

## Semantic tools

Read surfaces include projects, tasks, project milestones, permitted habits, priorities, knowledge search/backlinks/context, and effective agent/session inspection.

Developer mutation surfaces include task creation/status, project/milestone status, habit active state, knowledge creation/linking, and dedicated decision/risk/reference capture.

Every mutation routes through `orbit-application` and therefore preserves domain validation, atomic activity journaling, agent attribution, and semantic sync-outbox behavior.

## Capability actions

Capability installation, project authorization, action requests, execution and
reconciliation are local-CLI operations in this iteration. The MCP registries
advertise no capability administration or action-execution tools, and connected
mode refuses the corresponding CLI groups rather than queueing a request that
cannot execute. The local blueprint transition tool does use the project's
durable capability grants, so a capability-guarded transition becomes usable
after the capability is installed and authorized through the CLI.

## Audit identity

Mutations use `ActivityOrigin::Agent("<agent-id>:<session-id>")`. Agent-created knowledge additionally records `KnowledgeProvenance::Agent(<agent-id>)`.

## Transport

The standalone `orbit-mcp` binary speaks JSON-RPC over stdio, advertises tools, accepts tool calls, and keeps compatibility handlers for both `server/discover` and the legacy `initialize` opening.
