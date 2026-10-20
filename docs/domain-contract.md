# Orbit Core domain/application contract

MILESTONE-0001 establishes the semantic boundary that all later adapters must respect.

## Aggregate ownership

- `Project` is an outcome-oriented aggregate and does not embed milestone/task collections.
- `Milestone` references exactly one `ProjectId`.
- `Task` may be standalone, project-scoped, or project+milestone-scoped.
- `Habit` is not a recurring task and has its own cadence/lifecycle.
- `ScheduleBlock` may be standalone or target a task/habit by ID.
- `ActivityRecord` records semantic history independently of persistence diffs.

## Cross-aggregate rules

Rules requiring repository lookup belong in `orbit-application`, not `orbit-domain`:

- project-scoped task -> project must exist;
- milestone-scoped task -> project and milestone must exist;
- milestone-scoped task -> milestone must belong to the selected project;
- milestone creation -> parent project must exist;
- task/habit schedule target -> referenced entity must exist.

Standalone tasks and standalone schedule blocks require no synthetic container.

## Dependency direction

```text
orbit-domain
    ^
orbit-application
    ^
adapters / CLI / MCP / Flutter bridge
```

`orbit-domain` and `orbit-application` must remain independent of Flutter, SQLite, sync transports, AI SDKs, MCP transports, and terminal UI frameworks.

## Time

Domain entities store timestamps as Unix milliseconds where necessary. Wall-clock access is injected through the `Clock` application port so tests and future replay/sync behavior remain deterministic.

## Progress

Progress is normalized to basis points (`0..=10_000`) and supports task-derived, numeric metric, manual, external, and agent-reported sources. External/agent provider payloads must be normalized before entering the domain.

## Activity

Activity records describe semantic changes (`Created`, `StatusChanged`, `ProgressUpdated`, etc.). The persistent append-only journal is a MILESTONE-0002 adapter concern; MILESTONE-0001 defines only the semantic record and repository port.

## Validation rule

A source-complete task is not considered runtime-verified until the workspace passes:

```text
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```


## Task planning metadata

Tasks carry optional durable planning facts: estimated duration in minutes, earliest-start time, and deadline. Estimates must be positive when present; earliest-start cannot be after the deadline. These are domain facts available to every client, not planner-private state.
