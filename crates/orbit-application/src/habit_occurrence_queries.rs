use crate::{ApplicationError, HabitOccurrenceRepository};
use orbit_domain::{HabitCadence, HabitId, HabitOccurrence, HabitPeriod};
use orbit_habit_capability::{HabitRecurrenceStatus, HabitStreakSummary, evaluate_habit_periods};

/// Explicit outcome of an opt-in recurrence evaluation.
///
/// Recurrence is not core behavior: when `orbit.habit.recurrence` is not
/// installed (or not available for the caller's project), the application layer
/// returns the reason instead of computing a streak.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HabitStreakSummaryOutcome {
    Available(HabitStreakSummary),
    Unavailable {
        capability_id: String,
        reason: String,
    },
}

impl HabitStreakSummaryOutcome {
    pub fn unavailable_reason(&self) -> Option<&str> {
        match self {
            Self::Available(_) => None,
            Self::Unavailable { reason, .. } => Some(reason),
        }
    }
}

pub fn list_habit_occurrences(
    repository: &impl HabitOccurrenceRepository,
    habit_id: &HabitId,
) -> Result<Vec<HabitOccurrence>, ApplicationError> {
    Ok(repository.list_for_habit(habit_id)?)
}

/// Evaluate recurrence only when the trusted host resolved the capability as
/// available. There is no implicit fallback: an unavailable capability yields
/// [`HabitStreakSummaryOutcome::Unavailable`] rather than a core-computed streak.
pub fn habit_streak_summary(
    repository: &impl HabitOccurrenceRepository,
    habit_id: &HabitId,
    cadence: HabitCadence,
    periods: &[HabitPeriod],
    recurrence: &HabitRecurrenceStatus,
) -> Result<HabitStreakSummaryOutcome, ApplicationError> {
    if let Some((capability_id, reason)) = recurrence.unavailable_parts() {
        return Ok(HabitStreakSummaryOutcome::Unavailable {
            capability_id: capability_id.to_owned(),
            reason: reason.to_owned(),
        });
    }
    let occurrences = repository.list_for_habit(habit_id)?;
    Ok(HabitStreakSummaryOutcome::Available(
        evaluate_habit_periods(cadence, periods, &occurrences)?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::RepositoryError;
    use orbit_domain::HabitOccurrenceId;

    #[derive(Default)]
    struct OccurrenceRepository {
        occurrences: Vec<HabitOccurrence>,
    }

    impl HabitOccurrenceRepository for OccurrenceRepository {
        fn get(&self, id: &HabitOccurrenceId) -> Result<Option<HabitOccurrence>, RepositoryError> {
            Ok(self
                .occurrences
                .iter()
                .find(|occurrence| occurrence.id() == id)
                .cloned())
        }

        fn list_for_habit(
            &self,
            habit_id: &HabitId,
        ) -> Result<Vec<HabitOccurrence>, RepositoryError> {
            Ok(self
                .occurrences
                .iter()
                .filter(|occurrence| occurrence.habit_id() == habit_id)
                .cloned()
                .collect())
        }

        fn append(&mut self, occurrence: HabitOccurrence) -> Result<(), RepositoryError> {
            self.occurrences.push(occurrence);
            Ok(())
        }
    }

    fn occurrence(id: &str, at: i64, duration: Option<u32>) -> HabitOccurrence {
        HabitOccurrence::new(
            HabitOccurrenceId::new(id).unwrap(),
            HabitId::new("habit-1").unwrap(),
            at,
            duration,
        )
        .unwrap()
    }

    #[test]
    fn summary_matches_the_package_when_available_and_fails_closed_when_not() {
        let habit_id = HabitId::new("habit-1").unwrap();
        let mut repository = OccurrenceRepository::default();
        repository.append(occurrence("o1", 10, Some(20))).unwrap();
        repository.append(occurrence("o2", 120, None)).unwrap();
        let periods = vec![
            HabitPeriod::new(0, 100).unwrap(),
            HabitPeriod::new(100, 200).unwrap(),
        ];

        let expected =
            evaluate_habit_periods(HabitCadence::Daily, &periods, &repository.occurrences).unwrap();
        let available = habit_streak_summary(
            &repository,
            &habit_id,
            HabitCadence::Daily,
            &periods,
            &HabitRecurrenceStatus::available(),
        )
        .unwrap();
        assert_eq!(available, HabitStreakSummaryOutcome::Available(expected));

        let unavailable = habit_streak_summary(
            &repository,
            &habit_id,
            HabitCadence::Daily,
            &periods,
            &HabitRecurrenceStatus::not_installed(),
        )
        .unwrap();
        assert_eq!(
            unavailable,
            HabitStreakSummaryOutcome::Unavailable {
                capability_id: orbit_habit_capability::CAPABILITY_ID.to_owned(),
                reason: format!("{} is not installed", orbit_habit_capability::CAPABILITY_ID),
            }
        );
        assert!(unavailable.unavailable_reason().is_some());
    }
}
