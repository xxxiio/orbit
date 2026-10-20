use orbit_domain::TaskScope;

use crate::{ApplicationError, MilestoneRepository, ProjectRepository};

pub fn validate_task_scope(
    projects: &impl ProjectRepository,
    milestones: &impl MilestoneRepository,
    scope: &TaskScope,
) -> Result<(), ApplicationError> {
    let Some(project_id) = scope.project_id() else {
        return Ok(());
    };

    if projects.get(project_id)?.is_none() {
        return Err(ApplicationError::NotFound {
            entity: "project",
            id: project_id.to_string(),
        });
    }

    let Some(milestone_id) = scope.milestone_id() else {
        return Ok(());
    };

    let milestone = milestones
        .get(milestone_id)?
        .ok_or_else(|| ApplicationError::NotFound {
            entity: "milestone",
            id: milestone_id.to_string(),
        })?;

    if !milestone.belongs_to(project_id) {
        return Err(ApplicationError::InconsistentReference {
            relation: "task_milestone_project",
        });
    }

    Ok(())
}
