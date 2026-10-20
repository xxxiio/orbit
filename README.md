# Orbit

Orbit is a local-first Rust engine for projects, tasks, habits, schedules,
structured knowledge, and opt-in agent-assisted execution. A self-hosted
server is optional; Herdr and AI providers are not required for ordinary use.

## Install and learn

- [Install the local CLI or optional binaries](docs/getting-started/installation.md)
- [Build and verify offline binary distributions](docs/releasing/binary-distribution.md)
- [Platform requirements and optional components](docs/releasing/platforms-and-components.md)
- [Release candidate checklist](docs/releasing/release-checklist.md)
- [First project and blueprint](docs/getting-started/first-project.md)
- [Agent execution and handoffs](docs/guides/agent-workflow.md)
- [End-to-end agent workflow tutorial](docs/guides/agent-workflow-tutorial.md)
- [Offline behavior](docs/guides/offline-operation.md)
- [Self-hosted operation](docs/guides/self-hosting.md)
- [CLI command groups](docs/reference/cli.md)
- [Create custom project workflows](docs/guides/custom-blueprints.md)
- [Optional worker deployment](docs/guides/worker-deployment.md)
- [Troubleshooting](docs/guides/troubleshooting.md)

The [documentation index](docs/README.md) organizes the remaining task-oriented
guides. Optional trusted worker: `./scripts/install-worker.sh` (separate; Python
required only there). Local installation: `./scripts/install.sh` (requires
Cargo), optional server setup: `./scripts/setup-server.sh --init`; uninstall:
`./scripts/uninstall.sh`. The local CLI does not require Python or Herdr.

## Current architecture

| Mode | Canonical authority | Offline behavior |
| --- | --- | --- |
| Local-only | Local `AuthoritativeHost` + SQLite | Operations are immediately authoritative locally; no server required. |
| Connected | Self-hosted server `AuthoritativeHost` + server SQLite | Client SQLite contains accepted snapshots and a durable pending-command outbox. Commands are only accepted after server acknowledgment. |

The server never runs arbitrary agent shell commands. Enrolled machine-side
workers use the optional Herdr adapter to execute authorized jobs; Orbit keeps
job intentions, session observations, handoffs, and reviews. Agent job/session
completion **does not** automatically complete an Orbit task.

Use **`orbit`** as the public CLI (`orbit agent ...` for agent commands). The
machine worker scripts under `deploy/server/` are internal operational tools,
not alternative public command-line interfaces. The Flutter application is
outside this Rust repository and is not a release-acceptance prerequisite.

## Configuration

Local-only mode:

```sh
export ORBIT_DB=orbit.db
```

Connected mode (requires both settings; incomplete configuration fails closed):

```sh
export ORBIT_AUTHORITY_DB=replica.db
export ORBIT_AUTHORITY_SERVER=http://localhost:8787
```

`ORBIT_AUTHORITY_URL` is an alternate endpoint variable. Production endpoints
must use TLS; loopback cleartext is permitted for development. Pending agent
intentions join the canonical SQLite outbox and retain global command order;
external machine-worker launch journals are separate, because they fence
irreversible process side effects.

## Source layout and documentation

| Location | Responsibility |
| --- | --- |
| `crates/orbit-domain` | Domain identities and invariants |
| `crates/orbit-application` | Application use cases, ports, authority rules |
| `crates/orbit-storage` | SQLite repositories, schemas and migrations |
| `crates/orbit-runtime` | Composition and optional agent runtime adapters |
| `crates/orbit-client`, `crates/orbit-sync` | Connected replicas, pending commands and sync |
| `crates/orbit-server` | Authenticated server transport |
| `crates/orbit-cli`, `crates/orbit-tui`, `crates/orbit-mcp` | User-facing adapters |
| `deploy/server` | Optional machine, proposal, and automation workers |

- [Current roadmap, verified milestone status and readiness](docs/roadmap.md)
- [Current architecture and authority boundaries](docs/architecture.md)
- [Current server deployment contracts](docs/server-api.md)
- [M24 consolidation roadmap and iteration ledger](docs/m24-architecture-consolidation.md)
- [M24 worker ownership](docs/m24-worker-architecture.md)
- [M24 authority module ownership](docs/m24-final-module-ownership.md)
- [Historical development notes](docs/history/development-readmes/README.md)

## Milestone status (verified 2026-10-20)

| Milestone | Verified current status |
| --- | --- |
| M22 — Planning | Source complete and compiled; executable coverage partial (planning metadata/schedules, contract tests) |
| M23 — Agent integration | Source complete; local opencode + Herdr 0.9.5 path live-verified; remote machine-worker unverified |
| M24 — Framework consolidation | Source complete; builds, 1,235 tests, fmt/clippy clean |
| M25 — Project Blueprints | **Partial**: I1/I2 complete; capability actions, MCP parity, upgrade path and domain extraction remain |
| M26 — Packaging, installation and documentation | Executable-verified for unsigned Linux x86_64 archives; signing/cross-platform remain |
| M15 — Executable/release acceptance | Linux local/loopback/local-Herdr scope verified: 37/37 full profile (38/38 with live Herdr), 1,235 Rust tests, real archive install smoke |

**Source implementation is not runtime acceptance.** The current M15 profile
passes 37/37 checks (38/38 with opt-in live Herdr) with 0 failed and 0 not-run; `cargo test --workspace --locked`
reports 1,235 passing tests. A live Herdr launch/monitor/evidence/handoff/review
cycle passed in a disposable workspace, and a real release archive was built,
verified, installed into a disposable prefix, exercised, and uninstalled. These
results establish readiness for **Linux local internal testing**, and for
loopback-connected and local-agent internal testing with the qualifications in
`docs/roadmap.md`.

Not verified: TLS/Caddy ingress and browser deployment (Docker is unavailable in
this environment), remote-machine agent execution, macOS/Windows/cross-compiled
targets, distribution signing, and real installed-version database upgrades. The
M25 capability-action and upgrade gaps also remain. These are separate release
gates, not blockers for local internal testing.

## Testing and tracking

Maintained tests live under `tests/contracts/` (source-boundary checks),
`tests/acceptance/` (compiled CLI workflows), `tests/packaging/` (installers and
release archives), `tests/documentation/`, and `deploy/server/test_*.py` (optional
workers). Run `./scripts/m15-full-automated.sh --profile full` for the canonical
gate. Historical M18–M22 source-shape scripts are preserved under
`scripts/legacy-contracts/`; they are unmaintained and excluded from the gate.

GetDone records under `.agent/` own milestone status, acceptance evidence, and
the next deterministic step.
