> M20 Development 191: Connected TUI authoritative workflow palette — see [README_M20_191.md](history/development-readmes/README_M20_191.md).

> Latest development: [Orbit 181 — Project bootstrap workflow](history/development-readmes/README_M20_181.md).

> **Latest: Orbit Development 170 — M19 application API retirement.** Removed
> unused SQL-backed knowledge application commands, repository ports, retrieval
> abstractions, CLI DTOs and import types. Embedded client and CLI continue to
> use Markdown vault operations. M19 implementation handoff; full Rust build
> and executable acceptance are deferred to M15. See
> [docs/m19-development-170.md](docs/m19-development-170.md).

> Orbit 151 M18 integrated runtime query consolidation. See [docs/m18-development-151.md](docs/m18-development-151.md) for verified changes and limitations.

> Latest: [Orbit Development 138](docs/m18-development-138.md) — durable offline Markdown command outbox with accepted-versus-pending semantics. Legacy SQL knowledge remains; M18 estimate unchanged.

> Latest: [Orbit Development 136](docs/m18-development-136.md) — host-side durable Markdown command intents, receipt acceptance and crash replay; legacy SQL write path still active. M18 remains 89%.

> Latest: [Orbit Development 135](docs/m18-development-135.md) — cohesive file-backed create/edit/move/index staging workflow; **not** accepted canonical command cutover. M18 remains 89%.

> Latest: [Orbit Development 130](docs/m18-development-130.md) — knowledge recovery journal collision protection. M18 89%; full Markdown cutover incomplete.

> Latest iteration: [Orbit Development 129 — Markdown vault organization operations](docs/m18-development-129.md). These are filesystem primitives, not a completed canonical cutover.

> Development 126: see [docs/m18-development-126.md](docs/m18-development-126.md) for conflict-aware Markdown replacement and current limits.

# Orbit milestone implementation snapshot — 112

**Revised architectural contract:** one host-side runtime and authoritative
SQLite implementation in embedded and remote deployment; local/remote CLI and
TUI; local stdio MCP and remote HTTP MCP. See `unified-runtime-roadmap.md`.

Implementation estimates are provisional, not verified acceptance. The expanded
M18 scope is rebaselined to 75%; prior 99% referred to the older two-implementation
architecture and is not valid for the newly agreed design. M15 acceptance remains
unverified. Flutter is deferred to Atomic App.

| Milestone | Estimate |
|---|---:|
| M1 Domain foundation | 94% |
| M2 Local persistence | 96% |
| M3 CLI core | 92% |
| M4 Planning & scheduling | 94% |
| M5 Progress & forecasting | 94% |
| M6 Habits | 94% |
| M7 Retired Git sync (historical) | 92% |
| M8 Knowledge foundation | 94% |
| M9 Knowledge retrieval | 97% |
| M10 Embedded Rust client | 94% |
| M11 Local AI/BYOK | 94% |
| M12 MCP/agents | 95% |
| M13 Terminal UI | 94% |
| M14 Project Modes/AI Agent | 97% |
| M15 Release readiness | 8% |
| M16 Self-hosted server | 95% |
| M17 Remote agents/web | 98% |
| M18 Unified host runtime/refactor | 79% |

M18's next highest priority is U2: replace separate authoritative server
business implementations with shared runtime/application services. This is
more important than additional security hardening, release packaging or
new features at this stage. No internal-testing-ready claim until a runnable
build, core smoke acceptance, and no blocking data-loss issue.


Development 114: the server creates `orbit_runtime::AuthoritativeHost`, a host-side SQLite ownership and canonical command boundary shared by REST/MCP adapters. Embedded and remote SQLite schemas are **not yet unified**; see `docs/unified-runtime-roadmap.md`.

### Development 115: embedded authoritative host adapter

`orbit-client::EmbeddedAuthorityClient` now sends typed in-process commands to
`orbit-runtime::AuthoritativeHost`; it uses the same canonical persistence,
receipts and semantic history as the remote HTTP/MCP adapter. This is the local
canonical integration path, not an alternate business-logic implementation.
The older feature-rich `OrbitClient` still uses a separate SQLite repository;
full persistence consolidation is **not yet complete**. See
`docs/unified-runtime-roadmap.md`.

### Development 116: host ownership boundary

The server now accesses device enrollment, authorization, token rotation,
revocation, backup and canonical domain commands via explicit `AuthoritativeHost`
methods. Direct dereferencing into its SQLite store is no longer supported.
Local/server persistence consolidation remains the main unfinished U2 task.

### Development 117: local and remote canonical client connections

`orbit_client::unified::AuthorityConnection` is a Rust-level entry point for
canonical project/task/habit/metric commands. An embedded connection delegates
in-process to a host-owned `AuthoritativeHost` with immediate authoritative
receipts; a remote connection queues the same command intents in client SQLite
until an enrolled device submits them to server HTTP, then refreshes accepted
snapshots. This is the future Atomic App integration contract, **not** Flutter
UI integration. Remote native CLI requires `sync replica --device <id>` or
`ORBIT_AUTHORITY_DEVICE`; connected TUI uses `ORBIT_AUTHORITY_DEVICE` along with
`ORBIT_AUTHORITY_TOKEN`. Cached offline reads do not require credentials.

The separate feature-rich local `SqliteStore` still needs to be consolidated
with canonical host persistence before the unified runtime refactor is finished.

### Development 119
Canonical knowledge item edits now use `AuthoritativeHost::knowledge_update` with
revision checks, immutable receipts and semantic field-version history. See
`docs/m18-development-119.md` for limits and validation.

### Development 120
The self-hosted server now exposes authenticated canonical knowledge read/create/update HTTP routes delegating to `AuthoritativeHost`, not separate persistence logic. See `docs/m18-development-120.md` for limitations.


Development 121: see [M18 report](docs/m18-development-121.md).


## Development 122
Canonical knowledge title/body search is available through the authoritative host, embedded client, unified client and authenticated HTTP POST (`/v1/authoritative/knowledge-search`). See `docs/m18-development-122.md` for limitations.

## Development 131

See `docs/m18-development-131.md` for the filesystem-backed Markdown read integration and the remaining canonical knowledge write cutover.


M18 current blocking status and Orbit Development 140 change: [docs/m18-development-140.md](docs/m18-development-140.md).


## Development 150

Shared host-owned scheduling CLI (create/list/reschedule); see docs/m18-development-150.md.

### Development 155 — M18 rich command acceptance

The authority host now supports nested application transactions and typed
accepted project, task, habit, and routine commands with durable revision,
receipt, event, and activity coordination. See
[`docs/m18-development-155.md`](docs/m18-development-155.md) for the changes,
checks, and remaining acceptance requirements.


## Development iteration 156

See [`docs/m18-development-156.md`](docs/m18-development-156.md) for the accepted rich command cutover across CLI, MCP and embedded clients, tests, known M18 gaps and executable acceptance status.

### Markdown-native knowledge commands (M19)

The `note` and `folder` CLI subcommands operate on physical Markdown files and
folders via the authoritative runtime. For example:

```sh
orbit --db ./orbit.db folder create Work
orbit --db ./orbit.db note create Work/plan.md --id plan --body '# Plan'
orbit --db ./orbit.db note read plan
orbit --db ./orbit.db note search Plan
orbit --db ./orbit.db folder tree
```

The former `knowledge` CLI remains transitional SQL-backed functionality and
is not a substitute for the new file-authoritative note API. See
`docs/m19-development-160.md` for durable IDs, idempotent retries, and updates.

### Development 168 (M19 SQL knowledge cutover)

Fresh SQLite databases do not create retired knowledge body/resource/FTS tables. Legacy
SQL-knowledge databases are rejected without deletion; use a fresh workspace and
explicit Markdown import. TUI term navigation reads physical Markdown. This is a
breaking greenfield change; Rust compilation and full acceptance are pending M15.
See `docs/m19-development-168.md`.

### Orbit Development 172 checkpoint

M20 remote domain command retries are now locally idempotent across restarts;
remote clients can inspect durable command status/receipts and do not report
an already-acknowledged retry as pending. See `docs/m20-development-172.md`.


M20 development 184: see [planning-session report](history/development-readmes/README_M20_184.md).

## Development 186

Authoritative scheduling lifecycle and connected-mode command transport are documented in [README_M20_186.md](history/development-readmes/README_M20_186.md). Rust compilation and executable acceptance are still required.

## M20 Development 187 — Routine and milestone authority

See [README_M20_187.md](history/development-readmes/README_M20_187.md) for the canonical routine/milestone
command lifecycle, embedded/remote API integration, test cases, and remaining
compiled acceptance requirements. Rust build and integration verification remain
outstanding; the development archive is a source implementation, not an
acceptance-certified release.

## M20 connected domain CLI (Development 190)

See [README_M20_190.md](history/development-readmes/README_M20_190.md) for connected CLI commands across
schedule, routine, milestone and strategy families, receipt inspection,
accepted-only views and validation limitations. Rust binary verification remains
an open milestone gate.

### M20 development 192 — authoritative server MCP tools

The server MCP bridge now exposes accepted schedules, routines, milestones,
and project strategies, and dispatches their commands through the same
canonical AuthoritativeHost operations used by REST. Details and current
limitations are documented in [README_M20_192.md](history/development-readmes/README_M20_192.md).


M20 desktop MCP connected authority implementation: see [Development 193](history/development-readmes/README_M20_193.md).


### Development 194
See [README_M20_194.md](history/development-readmes/README_M20_194.md) for connected TUI and MCP core-domain integration and authorization boundaries.

M20 Development 195: cross-domain authoritative MCP workflows and explicit conflict recovery — see [README_M20_195.md](history/development-readmes/README_M20_195.md).

- [M20 Development 197: Connected TUI physical-vault command workflows](history/development-readmes/README_M20_197.md)
