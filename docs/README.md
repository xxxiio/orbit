# Orbit documentation

Orbit is a local-first personal execution system: projects, tasks, habits,
routines, schedules, Markdown notes and optional agent-assisted work in one
Rust CLI. The default workspace is a local SQLite database. A self-hosted
server, Herdr, an MCP client and AI providers are optional.

Start with the section that matches what you are trying to do. Every example
uses commands implemented by the current `orbit` binary; run `orbit --help` or
`orbit <group> <subcommand> --help` on your installed build when in doubt.

## What Orbit is

- A local SQLite workspace for projects, tasks, dependencies and planning.
- A file-authoritative Markdown knowledge vault alongside the database.
- A content-addressed attachment object store for tasks, projects and notes.
- Habits, routines, schedule blocks and deterministic daily planning.
- Versioned project workflows for custom Work Items, declared states, guards and
  Workflow Actions.
- Optional agent jobs, handoffs and human review through Herdr.
- Optional connected mode where a self-hosted server is authoritative.

## Install

- [Installation](getting-started/installation.md) — one-command install, versions, prefixes, checksums, update and uninstall.
- [Initialization](getting-started/initialization.md) — home layout, configuration, precedence and `orbit doctor`.
- [Installation components](guides/installation-components.md) — choose CLI, MCP, TUI, server or worker.
- [Platforms and components](releasing/platforms-and-components.md) — supported targets and verification status.
- [Binary distribution](releasing/binary-distribution.md) — archive layout, checksums and offline installs.
- [Troubleshooting](guides/troubleshooting.md) — common installation and runtime failures.

## First 10 minutes

1. Install Orbit and put the install prefix on `PATH`.
2. Run `orbit init`, then `orbit doctor`.
3. Create a project, add two tasks and make one depend on the other.
4. Add acceptance criteria to a task (or capture longer criteria in a note).
5. Complete a task and inspect the result.

- [Local quickstart](getting-started/quickstart.md) — a complete first session with representative output.
- [Your first project](getting-started/first-project.md) — project, tasks, criteria and planning.
- [Switching projects and workspaces](guides/switching-projects-and-workspaces.md) — `--db`, `ORBIT_DB`, `--project` and multi-workspace use.
- [Troubleshooting](guides/troubleshooting.md) — when a command fails.

## Terminal workspace

- [The Orbit terminal workspace](guides/tui-workspace.md) — project-centric navigation, keyboard, panels and agent monitoring.
- [TUI redesign specification](guides/tui-redesign-spec.md) — the proven M31/M32 web usage, translated into terminal patterns, non-goals, the shared API gaps and a measurable acceptance checklist. Implements no TUI change.
- [Workflow-defined project views](guides/blueprint-views.md) — declare overviews, boards, tables, metrics and detail views; Basic is the default.
- [Task details, acceptance criteria and attachments](guides/task-details-and-attachments.md) — rich task fields, ordered criteria and the managed attachment store.
- [UI view reference](reference/ui-views.md) — exact component, source, binding and action schema.
- [UI component and renderer development](developers/ui-components.md) — registration, data providers, actions and future Flutter integration.

## Everyday workflows

- [Projects and planning](guides/projects-and-planning.md) — modes, criteria, dependencies, readiness and planning.
- [Habits, routines and schedules](guides/habits-routines-schedules.md) — recurring personal work and calendar blocks.
- [Notes and folders](guides/notes-and-folders.md) — the Markdown vault, references and vault assets.
- [Search and navigation](guides/search-and-navigation.md) — `search`, `today`, `status` and task drilldowns.
- [Backup and recovery](guides/backup-and-recovery.md) — the built-in `.orbitbak` workspace backup, verification, isolated restore, maintenance replacement, automatic schedules/retention and the offline fallback.
- [Development projects](guides/development-projects.md) — a software project from task to accepted change.
- [Learning projects](guides/learning-projects.md) — learning evidence, notes and review.
- [Research workflow](guides/research-workflow.md) — research questions, evidence and decision memos.
- [Offline operation](guides/offline-operation.md) — local authority, replica queues and pending work.

## Workflows

- [Work Items versus tasks](guides/blueprints-vs-tasks.md) — when a workflow-defined record is the right model.
- [Custom workflow walkthrough](guides/blueprint-walkthrough.md) — export, edit, validate, publish and bind.
- [Build your own project workflow](guides/custom-blueprints.md) — manifest structure, kinds, states, guards, actions and safety boundaries.

## Agent-assisted work

Agent execution is optional and requires an existing Orbit task plus a
configured Herdr runtime. A finished agent session does **not** complete the
Orbit task.

- [Agent workflow tutorial](guides/agent-workflow-tutorial.md) — ten explicit steps from repository preparation to task completion.
- [Agent results: inspect, review, continue and resume](guides/agent-review-and-resumption.md) — evidence, review decisions and recovery.
- [Optional machine workers](guides/worker-deployment.md) — Herdr execution on enrolled machines.

## Optional connected mode and server

- [Connected mode onboarding](getting-started/connected-mode.md) — authority, enrollment, replicas and offline queues.
- [Self-hosting](guides/self-hosting.md) — optional server deployment and TLS boundaries.
- [The Orbit web app](web-app.md) — Home, Inbox, Projects, workflow views, Work Items, planning, knowledge, attachments, agents, settings, upgrades and the Backup & Restore page; includes the current limits.
- [Web app quickstart for internal testers](guides/web-quickstart.md) — build the client, start the server, enrol a device, sign in, create the first project, run the dogfood cycle and acceptance suite.
- [Local trusted web quickstart](guides/local-web-quickstart.md) — the opt-in, loopback-only mode with no device enrollment, plus its security boundary.
- [Server API](server-api.md) — the current authority endpoints, not the retired relay.

## MCP and extensions

- [MCP integration](guides/mcp-integration.md) — client configuration, permissions and workflow examples.
- [MCP tools reference](reference/mcp.md) — registered tools, modes and authority rules.
- [Capability and provider authoring](developers/extensions-and-capabilities.md) — trusted extension boundaries.
- [Trusted provider example](developers/capability-provider-example.md) — a worked provider contract.

## Reference

- [CLI reference](reference/cli.md) — every top-level group, subcommand and important flag, including `orbit backup`.
- [Workspace backup format](reference/backup-format.md) — the `.orbitbak` container, manifest contract, verification semantics and restore expectations.
- [Configuration reference](reference/configuration.md) — exact schema, defaults, precedence, environment and failure behavior.
- [Release assets reference](reference/release-assets.md) — canonical names, archive layout and verified matrix.
- [MCP tools reference](reference/mcp.md)
- [CLI contract](cli-contract.md) — stable output, exit codes and workspace selection.
- [Architecture](architecture.md) — authority and component boundaries.

## Releasing (maintainers)

- [Release checklist](releasing/release-checklist.md) — evidence and authorization gates.
- [Binary distribution](releasing/binary-distribution.md) — packaging, verification and `install.sh`.
- [Platforms and components](releasing/platforms-and-components.md) — support matrix and unverified targets.
- [Upgrades and recovery](releasing/upgrades-and-recovery.md) — pre-upgrade recovery points, restore and database recovery.
- [Source completion handoff](releasing/source-completion.md) — implemented interfaces versus certified release.

## Verification status

Linux x86_64 local, loopback-connected and local-Herdr workflows are the only
runtime-verified targets in this build. macOS target triples are declared in the
release matrix but **not runtime-verified**; Windows is unsupported. A GitHub
Release workflow exists and produces draft releases on tag push; publishing
requires manual authorization. This current user-facing documentation does not claim that a real public GitHub Release has been published or that macOS was executed. See
[platforms and components](releasing/platforms-and-components.md) for the exact
matrix and [the release checklist](releasing/release-checklist.md) for gates.

## Archive and history

Historical milestone notes under `docs/mNN-*.md`, `docs/history/` and
`docs/historical-*` are provenance records, not user guides. They may describe
retired commands and must not be used as current instructions.
