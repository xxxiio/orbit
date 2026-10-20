# Orbit 176 — M20 accepted dashboard projection

## Priority decision

Core daily workflows remain the highest-priority unfinished category. Providing one consistent accepted dashboard view across embedded and connected clients serves CLI/TUI/MCP and future application consumers without inventing a second authority. Rich-domain accepted write transport is still higher impact and remains outstanding; this iteration does **not** claim to deliver it. No dedicated standalone hardening, public plugin, or Flutter work was selected.

## Implementation

- Added `AuthorityConnection::dashboard_summary` using a **single** accepted snapshot for project, task, habit, occurrence, metric and observation counts, unassigned task count and task status breakdown.
- Returns `None` for an uninitialized remote replica, distinguishes no accepted snapshot from an accepted empty workspace.
- Retains revision and history fingerprint in the summary and reports pending offline outbox count separately without projecting unaccepted tasks.
- Fails on malformed task statuses and missing accepted collections instead of manufacturing counts.
- Added Rust behavioral regression for embedded/remote parity, missing remote snapshot and offline-pending separation.

## Validation performed

- Python server unittest suite: 12 tests passed.
- Python offline authority suite: 17 tests passed.
- Four M20 Python source-contract guards passed.
- Cargo and rustc unavailable: Rust compile and behavioral regressions not executed. No two-client real-server acceptance demonstrated.

## Progress and readiness

M20 provisional source-implementation estimate **71%** (prior **70%**). This number is not compiled acceptance. M18 99%, M19 99%, M21 65%, M22 10%, M15 8% retained as previously reported provisional percentages. No overall weighted estimate is asserted.

**Internal-testing status: not verified.** At minimum a runnable Rust development or packaged build, core smoke flows, and review of blocking data-loss defects are required.

## Next priority

Complete rich-domain command types and accepted remote server dispatch (planning, routines, schedules, project strategies, milestones) through the same authority admission and receipt semantics, then compiled end-to-end smoke. Flutter is intentionally outside this project iteration.

## Completion priority review

Core daily workflows are not yet complete; structured knowledge/automation, internal packaging, and plugin execution must not displace rich-domain connected write parity. This iteration adds a real user-facing accepted summary but does not close the higher-priority rich-write gap.
