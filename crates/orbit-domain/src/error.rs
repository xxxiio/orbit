use std::fmt;

/// Deterministic domain validation failures.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DomainError {
    EmptyIdentifier {
        kind: &'static str,
    },
    EmptyTaskTitle,
    InvalidTaskEstimate,
    InvalidTaskPlanningWindow,
    EmptyProjectName,
    EmptyProjectObjective,
    InvalidProjectMode,
    InvalidProjectProgressSignal,
    EmptyProjectProgressSignals,
    EmptySuccessCriterionKey,
    EmptySuccessCriterionDescription,
    DuplicateSuccessCriterionKey,
    UnknownSuccessCriterion,
    InvalidSuccessCriterionStatus,
    ProjectCompletionCriteriaUnsatisfied,
    EmptyMilestoneName,
    EmptyHabitName,
    EmptyRoutineName,
    DuplicateHabitInRoutine,
    InvalidHabitCadence,
    InvalidHabitOccurrenceDuration,
    InvalidHabitPeriod,
    OverlappingHabitPeriods,
    EmptyScheduleBlockTitle,
    InvalidScheduleDuration,
    FixedScheduleBlockCannotMove,
    MilestoneRequiresProject,
    InvalidTaskStatusTransition {
        from: &'static str,
        to: &'static str,
    },
    InvalidProjectStatusTransition {
        from: &'static str,
        to: &'static str,
    },
    InvalidMilestoneStatusTransition {
        from: &'static str,
        to: &'static str,
    },
    SelfDependency,
    EmptyBlockerReason,
    ProgressOutOfRange,
    InvalidTaskDerivedProgress,
    InvalidNumericProgress,
    EmptyProgressSourceName,
    EmptyActivitySummary,
    EmptyKnowledgeTitle,
    EmptyKnowledgeProvenance,
    KnowledgeTimestampRegression,
    InvalidKnowledgeStatusTransition {
        from: &'static str,
        to: &'static str,
    },
    SelfKnowledgeRelation,
    DuplicateKnowledgeRelation,
    EmptyKnowledgeResourceName,
    EmptyKnowledgeResourceMediaType,
    EmptyKnowledgeResourceHash,
    EmptyLocalResourcePath,
}

impl fmt::Display for DomainError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyIdentifier { kind } => write!(f, "{kind} identifier must not be empty"),
            Self::EmptyTaskTitle => write!(f, "task title must not be empty"),
            Self::InvalidTaskEstimate => {
                write!(f, "task estimate must be greater than zero minutes")
            }
            Self::InvalidTaskPlanningWindow => {
                write!(f, "task earliest start must not be after its deadline")
            }
            Self::EmptyProjectName => write!(f, "project name must not be empty"),
            Self::EmptyProjectObjective => {
                write!(f, "project objective must not be blank when supplied")
            }
            Self::InvalidProjectMode => write!(f, "project mode is not supported"),
            Self::InvalidProjectProgressSignal => {
                write!(f, "project progress signal is not supported")
            }
            Self::EmptyProjectProgressSignals => write!(
                f,
                "project must track at least one supported progress signal"
            ),
            Self::EmptySuccessCriterionKey => write!(f, "success criterion key must not be empty"),
            Self::EmptySuccessCriterionDescription => {
                write!(f, "success criterion description must not be empty")
            }
            Self::DuplicateSuccessCriterionKey => {
                write!(f, "success criterion keys must be unique within a project")
            }
            Self::UnknownSuccessCriterion => write!(f, "success criterion does not exist"),
            Self::InvalidSuccessCriterionStatus => {
                write!(f, "success criterion status is not supported")
            }
            Self::ProjectCompletionCriteriaUnsatisfied => write!(
                f,
                "required project completion criteria are not satisfied or waived"
            ),
            Self::EmptyMilestoneName => write!(f, "milestone name must not be empty"),
            Self::EmptyHabitName => write!(f, "habit name must not be empty"),
            Self::EmptyRoutineName => write!(f, "routine name must not be empty"),
            Self::DuplicateHabitInRoutine => write!(f, "habit is already in routine"),
            Self::InvalidHabitCadence => write!(f, "habit cadence must be valid"),
            Self::InvalidHabitOccurrenceDuration => {
                write!(f, "habit occurrence duration must be greater than zero")
            }
            Self::InvalidHabitPeriod => write!(f, "habit period start must be before end"),
            Self::OverlappingHabitPeriods => write!(f, "habit periods must not overlap"),
            Self::EmptyScheduleBlockTitle => write!(f, "schedule block title must not be empty"),
            Self::InvalidScheduleDuration => {
                write!(f, "schedule duration must be greater than zero")
            }
            Self::FixedScheduleBlockCannotMove => {
                write!(f, "fixed schedule blocks cannot be rescheduled")
            }
            Self::MilestoneRequiresProject => {
                write!(f, "a milestone association requires a project association")
            }
            Self::InvalidTaskStatusTransition { from, to } => {
                write!(f, "invalid task status transition: {from} -> {to}")
            }
            Self::InvalidProjectStatusTransition { from, to } => {
                write!(f, "invalid project status transition: {from} -> {to}")
            }
            Self::InvalidMilestoneStatusTransition { from, to } => {
                write!(f, "invalid milestone status transition: {from} -> {to}")
            }
            Self::SelfDependency => write!(f, "an entity cannot depend on itself"),
            Self::EmptyBlockerReason => write!(f, "blocker reason must not be empty"),
            Self::ProgressOutOfRange => write!(f, "progress must be between 0 and 100 percent"),
            Self::InvalidTaskDerivedProgress => write!(
                f,
                "task-derived progress requires total > 0 and completed <= total"
            ),
            Self::InvalidNumericProgress => write!(
                f,
                "numeric progress requires finite non-negative current and positive target values"
            ),
            Self::EmptyProgressSourceName => write!(f, "progress source name must not be empty"),
            Self::EmptyActivitySummary => write!(f, "activity summary must not be empty"),
            Self::EmptyKnowledgeTitle => write!(f, "knowledge title must not be empty"),
            Self::EmptyKnowledgeProvenance => {
                write!(f, "knowledge provenance source must not be empty")
            }
            Self::KnowledgeTimestampRegression => {
                write!(f, "knowledge updated timestamp must not move backwards")
            }
            Self::InvalidKnowledgeStatusTransition { from, to } => {
                write!(f, "invalid knowledge status transition: {from} -> {to}")
            }
            Self::SelfKnowledgeRelation => write!(f, "knowledge item cannot relate to itself"),
            Self::DuplicateKnowledgeRelation => write!(f, "knowledge relation already exists"),
            Self::EmptyKnowledgeResourceName => {
                write!(f, "knowledge resource display name must not be empty")
            }
            Self::EmptyKnowledgeResourceMediaType => write!(
                f,
                "knowledge resource media type must not be empty when supplied"
            ),
            Self::EmptyKnowledgeResourceHash => write!(
                f,
                "knowledge resource content hash must not be empty when supplied"
            ),
            Self::EmptyLocalResourcePath => {
                write!(f, "local knowledge resource path must not be empty")
            }
        }
    }
}

impl std::error::Error for DomainError {}
