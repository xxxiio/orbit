//! Validated, atomic project evidence association operations.
use crate::{
    ApplicationError, Clock, HabitRepository, PendingSemanticChange, ProjectEvidenceBinding,
    ProjectEvidenceRepository, ProjectRepository, SemanticEntityKind, UnitOfWork,
    enqueue_if_sync_enabled, sync_field,
};
use orbit_domain::ProjectId;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangeProjectEvidenceCommand {
    pub project_id: ProjectId,
    pub binding: ProjectEvidenceBinding,
}

fn validate<R>(
    repository: &R,
    command: &ChangeProjectEvidenceCommand,
    require_habit: bool,
) -> Result<(), ApplicationError>
where
    R: ProjectRepository + HabitRepository,
{
    if ProjectRepository::get(repository, &command.project_id)?.is_none() {
        return Err(ApplicationError::NotFound {
            entity: "project",
            id: command.project_id.to_string(),
        });
    }
    match &command.binding {
        ProjectEvidenceBinding::Habit(id) => {
            if require_habit && HabitRepository::get(repository, id)?.is_none() {
                return Err(ApplicationError::NotFound {
                    entity: "habit",
                    id: id.to_string(),
                });
            }
        }
        ProjectEvidenceBinding::Metric(id) => {
            if id.trim().is_empty() || id.trim() != id {
                return Err(ApplicationError::InconsistentReference {
                    relation: "project_metric_identifier",
                });
            }
        }
    }
    Ok(())
}

/// Idempotently adds an evidence binding with its durable semantic outbox mutation.
pub fn bind_project_evidence<R>(
    repository: &mut R,
    clock: &impl Clock,
    command: ChangeProjectEvidenceCommand,
) -> Result<(), ApplicationError>
where
    R: UnitOfWork,
{
    update_binding(repository, clock, command, true)
}

/// Removes an association without deleting its underlying habit or metric.
pub fn unbind_project_evidence<R>(
    repository: &mut R,
    clock: &impl Clock,
    command: ChangeProjectEvidenceCommand,
) -> Result<(), ApplicationError>
where
    R: UnitOfWork,
{
    update_binding(repository, clock, command, false)
}

fn update_binding<R: UnitOfWork>(
    repository: &mut R,
    clock: &impl Clock,
    command: ChangeProjectEvidenceCommand,
    add: bool,
) -> Result<(), ApplicationError> {
    repository.transact(|tx| {
        validate(tx, &command, add)?;
        let existing = ProjectEvidenceRepository::list_project_evidence(tx, &command.project_id)?;
        if existing.contains(&command.binding) == add {
            return Ok(());
        }
        let field_name = match &command.binding {
            ProjectEvidenceBinding::Habit(id) => format!("evidence:habit:{}", id.as_str()),
            ProjectEvidenceBinding::Metric(id) => format!("evidence:metric:{}", id),
        };
        let change = PendingSemanticChange::new(
            SemanticEntityKind::Project,
            command.project_id.to_string(),
            vec![sync_field(
                field_name,
                if add { Some("1".into()) } else { None },
            )?],
        )?;
        if add {
            ProjectEvidenceRepository::add_project_evidence(
                tx,
                &command.project_id,
                &command.binding,
            )?;
        } else {
            ProjectEvidenceRepository::remove_project_evidence(
                tx,
                &command.project_id,
                &command.binding,
            )?;
        }
        enqueue_if_sync_enabled(tx, clock.now_ms(), change)?;
        Ok(())
    })
}

pub fn list_project_evidence<R>(
    repository: &R,
    project_id: &ProjectId,
) -> Result<Vec<ProjectEvidenceBinding>, ApplicationError>
where
    R: ProjectRepository + ProjectEvidenceRepository,
{
    if ProjectRepository::get(repository, project_id)?.is_none() {
        return Err(ApplicationError::NotFound {
            entity: "project",
            id: project_id.to_string(),
        });
    }
    Ok(ProjectEvidenceRepository::list_project_evidence(
        repository, project_id,
    )?)
}
