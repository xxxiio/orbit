# M18 canonical project/task projection — integrated worktree

## Delivered

Canonical project and task commands now synchronously project the canonical
fields into the existing, richer application repository using SQLite triggers.
The triggers operate on the command writer's connection *inside the same
transaction* as the authority event, revision and accepted receipt. Therefore
an aborted authority transaction cannot leave a newer local project/task
projection, and a failed rich projection also aborts the authority command.

- Projects: `id`, `name`, and `status` are projected.
- Tasks: `id`, `title`, `status`, and `project_id` are projected.
- Rich application-only fields (project mode/objective/criteria and task
  planning/milestones) are retained; task status transitions do not overwrite
  these fields.
- Conflicting pre-existing rows are rejected, never overwritten.
- Existing divergent authority/application projections fail closed on host
  startup (no automatic migration/import).
- Direct legacy changes to canonical fields and deletion of canonical-owned
  rows are rejected at the SQLite level, across connections.
- Standalone SQLite contract test covers create, update, ancillary columns,
  direct-write guards, collision detection and transaction rollback.

## Unfinished architecture

This is a *transactional projection bridge*, not a declaration of M18
completion. The host still contains `RelayStore` and `SqliteStore` connections,
which retain distinct interfaces. Existing local-only records are not imported
into canonical history, and non-project/task domains have not been consolidated.
In-memory hosts use independent SQLite connections and deliberately do not
install the on-disk projection bridge. Complete embedded/server parity and
runnable Rust acceptance remain unverified.

A greenfield canonical workspace with no legacy project/task state is the
supported projection-bridge scenario. Older mixed workspaces fail closed where
projection mismatches are detected; there is no implicit data migration.

## Validation evidence

- `python3 scripts/test-m18-projection-bridge.py` — passed (real SQLite).
- `python3 scripts/test-authority-offline.py` — 17 passed.
- Agent/automation Python suites — 12 passed, 10 subtests passed.
- M18 source guard scripts — passed.
- Rust compiler and Cargo unavailable; Rust tests were not executed.

## Remaining M18 exit criteria

1. Converge the remaining local/canonical repository services and transactions,
   including habits, planning, project modes, metrics, and knowledge resources.
2. Complete equivalent embedded, HTTP, local/remote MCP, CLI/TUI, and offline
   command behaviors across supported domain operations.
3. Verify crash recovery (filesystem+SQLite), data integrity, and build/test
   acceptance in a working Rust environment.
4. Validate a runnable packaged/development build and core smoke workflows.

Flutter is explicitly deferred to the separate Atomic App project.

## Expanded canonical evidence integration

The same transactional bridge now projects canonical habits, occurrences,
metric targets, and observations into the richer local repository. It handles
weekly/daily cadence normalization and canonical increasing/decreasing target
semantics mapped to the local at-least/at-most representation. It preserves
habit activation and occurrence duration columns owned by local workflows.
Guards prohibit legacy modification/deletion of canonical-owned evidence.
The SQLite contract test exercises evidence projection, field preservation,
and rejected direct mutations.

Canonical-owned row identities are immutable through the rich application
repository, including project/task/habit primary keys and habit/metric
observation identity columns. This blocks a subtle divergence that would
otherwise survive a successful projection transaction.
