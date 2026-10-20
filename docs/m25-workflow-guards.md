# M25: Declarative workflow guards and prerequisite relationships

Blueprint transitions may specify optional `guards`. A `property_equals` guard matches a declared property against a schema-valid JSON value; `property_present` requires a declared populated property. `all_prerequisites_in_state` requires every prerequisite work item to have the selected state. Guards are ANDed. An empty guard list preserves the earlier transition behavior.

Example transition fragment:

```json
{
  "from": "review", "to": "done", "required_capability": null,
  "guards": [
    {"type": "property_equals", "field": "approved", "value": true},
    {"type": "all_prerequisites_in_state", "state": "done"}
  ]
}
```

The property `approved` must have been declared as a Boolean field of the same work-item kind. The target state must be declared in that kind. A transition guard is a deterministic predicate, not an authorization grant and not an instruction to launch an agent.

Prerequisites are stored in `orbit_blueprint_item_dependencies` by the authoritative SQLite store. New relationships must be within the same project, non-self-referential and acyclic. The application exposes `add_project_work_item_prerequisite` and `project_work_item_prerequisites`. Relationship predicates are evaluated against persisted prerequisite states at transition time. Existing tasks and task prerequisite tables are not modified.

Limitations: no declarative automations, plugin action execution, blueprint templates or YAML import yet. The new relationship enforcement is an authority-local feature; connected API exposure and end-to-end validation are separate milestones. Broad compilation and integration acceptance remain M15 work.
