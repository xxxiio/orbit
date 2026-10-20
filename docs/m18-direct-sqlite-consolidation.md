# M18: Direct application entity storage — development worktree

## Architectural decision

Local and remote clients use the same Orbit application contracts. Embedded
calls invoke the host in-process; remote calls reach the same host over HTTPS.
Both hosts use the feature-rich `SqliteStore` application entity schema and
Markdown files. The remote client maintains a nonauthoritative cache/outbox.

## Changes in this worktree

- Canonical project and task commands now read and write `projects` and `tasks`
  directly in the same SQLite transaction as the command receipt and history.
  Existing rich planning fields remain on the same record.
- Habit and metric commands now read and write `habits`, `habit_occurrences`,
  `metric_targets`, and `metric_observations` directly. The wire/API cadence and
  target-direction vocabulary is mapped to the application storage vocabulary.
- Removed creation of all six shadow `authority_*` entity tables and rejected
  previously initialized shadow-schema databases rather than silently merging
  incompatible data. No migration path is intended in this greenfield project.
- Removed the SQLite projection trigger/verification bridge and its obsolete
  tests; replaced with executable SQLite contract tests for the single entity
  schema (`scripts/legacy-contracts/test-m18-unified-entities.py` and
  `scripts/legacy-contracts/test-m18-single-project-task-store.py`).
- Preserved canonical command idempotency, revisions and receipt processing for
  project/task/habit/metric commands as currently implemented; changed only
  their entity tables and data projection.
- The runtime now owns **one** `SqliteStore` and one SQLite connection. The
  command ledger borrows that store for its SQL operations rather than opening
  another SQLite connection, including in-memory tests. This removes the
  previous shared-memory-URI workaround altogether.

## Honest limitations

This is a **substantial source-level cleanup, not full M18 completion**.
There is now **one owning SQLite connection**, held by `SqliteStore`. The
`RelayStore` name survives as a command-ledger wrapper (not a second DB).
The ledger still has its old relay-event semantics, and some rich application
repository mutations can bypass the canonical idempotent command acceptance
interface. Consequently **single business-rule ownership and unified revision
semantics are not yet demonstrated**, even though the physical persistence
split and the six shadow entity tables have been eliminated.

The rich local knowledge tables and old sync facilities still exist and
require carefully preserving metadata/resources while eliminating SQL-body
knowledge storage in favor of Markdown as content authority. CLI/TUI/MCP
parity, offline conflict resolution, and crash recovery have not been verified
by compiling or running Rust. The available environment has no `rustc` or
`cargo`; our SQLite/Python tests cannot establish Rust correctness.

### Executed checks

- `python3 scripts/legacy-contracts/test-m18-unified-entities.py` — passed
- `python3 scripts/legacy-contracts/test-m18-single-project-task-store.py` — passed
- `python3 scripts/legacy-contracts/test-m18-host-composition.py` — passed
- `python3 scripts/legacy-contracts/test-m18-markdown-cutover.py` — passed
- `python3 scripts/legacy-contracts/test-m18-no-sql-knowledge.py` — passed
- `python3 scripts/legacy-contracts/test-m18-folder-contract.py` — passed (source only)
- `python3 scripts/test-authority-offline.py` — 17 passed
- `python3 -m pytest -q deploy/server/test_agent_worker.py deploy/server/test_automation_worker.py` — 12 passed, 10 subtests

### Acceptance gate

`bash scripts/legacy-contracts/m18-acceptance.sh` must run successfully in a Rust-equipped
environment. The script will exit with code 2 here because rustc/cargo are
unavailable. Build and runtime errors might still require corrections.

Internal-testing readiness is **NOT established**. Flutter remains deferred to
Atomic App.

## M18 remaining completion gates

1. Retire residual relay/peer-sync domain paths and rehome the useful command
   revision/receipt machinery in the shared application service.
2. Ensure all rich planning, project mode, metric, habit, and knowledge writes
   are issued through one application command model; prevent backdoor writes.
3. Complete filesystem-backed resource metadata and canonical folder behavior.
4. Prove embedded/HTTP/MCP/CLI/TUI and remote offline parity.
5. Compile and execute Rust acceptance and crash recovery suites.

**No M18 completion percentage is inferred from these SQL/Python checks.**

## Source-evidence progress (not completion)

The versioned source-evidence script currently reports **51/100** with no
executable acceptance established. This is deliberately capped: source checks
show the presence of code but do not verify business-rule correctness, transport
parity, or lack of data loss. Previous numbers were 33/100 (older source-evidence
baseline) and 75.6% (a different implementation coverage model); **these are not
interchangeable**. There is no verified M18 completion percentage yet.

The objectively measurable architectural cleanup in this pass is:
- Six redundant authoritative entity tables no longer created.
- One projection/consistency bridge removed.
- One SQLite owner/connection per authoritative host (not two).
- One set of app entity rows now read and written by both command and rich
  application code.
