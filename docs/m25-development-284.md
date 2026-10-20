# Development 284 — Atomic Project Template Application

## Implementation

- Added `SqliteStore::apply_blueprint_template(project_id, template_id, namespace)`.
- Validates and plans from the project's pinned published blueprint before writes.
- Inserts work items and prerequisite edges under one SQLite savepoint; supports enclosing accepted-command transactions.
- Rolls back all insertions on any error (including duplicate item IDs).
- Exposed the operation through `AuthoritativeHost::apply_project_blueprint_template`.
- Added focused success, collision rollback and namespace-isolation test cases.
- Kept plugin/agent execution completely outside template application.

## Scope / progress

M25 implementation progress (provisional): 41% -> 47% (+6 percentage points).
M15 release acceptance stays at 5%. Broad Rust compilation, connected transport tests, and integration acceptance are M15 scope. Source tests are added but unexecuted here.

## Remaining M25

Declarative workflow actions, capability composition/authorization, optional blueprint packages, and CLI/MCP/connected APIs. Atomic template application currently exists at the authority service boundary only.
