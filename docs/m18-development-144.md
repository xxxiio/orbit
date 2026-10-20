# Orbit Development 144 — Host-owned rich application services

## Decision and scope

M18 target: one Rust application implementation and SqliteStore, exposed by identical logical operations through embedded and HTTPS transports; Markdown files own knowledge bodies; remote SQLite is a replica/outbox only. Flutter belongs to Atomic App. Greenfield databases require no migration compatibility.

## Changes

- Added `orbit-runtime::application_services::ApplicationServices`, hosted by `AuthoritativeHost` and using the existing single `SqliteStore` connection.
- Moved **16 rich application mutations** out of `orbit-client`'s repository-level invocation path into host-owned services: project create/status/strategy/criteria/evidence; task create/status/planning; habit create/activation/occurrences; schedule-block creation; metric target/observation; task/habit plan application.
- Each rich service delegates to the existing `orbit-application` unit-of-work use case, retaining its transaction and activity semantics; the client remains responsible for input/DTO conversion.
- Added a Rust integration scenario for project and task creation through the host-owned service and a source guard preventing the 16 client entry points reverting to direct repository calls.
- Wired the new guard into `scripts/legacy-contracts/m18-acceptance.sh`.

## Known incomplete work

**This is not M18 completion.** The HTTP API does not yet expose all rich application services through a common transport contract. The legacy relay-named command ledger and some independent canonical command semantics still exist. The public `application_store_mut` accessor remains for other native adapters. No Rust compilation occurred. A source guard is not proof of executable correctness. Cross-resource Markdown recovery and full embedded/HTTP/MCP parity are not established.

## Validation

- 16/16 rich application mutation entry points route through the host-owned service (source assertions).
- Offline Python regression suite: 17 passed.
- Worker Python regression suite: 12 passed, 10 subtests.
- Existing M18 schema, host-composition, Markdown, folder and source-contract guards passed.
- `cargo` and `rustc` not present; Rust tests added but not executed.
- Do not classify internal testing as ready: require working build, core smoke tests, and no known blocking data-loss issue.

## Progress assessment

Historical M1–M17 implementation estimates are unchanged. Existing automatic M18 source evidence remains 51/100; its rules do not award additional credit for moving these rich use-case call sites. The measurable gain here is 16 client mutation workflows rerouted via shared runtime services, reducing the specific rich-client direct mutation bypass from 16 to 0. The overall milestone is not accepted.
