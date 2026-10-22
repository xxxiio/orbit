//! Framework-neutral presentation definitions for blueprint-driven project views.
//!
//! This module deliberately has no terminal, GUI, storage, or execution
//! dependency. A renderer consumes the declarative shape; it must not infer
//! permissions, capabilities, or data access from a declaration.
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

pub const COMPONENT_TYPES: &[&str] = &[
    "text",
    "list",
    "table",
    "metric",
    "detail",
    "kanban",
    "task_list",
    "timeline",
    "habit_tracker",
    "metric_history",
    "knowledge_refs",
    "agent_jobs",
    "milestone_progress",
];

pub const DATA_SOURCES: &[&str] = &[
    "project",
    "tasks",
    "work_items",
    "milestones",
    "knowledge",
    "habits",
    "metrics",
    "agent_jobs",
    "schedule",
];

/// Actions a view may *mention*. This is presentation metadata only: the
/// renderer and the trusted application services decide what is available.
pub const UI_ACTIONS: &[&str] = &[
    "open",
    "create_task",
    "create_work_item",
    "set_task_status",
    "complete_task",
    "blueprint_transition",
    "record_habit",
    "open_knowledge",
    "inspect_dependencies",
    "review_agent",
    "inspect_agent",
    "preview_plan",
    "apply_plan",
    "record_metric",
    "set_metric_target",
    "bind_evidence",
    "inspect_evidence",
    "unbind_evidence",
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BlueprintView {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub layout: ViewLayout,
    #[serde(default)]
    pub components: Vec<ViewComponent>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ViewLayout {
    #[serde(rename = "type", default)]
    pub kind: ViewLayoutKind,
    #[serde(default = "one")]
    pub columns: u16,
}
impl Default for ViewLayout {
    fn default() -> Self {
        Self {
            kind: ViewLayoutKind::Vertical,
            columns: 1,
        }
    }
}
const fn one() -> u16 {
    1
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ViewLayoutKind {
    #[default]
    Vertical,
    Horizontal,
    Grid,
}

/// One declared component instance. Common fields cover the built-in
/// component vocabulary; `options` is a bounded, schema-validated extension
/// bag for options that only a specific component understands.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ViewComponent {
    pub id: String,
    #[serde(rename = "type")]
    pub component_type: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub aggregate: Option<String>,
    #[serde(default)]
    pub field: Option<String>,
    #[serde(default)]
    pub group_by: Option<String>,
    #[serde(default)]
    pub filter: BTreeMap<String, String>,
    #[serde(default)]
    pub sort: Option<String>,
    #[serde(default)]
    pub descending: bool,
    #[serde(default)]
    pub limit: Option<u32>,
    #[serde(default)]
    pub columns: Vec<String>,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub actions: Vec<String>,
    #[serde(default)]
    pub options: BTreeMap<String, Value>,
}

impl ViewComponent {
    pub fn display_title(&self) -> &str {
        self.title.as_deref().unwrap_or(&self.id)
    }
    pub fn effective_source(&self) -> Option<&str> {
        self.source
            .as_deref()
            .or(match self.component_type.as_str() {
                "task_list" => Some("tasks"),
                "kanban" => Some("tasks"),
                "timeline" => Some("milestones"),
                "habit_tracker" => Some("habits"),
                "metric_history" => Some("metrics"),
                "knowledge_refs" => Some("knowledge"),
                "agent_jobs" => Some("agent_jobs"),
                "milestone_progress" => Some("milestones"),
                "detail" => Some("project"),
                _ => None,
            })
    }
}

fn invalid(message: impl Into<String>) -> super::BlueprintError {
    super::BlueprintError::Invalid(message.into())
}

fn ident(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase()
                || byte.is_ascii_digit()
                || byte == b'_'
                || byte == b'.'
                || byte == b'-'
        })
}

fn accepted_sources(component_type: &str) -> &'static [&'static str] {
    match component_type {
        "text" => &[],
        "list" | "table" | "metric" => &[
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
        "detail" => &["project", "tasks", "work_items"],
        "kanban" => &["tasks", "work_items"],
        "task_list" => &["tasks"],
        "timeline" => &["milestones", "schedule"],
        "habit_tracker" => &["habits"],
        "metric_history" => &["metrics"],
        "knowledge_refs" => &["knowledge"],
        "agent_jobs" => &["agent_jobs"],
        "milestone_progress" => &["milestones"],
        _ => &[],
    }
}

fn known_options(component_type: &str) -> &'static [&'static str] {
    match component_type {
        "list" => &["empty_text", "show_status", "show_priority"],
        "table" => &["empty_text"],
        "metric" => &["caption", "unit"],
        "kanban" => &["empty_text", "column_width"],
        "task_list" => &["empty_text", "show_dependencies"],
        "timeline" => &["empty_text"],
        "habit_tracker" => &["empty_text", "days"],
        "metric_history" => &["empty_text", "window"],
        "knowledge_refs" => &["empty_text"],
        "agent_jobs" => &["empty_text"],
        "milestone_progress" => &["empty_text"],
        "detail" => &["empty_text"],
        "text" => &[],
        _ => &[],
    }
}

fn validate_component(component: &ViewComponent) -> Result<(), super::BlueprintError> {
    if !ident(&component.id) {
        return Err(invalid(format!(
            "invalid view component identifier: {}",
            component.id
        )));
    }
    if !COMPONENT_TYPES.contains(&component.component_type.as_str()) {
        return Err(invalid(format!(
            "unsupported view component type: {}",
            component.component_type
        )));
    }
    let source = component.effective_source();
    if let Some(source) = source
        && !DATA_SOURCES.contains(&source)
    {
        return Err(invalid(format!("unsupported view data source: {source}")));
    }
    if let Some(source) = source
        && !accepted_sources(&component.component_type).contains(&source)
    {
        return Err(invalid(format!(
            "component type {} does not accept source {source}",
            component.component_type
        )));
    }
    match component.component_type.as_str() {
        "text" => {
            if component.text.as_deref().is_none_or(str::is_empty)
                && component.title.as_deref().is_none_or(str::is_empty)
            {
                return Err(invalid(format!(
                    "text component {} needs text or title",
                    component.id
                )));
            }
        }
        "metric" => {
            if source.is_none() {
                return Err(invalid(format!(
                    "metric component {} needs a source",
                    component.id
                )));
            }
            if let Some(aggregate) = &component.aggregate
                && !matches!(
                    aggregate.as_str(),
                    "count" | "sum" | "average" | "min" | "max"
                )
            {
                return Err(invalid(format!(
                    "unsupported metric aggregate: {aggregate}"
                )));
            }
            if matches!(
                component.aggregate.as_deref(),
                Some("sum" | "average" | "min" | "max")
            ) && component.field.as_deref().is_none_or(str::is_empty)
            {
                return Err(invalid(format!(
                    "metric component {} needs a numeric field",
                    component.id
                )));
            }
        }
        "kanban" => {
            if source.is_none() {
                return Err(invalid(format!(
                    "kanban component {} needs a source",
                    component.id
                )));
            }
            if let Some(group_by) = &component.group_by
                && !ident(group_by)
            {
                return Err(invalid(format!("invalid group_by: {group_by}")));
            }
        }
        "task_list" if source != Some("tasks") => {
            return Err(invalid(format!(
                "task_list component {} needs the tasks source",
                component.id
            )));
        }
        "timeline" => {
            if !matches!(source, Some("milestones" | "schedule")) {
                return Err(invalid(format!(
                    "timeline component {} supports milestones or schedule",
                    component.id
                )));
            }
        }
        "detail" => {
            if !matches!(source, Some("project" | "tasks" | "work_items")) {
                return Err(invalid(format!(
                    "detail component {} supports project, tasks or work_items",
                    component.id
                )));
            }
        }
        "habit_tracker" if source != Some("habits") => {
            return Err(invalid(format!(
                "habit_tracker component {} needs the habits source",
                component.id
            )));
        }
        "metric_history" if source != Some("metrics") => {
            return Err(invalid(format!(
                "metric_history component {} needs the metrics source",
                component.id
            )));
        }
        "knowledge_refs" if source != Some("knowledge") => {
            return Err(invalid(format!(
                "knowledge_refs component {} needs the knowledge source",
                component.id
            )));
        }
        "agent_jobs" if source != Some("agent_jobs") => {
            return Err(invalid(format!(
                "agent_jobs component {} needs the agent_jobs source",
                component.id
            )));
        }
        "milestone_progress" if source != Some("milestones") => {
            return Err(invalid(format!(
                "milestone_progress component {} needs the milestones source",
                component.id
            )));
        }
        _ => {}
    }
    if component.filter.keys().any(|key| !ident(key)) {
        return Err(invalid(format!(
            "invalid filter key in component {}",
            component.id
        )));
    }
    if component
        .filter
        .values()
        .any(|value| value.len() > 256 || value.chars().any(char::is_control))
    {
        return Err(invalid(format!(
            "invalid filter value in component {}",
            component.id
        )));
    }
    if let Some(sort) = &component.sort
        && !ident(sort)
    {
        return Err(invalid(format!("invalid sort field: {sort}")));
    }
    if let Some(limit) = component.limit
        && !(1..=200).contains(&limit)
    {
        return Err(invalid(format!(
            "component {} limit must be between 1 and 200",
            component.id
        )));
    }
    if !component.columns.is_empty()
        && !matches!(
            component.component_type.as_str(),
            "table" | "list" | "task_list"
        )
    {
        return Err(invalid(format!(
            "columns are not supported by component type {}",
            component.component_type
        )));
    }
    let allowed_options = known_options(&component.component_type);
    if component
        .options
        .keys()
        .any(|key| !allowed_options.contains(&key.as_str()))
    {
        return Err(invalid(format!(
            "unsupported option for component {}",
            component.id
        )));
    }
    let mut action_ids = BTreeSet::new();
    for action in &component.actions {
        if !UI_ACTIONS.contains(&action.as_str()) {
            return Err(invalid(format!("unregistered view action: {action}")));
        }
        if !action_ids.insert(action) {
            return Err(invalid(format!("duplicate view action: {action}")));
        }
    }
    Ok(())
}

pub fn validate_views(views: &[BlueprintView]) -> Result<(), super::BlueprintError> {
    let mut view_ids = BTreeSet::new();
    for view in views {
        if !ident(&view.id) {
            return Err(invalid(format!("invalid view identifier: {}", view.id)));
        }
        if !view_ids.insert(&view.id) {
            return Err(invalid(format!("duplicate view identifier: {}", view.id)));
        }
        if view.title.trim().is_empty() {
            return Err(invalid(format!("view {} needs a title", view.id)));
        }
        if !(1..=4).contains(&view.layout.columns) {
            return Err(invalid(format!(
                "view {} layout columns must be between 1 and 4",
                view.id
            )));
        }
        let mut component_ids = BTreeSet::new();
        for component in &view.components {
            if !component_ids.insert(&component.id) {
                return Err(invalid(format!(
                    "duplicate component identifier in view {}: {}",
                    view.id, component.id
                )));
            }
            validate_component(component)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Blueprint;

    fn legacy() -> Blueprint {
        serde_json::from_str(include_str!("../../../blueprints/development.json")).unwrap()
    }

    fn component(id: &str, component_type: &str) -> ViewComponent {
        ViewComponent {
            id: id.into(),
            component_type: component_type.into(),
            title: Some(id.into()),
            source: Some("tasks".into()),
            aggregate: None,
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

    fn view(id: &str, components: Vec<ViewComponent>) -> BlueprintView {
        BlueprintView {
            id: id.into(),
            title: id.into(),
            description: String::new(),
            layout: ViewLayout::default(),
            components,
        }
    }

    #[test]
    fn legacy_manifests_without_views_remain_valid() {
        let mut blueprint = legacy();
        blueprint.views.clear();
        blueprint.validate(&BTreeSet::new()).unwrap();
    }

    #[test]
    fn duplicate_view_and_component_ids_are_rejected() {
        let mut blueprint = legacy();
        blueprint.views = vec![
            view("overview", vec![component("tasks", "task_list")]),
            view("overview", vec![component("other", "task_list")]),
        ];
        assert!(blueprint.validate(&BTreeSet::new()).is_err());
        blueprint.views = vec![view(
            "overview",
            vec![component("tasks", "task_list"), component("tasks", "list")],
        )];
        assert!(blueprint.validate(&BTreeSet::new()).is_err());
    }

    #[test]
    fn unsupported_component_and_source_are_rejected() {
        let mut blueprint = legacy();
        blueprint.views = vec![view("overview", vec![component("chart", "chart")])];
        assert!(blueprint.validate(&BTreeSet::new()).is_err());
        let mut unsupported = component("tasks", "list");
        unsupported.source = Some("shell".into());
        blueprint.views = vec![view("overview", vec![unsupported])];
        assert!(blueprint.validate(&BTreeSet::new()).is_err());
    }

    #[test]
    fn component_requirements_and_actions_are_typed() {
        let mut blueprint = legacy();
        let mut metric = component("ready", "metric");
        metric.source = None;
        blueprint.views = vec![view("overview", vec![metric])];
        assert!(blueprint.validate(&BTreeSet::new()).is_err());

        let mut sum = component("estimate", "metric");
        sum.source = Some("tasks".into());
        sum.aggregate = Some("sum".into());
        blueprint.views = vec![view("overview", vec![sum])];
        assert!(blueprint.validate(&BTreeSet::new()).is_err());

        let mut action = component("tasks", "list");
        action.actions = vec!["shell.exec".into()];
        blueprint.views = vec![view("overview", vec![action])];
        assert!(blueprint.validate(&BTreeSet::new()).is_err());
    }

    #[test]
    fn bundled_views_use_only_registered_components_and_sources() {
        for id in ["development", "research", "learning", "habits", "basic"] {
            let blueprint = crate::bundled::bundled_blueprint(id).unwrap();
            assert!(!blueprint.views.is_empty(), "{id} should demonstrate views");
            for view in &blueprint.views {
                for component in &view.components {
                    assert!(COMPONENT_TYPES.contains(&component.component_type.as_str()));
                    if let Some(source) = component.effective_source() {
                        assert!(DATA_SOURCES.contains(&source));
                    }
                }
            }
        }
    }
}
