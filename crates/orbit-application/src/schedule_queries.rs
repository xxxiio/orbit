use orbit_domain::{ScheduleBlock, ScheduleBlockId};

use crate::{ApplicationError, ScheduleRepository};

pub fn get_schedule_block(
    repository: &impl ScheduleRepository,
    id: &ScheduleBlockId,
) -> Result<ScheduleBlock, ApplicationError> {
    repository
        .get(id)?
        .ok_or_else(|| ApplicationError::NotFound {
            entity: "schedule_block",
            id: id.to_string(),
        })
}

pub fn list_schedule_between(
    repository: &impl ScheduleRepository,
    start_ms: i64,
    end_ms: i64,
) -> Result<Vec<ScheduleBlock>, ApplicationError> {
    Ok(repository.list_between(start_ms, end_ms)?)
}
