//! Embedded client DTOs and wire-neutral response contracts.
use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClientErrorCode {
    InvalidInput,
    NotFound,
    Conflict,
    Unavailable,
    CorruptData,
    Internal,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientError {
    pub code: ClientErrorCode,
    pub message: String,
}

impl From<RepositoryError> for ClientError {
    fn from(error: RepositoryError) -> Self {
        let code = match error {
            RepositoryError::Conflict => ClientErrorCode::Conflict,
            RepositoryError::Unavailable => ClientErrorCode::Unavailable,
            RepositoryError::CorruptData { .. } => ClientErrorCode::CorruptData,
            RepositoryError::Other(_) => ClientErrorCode::Internal,
        };
        Self {
            code,
            message: format!("{error:?}"),
        }
    }
}

impl From<ApplicationError> for ClientError {
    fn from(error: ApplicationError) -> Self {
        let code = match &error {
            ApplicationError::Domain(_)
            | ApplicationError::InconsistentReference { .. }
            | ApplicationError::PlanRejected { .. }
            | ApplicationError::HabitPlanRejected { .. }
            | ApplicationError::Progress(_) => ClientErrorCode::InvalidInput,
            ApplicationError::Repository(repository) => {
                return ClientError::from(repository.clone());
            }
            ApplicationError::AlreadyExists { .. } => ClientErrorCode::Conflict,
            ApplicationError::NotFound { .. } => ClientErrorCode::NotFound,
        };
        Self {
            code,
            message: format!("{error:?}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientBridgeContract {
    pub api_version: u32,
    pub primitive_dtos_only: bool,
    pub embedded_runtime: bool,
    pub refresh_after_sync_pull_required: bool,
    pub mobile_mcp_excluded: bool,
}

impl Default for ClientBridgeContract {
    fn default() -> Self {
        Self {
            api_version: ORBIT_CLIENT_API_VERSION,
            primitive_dtos_only: true,
            embedded_runtime: true,
            refresh_after_sync_pull_required: true,
            mobile_mcp_excluded: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientCapabilities {
    pub embedded_runtime: bool,
    pub structured_knowledge: bool,
    pub knowledge_resources: bool,
    pub deterministic_planning: bool,
    pub semantic_retrieval_optional: bool,
    pub durable_change_cursor: bool,
    pub external_secure_secret_store: bool,
    pub mobile_mcp: bool,
}

impl Default for ClientCapabilities {
    fn default() -> Self {
        Self {
            embedded_runtime: true,
            structured_knowledge: true,
            knowledge_resources: true,
            deterministic_planning: true,
            semantic_retrieval_optional: true,
            durable_change_cursor: true,
            external_secure_secret_store: true,
            mobile_mcp: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceSummaryDto {
    pub schema_version: i64,
    pub projects: usize,
    pub tasks: usize,
    pub habits: usize,
    pub routines: usize,
    pub knowledge_items: usize,
    pub knowledge_resources: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectSuccessCriterionDto {
    pub key: String,
    pub description: String,
    pub required: bool,
    pub status: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectDto {
    pub id: String,
    pub name: String,
    pub status: String,
    pub mode: String,
    pub objective: Option<String>,
    pub progress_signals: Vec<String>,
    pub success_criteria: Vec<ProjectSuccessCriterionDto>,
    pub completion_ready: bool,
}

impl From<&Project> for ProjectDto {
    fn from(project: &Project) -> Self {
        Self {
            id: project.id().to_string(),
            name: project.name().to_owned(),
            status: project.status().as_str().to_owned(),
            mode: project.mode().as_str().to_owned(),
            objective: project.objective().map(ToOwned::to_owned),
            progress_signals: project
                .progress_signals()
                .iter()
                .map(|signal| signal.as_str().to_owned())
                .collect(),
            success_criteria: project
                .success_criteria()
                .iter()
                .map(|criterion| ProjectSuccessCriterionDto {
                    key: criterion.key().to_owned(),
                    description: criterion.description().to_owned(),
                    required: criterion.required(),
                    status: criterion.status().as_str().to_owned(),
                })
                .collect(),
            completion_ready: project.required_completion_ready(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskDto {
    pub id: String,
    pub title: String,
    pub status: String,
    pub project_id: Option<String>,
    pub milestone_id: Option<String>,
    pub estimated_minutes: Option<u32>,
    pub earliest_start_at_ms: Option<i64>,
    pub deadline_at_ms: Option<i64>,
}

impl From<&Task> for TaskDto {
    fn from(task: &Task) -> Self {
        Self {
            id: task.id().to_string(),
            title: task.title().to_owned(),
            status: task.status().as_str().to_owned(),
            project_id: task.scope().project_id().map(ToString::to_string),
            milestone_id: task.scope().milestone_id().map(ToString::to_string),
            estimated_minutes: task.planning().estimated_minutes(),
            earliest_start_at_ms: task.planning().earliest_start_at_ms(),
            deadline_at_ms: task.planning().deadline_at_ms(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KnowledgeItemDto {
    pub id: String,
    pub kind: String,
    pub title: String,
    pub body_markdown: String,
    pub status: String,
    pub provenance: String,
    pub updated_at_ms: i64,
}

impl From<&KnowledgeItem> for KnowledgeItemDto {
    fn from(item: &KnowledgeItem) -> Self {
        let provenance = match item.provenance() {
            KnowledgeProvenance::Human => "human".to_owned(),
            KnowledgeProvenance::Agent(source) => format!("agent:{source}"),
            KnowledgeProvenance::Import(source) => format!("import:{source}"),
            KnowledgeProvenance::Automation(source) => format!("automation:{source}"),
        };
        Self {
            id: item.id().to_string(),
            kind: item.kind().as_str().to_owned(),
            title: item.title().to_owned(),
            body_markdown: item.body_markdown().to_owned(),
            status: item.status().as_str().to_owned(),
            provenance,
            updated_at_ms: item.updated_at_ms(),
        }
    }
}

// ---- FFI-friendly mutation DTOs ---------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateProjectRequest {
    pub id: Option<String>,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetProjectStatusRequest {
    pub id: String,
    pub status: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectCriterionInputDto {
    pub key: String,
    pub description: String,
    pub required: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigureProjectStrategyRequest {
    pub id: String,
    pub mode: String,
    pub objective: Option<String>,
    pub progress_signals: Vec<String>,
    pub success_criteria: Vec<ProjectCriterionInputDto>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetProjectCriterionStatusRequest {
    pub project_id: String,
    pub criterion_key: String,
    pub status: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateTaskRequest {
    pub id: Option<String>,
    pub title: String,
    pub project_id: Option<String>,
    pub milestone_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetTaskStatusRequest {
    pub id: String,
    pub status: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetTaskPlanningRequest {
    pub id: String,
    pub estimated_minutes: Option<u32>,
    pub earliest_start_at_ms: Option<i64>,
    pub deadline_at_ms: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateKnowledgeRequest {
    pub id: Option<String>,
    pub kind: String,
    pub title: String,
    pub body_markdown: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateKnowledgeRequest {
    pub id: String,
    pub title: String,
    pub body_markdown: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetKnowledgeStatusRequest {
    pub id: String,
    pub status: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HabitDto {
    pub id: String,
    pub name: String,
    pub cadence: String,
    pub active: bool,
}

impl From<&Habit> for HabitDto {
    fn from(habit: &Habit) -> Self {
        let cadence = match habit.cadence() {
            HabitCadence::Daily => "daily".to_owned(),
            HabitCadence::Weekly { target_per_week } => format!("weekly:{target_per_week}"),
        };
        Self {
            id: habit.id().to_string(),
            name: habit.name().to_owned(),
            cadence,
            active: habit.is_active(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HabitOccurrenceDto {
    pub id: String,
    pub habit_id: String,
    pub occurred_at_ms: i64,
    pub duration_minutes: Option<u32>,
}

impl From<&HabitOccurrence> for HabitOccurrenceDto {
    fn from(item: &HabitOccurrence) -> Self {
        Self {
            id: item.id().to_string(),
            habit_id: item.habit_id().to_string(),
            occurred_at_ms: item.occurred_at_ms(),
            duration_minutes: item.duration_minutes(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateHabitRequest {
    pub id: Option<String>,
    pub name: String,
    pub cadence: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetHabitActiveRequest {
    pub id: String,
    pub active: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordHabitOccurrenceRequest {
    pub id: Option<String>,
    pub habit_id: String,
    pub occurred_at_ms: Option<i64>,
    pub duration_minutes: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScheduleBlockDto {
    pub id: String,
    pub title: String,
    pub starts_at_ms: i64,
    pub ends_at_ms: i64,
    pub duration_minutes: u32,
    pub flexibility: String,
    pub target_kind: String,
    pub target_id: Option<String>,
}

impl From<&ScheduleBlock> for ScheduleBlockDto {
    fn from(block: &ScheduleBlock) -> Self {
        let flexibility = match block.flexibility() {
            ScheduleFlexibility::Fixed => "fixed",
            ScheduleFlexibility::Movable => "movable",
        }
        .to_owned();
        let (target_kind, target_id) = match block.target() {
            ScheduleTarget::Standalone => ("standalone".to_owned(), None),
            ScheduleTarget::Task(id) => ("task".to_owned(), Some(id.to_string())),
            ScheduleTarget::Habit(id) => ("habit".to_owned(), Some(id.to_string())),
        };
        Self {
            id: block.id().to_string(),
            title: block.title().to_owned(),
            starts_at_ms: block.starts_at_ms(),
            ends_at_ms: block.ends_at_ms(),
            duration_minutes: block.duration_minutes(),
            flexibility,
            target_kind,
            target_id,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateScheduleBlockRequest {
    pub id: Option<String>,
    pub title: String,
    pub starts_at_ms: i64,
    pub duration_minutes: u32,
    pub flexibility: String,
    pub target_kind: String,
    pub target_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TodaySnapshotDto {
    pub start_ms: i64,
    pub end_ms: i64,
    pub schedule: Vec<ScheduleBlockDto>,
    pub tasks: Vec<TaskDto>,
    pub habits: Vec<HabitDto>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PlanningPolicyDto {
    pub protected_minutes: u32,
    pub max_new_work_minutes: Option<u32>,
    pub fixed_buffer_before_minutes: u32,
    pub fixed_buffer_after_minutes: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProposedTaskBlockDto {
    pub task_id: String,
    pub start_ms: i64,
    pub end_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnscheduledTaskDto {
    pub task_id: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskPlanProposalDto {
    pub start_ms: i64,
    pub end_ms: i64,
    pub policy: PlanningPolicyDto,
    pub scheduled: Vec<ProposedTaskBlockDto>,
    pub unscheduled: Vec<UnscheduledTaskDto>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HabitPlanInputDto {
    pub habit_id: String,
    pub completed_in_period: u32,
    pub duration_per_occurrence_minutes: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProposedHabitBlockDto {
    pub habit_id: String,
    pub occurrence_index: u32,
    pub start_ms: i64,
    pub end_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnscheduledHabitDto {
    pub habit_id: String,
    pub remaining_occurrences: u32,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HabitPlanProposalDto {
    pub start_ms: i64,
    pub end_ms: i64,
    pub period_start_ms: i64,
    pub period_end_ms: i64,
    pub policy: PlanningPolicyDto,
    pub requests: Vec<HabitPlanInputDto>,
    pub scheduled: Vec<ProposedHabitBlockDto>,
    pub unscheduled: Vec<UnscheduledHabitDto>,
}

// ---- Durable change notification DTOs ---------------------------------------------------------

/// Cursor over Orbit's append-only semantic activity history.
///
/// The cursor is an activity count rather than a process-local generation. It
/// therefore survives app restarts and also observes changes made by CLI/MCP
/// processes that append to the same workspace activity journal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ClientChangeCursor {
    pub activity_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientChangeDto {
    pub activity_id: String,
    pub subject_kind: String,
    pub subject_id: String,
    pub change_kind: String,
    pub origin: String,
    pub occurred_at_ms: i64,
    pub summary: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientChangeBatch {
    pub cursor: ClientChangeCursor,
    pub changes: Vec<ClientChangeDto>,
}

/// Opaque reference to a secret held by a platform-owned secure store. Orbit's
/// normal SQLite state stores references/configuration only; raw secret values
/// must not be written to the Orbit database or semantic sync history.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ClientSecretRef {
    pub namespace: String,
    pub key: String,
}

impl ClientSecretRef {
    pub fn new(namespace: impl Into<String>, key: impl Into<String>) -> Result<Self, ClientError> {
        let namespace = namespace.into();
        let key = key.into();
        if namespace.trim().is_empty() || key.trim().is_empty() {
            return Err(ClientError {
                code: ClientErrorCode::InvalidInput,
                message: "secret namespace/key cannot be blank".to_owned(),
            });
        }
        Ok(Self { namespace, key })
    }
}

/// Platform adapter implemented by Flutter/mobile/desktop composition code.
/// Implementations may use Keychain, Android Keystore, Windows Credential
/// Manager, libsecret, or another OS-approved secure store.
pub trait SecureSecretStore {
    fn put(&mut self, reference: &ClientSecretRef, secret: &[u8]) -> Result<(), ClientError>;
    fn get(&self, reference: &ClientSecretRef) -> Result<Option<Vec<u8>>, ClientError>;
    fn delete(&mut self, reference: &ClientSecretRef) -> Result<(), ClientError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClientPlatform {
    Android,
    Ios,
    Macos,
    Windows,
    Linux,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientPlatformCapabilities {
    pub platform: ClientPlatform,
    pub mcp_available: bool,
    pub local_ai_endpoint_supported: bool,
    pub external_secure_secret_store_required: bool,
}

impl ClientPlatformCapabilities {
    pub fn for_platform(platform: ClientPlatform) -> Self {
        let desktop = matches!(
            platform,
            ClientPlatform::Macos | ClientPlatform::Windows | ClientPlatform::Linux
        );
        Self {
            platform,
            mcp_available: desktop,
            local_ai_endpoint_supported: desktop,
            external_secure_secret_store_required: true,
        }
    }
}

/// Primitive DTOs are suitable for FFI/Flutter bridges without exposing Rust enums.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectEvidenceBindingDto {
    pub kind: String,
    pub id: String,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectEvidenceProgressDto {
    pub project_id: String,
    pub task_progress_bp: u16,
    pub mode_progress_bp: Option<u16>,
    pub evidence_progress_bp: Option<u16>,
    pub habit_progress: Vec<(String, Option<u16>)>,
    pub metric_progress: Vec<(String, Option<u16>)>,
    pub available_signals: Vec<String>,
    pub unavailable_signals: Vec<String>,
    pub completion_ready: bool,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetricTargetDto {
    pub metric_id: String,
    pub baseline: i64,
    pub target: i64,
    pub direction: String,
}
