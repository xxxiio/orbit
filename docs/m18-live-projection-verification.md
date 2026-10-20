# M18 shared projection consistency checkpoint

This worktree adds a read-time consistency gate for the current transitional
SQLite design. Canonical projects, tasks, habits, occurrences, metric targets,
and observations are compared to their rich application projections when the
host serves project/task lists or its canonical workspace snapshot. It catches
external SQLite changes that occur **after** the host opens. `:memory:` hosts
skip this check because the rich repository and canonical authority occupy
separate SQLite handles there and the canonical projection triggers are absent.

`python3 scripts/test-m18-live-projection.py` executes the exact verification
SQL extracted from Rust against independent SQLite connections. It confirms
that a missing projection, an external project mutation, and an altered habit
target are rejected. This is an executable SQLite check, **not** a Rust
compilation or system-level acceptance test.

## Unresolved M18 blockers

- The rich application repository (`SqliteStore`) and canonical store
  (`RelayStore`) are still distinct implementations. Read-time checks protect
  against some drift; they do not unify their command semantics.
- Local feature-rich commands still write through a separate application path;
  canonical clients can follow a different route.
- Knowledge files, folder commands, MCP and remote offline acceptance still
  need compiled Rust end-to-end validation.
- No Rust compiler is installed in this environment. No release-readiness or
  internal-testing status is asserted.

Priority remains completing actual application-service unification rather than
adding more narrowly scoped source contracts.
