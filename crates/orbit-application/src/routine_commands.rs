use orbit_domain::{
    ActivityId, ActivityKind, ActivityOrigin, ActivityRecord, ActivitySubject, HabitId, Routine,
    RoutineId,
};

use crate::{
    ActivityRepository, ApplicationError, Clock, HabitRepository, PendingSemanticChange,
    RoutineRepository, SemanticEntityKind, UnitOfWork, enqueue_if_sync_enabled, sync_field,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateRoutineWithActivityCommand {
    pub id: RoutineId,
    pub name: String,
    pub activity_id: ActivityId,
    pub origin: ActivityOrigin,
    pub summary: String,
}

pub fn create_routine_with_activity(
    unit_of_work: &mut impl UnitOfWork,
    clock: &impl Clock,
    command: CreateRoutineWithActivityCommand,
) -> Result<Routine, ApplicationError> {
    unit_of_work.transact(|repositories| {
        if RoutineRepository::get(repositories, &command.id)?.is_some() {
            return Err(ApplicationError::AlreadyExists {
                entity: "routine",
                id: command.id.to_string(),
            });
        }
        if ActivityRepository::get(repositories, &command.activity_id)?.is_some() {
            return Err(ApplicationError::AlreadyExists {
                entity: "activity",
                id: command.activity_id.to_string(),
            });
        }
        let routine = Routine::new(command.id.clone(), command.name.clone())?;
        RoutineRepository::save(repositories, routine.clone())?;
        ActivityRepository::append(
            repositories,
            ActivityRecord::new(
                command.activity_id.clone(),
                ActivitySubject::Routine(routine.id().clone()),
                ActivityKind::Created,
                command.origin.clone(),
                clock.now_ms(),
                command.summary.clone(),
            )?,
        )?;
        enqueue_if_sync_enabled(
            repositories,
            clock.now_ms(),
            PendingSemanticChange::new(
                SemanticEntityKind::Routine,
                routine.id().to_string(),
                vec![sync_field("name", Some(routine.name().to_owned()))?],
            )?,
        )?;
        Ok(routine)
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetRoutineHabitMembershipWithActivityCommand {
    pub routine_id: RoutineId,
    pub habit_id: HabitId,
    pub present: bool,
    pub activity_id: ActivityId,
    pub origin: ActivityOrigin,
    pub summary: String,
}

pub fn set_routine_habit_membership_with_activity(
    unit_of_work: &mut impl UnitOfWork,
    clock: &impl Clock,
    command: SetRoutineHabitMembershipWithActivityCommand,
) -> Result<Routine, ApplicationError> {
    unit_of_work.transact(|repositories| {
        if HabitRepository::get(repositories, &command.habit_id)?.is_none() {
            return Err(ApplicationError::NotFound {
                entity: "habit",
                id: command.habit_id.to_string(),
            });
        }
        if ActivityRepository::get(repositories, &command.activity_id)?.is_some() {
            return Err(ApplicationError::AlreadyExists {
                entity: "activity",
                id: command.activity_id.to_string(),
            });
        }
        let mut routine =
            RoutineRepository::get(repositories, &command.routine_id)?.ok_or_else(|| {
                ApplicationError::NotFound {
                    entity: "routine",
                    id: command.routine_id.to_string(),
                }
            })?;

        if command.present {
            if !routine.habit_ids().iter().any(|id| id == &command.habit_id) {
                routine.add_habit(command.habit_id.clone())?;
            }
        } else {
            routine.remove_habit(&command.habit_id);
        }
        RoutineRepository::save(repositories, routine.clone())?;
        ActivityRepository::append(
            repositories,
            ActivityRecord::new(
                command.activity_id.clone(),
                ActivitySubject::Routine(routine.id().clone()),
                ActivityKind::Updated,
                command.origin.clone(),
                clock.now_ms(),
                command.summary.clone(),
            )?,
        )?;
        let mut mutations = routine
            .habit_ids()
            .iter()
            .enumerate()
            .map(|(position, habit_id)| {
                sync_field(format!("habit/{habit_id}"), Some(position.to_string()))
            })
            .collect::<Result<Vec<_>, _>>()?;
        if !command.present {
            mutations.push(sync_field(format!("habit/{}", command.habit_id), None)?);
        }
        enqueue_if_sync_enabled(
            repositories,
            clock.now_ms(),
            PendingSemanticChange::new(
                SemanticEntityKind::Routine,
                routine.id().to_string(),
                mutations,
            )?,
        )?;
        Ok(routine)
    })
}
