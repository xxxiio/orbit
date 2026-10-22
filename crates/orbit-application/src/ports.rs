use orbit_domain::{
    AcceptanceCriteria, AcceptanceCriterion, ActivityId, ActivityRecord, Attachment, AttachmentId,
    AttachmentOwner, Habit, HabitId, HabitOccurrence, HabitOccurrenceId, Milestone, MilestoneId,
    Project, ProjectId, Routine, RoutineId, ScheduleBlock, ScheduleBlockId, Task, TaskId,
};

use crate::{ApplicationError, RepositoryError, SyncOutboxRepository};

pub trait Clock {
    fn now_ms(&self) -> i64;
}

pub trait TaskRepository {
    fn get(&self, id: &TaskId) -> Result<Option<Task>, RepositoryError>;
    fn list(&self) -> Result<Vec<Task>, RepositoryError>;
    fn save(&mut self, task: Task) -> Result<(), RepositoryError>;
}

pub trait ProjectRepository {
    fn get(&self, id: &ProjectId) -> Result<Option<Project>, RepositoryError>;
    fn list(&self) -> Result<Vec<Project>, RepositoryError>;
    fn save(&mut self, project: Project) -> Result<(), RepositoryError>;
}

/// Local project evidence associations. Callers must publish these as semantic
/// changes before relying on them across devices.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectEvidenceBinding {
    Habit(HabitId),
    Metric(String),
}

pub trait ProjectEvidenceRepository {
    fn list_project_evidence(
        &self,
        project_id: &ProjectId,
    ) -> Result<Vec<ProjectEvidenceBinding>, RepositoryError>;
    fn add_project_evidence(
        &mut self,
        project_id: &ProjectId,
        binding: &ProjectEvidenceBinding,
    ) -> Result<(), RepositoryError>;
    fn remove_project_evidence(
        &mut self,
        project_id: &ProjectId,
        binding: &ProjectEvidenceBinding,
    ) -> Result<(), RepositoryError>;
}

/// A project-bound metric target with explicit time-stamped measurements.
/// Measurements are local-only until a semantic sync contract is introduced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetricTargetRecord {
    pub metric_id: String,
    pub baseline: i64,
    pub target: i64,
    pub direction: orbit_progress::MetricTargetDirection,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetricObservation {
    pub metric_id: String,
    pub observed_at_ms: i64,
    pub value: i64,
}

pub trait MetricEvidenceRepository {
    fn get_metric_target(
        &self,
        metric_id: &str,
    ) -> Result<Option<MetricTargetRecord>, RepositoryError>;
    fn put_metric_target(&mut self, target: &MetricTargetRecord) -> Result<(), RepositoryError>;
    fn append_metric_observation(
        &mut self,
        observation: &MetricObservation,
    ) -> Result<(), RepositoryError>;
    /// Materializer-only idempotent replacement following sync conflict resolution.
    fn put_metric_observation(
        &mut self,
        observation: &MetricObservation,
    ) -> Result<(), RepositoryError>;
    fn latest_metric_observation(
        &self,
        metric_id: &str,
        as_of_ms: i64,
    ) -> Result<Option<MetricObservation>, RepositoryError>;
}

pub trait MilestoneRepository {
    fn get(&self, id: &MilestoneId) -> Result<Option<Milestone>, RepositoryError>;
    fn list_for_project(&self, project_id: &ProjectId) -> Result<Vec<Milestone>, RepositoryError>;
    fn save(&mut self, milestone: Milestone) -> Result<(), RepositoryError>;
}

pub trait HabitRepository {
    fn get(&self, id: &HabitId) -> Result<Option<Habit>, RepositoryError>;
    fn list(&self) -> Result<Vec<Habit>, RepositoryError>;
    fn save(&mut self, habit: Habit) -> Result<(), RepositoryError>;
}

pub trait RoutineRepository {
    fn get(&self, id: &RoutineId) -> Result<Option<Routine>, RepositoryError>;
    fn list(&self) -> Result<Vec<Routine>, RepositoryError>;
    fn save(&mut self, routine: Routine) -> Result<(), RepositoryError>;
}

pub trait HabitOccurrenceRepository {
    fn get(&self, id: &HabitOccurrenceId) -> Result<Option<HabitOccurrence>, RepositoryError>;
    fn list_for_habit(&self, habit_id: &HabitId) -> Result<Vec<HabitOccurrence>, RepositoryError>;
    fn append(&mut self, occurrence: HabitOccurrence) -> Result<(), RepositoryError>;
}

pub trait ScheduleRepository {
    fn get(&self, id: &ScheduleBlockId) -> Result<Option<ScheduleBlock>, RepositoryError>;
    fn list_between(
        &self,
        start_ms: i64,
        end_ms: i64,
    ) -> Result<Vec<ScheduleBlock>, RepositoryError>;
    fn save(&mut self, block: ScheduleBlock) -> Result<(), RepositoryError>;
}

pub trait ActivityRepository {
    fn get(&self, id: &ActivityId) -> Result<Option<ActivityRecord>, RepositoryError>;
    fn list(&self) -> Result<Vec<ActivityRecord>, RepositoryError>;
    fn append(&mut self, activity: ActivityRecord) -> Result<(), RepositoryError>;
}

/// Ordered acceptance criteria are a separate aggregate from the task row so
/// each criterion keeps a stable identity and its own completion timestamp.
pub trait AcceptanceCriteriaRepository {
    fn list_acceptance_criteria(
        &self,
        task_id: &TaskId,
    ) -> Result<Vec<AcceptanceCriterion>, RepositoryError>;
    fn replace_acceptance_criteria(
        &mut self,
        task_id: &TaskId,
        criteria: &AcceptanceCriteria,
    ) -> Result<(), RepositoryError>;
}

/// Metadata for an immutable content object in the managed object store.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttachmentObjectRecord {
    pub object_hash: String,
    pub size_bytes: u64,
    pub created_at_ms: i64,
}

/// Attachment metadata and entity associations. Content bytes are never stored
/// here: they live in an `AttachmentContentStore`.
pub trait AttachmentRepository {
    fn get_attachment(&self, id: &AttachmentId) -> Result<Option<Attachment>, RepositoryError>;
    fn list_attachments(&self, owner: &AttachmentOwner)
    -> Result<Vec<Attachment>, RepositoryError>;
    fn save_attachment(&mut self, attachment: &Attachment) -> Result<(), RepositoryError>;
    fn delete_attachment(&self, id: &AttachmentId) -> Result<bool, RepositoryError>;
    fn attachment_object(
        &self,
        object_hash: &str,
    ) -> Result<Option<AttachmentObjectRecord>, RepositoryError>;
    fn record_attachment_object(
        &mut self,
        object: &AttachmentObjectRecord,
    ) -> Result<(), RepositoryError>;
    fn attachment_object_referenced(&self, object_hash: &str) -> Result<bool, RepositoryError>;
    fn delete_attachment_object(&mut self, object_hash: &str) -> Result<bool, RepositoryError>;
    fn unreferenced_attachment_objects(
        &self,
    ) -> Result<Vec<AttachmentObjectRecord>, RepositoryError>;
}

/// Content-addressed attachment storage failure. Implementations must never
/// expose the original filename as a filesystem path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AttachmentStoreError {
    TooLarge { max_bytes: u64 },
    Unavailable,
    Io(String),
}

impl std::fmt::Display for AttachmentStoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooLarge { max_bytes } => {
                write!(f, "attachment exceeds the {max_bytes} byte limit")
            }
            Self::Unavailable => write!(f, "attachment storage is unavailable"),
            Self::Io(detail) => write!(f, "attachment storage error: {detail}"),
        }
    }
}

/// One stored content object. `checksum` and `object` are both the lowercase
/// SHA-256 digest today; they remain separate fields so a future store can use
/// a different object identity without redefining the integrity contract.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredAttachmentContent {
    pub object: String,
    pub checksum: String,
    pub size_bytes: u64,
    pub created_at_ms: i64,
}

pub trait AttachmentContentStore {
    fn max_bytes(&self) -> u64;
    fn store(&mut self, content: &[u8]) -> Result<StoredAttachmentContent, AttachmentStoreError>;
    fn read(&self, object: &str) -> Result<Vec<u8>, AttachmentStoreError>;
    fn contains(&self, object: &str) -> Result<bool, AttachmentStoreError>;
    fn remove(&mut self, object: &str) -> Result<bool, AttachmentStoreError>;
}

/// Transaction boundary for application use cases that must persist state and
/// semantic activity atomically. Implementations must commit all writes on
/// success and roll every write back on error.
pub trait UnitOfWork:
    TaskRepository
    + ProjectRepository
    + ProjectEvidenceRepository
    + MetricEvidenceRepository
    + MilestoneRepository
    + HabitRepository
    + HabitOccurrenceRepository
    + RoutineRepository
    + ScheduleRepository
    + ActivityRepository
    + AcceptanceCriteriaRepository
    + AttachmentRepository
    + SyncOutboxRepository
{
    fn transact<T, F>(&mut self, operation: F) -> Result<T, ApplicationError>
    where
        Self: Sized,
        F: FnOnce(&mut Self) -> Result<T, ApplicationError>;
}
