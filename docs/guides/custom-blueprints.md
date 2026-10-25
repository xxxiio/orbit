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

## 4. Offer a trusted capability action

A blueprint can offer an action such as `agent.launch` or `append_note`, but the manifest cannot grant itself permission and cannot ship the implementation. Installation, authorization, request and execution are separate operations and must happen in this order:

1. install the capability in the local workspace;
2. import a manifest that declares it in `required_capabilities`;
3. bind or apply that manifest to a project;
4. authorize the capability for that project with `--confirm`;
5. request the action on an item at its current revision;
6. execute the recorded command and inspect the receipt.

The transition action declares which keys a request may supply. The capability validates them with its own input schema.

### Runnable example

Run this from the repository root after `cargo build -p orbit-cli --offline`. It uses an isolated home, configuration and scratch database.

```sh
cargo build -p orbit-cli --offline
DEMO=/tmp/orbit-capability-demo
rm -rf "$DEMO"
mkdir -p "$DEMO/home" "$DEMO/config"
export HOME="$DEMO/home"
export ORBIT_CONFIG="$DEMO/config/config.toml"
export ORBIT_DB="$DEMO/scratch.db"
./target/debug/orbit init >/dev/null

cat > "$DEMO/capability-demo.json" <<'JSON'
{
  "id": "capability-demo",
  "version": 1,
  "name": "Capability demo",
  "required_capabilities": ["orbit.reference.work-item-note"],
  "work_items": [
    {
      "id": "note-task",
      "name": "Note task",
      "initial_state": "open",
      "states": ["open", "closed"],
      "transitions": [
        {
          "from": "open",
          "to": "closed",
          "actions": [
            {
              "id": "append_note",
              "capability": "orbit.reference.work-item-note",
              "description": "Append a durable note to this blueprint work item",
              "parameters": {"note": ""}
            }
          ]
        }
      ]
    }
  ]
}
JSON

# Refused: the manifest requires a capability that is not installed yet.
./target/debug/orbit blueprint import "$DEMO/capability-demo.json" --json || true

./target/debug/orbit blueprint capability install orbit.reference.work-item-note --json >/dev/null
./target/debug/orbit blueprint import "$DEMO/capability-demo.json" --json >/dev/null

PROJECT=$(./target/debug/orbit project add "Capability demo" --json \
  | python3 -c 'import json,sys; print(json.load(sys.stdin)["project"]["id"])')
./target/debug/orbit blueprint apply "$PROJECT" capability-demo --json >/dev/null
./target/debug/orbit blueprint create-item "$PROJECT" note-1 note-task --json >/dev/null

./target/debug/orbit blueprint capability authorize orbit.reference.work-item-note \
  --project "$PROJECT" --confirm --json >/dev/null
./target/debug/orbit blueprint action offers note-1 --json
./target/debug/orbit blueprint action request note-1 --action append_note --revision 0 \
  --command cmd-note-1 --input '{"note":"first durable note"}' --json
./target/debug/orbit blueprint action execute cmd-note-1 --json
./target/debug/orbit blueprint item note-1 --json
```

The uninstalled import is refused:

```json
{
  "type": "error",
  "code": "usage_error",
  "message": "unavailable capability: orbit.reference.work-item-note"
}
```

After authorization the offer becomes available, and the request is durable intent rather than execution:

```json
{
  "result": {
    "command_id": "cmd-note-1",
    "plan_json": "{\"action_id\":\"append_note\",\"capability\":\"orbit.reference.work-item-note\",\"parameters\":{\"note\":\"first durable note\"},\"command_id\":\"cmd-note-1\"}",
    "state": "pending"
  }
}
```

Execution records the provider and outcome:

```json
{
  "result": {
    "command_id": "cmd-note-1",
    "outcome": "succeeded",
    "provider": "orbit",
    "recorded_at_ms": 1792856580205
  }
}
```

The effect is one note row keyed by the command ID, so replaying `execute` returns the same receipt instead of appending again. Inspect it through the item:

```json
{
  "result": {
    "capability_notes": [
      {
        "command_id": "cmd-note-1",
        "item_id": "note-1",
        "note": "first durable note",
        "tag": null
      }
    ]
  }
}
```

The bundled `blueprints/examples/capability-reference.json` is the same contract with a view attached. It declares only `note` as a default, but the capability's input schema also declares `tag`, so a request may supply either key: `--input '{"note":"...","tag":"review"}'`. A key that neither the manifest defaults nor the capability schema declares is rejected before anything is stored.

## 5. Offline behavior

For a connected client, mutating blueprint commands may enter the canonical SQLite outbox when the server is unreachable. A pending command is **not accepted authoritative state**. Use `orbit blueprint pending` and `orbit blueprint flush` to inspect and submit queued operations. External effects are never executed merely because a command was queued offline.

## 6. Updating existing projects

A published version is immutable and every project is pinned to one version.
Importing a newer manifest therefore never moves a project: the version change is
a separate, previewable, explicitly approved operation.

Never upgrade by editing the manifest in place, by republishing the same version,
or by assuming an existing item will be migrated for you. Use
`orbit blueprint upgrade-preview` first, then `orbit blueprint upgrade` with the
fingerprint that preview printed. `orbit blueprint apply` keeps its original
meaning (change the whole workflow) and performs no schema compatibility check.

### Preview, then apply

The two manifests below are a complete, runnable example. Version 2 adds a state,
a field and a transition, removes one transition, and narrows `effort` from free
text to a choice.

`upgrade-demo-v1.json`:

```json
{
  "id": "upgrade-demo",
  "version": 1,
  "name": "Upgrade demo",
  "work_items": [
    {
      "id": "feature",
      "name": "Feature",
      "initial_state": "backlog",
      "states": ["backlog", "done"],
      "fields": [
        {"id": "effort", "kind": {"type": "text"}, "required": true, "default": "small"},
        {"id": "priority", "kind": {"type": "choice", "options": ["low", "medium"]}, "required": true, "default": "medium"}
      ],
      "transitions": [{"from": "backlog", "to": "done"}]
    }
  ]
}
```

`upgrade-demo-v2.json`:

```json
{
  "id": "upgrade-demo",
  "version": 2,
  "name": "Upgrade demo",
  "work_items": [
    {
      "id": "feature",
      "name": "Feature",
      "initial_state": "backlog",
      "states": ["backlog", "review", "done"],
      "fields": [
        {"id": "effort", "kind": {"type": "choice", "options": ["small", "large"]}, "required": true, "default": "small"},
        {"id": "priority", "kind": {"type": "choice", "options": ["low", "medium"]}, "required": true, "default": "medium"},
        {"id": "owner", "kind": {"type": "text"}}
      ],
      "transitions": [
        {"from": "backlog", "to": "review"},
        {"from": "review", "to": "done"}
      ]
    }
  ]
}
```

Run this in an isolated workspace. Every command below was executed against the
`ORBIT_DB` and `ORBIT_CONFIG` shown; the transcripts are the real output.

```sh
export ORBIT_DB=/tmp/orbit-upgrade-demo/orbit.sqlite
export ORBIT_CONFIG=/tmp/orbit-upgrade-demo/config.toml

orbit --json project add "Upgrade demo"          # note the project id
orbit blueprint import upgrade-demo-v1.json
orbit blueprint apply PROJECT_ID upgrade-demo --version 1
echo '{"effort":"small"}' > properties.json
orbit blueprint create-item PROJECT_ID item-1 feature --properties properties.json
orbit blueprint import upgrade-demo-v2.json
orbit blueprint upgrade-preview PROJECT_ID --id upgrade-demo --version 2
```

The preview is a pure read: it does not bind, publish, migrate or touch a work
item. It reports the current binding (`current`), the `target`, the per-kind
schema delta (`kinds`), view changes (`views`), non-blocking notices
(`warnings`), impossible conditions (`blockers`), the accepted
`project_revision` and a `fingerprint` over all of it.

```json
{
  "type": "authority",
  "result": {
    "blockers": [],
    "current": {
      "blueprint_id": "upgrade-demo",
      "version": 1
    },
    "fingerprint": "c28aab07195276044829c508b012f430798bc9fd5a204e5510f9720efe343f36",
    "kinds": [
      {
        "actions_added": [],
        "actions_removed": [],
        "added": false,
        "fields_added": [
          {
            "default": null,
            "field_type": {
              "kind": "text",
              "options": []
            },
            "id": "owner",
            "required": false
          }
        ],
        "fields_changed": [
          {
            "after": {
              "default": "small",
              "field_type": {
                "kind": "choice",
                "options": [
                  "small",
                  "large"
                ]
              },
              "id": "effort",
              "required": true
            },
            "before": {
              "default": "small",
              "field_type": {
                "kind": "text",
                "options": []
              },
              "id": "effort",
              "required": true
            },
            "id": "effort"
          }
        ],
        "fields_removed": [],
        "initial_state_after": "backlog",
        "initial_state_before": "backlog",
        "kind": "feature",
        "removed": false,
        "required_capabilities_added": [],
        "required_capabilities_removed": [],
        "states_added": [
          "review"
        ],
        "states_removed": [],
        "transitions_added": [
          "backlog->review",
          "review->done"
        ],
        "transitions_changed": [],
        "transitions_removed": [
          "backlog->done"
        ]
      }
    ],
    "project_id": "project-1792889891525-338151-0",
    "project_revision": 4,
    "schema": "orbit.blueprint-upgrade-preview.v1",
    "target": {
      "blueprint_id": "upgrade-demo",
      "version": 2
    },
    "views": {
      "added": [],
      "changed": [],
      "removed": []
    },
    "warnings": [
      {
        "code": "state_added",
        "message": "state review is added to feature",
        "subjects": [
          "feature",
          "review"
        ]
      },
      {
        "code": "field_type_widened",
        "message": "type of feature.effort changes from text to choice; every live value stays valid",
        "subjects": [
          "feature",
          "effort"
        ]
      },
      {
        "code": "field_added",
        "message": "field owner is added to feature",
        "subjects": [
          "feature",
          "owner"
        ]
      },
      {
        "code": "transition_added",
        "message": "transition feature:backlog->review is added",
        "subjects": [
          "feature",
          "backlog->review"
        ]
      },
      {
        "code": "transition_added",
        "message": "transition feature:review->done is added",
        "subjects": [
          "feature",
          "review->done"
        ]
      },
      {
        "code": "transition_removed",
        "message": "transition feature:backlog->done is removed; live items keep their current state",
        "subjects": [
          "feature",
          "backlog->done"
        ]
      }
    ]
  }
}
```

Both fences are enforced by the authority, not only by the CLI. With the
project still at version 1:

```sh
# Missing confirmation: exit 2, nothing written.
orbit --json blueprint upgrade PROJECT_ID --id upgrade-demo --version 2 \
  --fingerprint c28aab07195276044829c508b012f430798bc9fd5a204e5510f9720efe343f36
```

```json
{
  "type": "error",
  "code": "usage_error",
  "message": "workflow upgrade requires explicit --confirm",
  "recoverable": true,
  "hint": "Run `orbit --help` or the subcommand help for valid arguments."
}
```

```sh
# Wrong or stale fingerprint: exit 4, nothing written.
orbit --json blueprint upgrade PROJECT_ID --id upgrade-demo --version 2 \
  --fingerprint 0000000000000000000000000000000000000000000000000000000000000000 \
  --confirm
```

```json
{
  "type": "error",
  "code": "repository_error",
  "message": "blueprint upgrade fingerprint mismatch: recompute the preview and retry (expected c28aab07195276044829c508b012f430798bc9fd5a204e5510f9720efe343f36)",
  "recoverable": true,
  "hint": "Check the workspace path and run storage diagnostics when available."
}
```

The fingerprint covers the current binding, the target id/version, both manifest
digests, the accepted revision, and every live item id/state/revision/properties.
Creating an item, editing a property or accepting another command changes it, so
a preview that was approved before other work cannot be applied afterwards.

### Apply

Apply exactly the report you approved:

```sh
orbit blueprint upgrade PROJECT_ID --id upgrade-demo --version 2 \
  --fingerprint c28aab07195276044829c508b012f430798bc9fd5a204e5510f9720efe343f36 \
  --confirm
```

The authority recomputes the preview inside the same savepoint that writes the
binding and refuses on any mismatch or blocker. The command echoes the approved
report and appends the applied binding (excerpt from the single JSON object;
`--json` prints one object):

```json
    "already_applied": false,
    "binding": {
      "blueprint_id": "upgrade-demo",
      "project_id": "project-1792889891525-338151-0",
      "version": 2
    },
    "previous": {
      "blueprint_id": "upgrade-demo",
      "version": 1
    },
```

The binding is durable and the work items are untouched, in a separate process:

```sh
orbit blueprint binding PROJECT_ID
orbit blueprint item item-1
```

```json
{
  "type": "authority",
  "result": {
    "blueprint_id": "upgrade-demo",
    "project_id": "project-1792889891525-338151-0",
    "version": 2
  }
}
```

```json
{
  "type": "authority",
  "result": {
    "capability_notes": [],
    "item": {
      "id": "item-1",
      "kind": "feature",
      "project_id": "project-1792889891525-338151-0",
      "properties": {
        "effort": "small",
        "priority": "medium"
      },
      "revision": 0,
      "state": "backlog"
    }
  }
}
```

Existing items keep their current state when the target still declares it. An
item whose state disappeared is a blocker instead of a silent remap, and an
upgrade never rewrites item properties or revisions. Repeating the identical
command is idempotent: it returns the same binding with `"already_applied": true`
and writes nothing.

### Blockers

A second project in the same workspace holds an item whose `effort` value is not
one of the new choice options:

```sh
orbit --json project add "Upgrade blocked"          # note the second project id
orbit blueprint apply BLOCKED_PROJECT_ID upgrade-demo --version 1
echo '{"effort":"tiny"}' > blocked-properties.json
orbit blueprint create-item BLOCKED_PROJECT_ID item-2 feature \
  --properties blocked-properties.json
orbit blueprint upgrade-preview BLOCKED_PROJECT_ID --id upgrade-demo --version 2
```

Its preview lists a blocker:

```json
"blockers": [
  {
    "code": "field_type_incompatible",
    "message": "type of feature.effort changes incompatibly from text to choice but 1 live item(s) store a value",
    "subjects": [
      "item-2"
    ]
  }
],
```

`upgrade-preview` still exits `0` (it is a report). `upgrade` refuses with exit
`4` and leaves the binding at version 1:

```json
{
  "type": "error",
  "code": "repository_error",
  "message": "blueprint upgrade blocked: field_type_incompatible: type of feature.effort changes incompatibly from text to choice but 1 live item(s) store a value",
  "recoverable": true,
  "hint": "Check the workspace path and run storage diagnostics when available."
}
```

| Blocker code | Meaning |
| --- | --- |
| `kind_removed_with_live_items` | A live item's work-item kind is not in the target. |
| `state_removed_with_live_items` | A live item is in a state the target kind no longer declares. |
| `field_removed_with_values` | A live item stores a value for a field the target removed. |
| `field_type_incompatible` | An existing value cannot be interpreted by the target type. |
| `field_choice_value_unsupported` | An existing value is no longer one of the target choices. |
| `field_required_without_default` | A new required field has no default and live items lack it. |
| `field_now_required_missing_value` | A field became required without a default. |
| `property_not_in_target_schema` | A live item stores a property neither schema declares. |
| `required_capability_unavailable` | The target requires a capability that is not installed. |
| `target_validation_failed` | The target does not validate against this workspace. |

Warnings never block: `kind_added`, `state_added`, `state_removed`, `field_added`,
`field_removed`, `field_type_widened`, `field_choice_options_trimmed`,
`field_type_changed_without_values`, `field_default_changed`,
`field_now_required`, `transition_added`, `transition_removed`,
`transition_capability_changed`, `transition_guards_changed`, `action_added`,
`action_removed`, `action_changed`, `initial_state_changed`, `view_added`,
`view_removed`, `view_changed` and `target_version_not_newer` (an explicit
downgrade).

Publishing or installing a newer version, including a bundled workflow, never
rebinds a project and never upgrades one automatically. Upgrade commands are
local-authority only in this build: connected mode refuses them instead of
queueing a change whose preview it cannot verify.
