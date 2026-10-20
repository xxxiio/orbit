use orbit_domain::{
    ActivityId, ActivityKind, ActivityOrigin, ActivityRecord, ActivitySubject, Habit, HabitCadence,
    HabitId,
};

use crate::{
    ActivityRepository, ApplicationError, Clock, HabitRepository, PendingSemanticChange,
    SemanticEntityKind, UnitOfWork, enqueue_if_sync_enabled, sync_field,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateHabitCommand {
    pub id: HabitId,
    pub name: String,
    pub cadence: HabitCadence,
}

pub fn create_habit(
    repository: &mut impl HabitRepository,
    command: CreateHabitCommand,
) -> Result<Habit, ApplicationError> {
    if repository.get(&command.id)?.is_some() {
        return Err(ApplicationError::AlreadyExists {
            entity: "habit",
            id: command.id.to_string(),
        });
    }

    let habit = Habit::new(command.id, command.name, command.cadence)?;
    repository.save(habit.clone())?;
    Ok(habit)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateHabitWithActivityCommand {
    pub habit: CreateHabitCommand,
    pub activity_id: ActivityId,
    pub origin: ActivityOrigin,
    pub summary: String,
}

pub fn create_habit_with_activity(
    unit_of_work: &mut impl UnitOfWork,
    clock: &impl Clock,
    command: CreateHabitWithActivityCommand,
) -> Result<Habit, ApplicationError> {
    unit_of_work.transact(|repositories| {
        if HabitRepository::get(repositories, &command.habit.id)?.is_some() {
            return Err(ApplicationError::AlreadyExists {
                entity: "habit",
                id: command.habit.id.to_string(),
            });
        }
        if ActivityRepository::get(repositories, &command.activity_id)?.is_some() {
            return Err(ApplicationError::AlreadyExists {
                entity: "activity",
                id: command.activity_id.to_string(),
            });
        }

        let habit = Habit::new(
            command.habit.id.clone(),
            command.habit.name.clone(),
            command.habit.cadence,
        )?;
        HabitRepository::save(repositories, habit.clone())?;
        ActivityRepository::append(
            repositories,
            ActivityRecord::new(
                command.activity_id.clone(),
                ActivitySubject::Habit(habit.id().clone()),
                ActivityKind::Created,
                command.origin.clone(),
                clock.now_ms(),
                command.summary.clone(),
            )?,
        )?;
        let cadence = match habit.cadence() {
            HabitCadence::Daily => "daily".to_owned(),
            HabitCadence::Weekly { target_per_week } => format!("weekly:{target_per_week}"),
        };
        enqueue_if_sync_enabled(
            repositories,
            clock.now_ms(),
            PendingSemanticChange::new(
                SemanticEntityKind::Habit,
                habit.id().to_string(),
                vec![
                    sync_field("name", Some(habit.name().to_owned()))?,
                    sync_field("cadence", Some(cadence))?,
                    sync_field("active", Some(habit.is_active().to_string()))?,
                ],
            )?,
        )?;
        Ok(habit)
    })
}

pub fn pause_habit(
    repository: &mut impl HabitRepository,
    id: &HabitId,
) -> Result<Habit, ApplicationError> {
    let mut habit = repository
        .get(id)?
        .ok_or_else(|| ApplicationError::NotFound {
            entity: "habit",
            id: id.to_string(),
        })?;

    habit.pause();
    repository.save(habit.clone())?;
    Ok(habit)
}

pub fn resume_habit(
    repository: &mut impl HabitRepository,
    id: &HabitId,
) -> Result<Habit, ApplicationError> {
    let mut habit = repository
        .get(id)?
        .ok_or_else(|| ApplicationError::NotFound {
            entity: "habit",
            id: id.to_string(),
        })?;

    habit.resume();
    repository.save(habit.clone())?;
    Ok(habit)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetHabitActiveWithActivityCommand {
    pub id: HabitId,
    pub active: bool,
    pub activity_id: ActivityId,
    pub origin: ActivityOrigin,
    pub summary: String,
}

pub fn set_habit_active_with_activity(
    unit_of_work: &mut impl UnitOfWork,
    clock: &impl Clock,
    command: SetHabitActiveWithActivityCommand,
) -> Result<Habit, ApplicationError> {
    unit_of_work.transact(|repositories| {
        if ActivityRepository::get(repositories, &command.activity_id)?.is_some() {
            return Err(ApplicationError::AlreadyExists {
                entity: "activity",
                id: command.activity_id.to_string(),
            });
        }

        let mut habit = HabitRepository::get(repositories, &command.id)?.ok_or_else(|| {
            ApplicationError::NotFound {
                entity: "habit",
                id: command.id.to_string(),
            }
        })?;
        if command.active {
            habit.resume();
        } else {
            habit.pause();
        }
        HabitRepository::save(repositories, habit.clone())?;
        ActivityRepository::append(
            repositories,
            ActivityRecord::new(
                command.activity_id.clone(),
                ActivitySubject::Habit(habit.id().clone()),
                ActivityKind::StatusChanged,
                command.origin.clone(),
                clock.now_ms(),
                command.summary.clone(),
            )?,
        )?;
        enqueue_if_sync_enabled(
            repositories,
            clock.now_ms(),
            PendingSemanticChange::new(
                SemanticEntityKind::Habit,
                habit.id().to_string(),
                vec![sync_field("active", Some(habit.is_active().to_string()))?],
            )?,
        )?;
        Ok(habit)
    })
}
