# M22 Development 217 — Today/Next terminal integration

## Why this development is next

Dependency-ready suggestions, schedule-aware planning, milestone bottlenecks and hypothetical completion impact were already implemented as read-only Rust services by Development 216. Their major missing daily-workflow entry point was the *actual* terminal Today surface, which still showed a flat unranked task list. Bringing those existing product capabilities into both local and connected terminals is higher priority than additional planner APIs or standalone durability work.

## Delivered product behavior

- The embedded terminal's Today tab now displays a single accepted-state Today/Next brief rather than an independent filter of task statuses. Active habits remain visible alongside it.
- The terminal's read-only completion-impact view explains which tasks would become actionable if the currently navigated task were completed. Escape returns to Today without changing any authoritative records.
- The embedded Today view supports line scrolling (`j/k`), task-header navigation (`J/K`), bounded planning horizon (`[` and `]`), explicit refresh (`r`), and project-specific drilldown (`F` from Projects, `F` on Today to clear).
- A failed refresh keeps the previous accepted Today display and visibly marks it stale rather than silently replacing it with an empty list.
- The connected terminal supports `plan.today` with a bounded hours/limit/project request and `plan.impact <task>` in its command palette; both evaluate a *single* previously accepted snapshot. Offline queued intentions never modify an accepted plan before the server acknowledges them.
- Connected planning is read-only: task navigation and impact simulation cannot queue, flush, or write authoritative commands. An absent/malformed accepted snapshot is an explicit error.
- Basic regressions cover accepted-state-only Today, controls, empty-selection handling, navigation, invalid input, and the offline connected client, including no pending commands after planning.

## Scope and acceptance

This is an integrated terminal UX feature; its data evaluation reuses the existing authoritative planning services. It does not build a second planner, does not create calendar reservations, and does not silently commit task-status changes. It does not introduce any Git or peer-replication path.

The source is not yet Cargo-compiled in this environment. All new regression tests require Rust toolchain execution. Broad compilation, packaged build, and runnable smoke acceptance are M15 responsibilities. Do not interpret this iteration's source-level completion as verified internal-testing readiness.

## Next priority

Agent capability discovery and consistent CLI/MCP/TUI task readiness semantics remain substantial M22 feature work. Feature-first delivery still precedes M15 release acceptance.
