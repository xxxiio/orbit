# Orbit 183 — Canonical task planning window (incomplete scheduling milestone)

## Priority
The next highest-priority unfinished daily workflow is planning and scheduling. This work adds a canonical task planning mutation across runtime, SQLite, HTTP command family, embedded client and connected outbox. This is **not** a complete schedule-block workflow, and should not be called a completed M20 iteration or have its progress percentage incremented.

## Changes
- Adds `TaskAction::SetPlanning`, setting estimated minutes, earliest start, deadline together.
- Validates estimate and temporal ordering before accepted persistence; missing tasks and invalid revisions fail within the authority transaction.
- Stores nullable planning properties on the canonical task SQLite rows; existing records receive additive columns.
- Returns planning in accepted task lists, full snapshot, receipts and client accepted projection; later rename/status/project commands preserve planning properties.
- Emits semantic field mutations for all three planning fields using history-version bases; idempotent command retries still use the existing ledger.
- Exposes `AuthorityConnection::set_task_planning` to embedded and remote transports; connected mode returns Pending and does not modify accepted state.
- Adds Rust test coverage of create-plan-replan/clear, invalid windows, receipts, read consistency, no mutation on invalid command, and remote pending isolation. Those tests have not been compiled or executed.

## Not delivered
- Schedule block create/move/delete, conflict detection, automatic time allocation, routine planning, or integrated end-to-end schedule acceptance.
- Rust compilation and integration validation (toolchain unavailable in current working environment).

## Progress
M20 stays **77% provisional**, unchanged: the stated full end-to-end feature acceptance has not been satisfied. Internal-testing readiness is not established.
