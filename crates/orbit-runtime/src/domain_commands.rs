//! Shared domain command contracts for the canonical authoritative server.
use orbit_domain::TaskStatus;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct TaskCommand {
    pub command_id: String,
    pub expected_revision: i64,
    pub action: TaskAction,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum TaskAction {
    Create {
        task_id: String,
        title: String,
        #[serde(default)]
        project_id: Option<String>,
    },
    ChangeStatus {
        task_id: String,
        status: String,
    },
    Rename {
        task_id: String,
        title: String,
    },
    AssignProject {
        task_id: String,
        project_id: Option<String>,
    },
    /// Attach a task to a checkpoint in the same project, or detach it.
    AssignMilestone {
        task_id: String,
        milestone_id: Option<String>,
    },
    /// Atomically replace the entire planning window (None clears a field).
    SetPlanning {
        task_id: String,
        estimated_minutes: Option<i64>,
        earliest_start_at_ms: Option<i64>,
        deadline_at_ms: Option<i64>,
    },
    /// Add an edge from a dependent task to a prerequisite.
    AddPrerequisite {
        task_id: String,
        prerequisite_id: String,
    },
    /// Remove an existing prerequisite; retrying a command ID is idempotent.
    RemovePrerequisite {
        task_id: String,
        prerequisite_id: String,
    },
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaskRecord {
    pub id: String,
    pub title: String,
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub milestone_id: Option<String>,
    #[serde(default)]
    pub estimated_minutes: Option<i64>,
    #[serde(default)]
    pub earliest_start_at_ms: Option<i64>,
    #[serde(default)]
    pub deadline_at_ms: Option<i64>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaskResult {
    pub revision: i64,
    pub duplicate: bool,
    pub task: TaskRecord,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ProjectCommand {
    pub command_id: String,
    pub expected_revision: i64,
    pub project_id: String,
    pub name: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectRecord {
    pub id: String,
    pub name: String,
    pub status: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectResult {
    pub revision: i64,
    pub duplicate: bool,
    pub project: ProjectRecord,
}

pub(crate) fn parse_status(value: &str) -> Option<TaskStatus> {
    match value {
        "inbox" => Some(TaskStatus::Inbox),
        "planned" => Some(TaskStatus::Planned),
        "ready" => Some(TaskStatus::Ready),
        "in_progress" => Some(TaskStatus::InProgress),
        "waiting" => Some(TaskStatus::Waiting),
        "blocked" => Some(TaskStatus::Blocked),
        "review" => Some(TaskStatus::Review),
        "deferred" => Some(TaskStatus::Deferred),
        "done" => Some(TaskStatus::Done),
        "cancelled" => Some(TaskStatus::Cancelled),
        _ => None,
    }
}
