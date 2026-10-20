use crate::{DomainError, HabitId};

/// How a habit target is expressed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HabitCadence {
    Daily,
    Weekly { target_per_week: u8 },
}

impl HabitCadence {
    pub fn validate(self) -> Result<Self, DomainError> {
        match self {
            Self::Weekly { target_per_week: 0 } => Err(DomainError::InvalidHabitCadence),
            Self::Weekly { target_per_week } if target_per_week > 7 => {
                Err(DomainError::InvalidHabitCadence)
            }
            _ => Ok(self),
        }
    }
}

/// A repeating personal behavior that is tracked independently from tasks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Habit {
    id: HabitId,
    name: String,
    cadence: HabitCadence,
    active: bool,
}

impl Habit {
    pub fn new(
        id: HabitId,
        name: impl Into<String>,
        cadence: HabitCadence,
    ) -> Result<Self, DomainError> {
        let name = name.into();
        if name.trim().is_empty() {
            return Err(DomainError::EmptyHabitName);
        }

        Ok(Self {
            id,
            name,
            cadence: cadence.validate()?,
            active: true,
        })
    }

    pub fn id(&self) -> &HabitId {
        &self.id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn cadence(&self) -> HabitCadence {
        self.cadence
    }

    pub fn is_active(&self) -> bool {
        self.active
    }

    pub fn pause(&mut self) {
        self.active = false;
    }

    pub fn resume(&mut self) {
        self.active = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn habit_id() -> HabitId {
        HabitId::new("habit-1").expect("valid habit id")
    }

    #[test]
    fn habit_is_not_modeled_as_a_task() {
        let habit = Habit::new(habit_id(), "Exercise", HabitCadence::Daily).expect("valid habit");

        assert_eq!(habit.name(), "Exercise");
        assert_eq!(habit.cadence(), HabitCadence::Daily);
        assert!(habit.is_active());
    }

    #[test]
    fn habit_name_must_not_be_blank() {
        let result = Habit::new(habit_id(), "   ", HabitCadence::Daily);

        assert_eq!(result, Err(DomainError::EmptyHabitName));
    }

    #[test]
    fn weekly_cadence_requires_a_target_between_one_and_seven() {
        assert_eq!(
            Habit::new(
                habit_id(),
                "Exercise",
                HabitCadence::Weekly { target_per_week: 0 },
            ),
            Err(DomainError::InvalidHabitCadence)
        );

        assert_eq!(
            Habit::new(
                habit_id(),
                "Exercise",
                HabitCadence::Weekly { target_per_week: 8 },
            ),
            Err(DomainError::InvalidHabitCadence)
        );
    }

    #[test]
    fn habit_can_be_paused_and_resumed_without_becoming_a_task_state() {
        let mut habit = Habit::new(
            habit_id(),
            "Exercise",
            HabitCadence::Weekly { target_per_week: 4 },
        )
        .expect("valid habit");

        habit.pause();
        assert!(!habit.is_active());

        habit.resume();
        assert!(habit.is_active());
    }
}
