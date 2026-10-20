# UI view reference

This reference describes the shared, renderer-neutral view schema used by
`orbit-blueprint` and `orbit-ui`. The schema is consumed by the terminal
renderer today and is suitable for a future Flutter renderer.

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

## Blueprint views

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

`priority` is not a core task field. The task source exposes the current status
as a coarse sorting value; typed priority belongs to a blueprint work-item field.

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
- A `SelectionItem` list in visual order. The detail panel and action menu use
  this order, so kanban/timeline ordering cannot point at a different row than
  the highlighted one.

A missing component or invalid binding affects only that component. The rest of
the view remains renderable and the UI explains the required capability.

## Actions

The action catalogue lives in `orbit_ui::actions::ACTIONS`. Blueprint action
strings are validated against `UI_ACTIONS`. Contextual availability is computed
from the selected item:

| Item | Offered actions |
|---|---|
| Task | Open, allowed status transitions, dependencies |
| Work item | Open, allowed blueprint transitions, dependencies |
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
- New bundled manifests use version 2 and demonstrate views.
- Publishing a version is immutable; rebinding is explicit.
- Unsupported component/source/action IDs fail publication; a runtime registry
  miss degrades to a missing-component panel.

## Related documentation

- [Blueprint-defined views](../guides/blueprint-views.md)
- [TUI workspace](../guides/tui-workspace.md)
- [UI component authoring](../developers/ui-components.md)
