# M25 blueprint authoring contracts (Development 283)

A published blueprint is immutable and project bindings pin its version. Blueprints support JSON and YAML import/export via `Blueprint::from_json`, `Blueprint::from_yaml`, `to_json`, `to_yaml`. YAML and JSON must validate identically.

`templates` is an optional array of `{id,name,items}`. Each item has a template-local `key`, blueprint work-item `kind`, optional typed `properties`, and prerequisite keys. `plan_template` returns a deterministic `TemplatePlan` of work items and prerequisite edges; the authoritative host must reserve unique item IDs and persist the entire plan atomically. Planning never mutates storage or launches agents. Invalid kinds, properties, missing dependencies, cycles, and duplicate keys fail closed.

`CapabilityCatalog` records capability provider, permission class and enabled availability. Blueprint publication validates capability availability, but capability declarations cannot grant execution rights. Host-side grants are required for execution. Runtime action dispatch and persisted template application remain subsequent M25 scope.

Broad Rust compilation and integrated acceptance are M15 responsibilities.


## Atomic template application (Development 284)

`AuthoritativeHost::apply_project_blueprint_template(project_id, template_id, namespace)`
resolves the project's pinned blueprint and validates a deterministic template plan.
It inserts all work items and prerequisite edges under a single SQLite savepoint,
which also nests correctly inside the authority's accepted-command transaction.
If any ID already exists, any relationship fails, or a write fails, all the
new work items and edges are rolled back. Namespaces are explicit and must be
unique per application; collision is a conflict, never a silent overwrite.

The operation has **no external side effects**: capability declarations cannot
launch agents or run plugins. A future transport must call the host operation,
not open its own SQLite connection. Publishing a new blueprint version does not
change the pinned version used by this operation.
