# UI view reference

This reference describes the shared, renderer-neutral view schema used by
`orbit-blueprint` and `orbit-ui`. The schema is consumed by the terminal
renderer today and is suitable for a future Flutter renderer.

Orbit calls a project manifest a **workflow** in the TUI and in prose. The
`orbit-blueprint` crate, persisted identifiers, the `orbit blueprint` command
group and `blueprint.*` MCP tools keep the older name for compatibility; they
describe the same schema.

## Location

| Layer | Path | Responsibility |
|---|---|---|
| Declarative schema | `crates/orbit-blueprint/src/presentation.rs` | Views, layouts, components, validation |
| Application data | `crates/orbit-client/src/view_data.rs` | Local project/workspace snapshots from client services |
| Connected adapter | `crates/orbit-client/src/unified.rs`, `crates/orbit-ui/src/snapshot.rs` | Accepted-snapshot mapping to the same contracts |
| Shared resolution | `crates/orbit-ui/src` | Registry, bindings, typed component data, actions |
| Terminal renderer | `crates/orbit-tui/src` | Layout, focus, input, modals, styling |
| Terminal theme | `crates/orbit-tui/src/theme.rs` | Color tokens and monochrome fallback |

The shared schema has no Ratatui dependency. Renderers receive
`orbit_ui::ResolvedView` values and must not query storage directly.

## Workflow views

A bundled manifest declares `views`. The default workflow for new projects is
**Basic** (version 1), which declares `overview`, `tasks`, `board` and
`timeline` and no specialized work items. Development, Research, Learning and
Habits are version 2 and declare additional views.

```json
{
  "views": [
    {
      "id": "overview",
      "title": "Overview",
      "description": "Project status",
      "layout": { "type": "grid", "columns": 2 },
      "components": [
        {
          "id": "ready",
          "type": "metric",
          "title": "Ready tasks",
          "source": "tasks",
          "aggregate": "count",
          "filter": { "status": "ready" }
        }
      ]
    }
  ]
}
```

All structs deny unknown fields. `views` defaults to an empty list. View and
component IDs use lower-case identifiers with digits, `_`, `.` or `-`, up to 128
bytes.

### View object

| Field | Type | Required | Default |
|---|---|---|---|
| `id` | string | yes | — |
| `title` | string | yes | — |
| `description` | string | no | empty |
| `layout` | object | no | vertical, 1 column |
| `components` | array | no | empty |

### Layout object

| Field | Type | Values |
|---|---|---|
| `type` | string | `vertical`, `horizontal`, `grid` |
| `columns` | integer | 1–4 |

### Component object

| Field | Type | Notes |
|---|---|---|
| `id` | string | unique within view |
| `type` | string | registered component type |
| `title` | string? | display title |
| `source` | string? | registered source; some types imply a default |
| `aggregate` | string? | metric only |
| `field` | string? | numeric field for metric aggregates |
| `group_by` | string? | kanban grouping |
| `filter` | object | string-to-string equality filters |
| `sort` | string? | source field |
| `descending` | bool | default false |
| `limit` | integer? | 1–200 |
| `columns` | string[] | table/list fields |
| `text` | string? | text component body |
| `actions` | string[] | presentation hints only |
| `options` | object | validated component options |

## Component types

| ID | Required source | Optional source default | Selection |
|---|---|---|---|
| `text` | none | none | none |
| `list` | yes | none | rows |
| `table` | yes | none | rows |
| `metric` | yes | none | none |
| `detail` | no | `project` | first item/project |
| `kanban` | no | `tasks` | cards |
| `task_list` | no | `tasks` | tasks |
| `timeline` | no | `milestones` | events |
| `habit_tracker` | yes | `habits` | habits |
| `metric_history` | yes | `metrics` | metrics |
| `knowledge_refs` | yes | `knowledge` | notes |
| `agent_jobs` | yes | `agent_jobs` | jobs |
| `milestone_progress` | yes | `milestones` | milestones |

The `detail` component stays a workspace component: it paints one item's fields
inside the view that declares it. It is not the TUI's item inspector, which is a
separate overlay opened with `Enter`/`D` and driven by the current selection
(see [TUI workspace](../guides/tui-workspace.md#the-item-inspector)).

## Data sources

The source list is closed. Each component declares which of these registered,
read-only providers it accepts; manifests cannot request arbitrary queries.

| Source | Fields available for filters, sorting and tables |
|---|---|
| `project` | `id`, `name`, `status`, `mode`, `objective`, `completion_ready`, `criteria`, `health`, `evidence` |
| `tasks` | `id`, `title`, `status`, `project`, `milestone`, `estimate`, `deadline`, `readiness`, `actionable`, `priority` |
| `work_items` | `id`, `title`, `kind`, `state`, `revision`, plus declared work-item properties |
| `milestones` | `id`, `name`, `status`, `progress`, `total`, `done` |
| `knowledge` | `id`, `title`, `kind`, `status`, `updated`, `provenance` |
| `habits` | `id`, `name`, `cadence`, `active`, `occurrences`, `last` |
| `metrics` | `id`, `latest`, `target`, `baseline`, `direction`, `value` |
| `agent_jobs` | `id`, `state`, `agent`, `task`, `updated`, `machine` |
| `schedule` | `id`, `title`, `start`, `end`, `target`, `flexibility` |

`priority` is a core task field (`low`, `medium`, `high`, `urgent`). The task
source exposes it for sorting, filtering and tables. Descriptions, labels and
acceptance criteria are not view sources; open the task to read them.

## Metric aggregates

| Aggregate | Behaviour |
|---|---|
| `count` | Number of filtered rows |
| `sum` | Sum of a numeric `field` |
| `average` | Integer average of available values |
| `min` / `max` | Smallest/largest available value |

Missing or non-numeric values are skipped for aggregates other than count. A
metric with no values renders `—`.

## Resolved state

Each component resolves to:

- `LoadStatus`: `Loading`, `Ready`, `Empty`, `Error(message)`, `Stale(message)`
  or `Missing(message)`.
- A typed `ComponentBody`: text, list, table, metric, detail, kanban, timeline,
  habit tracker, metric history, knowledge references, agent jobs, milestone
  progress or missing-component explanation.
- A `SelectionItem` list in visual order. The item inspector and the action menu
  use this order, so kanban/timeline ordering cannot point at a different row
  than the highlighted one.

A missing component or invalid binding affects only that component. The rest of
the view remains renderable and the UI explains the required capability.

## Actions

The action catalogue lives in `orbit_ui::actions::ACTIONS`. Workflow action
strings are validated against `UI_ACTIONS`. Contextual availability is computed
from the selected item:

| Item | Offered actions |
|---|---|
| Task | Open, allowed status transitions, dependencies |
| Work item | Open, allowed workflow transitions, dependencies |
| Habit | Record occurrence |
| Metric | Record observation, set target |
| Knowledge | Open note |
| Agent job | Inspect; review is disabled when no accepted review service is available |
| Project | New task, new work item, preview plan, bind evidence |

The renderer never invokes a shell, SQL, script or executable named by a
manifest. Every mutation is a typed `OrbitClient` call.

## Version compatibility

- Older manifests without `views` parse unchanged.
- Existing pinned versions keep their definitions and fall back to generated
  views.
- New bundled manifests are Basic v1 plus Development, Research, Learning and
  Habits v2; Basic is the default for new projects.
- Publishing a version is immutable; applying or changing a workflow is
  explicit.
- Unsupported component/source/action IDs fail publication; a runtime registry
  miss degrades to a missing-component panel.

## Related documentation

- [Workflow-defined views](../guides/blueprint-views.md)
- [TUI workspace](../guides/tui-workspace.md)
- [Task details and attachments](../guides/task-details-and-attachments.md)
- [UI component authoring](../developers/ui-components.md)
