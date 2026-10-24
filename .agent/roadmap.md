---
template: roadmap
template_version: 2.0.0
template_digest: "938b804e31855ad2d8e66c8a04f659b8750c6c3dbe1480242264f7cc5c1732ba"
project_owned: true
record_contract: roadmap
record_schema_version: 1
status: current
current_milestone: null
---

# Project Roadmap

## Product outcome

A local-first, optionally connected Orbit user can manage projects, tasks, plans, schedules and knowledge; run explicitly authorized agent jobs; and configure project variations through versioned blueprints without a mandatory server, Python, Herdr or cloud dependency.

## Ordering principles

1. Runtime correctness and authority boundaries precede feature breadth.
2. Completed source implementation must be distinguished from executable acceptance.
3. Local internal testing is not blocked on optional public distribution infrastructure.
4. Blueprint capability execution is security-sensitive and must be reachable only through explicit trusted grants.
5. Public release requires signing, cross-platform and upgrade evidence separate from internal-testing readiness.

## Milestones

### MILESTONE-0022 — M22 Planning

- **Status:** completed
- **Outcome:** Users and agents can plan tasks, inspect dependencies and readiness, produce daily/portfolio plans and handoffs, and review approved plans through one authoritative application model.
- **Why now:** Planning is the shared language between human project control and agent execution.
- **Depends on:** none

#### Scope

- Planner crate, dependency/readiness analysis and task graph operations.
- Daily and portfolio planning, capacity, scenarios and approved-plan application.
- Planning briefs, focus, handoffs, navigation and review surfaces.
- CLI, TUI, MCP and server planning adapters.

#### Non-goals

- Autonomous multi-agent orchestration.
- A visual planning UI outside the Rust repository.

#### Exit criteria

- [x] EC-001: Planning and portfolio modules compile in the workspace.
- [x] EC-002: Planner and planning contract tests pass.
- [x] EC-003: Local CLI planning metadata and schedule workflows pass with restart persistence.
- [x] EC-004: Planning brief and approval rules are enforced by the shared runtime rather than a transport.

#### Evidence

- EC-001: `cargo check --workspace --all-targets --locked` passed during the M15 gate.
- EC-002: `cargo test --workspace --locked` passed; planning contracts are under `tests/contracts/`.
- EC-003: `deploy/server/smoke-local-authority.py` passed in the 37-check M15 profile.
- EC-004: `crates/orbit-runtime/src/planning_*` and `task_commands.rs` contracts pass.

#### Next milestone

MILESTONE-0023

### MILESTONE-0023 — M23 Agent-Aware Execution

- **Status:** completed
- **Outcome:** Orbit can authorize, launch, observe, hand off and review an optional coding-agent job without letting agent completion silently complete an Orbit task.
- **Why now:** Agent assistance must remain explicit, auditable and fenced before wider use.
- **Depends on:** MILESTONE-0022

#### Scope

- Agent job domain and storage.
- Authority-owned launch/reservation, monitoring, evidence, handoff and review.
- Optional Herdr adapter and local `opencode` path.
- Machine-worker Python contracts and transport.
- Connected agent outbox and server routes.

#### Non-goals

- Autonomous multi-agent orchestration.
- Requiring Herdr, Python or an AI provider for ordinary Orbit use.
- Remote-machine execution as a local-testing prerequisite.

#### Exit criteria

- [x] EC-001: Agent job and authority modules compile and pass workspace tests.
- [x] EC-002: Deterministic fake-runtime contract proves single launch, fencing, handoff, review and no task auto-completion.
- [x] EC-003: Live local Herdr 0.9.5 launches `opencode`, monitors the session, captures read-only Git evidence, records a handoff and review, and leaves the Orbit task open.
- [x] EC-004: Python worker contract tests pass.

#### Evidence

- EC-001: `cargo test --workspace --locked` passed.
- EC-002: `crates/orbit-runtime/tests/agent_execution_contract.rs` passed.
- EC-003: `.agent/evidence/herdr-live.log`.
- EC-004: `.agent/evidence/m15-final.zip` contains the worker suite result.

#### Next milestone

MILESTONE-0024

### MILESTONE-0024 — M24 Architecture Consolidation

- **Status:** completed
- **Outcome:** Orbit has one authoritative host and a clear module ownership model across application, storage, runtime, client, server and worker boundaries.
- **Why now:** Later planning, agent and blueprint work depends on removing duplicate authority implementations.
- **Depends on:** none

#### Scope

- One `AuthoritativeHost` ownership model and shared application services.
- Split agent authority modules and worker transport.
- Client outbox versus external machine journal separation.
- Retirement of the legacy append-only relay architecture.

#### Non-goals

- New user-facing features.
- Public plugin isolation or a marketplace.

#### Exit criteria

- [x] EC-001: The workspace compiles with one authoritative host path.
- [x] EC-002: No transport-owned domain mutation replaces the shared application path.
- [x] EC-003: Full Rust tests, fmt and Clippy pass.
- [x] EC-004: Historical source-shape scripts are outside the active gate and documented as unmaintained.

#### Evidence

- EC-001: `cargo check --workspace --all-targets --locked` passed.
- EC-002: `tests/contracts/test_deployment_boundaries.py` and authority-entrypoint contracts pass.
- EC-003: `.agent/evidence/final-test.log`, `final-fmt.log`, `final-clippy.log`.
- EC-004: `scripts/legacy-contracts/README.md`.

#### Next milestone

MILESTONE-0025

### MILESTONE-0025 — M25 Project Blueprint Framework

- **Status:** deferred
- **Outcome:** Users can define, install, pin and operate versioned project blueprints with typed work items, guarded transitions, templates and explicitly authorized capabilities in local and connected modes.
- **Why now:** Blueprints are Orbit's extension model without per-project Rust code, and I3 capability actions are the largest unreachable block of already-written code.
- **Depends on:** MILESTONE-0024

#### Scope

- I1 model, registry, binding, import/export and validation.
- I2 typed properties, states, transitions, guards, templates and event history.
- I3 trusted capability catalog, action intents, executor composition and explicit authorization.
- I4 bundled packs and opt-in domain extraction.
- I5 CLI/MCP/server surfaces, offline outbox parity, receipts and upgrade policy.

#### Non-goals

- Arbitrary untrusted plugin execution or embedded scripting.
- A visual blueprint designer in this repository.
- A hosted blueprint registry or marketplace.

#### Exit criteria

- [x] EC-001: I1 manifests, registry, binding, import/export and validation are implemented and exercised.
- [x] EC-002: I2 typed properties, transitions, guards and templates are implemented and exercised.
- [ ] EC-003: A trusted capability can be installed through a public composition surface and an offered action can be explicitly requested, claimed, executed and reconciled end to end.
- [ ] EC-004: Connected MCP blueprint writes use the canonical outbox and command receipts, including a lost-ack replay test.
- [ ] EC-005: An explicit, previewable blueprint version upgrade path exists.
- [ ] EC-006: Habit recurrence/streak behavior is owned by an opt-in capability package rather than core.
- [x] EC-007: Local and connected blueprint smoke passes for install, binding, typed properties, guards, templates, offline replay, receipts and restart.

#### Evidence

- EC-001: `cargo test -p orbit-blueprint --locked`; `deploy/server/smoke-blueprint-workflows.py`.
- EC-002: same blueprint smoke; `crates/orbit-storage/src/blueprint_template_tests.rs`.
- EC-003: Unmet; executor primitives exist but publish paths pass an empty installed-capability set and no public action route exists.
- EC-004: Unmet; connected MCP posts directly to blueprint endpoints.
- EC-005: Unmet; binding is first-binding-wins and no upgrade operation exists.
- EC-006: Unmet; habit recurrence remains core.
- EC-007: `.agent/evidence/m15-final.zip` contains `blueprint-workflow-smoke`.

M25 I1/I2 and normal command receipts are implemented and verified. The remaining criteria are deferred because M27 distribution and first-run usability block ordinary users more than further blueprint depth. Previous "source complete" reports covered the I1/I2 source shape only; they did not satisfy EC-003 through EC-006. Reconsider M25 after M27 when a public release and first-run path exist.

#### Next milestone

MILESTONE-0026

### MILESTONE-0026 — M26 Packaging, Installation and Documentation

- **Status:** completed
- **Outcome:** A user can install Orbit from source or a verified prebuilt archive into a disposable prefix, inspect or diagnose the installation, remove only managed binaries, and follow task-oriented documentation to a first project.
- **Why now:** A runnable tree is not usable without a safe install/removal path and accurate documentation.
- **Depends on:** MILESTONE-0015, MILESTONE-0023, MILESTONE-0025

#### Scope

- Local source installer, prebuilt-archive installer and uninstaller.
- Release packaging, checksum manifest, archive verification and release evidence.
- Installation environment inspection and doctor.
- Optional worker and server setup helpers.
- Task-oriented user, release, CLI and MCP documentation.
- Packaging and documentation regression tests.

#### Non-goals

- Distribution signing or publisher authentication.
- A native Windows installer.
- TLS ingress automation or a hosted service.
- Cross-platform build certification.

#### Exit criteria

- [x] EC-001: Install/uninstall and manifest-safety tests pass.
- [x] EC-002: Release archive creation and verification tests pass, including corruption and extra-member rejection.
- [x] EC-003: A real Rust release archive is built, verified, installed, exercised and uninstalled in a disposable prefix.
- [x] EC-004: `orbit-env.sh` check/doctor behavior and optional worker/server setup contracts pass.
- [x] EC-005: Documentation link, command-inventory and release-guidance contracts pass.
- [x] EC-006: The packaged CLI creates and reopens a local SQLite workspace.

#### Evidence

- EC-001: `.agent/evidence/m15-final.zip` installer checks.
- EC-002: `.agent/evidence/m15-final.zip` release-archive checks.
- EC-003: `.agent/evidence/m15-final.zip` `real-release-archive-smoke`.
- EC-004: `.agent/evidence/m15-final.zip` environment/worker/server-setup checks.
- EC-005: `.agent/evidence/m15-final.zip` `user-documentation-contract`.
- EC-006: `tests/packaging/test_real_release_archive.sh`.

#### Next milestone

MILESTONE-0015

### MILESTONE-0015 — M15 Release Acceptance

- **Status:** completed
- **Outcome:** A runnable Orbit build and packaged CLI are accepted for internal testing on Linux x86_64, with local, loopback-connected, offline, blueprint and local-Herdr workflows passing reproducible checks.
- **Why now:** Source implementation is not runtime acceptance; internal testers need a verified runnable baseline.
- **Depends on:** MILESTONE-0022, MILESTONE-0023, MILESTONE-0024, MILESTONE-0025, MILESTONE-0026

#### Scope

- Rust format, check, test and Clippy gates.
- Python worker and maintained contract suites.
- Local CLI workflows with restart and SQLite integrity checks.
- Loopback connected-authority, offline replica, durability and blueprint workflows.
- Deterministic agent contract and opt-in live local Herdr acceptance.
- Installation/release-archive checks and a real build/package/verify/install/uninstall smoke.

#### Non-goals

- TLS/Caddy ingress, browser deployment and public distribution signing.
- Remote-machine Herdr execution against the real Rust server.
- macOS, Windows and cross-compiled target acceptance.
- Real version-to-version installed-database upgrade drills.

#### Exit criteria

- [x] EC-001: `cargo fmt --all -- --check` and `cargo clippy --workspace --all-targets --locked -- -D warnings` pass.
- [x] EC-002: `cargo test --workspace --locked` passes with 1,235 tests and 0 failures.
- [x] EC-003: `./scripts/m15-full-automated.sh --profile full` passes 37 of 37 checks (38 of 38 with opt-in live Herdr) with 0 failed and 0 not-run.
- [x] EC-004: Live local Herdr acceptance passes with a real `opencode` agent in a disposable workspace.
- [x] EC-005: A real release archive is built, verified, installed into a disposable prefix, exercised and uninstalled.
- [x] EC-006: No known blocking data-loss defect remains in the verified scope.

#### Evidence

- EC-001: `.agent/evidence/final-fmt.log`, `.agent/evidence/final-clippy.log`.
- EC-002: `.agent/evidence/final-test.log`.
- EC-003: `.agent/evidence/m15-final.zip` (38 passed with live Herdr, 0 failed, 0 not_run).
- EC-004: `.agent/evidence/herdr-live.log`.
- EC-005: `.agent/evidence/m15-final.zip` `real-release-archive-smoke`.
- EC-006: Passing local/connected/offline/durability/blueprint/agent checks and no observed blocking data-loss defect.

#### Next milestone

MILESTONE-0027

### MILESTONE-0027 — M27 GitHub Distribution, First-Run Experience and User Documentation

- **Status:** blocked
- **Outcome:** A new user can install a verified Orbit binary from a GitHub Release without Rust or Python, initialize and diagnose a local workspace, update safely, and follow accurate task-oriented documentation through real workflows.
- **Why now:** The executable product exists, but it is only installable from source or a local archive and has no first-run, diagnostic, update or complete user documentation path. Internal testers cannot follow the package as a product.
- **Depends on:** MILESTONE-0015, MILESTONE-0023, MILESTONE-0026

#### Scope

- Gated GitHub release workflow for supported Linux and macOS targets, canonical asset names, checksums, build metadata and provenance attestations.
- Root GitHub-hosted POSIX installer that resolves a stable release, verifies it, and reuses the M26 transactional archive installer.
- Versioned unified configuration, `orbit init`, `orbit doctor` and safe `orbit update`/`--check`.
- End-user documentation entry point, installation, first-project, everyday-workflow, agent, connected-mode, MCP and reference guides.
- Automated release, installer, initialization, diagnostics, update and documentation-drift checks.

#### Non-goals

- Publishing an official stable GitHub Release without explicit maintainer authorization.
- Windows support, cross-compilation, Homebrew/Nix/mise, signing-key infrastructure, a hosted documentation service or a plugin marketplace.
- Completing deferred M25 capability, connected-MCP parity, blueprint-upgrade or habit-extraction work.
- TLS ingress certification, remote-machine Herdr execution or real installed-version downgrade support.

#### Exit criteria

- [ ] EC-001: A tag-triggered release workflow validates version consistency, builds and verifies canonical archives for the declared Linux/macOS matrix, records source revision and target metadata, attests provenance, and creates only a gated draft release unless explicitly authorized.
- [x] EC-002: Root `install.sh` works when piped to `sh`, detects supported platforms, resolves the latest stable or an explicit version, verifies checksums before installation, reuses the transactional archive installer, and preserves an existing installation on failure.
- [x] EC-003: `orbit init` creates the documented `~/.orbit` layout, a versioned validated configuration and a local SQLite workspace; it is idempotent, preserves existing configuration and data, and honors CLI > environment > config-file > default precedence including `ORBIT_DB`.
- [x] EC-004: `orbit doctor` reports executable, configuration validity, effective database, authority mode, connected settings and optional dependencies without modifying data or printing secrets.
- [x] EC-005: `orbit update` and `orbit update --check` resolve stable/prerelease channels, verify artifacts, refuse unmanaged installations, back up the database before replacement, and leave the previous binary and data intact on failure.
- [x] EC-006: `docs/README.md`, root README, getting-started, everyday guides and references describe only implemented commands and cover installation, init, first project, projects/tasks/blueprints, notes/habits/routines/schedules/search, agent workflows, connected mode, MCP, backup/recovery and troubleshooting.
- [x] EC-007: Automated checks cover release gating, installer safety, init/doctor/update behavior, documentation links and command/config examples; the existing M15 acceptance suite still passes after integration.
- [x] EC-008: GetDone and the roadmap record actual implementation and separate local fixture acceptance from a real published GitHub Release installation.

#### Evidence

- EC-001: Workflow, installer-asset and version-validation tests pass, but no live GitHub Actions run or published release exists; a maintainer with write access must authorize and run it.
- EC-002: Release-asset installer fixture tests (10) and real local archive install/init/doctor smoke pass; real GitHub asset installation is not verified.
- EC-003: 14 init/doctor acceptance tests and config precedence unit tests pass; `.agent/evidence/m27-i3-init-doctor.txt`.
- EC-004: Doctor read-only, strict, secret-redaction and schema/authority tests pass; `.agent/evidence/m27-i3-init-doctor.txt`.
- EC-005: 12 update acceptance tests (channels, checksums, rollback, backup, unmanaged refusal) pass; `.agent/evidence/m27-i4-update.txt`.
- EC-006: Documentation suite 29 tests passes; `.agent/evidence/m27-m15-final.zip`.
- EC-007: `./scripts/m15-full-automated.sh --profile full` reports 42 passed, 0 failed, 0 not_run; `.agent/evidence/m27-m15-final.zip`.
- EC-008: This roadmap, milestone, task, status, evidence and journal records distinguish fixture acceptance from a real published release.

#### Next milestone

MILESTONE-0028

### MILESTONE-0028 — M28 Project-Centric TUI and Blueprint UI Composition

- **Status:** completed
- **Outcome:** Users navigate Orbit by project, workflows declare reusable views, and a trusted registry renders real application data with authorized actions in a responsive terminal workspace. Basic is the default offline workflow; tasks support descriptions, priority, labels, acceptance criteria and first-class local attachments; external agents manage Basic tasks through MCP without managed-agent orchestration.
- **Why now:** M27's remaining gap is an external publication gate. M28 was the highest-priority unfinished product work that can be completed locally, and the usability recovery turned the shell into a genuinely usable everyday project manager.
- **Depends on:** MILESTONE-0015, MILESTONE-0022, MILESTONE-0023, MILESTONE-0024

#### Scope

- Framework-neutral blueprint view schema and validation.
- Shared `orbit-ui` data contracts, registry, resolution, detail/action models.
- Application-facing project/workspace view data in `orbit-client`.
- Project-centric TUI shell, responsive layouts, theme, focus, palette and search.
- Built-in components and bundled v2 development/research/learning/habits views.
- Focused tests and user/developer documentation.

#### Non-goals

- Flutter implementation, dynamic plugin loading or a marketplace.
- A second persistence model or TUI business logic.
- Rewriting the connected command surface in this milestone.
- Broad M15 release acceptance, signing or cross-platform certification.

#### Exit criteria

- [x] EC-001: A Ratatui-free view schema validates views, layouts, components, sources, bindings and action hints; legacy manifests remain valid.
- [x] EC-002: The local fixed-tab shell is replaced by Home, Inbox, Projects, Agents and project navigation with sidebar/workspace/details regions and responsive layouts.
- [x] EC-003: Project/workspace data resolves projects, tasks, readiness, work items, milestones, knowledge, habits, metrics, schedules and agent jobs through existing services.
- [x] EC-004: A trusted registry provides 13 typed components with empty/loading/error/stale/missing states.
- [x] EC-005: Blueprints declare multiple named views, legacy projects receive generated defaults, and bundled v2 manifests demonstrate differentiated project types.
- [x] EC-006: Contextual actions, forms and the command palette share one catalogue, use authorized client calls and suppress duplicate submissions.
- [x] EC-007: Focused schema, rendering, interaction and architecture tests pass with 1,350 workspace tests and 37 real-PTY acceptance items.
- [x] EC-008: User/developer documentation and GetDone records state actual progress and remaining limitations.
- [x] EC-009: Usability recovery: Basic default workflow, rich task details and acceptance criteria, local attachments, non-Vim navigation, workflow terminology and MCP parity for external agents.
- [x] EC-010: Connected completion: server-authoritative rich task fields, criteria and activity; durable offline attachment staging with exactly-once flush, cross-client retrieval and idempotent removal; explicit agent job completion/interruption with resume guards.

#### Evidence

- EC-001: `cargo test -p orbit-blueprint` 20 tests; `presentation.rs` validation and bundled round-trip.
- EC-002: `crates/orbit-tui/src/{main,app,render,theme}.rs`; wide/narrow TestBackend tests.
- EC-003: `crates/orbit-client/src/view_data.rs`; workspace 1,350 tests including local/connected view data and shared backend.
- EC-004: `crates/orbit-ui` 14 tests; `orbit-tui` 125 tests for metrics, readable boards, tables, details and states.
- EC-005: bundled v2 JSON views; generated fallback and work-item transition tests.
- EC-006: action authority/declaration tests; duplicate-submission and palette/search tests.
- EC-007: `cargo test --workspace --offline` 1,350 passed; 37 PTY; full M15 profile 42/42; clippy and fmt clean.
- EC-008: TUI/blueprint/UI docs and GetDone records.
- EC-009: `blueprints/basic.json`; schema 20 task/attachment tables; TUI 136 tests; external MCP stdio acceptance; 1,423 workspace tests; 40 PTY/MCP acceptance items.
- EC-010: server attachment routes + replica staging; connected suite 6; connected MCP 1; scripted agent scenario 1; workspace 1,489; M15 45/47 (release workflow withheld).

##### Remaining work

- Authorization is single-tenant; per-device project ACLs are not implemented (TODO-0013).
- No server-side attachment GC endpoint (TODO-0014); connected MCP keeps attachments local-only.
- Connected multi-command edits are not atomic; inspect/recover the outbox.
- Connected blueprint/work-item, habit, metric and note editing remains on the legacy command surface.
- Live Herdr Agent Development launch/review acceptance is unavailable without an authorized runtime.

#### Next milestone

none

## Deferred work

- TLS/Caddy ingress and browser deployment — requires DNS/Docker/certificates and a real deployment environment; reconsider when a self-hosted deployment target is available.
- Remote-machine Herdr execution against the real Rust server — requires a second enrolled machine; reconsider before claiming remote agent readiness.
- Distribution signing and authenticated publishing — requires a signing key and release channel; reconsider before any public binary release.
- macOS/Windows/cross-compiled packaging — requires those toolchains and target machines; reconsider before advertising cross-platform support.
- Real installed-version database upgrade and rollback — requires an older release fixture; reconsider before the first supported upgrade.
- M25 capability installation/action execution, connected MCP outbox parity, blueprint upgrade path and habit recurrence extraction — M25 is deferred after M27; I1/I2 are complete, EC-003 through EC-006 remain unmet.
- Official stable GitHub Release publication and real-release installation verification — deferred until a maintainer explicitly authorizes publishing; M27 can implement and fixture-test the pipeline without claiming a published release.
