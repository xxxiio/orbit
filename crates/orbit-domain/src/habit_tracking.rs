//! Habit value types and their validation.
//!
//! Recurrence and streak evaluation are not core behavior. They live in the
//! opt-in `orbit-habit-capability` package and are only reachable after the
//! trusted host installs the `orbit.habit.recurrence` capability.

use crate::{DomainError, HabitId, HabitOccurrenceId};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HabitOccurrence {
    id: HabitOccurrenceId,
    habit_id: HabitId,
    occurred_at_ms: i64,
    duration_minutes: Option<u32>,
}

impl HabitOccurrence {
    pub fn new(
        id: HabitOccurrenceId,
        habit_id: HabitId,
        occurred_at_ms: i64,
        duration_minutes: Option<u32>,
    ) -> Result<Self, DomainError> {
        if matches!(duration_minutes, Some(0)) {
            return Err(DomainError::InvalidHabitOccurrenceDuration);
        }
        Ok(Self {
            id,
            habit_id,
            occurred_at_ms,
            duration_minutes,
        })
    }

    pub fn id(&self) -> &HabitOccurrenceId {
        &self.id
    }
    pub fn habit_id(&self) -> &HabitId {
        &self.habit_id
    }
    pub fn occurred_at_ms(&self) -> i64 {
        self.occurred_at_ms
    }
    pub fn duration_minutes(&self) -> Option<u32> {
        self.duration_minutes
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HabitPeriod {
    pub start_ms: i64,
    pub end_ms: i64,
}

impl HabitPeriod {
    pub fn new(start_ms: i64, end_ms: i64) -> Result<Self, DomainError> {
        if start_ms >= end_ms {
            return Err(DomainError::InvalidHabitPeriod);
        }
        Ok(Self { start_ms, end_ms })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn occurrence_rejects_zero_duration_but_keeps_optional_time_spent() {
        let id = HabitOccurrenceId::new("occurrence-1").unwrap();
        let habit_id = HabitId::new("habit-1").unwrap();
        assert_eq!(
            HabitOccurrence::new(id.clone(), habit_id.clone(), 10, Some(0)),
            Err(DomainError::InvalidHabitOccurrenceDuration)
        );
        let occurrence =
            HabitOccurrence::new(id, habit_id, 10, Some(30)).expect("valid occurrence");
        assert_eq!(occurrence.occurred_at_ms(), 10);
        assert_eq!(occurrence.duration_minutes(), Some(30));
    }

    #[test]
    fn period_rejects_empty_or_reversed_windows() {
        assert_eq!(
            HabitPeriod::new(10, 10),
            Err(DomainError::InvalidHabitPeriod)
        );
        assert_eq!(
            HabitPeriod::new(20, 10),
            Err(DomainError::InvalidHabitPeriod)
        );
        assert_eq!(
            HabitPeriod::new(10, 20).unwrap(),
            HabitPeriod {
                start_ms: 10,
                end_ms: 20
            }
        );
    }
}
