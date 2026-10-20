# Orbit 156 — Canonical Rich Mutations Across Local Adapters

## Milestone

M18 — Unified Application Runtime. This iteration addresses the highest-priority remaining M18 issue after 155: previously shared application services did not consistently contribute writes to the authoritative command receipt/history transaction. M19 knowledge and M20 remote parity are deliberately not broadened here.

## Source implementation

1. Added a reusable `AuthoritativeHost::accept_rich_state` command-admission helper. It wraps existing application use cases in `execute_application_command` using the same revision, immutable receipt, semantic replay and SQL transaction boundary as project/task canonical commands. Each successive mutation of a rich entity carries the previous `rich_state` semantic field version.
2. Added accepted entry points for task planning; project strategy/criterion updates; habit pause/resume and occurrence recording; routine membership; schedule creation/rescheduling; milestone transitions; metric targets/observations; and project evidence binding changes. The original domain validation and activity-unit-of-work implementation remain the source of business behavior.
3. Migrated eight CLI rich mutations plus metric target/observation and evidence operations to accepted command entry points. Core project/task/habit/routine creation and status transitions already used accepted commands.
4. Migrated ten agent-facing MCP mutation categories to accepted commands, retaining existing read-only, project-scope and unscoped-access checks. MCP callers may supply `command_id` for retry stability; for calls without an explicit entity ID the task and activity identities also remain stable under that command ID.
5. Migrated fourteen embedded OrbitClient mutation call sites onto accepted operations, including project/task/habit and schedule creation, metric and evidence operations. The client re-reads accepted entities through host-owned `ApplicationQueries`; it never opens an independent authoritative SQLite connection.
6. Added typed host-owned `habit`, `schedule_block`, and `routine` point queries for retrieving accepted results.

## Regression coverage

Added Rust unit tests for habit state retry, immutable command receipts, invalid-update rollback, task-planning acceptance, MCP idempotent retry/authorization, and embedded-client cross-domain acceptance. Added Python source-contract guards covering all three local adapter families, and integrated guards into the M18 acceptance script. Existing regression guards were updated to assert canonical accepted paths instead of obsolete plain-service call patterns.

## Important limitations

- **No Rust compilation or executable Rust test result.** `cargo` and `rustc` were not installed in this environment. The new Rust tests are only written, not passed.
- The two embedded client operations `apply_task_plan` and `apply_habit_plan` still use plain application services. These are *multi-entity* schedule mutations: representing them as a single synthetic schedule entity in the semantic history would be inaccurate. Completion requires a properly specified atomic multi-change receipt/ledger contract.
- CLI legacy knowledge/diagnostic commands still use a transitional repository bridge; they cannot be called fully consolidated until M18's remaining boundary is closed without breaking M19's Markdown knowledge cutover.
- `AuthoritativeHost::application_services()` remains public to support those two plan workflows and transitional paths. External callers could still bypass accepted history; this is a real unfinished M18 architectural criterion.
- Remote HTTP rich-workflow parity is separately assigned to M20. Offline replication acceptance is M21 and integrated release acceptance M22.
- Semantic rich-state history uses the immutable typed command payload as an ordering marker, not a complete field-by-field representation of every rich entity. A dedicated projection parity review remains necessary before M18 acceptance.

## Validation policy

The Python source guards demonstrate wiring, not behavior. The Rust acceptance requirements remain `cargo check --workspace --all-targets --locked`, `cargo test --workspace --locked`, and executable CLI workflows including restart, rollback, command replay, and multi-domain history invariants. No milestone-ready claim should be made without those checks and no known blocking data-loss issue.

## Internal-testing readiness

**Not established.** Code and source guards progressed, but the Rust build and end-to-end core smoke test have not run.

## Next priority

Finish the atomic multi-entity plan command/receipt contract and remove the remaining public plain-write bypass without introducing a second persistence implementation. Then compile and execute all M18 acceptance tests. If those checks reveal defects, address them before starting M19. Keep Flutter deferred to Atomic App.
