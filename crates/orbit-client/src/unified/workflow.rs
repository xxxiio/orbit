//! Revision-aware multi-command workflow manifests and planning inputs.
//! They produce intention sequences; they do not imply transactional acceptance.
use super::*;

/// An ordered domain workflow. Each step carries its own stable command ID
/// and expected authority revision; a workflow is NOT a database transaction.
#[derive(Debug, Clone)]
pub enum DomainWorkflowStep {
    Project(ProjectCommand),
    Task(TaskCommand),
    Evidence(EvidenceCommand),
    Schedule(ScheduleCommand),
    Routine(RoutineCommand),
    Milestone(MilestoneCommand),
    Strategy(StrategyCommand),
}

impl DomainWorkflowStep {
    pub fn command_id(&self) -> &str {
        match self {
            Self::Project(cmd) => &cmd.command_id,
            Self::Task(cmd) => &cmd.command_id,
            Self::Evidence(cmd) => &cmd.command_id,
            Self::Schedule(cmd) => &cmd.command_id,
            Self::Routine(cmd) => &cmd.command_id,
            Self::Milestone(cmd) => &cmd.command_id,
            Self::Strategy(cmd) => &cmd.command_id,
        }
    }
}

/// A failed step does not roll back earlier accepted or queued commands.
/// Callers should inspect `completed` before retrying from `failed_index`.
#[derive(Debug)]
pub struct WorkflowFailure {
    pub failed_index: usize,
    pub completed: Vec<CommandDisposition>,
    pub cause: ClientConnectionError,
}

/// A complete project bootstrap assembled before any authoritative writes.
/// Command identities are deterministic so an interrupted workflow can be retried.
/// Every step is explicitly revisioned, preserving the global history sequence.
#[derive(Debug, Clone)]
pub struct ProjectBootstrap {
    pub command_prefix: String,
    pub base_revision: i64,
    pub project_id: String,
    pub project_name: String,
    pub tasks: Vec<BootstrapTask>,
}

#[derive(Debug, Clone)]
pub struct BootstrapTask {
    pub task_id: String,
    pub title: String,
}

impl ProjectBootstrap {
    /// Validate the *entire* launch manifest before queueing or committing
    /// anything. This eliminates avoidable partial launches due to duplicate
    /// IDs or missing task titles. Execution is still intentionally non-atomic.
    pub fn into_workflow(self) -> Result<Vec<DomainWorkflowStep>, ClientConnectionError> {
        let valid_identity = |s: &str| {
            !s.is_empty()
                && s.len() <= 128
                && s.bytes()
                    .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
        };
        if !valid_identity(&self.command_prefix)
            || !valid_identity(&self.project_id)
            || self.project_name.trim().is_empty()
            || self.project_name.len() > 4096
            || self.base_revision < 0
            || self.tasks.len() > 1000
        {
            return Err(ClientConnectionError::Unsupported(
                "invalid project bootstrap manifest",
            ));
        }
        let mut seen = std::collections::HashSet::new();
        for task in &self.tasks {
            if !valid_identity(&task.task_id)
                || !seen.insert(task.task_id.as_str())
                || task.title.trim().is_empty()
                || task.title.len() > 4096
            {
                return Err(ClientConnectionError::Unsupported(
                    "invalid or duplicated bootstrap task",
                ));
            }
        }
        let count = i64::try_from(self.tasks.len())
            .map_err(|_| ClientConnectionError::Unsupported("too many bootstrap tasks"))?;
        self.base_revision
            .checked_add(count + 1)
            .ok_or(ClientConnectionError::Unsupported(
                "bootstrap revision overflow",
            ))?;
        // Reserve room for the suffix so generated command IDs satisfy the
        // server's 128-byte limit, even for the final (1000th) task.
        if self.command_prefix.len() > 115 {
            return Err(ClientConnectionError::Unsupported(
                "bootstrap command prefix too long",
            ));
        }
        let mut steps = Vec::with_capacity(self.tasks.len() + 1);
        steps.push(DomainWorkflowStep::Project(ProjectCommand {
            command_id: format!("{}-project", self.command_prefix),
            expected_revision: self.base_revision,
            project_id: self.project_id.clone(),
            name: self.project_name,
        }));
        for (index, task) in self.tasks.into_iter().enumerate() {
            steps.push(DomainWorkflowStep::Task(TaskCommand {
                command_id: format!("{}-task-{index}", self.command_prefix),
                expected_revision: self.base_revision + 1 + index as i64,
                action: orbit_runtime::domain_commands::TaskAction::Create {
                    task_id: task.task_id,
                    title: task.title,
                    project_id: Some(self.project_id.clone()),
                },
            }));
        }
        Ok(steps)
    }
}

/// A complete task-refinement pass prepared as an ordered authority workflow.
/// This is intentionally non-atomic. Stable IDs make retries safe after a
/// partial commit; the server still owns transition and relationship checks.
#[derive(Debug, Clone)]
pub struct TaskRefinement {
    pub command_prefix: String,
    pub base_revision: i64,
    pub edits: Vec<TaskRefinementEdit>,
}

#[derive(Debug, Clone)]
pub struct TaskRefinementEdit {
    pub task_id: String,
    /// `None` leaves the title unchanged.
    pub title: Option<String>,
    /// `None` leaves the project unchanged; `Some(None)` moves to inbox.
    pub project_id: Option<Option<String>>,
    /// `None` leaves status unchanged.
    pub status: Option<String>,
}

impl TaskRefinement {
    /// Validate all edits and generate retry-stable command IDs before the
    /// first side effect. Each changed field becomes a canonical task command.
    pub fn into_workflow(self) -> Result<Vec<DomainWorkflowStep>, ClientConnectionError> {
        let identity = |s: &str| {
            !s.is_empty()
                && s.len() <= 128
                && s.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
        };
        if !identity(&self.command_prefix)
            || self.command_prefix.len() > 108
            || self.base_revision < 0
            || self.edits.is_empty()
            || self.edits.len() > 1000
        {
            return Err(ClientConnectionError::Unsupported(
                "invalid task refinement manifest",
            ));
        }
        let mut seen = std::collections::HashSet::new();
        let mut actions = Vec::new();
        for edit in self.edits {
            if !identity(&edit.task_id) || !seen.insert(edit.task_id.clone()) {
                return Err(ClientConnectionError::Unsupported(
                    "invalid or repeated task id",
                ));
            }
            let start_len = actions.len();
            if let Some(title) = edit.title {
                if title.trim().is_empty() || title.len() > 4096 {
                    return Err(ClientConnectionError::Unsupported("invalid task title"));
                }
                actions.push(orbit_runtime::domain_commands::TaskAction::Rename {
                    task_id: edit.task_id.clone(),
                    title,
                });
            }
            if let Some(project_id) = edit.project_id {
                if project_id.as_deref().is_some_and(|p| !identity(p)) {
                    return Err(ClientConnectionError::Unsupported(
                        "invalid destination project",
                    ));
                }
                actions.push(orbit_runtime::domain_commands::TaskAction::AssignProject {
                    task_id: edit.task_id.clone(),
                    project_id,
                });
            }
            if let Some(status) = edit.status {
                if !matches!(
                    status.as_str(),
                    "inbox"
                        | "planned"
                        | "ready"
                        | "in_progress"
                        | "waiting"
                        | "blocked"
                        | "review"
                        | "deferred"
                        | "done"
                        | "cancelled"
                ) {
                    return Err(ClientConnectionError::Unsupported("invalid task status"));
                }
                actions.push(orbit_runtime::domain_commands::TaskAction::ChangeStatus {
                    task_id: edit.task_id,
                    status,
                });
            }
            if actions.len() == start_len {
                return Err(ClientConnectionError::Unsupported("empty task edit"));
            }
        }
        let size = i64::try_from(actions.len())
            .map_err(|_| ClientConnectionError::Unsupported("too many task commands"))?;
        self.base_revision
            .checked_add(size)
            .ok_or(ClientConnectionError::Unsupported("task revision overflow"))?;
        Ok(actions
            .into_iter()
            .enumerate()
            .map(|(index, action)| {
                DomainWorkflowStep::Task(TaskCommand {
                    command_id: format!("{}-edit-{index}", self.command_prefix),
                    expected_revision: self.base_revision + index as i64,
                    action,
                })
            })
            .collect())
    }
}

/// One planning session that atomically validates the proposed task windows
/// before any command is committed locally or queued for remote acceptance.
/// Accepted tasks must already exist. Execution is revisioned and non-atomic.
#[derive(Debug, Clone)]
pub struct TaskPlanningSession {
    pub command_prefix: String,
    pub base_revision: i64,
    pub plans: Vec<PlannedTaskWindow>,
}

#[derive(Debug, Clone)]
pub struct PlannedTaskWindow {
    pub task_id: String,
    pub estimated_minutes: Option<i64>,
    pub earliest_start_at_ms: Option<i64>,
    pub deadline_at_ms: Option<i64>,
}

impl TaskPlanningSession {
    /// Reject the entire batch if it includes invalid identities, contradictory
    /// time windows, or duplicate tasks; never partially enqueue malformed plans.
    pub fn into_workflow(self) -> Result<Vec<DomainWorkflowStep>, ClientConnectionError> {
        let valid_id = |s: &str| {
            !s.is_empty()
                && s.len() <= 128
                && s.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
        };
        if !valid_id(&self.command_prefix)
            || self.command_prefix.len() > 112
            || self.base_revision < 0
            || self.plans.is_empty()
            || self.plans.len() > 1000
        {
            return Err(ClientConnectionError::Unsupported(
                "invalid planning session",
            ));
        }
        self.base_revision
            .checked_add(self.plans.len() as i64)
            .ok_or(ClientConnectionError::Unsupported(
                "planning revision overflow",
            ))?;
        let mut seen = std::collections::HashSet::new();
        let mut steps = Vec::with_capacity(self.plans.len());
        for (index, plan) in self.plans.into_iter().enumerate() {
            if !valid_id(&plan.task_id)
                || !seen.insert(plan.task_id.clone())
                || plan.estimated_minutes.is_some_and(|m| m <= 0 || m > 525600)
                || plan.earliest_start_at_ms.is_some_and(|t| t < 0)
                || plan.deadline_at_ms.is_some_and(|t| t < 0)
                || matches!((plan.earliest_start_at_ms, plan.deadline_at_ms),
                    (Some(start), Some(end)) if end < start)
                || matches!((plan.earliest_start_at_ms, plan.deadline_at_ms, plan.estimated_minutes),
                    (Some(start), Some(end), Some(minutes)) if
                        minutes.checked_mul(60_000).is_none_or(|duration| end - start < duration))
            {
                return Err(ClientConnectionError::Unsupported(
                    "invalid task planning window",
                ));
            }
            steps.push(DomainWorkflowStep::Task(TaskCommand {
                command_id: format!("{}-plan-{index}", self.command_prefix),
                expected_revision: self.base_revision + index as i64,
                action: orbit_runtime::domain_commands::TaskAction::SetPlanning {
                    task_id: plan.task_id,
                    estimated_minutes: plan.estimated_minutes,
                    earliest_start_at_ms: plan.earliest_start_at_ms,
                    deadline_at_ms: plan.deadline_at_ms,
                },
            }));
        }
        Ok(steps)
    }
}
