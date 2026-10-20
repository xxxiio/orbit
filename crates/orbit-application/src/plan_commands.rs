use std::collections::{BTreeMap, BTreeSet};

use orbit_domain::{
    ActivityId, ActivityKind, ActivityOrigin, ActivityRecord, ActivitySubject, HabitId,
    ScheduleBlock, ScheduleBlockId, ScheduleFlexibility, ScheduleTarget, TaskId,
};
use orbit_planner::{
    CapacityPolicy, FixedBlockBufferPolicy, HabitPlanRequest, PlanningWindow,
    ValidatedHabitPlanProposal, ValidatedPlanProposal, validate_habit_proposal_with_buffers,
    validate_proposal_with_buffers,
};

use crate::{
    ActivityRepository, ApplicationError, Clock, HabitOccurrenceRepository, HabitRepository,
    PendingSemanticChange, ScheduleRepository, SemanticEntityKind, TaskRepository, UnitOfWork,
    enqueue_if_sync_enabled, sync_field,
};

fn schedule_block_semantic_change(
    block: &ScheduleBlock,
) -> Result<PendingSemanticChange, crate::RepositoryError> {
    let (target_kind, target_id) = match block.target() {
        ScheduleTarget::Standalone => ("standalone", None),
        ScheduleTarget::Task(id) => ("task", Some(id.to_string())),
        ScheduleTarget::Habit(id) => ("habit", Some(id.to_string())),
    };
    PendingSemanticChange::new(
        SemanticEntityKind::ScheduleBlock,
        block.id().to_string(),
        vec![
            sync_field("title", Some(block.title().to_owned()))?,
            sync_field("starts_at_ms", Some(block.starts_at_ms().to_string()))?,
            sync_field(
                "duration_minutes",
                Some(block.duration_minutes().to_string()),
            )?,
            sync_field(
                "flexibility",
                Some(
                    match block.flexibility() {
                        ScheduleFlexibility::Fixed => "fixed",
                        ScheduleFlexibility::Movable => "movable",
                    }
                    .to_owned(),
                ),
            )?,
            sync_field("target_kind", Some(target_kind.to_owned()))?,
            sync_field("target_id", target_id)?,
        ],
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanApplicationIdentity {
    pub task_id: TaskId,
    pub schedule_block_id: ScheduleBlockId,
    pub activity_id: ActivityId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplyValidatedPlanCommand {
    pub window: PlanningWindow,
    pub policy: CapacityPolicy,
    pub fixed_buffers: FixedBlockBufferPolicy,
    pub proposal: ValidatedPlanProposal,
    pub identities: Vec<PlanApplicationIdentity>,
    pub origin: ActivityOrigin,
}

/// Applies only a previously validated proposal, and re-validates it against the
/// current canonical task/schedule snapshot inside the transaction. This closes
/// the stale-plan gap between preview and apply.
pub fn apply_validated_plan(
    unit_of_work: &mut impl UnitOfWork,
    clock: &impl Clock,
    command: ApplyValidatedPlanCommand,
) -> Result<Vec<ScheduleBlock>, ApplicationError> {
    unit_of_work.transact(|repositories| {
        let tasks = TaskRepository::list(repositories)?;
        let blocks = ScheduleRepository::list_between(
            repositories,
            command.window.start_ms(),
            command.window.end_ms(),
        )?;
        let proposal = command.proposal.proposal().clone();
        validate_proposal_with_buffers(
            command.window,
            &blocks,
            &tasks,
            command.policy,
            command.fixed_buffers,
            proposal.clone(),
        )
        .map_err(|violations| ApplicationError::PlanRejected { violations })?;

        let expected_tasks: BTreeSet<_> = proposal
            .scheduled
            .iter()
            .map(|item| item.task_id.clone())
            .collect();
        let identity_tasks: BTreeSet<_> = command
            .identities
            .iter()
            .map(|identity| identity.task_id.clone())
            .collect();
        if expected_tasks != identity_tasks || command.identities.len() != expected_tasks.len() {
            return Err(ApplicationError::InconsistentReference {
                relation: "plan_application_identities",
            });
        }

        let identity_map: BTreeMap<_, _> = command
            .identities
            .iter()
            .map(|identity| (identity.task_id.clone(), identity))
            .collect();
        let task_map: BTreeMap<_, _> = tasks.iter().map(|task| (task.id().clone(), task)).collect();

        let mut created = Vec::with_capacity(proposal.scheduled.len());
        for item in &proposal.scheduled {
            let identity =
                identity_map
                    .get(&item.task_id)
                    .ok_or(ApplicationError::InconsistentReference {
                        relation: "plan_application_identities",
                    })?;
            if ScheduleRepository::get(repositories, &identity.schedule_block_id)?.is_some() {
                return Err(ApplicationError::AlreadyExists {
                    entity: "schedule_block",
                    id: identity.schedule_block_id.to_string(),
                });
            }
            if ActivityRepository::get(repositories, &identity.activity_id)?.is_some() {
                return Err(ApplicationError::AlreadyExists {
                    entity: "activity",
                    id: identity.activity_id.to_string(),
                });
            }
            let task = task_map
                .get(&item.task_id)
                .ok_or_else(|| ApplicationError::NotFound {
                    entity: "task",
                    id: item.task_id.to_string(),
                })?;
            let block = ScheduleBlock::new(
                identity.schedule_block_id.clone(),
                task.title(),
                item.start_ms,
                item.duration_minutes(),
                ScheduleFlexibility::Movable,
                ScheduleTarget::Task(item.task_id.clone()),
            )?;
            ScheduleRepository::save(repositories, block.clone())?;
            ActivityRepository::append(
                repositories,
                ActivityRecord::new(
                    identity.activity_id.clone(),
                    ActivitySubject::ScheduleBlock(block.id().clone()),
                    ActivityKind::Scheduled,
                    command.origin.clone(),
                    clock.now_ms(),
                    format!("Scheduled task {} from validated plan", task.id()),
                )?,
            )?;
            enqueue_if_sync_enabled(
                repositories,
                clock.now_ms(),
                schedule_block_semantic_change(&block)?,
            )?;
            created.push(block);
        }

        Ok(created)
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HabitPlanApplicationIdentity {
    pub habit_id: HabitId,
    pub occurrence_index: u32,
    pub schedule_block_id: ScheduleBlockId,
    pub activity_id: ActivityId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplyValidatedHabitPlanCommand {
    pub window: PlanningWindow,
    pub period_start_ms: i64,
    pub period_end_ms: i64,
    pub policy: CapacityPolicy,
    pub fixed_buffers: FixedBlockBufferPolicy,
    pub requests: Vec<HabitPlanRequest>,
    pub proposal: ValidatedHabitPlanProposal,
    pub identities: Vec<HabitPlanApplicationIdentity>,
    pub origin: ActivityOrigin,
}

/// Applies habit proposals directly as `ScheduleTarget::Habit` blocks. The
/// cadence count and active state are re-read inside the transaction so a habit
/// completion recorded after preview can invalidate a stale proposal.
pub fn apply_validated_habit_plan(
    unit_of_work: &mut impl UnitOfWork,
    clock: &impl Clock,
    command: ApplyValidatedHabitPlanCommand,
) -> Result<Vec<ScheduleBlock>, ApplicationError> {
    unit_of_work.transact(|repositories| {
        if command.period_end_ms <= command.period_start_ms {
            return Err(ApplicationError::InconsistentReference {
                relation: "habit_plan_period",
            });
        }
        let blocks = ScheduleRepository::list_between(
            repositories,
            command.window.start_ms(),
            command.window.end_ms(),
        )?;
        let mut canonical_requests = Vec::with_capacity(command.requests.len());
        let mut habits = BTreeMap::new();
        for request in &command.requests {
            let habit =
                HabitRepository::get(repositories, &request.habit_id)?.ok_or_else(|| {
                    ApplicationError::NotFound {
                        entity: "habit",
                        id: request.habit_id.to_string(),
                    }
                })?;
            let completed = HabitOccurrenceRepository::list_for_habit(repositories, habit.id())?
                .into_iter()
                .filter(|item| {
                    item.occurred_at_ms() >= command.period_start_ms
                        && item.occurred_at_ms() < command.period_end_ms
                })
                .count() as u32;
            canonical_requests.push(HabitPlanRequest {
                habit_id: habit.id().clone(),
                cadence: habit.cadence(),
                active: habit.is_active(),
                completed_in_period: completed,
                duration_per_occurrence_minutes: request.duration_per_occurrence_minutes,
            });
            habits.insert(habit.id().clone(), habit);
        }
        let proposal = command.proposal.proposal().clone();
        validate_habit_proposal_with_buffers(
            command.window,
            &blocks,
            &canonical_requests,
            command.policy,
            command.fixed_buffers,
            proposal.clone(),
        )
        .map_err(|violations| ApplicationError::HabitPlanRejected { violations })?;

        let expected: BTreeSet<_> = proposal
            .scheduled
            .iter()
            .map(|item| (item.habit_id.clone(), item.occurrence_index))
            .collect();
        let supplied: BTreeSet<_> = command
            .identities
            .iter()
            .map(|item| (item.habit_id.clone(), item.occurrence_index))
            .collect();
        if expected != supplied || command.identities.len() != expected.len() {
            return Err(ApplicationError::InconsistentReference {
                relation: "habit_plan_application_identities",
            });
        }
        let identity_map: BTreeMap<_, _> = command
            .identities
            .iter()
            .map(|item| ((item.habit_id.clone(), item.occurrence_index), item))
            .collect();
        let mut created = Vec::with_capacity(proposal.scheduled.len());
        for item in &proposal.scheduled {
            let identity = identity_map
                .get(&(item.habit_id.clone(), item.occurrence_index))
                .ok_or(ApplicationError::InconsistentReference {
                    relation: "habit_plan_application_identities",
                })?;
            if ScheduleRepository::get(repositories, &identity.schedule_block_id)?.is_some() {
                return Err(ApplicationError::AlreadyExists {
                    entity: "schedule_block",
                    id: identity.schedule_block_id.to_string(),
                });
            }
            if ActivityRepository::get(repositories, &identity.activity_id)?.is_some() {
                return Err(ApplicationError::AlreadyExists {
                    entity: "activity",
                    id: identity.activity_id.to_string(),
                });
            }
            let habit = habits
                .get(&item.habit_id)
                .ok_or_else(|| ApplicationError::NotFound {
                    entity: "habit",
                    id: item.habit_id.to_string(),
                })?;
            let block = ScheduleBlock::new(
                identity.schedule_block_id.clone(),
                habit.name(),
                item.start_ms,
                item.duration_minutes(),
                ScheduleFlexibility::Movable,
                ScheduleTarget::Habit(item.habit_id.clone()),
            )?;
            ScheduleRepository::save(repositories, block.clone())?;
            ActivityRepository::append(
                repositories,
                ActivityRecord::new(
                    identity.activity_id.clone(),
                    ActivitySubject::ScheduleBlock(block.id().clone()),
                    ActivityKind::Scheduled,
                    command.origin.clone(),
                    clock.now_ms(),
                    format!("Scheduled habit {} from validated habit plan", habit.id()),
                )?,
            )?;
            enqueue_if_sync_enabled(
                repositories,
                clock.now_ms(),
                schedule_block_semantic_change(&block)?,
            )?;
            created.push(block);
        }
        Ok(created)
    })
}
