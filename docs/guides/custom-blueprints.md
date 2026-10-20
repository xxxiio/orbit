# Build your own project workflow

Orbit projects can pin a **versioned blueprint**. A blueprint defines work-item types, custom fields, states, transition guards, reusable templates, and *offered* capability actions. A capability must be separately registered and authorized before it can run.

## 1. Start from a working manifest

Copy `blueprints/development.json` into a file you own. The shipped manifest is a schema example and should not be edited in place. Explore other examples under `blueprints/` for learning, habits and research.

```sh
cp blueprints/development.json my-workflow.json
orbit blueprint validate my-workflow.json
```

Change IDs and descriptions, choose work-item types and properties, and define transitions and optional guards. Use `orbit blueprint validate` after each edit to catch unknown properties and invalid transition references before publication. JSON and YAML (`.yaml`/`.yml`) are supported.

## 2. Publish and pin

```sh
orbit blueprint import my-workflow.json
orbit blueprint list
orbit blueprint show --help
orbit blueprint bind --help
```

Use the installed CLI's help for required project and version arguments. Publication is versioned: changing a file on disk does not mutate a published blueprint version, nor does publishing a newer version automatically rebind projects.

## 3. Customize work items and apply templates

The published blueprint supplies default property values and validates custom fields. Create items with `orbit blueprint create-item --help`, update with `set-properties --help`, and move through allowed states with `transition --help`. State changes are revision checked.

Templates let you create related work items with prerequisites: use `orbit blueprint apply-template --help` to inspect the invocation. Applied templates are committed atomically by the authority rather than one item at a time.

## 3a. Add project views

A blueprint may also declare terminal/UI views. Add a `views` list with layouts,
components, sources, filters, sorting and limits; the renderer only accepts
registered component IDs and read-only data sources. See
[Blueprint-defined project views](blueprint-views.md) for a complete example and
the [UI view reference](../reference/ui-views.md) for every field.

Views are optional. Existing manifests without `views` remain valid and open
through generated defaults.

## 4. Understand action security

A blueprint can offer an action such as `agent.launch`, but the manifest cannot grant itself permission. The domain model and executor primitives for explicit request/authorization exist, but **no CLI, MCP or HTTP surface currently exposes an action request or execution command**, so a user cannot yet request one of these actions from Orbit. When such a surface exists, execution must be explicitly requested and authorized before Orbit persists the action intent for an executor to claim. If delivery is uncertain, do not launch again merely because no success acknowledgment arrived.

## 5. Offline behavior

For a connected client, mutating blueprint commands may enter the canonical SQLite outbox when the server is unreachable. A pending command is **not accepted authoritative state**. Use `orbit blueprint pending` and `orbit blueprint flush` to inspect and submit queued operations. External effects are never executed merely because a command was queued offline.

## 6. Updating existing projects

Publish a new version, inspect the differences, and explicitly bind each project to the chosen version. Do not assume changing a blueprint repairs or migrates existing work items. Back up real data before changing pinned workflows until M15 integration acceptance has verified your upgrade scenario.
