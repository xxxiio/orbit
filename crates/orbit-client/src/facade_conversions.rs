//! Client-facing DTO conversion, validation and status parsing.
//! Shared by the embedded facade without introducing another authority path.
use super::*;

impl From<&ActivityRecord> for ClientChangeDto {
    fn from(activity: &ActivityRecord) -> Self {
        let (subject_kind, subject_id) = match activity.subject() {
            ActivitySubject::Project(id) => ("project", id.to_string()),
            ActivitySubject::Milestone(id) => ("milestone", id.to_string()),
            ActivitySubject::Task(id) => ("task", id.to_string()),
            ActivitySubject::Habit(id) => ("habit", id.to_string()),
            ActivitySubject::Knowledge(id) => ("knowledge", id.to_string()),
            ActivitySubject::Routine(id) => ("routine", id.to_string()),
            ActivitySubject::ScheduleBlock(id) => ("schedule_block", id.to_string()),
        };
        Self {
            activity_id: activity.id().to_string(),
            subject_kind: subject_kind.to_owned(),
            subject_id,
            change_kind: activity_kind_label(activity.kind()),
            origin: activity_origin_label(activity.origin()),
            occurred_at_ms: activity.occurred_at_ms(),
            summary: activity.summary().to_owned(),
        }
    }
}

/// Wire-neutral label for one semantic activity kind.
pub(super) fn activity_kind_label(kind: &ActivityKind) -> String {
    match kind {
        ActivityKind::Created => "created".to_owned(),
        ActivityKind::Updated => "updated".to_owned(),
        ActivityKind::StatusChanged => "status_changed".to_owned(),
        ActivityKind::ProgressUpdated => "progress_updated".to_owned(),
        ActivityKind::BlockerOpened => "blocker_opened".to_owned(),
        ActivityKind::BlockerResolved => "blocker_resolved".to_owned(),
        ActivityKind::Scheduled => "scheduled".to_owned(),
        ActivityKind::Rescheduled => "rescheduled".to_owned(),
        ActivityKind::Completed => "completed".to_owned(),
        ActivityKind::AttachmentAdded => "attachment_added".to_owned(),
        ActivityKind::AttachmentRemoved => "attachment_removed".to_owned(),
        ActivityKind::Custom(value) => format!("custom:{value}"),
    }
}

/// Wire-neutral label for one activity origin.
pub(super) fn activity_origin_label(origin: &ActivityOrigin) -> String {
    match origin {
        ActivityOrigin::Human => "human".to_owned(),
        ActivityOrigin::System => "system".to_owned(),
        ActivityOrigin::Agent(value) => format!("agent:{value}"),
        ActivityOrigin::External(value) => format!("external:{value}"),
    }
}

pub(super) fn authority_client_error(error: orbit_runtime::AuthorityError) -> ClientError {
    use orbit_runtime::AuthorityError;
    let code = match &error {
        AuthorityError::StaleRevision | AuthorityError::ImmutableConflict => {
            ClientErrorCode::Conflict
        }
        AuthorityError::Invalid(_) => ClientErrorCode::InvalidInput,
        AuthorityError::MigrationConflict | AuthorityError::IncompleteProjection => {
            ClientErrorCode::CorruptData
        }
        _ => ClientErrorCode::Unavailable,
    };
    ClientError {
        code,
        message: format!("{error:?}"),
    }
}

pub(super) fn markdown_client_error(error: impl std::fmt::Debug) -> ClientError {
    ClientError {
        code: ClientErrorCode::Conflict,
        message: format!("{error:?}"),
    }
}

pub(super) fn invalid_input(error: impl std::fmt::Debug) -> ClientError {
    ClientError {
        code: ClientErrorCode::InvalidInput,
        message: format!("{error:?}"),
    }
}

pub(super) fn new_activity_id() -> Result<ActivityId, ClientError> {
    ActivityId::new(generate_id("activity")).map_err(invalid_input)
}

pub(super) fn generate_id(prefix: &str) -> String {
    let sequence = CLIENT_ID_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    format!("{prefix}-{}-{}-{sequence}", now_ms(), process::id())
}

pub(super) fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(i64::MAX as u128) as i64
}

pub(super) fn parse_habit_cadence(value: &str) -> Result<HabitCadence, ClientError> {
    let normalized = value.trim().to_ascii_lowercase();
    if normalized == "daily" {
        return Ok(HabitCadence::Daily);
    }
    if let Some(target) = normalized.strip_prefix("weekly:") {
        let target_per_week: u8 = target.parse().map_err(|_| ClientError {
            code: ClientErrorCode::InvalidInput,
            message: format!("invalid habit cadence: {value}"),
        })?;
        return HabitCadence::Weekly { target_per_week }
            .validate()
            .map_err(invalid_input);
    }
    Err(ClientError {
        code: ClientErrorCode::InvalidInput,
        message: format!("unknown habit cadence: {value}"),
    })
}

pub(super) fn parse_schedule_flexibility(value: &str) -> Result<ScheduleFlexibility, ClientError> {
    match value.trim().to_ascii_lowercase().as_str() {
        "fixed" => Ok(ScheduleFlexibility::Fixed),
        "movable" => Ok(ScheduleFlexibility::Movable),
        _ => Err(ClientError {
            code: ClientErrorCode::InvalidInput,
            message: format!("unknown schedule flexibility: {value}"),
        }),
    }
}

pub(super) fn parse_schedule_target(
    kind: &str,
    id: Option<String>,
) -> Result<ScheduleTarget, ClientError> {
    match kind.trim().to_ascii_lowercase().as_str() {
        "standalone" => {
            if id.is_some() {
                return Err(ClientError {
                    code: ClientErrorCode::InvalidInput,
                    message: "standalone target must not include target_id".to_owned(),
                });
            }
            Ok(ScheduleTarget::Standalone)
        }
        "task" => Ok(ScheduleTarget::Task(
            TaskId::new(id.ok_or_else(|| ClientError {
                code: ClientErrorCode::InvalidInput,
                message: "task target requires target_id".to_owned(),
            })?)
            .map_err(invalid_input)?,
        )),
        "habit" => Ok(ScheduleTarget::Habit(
            HabitId::new(id.ok_or_else(|| ClientError {
                code: ClientErrorCode::InvalidInput,
                message: "habit target requires target_id".to_owned(),
            })?)
            .map_err(invalid_input)?,
        )),
        _ => Err(ClientError {
            code: ClientErrorCode::InvalidInput,
            message: format!("unknown schedule target kind: {kind}"),
        }),
    }
}

pub(super) fn capacity_policy(dto: &PlanningPolicyDto) -> Result<CapacityPolicy, ClientError> {
    CapacityPolicy::new(dto.protected_minutes, dto.max_new_work_minutes).map_err(invalid_input)
}

pub(super) fn buffer_policy(dto: &PlanningPolicyDto) -> FixedBlockBufferPolicy {
    FixedBlockBufferPolicy::new(
        dto.fixed_buffer_before_minutes,
        dto.fixed_buffer_after_minutes,
    )
}

pub(super) fn task_plan_dto(
    start_ms: i64,
    end_ms: i64,
    policy: PlanningPolicyDto,
    proposal: PlanProposal,
) -> TaskPlanProposalDto {
    TaskPlanProposalDto {
        start_ms,
        end_ms,
        policy,
        scheduled: proposal
            .scheduled
            .into_iter()
            .map(|item| ProposedTaskBlockDto {
                task_id: item.task_id.to_string(),
                start_ms: item.start_ms,
                end_ms: item.end_ms,
            })
            .collect(),
        unscheduled: proposal
            .unscheduled
            .into_iter()
            .map(|item| UnscheduledTaskDto {
                task_id: item.task_id.to_string(),
                reason: unscheduled_task_reason(item.reason).to_owned(),
            })
            .collect(),
    }
}

pub(super) fn task_plan_from_dto(dto: &TaskPlanProposalDto) -> Result<PlanProposal, ClientError> {
    Ok(PlanProposal {
        scheduled: dto
            .scheduled
            .iter()
            .map(|item| {
                Ok(ProposedTaskBlock {
                    task_id: TaskId::new(item.task_id.clone()).map_err(invalid_input)?,
                    start_ms: item.start_ms,
                    end_ms: item.end_ms,
                })
            })
            .collect::<Result<Vec<_>, ClientError>>()?,
        unscheduled: dto
            .unscheduled
            .iter()
            .map(|item| {
                Ok(UnscheduledTask {
                    task_id: TaskId::new(item.task_id.clone()).map_err(invalid_input)?,
                    reason: parse_unscheduled_task_reason(&item.reason)?,
                })
            })
            .collect::<Result<Vec<_>, ClientError>>()?,
    })
}

pub(super) fn unscheduled_task_reason(reason: UnscheduledReason) -> &'static str {
    match reason {
        UnscheduledReason::NotEligible => "not_eligible",
        UnscheduledReason::MissingEstimate => "missing_estimate",
        UnscheduledReason::CapacityExhausted => "capacity_exhausted",
        UnscheduledReason::NoFittingSlot => "no_fitting_slot",
    }
}

pub(super) fn parse_unscheduled_task_reason(value: &str) -> Result<UnscheduledReason, ClientError> {
    match value {
        "not_eligible" => Ok(UnscheduledReason::NotEligible),
        "missing_estimate" => Ok(UnscheduledReason::MissingEstimate),
        "capacity_exhausted" => Ok(UnscheduledReason::CapacityExhausted),
        "no_fitting_slot" => Ok(UnscheduledReason::NoFittingSlot),
        _ => Err(ClientError {
            code: ClientErrorCode::InvalidInput,
            message: format!("unknown unscheduled task reason: {value}"),
        }),
    }
}

pub(super) fn habit_plan_dto(
    start_ms: i64,
    end_ms: i64,
    period_start_ms: i64,
    period_end_ms: i64,
    policy: PlanningPolicyDto,
    requests: Vec<HabitPlanInputDto>,
    proposal: HabitPlanProposal,
) -> HabitPlanProposalDto {
    HabitPlanProposalDto {
        start_ms,
        end_ms,
        period_start_ms,
        period_end_ms,
        policy,
        requests,
        scheduled: proposal
            .scheduled
            .into_iter()
            .map(|item| ProposedHabitBlockDto {
                habit_id: item.habit_id.to_string(),
                occurrence_index: item.occurrence_index,
                start_ms: item.start_ms,
                end_ms: item.end_ms,
            })
            .collect(),
        unscheduled: proposal
            .unscheduled
            .into_iter()
            .map(|item| UnscheduledHabitDto {
                habit_id: item.habit_id.to_string(),
                remaining_occurrences: item.remaining_occurrences,
                reason: unscheduled_habit_reason(item.reason).to_owned(),
            })
            .collect(),
    }
}

pub(super) fn habit_plan_from_dto(
    dto: &HabitPlanProposalDto,
) -> Result<HabitPlanProposal, ClientError> {
    Ok(HabitPlanProposal {
        scheduled: dto
            .scheduled
            .iter()
            .map(|item| {
                Ok(ProposedHabitBlock {
                    habit_id: HabitId::new(item.habit_id.clone()).map_err(invalid_input)?,
                    occurrence_index: item.occurrence_index,
                    start_ms: item.start_ms,
                    end_ms: item.end_ms,
                })
            })
            .collect::<Result<Vec<_>, ClientError>>()?,
        unscheduled: dto
            .unscheduled
            .iter()
            .map(|item| {
                Ok(UnscheduledHabit {
                    habit_id: HabitId::new(item.habit_id.clone()).map_err(invalid_input)?,
                    remaining_occurrences: item.remaining_occurrences,
                    reason: parse_unscheduled_habit_reason(&item.reason)?,
                })
            })
            .collect::<Result<Vec<_>, ClientError>>()?,
    })
}

pub(super) fn unscheduled_habit_reason(reason: UnscheduledHabitReason) -> &'static str {
    match reason {
        UnscheduledHabitReason::Inactive => "inactive",
        UnscheduledHabitReason::NoRemainingCompletions => "no_remaining_completions",
        UnscheduledHabitReason::CapacityExhausted => "capacity_exhausted",
        UnscheduledHabitReason::NoFittingSlot => "no_fitting_slot",
        UnscheduledHabitReason::InvalidDuration => "invalid_duration",
    }
}

pub(super) fn parse_unscheduled_habit_reason(
    value: &str,
) -> Result<UnscheduledHabitReason, ClientError> {
    match value {
        "inactive" => Ok(UnscheduledHabitReason::Inactive),
        "no_remaining_completions" => Ok(UnscheduledHabitReason::NoRemainingCompletions),
        "capacity_exhausted" => Ok(UnscheduledHabitReason::CapacityExhausted),
        "no_fitting_slot" => Ok(UnscheduledHabitReason::NoFittingSlot),
        "invalid_duration" => Ok(UnscheduledHabitReason::InvalidDuration),
        _ => Err(ClientError {
            code: ClientErrorCode::InvalidInput,
            message: format!("unknown unscheduled habit reason: {value}"),
        }),
    }
}

pub(super) fn parse_task_status(value: &str) -> Result<TaskStatus, ClientError> {
    match value.trim().to_ascii_lowercase().replace('-', "_").as_str() {
        "inbox" => Ok(TaskStatus::Inbox),
        "planned" => Ok(TaskStatus::Planned),
        "ready" => Ok(TaskStatus::Ready),
        "in_progress" => Ok(TaskStatus::InProgress),
        "waiting" => Ok(TaskStatus::Waiting),
        "blocked" => Ok(TaskStatus::Blocked),
        "review" => Ok(TaskStatus::Review),
        "deferred" => Ok(TaskStatus::Deferred),
        "done" => Ok(TaskStatus::Done),
        "cancelled" | "canceled" => Ok(TaskStatus::Cancelled),
        _ => Err(ClientError {
            code: ClientErrorCode::InvalidInput,
            message: format!("unknown task status: {value}"),
        }),
    }
}

pub(super) fn parse_project_status(value: &str) -> Result<ProjectStatus, ClientError> {
    match value.trim().to_ascii_lowercase().replace('-', "_").as_str() {
        "proposed" => Ok(ProjectStatus::Proposed),
        "active" => Ok(ProjectStatus::Active),
        "on_hold" => Ok(ProjectStatus::OnHold),
        "completed" => Ok(ProjectStatus::Completed),
        "cancelled" | "canceled" => Ok(ProjectStatus::Cancelled),
        "archived" => Ok(ProjectStatus::Archived),
        _ => Err(ClientError {
            code: ClientErrorCode::InvalidInput,
            message: format!("unknown project status: {value}"),
        }),
    }
}

pub(super) fn parse_knowledge_status(value: &str) -> Result<KnowledgeStatus, ClientError> {
    match value.trim().to_ascii_lowercase().replace('-', "_").as_str() {
        "active" => Ok(KnowledgeStatus::Active),
        "resolved" => Ok(KnowledgeStatus::Resolved),
        "superseded" => Ok(KnowledgeStatus::Superseded),
        "archived" => Ok(KnowledgeStatus::Archived),
        _ => Err(ClientError {
            code: ClientErrorCode::InvalidInput,
            message: format!("unknown knowledge status: {value}"),
        }),
    }
}

pub(super) fn parse_knowledge_kind(value: &str) -> Result<KnowledgeKind, ClientError> {
    match value.trim().to_ascii_lowercase().replace('-', "_").as_str() {
        "note" => Ok(KnowledgeKind::Note),
        "decision" => Ok(KnowledgeKind::Decision),
        "finding" => Ok(KnowledgeKind::Finding),
        "assumption" => Ok(KnowledgeKind::Assumption),
        "risk" => Ok(KnowledgeKind::Risk),
        "reference" => Ok(KnowledgeKind::Reference),
        "concept" => Ok(KnowledgeKind::Concept),
        "resource" => Ok(KnowledgeKind::Resource),
        "meeting_note" => Ok(KnowledgeKind::MeetingNote),
        "daily_note" => Ok(KnowledgeKind::DailyNote),
        "project_context" => Ok(KnowledgeKind::ProjectContext),
        _ => Err(ClientError {
            code: ClientErrorCode::InvalidInput,
            message: format!("unknown knowledge kind: {value}"),
        }),
    }
}
