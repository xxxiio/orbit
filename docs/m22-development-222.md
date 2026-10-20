# Development 222 — Portfolio-aware Today/Next

The accepted-state Today and Next planning brief now incorporates the portfolio
recommendation engine: each actionable task has leverage metadata and the
schedule allocator places portfolio-ranked tasks into verified free windows.

## Contract

- One revision is supplied for accepted tasks, prerequisite edges, and schedule blocks.
- Recommendations remain deterministic and read-only; no calendar reservations are written.
- Tasks blocked on unfinished prerequisites are never scheduled.
- Existing accepted busy windows are merged by the established schedule planner.
- A task is indivisible: available time in separated gaps cannot be added together.
- A project-scoped view returns aggregate downstream counts, never foreign task identities.
- The Today TUI, connected TUI, CLI work brief, MCP work brief, and HTTP work brief
  all consume the same shared read model. This is not a new persistence system.
- Actions without a valid estimate are explicitly excluded unless the request
  opts into an accepted positive fallback.

## Acceptance notes

Added focused Rust test cases for calendar overlaps, unsized work, project
scoping, cycle rejection, input-order invariance, portfolio leverage,
blocked task exclusion, zero capacity and terminal presentation.

Executable Cargo build and test runs were not performed in this environment.
Broad compilation and release acceptance remain owned by M15.
