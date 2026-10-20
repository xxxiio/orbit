# Orbit Development 152 — Shared Runtime Contract and Executable Acceptance

## Scope

This development iteration uses Orbit 151 as its source baseline. It advances
M18 (Unified Application Runtime) while retaining the separate M19–M22
milestone boundaries. It does **not** claim M18 completion without a Rust build.

Changes were intentionally grouped into a single integrated release rather
than packaging individual CLI commands as separate numbered iterations.

## Architectural outcome

The local CLI is an embedded transport for the same application runtime used
by the server. It must not implement a second persistence model. This change
brings both workspace diagnostics and global search through the host boundary,
and establishes a comprehensive executable client regression gate.

### The runtime owns diagnostics

`ApplicationQueries::diagnostics()` returns `ApplicationDiagnostics`. It
captures the application schema version, SQLite integrity report, concurrency
policy, and project/task/habit/routine counts from the same host-owned store.
CLI status no longer opens its own repository or invokes local SQL directly.

The knowledge count is derived from physical Markdown document discovery,
not a SQL knowledge body table. This ensures workspace status reflects the
agreed filesystem-authoritative knowledge design. An invalid or conflicting
Markdown identity causes status to report an error rather than silently
counting inconsistent content.

### Global search uses the host

CLI global search now obtains projects, tasks, habits, routines, and scheduled
blocks through `ApplicationQueries`. Knowledge hits come from
`AuthoritativeHost::markdown_search()`, which searches real Markdown files.
The serialized search results include a typed `Markdown` result with stable
ID, path, and title. Search results preserve deterministic type/ID ordering.

Legacy SQL-backed CLI knowledge-management subcommands are not silently
reimplemented in this change: their rich metadata and resource semantics
belong to M19 and require a deliberate Markdown-source-of-truth cutover.

### Full-workflow regression gate

`scripts/legacy-contracts/test-m18-e2e-workflows.py` exercises the actual compiled Orbit CLI
using a temporary SQLite database, real process execution, JSON response
validation, independent process reopen, and direct SQLite integrity checks.

The harness refuses to run if an executable binary is not provided. It never
uses fake transports, in-memory mocks, or Python implementations of domain
commands. The script is a test definition, not a claim that tests passed here.

It runs independently from the Rust unit tests so failures can be isolated
between the application runtime, CLI adapter, and persisted database state.

## Acceptance categories

| Category | Coverage |
|---|---|
| Workspace bootstrap | A fresh authoritative application database opens |
| Restart | Reads after a new process observe previously accepted writes |
| Projects | Create, list, status, filtering, generated ID stability |
| Tasks | Create, list, start, complete, planning, project association |
| Habits | Create, cadence, pause/resume, occurrence history |
| Routines | Create, list, habit membership and missing-target handling |
| Schedules | Create, list, filter, task association and invalid targets |
| Search | Projects, tasks, habits, case-insensitive matching |
| Diagnostics | Schema/policy and entity counts match persisted tables |
| Integrity | SQLite integrity check and foreign-key check remain clean |
| Single schema | Retired authority entity tables are absent in fresh workspaces |
| Failures | Rejected commands preserve previously accepted records |
| Isolation | Independent subprocesses read and mutate the same workspace |

### Fail-closed cases

- Creating a task with a nonexistent project cannot leave a half-created row.
- Attempting duplicate task identities cannot silently overwrite a record.
- A failed status transition must not roll back earlier accepted work.
- An invalid occurrence identity must not create conflicting history records.
- Invalid scheduling ranges must leave application data unchanged.
- Missing routine members must not create invalid foreign-key associations.
- Read-only search and status commands must not introduce duplicate tables.
- Unrecognized CLI subcommands must not corrupt persistent workspace state.
- Reopening a workspace repeatedly must not create unexpected schema objects.
- Entity counts reported by the client must match persisted application rows.

### Source boundary regression guard

`scripts/legacy-contracts/test-m18-runtime-boundaries.py` provides fast checks before Rust
compilation. Its purpose is to catch reintroduction of known architectural
mistakes, such as new direct SQLite connections in transport adapters.

These checks assert that:

1. `AuthoritativeHost` owns rich application services and queries.
2. Host interfaces do not expose a mutable `SqliteStore` escape hatch.
3. Application services do not create independent SQLite connections.
4. CLI project/task/routine/scheduling calls follow host-owned operations.
5. CLI workspace diagnostics use host queries and real Markdown metadata.
6. CLI global search derives knowledge results from the Markdown vault.
7. Embedded, MCP, and remote-server adapters do not create a new SQL store.
8. Canonical runtime schema does not recreate duplicated authority entities.
9. Startup recovery runs before the host serves knowledge operations.
10. The Rust acceptance gate requires compilation and executable workflows.

Passing these guards does **not** prove Rust compilation or application
correctness. They serve as cheap architecture regression tests only.

## How to validate in a Rust-capable environment

From the repository root:

```bash
cargo check --workspace --all-targets --locked
cargo test --workspace --locked
cargo build -p orbit-cli --locked
python3 scripts/legacy-contracts/test-m18-e2e-workflows.py --binary target/debug/orbit
python3 scripts/legacy-contracts/test-m18-runtime-boundaries.py
bash scripts/legacy-contracts/m18-acceptance.sh
```

The final command runs the complete M18 acceptance chain, including existing
source, Python, and Rust tests. It exits with code 2 when Rust/Cargo are missing
and must not be reported as successful in that case.

For an installation where the executable binary is named differently, provide
the correct path to the executable acceptance script. Do not rename or copy an
unrelated executable to bypass the gate.

## Known limitations

### Runtime acceptance remains blocked

This environment does not expose `cargo` or `rustc`. Thus no Rust workspace
compile, Rust integration test, or executable CLI test was run. The new
acceptance scenarios describe behavior to verify and may reveal bugs when
first executed. Source-check results must not be presented as successful
runtime acceptance.

### Transitional CLI knowledge commands

A number of original CLI knowledge subcommands still use the rich legacy
application schema. Their replacement must preserve Markdown-first document
content and real folders while rationalizing metadata, resource references,
and activity projections. This is a substantive M19 capability rather than a
reason to keep creating SQL knowledge-content paths.

### Remaining command semantics

The host still contains relay-era revision and receipt implementation and the
rich application services do not have uniform command-acceptance contracts.
M18 remains incomplete until the agreed single-runtime semantics have been
reconciled and demonstrated by executable workflows.

### Source checks are not safety proofs

String-level assertions can miss dynamically dispatched bypasses. SQL foreign
key and integrity checks on a workspace do not by themselves prove domain
transactionality. Crash-injection tests must cover the actual filesystem and
SQLite protocol in M19/M22.

## Revised milestone boundaries

- **M18**: one authoritative Rust application runtime and `SqliteStore` model,
  preserving rich application functionality and removing competing write paths.
- **M19**: Markdown-authoritative knowledge, resources, folder operations,
  identity, search, relationships, and coordinated recovery.
- **M20**: embedded, HTTP, MCP, CLI, and TUI transport parity for supported
  application operations using the single shared Rust application interface.
- **M21**: remote accepted-state cache, ordered offline outbox, resubmission,
  authoritative conflict resolution, and disconnected knowledge/folder support.
- **M22**: integrated executable acceptance and internal-testing readiness.

Flutter development remains deferred to Atomic App. It is not an M18/M22
release blocker.

## Progress reporting contract

For every future development archive:

- State the previous source package, current package, and exact Library path.
- Measure added/deleted/changed files against the previous package, including
  test and documentation changes, and identify how much is production code.
- Describe complete capability changes rather than counting methods alone.
- Report tests actually run separately from tests merely introduced.
- Keep implementation estimates distinct from executable acceptance.
- Include the full M1–M22 table, noting historical M1–M17 values.
- Explicitly assess internal-testing readiness after every iteration.
- Do not declare a milestone complete without its actual acceptance contract.

Line counts measure effort and churn, not correctness. A change containing
many lines of tests is not equivalent to changing that many production lines.
