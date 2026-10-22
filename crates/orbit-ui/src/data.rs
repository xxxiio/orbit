//! Framework-neutral, read-only data contracts shared by terminal, desktop and
//! future Flutter renderers. These types contain no persistence handles and no
//! mutation methods.
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceData {
    pub projects: Vec<ProjectRow>,
    pub tasks: Vec<TaskRow>,
    pub inbox: Vec<TaskRow>,
    pub active_habits: Vec<HabitRow>,
    pub recent_knowledge: Vec<KnowledgeRow>,
    pub agent_jobs: Vec<AgentJobRow>,
    pub pending_commands: usize,
    #[serde(default)]
    pub revision: Option<i64>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectData {
    #[serde(default)]
    pub revision: Option<i64>,
    pub project: ProjectRow,
    #[serde(default)]
    pub tasks: Vec<TaskRow>,
    #[serde(default)]
    pub work_items: Vec<WorkItemRow>,
    #[serde(default)]
    pub milestones: Vec<MilestoneRow>,
    #[serde(default)]
    pub knowledge: Vec<KnowledgeRow>,
    #[serde(default)]
    pub habits: Vec<HabitRow>,
    #[serde(default)]
    pub metrics: Vec<MetricRow>,
    #[serde(default)]
    pub agent_jobs: Vec<AgentJobRow>,
    #[serde(default)]
    pub schedule: Vec<ScheduleRow>,
    #[serde(default)]
    pub pending_commands: usize,
    #[serde(default)]
    pub observed_at_ms: i64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectRow {
    pub id: String,
    pub name: String,
    pub status: String,
    pub mode: String,
    pub objective: Option<String>,
    pub completion_ready: bool,
    pub criteria_satisfied: usize,
    pub criteria_total: usize,
    pub health: Option<String>,
    pub evidence: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskRow {
    pub id: String,
    pub title: String,
    pub status: String,
    pub project_id: Option<String>,
    pub milestone_id: Option<String>,
    pub estimated_minutes: Option<u32>,
    pub earliest_start_at_ms: Option<i64>,
    pub deadline_at_ms: Option<i64>,
    pub actionable: bool,
    pub readiness: String,
    pub prerequisite_ids: Vec<String>,
    pub blocking_task_ids: Vec<String>,
    /// Workflow transitions offered by the application layer. The authority
    /// still validates every request; this is presentation input only.
    pub allowed_statuses: Vec<String>,
    /// Task priority exactly as accepted by the client, e.g. `high`. Empty when
    /// the task has no priority; the renderer never invents a default.
    #[serde(default)]
    pub priority: String,
    /// Preformatted `YYYY-MM-DD` due date. Empty when the task has no due date.
    /// Conversion code owns the formatting so renderers stay dependency-free.
    #[serde(default)]
    pub due_date: String,
    /// Acceptance criteria progress. Zero/zero when the task has none.
    #[serde(default)]
    pub acceptance_total: usize,
    #[serde(default)]
    pub acceptance_completed: usize,
}

impl TaskRow {
    /// Minimal constructor for renderers and tests that only know the identity
    /// columns. Every planning/rich field defaults to empty; callers that have
    /// richer data should set the remaining fields explicitly.
    pub fn new(id: impl Into<String>, title: impl Into<String>, status: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            status: status.into(),
            ..Self::default()
        }
    }

    /// `done/total` acceptance progress, or `None` when the task has no
    /// criteria. Bounded so the detail view cannot report more done than total.
    pub fn acceptance_progress(&self) -> Option<(usize, usize)> {
        if self.acceptance_total == 0 {
            return None;
        }
        Some((
            self.acceptance_completed.min(self.acceptance_total),
            self.acceptance_total,
        ))
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkItemRow {
    pub id: String,
    pub project_id: String,
    pub kind: String,
    pub state: String,
    pub revision: u64,
    pub properties: Vec<(String, String)>,
    pub prerequisite_ids: Vec<String>,
    pub allowed_transitions: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MilestoneRow {
    pub id: String,
    pub project_id: String,
    pub name: String,
    pub status: String,
    pub task_total: usize,
    pub task_done: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct KnowledgeRow {
    pub id: String,
    pub kind: String,
    pub title: String,
    pub status: String,
    pub provenance: String,
    pub updated_at_ms: i64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct HabitRow {
    pub id: String,
    pub name: String,
    pub cadence: String,
    pub active: bool,
    pub occurrences_28d: usize,
    pub last_occurred_at_ms: Option<i64>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MetricRow {
    pub id: String,
    pub baseline: i64,
    pub target: i64,
    pub direction: String,
    pub latest: Option<i64>,
    pub observations: Vec<MetricObservationRow>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MetricObservationRow {
    pub observed_at_ms: i64,
    pub value: i64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentJobRow {
    pub id: String,
    pub project_id: String,
    pub task_id: String,
    pub state: String,
    pub agent_kind: String,
    pub machine_ref: String,
    pub runtime_kind: String,
    pub updated_at_ms: i64,
    pub runtime_session_ref: Option<String>,
    pub handoff_count: usize,
    pub review_decision: Option<String>,
    pub needs_attention: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScheduleRow {
    pub id: String,
    pub title: String,
    pub starts_at_ms: i64,
    pub ends_at_ms: i64,
    pub flexibility: String,
    pub target_kind: String,
    pub target_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewDataError {
    pub code: String,
    pub message: String,
}
impl ViewDataError {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }
    pub fn unavailable(message: impl Into<String>) -> Self {
        Self::new("unavailable", message)
    }
    pub fn invalid(message: impl Into<String>) -> Self {
        Self::new("invalid", message)
    }
}
impl std::fmt::Display for ViewDataError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}
impl std::error::Error for ViewDataError {}

/// Read-only source used by renderers and by tests. Implementations live in
/// `orbit-client`, never inside a rendering crate.
pub trait ViewDataSource {
    fn workspace(&self) -> Result<WorkspaceData, ViewDataError>;
    fn project(&self, project_id: &str) -> Result<ProjectData, ViewDataError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_row_new_defaults_rich_fields() {
        let row = TaskRow::new("t1", "Title", "ready");
        assert_eq!(row.id, "t1");
        assert_eq!(row.title, "Title");
        assert_eq!(row.status, "ready");
        assert_eq!(row.priority, "");
        assert_eq!(row.due_date, "");
        assert_eq!(row.acceptance_total, 0);
        assert_eq!(row.acceptance_completed, 0);
        assert_eq!(row.acceptance_progress(), None);
    }

    #[test]
    fn acceptance_progress_is_bounded() {
        let mut row = TaskRow::new("t1", "Title", "ready");
        row.acceptance_total = 3;
        row.acceptance_completed = 9;
        assert_eq!(row.acceptance_progress(), Some((3, 3)));
    }

    #[test]
    fn new_fields_deserialize_when_absent() {
        let row: TaskRow = serde_json::from_str(
            r#"{"id":"t1","title":"T","status":"ready","project_id":null,
                "milestone_id":null,"estimated_minutes":null,
                "earliest_start_at_ms":null,"deadline_at_ms":null,
                "actionable":false,"readiness":"ready","prerequisite_ids":[],
                "blocking_task_ids":[],"allowed_statuses":[]}"#,
        )
        .unwrap();
        assert_eq!(row.priority, "");
        assert_eq!(row.due_date, "");
    }
}
