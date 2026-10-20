# Orbit 174 — M20 accepted query projections

## Priority and scope

M20's highest-priority unfinished feature is one shared domain/read contract for embedded and remote clients. Before another hardening-only iteration, this change extends useful day-to-day read workflows on `AuthorityConnection`: project-scoped and unassigned tasks, status-filtered tasks, and habit-specific occurrences. This is a feature iteration, not standalone integrity hardening. It does not implement the remaining remote rich-domain write commands.

## Implementation

- Added `tasks_for_project(Some(id))` and `tasks_for_project(None)` with explicit empty-id rejection. Remote results use the last accepted snapshot; pending tasks cannot leak into project views.
- Added `tasks_with_status(status)` and `habit_occurrences_for(habit_id)` using the same accepted projection boundary. Invalid blank filters fail explicitly.
- Added Rust behavioral regression covering local/remote task-filter parity, unassigned task filtering and exclusion of pending remote writes.
- Added a source-contract guard for the new read API.

## Verification and limits

- `scripts/legacy-contracts/test-m20-accepted-query-filters.py`: PASS (source guard).
- Existing M20 unified accepted reads, integrity and idempotent outbox source guards: PASS.
- `python3 -m unittest discover -s deploy/server -p 'test*.py'`: 12 passed.
- `python3 scripts/test-authority-offline.py`: 17 passed.
- Rust compilation and Rust behavioral regression: NOT RUN (Cargo/Rust unavailable).

## Milestone progress report (provisional implementation estimates)

M20: **68%** (previously 65% in the M19 historical status); M18: **99%** provisional implementation estimate (separate stricter evidence metric 33/100; runtime acceptance unproven); M19: **99%** provisional implementation estimate; M21: **65%** historical provisional estimate; M22: **10%** historical provisional estimate; M15: **8%** historical provisional estimate. These are **not measured completion or acceptance percentages**. M20's increment reflects added accepted read functionality, not validation of rich remote commands.

Do not aggregate these into an overall project percentage: scopes and historical methods are not comparable. M20 remains IN PROGRESS and overall internal-test readiness is NOT VERIFIED. The Rust executable/smoke gates and absence of blocking data loss have not been demonstrated.

## Highest-priority next work

Build the common accepted command envelope for rich planning/routines/schedules/project strategies/milestones, wire remote transport and CLI/TUI/MCP against it, retire conflicting legacy direct SQL write paths. No Flutter in Orbit core. Next iterations must prioritize those user-facing operations above dedicated hardening work; essential regression checks travel with each feature.
