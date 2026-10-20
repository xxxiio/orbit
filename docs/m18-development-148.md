# Orbit Development 148 — CLI multi-domain application-service consolidation

## Implemented

The CLI now dispatches its entire habit command family to host-owned application services and queries, including creation, filtering, pausing, resuming, occurrence recording, and occurrence history. Advanced project strategy, criterion status, and metric target/observation writes likewise use the host-owned `ApplicationServices` path. These join the task and basic project workflows migrated in Orbit 147.

Added `ApplicationQueries::habit_occurrences` so the CLI does not need direct repository access for that read. An explicit multi-domain source guard ensures the paths are not inadvertently reverted.

## Scope not yet consolidated

Project evidence/health and other CLI command families (routines, schedules, legacy knowledge commands, search, status, today) still use the transitional repository bridge. These must be handled coherently, especially the old knowledge commands, in M18/M19. The shared application service also does not yet prove equivalence with remote HTTP command execution; this is M20 acceptance work.

## Validation limits

Python offline sync suite and source guards passed. The execution environment lacks cargo/rustc; Rust compilation and executable CLI smoke tests are **not verified**. Source checks do not imply runtime parity or M18 acceptance.

## M18 estimate

M18 estimated implementation 76% (previous 73%): a rough +3 points for removing another complete core CLI command family from direct repository access and moving project strategy and metric writes. This estimate is not executable acceptance. M18 is incomplete.
