# M22 Development 218 — Planning capability discovery

## Priority

The M22 planning engine has usable dependency, readiness, schedule, workload,
Today/Next and completion-impact operations. Its APIs were not discoverable
through a stable machine contract, which forced agent integrations to hardcode
command names and assumptions. A shared capability catalog is the next
user-facing integration priority; release acceptance stays in M15.

## Implemented

- `orbit-runtime::capabilities` defines stable planning IDs, argument contracts,
  authority requirements, scope, deterministic behavior and offline read safety.
- `orbit capabilities --json` lists the registry; `--id` searches an exact ID and
  `--accepted-only` lists read-only operations against accepted state.
- Both embedded and connected MCP advertise and execute `planning.capabilities`,
  returning the same versioned contract as CLI without opening a database.
- Planning MCP `work_brief` and `completion_impact` input schemas are now generated from the registered argument metadata instead of untyped permissive placeholders.
- Registry tests enforce unique IDs, unique CLI paths, argument name uniqueness,
  task-scoped task identifiers, serialization and accepted-read filters.

## Limitations and follow-up

The discovery registry is planning-only, not a complete inventory of every Orbit
operation. It does not grant authorization: every existing execution handler
continues to enforce its own access rules. Some planning read operations lack
a corresponding MCP tool, and `mcp: null` explicitly indicates that absence.
Source-level checks do not establish executable Rust acceptance; that remains
in M15. No Git sync, cloud service, or mandatory agent subsystem was added.

## Additional end-to-end feature scope

- `task.readiness`, `planning.day`, `planning.schedule`, and
  `planning.workload` are now callable through both local and connected MCP.
- These four MCP operations delegate to existing authoritative/accepted-snapshot
  client services; no duplicate scheduling or dependency rules were created.
- The new MCP handlers enforce workspace read-scope restrictions consistently
  in both authority modes.
- The MCP advertised parameter schemas for six planning operations come from
  the same versioned registry used by CLI discovery.

`planning.capabilities` is explicitly discovery-only and never grants permission
or implies that every planning operation is callable in a restricted session.
