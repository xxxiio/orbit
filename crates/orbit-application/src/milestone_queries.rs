use orbit_domain::{Milestone, MilestoneId, ProjectId};

use crate::{ApplicationError, MilestoneRepository};

pub fn get_milestone(
    repository: &impl MilestoneRepository,
    id: &MilestoneId,
) -> Result<Milestone, ApplicationError> {
    repository
        .get(id)?
        .ok_or_else(|| ApplicationError::NotFound {
            entity: "milestone",
            id: id.to_string(),
        })
}

pub fn list_project_milestones(
    repository: &impl MilestoneRepository,
    project_id: &ProjectId,
) -> Result<Vec<Milestone>, ApplicationError> {
    Ok(repository.list_for_project(project_id)?)
}
