# M25 blueprint work item implementation (Development 281)

The blueprint runtime now supports typed, independent work items scoped to
projects explicitly bound to immutable blueprint versions. The legacy Task
and ProjectMode APIs remain unchanged. No automatic conversion or migration
occurs. `orbit-blueprint::workflow` implements normalization, strict custom
property validation, allowed transitions, capability checks, and optimistic
revisions. Neither transition nor property editing triggers external actions.

The authoritative host exposes `create_project_work_item`,
`project_work_item`, `transition_project_work_item`, and
`update_project_work_item_properties`. A new SQLite table stores work items
under their project's pinned blueprint. Revision-conditional updates reject
stale writers. Any capability grant must originate from trusted server/local
host composition, never arbitrary client input. Default development-blueprint
installation is explicitly invoked and does not auto-bind existing projects.

## Boundaries / next deliverables

- These are new blueprint-specific work items, not replacements for existing
  Orbit tasks; binding reconciliation and unified work-item queries remain.
- Transition guards currently support declared capability requirements only.
  Approval and validation checks will be introduced through registered
  deterministic predicates, not user-supplied executable scripts.
- User-facing YAML import/export, server and CLI/MCP endpoints, blueprint
  project templates, and plugin action registration are not yet implemented.
- Broad Rust builds, end-to-end integration and resulting fixes belong to M15.
