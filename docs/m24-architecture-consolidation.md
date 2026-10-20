# M24 — Framework Architecture Consolidation

Status: **complete (Development 274), historical record.** M24 source consolidation
is complete and compiles. This milestone is a behavior-preserving architecture effort,
not release acceptance, which belongs to M15.

## Priority rationale

M23 source implementation is complete. The next priority is reducing divergent
application/authority boundaries before adding more features. No unfinished core
feature milestone outranks this approved M24 work. This iteration targets the
high-priority application-service boundary before CLI module decomposition,
offline queue unification, or worker consolidation.

## Acceptance map

The table below records the plan as written at M24 start; the "Status" column is a
**historical opening snapshot** (Development 261), not the final state. All four
iterations were subsequently delivered in source; see the per-development sections
from 262 onward and the Development 274 closure.

| Iteration | Cohesive scope | Target contribution | Status at opening |
|---|---|---:|---|
| I1 | Centralize authority/application launch ports; document canonical topology and invariants; converge remaining agent command paths | 25 pp | Partial (later complete) |
| I2 | Split CLI, client and persistence modules by coherent capability without changing public commands | 25 pp | Not started (later complete) |
| I3 | Unify or deliberately isolate offline pending commands; align generic runtime capabilities | 25 pp | Not started (later complete) |
| I4 | Clarify machine workers, retire obsolete entry points, reconcile docs and public API contracts | 25 pp | Not started (later complete) |

**Closure:** M24 source consolidation is complete and the workspace compiles.
Broad executable acceptance — build, integration, live Herdr, packaging — belongs
to M15, not to this milestone.

## Current deliverable (Development 261)

- Introduced application-owned `AgentLaunchAuthority` port. The authority
  adapter now lives in the storage crate that owns `SqliteStore`.
- Refactored `dispatch_prepared_job` to depend only on the authority and
  runtime ports; it no longer directly imports SQLite.
- Added in-memory port tests for launch fencing and ambiguous outcomes.
- Replaced obsolete 'future server' architecture statements with explicit
  local and connected server-authoritative topologies.

## Invariants

- Authority launch claims and task-readiness checks are atomic.
- No retry after an ambiguous external launch outcome.
- No Herdr subprocess execution in the authoritative HTTP server.
- Agent runtime completion never implies task acceptance.
- Connected offline intents are not authoritative until accepted.
- Herdr is an optional swappable adapter.

## Progress method (as written at Development 261; historical)

M24 implementation progress: 10% (first cross-crate authority-boundary slice).
At Development 261 I1 was not yet complete, so its remaining 15 percentage points
were not credited. Compile/integration/Herdr acceptance belongs to M15.

## Next I1 focus (historical plan; superseded by Development 262 onward)

Continue moving other authority coordination operations behind application ports,
then consolidate shared command semantics across CLI, MCP and HTTP. Do not split
large files mechanically or create a second canonical authority.

## Development 262 — Monitoring authority port

Before: `orbit-runtime::agent_monitor::reconcile_job` directly referenced
`SqliteStore`, `AgentRuntimeObservation`, and `RuntimeActivity`. As a result,
launch dispatch used the application-owned port while monitoring bypassed it.

Now: `orbit-application::agent_runtime::AgentObservationAuthority` defines the
minimum operations for fetching an authority-owned job, reading its durable
launch token, and recording a compare-and-swap observation. `AgentObservedSession`
and `AgentActivity` carry normalized observations across the port. The
`orbit-storage` adapter owns the mapping into persistence representations.
`orbit-runtime::agent_monitor` depends only on these application ports and the
runtime adapter; `AuthoritativeHost` continues composing the concrete storage
adapter. Both launch and monitor share the same dependency direction.

No schema or HTTP shape changes. Reconciliation still checks job revision before
contacting the runtime, refuses unclaimed/terminal jobs, and relies on the
storage transaction to check the claim token and compare-and-swap revision.
Unreachable observations never imply termination or task acceptance.

In-memory Rust unit tests document the no-dispatch invariant, stale revision
behavior, lost claim behavior, review-boundary mapping and unreachable state.
Broad executable checks and any defects found there remain within M15.

**Implementation progress:** M24 16% (+6 percentage points, source scope).
I1 remains open: command use cases across CLI/MCP/HTTP have not yet been
consolidated. Internal-testing readiness is not established (M15 acceptance).

## Development 264 — CLI capability boundaries

The public `orbit` executable continues to own argument parsing and deployment selection. Local agent, task, project, habit/routine and schedule command handling now lives in dedicated modules, all delegating to the same `AuthoritativeHost`; the existing connected agent path is unchanged. This extracts over 900 lines of capability-specific handling out of the root CLI module without creating new executables, databases or parallel domain logic. M24 implementation scope: 31% (+8 pp from Development 263). Full build and integration acceptance remains M15.

## Development 265 — Client workflow and planning read-model boundaries

Moved 657 lines of cohesive implementation out of `orbit-client/src/unified.rs`:

- `unified/workflow.rs` owns revision-aware multi-command workflow manifests, project bootstrap, task refinement, and task planning input-to-intention translation. Workflows retain their existing explicit non-transactional semantics.
- `unified/planning_queries.rs` owns the shared planning/readiness query methods of `AuthorityConnection`, preserving the distinction between local authority reads and server-accepted connected projections.
- `unified.rs` retains the public `AuthorityConnection` facade, mutation routing and deployment behavior and re-exports the previous workflow types without changing their public paths.

This is a behavior-preserving module ownership refactor, not a new synchronization path or authority. Existing client API consumers can still refer to `orbit_client::unified::ProjectBootstrap`, `DomainWorkflowStep`, and planning methods on `AuthorityConnection`. No schema or wire-contract changes.

**M24 source progress:** 38% (+7 percentage points from Development 264), reflecting the completed I2 client capability extraction. The remaining I2 work includes the oversized authority/client and storage facades. Broad compilation and runtime validation remain M15 responsibilities.

## Development 266 — Connected authority transport and Markdown mirror boundaries

**Why now:** I2's oversized connected authority module mixes device-authenticated HTTP
transport, snapshot/outbox coordination, and accepted Markdown mirror publication.
After the previous CLI and unified-client extractions, this was the next largest
concentrated capability boundary. No higher-priority unimplemented daily-workflow
feature is being displaced; the approved M24 structural consolidation remains the
active implementation milestone. The proposed generic outbox consolidation stays I3.

Extracted two cohesive implementation modules from `orbit-client/src/authority.rs`:

- `authority/http.rs` owns device-enrolled HTTP requests, base URL policy,
  conflict mapping, and redirect-free client configuration. The existing
  `orbit_client::authority::AuthorityHttp` symbol is re-exported unchanged.
- `authority/markdown_mirror.rs` owns accepted Markdown cache reads, full-mirror
  reconciliation, manifest integrity verification, transactional publication,
  and cache provenance. It implements the original `AuthorityReplica` methods,
  preserving the public method paths and storage schema.

`authority.rs` retains the canonical `AuthorityReplica` type, server-snapshot and
pending-command coordination, and the preexisting authority tests. Neither a new
replica nor a second outbox was introduced. The refactor deliberately avoids
changing HTTP routes, JSON payloads, cache storage, or command acceptance rules.

**Source-scope progress:** M24 45% (+7 percentage points from Development 265).
I2 remains open for storage facade decomposition. No build acceptance is claimed;
broad compilation/integration and any discovered fixes remain M15.

**Internal-testing assessment:** not ready for declaration until M15 provides a
runnable build and successful core smoke workflows without blocking data loss.
The next highest-priority M24 work is storage facade decomposition, followed by
I3's offline intent architecture review and consolidation.

## Development 267 — SQLite storage facade decomposition

**Priority justification (before):** M24-I2 storage modularization remains the highest-priority unfinished approved framework work. I1's application boundary and CLI/client extractions are already represented in previous iterations. Offline-command consolidation (I3), worker protocols and documentation retirement (I4), and broad executable validation (M15) should not preempt this I2 structural work.

Split the `orbit-storage` root module according to ownership, retaining its original public `SqliteStore` type, `Deref` compatibility boundary, exported agent runtime observation types, schema version, repository adapters, SQL text, and transaction semantics. New private modules: `store_core` (connection/configuration/migrations), `project_criteria` (criteria hydration), `unit_of_work` (savepoint lifecycle), `row_mapping` (domain-to-SQL codecs), `sqlite_helpers` (I/O and SQL helpers), and `storage_tests` (existing regression suites). The former 3,252-line `lib.rs` is now a ~103-line crate facade; ~1,235 lines of runtime code and ~1,926 lines of test code reside in responsibility-focused modules. No schema, public method signature, executable, HTTP route, or authority-mode change was intended.

**Implementation progress:** M24 45% → 52% (+7 percentage points; source-scope estimate). This iteration is a substantial I2 implementation deliverable; it does not claim M15 runtime acceptance. Essential source-level structural checks are part of the feature work; Rust compilation and full integration testing remain M15.

**Priority justification (after):** I2's storage entrypoint is decomposed but storage modules and client internals can still be improved. Finish that higher-priority I2 work before starting I3 agent-outbox consolidation, then I4 worker documentation and obsolete-code retirement. Internal-testing readiness is not established until M15's executable requirements are met.

## Development 269 — one connected command outbox

**Priority justification (before):** M24-I3 unified pending command ownership is
higher-priority unfinished work than I4 worker cleanup. I2 component splitting
has already been delivered. There is no newly identified higher-priority daily
workflow implementation blocking the approved M24 refactor.

Agent intentions now use the canonical `AuthorityReplica` SQLite outbox and
its global ordering, authenticated flush, receipts, and conflict barriers.
The separate file-backed agent journal and compatibility facade are removed.
`orbit agent pending` shows agent rows in the global queue; `orbit agent flush`
flushes the **entire** global queue, rather than overtaking earlier pending
domain intents. Greenfield replica recreation is required when upgrading from
an earlier restricted schema, and any legacy JSON outbox must be reconciled
before cutover; no destructive migration is attempted.

**Progress:** M24 58% → 66% (+8 percentage points provisional source scope).
**Priority justification (after):** I3 runtime capability consolidation remains
before I4 worker/deployment documentation cleanup. M15 remains responsible for
broad build validation, integration, and repairs uncovered by those checks.
**Internal testing:** not ready for declaration; M15 acceptance still pending.


## Development 270 — Runtime capability segregation

Dispatch consumes `AgentLauncher` and monitoring consumes `AgentInspector`, with legacy `AgentRuntime` adapters bridged via blanket implementations. Optional `AgentMessenger` handles continuation with explicit ambiguous delivery. Preserve existing runtime behavior while allowing new providers to implement only the capabilities they support.

## Development 271 — Worker HTTP transport boundary

**Priority (before):** M24-I3's runtime capabilities and canonical outbox work have
been implemented. M24-I4 worker integration ownership is the next unfinished
architecture slice; no higher-priority M24 source work should precede it. M15
end-to-end acceptance remains separate.

Consolidated authenticated JSON HTTP transport for three optional Python workers
into `deploy/server/worker_transport.py` with one fail-closed server address policy,
common redirect rejection, response bounds, and device header handling. Each worker
retains its own API routes, domain-specific operation semantics, authorization role,
and execution behavior. Created `docs/m24-worker-architecture.md` to distinguish
Herdr machine execution, approved domain-proposal application, and read-only
scheduled reports. Kept the Herdr crash journal separate from the now-canonical
client SQLite outbox because they protect different failure boundaries.

**Priority (after):** Worker protocol ownership is clearer; remaining I4 scope is
obsolete-code and deployment documentation cleanup. No additional worker
framework or public executable should be introduced. Broad acceptance remains M15.

## Development 272 — Runtime worker internals and canonical documentation

**Priority (before):** I1–I3 and the shared worker transport portion of I4 have
been delivered in source. The remaining I4 deliverable is obsolete-code and
architectural-documentation consolidation; it precedes future additions and
is more urgent than general M15 acceptance. No unfinished higher-priority
M24 implementation category is being displaced.

- Extracted Herdr workspace naming and read-only Git evidence construction to
  `deploy/server/agent_worker_evidence.py`. Evidence production has no authority
  HTTP, durable handoff submission, or launch side effects.
- Extracted worker crash journal, exclusive directory lock, and diagnostics to
  `deploy/server/agent_worker_journal.py`. The journal remains separate from
  the canonical client command outbox because it fences irreversible launches.
- Retained the existing `agent-machine-worker.py` entrypoint and imported
  names to avoid breaking deployed callers and pre-existing test imports.
  No additional public CLI or protocol was added.
- Replaced the misleading root README milestone stream with current deployment
  guidance and preserved the prior text under `docs/historical-root-readme-through-271.md`.
- Archived the obsolete relay/Git-era self-hosted decision under
  `docs/historical-self-hosted-decision-2026-10-10.md`; the current
  `self-hosted-architecture.md` now describes the sole supported connected
  topology. Old milestone-specific README files remain historical and are not
  instructions to reinstate a retired executable or sync model.

**Feature-level check:** Python syntax checks and 28 machine worker unit/contract
cases passed after the module extraction. This does not establish live Herdr,
Rust compilation or release integration; those remain M15.

**Provisional M24 implementation progress:** 77% → 83% (+6 percentage points).
The iteration combines a substantial worker-module boundary refactor and the
replacement of conflicting current architecture documentation, rather than
credit for standalone copy editing.

**Priority (after):** M24-I4 has additional obsolete reference and cross-crate
surface review before implementation closure. These should be completed before
any new feature milestone. Internal-testing readiness is **not established**;
M15 still requires a runnable build, core smoke workflows, and no known
blocking data-loss issue.

## Development 273 — facade conversion and authority test-module boundaries

The embedded client DTO conversion/validation functions now live in
`orbit-client/src/facade_conversions.rs`. This removes conversion concerns from
the facade implementation while preserving existing public client methods.
The client regression suites were extracted to `client_facade_tests.rs` and
nine authority store regression modules to `orbit-runtime/src/store_tests.rs`.
The authority's production-only helper `latest_task_field_version` remains in
`store.rs` because accepted command logic relies on it.

M24 provisional implementation progress is 90% (+7 percentage points over 272).
No Rust executable acceptance is claimed; M15 retains compilation, integration
validation, live Herdr testing and fixes discovered during acceptance.


## Development 274 — Final authority-module ownership and reference cleanup

**Priority (before):** M24 had reached 90% source scope. The remaining highest-priority M24 work was eliminating the mixed responsibilities in `orbit-runtime/src/agent_authority.rs` and reconciling current deployment instructions. No higher-priority unfinished M24 category precedes this architectural cleanup. Broad runtime acceptance remains M15.

- Extracted three responsibility-focused implementation modules for machine operations, local agent execution, and device-owned job management. Public request/response types and the `AuthoritativeHost` API remain in the same namespace; no database or HTTP protocol change was intended.
- Kept resume-context construction and transactional start/resume authority operations in the parent module to avoid inventing a second application authority.
- Updated TLS deployment guidance to refer to the current authoritative server rather than the retired relay topology, and removed the outdated claim that machine monitoring is not implemented.
- Added `docs/m24-final-module-ownership.md` as the source-level ownership guide and updated the root README source milestone status.

**Provisional M24 implementation progress:** 90% → 100% (+10 percentage points), based on closing the remaining structural/contract documentation scope. This does not represent successful Rust compilation or live Herdr acceptance.

**Priority (after):** No additional M24 refactoring scope is planned. Internal-testing readiness is not established. M15 retains runnable build, core smoke workflows, live Herdr acceptance and defects exposed by those checks.
