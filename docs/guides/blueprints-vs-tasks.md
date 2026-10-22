# Blueprint work items versus Orbit tasks

**Projects and tasks** are Orbit's base productivity entities. **Blueprints** are versioned definitions that add domain-specific kinds, properties, states, transitions, templates, and optional capability actions. Orbit calls them **workflows** in the TUI and in prose; the persisted identifiers and the `orbit blueprint` command group keep the original name for compatibility. Every new project starts on the bundled **Basic** workflow.

A blueprint work item is **not automatically** an Orbit task, and an item transition is not a call to an agent runtime.

## Start from a bundled blueprint

```sh
orbit blueprint bundled
orbit blueprint list
orbit blueprint install development
orbit blueprint list
orbit blueprint bind --help
```

Use a real project ID and an installed blueprint version when binding; record that version, since changes to a definition should not silently mutate existing projects. For authoring, export and validate your modified manifest before publishing:

```sh
orbit blueprint export --help
orbit blueprint validate --help
orbit blueprint import --help
```

Inspect `orbit blueprint create-item --help`, `orbit blueprint transition --help`, and `orbit blueprint set-properties --help` to operate on items according to the bound workflow.

## Choosing the correct model

| Need | Use |
| --- | --- |
| One-off actionable work with planning and dependencies | Core Orbit task |
| Custom editorial/research/learning stages | Blueprint work item |
| Shared portfolio project status | Core Orbit project |
| Reusable process across many projects | Versioned blueprint manifest |
| Actually launch a coding agent | Explicit authorized agent job / capability |

## Capability safety

A manifest can **offer** an action but does not authorize arbitrary code execution. A trusted registered provider, authorized action request, durable claim, project/task association, and existing execution policy still apply. If a blueprint item represents development work, explicitly associate it with the appropriate existing Orbit task before using an agent-launch capability. Never assume an item's ID is a task ID.

## When offline

In connected mode, edits can be pending in the client outbox; they become authoritative only after the server accepts them. A pending blueprint action must not create an external side effect. Reconcile uncertain claims instead of blind replay.

Related: [Blueprint walkthrough](blueprint-walkthrough.md), [Custom blueprints](custom-blueprints.md), [MCP integration](mcp-integration.md).
