use crate::{DomainError, MilestoneId, ProjectId, TaskId};

/// Optional placement of a task in the project hierarchy.
///
/// Standalone tasks have neither a project nor a milestone. A milestone is
/// never valid without its parent project being specified.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TaskScope {
    project_id: Option<ProjectId>,
    milestone_id: Option<MilestoneId>,
}

impl TaskScope {
    pub fn standalone() -> Self {
        Self::default()
    }

    pub fn in_project(project_id: ProjectId) -> Self {
        Self {
            project_id: Some(project_id),
            milestone_id: None,
        }
    }

    pub fn in_milestone(project_id: ProjectId, milestone_id: MilestoneId) -> Self {
        Self {
            project_id: Some(project_id),
            milestone_id: Some(milestone_id),
        }
    }

    pub fn try_new(
        project_id: Option<ProjectId>,
        milestone_id: Option<MilestoneId>,
    ) -> Result<Self, DomainError> {
        if project_id.is_none() && milestone_id.is_some() {
            return Err(DomainError::MilestoneRequiresProject);
        }

        Ok(Self {
            project_id,
            milestone_id,
        })
    }

    pub fn project_id(&self) -> Option<&ProjectId> {
        self.project_id.as_ref()
    }

    pub fn milestone_id(&self) -> Option<&MilestoneId> {
        self.milestone_id.as_ref()
    }

    pub fn is_standalone(&self) -> bool {
        self.project_id.is_none()
    }
}

/// Built-in task workflow states.
///
/// Custom workflows can be layered on later; these states establish a stable
/// core vocabulary for planning and interoperability.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TaskStatus {
    Inbox,
    Planned,
    Ready,
    InProgress,
    Waiting,
    Blocked,
    Review,
    Deferred,
    Done,
    Cancelled,
}

impl TaskStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Inbox => "inbox",
            Self::Planned => "planned",
            Self::Ready => "ready",
            Self::InProgress => "in_progress",
            Self::Waiting => "waiting",
            Self::Blocked => "blocked",
            Self::Review => "review",
            Self::Deferred => "deferred",
            Self::Done => "done",
            Self::Cancelled => "cancelled",
        }
    }

    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Done | Self::Cancelled)
    }

    pub fn can_transition_to(self, next: Self) -> bool {
        if self == next {
            return true;
        }

        match self {
            Self::Inbox => matches!(
                next,
                Self::Planned | Self::Ready | Self::Deferred | Self::Cancelled
            ),
            Self::Planned => matches!(
                next,
                Self::Ready | Self::InProgress | Self::Deferred | Self::Cancelled
            ),
            Self::Ready => matches!(
                next,
                Self::InProgress | Self::Blocked | Self::Deferred | Self::Cancelled
            ),
            Self::InProgress => matches!(
                next,
                Self::Waiting
                    | Self::Blocked
                    | Self::Review
                    | Self::Done
                    | Self::Deferred
                    | Self::Cancelled
            ),
            Self::Waiting => matches!(
                next,
                Self::Ready | Self::InProgress | Self::Blocked | Self::Deferred | Self::Cancelled
            ),
            Self::Blocked => matches!(
                next,
                Self::Ready | Self::InProgress | Self::Waiting | Self::Deferred | Self::Cancelled
            ),
            Self::Review => matches!(
                next,
                Self::InProgress | Self::Blocked | Self::Done | Self::Cancelled
            ),
            Self::Deferred => matches!(next, Self::Planned | Self::Ready | Self::Cancelled),
            Self::Done | Self::Cancelled => false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TaskPlanning {
    estimated_minutes: Option<u32>,
    earliest_start_at_ms: Option<i64>,
    deadline_at_ms: Option<i64>,
}

impl TaskPlanning {
    pub fn new(
        estimated_minutes: Option<u32>,
        earliest_start_at_ms: Option<i64>,
        deadline_at_ms: Option<i64>,
    ) -> Result<Self, DomainError> {
        if matches!(estimated_minutes, Some(0)) {
            return Err(DomainError::InvalidTaskEstimate);
        }
        if let (Some(earliest), Some(deadline)) = (earliest_start_at_ms, deadline_at_ms)
            && earliest > deadline
        {
            return Err(DomainError::InvalidTaskPlanningWindow);
        }
        Ok(Self {
            estimated_minutes,
            earliest_start_at_ms,
            deadline_at_ms,
        })
    }

    pub fn estimated_minutes(self) -> Option<u32> {
        self.estimated_minutes
    }
    pub fn earliest_start_at_ms(self) -> Option<i64> {
        self.earliest_start_at_ms
    }
    pub fn deadline_at_ms(self) -> Option<i64> {
        self.deadline_at_ms
    }
}

/// User-visible task priority. Absence of a priority is represented by
/// `Option::None` rather than a synthetic "none" variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TaskPriority {
    Low,
    Medium,
    High,
    Urgent,
}

impl TaskPriority {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::Urgent => "urgent",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "low" => Some(Self::Low),
            "medium" => Some(Self::Medium),
            "high" => Some(Self::High),
            "urgent" => Some(Self::Urgent),
            _ => None,
        }
    }

    pub fn from_str_checked(value: &str) -> Result<Self, DomainError> {
        Self::parse(value).ok_or(DomainError::InvalidTaskPriority)
    }
}

/// Maximum rich-description length in characters. Descriptions are
/// Markdown-compatible text and are never interpreted as executable input.
pub const MAX_TASK_DESCRIPTION_CHARS: usize = 100_000;
/// Maximum number of labels attached to one task.
pub const MAX_TASK_LABELS: usize = 32;
/// Maximum length of one label in characters.
pub const MAX_TASK_LABEL_CHARS: usize = 64;

/// Rich, optional task metadata that is not part of the task identity.
///
/// Keeping these fields together avoids growing the base task aggregate into an
/// untyped property bag while still supporting ordinary project management.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TaskDetails {
    description: String,
    priority: Option<TaskPriority>,
    labels: Vec<String>,
}

impl TaskDetails {
    pub fn new(
        description: impl Into<String>,
        priority: Option<TaskPriority>,
        labels: impl IntoIterator<Item = String>,
    ) -> Result<Self, DomainError> {
        let mut details = Self::default();
        details.set_description(description)?;
        details.set_priority(priority);
        details.set_labels(labels)?;
        Ok(details)
    }

    pub fn description(&self) -> &str {
        &self.description
    }

    pub fn priority(&self) -> Option<TaskPriority> {
        self.priority
    }

    pub fn labels(&self) -> &[String] {
        &self.labels
    }

    pub fn is_empty(&self) -> bool {
        self.description.is_empty() && self.priority.is_none() && self.labels.is_empty()
    }

    pub fn set_description(&mut self, description: impl Into<String>) -> Result<(), DomainError> {
        let description = description.into();
        if description.chars().count() > MAX_TASK_DESCRIPTION_CHARS {
            return Err(DomainError::TaskDescriptionTooLong);
        }
        if description.contains('\0') {
            return Err(DomainError::TaskDescriptionTooLong);
        }
        self.description = description;
        Ok(())
    }

    pub fn set_priority(&mut self, priority: Option<TaskPriority>) {
        self.priority = priority;
    }

    /// Normalises labels: trims, rejects empty/control-character labels,
    /// removes duplicates while preserving the caller's order.
    pub fn set_labels(
        &mut self,
        labels: impl IntoIterator<Item = String>,
    ) -> Result<(), DomainError> {
        let mut normalised: Vec<String> = Vec::new();
        for label in labels {
            let label = label.trim().to_owned();
            if label.is_empty()
                || label.chars().count() > MAX_TASK_LABEL_CHARS
                || label.chars().any(char::is_control)
            {
                return Err(DomainError::InvalidTaskLabel);
            }
            if !normalised.iter().any(|existing| existing == &label) {
                normalised.push(label);
            }
        }
        if normalised.len() > MAX_TASK_LABELS {
            return Err(DomainError::TooManyTaskLabels);
        }
        self.labels = normalised;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Task {
    id: TaskId,
    title: String,
    status: TaskStatus,
    scope: TaskScope,
    planning: TaskPlanning,
    details: TaskDetails,
}

impl Task {
    pub fn new(
        id: TaskId,
        title: impl Into<String>,
        scope: TaskScope,
    ) -> Result<Self, DomainError> {
        let title = title.into();
        if title.trim().is_empty() {
            return Err(DomainError::EmptyTaskTitle);
        }

        Ok(Self {
            id,
            title,
            status: TaskStatus::Inbox,
            scope,
            planning: TaskPlanning::default(),
            details: TaskDetails::default(),
        })
    }

    pub fn id(&self) -> &TaskId {
        &self.id
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn status(&self) -> TaskStatus {
        self.status
    }

    pub fn scope(&self) -> &TaskScope {
        &self.scope
    }

    pub fn planning(&self) -> TaskPlanning {
        self.planning
    }

    pub fn details(&self) -> &TaskDetails {
        &self.details
    }

    pub fn set_details(&mut self, details: TaskDetails) {
        self.details = details;
    }

    pub fn set_planning(&mut self, planning: TaskPlanning) {
        self.planning = planning;
    }

    pub fn transition_to(&mut self, next: TaskStatus) -> Result<(), DomainError> {
        if !self.status.can_transition_to(next) {
            return Err(DomainError::InvalidTaskStatusTransition {
                from: self.status.as_str(),
                to: next.as_str(),
            });
        }

        self.status = next;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn task_id() -> TaskId {
        TaskId::new("task-1").expect("valid task id")
    }

    #[test]
    fn task_can_be_created_without_a_project() {
        let task = Task::new(task_id(), "Renew passport", TaskScope::standalone())
            .expect("standalone task should be valid");

        assert!(task.scope().is_standalone());
        assert_eq!(task.scope().project_id(), None);
        assert_eq!(task.scope().milestone_id(), None);
        assert_eq!(task.status(), TaskStatus::Inbox);
    }

    #[test]
    fn milestone_association_without_project_is_rejected() {
        let milestone = MilestoneId::new("milestone-1").expect("valid milestone id");

        let scope = TaskScope::try_new(None, Some(milestone));

        assert_eq!(scope, Err(DomainError::MilestoneRequiresProject));
    }

    #[test]
    fn task_can_belong_to_project_and_milestone() {
        let project = ProjectId::new("project-1").expect("valid project id");
        let milestone = MilestoneId::new("milestone-1").expect("valid milestone id");
        let scope = TaskScope::in_milestone(project.clone(), milestone.clone());
        let task = Task::new(task_id(), "Implement planner", scope).expect("valid task");

        assert_eq!(task.scope().project_id(), Some(&project));
        assert_eq!(task.scope().milestone_id(), Some(&milestone));
    }

    #[test]
    fn valid_status_transition_is_applied() {
        let mut task =
            Task::new(task_id(), "Plan the week", TaskScope::standalone()).expect("valid task");

        task.transition_to(TaskStatus::Ready)
            .expect("inbox -> ready");
        task.transition_to(TaskStatus::InProgress)
            .expect("ready -> in progress");
        task.transition_to(TaskStatus::Done)
            .expect("in progress -> done");

        assert_eq!(task.status(), TaskStatus::Done);
    }

    #[test]
    fn terminal_task_cannot_be_reopened_implicitly() {
        let mut task =
            Task::new(task_id(), "Plan the week", TaskScope::standalone()).expect("valid task");
        task.transition_to(TaskStatus::Ready)
            .expect("inbox -> ready");
        task.transition_to(TaskStatus::InProgress)
            .expect("ready -> in progress");
        task.transition_to(TaskStatus::Done)
            .expect("in progress -> done");

        let error = task
            .transition_to(TaskStatus::InProgress)
            .expect_err("done -> in progress must require an explicit reopen command later");

        assert_eq!(
            error,
            DomainError::InvalidTaskStatusTransition {
                from: "done",
                to: "in_progress",
            }
        );
    }

    #[test]
    fn planning_metadata_rejects_zero_estimates_and_inverted_windows() {
        assert_eq!(
            TaskPlanning::new(Some(0), None, None),
            Err(DomainError::InvalidTaskEstimate)
        );
        assert_eq!(
            TaskPlanning::new(Some(30), Some(200), Some(100)),
            Err(DomainError::InvalidTaskPlanningWindow)
        );
    }

    #[test]
    fn task_planning_metadata_is_optional_and_replaceable() {
        let mut task = Task::new(task_id(), "Plan", TaskScope::standalone()).expect("task");
        assert_eq!(task.planning(), TaskPlanning::default());
        let planning = TaskPlanning::new(Some(45), Some(100), Some(500)).expect("planning");
        task.set_planning(planning);
        assert_eq!(task.planning(), planning);
    }

    #[test]
    fn blank_task_title_is_rejected() {
        let result = Task::new(task_id(), "   ", TaskScope::standalone());
        assert_eq!(result, Err(DomainError::EmptyTaskTitle));
    }
}
