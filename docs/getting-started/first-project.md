# First project: local workflows

Start in local-authoritative mode, which needs neither a server nor an AI provider.

```sh
export ORBIT_DB="$HOME/.orbit/tutorial.db"
orbit status
orbit project --help
orbit task --help
```

Create the project using the syntax shown by `orbit project --help`, then identify its project ID using `orbit project list`. The exact required arguments depend on the installed CLI version; the help output is authoritative. Add tasks with `orbit task add --help`, inspect them with `orbit task list`, and review prerequisites with `orbit task ready --help` and `orbit task graph --help`.

## Custom project workflows

Orbit distinguishes **blueprint definitions** from individual projects. A blueprint is a versioned definition of work item kinds, properties, states, guards, dependencies, and optional capability actions.

```sh
orbit blueprint bundled
orbit blueprint install development
orbit blueprint list
orbit blueprint validate blueprints/development.json
```

Bind a specific installed blueprint version to an existing project using `orbit blueprint bind --help`. A project stays pinned to that blueprint version until explicitly rebound. Applying a template is an authoritative operation; review its intended contents before running `orbit blueprint apply-template --help`.

Blueprints do not automatically execute plugins. An offered agent action still needs an explicit authorized request.

## Working with coding agents

Agent execution is optional. When Herdr and the required agent are configured, use `orbit agent start --help` to select the task, repository and machine. Inspect progress with `orbit agent overview`, collect/review handoffs with `orbit agent handoffs --help` and `orbit agent review --help`, and resume unfinished work with `orbit agent resume --help`.

Agent-session completion is **not** task acceptance. Review evidence and acceptance criteria before marking the task done.

## What happens while disconnected?

In local mode, mutations are immediately authoritative. In connected mode, offline-capable operations enter the canonical pending-command outbox. They are *not* accepted until the server acknowledges them. Agent launches and other external actions must not run merely because a disconnected client queued an intention. See [Offline operation](../guides/offline-operation.md).
