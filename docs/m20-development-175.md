# Orbit 175 — M20 accepted project read models

## Priority selection

The highest-priority unfinished category remains **core daily workflows and consistent embedded/remote contracts**. Project-detail views and status summaries are required by future CLI/TUI/MCP and desktop clients. This is an incremental read workflow delivery, not the higher-impact outstanding rich-domain write-command wiring. No lower-priority standalone durability/security/release work was selected.

## Delivered

- `AuthorityConnection::project_overview(project_id)` returns project identity, assigned tasks, revision and fingerprint from **one** accepted snapshot, avoiding mixed-revision project/task reads.
- `task_status_counts(project_id)` provides accepted-only project/inbox status counts with forward-compatible status keys.
- `metric_history(metric_id)` provides a named accepted metric-observation read API for clients; no pending writes are projected.
- Added a Rust behavioral regression exercising embedded/remote parity, project scoping, pending-write exclusion and input validation.

## Validation

- Python server tests: 12 passing.
- Python offline authority tests: 17 passing.
- M20 Python source guards: passing.
- Rust compilation and behavioral tests: **not run** (Cargo unavailable). These APIs are therefore not release-verified.

## Milestone progress

M20 estimated **70% implementation** (prior 68%). This is an approximate scope-based estimate, **not** compiled acceptance evidence. M18 and M19 remain at provisional 99% implementation estimates with acceptance pending; historical M21 65%, M22 10%, M15 8% unchanged. Milestone percentages are not additive.

## Next higher-priority work

The next iteration should implement the outstanding rich planning/routine/schedule/strategy/milestone write-command transport and unified client dispatch across embedded and server modes. The rich accepted commands already exist in the Rust runtime, but not as a complete connected `AuthorityConnection` command family. Do not call M20 complete before compiled end-to-end acceptance. Flutter remains external to this core.

## Internal testing

**Not ready / not verified**. No executable Rust build and end-to-end multi-client smoke were demonstrated in this environment. No claim about freedom from blocking data-loss defects is possible.
