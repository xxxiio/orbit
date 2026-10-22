//! Trusted, compiled-in component registry.
//!
//! Blueprint manifests reference only approved component identifiers. A future
//! extension may add a renderer through this interface, but M28 intentionally
//! ships no dynamic loading, scripting or third-party code execution.
use crate::data::ProjectData;
use orbit_blueprint::presentation::{COMPONENT_TYPES, DATA_SOURCES, UI_ACTIONS, ViewComponent};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RendererSupport {
    Tui,
    FutureClients,
    All,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ComponentDescriptor {
    pub id: &'static str,
    pub label: &'static str,
    pub description: &'static str,
    pub accepted_sources: &'static [&'static str],
    /// Fields the component understands in `columns` / `field` / `sort`.
    pub fields: &'static [&'static str],
    pub user_actions: &'static [&'static str],
    pub renderer: RendererSupport,
}

#[derive(Clone, Debug, Default)]
pub struct ComponentRegistry {
    components: BTreeMap<&'static str, ComponentDescriptor>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidationIssue {
    pub component_id: String,
    pub message: String,
}

impl ComponentRegistry {
    pub fn builtin() -> Self {
        let mut registry = Self::default();
        for descriptor in descriptors() {
            let _ = registry.register(descriptor);
        }
        registry
    }

    pub fn register(&mut self, descriptor: ComponentDescriptor) -> Result<(), ValidationIssue> {
        if !COMPONENT_TYPES.contains(&descriptor.id) {
            return Err(ValidationIssue {
                component_id: descriptor.id.into(),
                message: "component identifier is not part of the shared schema".into(),
            });
        }
        if self.components.contains_key(descriptor.id) {
            return Err(ValidationIssue {
                component_id: descriptor.id.into(),
                message: "duplicate component registration".into(),
            });
        }
        self.components.insert(descriptor.id, descriptor);
        Ok(())
    }

    pub fn get(&self, id: &str) -> Option<&ComponentDescriptor> {
        self.components.get(id)
    }

    pub fn ids(&self) -> impl Iterator<Item = &'static str> + '_ {
        self.components.keys().copied()
    }

    /// Validate a component instance against the trusted registry. Publication
    /// validation is repeated here so unvalidated persisted/local definitions
    /// fail safe instead of crashing a renderer.
    pub fn validate(&self, component: &ViewComponent) -> Vec<ValidationIssue> {
        let mut issues = Vec::new();
        let Some(descriptor) = self.get(&component.component_type) else {
            issues.push(ValidationIssue {
                component_id: component.id.clone(),
                message: format!(
                    "component type '{}' is not registered; install the extension that provides it",
                    component.component_type
                ),
            });
            return issues;
        };
        if let Some(source) = component.effective_source() {
            if !DATA_SOURCES.contains(&source) {
                issues.push(ValidationIssue {
                    component_id: component.id.clone(),
                    message: format!("data source '{source}' is not supported"),
                });
            } else if !descriptor.accepted_sources.contains(&source) {
                issues.push(ValidationIssue {
                    component_id: component.id.clone(),
                    message: format!(
                        "component '{}' does not accept source '{source}'",
                        component.component_type
                    ),
                });
            }
        } else if component.component_type != "text" {
            issues.push(ValidationIssue {
                component_id: component.id.clone(),
                message: format!(
                    "component '{}' requires a data source",
                    component.component_type
                ),
            });
        }
        let dynamic_work_item_fields = component.effective_source() == Some("work_items");
        for field in component
            .columns
            .iter()
            .chain(component.field.iter())
            .chain(component.sort.iter())
            .chain(component.group_by.iter())
        {
            if !dynamic_work_item_fields
                && !descriptor.fields.is_empty()
                && !descriptor.fields.contains(&field.as_str())
            {
                issues.push(ValidationIssue {
                    component_id: component.id.clone(),
                    message: format!(
                        "field '{field}' is not available for component '{}'",
                        component.component_type
                    ),
                });
            }
        }
        if let Some(action) = component
            .actions
            .iter()
            .find(|action| !UI_ACTIONS.contains(&action.as_str()))
        {
            issues.push(ValidationIssue {
                component_id: component.id.clone(),
                message: format!("action '{action}' is not registered"),
            });
        }
        issues
    }

    /// A testable guarantee that the renderer registry and shared schema cannot
    /// silently drift apart.
    pub fn missing_schema_components(&self) -> Vec<&'static str> {
        COMPONENT_TYPES
            .iter()
            .copied()
            .filter(|id| !self.components.contains_key(id))
            .collect()
    }

    pub fn unknown_registered_components(&self) -> Vec<&'static str> {
        self.components
            .keys()
            .copied()
            .filter(|id| !COMPONENT_TYPES.contains(id))
            .collect()
    }
}

fn descriptors() -> Vec<ComponentDescriptor> {
    vec![
        ComponentDescriptor {
            id: "text",
            label: "Text",
            description: "Heading and wrapped explanatory text.",
            accepted_sources: &[],
            fields: &[],
            user_actions: &[],
            renderer: RendererSupport::All,
        },
        ComponentDescriptor {
            id: "list",
            label: "List",
            description: "Compact selectable collection with status and metadata.",
            accepted_sources: &[
                "tasks",
                "work_items",
                "milestones",
                "knowledge",
                "habits",
                "metrics",
                "agent_jobs",
                "schedule",
            ],
            fields: &[
                "title", "status", "state", "kind", "name", "priority", "deadline", "due",
                "due_date", "updated", "created", "id",
            ],
            user_actions: &["open"],
            renderer: RendererSupport::All,
        },
        ComponentDescriptor {
            id: "table",
            label: "Table",
            description: "Typed columns with selection and scrolling.",
            accepted_sources: &[
                "tasks",
                "work_items",
                "milestones",
                "knowledge",
                "habits",
                "metrics",
                "agent_jobs",
                "schedule",
            ],
            fields: &[
                "id",
                "title",
                "name",
                "status",
                "state",
                "kind",
                "priority",
                "project",
                "deadline",
                "due",
                "due_date",
                "estimate",
                "readiness",
                "acceptance",
                "updated",
            ],
            user_actions: &["open"],
            renderer: RendererSupport::All,
        },
        ComponentDescriptor {
            id: "metric",
            label: "Metric",
            description: "A bounded aggregate over a supported project source.",
            accepted_sources: &[
                "project",
                "tasks",
                "work_items",
                "milestones",
                "knowledge",
                "habits",
                "metrics",
                "agent_jobs",
                "schedule",
            ],
            fields: &[
                "estimate",
                "deadline",
                "value",
                "latest",
                "target",
                "baseline",
                "occurrences",
                "count",
            ],
            user_actions: &[],
            renderer: RendererSupport::All,
        },
        ComponentDescriptor {
            id: "detail",
            label: "Detail",
            description: "Typed properties and explanatory notes.",
            accepted_sources: &["project", "tasks", "work_items"],
            fields: &[
                "id",
                "title",
                "name",
                "status",
                "state",
                "kind",
                "mode",
                "objective",
                "criteria",
                "health",
                "evidence",
                "priority",
                "deadline",
                "due",
                "due_date",
                "estimate",
                "acceptance",
            ],
            user_actions: &["open"],
            renderer: RendererSupport::All,
        },
        ComponentDescriptor {
            id: "kanban",
            label: "Kanban board",
            description: "Work grouped by workflow state with authorized transitions.",
            accepted_sources: &["tasks", "work_items"],
            fields: &["status", "state", "kind", "priority", "title", "name"],
            user_actions: &["open", "blueprint_transition", "set_task_status"],
            renderer: RendererSupport::All,
        },
        ComponentDescriptor {
            id: "task_list",
            label: "Task list",
            description: "Tasks with status, readiness, dependencies and estimates.",
            accepted_sources: &["tasks"],
            fields: &[
                "status",
                "readiness",
                "deadline",
                "due",
                "estimate",
                "title",
            ],
            user_actions: &["open", "create_task", "set_task_status", "complete_task"],
            renderer: RendererSupport::All,
        },
        ComponentDescriptor {
            id: "timeline",
            label: "Timeline",
            description: "Milestones or scheduled blocks in chronological order.",
            accepted_sources: &["milestones", "schedule"],
            fields: &["status", "start", "end", "name", "title", "target"],
            user_actions: &["open", "preview_plan"],
            renderer: RendererSupport::All,
        },
        ComponentDescriptor {
            id: "habit_tracker",
            label: "Habit tracker",
            description: "Active habits with recent occurrence counts.",
            accepted_sources: &["habits"],
            fields: &["cadence", "name", "active", "occurrences", "last"],
            user_actions: &["create_task", "record_habit"],
            renderer: RendererSupport::All,
        },
        ComponentDescriptor {
            id: "metric_history",
            label: "Metric history",
            description: "Latest values and an ASCII-safe observation history.",
            accepted_sources: &["metrics"],
            fields: &["value", "latest", "target", "baseline", "direction"],
            user_actions: &["record_metric", "set_metric_target"],
            renderer: RendererSupport::All,
        },
        ComponentDescriptor {
            id: "knowledge_refs",
            label: "Knowledge references",
            description: "Project-bound research notes and evidence.",
            accepted_sources: &["knowledge"],
            fields: &["kind", "status", "title", "updated"],
            user_actions: &["open_knowledge"],
            renderer: RendererSupport::All,
        },
        ComponentDescriptor {
            id: "agent_jobs",
            label: "Agent jobs",
            description: "Execution state for explicitly authorized agent work.",
            accepted_sources: &["agent_jobs"],
            fields: &["state", "agent", "task", "updated"],
            user_actions: &["open", "review_agent"],
            renderer: RendererSupport::All,
        },
        ComponentDescriptor {
            id: "milestone_progress",
            label: "Milestone progress",
            description: "Milestone completion derived from associated tasks.",
            accepted_sources: &["milestones"],
            fields: &["status", "name"],
            user_actions: &["open"],
            renderer: RendererSupport::All,
        },
    ]
}

/// Fields exposed by a source for filter/sort/table use. This is intentionally
/// a closed allowlist, not arbitrary property access.
pub fn source_fields(source: &str) -> &'static [&'static str] {
    match source {
        "project" => &[
            "id",
            "name",
            "status",
            "mode",
            "objective",
            "completion_ready",
        ],
        "tasks" => &[
            "id",
            "title",
            "status",
            "project",
            "milestone",
            "estimate",
            "deadline",
            "due",
            "due_date",
            "readiness",
            "actionable",
            "priority",
            "acceptance",
        ],
        "work_items" => &[
            "id", "title", "kind", "state", "revision", "priority", "question", "activity",
            "topic", "subject", "method",
        ],
        "milestones" => &["id", "name", "status", "progress", "total", "done"],
        "knowledge" => &["id", "title", "kind", "status", "updated", "provenance"],
        "habits" => &["id", "name", "cadence", "active", "occurrences", "last"],
        "metrics" => &["id", "latest", "target", "baseline", "direction", "value"],
        "agent_jobs" => &["id", "state", "agent", "task", "updated", "machine"],
        "schedule" => &["id", "title", "start", "end", "target", "flexibility"],
        _ => &[],
    }
}

pub fn source_known(source: &str) -> bool {
    DATA_SOURCES.contains(&source)
}

/// Numeric fields accepted by metric aggregates, with a bounded parser.
pub fn numeric_field(
    project: &ProjectData,
    source: &str,
    field: &str,
    row_id: &str,
) -> Option<i64> {
    match (source, field) {
        ("tasks", "estimate") => project
            .tasks
            .iter()
            .find(|task| task.id == row_id)
            .and_then(|task| task.estimated_minutes)
            .map(i64::from),
        ("metrics", "latest") | ("metrics", "value") => project
            .metrics
            .iter()
            .find(|metric| metric.id == row_id)
            .and_then(|metric| metric.latest),
        ("metrics", "target") => project
            .metrics
            .iter()
            .find(|metric| metric.id == row_id)
            .map(|metric| metric.target),
        ("metrics", "baseline") => project
            .metrics
            .iter()
            .find(|metric| metric.id == row_id)
            .map(|metric| metric.baseline),
        ("work_items", field) => project
            .work_items
            .iter()
            .find(|item| item.id == row_id)
            .and_then(|item| {
                item.properties
                    .iter()
                    .find(|(key, _)| key == field)
                    .and_then(|(_, value)| value.parse::<i64>().ok())
            }),
        _ => None,
    }
}

pub fn declared_component_types() -> BTreeSet<&'static str> {
    COMPONENT_TYPES.iter().copied().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn component(id: &str, component_type: &str, source: &str) -> ViewComponent {
        ViewComponent {
            id: id.into(),
            component_type: component_type.into(),
            title: Some(id.into()),
            source: Some(source.into()),
            aggregate: Some("count".into()),
            field: None,
            group_by: None,
            filter: BTreeMap::new(),
            sort: None,
            descending: false,
            limit: None,
            columns: Vec::new(),
            text: None,
            actions: Vec::new(),
            options: BTreeMap::new(),
        }
    }

    #[test]
    fn schema_and_registry_cannot_drift() {
        let registry = ComponentRegistry::builtin();
        assert!(registry.missing_schema_components().is_empty());
        assert!(registry.unknown_registered_components().is_empty());
        for descriptor in descriptors() {
            for action in descriptor.user_actions {
                assert!(
                    UI_ACTIONS.contains(action),
                    "{} declares unregistered action {action}",
                    descriptor.id
                );
            }
        }
    }

    #[test]
    fn unknown_components_and_incompatible_sources_fail_closed() {
        let registry = ComponentRegistry::builtin();
        let mut unknown = component("mystery", "chart", "tasks");
        assert!(!registry.validate(&unknown).is_empty());
        unknown.component_type = "mystery".into();
        assert!(!registry.validate(&unknown).is_empty());

        let timeline = component("timeline", "timeline", "tasks");
        assert!(!registry.validate(&timeline).is_empty());

        let metric = component("count", "metric", "project");
        assert!(registry.validate(&metric).is_empty());
    }

    #[test]
    fn registration_is_explicit_and_duplicate_free() {
        let mut registry = ComponentRegistry::builtin();
        assert!(
            registry
                .register(ComponentDescriptor {
                    id: "list",
                    label: "List",
                    description: "",
                    accepted_sources: &["tasks"],
                    fields: &[],
                    user_actions: &[],
                    renderer: RendererSupport::All,
                })
                .is_err()
        );
        assert!(
            registry
                .register(ComponentDescriptor {
                    id: "custom",
                    label: "Custom",
                    description: "",
                    accepted_sources: &[],
                    fields: &[],
                    user_actions: &[],
                    renderer: RendererSupport::Tui,
                })
                .is_err()
        );
    }
}
