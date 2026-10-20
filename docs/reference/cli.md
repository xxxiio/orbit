# CLI reference (source-backed)

`orbit` is the single public primary CLI. It is local-first, and the default workspace is not a connected server replica. See `orbit --help`, `orbit <group> --help`, and `orbit <group> <subcommand> --help` for version-specific argument details. This reference documents commands in `crates/orbit-cli/src/commands.rs` and `blueprint_commands.rs`; comprehensive execution validation belongs to M15.

## Global options

| Option | Purpose |
| --- | --- |
| `--db PATH` | Select local SQLite workspace (global) |
| `--json` | Request structured output (global) |
| `--help` | Inspect usage and option definitions |

## Top-level commands

| Command | Use |
| --- | --- |
| `capabilities` | Inspect planning capabilities and contracts |
| `project` | Projects, criteria, health, evidence, metrics |
| `task` | Core tasks, dependencies, deterministic planning |
| `blueprint` | Versioned project blueprints and custom items |
| `agent` | Optional agent execution, handoffs, reviews |
| `habit`, `routine` | Personal routines and habits |
| `note`, `folder` | Notes and folder management |
| `schedule` | Calendar/scheduling |
| `sync` | Connected authority and offline replica |
| `search`, `status`, `today` | Find items and inspect current workspace |

## Everyday workflows

| Intent | Commands |
| --- | --- |
| New project | `orbit project add`, `orbit project list`, `orbit project status` |
| Manage tasks | `orbit task add`, `orbit task list`, `orbit task start`, `orbit task complete` |
| Dependencies | `orbit task depends-on`, `orbit task unblock`, `orbit task graph`, `orbit task ready` |
| Planning | `orbit task next`, `orbit task day-plan`, `orbit task workload`, `orbit today` |
| Inspect blueprint | `orbit blueprint bundled`, `orbit blueprint list`, `orbit blueprint show` |
| Publish blueprint | `orbit blueprint validate`, `orbit blueprint import`, `orbit blueprint export` |
| Use blueprint | `orbit blueprint bind`, `orbit blueprint binding`, `orbit blueprint create-item`, `orbit blueprint item`, `orbit blueprint transition`, `orbit blueprint set-properties`, `orbit blueprint apply-template` |
| Connected blueprint outbox | `orbit blueprint pending`, `orbit blueprint flush` |
| Agent lifecycle | `orbit agent start`, `orbit agent status`, `orbit agent overview`, `orbit agent attention`, `orbit agent handoff`, `orbit agent handoffs`, `orbit agent review`, `orbit agent resume-context`, `orbit agent resume` |
| Connected agent intentions | `orbit agent queue-start`, `orbit agent queue-continue`, `orbit agent queue-resume`, `orbit agent pending`, `orbit agent flush` |
| Connected sync | `orbit sync replica`, `orbit sync authority` |

### Safety and operational distinctions

`orbit task complete` accepts an Orbit task; a terminated agent process does **not** automatically complete a task. `orbit blueprint transition` changes the blueprint-defined item state and does **not** imply launching an agent. Blueprint mutation requests in connected mode may queue while offline; **pending is not accepted**. The older `orbit blueprint queue ...` command is not supported.

`orbit agent start` requires a task and repository plus an optional agent choice; Herdr and the specific agent must be configured to execute. `orbit agent continue` sends to an existing session; ambiguous delivery should be reconciled instead of retried blindly. `orbit agent review` records a decision, not task completion.

See [local quickstart](../getting-started/quickstart.md), [connected mode](../getting-started/connected-mode.md), and [agent review](../guides/agent-review-and-resumption.md).
