use crate::RepositoryError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SemanticEntityKind {
    Project,
    Milestone,
    Task,
    Habit,
    HabitOccurrence,
    Routine,
    ScheduleBlock,
    Metric,
}

impl SemanticEntityKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Project => "project",
            Self::Milestone => "milestone",
            Self::Task => "task",
            Self::Habit => "habit",
            Self::HabitOccurrence => "habit_occurrence",
            Self::Routine => "routine",
            Self::ScheduleBlock => "schedule_block",
            Self::Metric => "metric",
        }
    }

    pub fn parse(value: &str) -> Result<Self, RepositoryError> {
        match value {
            "project" => Ok(Self::Project),
            "milestone" => Ok(Self::Milestone),
            "task" => Ok(Self::Task),
            "habit" => Ok(Self::Habit),
            "habit_occurrence" => Ok(Self::HabitOccurrence),
            "routine" => Ok(Self::Routine),
            "schedule_block" => Ok(Self::ScheduleBlock),
            "metric" => Ok(Self::Metric),
            other => Err(RepositoryError::CorruptData {
                detail: format!("invalid semantic entity kind: {other}"),
            }),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticFieldMutation {
    pub field: String,
    pub value: Option<String>,
    /// Sync field version known by the author when this mutation was committed.
    pub base_device_id: Option<String>,
    pub base_sequence: Option<u64>,
}

impl SemanticFieldMutation {
    pub fn new(field: impl Into<String>, value: Option<String>) -> Result<Self, RepositoryError> {
        let field = field.into();
        if field.trim().is_empty() {
            return Err(RepositoryError::Other(
                "semantic field name cannot be blank".into(),
            ));
        }
        Ok(Self {
            field,
            value,
            base_device_id: None,
            base_sequence: None,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingSemanticChange {
    pub entity_kind: SemanticEntityKind,
    pub entity_id: String,
    pub mutations: Vec<SemanticFieldMutation>,
}

impl PendingSemanticChange {
    pub fn new(
        entity_kind: SemanticEntityKind,
        entity_id: impl Into<String>,
        mut mutations: Vec<SemanticFieldMutation>,
    ) -> Result<Self, RepositoryError> {
        let entity_id = entity_id.into();
        if entity_id.trim().is_empty() {
            return Err(RepositoryError::Other(
                "semantic entity id cannot be blank".into(),
            ));
        }
        if mutations.is_empty() {
            return Err(RepositoryError::Other(
                "semantic change must contain at least one field mutation".into(),
            ));
        }
        mutations.sort_by(|a, b| a.field.cmp(&b.field));
        for pair in mutations.windows(2) {
            if pair[0].field == pair[1].field {
                return Err(RepositoryError::Other(format!(
                    "duplicate semantic field mutation: {}",
                    pair[0].field
                )));
            }
        }
        Ok(Self {
            entity_kind,
            entity_id,
            mutations,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingConflictResolution {
    /// Transport-independent JSON representation of the stable conflict identity.
    /// The application layer keeps this opaque so merge semantics remain owned by orbit-sync.
    pub conflict_id_json: String,
    pub value: Option<String>,
}

impl PendingConflictResolution {
    pub fn new(
        conflict_id_json: impl Into<String>,
        value: Option<String>,
    ) -> Result<Self, RepositoryError> {
        let conflict_id_json = conflict_id_json.into();
        if conflict_id_json.trim().is_empty() {
            return Err(RepositoryError::Other(
                "sync conflict id cannot be blank".into(),
            ));
        }
        Ok(Self {
            conflict_id_json,
            value,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReservedConflictResolution {
    pub device_id: String,
    pub sequence: u64,
    pub recorded_at_ms: i64,
    pub resolution: PendingConflictResolution,
    pub exported_at_ms: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncFieldVersion {
    pub entity_kind: SemanticEntityKind,
    pub entity_id: String,
    pub field: String,
    pub device_id: String,
    pub sequence: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReservedSemanticChange {
    pub device_id: String,
    pub sequence: u64,
    pub recorded_at_ms: i64,
    pub change: PendingSemanticChange,
    pub exported_at_ms: Option<i64>,
}

pub trait SyncOutboxRepository {
    fn initialize_sync_device(&mut self, device_id: &str) -> Result<(), RepositoryError>;
    fn sync_device(&self) -> Result<Option<(String, u64)>, RepositoryError>;
    fn enqueue_semantic_change(
        &mut self,
        recorded_at_ms: i64,
        change: PendingSemanticChange,
    ) -> Result<ReservedSemanticChange, RepositoryError>;
    fn list_pending_semantic_changes(&self)
    -> Result<Vec<ReservedSemanticChange>, RepositoryError>;
    fn enqueue_conflict_resolution(
        &mut self,
        recorded_at_ms: i64,
        resolution: PendingConflictResolution,
    ) -> Result<ReservedConflictResolution, RepositoryError>;
    fn list_pending_conflict_resolutions(
        &self,
    ) -> Result<Vec<ReservedConflictResolution>, RepositoryError>;
    fn mark_conflict_resolution_exported(
        &mut self,
        device_id: &str,
        sequence: u64,
        exported_at_ms: i64,
    ) -> Result<(), RepositoryError>;
    fn mark_semantic_change_exported(
        &mut self,
        device_id: &str,
        sequence: u64,
        exported_at_ms: i64,
    ) -> Result<(), RepositoryError>;
    fn set_known_sync_field_version(
        &mut self,
        version: SyncFieldVersion,
    ) -> Result<(), RepositoryError>;
}

pub fn field(
    field: impl Into<String>,
    value: Option<String>,
) -> Result<SemanticFieldMutation, RepositoryError> {
    SemanticFieldMutation::new(field, value)
}

pub fn enqueue_if_sync_enabled(
    repository: &mut impl SyncOutboxRepository,
    recorded_at_ms: i64,
    change: PendingSemanticChange,
) -> Result<Option<ReservedSemanticChange>, RepositoryError> {
    if repository.sync_device()?.is_none() {
        return Ok(None);
    }
    repository
        .enqueue_semantic_change(recorded_at_ms, change)
        .map(Some)
}
