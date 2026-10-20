use crate::{ApplicationError, HabitOccurrenceRepository};
use orbit_domain::{
    HabitCadence, HabitId, HabitOccurrence, HabitPeriod, HabitStreakSummary, evaluate_habit_periods,
};

pub fn list_habit_occurrences(
    repository: &impl HabitOccurrenceRepository,
    habit_id: &HabitId,
) -> Result<Vec<HabitOccurrence>, ApplicationError> {
    Ok(repository.list_for_habit(habit_id)?)
}

pub fn habit_streak_summary(
    repository: &impl HabitOccurrenceRepository,
    habit_id: &HabitId,
    cadence: HabitCadence,
    periods: &[HabitPeriod],
) -> Result<HabitStreakSummary, ApplicationError> {
    let occurrences = repository.list_for_habit(habit_id)?;
    Ok(evaluate_habit_periods(cadence, periods, &occurrences)?)
}
