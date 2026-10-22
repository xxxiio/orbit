//! Resolve declarative blueprint views into typed renderer-neutral components.
use crate::actions;
use crate::binder::{self, Row, SourceKind};
use crate::data::ProjectData;
use crate::model::*;
use crate::registry::ComponentRegistry;
use orbit_blueprint::Blueprint;
use orbit_blueprint::presentation::{BlueprintView, ViewComponent, ViewLayout, ViewLayoutKind};

pub fn resolve_blueprint_views(
    blueprint: Option<&Blueprint>,
    data: &ProjectData,
    registry: &ComponentRegistry,
) -> (Vec<ResolvedView>, bool) {
    match blueprint {
        Some(blueprint) if !blueprint.views.is_empty() => (
            blueprint
                .views
                .iter()
                .map(|view| resolve_view(view, data, registry, Some(blueprint), false))
                .collect(),
            false,
        ),
        _ => (default_views(data, registry), true),
    }
}

pub fn resolve_view(
    view: &BlueprintView,
    data: &ProjectData,
    registry: &ComponentRegistry,
    blueprint: Option<&Blueprint>,
    generated: bool,
) -> ResolvedView {
    let components = view
        .components
        .iter()
        .map(|component| resolve_component(component, data, registry, blueprint))
        .collect();
    ResolvedView {
        id: view.id.clone(),
        title: view.title.clone(),
        description: view.description.clone(),
        layout: view.layout.clone(),
        components,
        generated,
    }
}

pub fn resolve_component(
    component: &ViewComponent,
    data: &ProjectData,
    registry: &ComponentRegistry,
    blueprint: Option<&Blueprint>,
) -> ResolvedComponent {
    if !registry.validate(component).is_empty() {
        let issues = registry.validate(component);
        let reason = issues
            .iter()
            .map(|issue| issue.message.clone())
            .collect::<Vec<_>>()
            .join("; ");
        let missing = registry.get(&component.component_type).is_none();
        return ResolvedComponent {
            id: component.id.clone(),
            title: component.display_title().to_owned(),
            component_type: component.component_type.clone(),
            status: if missing {
                LoadStatus::Missing(reason)
            } else {
                LoadStatus::Error(reason)
            },
            body: ComponentBody::Missing {
                component_type: component.component_type.clone(),
                reason: "This component is unavailable in the current renderer. Install or enable the extension that provides it.".into(),
            },
            selection: Vec::new(),
        };
    }
    let source = component.effective_source().unwrap_or("project");
    if SourceKind::parse(source).is_none() {
        return ResolvedComponent {
            id: component.id.clone(),
            title: component.display_title().to_owned(),
            component_type: component.component_type.clone(),
            status: LoadStatus::Error(format!("unsupported data source: {source}")),
            body: ComponentBody::Missing {
                component_type: component.component_type.clone(),
                reason: format!("The data source '{source}' is not registered."),
            },
            selection: Vec::new(),
        };
    }
    let rows = binder::query_rows(data, source, component);
    let mut resolved = build_body(component, data, source, &rows, blueprint);
    resolved.selection = ordered_selection(&resolved.body, &resolved.selection);
    if rows.is_empty()
        && !matches!(
            resolved.body,
            ComponentBody::Metric { .. }
                | ComponentBody::Text { .. }
                | ComponentBody::Detail { .. }
                | ComponentBody::Kanban { .. }
                | ComponentBody::Missing { .. }
        )
    {
        resolved.status = LoadStatus::Empty;
        resolved.body = ComponentBody::Text {
            heading: component.display_title().to_owned(),
            lines: vec![empty_text(component, data, source)],
        };
        resolved.selection.clear();
    }
    resolved
}

/// Reorder selection to match the body's visual order (kanban columns,
/// chronological timeline, metric history and milestone lists). Without this,
/// the detail panel and actions could address a different row than the one
/// highlighted by the renderer.
fn ordered_selection(body: &ComponentBody, selection: &[SelectionItem]) -> Vec<SelectionItem> {
    let ids: Vec<&str> = match body {
        ComponentBody::Kanban { columns } => columns
            .iter()
            .flat_map(|column| column.items.iter())
            .map(|item| item.id.as_str())
            .collect(),
        ComponentBody::Timeline { events } => {
            events.iter().map(|event| event.id.as_str()).collect()
        }
        ComponentBody::HabitTracker { habits } => {
            habits.iter().map(|habit| habit.id.as_str()).collect()
        }
        ComponentBody::MetricHistory { metrics } => {
            metrics.iter().map(|metric| metric.id.as_str()).collect()
        }
        ComponentBody::MilestoneProgress { milestones } => milestones
            .iter()
            .map(|milestone| milestone.id.as_str())
            .collect(),
        _ => return selection.to_vec(),
    };
    let by_id: std::collections::BTreeMap<&str, &SelectionItem> = selection
        .iter()
        .map(|item| (item.id.as_str(), item))
        .collect();
    ids.into_iter()
        .filter_map(|id| by_id.get(id).copied().cloned())
        .collect()
}

fn empty_text(component: &ViewComponent, data: &ProjectData, source: &str) -> String {
    component
        .options
        .get("empty_text")
        .and_then(|value| value.as_str())
        .map(str::to_owned)
        .unwrap_or_else(|| {
            format!(
                "No {} in {} yet. Press c to capture work, or r to refresh.",
                source_label(source),
                data.project.name
            )
        })
}

fn source_label(source: &str) -> &'static str {
    match source {
        "tasks" => "tasks",
        "work_items" => "workflow items",
        "milestones" => "milestones",
        "knowledge" => "knowledge notes",
        "habits" => "habits",
        "metrics" => "metrics",
        "agent_jobs" => "agent jobs",
        "schedule" => "scheduled blocks",
        _ => "records",
    }
}

fn build_body(
    component: &ViewComponent,
    data: &ProjectData,
    source: &str,
    rows: &[Row<'_>],
    blueprint: Option<&Blueprint>,
) -> ResolvedComponent {
    let title = component.display_title().to_owned();
    let status = if rows.is_empty() {
        LoadStatus::Empty
    } else {
        LoadStatus::Ready
    };
    let selection = if matches!(component.component_type.as_str(), "metric" | "text") {
        Vec::new()
    } else if component.component_type == "detail" {
        match source {
            "project" => vec![project_selection(&data.project)],
            "tasks" if !data.tasks.is_empty() => vec![task_selection(&data.tasks[0])],
            "work_items" if !data.work_items.is_empty() => {
                vec![work_item_selection(&data.work_items[0])]
            }
            _ => Vec::new(),
        }
    } else {
        rows.iter()
            .map(|row| row.selection())
            .collect::<Vec<SelectionItem>>()
    };
    let body = match component.component_type.as_str() {
        "text" => ComponentBody::Text {
            heading: title.clone(),
            lines: component
                .text
                .as_deref()
                .unwrap_or_default()
                .lines()
                .map(str::to_owned)
                .collect(),
        },
        "list" => ComponentBody::List {
            items: rows.iter().map(|row| row.list_item()).collect(),
        },
        "table" => {
            let columns = if component.columns.is_empty() {
                default_columns(source)
                    .iter()
                    .map(|column| (*column).to_owned())
                    .collect()
            } else {
                component.columns.clone()
            };
            ComponentBody::Table {
                rows: rows
                    .iter()
                    .map(|row| TableRowModel {
                        id: row.id().into(),
                        kind: row.kind(),
                        status: row.status(),
                        cells: columns
                            .iter()
                            .map(|column| {
                                if column == "title" || column == "name" {
                                    row.title()
                                } else {
                                    row.field(column)
                                }
                            })
                            .collect(),
                    })
                    .collect(),
                columns,
            }
        }
        "metric" => {
            let aggregate = binder::aggregate(data, source, component, rows);
            ComponentBody::Metric {
                value: aggregate
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "—".into()),
                label: title.clone(),
                detail: component
                    .options
                    .get("caption")
                    .and_then(|value| value.as_str())
                    .map(str::to_owned)
                    .unwrap_or_else(|| {
                        format!(
                            "{} · {}",
                            source_label(source),
                            if component.filter.is_empty() {
                                "all records".to_owned()
                            } else {
                                component
                                    .filter
                                    .iter()
                                    .map(|(key, value)| format!("{key}={value}"))
                                    .collect::<Vec<_>>()
                                    .join(" · ")
                            }
                        )
                    }),
            }
        }
        "detail" => {
            let (properties, note) = match source {
                "tasks" => data
                    .tasks
                    .first()
                    .map(|task| (task_selection(task).fields, "Task detail".to_owned()))
                    .unwrap_or_else(|| (Vec::new(), "No tasks yet".into())),
                "work_items" => data
                    .work_items
                    .first()
                    .map(|item| {
                        (
                            work_item_selection(item).fields,
                            "Work item detail".to_owned(),
                        )
                    })
                    .unwrap_or_else(|| (Vec::new(), "No work items yet".into())),
                _ => (
                    project_selection(&data.project).fields,
                    data.project
                        .objective
                        .clone()
                        .unwrap_or_else(|| "Project overview".into()),
                ),
            };
            ComponentBody::Detail {
                properties: properties
                    .into_iter()
                    .map(|(key, value)| PropertyModel { key, value })
                    .collect(),
                note,
            }
        }
        "kanban" => {
            let group_by = component
                .group_by
                .as_deref()
                .unwrap_or(if source == "work_items" {
                    "state"
                } else {
                    "status"
                });
            let mut column_keys = Vec::new();
            if source == "tasks" {
                for state in task_workflow_states() {
                    if !column_keys.contains(&(*state).to_owned()) {
                        column_keys.push((*state).to_owned());
                    }
                }
            } else if source == "work_items"
                && let Some(blueprint) = blueprint
            {
                for kind in &blueprint.work_items {
                    for state in &kind.states {
                        if !column_keys.contains(state) {
                            column_keys.push(state.clone());
                        }
                    }
                }
            }
            for row in rows {
                let key = row.group(group_by);
                if !column_keys.contains(&key) {
                    column_keys.push(key);
                }
            }
            if column_keys.is_empty() {
                column_keys.push(if source == "work_items" {
                    "state".into()
                } else {
                    "status".into()
                });
            }
            ComponentBody::Kanban {
                columns: column_keys
                    .into_iter()
                    .map(|key| KanbanColumnModel {
                        title: humanize(&key),
                        items: rows
                            .iter()
                            .filter(|row| row.group(group_by) == key)
                            .map(|row| row.list_item())
                            .collect(),
                        key,
                    })
                    .collect(),
            }
        }
        "task_list" => ComponentBody::List {
            items: rows.iter().map(|row| row.list_item()).collect(),
        },
        "timeline" => {
            let mut events: Vec<TimelineEventModel> = match source {
                "schedule" => data
                    .schedule
                    .iter()
                    .map(|row| TimelineEventModel {
                        id: row.id.clone(),
                        kind: ItemKind::Schedule,
                        title: row.title.clone(),
                        when: format_epoch_ms(row.starts_at_ms),
                        sort_key: row.starts_at_ms,
                        status: row.flexibility.clone(),
                        detail: format!(
                            "{} → {}",
                            format_epoch_ms(row.starts_at_ms),
                            format_epoch_ms(row.ends_at_ms)
                        ),
                    })
                    .collect(),
                _ => data
                    .milestones
                    .iter()
                    .enumerate()
                    .map(|(index, row)| TimelineEventModel {
                        id: row.id.clone(),
                        kind: ItemKind::Milestone,
                        title: row.name.clone(),
                        when: row.status.clone(),
                        sort_key: index as i64,
                        status: row.status.clone(),
                        detail: format!("{}/{} tasks complete", row.task_done, row.task_total),
                    })
                    .collect(),
            };
            events.sort_by_key(|event| (event.sort_key, event.id.clone()));
            ComponentBody::Timeline { events }
        }
        "habit_tracker" => ComponentBody::HabitTracker {
            habits: data
                .habits
                .iter()
                .map(|row| HabitLineModel {
                    id: row.id.clone(),
                    name: row.name.clone(),
                    cadence: row.cadence.clone(),
                    active: row.active,
                    count: row.occurrences_28d,
                    last: row
                        .last_occurred_at_ms
                        .map(format_epoch_ms)
                        .unwrap_or_else(|| "never".into()),
                })
                .collect(),
        },
        "metric_history" => ComponentBody::MetricHistory {
            metrics: data
                .metrics
                .iter()
                .map(|row| MetricLineModel {
                    id: row.id.clone(),
                    label: row.id.clone(),
                    latest: row.latest,
                    target: row.target,
                    baseline: row.baseline,
                    direction: row.direction.clone(),
                    progress: metric_progress(row),
                    history: row.observations.iter().map(|item| item.value).collect(),
                })
                .collect(),
        },
        "knowledge_refs" => ComponentBody::KnowledgeRefs {
            items: rows.iter().map(|row| row.list_item()).collect(),
        },
        "agent_jobs" => ComponentBody::AgentJobs {
            jobs: rows.iter().map(|row| row.list_item()).collect(),
        },
        "milestone_progress" => ComponentBody::MilestoneProgress {
            milestones: data
                .milestones
                .iter()
                .map(|row| MilestoneLineModel {
                    id: row.id.clone(),
                    name: row.name.clone(),
                    status: row.status.clone(),
                    done: row.task_done,
                    total: row.task_total,
                    progress: row
                        .task_done
                        .checked_mul(100)
                        .and_then(|value| value.checked_div(row.task_total))
                        .map(|value| value as u16),
                })
                .collect(),
        },
        other => ComponentBody::Missing {
            component_type: other.into(),
            reason: "No renderer is registered for this component.".into(),
        },
    };
    ResolvedComponent {
        id: component.id.clone(),
        title,
        component_type: component.component_type.clone(),
        status,
        body,
        selection,
    }
}

fn metric_progress(row: &crate::data::MetricRow) -> Option<u16> {
    let latest = row.latest?;
    let span = row.target - row.baseline;
    if span == 0 {
        return Some(if latest >= row.target { 100 } else { 0 });
    }
    let raw = if row.direction == "at_most" {
        row.baseline - latest
    } else {
        latest - row.baseline
    };
    let percent = raw.saturating_mul(100) / span.abs().max(1);
    Some(percent.clamp(0, 100) as u16)
}

fn default_columns(source: &str) -> &'static [&'static str] {
    match source {
        "tasks" => &[
            "title",
            "status",
            "priority",
            "due",
            "readiness",
            "estimate",
        ],
        "work_items" => &["title", "kind", "state"],
        "milestones" => &["name", "status", "progress"],
        "knowledge" => &["title", "kind", "status"],
        "habits" => &["name", "cadence", "occurrences"],
        "metrics" => &["id", "latest", "target"],
        "agent_jobs" => &["task", "agent", "state", "updated"],
        "schedule" => &["title", "start", "end"],
        "project" => &["name", "status", "mode"],
        _ => &["title", "status"],
    }
}

pub fn task_workflow_states() -> &'static [&'static str] {
    &[
        "inbox",
        "planned",
        "ready",
        "in_progress",
        "waiting",
        "blocked",
        "review",
        "deferred",
        "done",
    ]
}

/// User-facing table header for a column identifier. Renderers keep the raw
/// identifier for layout/status decisions and use this only for header text.
pub fn column_label(column: &str) -> &str {
    match column {
        "priority" => "Priority",
        "due" | "due_date" => "Due",
        other => other,
    }
}

fn humanize(value: &str) -> String {
    let mut text = value.replace('_', " ");
    if let Some(first) = text.get_mut(0..1) {
        first.make_ascii_uppercase();
    }
    text
}

fn view(
    id: &str,
    title: &str,
    description: &str,
    layout: ViewLayout,
    components: Vec<ViewComponent>,
) -> BlueprintView {
    BlueprintView {
        id: id.into(),
        title: title.into(),
        description: description.into(),
        layout,
        components,
    }
}

fn component(id: &str, component_type: &str, source: Option<&str>, title: &str) -> ViewComponent {
    ViewComponent {
        id: id.into(),
        component_type: component_type.into(),
        title: Some(title.into()),
        source: source.map(str::to_owned),
        aggregate: (component_type == "metric").then(|| "count".into()),
        field: None,
        group_by: None,
        filter: Default::default(),
        sort: None,
        descending: false,
        limit: None,
        columns: Vec::new(),
        text: None,
        actions: Vec::new(),
        options: Default::default(),
    }
}

fn with_filter(mut component: ViewComponent, filters: &[(&str, &str)]) -> ViewComponent {
    for (key, value) in filters {
        component.filter.insert((*key).into(), (*value).into());
    }
    component
}

fn with_limit(mut component: ViewComponent, limit: u32) -> ViewComponent {
    component.limit = Some(limit);
    component
}

fn with_sort(mut component: ViewComponent, sort: &str, descending: bool) -> ViewComponent {
    component.sort = Some(sort.into());
    component.descending = descending;
    component
}

/// A generated fallback for legacy blueprints and for projects whose pinned
/// blueprint has no presentation section. It only uses real project data.
pub fn default_views(data: &ProjectData, registry: &ComponentRegistry) -> Vec<ResolvedView> {
    let mut views = vec![
        view(
            "overview",
            "Overview",
            "Generated project overview",
            ViewLayout {
                kind: ViewLayoutKind::Grid,
                columns: 2,
            },
            vec![
                with_limit(
                    component("ready_tasks", "metric", Some("tasks"), "Ready tasks"),
                    200,
                )
                .pipe(|component| with_filter(component, &[("status", "ready")])),
                with_limit(
                    component(
                        "milestone_count",
                        "metric",
                        Some("milestones"),
                        "Milestones",
                    ),
                    200,
                ),
                with_limit(
                    component("agent_jobs", "metric", Some("agent_jobs"), "Agent jobs"),
                    200,
                ),
                component("task_list", "task_list", Some("tasks"), "Tasks"),
                component("project_detail", "detail", Some("project"), "Project"),
                with_limit(
                    component(
                        "milestones",
                        "milestone_progress",
                        Some("milestones"),
                        "Milestone progress",
                    ),
                    8,
                ),
            ],
        ),
        view(
            "work",
            "Work",
            "Tasks and workflow",
            ViewLayout {
                kind: ViewLayoutKind::Vertical,
                columns: 1,
            },
            {
                let mut components = vec![with_limit(
                    with_sort(
                        component("task_board", "kanban", Some("tasks"), "Task board"),
                        "status",
                        false,
                    ),
                    100,
                )];
                if !data.work_items.is_empty() {
                    components.push(with_limit(
                        component("work_items", "kanban", Some("work_items"), "Workflow items"),
                        100,
                    ));
                }
                components
            },
        ),
    ];
    if !data.knowledge.is_empty() {
        views.push(view(
            "knowledge",
            "Knowledge",
            "Project knowledge references",
            ViewLayout::default(),
            vec![with_limit(
                component("notes", "knowledge_refs", Some("knowledge"), "Knowledge"),
                50,
            )],
        ));
    }
    if !data.habits.is_empty() || !data.metrics.is_empty() {
        views.push(view(
            "habits",
            "Habits & metrics",
            "Project-associated recurring activity",
            ViewLayout {
                kind: ViewLayoutKind::Grid,
                columns: 2,
            },
            vec![
                component("habits", "habit_tracker", Some("habits"), "Habits"),
                component("metrics", "metric_history", Some("metrics"), "Metrics"),
            ],
        ));
    }
    if !data.agent_jobs.is_empty() || !data.schedule.is_empty() {
        views.push(view(
            "activity",
            "Activity",
            "Agent jobs and scheduled work",
            ViewLayout {
                kind: ViewLayoutKind::Vertical,
                columns: 1,
            },
            vec![
                component("jobs", "agent_jobs", Some("agent_jobs"), "Agent jobs"),
                component("schedule", "timeline", Some("schedule"), "Schedule"),
            ],
        ));
    }
    views
        .into_iter()
        .map(|view| resolve_view(&view, data, registry, None, true))
        .collect()
}

trait Pipe: Sized {
    fn pipe<T>(self, f: impl FnOnce(Self) -> T) -> T {
        f(self)
    }
}
impl<T> Pipe for T {}

/// Build a compact workspace summary view for Home. This uses the same
/// components and data contracts as project views.
pub fn home_view(workspace: &crate::data::WorkspaceData) -> ResolvedView {
    let registry = ComponentRegistry::builtin();
    let data = ProjectData {
        project: crate::data::ProjectRow {
            id: "workspace".into(),
            name: "Workspace".into(),
            status: "active".into(),
            mode: "local".into(),
            ..Default::default()
        },
        tasks: workspace.tasks.clone(),
        knowledge: workspace.recent_knowledge.clone(),
        habits: workspace.active_habits.clone(),
        agent_jobs: workspace.agent_jobs.clone(),
        pending_commands: workspace.pending_commands,
        ..Default::default()
    };
    let view = view(
        "home",
        "Home",
        "Workspace aggregation",
        ViewLayout {
            kind: ViewLayoutKind::Grid,
            columns: 2,
        },
        vec![
            component("projects", "metric", Some("project"), "Projects"),
            with_filter(
                component("inbox", "metric", Some("tasks"), "Inbox"),
                &[("status", "inbox")],
            ),
            with_filter(
                component("ready", "metric", Some("tasks"), "Ready"),
                &[("status", "ready")],
            ),
            with_filter(
                component("inbox_list", "list", Some("tasks"), "Inbox"),
                &[("status", "inbox")],
            ),
            component("habits", "habit_tracker", Some("habits"), "Active habits"),
            component(
                "knowledge",
                "knowledge_refs",
                Some("knowledge"),
                "Recent knowledge",
            ),
        ],
    );
    let mut resolved = resolve_view(&view, &data, &registry, None, true);
    // The project source row is the synthetic workspace row; replace its
    // metric with a real project count so Home reports the actual workspace.
    if let Some(component) = resolved
        .components
        .iter_mut()
        .find(|component| component.id == "projects")
    {
        component.body = ComponentBody::Metric {
            value: workspace.projects.len().to_string(),
            label: "Projects".into(),
            detail: format!("{} pending commands", workspace.pending_commands),
        };
    }
    resolved.components.retain(|component| {
        !matches!(component.status, LoadStatus::Empty) || component.id == "inbox_list"
    });
    resolved
}

/// Build the global inbox as a reusable resolved view.
pub fn inbox_view(workspace: &crate::data::WorkspaceData) -> ResolvedView {
    let registry = ComponentRegistry::builtin();
    let data = ProjectData {
        tasks: workspace.inbox.clone(),
        ..Default::default()
    };
    let view = view(
        "inbox",
        "Inbox",
        "Unassigned and untriaged tasks",
        ViewLayout::default(),
        vec![with_filter(
            component("inbox_tasks", "task_list", Some("tasks"), "Inbox tasks"),
            &[("status", "inbox")],
        )],
    );
    resolve_view(&view, &data, &registry, None, true)
}

/// Build the global agent monitor from the same agent-job contract used by
/// project views.
pub fn agents_view(workspace: &crate::data::WorkspaceData) -> ResolvedView {
    let registry = ComponentRegistry::builtin();
    let data = ProjectData {
        agent_jobs: workspace.agent_jobs.clone(),
        ..Default::default()
    };
    let view = view(
        "agents",
        "Agents",
        "Explicitly authorized agent execution",
        ViewLayout {
            kind: ViewLayoutKind::Grid,
            columns: 2,
        },
        vec![
            component("agent_jobs", "agent_jobs", Some("agent_jobs"), "Jobs"),
            component("agent_config", "text", None, "Connection"),
        ],
    );
    let mut resolved = resolve_view(&view, &data, &registry, None, true);
    if let Some(component) = resolved
        .components
        .iter_mut()
        .find(|component| component.id == "agent_config")
    {
        component.body = ComponentBody::Text {
            heading: "Local agent configuration".into(),
            lines: vec![
                "Agents execute only after an explicit authorized launch.".into(),
                "Use the project Activity view to inspect job state and handoffs.".into(),
                "Connected-mode reviews remain subject to server acceptance.".into(),
            ],
        };
        component.status = LoadStatus::Ready;
    }
    resolved
}

/// Detail model for a resolved component's selected row.
pub fn detail_for_component(
    component: &ResolvedComponent,
    index: usize,
    project_scope: bool,
) -> Option<DetailModel> {
    component
        .selected(index)
        .map(|item| actions::detail_for(item, project_scope))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::*;

    fn project() -> ProjectData {
        ProjectData {
            project: ProjectRow {
                id: "p1".into(),
                name: "Apollo".into(),
                status: "active".into(),
                mode: "delivery".into(),
                criteria_satisfied: 1,
                criteria_total: 2,
                ..Default::default()
            },
            tasks: vec![
                TaskRow {
                    id: "t1".into(),
                    title: "Ready task".into(),
                    status: "ready".into(),
                    actionable: true,
                    readiness: "ready".into(),
                    priority: "high".into(),
                    due_date: "2026-11-03".into(),
                    acceptance_total: 3,
                    acceptance_completed: 2,
                    ..Default::default()
                },
                TaskRow {
                    id: "t2".into(),
                    title: "Inbox task".into(),
                    status: "inbox".into(),
                    readiness: "inbox".into(),
                    ..Default::default()
                },
            ],
            work_items: vec![WorkItemRow {
                id: "w1".into(),
                project_id: "p1".into(),
                kind: "feature".into(),
                state: "backlog".into(),
                properties: vec![("priority".into(), "high".into())],
                allowed_transitions: vec!["ready".into()],
                ..Default::default()
            }],
            ..Default::default()
        }
    }

    #[test]
    fn generated_default_view_uses_real_data() {
        let registry = ComponentRegistry::builtin();
        let views = default_views(&project(), &registry);
        assert!(views.iter().any(|view| view.id == "overview"));
        let overview = views.iter().find(|view| view.id == "overview").unwrap();
        assert!(overview.generated);
        assert!(overview.components.iter().any(|component| {
            matches!(&component.body, ComponentBody::Metric { value, .. } if value == "1")
        }));
    }

    #[test]
    fn unsupported_component_fails_safe() {
        let registry = ComponentRegistry::builtin();
        let mut view = view(
            "x",
            "X",
            "",
            ViewLayout::default(),
            vec![component("mystery", "mystery", Some("tasks"), "Mystery")],
        );
        view.components[0].component_type = "mystery".into();
        let resolved = resolve_view(&view, &project(), &registry, None, false);
        assert!(resolved.components[0].status.is_error());
        assert!(matches!(
            resolved.components[0].body,
            ComponentBody::Missing { .. }
        ));
    }

    #[test]
    fn filters_sorting_and_limits_are_bounded() {
        let registry = ComponentRegistry::builtin();
        let component = with_limit(
            with_sort(
                with_filter(
                    component("tasks", "list", Some("tasks"), "Tasks"),
                    &[("status", "ready")],
                ),
                "title",
                false,
            ),
            1,
        );
        let resolved = resolve_component(&component, &project(), &registry, None);
        assert_eq!(resolved.selection.len(), 1);
        assert_eq!(resolved.selection[0].id, "t1");
    }

    #[test]
    fn kanban_keeps_workflow_columns_for_empty_states() {
        let registry = ComponentRegistry::builtin();
        let component = component("board", "kanban", Some("work_items"), "Board");
        let blueprint: Blueprint =
            serde_json::from_str(include_str!("../../../blueprints/development.json")).unwrap();
        let resolved = resolve_component(&component, &project(), &registry, Some(&blueprint));
        let ComponentBody::Kanban { columns } = resolved.body else {
            panic!("expected kanban");
        };
        assert!(columns.iter().any(|column| column.key == "review"));
    }

    #[test]
    fn task_table_exposes_priority_and_due_columns() {
        let registry = ComponentRegistry::builtin();
        let component = component("tasks", "table", Some("tasks"), "Tasks");
        let resolved = resolve_component(&component, &project(), &registry, None);
        let ComponentBody::Table { columns, rows } = resolved.body else {
            panic!("expected table");
        };
        assert!(columns.contains(&"priority".to_string()));
        assert!(columns.contains(&"due".to_string()));
        let priority = columns
            .iter()
            .position(|column| column == "priority")
            .unwrap();
        let due = columns.iter().position(|column| column == "due").unwrap();
        assert_eq!(rows[0].cells[priority], "high");
        assert_eq!(rows[0].cells[due], "2026-11-03");
        // An unset due date renders empty rather than inventing a date.
        assert_eq!(rows[1].cells[due], "");
        assert_eq!(column_label("priority"), "Priority");
        assert_eq!(column_label("due_date"), "Due");
    }

    #[test]
    fn work_items_use_workflow_terminology_in_rendered_text() {
        assert_eq!(source_label("work_items"), "workflow items");
        let registry = ComponentRegistry::builtin();
        let views = default_views(&project(), &registry);
        let work = views.iter().find(|view| view.id == "work").unwrap();
        assert!(
            work.components
                .iter()
                .any(|component| component.title == "Workflow items")
        );
    }
}
