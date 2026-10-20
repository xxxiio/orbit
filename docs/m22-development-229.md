# Orbit Development 229 — M22 action-preview handoff completion

## Implemented

M22's Today action navigation now opens the actual read-only analysis for two
remaining preview destinations, instead of only presenting a textual pointer.

* Existing dependency-removal selections translate to an accepted-revision-bound
  `DependencyPreviewRequest` and invoke the existing task graph simulator. The
  report includes before/after blockers and readiness without mutating the
  authoritative command ledger. Dependency addition still requires the user
  to choose a visible prerequisite explicitly; no hidden or guessed ID is used.
* Calendar-slot selections invoke the existing accepted-only scheduling
  allocator. The terminal shows only the chosen task's proposed slot or its
  exclusion reason, never a foreign task's candidate, and explicitly requires
  separate review/approval for any reservation.
* Both embedded and connected Today interfaces implement these behaviors.
  Connected mode reads the same immutable accepted replica snapshot for
  tasks, dependency edges, and schedule blocks. Offline pending intentions are
  never merged into accepted previews.
* Request/response revision checks refuse to present stale schedules or
  dependency simulations as current. Action selection also retains the
  preexisting accepted-menu recomputation and authoritative revision fence.

## Acceptance scope and remaining verification

The M22 source feature set now covers dependency modeling, readiness, daily
and portfolio planning, scenario comparison and approval, scoped drilldowns,
agent capability discovery, and terminal action previews. This document marks
*source-implementation scope*, not verified runtime readiness. M15 continues
to own comprehensive Rust compilation, executable contract tests, multi-client
smoke workflows, packaged development builds, and data-loss blockers. The
Flutter application remains separately owned by Atomic App.

Focused Rust regression cases cover previews that are read-only, revision
changes, invalid or hidden prerequisites, cross-task calendar redaction, and
no-slot exclusion behavior. `scripts/legacy-contracts/check_m22_229.py` checks source-level
wiring. Neither these checks nor the earlier source checks prove that the Rust
workspace builds or that the tests execute successfully.
