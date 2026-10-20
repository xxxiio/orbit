# Orbit 151 — shared query and project workflow consolidation

## Scope and design
Continue M18 Unified Application Runtime without touching the separate M19–M22 acceptance scopes. The same `AuthoritativeHost` owns SqliteStore and rich domain services in local-only and remote-hosted deployments.

## Integrated changes
- Added typed, host-owned queries for projects, tasks, milestones, project evidence and combined reports, canonical and mode-aware project health, activity journal, and diagnostics.
- Moved MCP evidence, health, task, and milestone lookups to these host-owned queries.
- Moved embedded-client project lookups, evidence and health reads, schedule/occurrence reads, and activity cursor/polling onto the same query API.
- Moved TUI project-health queries through the host query API; TUI term references remain scheduled for M19.
- Completed the CLI advanced project evidence binding, reporting, and health execution through shared host services; removed the obsolete duplicate project command implementation.
- Routed the CLI today queue through host-owned task queries.
- Added Rust query assertions and a cross-adapter regression guard; wired the guard into the M18 acceptance script.
- Explicitly declared `orbit-progress` as a runtime dependency for the moved health query types.

## Known limitations
- No cargo/rustc available: do not claim Rust compiles or executable acceptance passes.
- CLI knowledge, cross-entity search, and workspace diagnostics still rely on transitional repository access, pending M19 integration and final M18 cleanup.
- Other application command-ledger semantics and HTTP capability parity are not yet proven; do not mark M18 complete.
- Source checks are not a substitute for runtime behavior, particularly preservation of rich planning and recovery semantics.

## Verification
- Existing M18 source contracts: passed.
- New cross-adapter source contract: passed.
- Remote offline synchronization Python tests: 17 passed.
- Python agent/automation workers: 12 passed, 10 subtests.
- Python syntax checks and bash acceptance-script syntax: passed.
- Rust compile and tests: not run; compiler unavailable.

## Acceptance priorities
Finish eliminating remaining legacy application mutation escape hatches, then carry out full Rust compilation and core local-authority smoke tests. M19–M22 remain distinct milestones for Markdown knowledge, transport parity, offline sync, and integrated acceptance. Flutter is deferred to Atomic App.
