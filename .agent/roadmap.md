---
template: roadmap
template_version: 2.0.0
template_digest: "938b804e31855ad2d8e66c8a04f659b8750c6c3dbe1480242264f7cc5c1732ba"
project_owned: true
record_contract: roadmap
record_schema_version: 1
status: current
current_milestone: MILESTONE-0032
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

- **Status:** completed
- **Outcome:** Users can define, install, pin and operate versioned project blueprints with typed work items, guarded transitions, templates and explicitly authorized capabilities in local and connected modes.
- **Why now:** Blueprints are Orbit's extension model without per-project Rust code; M30 completed the capability, connected-outbox, upgrade and habit-ownership criteria.
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
- [x] EC-003: A trusted capability can be installed through a public composition surface and an offered action can be explicitly requested, claimed, executed and reconciled end to end.
- [x] EC-004: Connected MCP blueprint writes use the canonical outbox and command receipts, including a lost-ack replay test.
- [x] EC-005: An explicit, previewable blueprint version upgrade path exists.
- [x] EC-006: Habit recurrence/streak behavior is owned by an opt-in capability package rather than core.
- [x] EC-007: Local and connected blueprint smoke passes for install, binding, typed properties, guards, templates, offline replay, receipts and restart.

#### Evidence

- EC-001: `cargo test -p orbit-blueprint`; `deploy/server/smoke-blueprint-workflows.py`.
- EC-002: same blueprint smoke; `crates/orbit-storage/src/blueprint_template_tests.rs`.
- EC-003: `orbit-runtime`/`orbit-storage` capability tests (542 + 105), `tests/acceptance/test_m30_capability_workflow.py` (restart/replay, connected refusal), TUI capability-action tests and PTY scenario.
- EC-004: `cargo test -p orbit-mcp` (76 passed, 8 outbox tests) and `tests/acceptance/mcp/test_m30_blueprint_outbox.py` (offline pending → accepted receipt → replay → lost-ack → duplicate-id conflict; server receipt counts).
- EC-005: `crates/orbit-runtime/src/blueprint_upgrade.rs` with 13 tests; CLI 128 tests; `tests/acceptance/local_cli_workflows.py` (60 passed incl. preview → refusals → apply → restart).
- EC-006: `crates/orbit-habit-capability` (5 tests), domain 56, application 17, runtime 542; install/authorize gating tests and the executed habits guide.
- EC-007: `.agent/evidence/m15-final.zip` blueprint smoke plus the M30 acceptance runs above.

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

- [x] EC-001: A tag-triggered release workflow validates version consistency, builds and verifies canonical archives for the declared Linux/macOS matrix, records source revision and target metadata, attests provenance, and creates only a gated draft release unless explicitly authorized.
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
- Connected blueprint/work-item binding and project-status editing remain local-only or limited to specific console commands; see M29.
- Live Herdr Agent Development launch/review acceptance is unavailable without an authorized runtime.

#### Next milestone

MILESTONE-0029

### MILESTONE-0029 — M29 TUI Product Experience, Interaction Quality and Maintainability

- **Status:** completed
- **Outcome:** Working context, discoverable actions, safe forms, responsive layout, one local/connected shell and maintainable modules with measured interaction improvements.
- **Why now:** M28 completed the product surfaces; daily use exposed interaction defects and maintenance concentration that block confident internal testing.
- **Depends on:** MILESTONE-0028

#### Scope

- Stable selection, component, focus and scroll restoration with safe fallback.
- Ranked compact action bar, grouped menu, ranked palette with recents and disabled reasons.
- Quick edit, reordered task forms, field-level validation, multiline editing and discard safety.
- Component-aware layout 60x18–160x45, windowed Kanban, priority tables, refined no-colour-safe theme.
- Shared-shell connected pending inspector, conflict review and command console; legacy connected TUI deleted.
- `app/`, `input/`, `render/` extraction, realistic fixture and measured performance work.
- PTY scenarios A–G, documentation upgrade and real terminal captures.

#### Non-goals

- New domain capabilities, a Flutter renderer or dynamic blueprint execution.
- Asynchronous runtime, direct SQL in the TUI or a second persistence model.
- Per-device ACLs, server attachment GC or public release verification.

#### Exit criteria

- [x] EC-001: Selected item, component, scroll and focus restore across view changes and restart, with safe fallback and corrupt-state tolerance.
- [x] EC-002: Focus, selection and back-context are preserved across modals, resize and view switches; Esc recovers context.
- [x] EC-003: Contextual actions are discoverable with honest keys; the palette ranks deterministically, remembers destinations and refuses disabled entries.
- [x] EC-004: Quick edit and forms are predictable, preserve input, mark failing fields and confirm discards.
- [x] EC-005: Layout adapts at five sizes; the board windows columns and keeps selection visible; tables and details prioritise information.
- [x] EC-006: The theme is restrained, colour-independent and uses complete status glyphs with useful empty states.
- [x] EC-007: One shell serves local and connected operation; pending/conflict recovery is inspectable and the legacy presentation is retired without capability loss.
- [x] EC-008: The event loop is simple, dispatch has one owner, forms/modals/render have explicit owners and prior tests pass.
- [x] EC-009: The 20/200/1,017 fixture is measured before and after; quick edit and render hot paths improved where the data justified it.
- [x] EC-010: PTY scenarios A–G pass, documentation matches behaviour and captures exist.

#### Evidence

- EC-001: `ui_state` schema v1 + 18 state tests; PTY Scenario A (3 passed).
- EC-002: navigation/focus tests; PTY Scenarios A/C/F.
- EC-003: action/palette tests; captures `100x30-action-menu.txt`, `100x30-palette-query.txt`.
- EC-004: quick-edit/form tests; PTY Scenario B.
- EC-005: five-size layout/kanban/table tests; PTY Scenario D; board captures at 60x18/100x30/160x45.
- EC-006: theme glyph tests; `60x18-home.txt` under `NO_COLOR`.
- EC-007: `connected.rs` deleted; inspector/conflict/console tests; PTY Scenario E; contract guards.
- EC-008: `main.rs` 196 lines; `input/`, `app/`, `render/` trees; workspace 1,563 tests.
- EC-009: `scripts/tui-fixture.py`; `.agent/evidence/m29-perf-{baseline,after}.{md,json}`.
- EC-010: TUI suite 54 passed/1 skipped; full fixture 2 passed; 21 captures; docs rewritten; M15 45/47 (withheld release workflow).

#### Next milestone

MILESTONE-0030

### MILESTONE-0030 — M30 TUI Workspace Simplification and Executable Blueprint Capabilities

- **Status:** completed
- **Outcome:** A two-region default TUI workspace with an explicit dismissible inspector, plus a trusted Blueprint capability that can be installed, authorized per project, offered, requested, claimed, executed and inspected through public CLI/TUI surfaces with durable receipts.
- **Why now:** M28/M29 completed the project-centric TUI; the permanent Details column was a layout defect, and M25 EC-003 — the only capability path that makes the Blueprint extension model demonstrable — was unmet while every publication path supplied an empty installed-capability set. M27 remains externally blocked, so this is the highest-priority work that can be completed and verified locally.
- **Depends on:** MILESTONE-0024, MILESTONE-0028, MILESTONE-0029

#### Scope

- Two-region default layout; explicit inspector overlay with full-screen fallback.
- Durable trusted-capability installation, project-scoped authorization, offers, explicit requests, single-claim execution, receipts and reconciliation.
- Deterministic built-in reference capability proving the whole public composition path.
- Capability/action CLI with fail-closed connected behaviour.
- TUI capability action discovery, inputs, confirmation and honest dispositions.
- Focused tests, PTY scenarios, isolated end-to-end acceptance, documentation and records.

#### Non-goals

- Reopening M22–M29 or rebuilding the TUI architecture.
- Arbitrary plugin loading, a marketplace or untrusted executable code.
- Connected capability execution, MCP capability tools or public release publication.

#### Exit criteria

- [x] EC-001: Two-region default layout at 80x24, 120x35 and 160x45 with no permanent Details box.
- [x] EC-002: Explicit inspector with preservation, focus restoration, small-terminal fallback, missing-entity fallback and edit round trip.
- [x] EC-003: Trusted capability installed, authorized, offered, requested, claimed, executed and receipted through a public composition path.
- [x] EC-004: Uninstalled, unauthorized, unsupported, invalid-input, revision and duplicate cases fail closed without duplicate effects.
- [x] EC-005: TUI discoverability and execution with inputs, confirmation and accurate dispositions; no renderer mutations.
- [x] EC-006: Documented CLI programmatic path; connected mode refuses rather than bypasses.
- [x] EC-007: Isolated end-to-end acceptance including restart, plus PTY and runtime tests.
- [x] EC-008: Documentation and records updated; M25 EC-003 closed with evidence.

#### Evidence

- EC-001: `orbit-tui` 17 inspector tests at 80x24/120x35/160x45; PTY `test_m30_inspector.py`.
- EC-002: `Modal::Inspector` tests for Enter/Esc/D, full-screen boundary, deleted entity, edit round trip, render purity; PTY cases.
- EC-003: `capability_composition.rs` + `capability_installs.rs`; runtime 527 and storage 105 tests; `tests/acceptance/test_m30_capability_workflow.py`.
- EC-004: offers availability matrix and request/claim/result rejection tests in `capability_composition.rs`.
- EC-005: `app/capability.rs` + 9 TUI tests + PTY `test_m30_capability_ui.py`.
- EC-006: CLI `blueprint capability|action` in `blueprint_commands.rs`; connected refusal in `connected_blueprint.rs`; acceptance test asserts refusal.
- EC-007: acceptance pytest restart/replay; workspace 1,623 tests; PTY 65 passed/1 skipped.
- EC-008: updated docs tree; M25 EC-003 evidence; `.agent` records; `getdone-validate-project`.

#### Next milestone

MILESTONE-0031

### MILESTONE-0031 — M31 Self-Hosted Web Application as Orbit's Reference Product Experience

- **Status:** completed
- **Outcome:** A self-hosted browser client served by `orbit-server` that lets a person sign in, manage projects and tasks, plan work, drive workflow work items and upgrades, browse knowledge, handle attachments and inspect agent jobs — and that has been used for a real Orbit development cycle, producing the evidence-based input for the next TUI redesign.
- **Why now:** M22–M30 built real domain and application capability reachable only through the CLI/TUI, and the only browser surface was an administrative console. The web app is the internal dogfooding environment, the reference experience for a future TUI redesign, and the reference client for the planned Flutter app.
- **Depends on:** MILESTONE-0024, MILESTONE-0025, MILESTONE-0028, MILESTONE-0029, MILESTONE-0030

#### Scope

- Browser session authentication, CSRF defence and SPA serving through the existing server.
- Home, Inbox, Projects, project workspaces, task drawer and shareable list state.
- Workflow-driven views, work items, transitions, guards, upgrade preview/apply and honest action discovery.
- Planning with authored capacity, review, approval and fingerprint-fenced apply.
- Knowledge, attachments and agent job/handoff/review inspection.
- A real dogfooding cycle, refinement, documentation and a derived TUI redesign specification.

#### Non-goals

- Reopening M22–M30, the M25 workflow engine, or TUI architecture in this milestone.
- Offline browser editing, a second business-logic layer, authoritative scheduling in TypeScript.
- Hosted cloud dependencies, third-party identity providers, or administrator privileges in the product UI.
- Connected capability execution, agent launch from the browser, or Flutter implementation.

#### Exit criteria

- [x] EC-001: Browser session authentication with HttpOnly cookies, CSRF defence, expiry and revocation.
- [x] EC-002: Full project and task management in the browser including descriptions, criteria, dependencies, priorities and attachments.
- [x] EC-003: Home and Inbox functional with server-derived readiness, blockers and due/overdue data.
- [x] EC-004: Five bundled workflows produce functional workspaces through one view registry, with work items, transitions, upgrade preview/apply and honest action discovery.
- [x] EC-005: No user-visible legacy product term; enforced by an automated check over the frontend.
- [x] EC-006: Knowledge, attachments and agent jobs/handoffs inspectable within existing authorization and integrity limits.
- [x] EC-007: Real-browser acceptance against an isolated authoritative server covering the acceptance surface.
- [x] EC-008: An Orbit Development project managed through the browser for a real planning-to-completion cycle, with a layer-classified issue log.
- [x] EC-009: User documentation matches the shipped product and every documented command was executed.
- [x] EC-010: A TUI redesign specification derived from real usage, classifying each proven pattern.

#### Evidence

- EC-001: `crates/orbit-server/src/session.rs` + session/CSRF HTTP tests; Playwright `auth.spec.ts` (wrong token, login, reload, sign-out, expiry); live curl smoke (login 201 + cookie, CSRF-less mutation 403).
- EC-002: partial — projects, tasks, descriptions, criteria, priorities, dependencies, milestones and planning are implemented and covered (`tasks.spec.ts`, `planning-estimates.spec.ts`); attachments exist (I4) but the full criterion stays open until I6 re-checks the whole list.
- EC-003: Playwright `home.spec.ts` (server readiness, overdue, empty workspace) and `tasks.spec.ts` (Inbox capture/triage).
- EC-004: `workspaces-*` and `work-items.spec.ts`; five workflows resolve through one registry; upgrade preview/apply and action honesty panels; `web/evidence/i2`–`i3`.
- EC-005: `web/scripts/check-terminology.mjs` scanning 217 files with zero findings; `terminology.spec.ts` asserts rendered routes and titles.
- EC-006: partial — knowledge, attachments and agent job/handoff/review surfaces ship (`knowledge.spec.ts`, `attachments*.spec.ts`, `agents.spec.ts`); criterion stays open until I6 re-checks authorization/integrity limits end to end.
- EC-007: Playwright suite (65 passed, 4 opt-in captures skipped) against per-worker real servers plus the I5 dogfood script on an isolated persistent server.
- EC-008: `web/tests/e2e/i5/dogfood.mjs` + `web/evidence/i5/` (run on `987bcf1`, evidence `dfcfb8a`; dist/binary hashes, 30 captures, DB+WAL), issue log `.slim/deepwork/m31-web-app/i5-issues.md`.
- EC-009: partial — `docs/web-app.md` covers I1 and is being extended for I3–I6 in I6; criterion stays open until every documented command is re-executed.
- EC-010: open — the TUI redesign specification is an I6 deliverable.

#### Next milestone

none

### MILESTONE-0032 — M32 Web Product Completeness, Agent Workflows & Extended Dogfooding

- **Status:** active
- **Outcome:** The M31 browser client becomes a daily-use product: editable project identity, one search across projects/tasks/knowledge, saved views, managed knowledge folders and note links, browser-authorized agent execution, and connected trusted Workflow Capability execution — each proven by real-browser acceptance and by repeated Orbit Development use.
- **Why now:** M31 could inspect more than it could operate. Rename, status, cross-project search, saved filters, folders, note links, agent launch and connected capability execution all required the CLI, so ordinary workflows had workarounds in the middle. Finishing them is the highest-priority unfinished product work, and the resulting proven experience is the correct input for the TUI redesign that follows.
- **Depends on:** MILESTONE-0024, MILESTONE-0025, MILESTONE-0030, MILESTONE-0031

#### Scope

- Authoritative project rename/status/metadata commands with revision fencing and idempotent receipts.
- One server-backed bounded search over projects, tasks and knowledge.
- Saved views as personal presentation preferences, distinct from workflow-defined views.
- Authoritative task created/updated timestamps with honest unknown state.
- Knowledge folders, document move/rename, note links, back-references and broken-link indication.
- Project-contextual knowledge and entity association reuse.
- Browser-authorized agent job launch, monitoring, evidence, handoff and review.
- Connected trusted Workflow Capability execution with grants, claims, receipts and reconciliation.
- Extended dogfooding on a persistent Orbit Development workspace and fixes in the owning layer.
- Reproducible clean frontend installation, final acceptance and an updated TUI redesign specification.

#### Non-goals

- Rebuilding the M31 web app; a second TypeScript domain engine; direct SQL in HTTP handlers.
- Any TUI redesign or TUI feature work in this milestone.
- Reopening M22–M31, or M27 release publication and M15 release qualification.
- Flutter, arbitrary executable plugins, a public marketplace, cloud AI, mandatory Herdr, multi-tenancy.
- A server dependency for local-only CLI/TUI operation.

#### Exit criteria

- [x] EC-001: Project rename and status change through authoritative commands, with stable identity, fencing, idempotency and persistence.
- [x] EC-002: Unified bounded server-backed search across projects, tasks and knowledge with navigable grouped results.
- [x] EC-003: Saved views persist across browser restart and never modify a workflow definition.
- [x] EC-004: Task created/updated timestamps from recorded history, with explicit unknown state.
- [x] EC-005: Knowledge folders and documents can be created, moved, renamed and browsed; links have back-references and broken-link indication; knowledge is reachable from project context.
- [ ] EC-006: An authorized agent job can be launched and reviewed from the browser, with restart persistence and no automatic task completion.
- [ ] EC-007: Trusted Workflow Capabilities execute through the authoritative server with grants, claims, durable receipts, retry safety and reconciliation.
- [ ] EC-008: Extended browser dogfooding with recorded findings and layer-correct fixes.
- [ ] EC-009: Real-browser acceptance for every new operation against an isolated authoritative server.
- [ ] EC-010: Reproducible clean frontend installation in a supported environment.
- [ ] EC-011: Workflow vocabulary enforced; user documentation matches shipped behaviour.
- [ ] EC-012: TUI redesign specification updated from proven M31/M32 usage; no TUI redesign implemented.

#### Evidence

- EC-001: I1 — authoritative project update command, status edges read and Playwright `project-settings.spec.ts` (see `MILESTONE-0032.md`).
- EC-002: I1 — `GET /authoritative/search` and the palette search surface; Playwright `search.spec.ts`.
- EC-003: I1 — saved-view commands, storage and browser-restart persistence; Playwright `saved-views.spec.ts`.
- EC-004: I1 — task-detail timestamps derived from recorded activity; Rust and Playwright `task-metadata.spec.ts`.
- EC-005: I2 — shared typed-reference grammar, reference/project-knowledge routes, a folder-command queue fix, and `knowledge-folders`/`knowledge-links`/`project-knowledge` browser specs.
- EC-006: I3 — browser agent launch, monitoring, handoff and review (open).
- EC-007: I4 — connected Workflow Capability execution through the server (open).
- EC-008: I5 — extended dogfooding record and layer-correct fixes (open).
- EC-009: I6 — full real-browser acceptance against an isolated server (open).
- EC-010: I6 — reproducible clean frontend installation (open).
- EC-011: I6 — terminology check and user documentation matching shipped behaviour (open).
- EC-012: I6 — updated TUI redesign specification; no TUI redesign implemented (open).

#### Next milestone

none

## Deferred work

- TLS/Caddy ingress and browser deployment — requires DNS/Docker/certificates and a real deployment environment; reconsider when a self-hosted deployment target is available.
- Remote-machine Herdr execution against the real Rust server — requires a second enrolled machine; reconsider before claiming remote agent readiness.
- Distribution signing and authenticated publishing — requires a signing key and release channel; reconsider before any public binary release.
- macOS/Windows/cross-compiled packaging — requires those toolchains and target machines; reconsider before advertising cross-platform support.
- Real installed-version database upgrade and rollback — requires an older release fixture; reconsider before the first supported upgrade.
- M25 is complete (EC-001..EC-007). Remaining product follow-ups: connected capability execution, MCP capability tools, per-device project ACLs and server attachment GC.
- Official stable GitHub Release publication and real-release installation verification — deferred until a maintainer explicitly authorizes publishing; M27 can implement and fixture-test the pipeline without claiming a published release.
