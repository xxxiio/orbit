//! Resolved, renderer-neutral view model. No Ratatui types appear here.
use crate::data::*;
use orbit_blueprint::presentation::{ViewComponent, ViewLayout};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedView {
    pub id: String,
    pub title: String,
    pub description: String,
    pub layout: ViewLayout,
    pub components: Vec<ResolvedComponent>,
    pub generated: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedComponent {
    pub id: String,
    pub title: String,
    pub component_type: String,
    pub status: LoadStatus,
    pub body: ComponentBody,
    pub selection: Vec<SelectionItem>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum LoadStatus {
    Loading,
    Ready,
    Empty,
    Error(String),
    Stale(String),
    Missing(String),
}
impl LoadStatus {
    pub fn is_error(&self) -> bool {
        matches!(self, Self::Error(_) | Self::Missing(_))
    }
    pub fn badge(&self) -> Option<&str> {
        match self {
            Self::Loading => Some("loading"),
            Self::Stale(_) => Some("stale"),
            Self::Error(_) | Self::Missing(_) => Some("unavailable"),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ComponentBody {
    Text {
        heading: String,
        lines: Vec<String>,
    },
    List {
        items: Vec<ListItemModel>,
    },
    Table {
        columns: Vec<String>,
        rows: Vec<TableRowModel>,
    },
    Metric {
        value: String,
        label: String,
        detail: String,
    },
    Detail {
        properties: Vec<PropertyModel>,
        note: String,
    },
    Kanban {
        columns: Vec<KanbanColumnModel>,
    },
    Timeline {
        events: Vec<TimelineEventModel>,
    },
    HabitTracker {
        habits: Vec<HabitLineModel>,
    },
    MetricHistory {
        metrics: Vec<MetricLineModel>,
    },
    KnowledgeRefs {
        items: Vec<ListItemModel>,
    },
    AgentJobs {
        jobs: Vec<ListItemModel>,
    },
    MilestoneProgress {
        milestones: Vec<MilestoneLineModel>,
    },
    Missing {
        component_type: String,
        reason: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListItemModel {
    pub id: String,
    pub kind: ItemKind,
    pub title: String,
    pub subtitle: String,
    pub status: String,
    pub group: String,
    pub meta: Vec<(String, String)>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TableRowModel {
    pub id: String,
    pub kind: ItemKind,
    pub status: String,
    pub cells: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct KanbanColumnModel {
    pub key: String,
    pub title: String,
    pub items: Vec<ListItemModel>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimelineEventModel {
    pub id: String,
    pub kind: ItemKind,
    pub title: String,
    pub when: String,
    pub sort_key: i64,
    pub status: String,
    pub detail: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HabitLineModel {
    pub id: String,
    pub name: String,
    pub cadence: String,
    pub active: bool,
    pub count: usize,
    pub last: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MetricLineModel {
    pub id: String,
    pub label: String,
    pub latest: Option<i64>,
    pub target: i64,
    pub baseline: i64,
    pub direction: String,
    pub progress: Option<u16>,
    pub history: Vec<i64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MilestoneLineModel {
    pub id: String,
    pub name: String,
    pub status: String,
    pub done: usize,
    pub total: usize,
    pub progress: Option<u16>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PropertyModel {
    pub key: String,
    pub value: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SelectionItem {
    pub id: String,
    pub kind: ItemKind,
    pub title: String,
    pub subtitle: String,
    pub status: String,
    pub fields: Vec<(String, String)>,
    pub allowed_statuses: Vec<String>,
    pub allowed_transitions: Vec<String>,
    pub revision: Option<u64>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemKind {
    #[default]
    Project,
    Task,
    WorkItem,
    Milestone,
    Knowledge,
    Habit,
    Metric,
    AgentJob,
    Schedule,
    Text,
}
impl ItemKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Project => "project",
            Self::Task => "task",
            Self::WorkItem => "work_item",
            Self::Milestone => "milestone",
            Self::Knowledge => "knowledge",
            Self::Habit => "habit",
            Self::Metric => "metric",
            Self::AgentJob => "agent_job",
            Self::Schedule => "schedule",
            Self::Text => "text",
        }
    }
}

impl ResolvedComponent {
    pub fn empty(component: &ViewComponent, message: impl Into<String>) -> Self {
        Self {
            id: component.id.clone(),
            title: component.display_title().to_owned(),
            component_type: component.component_type.clone(),
            status: LoadStatus::Empty,
            body: ComponentBody::Text {
                heading: component.display_title().to_owned(),
                lines: vec![message.into()],
            },
            selection: Vec::new(),
        }
    }
    pub fn selected(&self, index: usize) -> Option<&SelectionItem> {
        if self.selection.is_empty() {
            None
        } else {
            self.selection.get(index.min(self.selection.len() - 1))
        }
    }
}

/// Rendering-oriented detail for the selected row, also framework-neutral.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DetailModel {
    pub title: String,
    pub subtitle: String,
    pub status: String,
    pub kind: ItemKind,
    pub fields: Vec<(String, String)>,
    pub note: String,
    pub actions: Vec<AvailableAction>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AvailableAction {
    pub id: String,
    pub label: String,
    pub description: String,
    pub payload: Option<String>,
    pub enabled: bool,
    pub disabled_reason: Option<String>,
    pub destructive: bool,
}

impl AvailableAction {
    pub fn enabled(id: &str, label: impl Into<String>, payload: Option<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            description: String::new(),
            payload,
            enabled: true,
            disabled_reason: None,
            destructive: false,
        }
    }
    pub fn disabled(id: &str, label: impl Into<String>, reason: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            description: String::new(),
            payload: None,
            enabled: false,
            disabled_reason: Some(reason.into()),
            destructive: false,
        }
    }
}

pub fn task_selection(task: &TaskRow) -> SelectionItem {
    let mut fields = vec![
        ("status".into(), task.status.clone()),
        ("readiness".into(), task.readiness.clone()),
        ("actionable".into(), task.actionable.to_string()),
    ];
    if let Some(minutes) = task.estimated_minutes {
        fields.push(("estimate".into(), format!("{minutes}m")));
    }
    if !task.prerequisite_ids.is_empty() {
        fields.push(("prerequisites".into(), task.prerequisite_ids.join(", ")));
    }
    if !task.blocking_task_ids.is_empty() {
        fields.push(("blocked by".into(), task.blocking_task_ids.join(", ")));
    }
    if let Some(deadline) = task.deadline_at_ms {
        fields.push(("deadline".into(), format_epoch_ms(deadline)));
    }
    SelectionItem {
        id: task.id.clone(),
        kind: ItemKind::Task,
        title: task.title.clone(),
        subtitle: task.readiness.clone(),
        status: task.status.clone(),
        fields,
        allowed_statuses: task.allowed_statuses.clone(),
        allowed_transitions: Vec::new(),
        revision: None,
    }
}

pub fn work_item_selection(item: &WorkItemRow) -> SelectionItem {
    SelectionItem {
        id: item.id.clone(),
        kind: ItemKind::WorkItem,
        title: item
            .properties
            .iter()
            .find(|(key, _)| {
                matches!(
                    key.as_str(),
                    "title" | "question" | "activity" | "topic" | "subject" | "method"
                )
            })
            .map(|(_, value)| value.clone())
            .unwrap_or_else(|| format!("{} {}", item.kind, item.id)),
        subtitle: format!("{} · rev {}", item.kind, item.revision),
        status: item.state.clone(),
        fields: item.properties.clone(),
        allowed_statuses: Vec::new(),
        allowed_transitions: item.allowed_transitions.clone(),
        revision: Some(item.revision),
    }
}

pub fn project_selection(project: &ProjectRow) -> SelectionItem {
    let mut fields = vec![
        ("status".into(), project.status.clone()),
        ("mode".into(), project.mode.clone()),
        (
            "criteria".into(),
            format!("{}/{}", project.criteria_satisfied, project.criteria_total),
        ),
    ];
    if let Some(objective) = &project.objective {
        fields.push(("objective".into(), objective.clone()));
    }
    if let Some(health) = &project.health {
        fields.push(("health".into(), health.clone()));
    }
    if let Some(evidence) = &project.evidence {
        fields.push(("evidence".into(), evidence.clone()));
    }
    SelectionItem {
        id: project.id.clone(),
        kind: ItemKind::Project,
        title: project.name.clone(),
        subtitle: project.mode.clone(),
        status: project.status.clone(),
        fields,
        allowed_statuses: Vec::new(),
        allowed_transitions: Vec::new(),
        revision: None,
    }
}

pub fn format_epoch_ms(ms: i64) -> String {
    // Renderer-neutral UTC date-time without a chrono dependency. The value is
    // informational; authoritative scheduling remains in Orbit's planner.
    let seconds = ms.div_euclid(1000);
    let days = seconds.div_euclid(86_400);
    let day_seconds = seconds.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02}Z",
        day_seconds / 3600,
        (day_seconds % 3600) / 60
    )
}

fn civil_from_days(days: i64) -> (i64, i64, i64) {
    // Howard Hinnant's civil_from_days, adjusted to Unix epoch.
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = mp + if mp < 10 { 3 } else { -9 };
    (y + i64::from(m <= 2), m, d)
}
