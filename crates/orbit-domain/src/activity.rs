use crate::{
    ActivityId, DomainError, HabitId, KnowledgeItemId, MilestoneId, ProjectId, RoutineId,
    ScheduleBlockId, TaskId,
};

/// The entity whose state or context changed.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ActivitySubject {
    Project(ProjectId),
    Milestone(MilestoneId),
    Task(TaskId),
    Habit(HabitId),
    Knowledge(KnowledgeItemId),
    Routine(RoutineId),
    ScheduleBlock(ScheduleBlockId),
}

/// Stable semantic categories suitable for activity history and future sync.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ActivityKind {
    Created,
    Updated,
    StatusChanged,
    ProgressUpdated,
    BlockerOpened,
    BlockerResolved,
    Scheduled,
    Rescheduled,
    Completed,
    AttachmentAdded,
    AttachmentRemoved,
    Custom(String),
}

/// Origin of an activity without coupling the domain to a particular agent or provider.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ActivityOrigin {
    Human,
    System,
    Agent(String),
    External(String),
}

/// Append-friendly semantic record of meaningful work/state change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActivityRecord {
    id: ActivityId,
    subject: ActivitySubject,
    kind: ActivityKind,
    origin: ActivityOrigin,
    occurred_at_ms: i64,
    summary: String,
}

impl ActivityRecord {
    pub fn new(
        id: ActivityId,
        subject: ActivitySubject,
        kind: ActivityKind,
        origin: ActivityOrigin,
        occurred_at_ms: i64,
        summary: impl Into<String>,
    ) -> Result<Self, DomainError> {
        let summary = summary.into();
        if summary.trim().is_empty() {
            return Err(DomainError::EmptyActivitySummary);
        }

        Ok(Self {
            id,
            subject,
            kind,
            origin,
            occurred_at_ms,
            summary,
        })
    }

    pub fn id(&self) -> &ActivityId {
        &self.id
    }

    pub fn subject(&self) -> &ActivitySubject {
        &self.subject
    }

    pub fn kind(&self) -> &ActivityKind {
        &self.kind
    }

    pub fn origin(&self) -> &ActivityOrigin {
        &self.origin
    }

    pub fn occurred_at_ms(&self) -> i64 {
        self.occurred_at_ms
    }

    pub fn summary(&self) -> &str {
        &self.summary
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn activity_records_are_semantic_and_timestamped() {
        let record = ActivityRecord::new(
            ActivityId::new("activity-1").expect("valid activity id"),
            ActivitySubject::Task(TaskId::new("task-1").expect("valid task id")),
            ActivityKind::StatusChanged,
            ActivityOrigin::Agent("coding-agent".to_owned()),
            1234,
            "Task moved to review",
        )
        .expect("valid activity");

        assert_eq!(record.occurred_at_ms(), 1234);
        assert_eq!(record.summary(), "Task moved to review");
        assert_eq!(record.kind(), &ActivityKind::StatusChanged);
    }

    #[test]
    fn activity_summary_must_be_non_blank() {
        let result = ActivityRecord::new(
            ActivityId::new("activity-1").expect("valid activity id"),
            ActivitySubject::Project(ProjectId::new("project-1").expect("valid project id")),
            ActivityKind::Updated,
            ActivityOrigin::Human,
            0,
            "   ",
        );

        assert_eq!(result, Err(DomainError::EmptyActivitySummary));
    }
}
