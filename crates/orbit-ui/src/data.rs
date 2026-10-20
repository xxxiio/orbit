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
    pub deadline_at_ms: Option<i64>,
    pub actionable: bool,
    pub readiness: String,
    pub prerequisite_ids: Vec<String>,
    pub blocking_task_ids: Vec<String>,
    /// Workflow transitions offered by the application layer. The authority
    /// still validates every request; this is presentation input only.
    pub allowed_statuses: Vec<String>,
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
