use crate::{DomainError, HabitId, RoutineId};

/// A lightweight ordered grouping of habits for organization and planning.
///
/// Routines are deliberately not tasks and do not own habit lifecycle. A habit
/// may appear in more than one routine; the routine only stores stable habit
/// references and their preferred order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Routine {
    id: RoutineId,
    name: String,
    habit_ids: Vec<HabitId>,
}

impl Routine {
    pub fn new(id: RoutineId, name: impl Into<String>) -> Result<Self, DomainError> {
        let name = name.into();
        if name.trim().is_empty() {
            return Err(DomainError::EmptyRoutineName);
        }
        Ok(Self {
            id,
            name,
            habit_ids: Vec::new(),
        })
    }

    pub fn id(&self) -> &RoutineId {
        &self.id
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn habit_ids(&self) -> &[HabitId] {
        &self.habit_ids
    }

    pub fn add_habit(&mut self, habit_id: HabitId) -> Result<(), DomainError> {
        if self.habit_ids.iter().any(|id| id == &habit_id) {
            return Err(DomainError::DuplicateHabitInRoutine);
        }
        self.habit_ids.push(habit_id);
        Ok(())
    }

    pub fn remove_habit(&mut self, habit_id: &HabitId) -> bool {
        let before = self.habit_ids.len();
        self.habit_ids.retain(|id| id != habit_id);
        before != self.habit_ids.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn routine_id() -> RoutineId {
        RoutineId::new("routine-1").unwrap()
    }
    fn habit_id(value: &str) -> HabitId {
        HabitId::new(value).unwrap()
    }

    #[test]
    fn routine_preserves_habit_order_without_owning_habit_state() {
        let mut routine = Routine::new(routine_id(), "Morning").unwrap();
        routine.add_habit(habit_id("water")).unwrap();
        routine.add_habit(habit_id("stretch")).unwrap();
        assert_eq!(
            routine.habit_ids(),
            &[habit_id("water"), habit_id("stretch")]
        );
    }

    #[test]
    fn routine_rejects_duplicate_membership() {
        let mut routine = Routine::new(routine_id(), "Morning").unwrap();
        routine.add_habit(habit_id("water")).unwrap();
        assert_eq!(
            routine.add_habit(habit_id("water")),
            Err(DomainError::DuplicateHabitInRoutine)
        );
    }

    #[test]
    fn habit_can_be_removed_from_routine_without_touching_habit() {
        let mut routine = Routine::new(routine_id(), "Morning").unwrap();
        let habit = habit_id("water");
        routine.add_habit(habit.clone()).unwrap();
        assert!(routine.remove_habit(&habit));
        assert!(routine.habit_ids().is_empty());
    }
}
