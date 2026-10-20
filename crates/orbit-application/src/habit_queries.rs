use orbit_domain::{Habit, HabitId};

use crate::{ApplicationError, HabitRepository};

pub fn get_habit(
    repository: &impl HabitRepository,
    id: &HabitId,
) -> Result<Habit, ApplicationError> {
    repository
        .get(id)?
        .ok_or_else(|| ApplicationError::NotFound {
            entity: "habit",
            id: id.to_string(),
        })
}

pub fn list_habits(repository: &impl HabitRepository) -> Result<Vec<Habit>, ApplicationError> {
    Ok(repository.list()?)
}
