use orbit_domain::{
    ActivityId, ActivityKind, ActivityOrigin, ActivityRecord, ActivitySubject, ScheduleBlock,
    ScheduleBlockId, ScheduleFlexibility, ScheduleTarget,
};

use crate::{
    ActivityRepository, ApplicationError, Clock, HabitRepository, PendingSemanticChange,
    ScheduleRepository, SemanticEntityKind, TaskRepository, UnitOfWork, enqueue_if_sync_enabled,
    sync_field,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateScheduleBlockCommand {
    pub id: ScheduleBlockId,
    pub title: String,
    pub starts_at_ms: i64,
    pub duration_minutes: u32,
    pub flexibility: ScheduleFlexibility,
    pub target: ScheduleTarget,
}

pub fn validate_schedule_target(
    tasks: &impl TaskRepository,
    habits: &impl HabitRepository,
    target: &ScheduleTarget,
) -> Result<(), ApplicationError> {
    match target {
        ScheduleTarget::Standalone => Ok(()),
        ScheduleTarget::Task(id) => {
            if tasks.get(id)?.is_some() {
                Ok(())
            } else {
                Err(ApplicationError::NotFound {
                    entity: "task",
                    id: id.to_string(),
                })
            }
        }
        ScheduleTarget::Habit(id) => {
            if habits.get(id)?.is_some() {
                Ok(())
            } else {
                Err(ApplicationError::NotFound {
                    entity: "habit",
                    id: id.to_string(),
                })
            }
        }
    }
}

pub fn create_schedule_block(
    schedules: &mut impl ScheduleRepository,
    tasks: &impl TaskRepository,
    habits: &impl HabitRepository,
    command: CreateScheduleBlockCommand,
) -> Result<ScheduleBlock, ApplicationError> {
    if schedules.get(&command.id)?.is_some() {
        return Err(ApplicationError::AlreadyExists {
            entity: "schedule_block",
            id: command.id.to_string(),
        });
    }

    validate_schedule_target(tasks, habits, &command.target)?;
    let block = ScheduleBlock::new(
        command.id,
        command.title,
        command.starts_at_ms,
        command.duration_minutes,
        command.flexibility,
        command.target,
    )?;
    schedules.save(block.clone())?;
    Ok(block)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateScheduleBlockWithActivityCommand {
    pub block: CreateScheduleBlockCommand,
    pub activity_id: ActivityId,
    pub origin: ActivityOrigin,
    pub summary: String,
}

pub fn create_schedule_block_with_activity(
    unit_of_work: &mut impl UnitOfWork,
    clock: &impl Clock,
    command: CreateScheduleBlockWithActivityCommand,
) -> Result<ScheduleBlock, ApplicationError> {
    unit_of_work.transact(|repositories| {
        if ScheduleRepository::get(repositories, &command.block.id)?.is_some() {
            return Err(ApplicationError::AlreadyExists {
                entity: "schedule_block",
                id: command.block.id.to_string(),
            });
        }
        if ActivityRepository::get(repositories, &command.activity_id)?.is_some() {
            return Err(ApplicationError::AlreadyExists {
                entity: "activity",
                id: command.activity_id.to_string(),
            });
        }

        validate_schedule_target(repositories, repositories, &command.block.target)?;
        let block = ScheduleBlock::new(
            command.block.id.clone(),
            command.block.title.clone(),
            command.block.starts_at_ms,
            command.block.duration_minutes,
            command.block.flexibility,
            command.block.target.clone(),
        )?;
        ScheduleRepository::save(repositories, block.clone())?;
        ActivityRepository::append(
            repositories,
            ActivityRecord::new(
                command.activity_id.clone(),
                ActivitySubject::ScheduleBlock(block.id().clone()),
                ActivityKind::Scheduled,
                command.origin.clone(),
                clock.now_ms(),
                command.summary.clone(),
            )?,
        )?;
        let (target_kind, target_id) = match block.target() {
            ScheduleTarget::Standalone => ("standalone", None),
            ScheduleTarget::Task(id) => ("task", Some(id.to_string())),
            ScheduleTarget::Habit(id) => ("habit", Some(id.to_string())),
        };
        enqueue_if_sync_enabled(
            repositories,
            clock.now_ms(),
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
            )?,
        )?;
        Ok(block)
    })
}

pub fn reschedule_block(
    repository: &mut impl ScheduleRepository,
    id: &ScheduleBlockId,
    starts_at_ms: i64,
) -> Result<ScheduleBlock, ApplicationError> {
    let mut block = repository
        .get(id)?
        .ok_or_else(|| ApplicationError::NotFound {
            entity: "schedule_block",
            id: id.to_string(),
        })?;

    block.reschedule(starts_at_ms)?;
    repository.save(block.clone())?;
    Ok(block)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RescheduleBlockWithActivityCommand {
    pub id: ScheduleBlockId,
    pub starts_at_ms: i64,
    pub activity_id: ActivityId,
    pub origin: ActivityOrigin,
    pub summary: String,
}

pub fn reschedule_block_with_activity(
    unit_of_work: &mut impl UnitOfWork,
    clock: &impl Clock,
    command: RescheduleBlockWithActivityCommand,
) -> Result<ScheduleBlock, ApplicationError> {
    unit_of_work.transact(|repositories| {
        if ActivityRepository::get(repositories, &command.activity_id)?.is_some() {
            return Err(ApplicationError::AlreadyExists {
                entity: "activity",
                id: command.activity_id.to_string(),
            });
        }
        let block = reschedule_block(repositories, &command.id, command.starts_at_ms)?;
        ActivityRepository::append(
            repositories,
            ActivityRecord::new(
                command.activity_id.clone(),
                ActivitySubject::ScheduleBlock(block.id().clone()),
                ActivityKind::Rescheduled,
                command.origin.clone(),
                clock.now_ms(),
                command.summary.clone(),
            )?,
        )?;
        enqueue_if_sync_enabled(
            repositories,
            clock.now_ms(),
            PendingSemanticChange::new(
                SemanticEntityKind::ScheduleBlock,
                block.id().to_string(),
                vec![sync_field(
                    "starts_at_ms",
                    Some(block.starts_at_ms().to_string()),
                )?],
            )?,
        )?;
        Ok(block)
    })
}
