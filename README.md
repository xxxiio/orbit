# Orbit

Orbit is a local-first Rust CLI for projects, tasks, workflow-defined Work
Items, habits, routines, schedules, Markdown notes and optional agent-assisted
execution. The default workspace is a local SQLite database that works offline.
A self-hosted server, Herdr, MCP clients and AI providers are optional.

## Install

```sh
curl -fsSL https://github.com/xxxiio/orbit/releases/latest/download/install.sh | sh
```

The installer is attached to each release together with its transactional core
and verifier, so the public URL stays on `github.com` and does not use raw source
hosting. It downloads a release archive for the detected target, verifies its
SHA-256 checksum, and installs the `orbit` CLI into `~/.local/bin` without
`sudo`. It does not edit shell startup files. Add the prefix to `PATH`:

```sh
export PATH="$HOME/.local/bin:$PATH"
```

If no public release is available yet, install from a checkout with
`./scripts/install.sh --prefix "$HOME/.local"` (requires Rust), or build a
local archive with `./scripts/package-release.sh --build --components cli`.
See the [installation guide](docs/getting-started/installation.md#install-from-this-checkout-before-a-release).
The GitHub workflow creates a draft release, and publishing requires manual
authorization; this repository does not assume a public release exists today.

Pin a version with `--version`, choose a prefix with `--prefix`, or save the
script first:

```sh
curl -fsSL https://github.com/xxxiio/orbit/releases/latest/download/install.sh | sh -s -- --version v0.1.0
curl -fsSL https://github.com/xxxiio/orbit/releases/latest/download/install.sh -o install.sh
sh ./install.sh --version v0.1.0 --prefix "$HOME/.local"
```

Checksums downloaded next to the archive protect against corruption in transit;
they are not publisher authenticity. See
[binary distribution](docs/releasing/binary-distribution.md) and
[platforms and components](docs/releasing/platforms-and-components.md).

## Initialize and check

```sh
orbit init
orbit doctor
```

`orbit init` creates `~/.orbit/config.toml`, the SQLite database, the Markdown
vault, and `logs/`, `state/` and `backups/` directories. It is idempotent and
does not overwrite an existing configuration without `--force`. `orbit doctor`
is read-only and reports the executable, PATH resolution, configuration,
database integrity, authority mode and optional tools.

## First project

```sh
orbit project add "Prototype API"
orbit project list
orbit task add "Design the API" --project PROJECT_ID
orbit task add "Implement the API" --project PROJECT_ID
orbit task depends-on IMPLEMENT_TASK_ID DESIGN_TASK_ID
orbit task ready --now-ms 1792480200000 --all
orbit task start DESIGN_TASK_ID
orbit task complete DESIGN_TASK_ID
```

Replace `PROJECT_ID`, `DESIGN_TASK_ID` and `IMPLEMENT_TASK_ID` with the IDs
printed by the commands. `--now-ms` is Unix milliseconds, not seconds.

## Terminal workspace

`orbit-tui` is a project-centric terminal client over the same authority:

```sh
orbit-tui
```

The sidebar exposes Home/Today, Inbox, Projects and Agents; project screens show
the views declared by the pinned workflow, with generated defaults for older
workflows. Press `?` for the keyboard map, `:` for the command palette and `/`
to search. Mutations use the same authorized client commands as the CLI, while
job completion never silently completes a task.

See the [terminal workspace guide](docs/guides/tui-workspace.md) and
[workflow view authoring](docs/guides/blueprint-views.md).

## Web app

`orbit-server` can serve a browser client at `http://127.0.0.1:8787/`. It signs in
with a device credential enrolled by the server operator and covers:

- Home and Inbox, including task capture, triage and the agent-attention rail.
- Projects with the views their workflow declares: tasks, boards, milestones,
  timelines, metrics, detail, knowledge references and agent jobs.
- Work Items whose fields, states, guards and Workflow Actions come from the
  workflow's kind declaration, with declared moves and capability chips.
- Planning: authoring estimates, earliest starts and deadlines; day plans and
  schedule windows; review, explicit approval, confirmed apply and a receipt.
- Knowledge (tree, note, edit, move, relations), attachments, agent job review
  and resume, project settings, and previewed workflow upgrades.

The browser cannot start an agent job, execute a Workflow Action, rename or
transition a project, create a knowledge folder, or persist saved views; those
limits are stated in the UI. See the [web app guide](docs/web-app.md) and the
[internal-tester quickstart](docs/guides/web-quickstart.md).

## Documentation

The [documentation index](docs/README.md) is organized by task: installation,
first project, everyday work, project workflows, agent work, connected mode, MCP,
reference and release maintenance. Start with the
[local quickstart](docs/getting-started/quickstart.md) or
[installation guide](docs/getting-started/installation.md).

## Status and limits

- Local mode, loopback connected mode and local-Herdr agent workflows are
  runtime-verified on **Linux x86_64** only.
- Declared release targets are `x86_64-unknown-linux-gnu`,
  `aarch64-unknown-linux-gnu`, `x86_64-apple-darwin` and
  `aarch64-apple-darwin`. macOS is **not runtime-verified**; Windows is
  unsupported and there is no native Windows installer.
- The GitHub workflow creates a **draft** release on a tag push and requires
  explicit manual authorization to publish. This repository does not claim that
  a real public GitHub Release has been published.
- Agent session completion does not complete an Orbit task. Run
  `orbit task complete` after reviewing evidence.
- Core task priority, standalone milestone CRUD and a dedicated backup
  subcommand are not implemented in this build. Use deadlines, readiness and
  impact for planning; use `task add --milestone` only as a reference to an
  existing milestone; use the documented SQLite backup procedure.
- Capability execution: Workflow Actions declared by a workflow are listed in
  the web app and API, but the browser does not execute them. Execution belongs
  to the local Orbit authority, and the server exposes no execution route.

See [platforms and components](docs/releasing/platforms-and-components.md),
[the release checklist](docs/releasing/release-checklist.md), and
[upgrades and recovery](docs/releasing/upgrades-and-recovery.md).

## Architecture

| Layer | Responsibility |
| --- | --- |
| `crates/orbit-domain` | Identities, lifecycle rules and domain invariants |
| `crates/orbit-application` | Use cases, ports and command validation |
| `crates/orbit-storage` | SQLite repositories, schema and migrations |
| `crates/orbit-runtime` | Composition root and optional runtime adapters |
| `crates/orbit-client`, `crates/orbit-sync` | Configuration, deployment resolution, replicas and outboxes |
| `crates/orbit-server` | Optional authenticated connected authority |
| `crates/orbit-cli`, `crates/orbit-mcp`, `crates/orbit-tui` | User-facing adapters |
| `deploy/server` | Optional machine, proposal and automation workers |

Local SQLite is authoritative in local mode. In connected mode the server's
SQLite is authoritative; client SQLite is an accepted-state replica plus a
durable pending-command outbox. Pending work is not accepted state.

## Testing and tracking

Documentation checks run with:

```sh
python3 -m unittest discover -s tests/documentation -v
python3 -m pytest -q tests/documentation
```

Compiled CLI acceptance lives under `tests/acceptance/`, packaging checks under
`tests/packaging/`, and the optional workers under `deploy/server/test_*.py`.
GetDone records under `.agent/` own milestone status and release evidence; the
user documentation intentionally does not duplicate that ledger.
