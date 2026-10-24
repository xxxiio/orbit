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
/// One ordered acceptance-criteria operation on the wire.
///
/// Additive alongside the task command envelope: clients that do not know
/// these operations continue to send the older actions unchanged.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum AcceptanceCriteriaAction {
    Add {
        criterion_id: String,
        text: String,
    },
    SetText {
        criterion_id: String,
        text: String,
    },
    SetCompleted {
        criterion_id: String,
        completed: bool,
    },
    Reorder {
        criterion_id: String,
        position: usize,
    },
    Remove {
        criterion_id: String,
    },
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
    /// Replace the rich task details. These variants are additive: older
    /// payloads remain valid and older clients simply never emit them.
    SetDetails {
        task_id: String,
        description: String,
        priority: Option<String>,
        labels: Vec<String>,
    },
    /// Apply one ordered acceptance-criteria operation. Checking criteria never
    /// changes the task lifecycle status.
    AcceptanceCriteria {
        task_id: String,
        change: AcceptanceCriteriaAction,
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
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub priority: Option<String>,
    #[serde(default)]
    pub labels: Vec<String>,
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
    /// Optional workflow to pin at creation. Absent means the Basic default.
    /// Only read on the first, accepted command; retries return the receipt.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workflow_id: Option<String>,
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
