//! Closed data-source allowlists and bounded filtering/sorting.
use crate::data::*;
use crate::model::*;
use crate::registry::{numeric_field, source_fields};
use orbit_blueprint::presentation::ViewComponent;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceKind {
    Project,
    Tasks,
    WorkItems,
    Milestones,
    Knowledge,
    Habits,
    Metrics,
    AgentJobs,
    Schedule,
}
impl SourceKind {
    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "project" => Self::Project,
            "tasks" => Self::Tasks,
            "work_items" => Self::WorkItems,
            "milestones" => Self::Milestones,
            "knowledge" => Self::Knowledge,
            "habits" => Self::Habits,
            "metrics" => Self::Metrics,
            "agent_jobs" => Self::AgentJobs,
            "schedule" => Self::Schedule,
            _ => return None,
        })
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Project => "project",
            Self::Tasks => "tasks",
            Self::WorkItems => "work_items",
            Self::Milestones => "milestones",
            Self::Knowledge => "knowledge",
            Self::Habits => "habits",
            Self::Metrics => "metrics",
            Self::AgentJobs => "agent_jobs",
            Self::Schedule => "schedule",
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum Row<'a> {
    Project(&'a ProjectRow),
    Task(&'a TaskRow),
    WorkItem(&'a WorkItemRow),
    Milestone(&'a MilestoneRow),
    Knowledge(&'a KnowledgeRow),
    Habit(&'a HabitRow),
    Metric(&'a MetricRow),
    AgentJob(&'a AgentJobRow),
    Schedule(&'a ScheduleRow),
}

impl<'a> Row<'a> {
    pub fn id(self) -> &'a str {
        match self {
            Self::Project(row) => &row.id,
            Self::Task(row) => &row.id,
            Self::WorkItem(row) => &row.id,
            Self::Milestone(row) => &row.id,
            Self::Knowledge(row) => &row.id,
            Self::Habit(row) => &row.id,
            Self::Metric(row) => &row.id,
            Self::AgentJob(row) => &row.id,
            Self::Schedule(row) => &row.id,
        }
    }
    pub fn kind(self) -> ItemKind {
        match self {
            Self::Project(_) => ItemKind::Project,
            Self::Task(_) => ItemKind::Task,
            Self::WorkItem(_) => ItemKind::WorkItem,
            Self::Milestone(_) => ItemKind::Milestone,
            Self::Knowledge(_) => ItemKind::Knowledge,
            Self::Habit(_) => ItemKind::Habit,
            Self::Metric(_) => ItemKind::Metric,
            Self::AgentJob(_) => ItemKind::AgentJob,
            Self::Schedule(_) => ItemKind::Schedule,
        }
    }
    pub fn title(self) -> String {
        match self {
            Self::Project(row) => row.name.clone(),
            Self::Task(row) => row.title.clone(),
            Self::WorkItem(row) => work_item_title(row),
            Self::Milestone(row) => row.name.clone(),
            Self::Knowledge(row) => row.title.clone(),
            Self::Habit(row) => row.name.clone(),
            Self::Metric(row) => row.id.clone(),
            Self::AgentJob(row) => format!("{} · {}", row.agent_kind, row.task_id),
            Self::Schedule(row) => row.title.clone(),
        }
    }
    pub fn status(self) -> String {
        match self {
            Self::Project(row) => row.status.clone(),
            Self::Task(row) => row.status.clone(),
            Self::WorkItem(row) => row.state.clone(),
            Self::Milestone(row) => row.status.clone(),
            Self::Knowledge(row) => row.status.clone(),
            Self::Habit(row) => {
                if row.active {
                    "active".into()
                } else {
                    "paused".into()
                }
            }
            Self::Metric(row) => row
                .latest
                .map(|value| value.to_string())
                .unwrap_or_else(|| "no data".into()),
            Self::AgentJob(row) => row.state.clone(),
            Self::Schedule(row) => row.flexibility.clone(),
        }
    }
    pub fn group(self, group_by: &str) -> String {
        match group_by {
            "state" | "status" => self.status(),
            "kind" => match self {
                Self::WorkItem(row) => row.kind.clone(),
                Self::Knowledge(row) => row.kind.clone(),
                _ => self.kind().as_str().into(),
            },
            "readiness" => match self {
                Self::Task(row) => row.readiness.clone(),
                _ => self.status(),
            },
            "agent" => match self {
                Self::AgentJob(row) => row.agent_kind.clone(),
                _ => self.status(),
            },
            "project" => match self {
                Self::Task(row) => row
                    .project_id
                    .clone()
                    .unwrap_or_else(|| "unassigned".into()),
                Self::WorkItem(row) => row.project_id.clone(),
                Self::AgentJob(row) => row.project_id.clone(),
                _ => self.status(),
            },
            _ => self.status(),
        }
    }
    pub fn field(self, field: &str) -> String {
        match (self, field) {
            (Self::Project(row), "id") => row.id.clone(),
            (Self::Project(row), "name") => row.name.clone(),
            (Self::Project(row), "status") => row.status.clone(),
            (Self::Project(row), "mode") => row.mode.clone(),
            (Self::Project(row), "objective") => row.objective.clone().unwrap_or_default(),
            (Self::Project(row), "completion_ready") => row.completion_ready.to_string(),
            (Self::Project(row), "criteria") => {
                format!("{}/{}", row.criteria_satisfied, row.criteria_total)
            }
            (Self::Project(row), "health") => row.health.clone().unwrap_or_default(),
            (Self::Project(row), "evidence") => row.evidence.clone().unwrap_or_default(),
            (Self::Task(row), "id") => row.id.clone(),
            (Self::Task(row), "title") => row.title.clone(),
            (Self::Task(row), "status") => row.status.clone(),
            (Self::Task(row), "project") => row.project_id.clone().unwrap_or_default(),
            (Self::Task(row), "milestone") => row.milestone_id.clone().unwrap_or_default(),
            (Self::Task(row), "estimate") => row
                .estimated_minutes
                .map(|value| value.to_string())
                .unwrap_or_default(),
            (Self::Task(row), "deadline") => {
                row.deadline_at_ms.map(format_epoch_ms).unwrap_or_default()
            }
            (Self::Task(row), "readiness") => row.readiness.clone(),
            (Self::Task(row), "actionable") => row.actionable.to_string(),
            (Self::Task(row), "priority") => row.status.clone(),
            (Self::WorkItem(row), "id") => row.id.clone(),
            (Self::WorkItem(row), "title") => work_item_title(row),
            (Self::WorkItem(row), "kind") => row.kind.clone(),
            (Self::WorkItem(row), "state") | (Self::WorkItem(row), "status") => row.state.clone(),
            (Self::WorkItem(row), "revision") => row.revision.to_string(),
            (Self::WorkItem(row), field) => row
                .properties
                .iter()
                .find(|(key, _)| key == field)
                .map(|(_, value)| value.clone())
                .unwrap_or_default(),
            (Self::Milestone(row), "id") => row.id.clone(),
            (Self::Milestone(row), "name") => row.name.clone(),
            (Self::Milestone(row), "status") => row.status.clone(),
            (Self::Milestone(row), "progress") => row
                .task_total
                .checked_div(1)
                .filter(|_| row.task_total > 0)
                .map(|_| format!("{}%", row.task_done * 100 / row.task_total.max(1)))
                .unwrap_or_else(|| "unknown".into()),
            (Self::Milestone(row), "total") => row.task_total.to_string(),
            (Self::Milestone(row), "done") => row.task_done.to_string(),
            (Self::Knowledge(row), "id") => row.id.clone(),
            (Self::Knowledge(row), "title") => row.title.clone(),
            (Self::Knowledge(row), "kind") => row.kind.clone(),
            (Self::Knowledge(row), "status") => row.status.clone(),
            (Self::Knowledge(row), "provenance") => row.provenance.clone(),
            (Self::Knowledge(row), "updated") => format_epoch_ms(row.updated_at_ms),
            (Self::Habit(row), "id") => row.id.clone(),
            (Self::Habit(row), "name") => row.name.clone(),
            (Self::Habit(row), "cadence") => row.cadence.clone(),
            (Self::Habit(row), "active") => row.active.to_string(),
            (Self::Habit(row), "occurrences") => row.occurrences_28d.to_string(),
            (Self::Habit(row), "last") => row
                .last_occurred_at_ms
                .map(format_epoch_ms)
                .unwrap_or_default(),
            (Self::Metric(row), "id") => row.id.clone(),
            (Self::Metric(row), "latest") | (Self::Metric(row), "value") => row
                .latest
                .map(|value| value.to_string())
                .unwrap_or_default(),
            (Self::Metric(row), "target") => row.target.to_string(),
            (Self::Metric(row), "baseline") => row.baseline.to_string(),
            (Self::Metric(row), "direction") => row.direction.clone(),
            (Self::AgentJob(row), "id") => row.id.clone(),
            (Self::AgentJob(row), "state") | (Self::AgentJob(row), "status") => row.state.clone(),
            (Self::AgentJob(row), "agent") => row.agent_kind.clone(),
            (Self::AgentJob(row), "task") => row.task_id.clone(),
            (Self::AgentJob(row), "machine") => row.machine_ref.clone(),
            (Self::AgentJob(row), "updated") => format_epoch_ms(row.updated_at_ms),
            (Self::Schedule(row), "id") => row.id.clone(),
            (Self::Schedule(row), "title") => row.title.clone(),
            (Self::Schedule(row), "start") => format_epoch_ms(row.starts_at_ms),
            (Self::Schedule(row), "end") => format_epoch_ms(row.ends_at_ms),
            (Self::Schedule(row), "target") => row.target_id.clone().unwrap_or_default(),
            (Self::Schedule(row), "flexibility") => row.flexibility.clone(),
            _ => String::new(),
        }
    }
    pub fn list_item(self) -> ListItemModel {
        ListItemModel {
            id: self.id().into(),
            kind: self.kind(),
            title: self.title(),
            subtitle: self.subtitle(),
            status: self.status(),
            group: self.status(),
            meta: self.meta(),
        }
    }
    pub fn subtitle(self) -> String {
        match self {
            Self::Task(row) => format!("{} · {}", row.readiness, row.status),
            Self::WorkItem(row) => format!("{} · {}", row.kind, row.state),
            Self::Milestone(row) => {
                format!(
                    "{} · {}/{} tasks",
                    row.status, row.task_done, row.task_total
                )
            }
            Self::Knowledge(row) => format!("{} · {}", row.kind, row.status),
            Self::Habit(row) => format!("{} · {} in 28d", row.cadence, row.occurrences_28d),
            Self::Metric(row) => format!(
                "latest {} / target {}",
                row.latest
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "—".into()),
                row.target
            ),
            Self::AgentJob(row) => format!(
                "{} · {} handoff(s) · {}{}",
                row.state,
                row.handoff_count,
                row.machine_ref,
                if row.needs_attention {
                    " · attention"
                } else {
                    ""
                }
            ),
            Self::Schedule(row) => {
                format!(
                    "{} → {}",
                    format_epoch_ms(row.starts_at_ms),
                    format_epoch_ms(row.ends_at_ms)
                )
            }
            Self::Project(row) => format!("{} · {}", row.mode, row.status),
        }
    }
    pub fn meta(self) -> Vec<(String, String)> {
        match self {
            Self::Task(row) => {
                let mut meta = vec![("status".into(), row.status.clone())];
                if let Some(estimate) = row.estimated_minutes {
                    meta.push(("estimate".into(), format!("{estimate}m")));
                }
                if !row.blocking_task_ids.is_empty() {
                    meta.push(("blocked by".into(), row.blocking_task_ids.join(", ")));
                }
                meta
            }
            Self::WorkItem(row) => row.properties.clone(),
            Self::Milestone(row) => vec![
                ("done".into(), row.task_done.to_string()),
                ("total".into(), row.task_total.to_string()),
            ],
            Self::Knowledge(row) => vec![("kind".into(), row.kind.clone())],
            Self::Habit(row) => vec![("cadence".into(), row.cadence.clone())],
            Self::Metric(row) => vec![
                (
                    "latest".into(),
                    row.latest.map(|v| v.to_string()).unwrap_or_default(),
                ),
                ("target".into(), row.target.to_string()),
            ],
            Self::AgentJob(row) => vec![("agent".into(), row.agent_kind.clone())],
            Self::Schedule(row) => vec![("flexibility".into(), row.flexibility.clone())],
            Self::Project(row) => vec![("mode".into(), row.mode.clone())],
        }
    }
    pub fn selection(self) -> SelectionItem {
        match self {
            Self::Project(row) => project_selection(row),
            Self::Task(row) => task_selection(row),
            Self::WorkItem(row) => work_item_selection(row),
            Self::Milestone(row) => SelectionItem {
                id: row.id.clone(),
                kind: ItemKind::Milestone,
                title: row.name.clone(),
                subtitle: format!("{}/{} tasks", row.task_done, row.task_total),
                status: row.status.clone(),
                fields: vec![_pair("status", &row.status)],
                allowed_statuses: Vec::new(),
                allowed_transitions: Vec::new(),
                revision: None,
            },
            Self::Knowledge(row) => {
                let mut fields = vec![_pair("kind", &row.kind), _pair("status", &row.status)];
                fields.push(_pair("updated", &format_epoch_ms(row.updated_at_ms)));
                SelectionItem {
                    id: row.id.clone(),
                    kind: ItemKind::Knowledge,
                    title: row.title.clone(),
                    subtitle: row.kind.clone(),
                    status: row.status.clone(),
                    fields,
                    allowed_statuses: Vec::new(),
                    allowed_transitions: Vec::new(),
                    revision: None,
                }
            }
            Self::Habit(row) => {
                let mut fields = vec![
                    _pair("cadence", &row.cadence),
                    _pair("occurrences-28d", &row.occurrences_28d.to_string()),
                ];
                if let Some(last) = row.last_occurred_at_ms {
                    fields.push(_pair("last", &format_epoch_ms(last)));
                }
                SelectionItem {
                    id: row.id.clone(),
                    kind: ItemKind::Habit,
                    title: row.name.clone(),
                    subtitle: row.cadence.clone(),
                    status: if row.active {
                        "active".into()
                    } else {
                        "paused".into()
                    },
                    fields,
                    allowed_statuses: Vec::new(),
                    allowed_transitions: Vec::new(),
                    revision: None,
                }
            }
            Self::Metric(row) => SelectionItem {
                id: row.id.clone(),
                kind: ItemKind::Metric,
                title: row.id.clone(),
                subtitle: format!("target {}", row.target),
                status: row
                    .latest
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "no data".into()),
                fields: vec![
                    _pair("baseline", &row.baseline.to_string()),
                    _pair("target", &row.target.to_string()),
                    _pair("direction", &row.direction),
                ],
                allowed_statuses: Vec::new(),
                allowed_transitions: Vec::new(),
                revision: None,
            },
            Self::AgentJob(row) => SelectionItem {
                id: row.id.clone(),
                kind: ItemKind::AgentJob,
                title: format!("{} · {}", row.agent_kind, row.task_id),
                subtitle: row.machine_ref.clone(),
                status: row.state.clone(),
                fields: vec![
                    _pair("agent", &row.agent_kind),
                    _pair("task", &row.task_id),
                    _pair("runtime", &row.runtime_kind),
                    _pair("updated", &format_epoch_ms(row.updated_at_ms)),
                    _pair("handoffs", &row.handoff_count.to_string()),
                    _pair("review", row.review_decision.as_deref().unwrap_or("none")),
                ],
                allowed_statuses: Vec::new(),
                allowed_transitions: Vec::new(),
                revision: None,
            },
            Self::Schedule(row) => SelectionItem {
                id: row.id.clone(),
                kind: ItemKind::Schedule,
                title: row.title.clone(),
                subtitle: format_epoch_ms(row.starts_at_ms),
                status: row.flexibility.clone(),
                fields: vec![
                    _pair("start", &format_epoch_ms(row.starts_at_ms)),
                    _pair("end", &format_epoch_ms(row.ends_at_ms)),
                    _pair("target", row.target_id.as_deref().unwrap_or("")),
                ],
                allowed_statuses: Vec::new(),
                allowed_transitions: Vec::new(),
                revision: None,
            },
        }
    }
}

fn _pair(key: &str, value: &str) -> (String, String) {
    (key.to_owned(), value.to_owned())
}

pub fn work_item_title(row: &WorkItemRow) -> String {
    row.properties
        .iter()
        .find(|(key, _)| {
            matches!(
                key.as_str(),
                "title" | "name" | "question" | "activity" | "topic" | "subject" | "method"
            )
        })
        .map(|(_, value)| value.clone())
        .unwrap_or_else(|| format!("{} {}", row.kind, row.id))
}

pub fn rows<'a>(data: &'a ProjectData, source: &str) -> Vec<Row<'a>> {
    match source {
        "project" => vec![Row::Project(&data.project)],
        "tasks" => data.tasks.iter().map(Row::Task).collect(),
        "work_items" => data.work_items.iter().map(Row::WorkItem).collect(),
        "milestones" => data.milestones.iter().map(Row::Milestone).collect(),
        "knowledge" => data.knowledge.iter().map(Row::Knowledge).collect(),
        "habits" => data.habits.iter().map(Row::Habit).collect(),
        "metrics" => data.metrics.iter().map(Row::Metric).collect(),
        "agent_jobs" => data.agent_jobs.iter().map(Row::AgentJob).collect(),
        "schedule" => data.schedule.iter().map(Row::Schedule).collect(),
        _ => Vec::new(),
    }
}

pub fn query_rows<'a>(
    data: &'a ProjectData,
    source: &str,
    component: &ViewComponent,
) -> Vec<Row<'a>> {
    let filters: Vec<(&String, &String)> = component.filter.iter().collect();
    for key in filters.iter().map(|(key, _)| key.as_str()) {
        if !source_fields(source).contains(&key)
            && !matches!(key, "active" | "actionable" | "priority")
            && !(source == "work_items"
                && key
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b == b'_' || b.is_ascii_digit()))
        {
            // Unknown filters are ignored rather than broadening the query.
            return Vec::new();
        }
    }
    let mut rows: Vec<Row<'a>> = rows(data, source)
        .into_iter()
        .filter(|row| {
            filters.iter().all(|(key, value)| {
                row.field(key).eq_ignore_ascii_case(value)
                    || row.status().eq_ignore_ascii_case(value)
            })
        })
        .collect();
    if let Some(sort) = &component.sort {
        rows.sort_by(|a, b| {
            let left = a.field(sort);
            let right = b.field(sort);
            let ordering = left.cmp(&right);
            if component.descending {
                ordering.reverse()
            } else {
                ordering
            }
            .then_with(|| a.id().cmp(b.id()))
        });
    } else {
        rows.sort_by(|a, b| a.id().cmp(b.id()));
    }
    if let Some(limit) = component.limit {
        rows.truncate(limit as usize);
    }
    rows
}

pub fn aggregate(
    data: &ProjectData,
    source: &str,
    component: &ViewComponent,
    rows: &[Row<'_>],
) -> Option<i64> {
    match component.aggregate.as_deref().unwrap_or("count") {
        "count" => Some(rows.len() as i64),
        aggregate => {
            let field = component.field.as_deref()?;
            let values: Vec<i64> = rows
                .iter()
                .filter_map(|row| numeric_field(data, source, field, row.id()))
                .collect();
            match aggregate {
                "sum" => Some(values.iter().sum()),
                "average" if !values.is_empty() => {
                    Some(values.iter().sum::<i64>() / values.len() as i64)
                }
                "min" => values.iter().copied().min(),
                "max" => values.iter().copied().max(),
                _ => None,
            }
        }
    }
}
