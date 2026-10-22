use orbit_domain::DomainError;
use orbit_planner::{HabitPlanViolation, PlanViolation};
use orbit_progress::ProgressError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RepositoryError {
    Unavailable,
    Conflict,
    CorruptData { detail: String },
    Other(String),
}

impl std::fmt::Display for RepositoryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApplicationError {
    Domain(DomainError),
    Repository(RepositoryError),
    AlreadyExists { entity: &'static str, id: String },
    NotFound { entity: &'static str, id: String },
    InconsistentReference { relation: &'static str },
    PlanRejected { violations: Vec<PlanViolation> },
    HabitPlanRejected { violations: Vec<HabitPlanViolation> },
    Progress(ProgressError),
    AttachmentTooLarge { max_bytes: u64 },
    AttachmentContentUnavailable { object: String },
    AttachmentStore(String),
}

impl From<DomainError> for ApplicationError {
    fn from(value: DomainError) -> Self {
        Self::Domain(value)
    }
}

impl From<RepositoryError> for ApplicationError {
    fn from(value: RepositoryError) -> Self {
        Self::Repository(value)
    }
}

impl From<ProgressError> for ApplicationError {
    fn from(value: ProgressError) -> Self {
        Self::Progress(value)
    }
}
