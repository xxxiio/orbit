# Orbit TUI contract

`orbit-tui` is a thin Rust/Ratatui client. It must not duplicate Orbit business rules or mutate canonical domain tables directly. Sync transport orchestration may use the application-layer sync repository contract, while canonical entity materialization remains in application services.

## Current workflow surface

- Today: active work and active habits.
- Habits + Plan: check off active habits, preview deterministic task plans, and apply only after Orbit revalidation.
- Inbox: task capture and Inbox triage to Ready, Planned, or Cancelled.
- Projects: project lifecycle plus deterministic task-derived completion/blocked counts.
- Knowledge: local title/body/kind filtering and quick note capture.
- Sync + Agent: pending semantic/resolution outbox counts, unresolved field-level conflicts, explicit local/incoming resolution, and effective MCP composition environment.

Quick capture and triage call `orbit-client`; reads are refreshed from canonical Orbit state.

## Remaining M13 work

The high-frequency source workflows are implemented. Formal milestone closure still requires Rust compile/fmt/clippy/tests and a terminal smoke run; richer keyboard help and deeper project/habit visualization can continue as follow-up UX polish rather than blocking the semantic workflow exit criteria.

## Semantic sync conflict workflow

The Sync + Agent tab is an interactive semantic-conflict surface, not a line-diff UI. It lists unresolved entity/field conflicts with the currently materialized local value and incoming value. The selected conflict can be resolved with `l` to keep the local value or `v` to accept the incoming value.

Resolution is durable before canonical refresh: the TUI reserves the local sync sequence in SQLite, exports an immutable conflict-resolution artifact, replays the semantic replica, and materializes only conflict-free fields back into canonical repositories. Pending resolution-outbox entries are retried idempotently before a new resolution is reserved.

`ORBIT_SYNC_ROOT` may override the sync workspace; otherwise the TUI uses the same `sync` sibling of the Orbit database as the CLI.
