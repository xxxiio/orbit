# Orbit M20 — Development 192: Server-authoritative MCP rich-domain integration

## Goal and scope

Deliver the missing server MCP bridge for schedule, routine, milestone, and
strategy domains. These tools use the exact same AuthoritativeHost methods as
REST, local Rust clients, and the connected CLI/TUI. No independent MCP SQL
mutation layer or parallel business rules are introduced.

## Added capabilities

* Four snapshot tools: `orbit_get_schedule`, `orbit_get_routines`,
  `orbit_get_milestones`, `orbit_get_strategies`.
* Four command tools: `orbit_schedule_command`, `orbit_routine_command`,
  `orbit_milestone_command`, `orbit_strategy_command`.
* Mutations require device credentials, as with the existing project, task,
  and evidence command tools. Admin-only requests remain read-only.
* All command tools require a command ID, expected revision, and typed action.
  They forward to the canonical Rust command engine and emit canonical
  receipts rather than claiming acceptance before authority has committed.
* Read operations expose only accepted projections. There is no path for an
  MCP tool to promote offline pending changes into accepted state.
* Explicit regression scenarios for tool discovery, read authorization,
  denied writes, malformed payloads, stable receipts, and routine readback.

## Remaining implementation work

This completes the server-side MCP bridge for the four rich domains. It does
not unify the separately running local desktop MCP binary, which remains
configured around legacy application services. That binary requires a
separate intentional routing cleanup, not a second authority implementation.

## Verification status

Compilation and full Rust integration acceptance intentionally deferred to
M15 per the project's feature-first iteration priority. Tests have been
written but not executed. This iteration makes no executable acceptance claim.

## M20 progress

M20 remains **90% provisional**, with substantive server MCP integration
completed. Remaining implementation includes local desktop MCP routing and
residual CLI/TUI interface parity. M15 handles full compilation, integration
smoke testing, and release acceptance.
