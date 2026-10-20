# Blueprint-defined project views

A project blueprint can declare presentation views in addition to work-item
types, fields, transitions and templates. The view declaration says *what* to
show; Orbit's trusted renderer registry decides *how* to render it.

The `views` section is optional. Manifests without it remain valid and open
through generated default views.

## Minimal overview

```yaml
id: development
version: 3
name: Development
work_items:
  - id: feature
    name: Feature
    initial_state: backlog
    states: [backlog, ready, done]
    transitions:
      - from: backlog
        to: ready
views:
  - id: overview
    title: Overview
    layout:
      type: grid
      columns: 2
    components:
      - id: ready_tasks
        type: metric
        title: Ready tasks
        source: tasks
        aggregate: count
        filter:
          status: ready

      - id: recent_tasks
        type: task_list
        title: Tasks
        source: tasks
        limit: 8
```

The same structure is accepted in JSON. Bundled manifests are JSON because
that is the format already used by the CLI and server contracts.

## Layout

| `layout.type` | Behaviour |
|---|---|
| `vertical` | Components stack top to bottom |
| `horizontal` | Components place side by side |
| `grid` | Components fill `columns` columns, equal rows |

`columns` must be between 1 and 4. A renderer may collapse a multi-component
view to one focused component on a narrow terminal rather than truncating it
silently.

## Components

| Type | Purpose | Sources |
|---|---|---|
| `text` | Heading and explanatory text | none |
| `list` | Selectable collection | project data sources |
| `table` | Typed columns with scrolling | project data sources |
| `metric` | Count/sum/average/min/max | project data sources |
| `detail` | Typed properties | `project`, `tasks`, `work_items` |
| `kanban` | Work grouped by workflow state | `tasks`, `work_items` |
| `task_list` | Tasks with readiness and dependencies | `tasks` |
| `timeline` | Milestones or scheduled blocks | `milestones`, `schedule` |
| `habit_tracker` | Active habits and recent occurrences | `habits` |
| `metric_history` | Latest values and observation history | `metrics` |
| `knowledge_refs` | Knowledge notes | `knowledge` |
| `agent_jobs` | Authorized agent execution state | `agent_jobs` |
| `milestone_progress` | Task-derived milestone progress | `milestones` |

Supported sources are `project`, `tasks`, `work_items`, `milestones`,
`knowledge`, `habits`, `metrics`, `agent_jobs` and `schedule`. A source is not a
SQL interface: it is a registered, read-only provider over existing application
contracts. Blueprint manifests cannot request arbitrary files, shell commands,
SQL, scripts or executable code.

## Common component fields

| Field | Meaning |
|---|---|
| `id` | Stable component ID, unique within the view |
| `type` | Registered component type |
| `title` | Display title; defaults to the ID |
| `source` | Registered data source |
| `filter` | Equality filters over source fields |
| `sort` | Source field name |
| `descending` | Reverse the sort |
| `limit` | 1–200, bounded rendering/preview |
| `columns` | Table/list fields |
| `aggregate` | `count`, `sum`, `average`, `min`, `max` for metrics |
| `field` | Numeric field for non-count aggregates |
| `group_by` | `status`, `state`, `kind`, `readiness`, `agent` or `project` |
| `text` | Static text for `text` components |
| `actions` | Presentation hints only; never authority |
| `options` | Small validated option bag (`empty_text`, `caption`, etc.) |

A metric with `count` needs no field. `sum`, `average`, `min` and `max` need a
numeric `field` such as `estimate` for tasks or `latest` for metrics.

Filters are exact, case-insensitive matches. Unknown filters fail closed instead
of broadening a query. A view cannot request a cross-project aggregate; project
views resolve against the selected project. Global Home/Inbox/Agents screens
are host-generated aggregations, not blueprint-controlled queries.

## Contextual actions

Components may list actions as presentation metadata. A declaration does not
enable an action and does not grant a capability. The renderer offers only
actions that the application can authorize for the selected item. For example,
a task action menu is derived from the current task status and the domain
workflow; a blueprint cannot make an inapplicable transition appear.

## Validation

Publication and installation validate:

- view IDs and component IDs are valid and unique;
- the view has a title and 1–4 layout columns;
- component types, sources, options and action IDs are registered;
- required component fields are present and limits are bounded;
- text components have text or a title;
- timelines use milestone/schedule sources;
- detail components use project/task/work-item sources.

The TUI repeats registry validation when it resolves a view, so a stored or
locally edited definition that bypassed publication fails safe with a missing
component panel instead of a crash.

## Legacy fallback

A project bound to a blueprint with no `views` gets a generated:

- **Overview** with live task, milestone and agent metrics, project details,
  task list and milestone progress;
- **Work** with a task workflow board and blueprint work items when present;
- **Knowledge**, **Habits & metrics** and **Activity** views when that data
  exists.

Generated views use real application data and are marked `generated default` in
the UI. They do not require a manual migration to open the TUI.

## Versioning and pinning

Blueprint versions are immutable. Publishing a new version does not rebind a
project, and binding is an explicit command. Projects keep their pinned version
and its view definitions until rebound. Existing work items are not migrated or
reinterpreted by a view change.

See [Custom workflows](custom-blueprints.md) for the authoring workflow and the
[UI view reference](../reference/ui-views.md) for the exact field list.
