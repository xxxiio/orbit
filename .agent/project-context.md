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

- Linux x86_64 is the only runtime-verified target: local, loopback connected, local-Herdr and the M27 installer/init/update fixture workflows.
- Declared release targets are `x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu`, `x86_64-apple-darwin` and `aarch64-apple-darwin`; macOS is not runtime-verified and Windows is unsupported.
- Installing a prebuilt CLI requires no Rust, Python, Herdr, Docker or server. Rust is required only for source builds; Python only for optional workers/tests.
- Connected deployment uses loopback HTTP for tests; TLS ingress is a separate unverified release gate.
- No mandatory cloud service, hosted registry or account is required for local use. A real GitHub Release has not been published; release publication is draft/manual and externally authorized.

## Canonical commands

- `orbit`
- `orbit init`
- `orbit doctor`
- `orbit update`
- `orbit project`
- `orbit task`
- `orbit blueprint`
- `orbit agent`
- `orbit sync`
- `./scripts/m15-full-automated.sh --profile full`
- `./install.sh` (GitHub-hosted installer)
- `./scripts/install.sh`
- `./scripts/package-release.sh`
- `./scripts/verify-release.sh`
- `python3 -m unittest discover -s tests/documentation -v`

## Non-negotiable constraints

- Preserve one authoritative command/revision/receipt path; do not add transport-owned business rules.
- Do not make Python, Herdr, Docker, Git or a server mandatory for ordinary local use.
- Do not weaken the no-agent-session-to-task-completion rule.
- Do not let blueprint manifests self-authorize capabilities.
- Preserve user databases and credentials; all acceptance tests use disposable directories.
- Do not claim public-release readiness from internal Linux acceptance alone.

## Current priorities

1. Obtain explicit maintainer authorization and write access to run the M27 release workflow and verify the real GitHub installer/update path.
2. Maintain the 42/42 M15 Linux acceptance baseline and archive evidence after further changes.
3. Defer M25 EC-003 through EC-006 until distribution/first-run work is accepted.
4. Defer TLS ingress, remote-machine execution, signing, macOS runtime and installed-version upgrade claims until separately verified.

## Known risks

- No real GitHub Release exists; fixture acceptance does not prove the public distribution channel or native arm/macOS runners.
- macOS installer/update behavior is statically portable but not executed on macOS or bash 3.2.
- M25 capability actions remain unreachable; publishing a capability-requiring blueprint fails closed.
- Connected MCP blueprint writes bypass the canonical outbox and receipts.
- Public release lacks signing, TLS ingress evidence, cross-platform evidence and a real installed-version upgrade test.
- The connected server is single-user trust; any enrolled device can call mutation routes.
- Historical scripts under `scripts/legacy-contracts/` are unmaintained and expected to fail.
