use orbit_domain::{
    ActivityId, ActivityKind, ActivityOrigin, ActivityRecord, ActivitySubject, HabitId,
    HabitOccurrence, HabitOccurrenceId,
};

use crate::{
    ActivityRepository, ApplicationError, Clock, HabitOccurrenceRepository, HabitRepository,
    PendingSemanticChange, SemanticEntityKind, UnitOfWork, enqueue_if_sync_enabled, sync_field,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordHabitOccurrenceWithActivityCommand {
    pub id: HabitOccurrenceId,
    pub habit_id: HabitId,
    pub occurred_at_ms: Option<i64>,
    pub duration_minutes: Option<u32>,
    pub activity_id: ActivityId,
    pub origin: ActivityOrigin,
    pub summary: String,
}

pub fn record_habit_occurrence_with_activity(
    unit_of_work: &mut impl UnitOfWork,
    clock: &impl Clock,
    command: RecordHabitOccurrenceWithActivityCommand,
) -> Result<HabitOccurrence, ApplicationError> {
    unit_of_work.transact(|repositories| {
        let habit = HabitRepository::get(repositories, &command.habit_id)?.ok_or_else(|| {
            ApplicationError::NotFound {
                entity: "habit",
                id: command.habit_id.to_string(),
            }
        })?;
        if !habit.is_active() {
            return Err(ApplicationError::InconsistentReference {
                relation: "cannot record occurrence for paused habit",
            });
        }
        if HabitOccurrenceRepository::get(repositories, &command.id)?.is_some() {
            return Err(ApplicationError::AlreadyExists {
                entity: "habit_occurrence",
                id: command.id.to_string(),
            });
        }
        if ActivityRepository::get(repositories, &command.activity_id)?.is_some() {
            return Err(ApplicationError::AlreadyExists {
                entity: "activity",
                id: command.activity_id.to_string(),
            });
        }
        let occurred_at_ms = command.occurred_at_ms.unwrap_or_else(|| clock.now_ms());
        let occurrence = HabitOccurrence::new(
            command.id.clone(),
            command.habit_id.clone(),
            occurred_at_ms,
            command.duration_minutes,
        )?;
        HabitOccurrenceRepository::append(repositories, occurrence.clone())?;
        ActivityRepository::append(
            repositories,
            ActivityRecord::new(
                command.activity_id.clone(),
                ActivitySubject::Habit(command.habit_id.clone()),
                ActivityKind::Completed,
                command.origin.clone(),
                occurred_at_ms,
                command.summary.clone(),
            )?,
        )?;
        enqueue_if_sync_enabled(
            repositories,
            occurred_at_ms,
            PendingSemanticChange::new(
                SemanticEntityKind::HabitOccurrence,
                occurrence.id().to_string(),
                vec![
                    sync_field("habit_id", Some(occurrence.habit_id().to_string()))?,
                    sync_field(
                        "occurred_at_ms",
                        Some(occurrence.occurred_at_ms().to_string()),
                    )?,
                    sync_field(
                        "duration_minutes",
                        occurrence.duration_minutes().map(|v| v.to_string()),
                    )?,
                ],
            )?,
        )?;
        Ok(occurrence)
    })
}
