# Orbit 089 — server-authoritative projects and tasks

## Implemented

The self-hosted server now accepts project creation through the *same* canonical SQLite
write transaction protocol already used for tasks: shared `authority_commands` idempotency
receipts, `relay_events` semantic journal, monotonic revision, `accepted_projection`,
and server-owned `authority_projects` materialization.

- `GET /v1/authoritative/projects` returns `{revision, projects}`.
- `POST /v1/authoritative/project-commands` accepts
  `{command_id, expected_revision, project_id, name}` and returns a stable receipt.
- A command ID cannot be reused by another actor or payload.
- Project creation and task creation share the same server revision sequence.
- Existing legacy histories remain cutover-blocked; no automatic migration or deletion.

## Remaining M18 feature gaps

Project updates, task-to-project assignments, habit and metric domain services,
offline client reconciliation, browser/agent canonical state integration, and lossless
legacy import remain unfinished. The server remains optional. The legacy relay path
remains quarantined after authority activation.

## Verification

Rust regression test source was added but cannot be executed in this environment
(no Cargo). Execute M15 evidence collection on a Rust-enabled machine.
