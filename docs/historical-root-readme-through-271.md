# Historical root README through Development 271

> Historical development log. Superseded by the current root README and `docs/architecture.md`. References to proposed milestones and legacy commands describe the point in time at which they were written; they are not current guidance.

> Development 267 (M24): see [storage facade modularization](history/development-readmes/README_M24_267.md).
> Development 264 (M24): see [CLI capability refactoring](history/development-readmes/README_M24_264.md).
> Development 263 (M24): see [architecture context consolidation](history/development-readmes/README_M24_263.md).

> Latest M23 source checkpoint: [Development 238](history/development-readmes/README_M23_238.md). Herdr adapter added, runtime acceptance unverified.

> Development 236: See [README_M23_236.md](history/development-readmes/README_M23_236.md) for the authenticated agent-job API checkpoint and validation limits.

# Orbit

Orbit is a local-first knowledge, task, habit, project and scheduling engine
written in Rust. A self-hosted authority server is **optional**.

## Supported deployment modes

**Local-only:** CLI, TUI or MCP host the Rust `AuthoritativeHost` with SQLite
and a physical Markdown vault. No network service is needed.

**Connected:** A self-hosted Orbit server owns canonical SQLite, business
rules, history and accepted command receipts. Native clients maintain local
SQLite replicas containing accepted snapshots and durable pending commands.
A command is never shown as accepted until the server accepts it.

Both modes share the host's business implementation. The TUI, CLI and MCP do
not run a parallel set of domain rules. Desktop/mobile app integration through
Flutter is deferred to the separate Atomic App project.

## Configuration

Local-only mode (default):

```sh
export ORBIT_DB=orbit.db
# run a local CLI/TUI/MCP command
```

Connected terminal UI or MCP mode:

```sh
export ORBIT_DB=local-only.db             # used only in local mode
export ORBIT_AUTHORITY_DB=replica.db       # separate SQLite cache/outbox
export ORBIT_AUTHORITY_SERVER=http://localhost:8787
# ORBIT_AUTHORITY_URL is an equivalent endpoint alias
```

Set both connected variables. Partial deployment configuration fails closed;
there is no silent local-authority fallback. CLI connected workflows use its
explicit `sync replica` subcommands instead of local domain commands.

## Connected synchronization

Connected replicas maintain accepted snapshots independently from a durable
command outbox. `orbit sync replica ... converge` replays pending commands,
refreshes accepted state and Markdown, then observes whether the local
revision and history fingerprint match the live server. `convergence` works
offline; `observe-convergence` requires server access. A conflicted command
may be reviewed online and replaced with a revision-bound reviewed action;
a replacement is pending until the server accepts it. See
`docs/m21-development-208.md` for commands, invariants and limitations.

## Project status

- M18: authoritative host consolidation — source complete; acceptance deferred.
- M19: legacy relay/Git retirement — source complete; acceptance deferred.
- **M20: unified authoritative runtime — implementation complete at Development 204.**
- **M15: release and executable acceptance pending.** A successful build, core
  smoke workflows and no known blocking data-loss issues are needed before
  declaring Orbit ready for internal testing.

See [M20 implementation closure](docs/m20-implementation-closure.md),
[architecture](docs/self-hosted-architecture.md),
[public runtime contracts](docs/unified-runtime-roadmap.md) and
[historical development notes](docs/historical-development-readme-through-203.md).

This status is a source-implementation claim, **not** a statement that the
current Rust workspace has been compiled or release-tested.

## Current implementation work

- **M21: offline replication and connected-client synchronization — source
  implementation complete at Development 212.** See `README_M21_212.md`.
  This includes conflict-safe convergence, incremental verified Markdown
  replication, and shared cross-domain sync state. Executable acceptance is
  explicitly deferred to M15.


### Accepted Today/Next brief (M22)

`orbit task work-brief --now-ms 1800000000000 --ends-at-ms 1800028800000` provides a read-only schedule-aware action list with dependency blockers, project/milestone grouping, and explanations. It never creates calendar reservations. See `docs/m22-development-216.md`.


`orbit task completion-impact <task-id> --now-ms 1800000000000` previews the immediate dependency impact of completing a task without changing authoritative state. Both planning reports are also available to appropriately authorized MCP agents.


### Development 219: agent-facing task dependency planning

MCP clients can discover and invoke `task.dependencies.add`, `task.dependencies.remove`,
`task.dependencies.preview`, `task.graph`, and `task.next` in both local and
connected modes. The read-only dependency preview is also exposed as
`orbit task preview-dependency TASK PREREQUISITE --now-ms EPOCH_MS [--remove]`.
Dependency mutations are revision-fenced and check permission for both tasks.
See `docs/m22-development-219.md` for scope, security, and verification status.

Development 224: explicit reviewed task-plan approval boundary; see `docs/m22-development-224.md`.


### Development 225 — scenario-to-calendar approval

The M22 scenario planner can now convert a compared portfolio alternative to
concrete accepted free time slots, review exact reservations and execute only
through a separate explicit approval. See
[`docs/m22-development-225.md`](docs/m22-development-225.md) for CLI, TUI,
MCP, HTTP and native Rust interfaces, authority fencing and known verification
limitations. These source-level changes have not yet undergone Cargo or
executable acceptance testing.

M22 Development 229 closes the remaining terminal action-preview handoffs:
Today dependency-removal actions open the accepted-state dependency simulator,
and calendar-preview actions show a selected-task slot or exclusion without
creating reservations. Details: `docs/m22-development-229.md`. Runtime
compilation and executable integration acceptance remain in M15.

### Proposed next feature milestone: M23 Agent-aware project execution

After M22 source-feature closure, the proposed M23 integrates an optional Herdr
agent runtime with canonical Orbit jobs, monitored execution, explicit review,
structured handoffs and deterministic cross-project resume. This is a **roadmap
proposal only**, not implemented functionality. M15 executable/release acceptance
remains separately outstanding and must not be displaced.
See [`docs/m23-agent-jobs-roadmap.md`](docs/m23-agent-jobs-roadmap.md).


M23 Development 233 checkpoint: see [agent authority and runtime-port notes](docs/m23-development-233.md).


M23 Development 234: see `docs/m23-development-234.md` for the dependency-aware launch gate and outstanding acceptance.

- [Development 237 M23 job ownership boundary](history/development-readmes/README_M23_237.md)

### M23 Development 248 — machine execution authority

The authenticated server now exposes an assigned-machine agent inbox, immutable job brief, atomic launch claim, launch-result submission, and runtime observation routes. See `docs/m23-development-248.md`. No second public CLI is introduced, and the server never executes Herdr itself. These endpoints are source-implemented but await Rust build and live integration acceptance.

### M23 agent-facing MCP (Development 257)

The `orbit-mcp` adapter supports opt-in `agent.job_context` and
`agent.handoff_submit` for a specific `ORBIT_MCP_JOB_ID`. These reuse
`AuthoritativeHost` and preserve immutable job handoffs. See
`docs/m23-development-257.md` for permissions, arguments, and limitations.


M23 offline agent command source: see [Development 260](history/development-readmes/README_M23_260.md).
