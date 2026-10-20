use crate::{ApplicationError, RoutineRepository};
use orbit_domain::{Routine, RoutineId};

pub fn get_routine(
    repository: &impl RoutineRepository,
    id: &RoutineId,
) -> Result<Routine, ApplicationError> {
    repository
        .get(id)?
        .ok_or_else(|| ApplicationError::NotFound {
            entity: "routine",
            id: id.to_string(),
        })
}

pub fn list_routines(
    repository: &impl RoutineRepository,
) -> Result<Vec<Routine>, ApplicationError> {
    Ok(repository.list()?)
}
