# Orbit Core architecture

Orbit Core is a headless Rust platform. Presentation clients are peers, not owners of business logic.

```text
               orbit-domain
                    ↑
             orbit-application
          ↑      ↑      ↑      ↑
      storage  planner progress sync/ai
          ↑      ↑      ↑      ↑
          ├──────────────┬──────────────┐
          │              │              │
        CLI/TUI         MCP        Flutter client
                                      (separate repo)
```


## Supported authoritative deployment modes (current)

Orbit supports two authoritative modes. Neither mode needs OpenAI services,
Herdr, or a first-party cloud backend to manage ordinary Orbit entities.

### Local-only authoritative mode

```text
Orbit CLI / TUI / embedded client / MCP
                   |
              Application host
                   |
            canonical local SQLite
```

All ordinary local commands are accepted against local SQLite. Optional
Herdr sessions are external execution processes; Orbit only owns durable job
intentions, observations, handoffs, and reviews.

### Optional self-hosted server-authoritative mode

```text
CLI / desktop / mobile                  Browser
       |                                   |
 offline SQLite cache                     |
 pending commands                         |
       +--------------+--------------------+
                      |
             authenticated server API
                      |
              AuthoritativeHost
                      |
              canonical server SQLite
                      |
         machine-assigned agent intentions
                      |
             trusted machine worker
                      |
                optional Herdr
```

The server is the **only** authority in connected mode. Client SQLite is an
offline-capable replica/cache, not a second canonical writer. A disconnected
edit is pending until the server accepts it; an offline agent launch intention
must never invoke an executor until acceptance. The server does not execute
shell commands. The trusted machine worker uses device-scoped authorization
and the durable launch claim before invoking Herdr.

The modes are separate composition roots; switching modes is not peer-to-peer
synchronization or Git event replication. No legacy relay compatibility or
migration/import pathway is required by this greenfield architecture.

### Stable dependency and service boundaries

- `orbit-domain`: entities, typed identities and domain invariants.
- `orbit-application`: use cases and infrastructure ports; no SQL, HTTP,
  subprocess or Herdr dependency.
- `orbit-storage`: SQLite implementations of authority repository ports.
- `orbit-runtime`: application composition and dispatch coordination.
- `orbit-client` / `orbit-sync`: connected protocol, replicas and pending state.
- `orbit-server`: authentication, serialization and server transport; delegates
  canonical decisions to the host.
- `orbit-cli`, `orbit-tui`, `orbit-mcp`: public adapters, not duplicated authority.
- Machine worker / Herdr adapter: optional side-effect infrastructure.

The agent launch port explicitly separates the durable authority claim and
captured task context from runtime dispatch. The SQLite adapter implements
`AgentLaunchAuthority`, while `dispatch_prepared_job` accepts that port and
`AgentRuntime`; neither CLI nor the transport should implement job lifecycle
rules. An uncertain runtime response is reconciled, never blindly relaunched.

## Canonical documentation and historical records

`README.md` is the entrypoint for current product status and `architecture.md`
is the normative architecture contract. `self-hosted-architecture.md` summarizes
connected mode. Individual `README_M*` files and `m*-development-*` documents
are historical milestone artifacts, **not** alternate architecture specifications.
The superseded October 10 relay design is preserved only under
`historical-self-hosted-decision-2026-10-10.md`.

## Structured knowledge architecture

Orbit treats execution and structured knowledge as equal first-class subsystems. Knowledge uses preset semantic types and typed relationships rather than an arbitrary Notion-style schema builder. Core knowledge contracts are expected to include notes, decisions, findings, assumptions, risks, references, concepts/resources, provenance, and links to execution entities. Longer bodies may use Markdown/text while identity, type, relationships, lifecycle and provenance remain structured.

Retrieval is local-first: deterministic full-text/typed search and backlinks must work without AI. Optional semantic search can augment this later.

## Data and external-service boundary

Canonical user content stays local by default and normal Orbit operation does not require a first-party content backend. User-selected Git remotes and AI providers are external boundaries. When a cloud AI provider is configured, only context allowed by Orbit's user/project policy should be sent; that submitted context is then subject to the selected provider's privacy/retention terms. Orbit itself does not need to centrally collect the content to orchestrate the request.

Any future first-party hosted sync must remain optional and should be content-blind/end-to-end encrypted, without requiring server access to plaintext content or user-held decryption keys.

## Capability split

Mobile-capable core: domain, application, storage, planner, progress, sync protocol, AI client adapters.

Desktop/headless only: CLI, TUI, Git-specific desktop integration as needed, MCP server.

## Backup and local recovery

Orbit distinguishes recovery artifacts from synchronization artifacts.

- `SqliteStore::export_backup` creates a validated point-in-time SQLite snapshot for user-owned backup and portability.
- Backups are written to a sibling temporary file, fsynced, then renamed into place.
- `SqliteStore::restore_backup` never mutates the source backup and never overwrites an existing destination.
- Restore copies into a temporary sibling, applies supported migrations to that copy, runs integrity checks, fsyncs it, and only then publishes it with a rename.
- Newer unsupported schema versions fail closed and leave the requested destination absent.
- A live SQLite database or backup snapshot is never a sync merge unit. Sync operates on semantic Orbit changes/events.

Replacing an existing live workspace is intentionally not part of the primitive restore API. A future higher-level recovery workflow must explicitly close the live store, preserve or move the existing database, and then invoke restore. This prevents accidental destructive recovery.

## Local SQLite concurrency policy

Orbit may have multiple local clients attached to the same workspace on desktop/headless systems (for example CLI, TUI, Flutter desktop, and MCP). The SQLite adapter therefore applies one consistent connection policy whenever a file-backed workspace is opened:

- `journal_mode=WAL` for concurrent readers while one writer commits.
- `foreign_keys=ON` on every connection; this is a connection property, not a one-time migration setting.
- a bounded 5-second SQLite busy timeout so transient writer contention waits briefly instead of failing immediately or spinning indefinitely.
- short write transactions. Application services must not keep transactions open while doing network, AI, filesystem traversal, user interaction, or other unbounded work.
- one SQLite write transaction is the serialization boundary. Orbit does not assume that only one process exists; competing writers are expected to serialize through SQLite and may receive a repository error after the bounded timeout.
- long-lived read views should be avoided when a fresh query is cheap, because stale readers can delay WAL checkpointing.

The application layer owns semantic commands and units of work. CLI, TUI, Flutter, and MCP must not bypass those services and issue ad-hoc SQL. This keeps cross-entity validation and activity journaling consistent regardless of which local client initiated the mutation.

Mobile clients may use the same repository semantics, but desktop-only MCP functionality is not compiled into mobile clients.


## Deterministic plan proposal boundary

`orbit-planner` proposes work without mutating canonical state. Initial proposals never split one task across multiple schedule slots. Candidate order is deterministic: deadline, workflow urgency, earliest-start, then stable task id. Every proposal must pass `validate_proposal` before future application; this validator is intentionally independent so AI or external planners cannot bypass capacity, schedule overlap, estimate, availability, or deadline constraints.

## Planning horizons, recurring availability, and apply boundary

Orbit separates calendar recurrence from task/habit recurrence. In M4, recurrence means recurring **availability windows** used by deterministic day/week planning. Habit occurrence/history remains a first-class M6 concern and is not synthesized into recurring tasks.

- `PlanningWeek` contains seven caller-resolved local day boundaries (Monday through Sunday). The pure Rust planner does not perform timezone or DST conversion; platform adapters resolve those boundaries explicitly, so 23/25-hour local days remain representable.
- `WeeklyAvailabilityRule` expands minute offsets against those explicit day boundaries and fails closed if a requested local-time window cannot fit inside the supplied day.
- `propose_day` and `propose_week` are adapters over the same deterministic proposal engine. Explicit availability constrains where work may be placed even when the rest of the calendar is empty.
- `FixedBlockBufferPolicy` reserves preparation/decompression time around fixed commitments without creating fake schedule records and without forcing gaps between ordinary movable task blocks.
- `ValidatedPlanProposal` is still not authority to mutate state indefinitely. `apply_validated_plan` re-reads canonical tasks and schedule blocks and re-validates inside the unit-of-work transaction before creating any schedule blocks.
- Plan application assigns caller-provided schedule/activity identities, writes movable task schedule blocks, and appends semantic `Scheduled` activity records atomically. If the canonical schedule changed after preview, the stale proposal is rejected and nothing is written.

This same boundary will be used later by AI planning: AI may propose, but it cannot bypass deterministic validation or directly mutate the canonical schedule.

### Habit routines and planning boundary

Routines are ordered views over habit IDs, not a new execution hierarchy. They do not own habit lifecycle and do not synthesize tasks. Habit planner proposals remain distinct from task proposals and materialize only as movable `ScheduleTarget::Habit` schedule blocks after independent validation and transactional revalidation.

## Optional self-hosted deployment

The optional self-hosted server is an implemented connected-mode authority, not a hypothetical future adapter. See [self-hosted architecture](self-hosted-architecture.md) for deployment details. Historical milestone documents are not normative architecture specifications.
