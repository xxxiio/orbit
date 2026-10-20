---
template: project-context
template_version: 3.0.0
template_digest: "2ee21052895810545165cacba665d3d07eabca0ec32de543b2e56041c02c373a"
project_owned: true
record_contract: project-context
record_schema_version: 1
status: current
---

# Project Context: Orbit

## Purpose

Orbit is a local-first Rust engine for projects, tasks, habits, schedules, structured knowledge and opt-in agent-assisted execution, with an optional self-hosted server for connected multi-client use.

## Users and stakeholders

- Individual users managing projects and daily work locally.
- Small trusted teams using an optional self-hosted authoritative server.
- Developers and agents using the CLI, MCP or TUI adapters.
- Operators installing and diagnosing binary releases.
- Maintainers of blueprints and trusted capability providers.

## Architecture summary

A single `orbit` CLI fronts shared application and runtime services. In local mode, `AuthoritativeHost` and SQLite are canonical. In connected mode, the self-hosted server owns authoritative mutations while client SQLite stores accepted snapshots and a durable pending-command outbox. Optional machine-side workers use Herdr for explicitly authorized agent jobs. Project Blueprints provide versioned declarative work-item types, states, guards and templates; trusted capabilities implement optional actions. See `docs/architecture.md` and `docs/roadmap.md`.

## Important boundaries

- Local SQLite is canonical only in local mode; server SQLite is canonical in connected mode.
- Client previews and pending commands are never accepted state.
- The server never runs arbitrary shell commands; Herdr execution is machine-side and explicitly authorized.
- Blueprint manifests cannot grant themselves capability execution rights.
- Python, Herdr and Docker are optional and required only for their specific modes.

## Supported platforms and environments

- Linux x86_64 with Rust 1.99 and Python 3.12 is the currently verified development and acceptance platform.
- macOS and Windows are source-buildable in principle but have no current executable acceptance evidence.
- Connected deployment uses loopback HTTP for tests; TLS ingress is a separate unverified release gate.
- No mandatory cloud service, hosted registry or account is required for local use.

## Canonical commands

- `orbit`
- `orbit project`
- `orbit task`
- `orbit blueprint`
- `orbit agent`
- `orbit sync`
- `./scripts/m15-full-automated.sh`
- `./scripts/install.sh`
- `./scripts/package-release.sh`
- `./scripts/verify-release.sh`

## Non-negotiable constraints

- Preserve one authoritative command/revision/receipt path; do not add transport-owned business rules.
- Do not make Python, Herdr, Docker, Git or a server mandatory for ordinary local use.
- Do not weaken the no-agent-session-to-task-completion rule.
- Do not let blueprint manifests self-authorize capabilities.
- Preserve user databases and credentials; all acceptance tests use disposable directories.
- Do not claim public-release readiness from internal Linux acceptance alone.

## Current priorities

1. Make M25-I3 trusted capability action installation/request/execute reachable and prove it with a deterministic acceptance test.
2. Close M25-I5 parity gaps: connected MCP outbox/receipts, lost-ack replay and blueprint upgrade.
3. Complete M25-I4 habit recurrence extraction.
4. Maintain the M15 Linux acceptance baseline and archive evidence.
5. Defer TLS ingress, remote-machine execution, signing and cross-platform claims until separately verified.

## Known risks

- M25 capability execution is partially implemented but unreachable; publishing a capability-requiring blueprint currently fails closed.
- Connected MCP blueprint writes bypass the canonical outbox and receipts.
- Public release lacks signing, TLS ingress evidence, cross-platform evidence and an installed-version upgrade test.
- The connected server is single-user trust; any enrolled device can call mutation routes.
- Historical scripts under `scripts/legacy-contracts/` are unmaintained and expected to fail.
