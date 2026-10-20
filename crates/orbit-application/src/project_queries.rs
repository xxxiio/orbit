use orbit_domain::{Project, ProjectId};

use crate::{ApplicationError, ProjectRepository};

pub fn get_project(
    repository: &impl ProjectRepository,
    id: &ProjectId,
) -> Result<Project, ApplicationError> {
    repository
        .get(id)?
        .ok_or_else(|| ApplicationError::NotFound {
            entity: "project",
            id: id.to_string(),
        })
}

pub fn list_projects(
    repository: &impl ProjectRepository,
) -> Result<Vec<Project>, ApplicationError> {
    Ok(repository.list()?)
}
