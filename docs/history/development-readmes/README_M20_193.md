# Orbit M20 — Development 193: desktop MCP connected authority mode

## Implementation

The standalone `orbit-mcp` binary now has two mutually exclusive start modes:

* `ORBIT_DB=/path/to/local.db` without a server URL: existing standalone local
  authoritative mode, unchanged.
* `ORBIT_AUTHORITY_URL=https://your-orbit-server ORBIT_DB=/path/to/replica.db`:
  remote connected mode. This mode never opens `AuthoritativeHost` locally;
  SQLite holds the accepted-state replica and durable pending outbox only.

Supply `ORBIT_DEVICE_ID` and `ORBIT_DEVICE_TOKEN` together to authorize
refreshing and flushing against the server; omit both for intentionally offline
queueing and accepted-cached-state inspection. Remote operations use the
`AuthorityConnection` client instead of separate business rules or SQLite
write paths.

The connected MCP tool surface provides accepted projects/tasks/schedule/
routines/milestones/strategies reads; typed schedule/routine/milestone/strategy
commands; outbox status, inspection, refresh, and flush. Commands are enqueued
until server acceptance, never represented as accepted based on a local queue.
Unsupported legacy MCP mutation tools are rejected in connected mode rather
than silently writing to an independent local authority.

The existing developer/read-only policy applies. Project strategy mutation
requires project allowlist permission. Milestone mutations check project scope,
including both old and new project when moving. Routine/schedule commands and
reads require explicit unscoped-execution permission. Remote MCP tool discovery
advertises only operations that its selected authority mode can execute.

## Deferred work and acceptance

No full Rust build, end-to-end deployment, or release acceptance is claimed.
Broad compilation and executable acceptance remain in M15. The source includes
focused behavioral test scenarios for offline semantics, permissions, denied
local command paths, and tool discovery.

Remaining M20 implementation: align any residual CLI/TUI read and write paths,
remove any stray direct local mutation entrypoints in connected mode, and
finish the common runtime contract documentation. Keep Flutter deferred.
