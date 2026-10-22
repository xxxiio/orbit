//! Core domain model for Orbit.
//!
//! This crate is intentionally UI-, persistence-, sync-, AI-, and transport-free.

mod acceptance;
mod activity;
mod agent_job;
mod attachment;
mod dependency;
mod error;
mod habit;
mod habit_tracking;
mod ids;
mod knowledge;
mod knowledge_resource;
mod milestone;
mod progress;
mod project;
mod routine;
mod schedule;
mod task;

pub use acceptance::{
    AcceptanceCriteria, AcceptanceCriterion, MAX_ACCEPTANCE_CRITERIA,
    MAX_ACCEPTANCE_CRITERION_CHARS,
};
pub use activity::{ActivityKind, ActivityOrigin, ActivityRecord, ActivitySubject};
pub use agent_job::{AgentJob, AgentJobState};
pub use attachment::{
    Attachment, AttachmentOwner, AttachmentOwnerKind, DEFAULT_ATTACHMENT_MAX_BYTES,
    MAX_ATTACHMENT_DESCRIPTION_CHARS, content_type_for_filename, validate_attachment_content_type,
    validate_attachment_filename,
};
pub use dependency::{Blocker, BlockerStatus, Dependency, WorkRef};
pub use error::DomainError;
pub use habit::{Habit, HabitCadence};
pub use habit_tracking::{
    HabitOccurrence, HabitPeriod, HabitPeriodResult, HabitStreakSummary, evaluate_habit_periods,
};
pub use ids::{
    AcceptanceCriterionId, ActivityId, AttachmentId, HabitId, HabitOccurrenceId, KnowledgeItemId,
    KnowledgeResourceId, MilestoneId, ProjectId, RoutineId, ScheduleBlockId, TaskId,
};
pub use knowledge::{
    KnowledgeItem, KnowledgeKind, KnowledgeProvenance, KnowledgeRelation, KnowledgeRelationKind,
    KnowledgeStatus, KnowledgeTarget,
};
pub use knowledge_resource::{KnowledgeResource, LocalResourceLocator};
pub use milestone::{Milestone, MilestoneStatus};
pub use progress::{ProgressBasisPoints, ProgressCalculator, ProgressSource};
pub use project::{
    Project, ProjectMode, ProjectProgressSignal, ProjectStatus, ProjectSuccessCriterion,
    SuccessCriterionStatus,
};
pub use routine::Routine;
pub use schedule::{ScheduleBlock, ScheduleFlexibility, ScheduleTarget};
pub use task::{
    MAX_TASK_DESCRIPTION_CHARS, MAX_TASK_LABEL_CHARS, MAX_TASK_LABELS, Task, TaskDetails,
    TaskPlanning, TaskPriority, TaskScope, TaskStatus,
};
