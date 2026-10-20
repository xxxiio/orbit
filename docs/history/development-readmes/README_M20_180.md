# Orbit 180 — Canonical task editing

## Why this iteration
The highest priority is usable authoritative daily workflows, not additional read-only dashboard or isolated hardening. The existing minimal command contract could create tasks and change status but could not rename or assign them to projects; this prevented basic project/task planning with the shared embedded/remote command pipeline.

## Delivered
- Added `TaskAction::Rename` and `TaskAction::AssignProject` (including `None` for inbox/unassigned).
- The authoritative task handler validates title and referenced project, writes the task row, a semantic field mutation with its most recent field-version base, history revision, and immutable command receipt in one SQL transaction.
- Both actions use the existing `/v1/authoritative/task-commands` endpoint and `AuthorityConnection::task_command` remote outbox transport without introducing another authority owner.
- Added Rust regression scenarios for duplicate delivery, rename, assignment, unassignment and missing project, plus a Python source guard.

## Validation / limitations
Rust toolchain was not available here; Cargo compile and behavioral test results are **not claimed**. The Python source guard validates required code structure only. Rich planning, routines, scheduling, strategy and milestone APIs remain incomplete across remote and embedded paths. Future work must build/execute Rust tests and verify two-client behavior before claiming internal-testing readiness.

## Progress
M20 estimated implementation 75% (previous 73%), provisional; acceptance unverified. The priority remains user-facing unified rich-domain operations. No higher-priority feature category should be displaced by standalone hardening. Flutter remains out of scope.
