# Local quickstart: first 15 minutes

Orbit runs with an authoritative local SQLite workspace. Start here before setting up a server or an agent runtime. All commands below are implemented by the `orbit` CLI; IDs returned by create operations must be substituted into later commands.

## 1. Install and inspect

Install from this source tree with `./scripts/install.sh`, or from a verified binary archive with `./scripts/install.sh --from-archive /path/to/archive.tar.gz`. Confirm `orbit --help` and `orbit status` work. Optional binaries and Herdr are **not** needed for local work.

The global `--db` switch selects the workspace: `orbit --db "$HOME/.orbit/quickstart.db" status`. Use this same switch on *every command* in a tutorial workspace; don't accidentally split work across databases. `--json` emits machine-readable results.

## 2. Create a project and tasks

```sh
orbit project add "Prototype API"
orbit project list
orbit task add "Design API" --project PROJECT_ID
orbit task add "Implement API" --project PROJECT_ID
orbit task list
```

Replace `PROJECT_ID` with the ID returned by `project add`. Use `orbit task add --help` if your installed build presents different optional flags.

## 3. Dependencies and planning

```sh
orbit task depends-on IMPLEMENT_TASK_ID DESIGN_TASK_ID
orbit task graph --now-ms 1792454400000 --all
orbit task ready --now-ms 1792454400000 --all
orbit today
```

The `--now-ms` argument is a Unix millisecond timestamp, **not** seconds. Readiness and planning are informational; they do not complete work or launch external processes.

## 4. Add a project blueprint (optional)

```sh
orbit blueprint bundled
orbit blueprint install development
orbit blueprint list
orbit blueprint bind PROJECT_ID development 1
orbit blueprint binding PROJECT_ID
```

A blueprint controls its own versioned work items and workflow; it does **not** replace the core Orbit tasks created above. Confirm the installed blueprint version before binding it.

## 5. Keep working and inspect

```sh
orbit task list
orbit project list
orbit status
```

Learn the agent workflow in [Agent review and resumption](../guides/agent-review-and-resumption.md), or see [Connected mode onboarding](connected-mode.md) when you need a self-hosted server. Do not mix local authoritative and connected replica databases.
