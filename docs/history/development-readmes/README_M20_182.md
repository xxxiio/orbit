# Orbit 182 — Cohesive task review and refinement workflow

## Priority rationale
Core daily workflow takes priority over standalone hardening, release preparation and new client surfaces. Task review previously required callers to hand-compose rename, reassign and status commands and manually fence revisions; this iteration delivers a complete task-review orchestration feature over the existing authority engine. Rich planning/scheduling, routines and project strategies remain higher priority for subsequent functional iterations.

## Delivered
- Typed `TaskRefinement` manifest supports multi-task review: optional rename, project assignment/unassignment, and status transition per task.
- Full-manifest preflight validation prevents empty edits, repeated task IDs, bad names, invalid destination identity, invalid statuses, excessive batch size and revision overflow before side effects.
- Deterministic command IDs and consecutive expected revisions allow safe retry of an interrupted ordered review.
- Unified `AuthorityConnection::refine_tasks` uses the same embedded/remote command dispatch and durable outbox. Remote pending commands never masquerade as accepted state.
- Detailed `WorkflowFailure` identifies partial acceptance; no false atomicity claim.
- Four Rust regression tests added for invalid requests, command stability, remote pending isolation and embedded failure.

## Acceptance and limits
Rust compiler/toolchain absent. New Rust behavior and tests not compiled or executed; only static contract checks performed. Valid status strings are preflight checked; actual status transitions, task existence and project existence remain server-side authority checks. Do not claim internal-testing readiness without runnable build, smoke workflows and no blocking data-loss defects. Flutter stays in Atomic App.

## Provisional progress
M20 79% implementation estimate (previous 77%). M15 8%, M18 99%, M19 99%, M21 65%, M22 10% retained. These are not verified acceptance percentages.
