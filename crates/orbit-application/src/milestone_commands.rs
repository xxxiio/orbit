use orbit_domain::{
    ActivityId, ActivityKind, ActivityOrigin, ActivityRecord, ActivitySubject, Milestone,
    MilestoneId, MilestoneStatus, ProjectId,
};

use crate::{
    ActivityRepository, ApplicationError, Clock, MilestoneRepository, PendingSemanticChange,
    ProjectRepository, SemanticEntityKind, UnitOfWork, enqueue_if_sync_enabled, sync_field,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateMilestoneCommand {
    pub id: MilestoneId,
    pub project_id: ProjectId,
    pub name: String,
}

pub fn create_milestone(
    milestones: &mut impl MilestoneRepository,
    projects: &impl ProjectRepository,
    command: CreateMilestoneCommand,
) -> Result<Milestone, ApplicationError> {
    if milestones.get(&command.id)?.is_some() {
        return Err(ApplicationError::AlreadyExists {
            entity: "milestone",
            id: command.id.to_string(),
        });
    }
    if projects.get(&command.project_id)?.is_none() {
        return Err(ApplicationError::NotFound {
            entity: "project",
            id: command.project_id.to_string(),
        });
    }
    let milestone = Milestone::new(command.id, command.project_id, command.name)?;
    milestones.save(milestone.clone())?;
    Ok(milestone)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransitionMilestoneCommand {
    pub id: MilestoneId,
    pub status: MilestoneStatus,
}

pub fn transition_milestone(
    repository: &mut impl MilestoneRepository,
    command: TransitionMilestoneCommand,
) -> Result<Milestone, ApplicationError> {
    let mut milestone = repository
        .get(&command.id)?
        .ok_or_else(|| ApplicationError::NotFound {
            entity: "milestone",
            id: command.id.to_string(),
        })?;
    milestone.transition_to(command.status)?;
    repository.save(milestone.clone())?;
    Ok(milestone)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransitionMilestoneWithActivityCommand {
    pub milestone: TransitionMilestoneCommand,
    pub activity_id: ActivityId,
    pub origin: ActivityOrigin,
    pub summary: String,
}

/// Audited milestone transition used by CLI/TUI/MCP composition layers.
/// State, activity, and sync-outbox mutation commit atomically.
pub fn transition_milestone_with_activity(
    unit_of_work: &mut impl UnitOfWork,
    clock: &impl Clock,
    command: TransitionMilestoneWithActivityCommand,
) -> Result<Milestone, ApplicationError> {
    unit_of_work.transact(|repositories| {
        if ActivityRepository::get(repositories, &command.activity_id)?.is_some() {
            return Err(ApplicationError::AlreadyExists {
                entity: "activity",
                id: command.activity_id.to_string(),
            });
        }
        let milestone = transition_milestone(repositories, command.milestone.clone())?;
        let kind = if milestone.status() == MilestoneStatus::Completed {
            ActivityKind::Completed
        } else {
            ActivityKind::StatusChanged
        };
        ActivityRepository::append(
            repositories,
            ActivityRecord::new(
                command.activity_id.clone(),
                ActivitySubject::Milestone(milestone.id().clone()),
                kind,
                command.origin.clone(),
                clock.now_ms(),
                command.summary.clone(),
            )?,
        )?;
        enqueue_if_sync_enabled(
            repositories,
            clock.now_ms(),
            PendingSemanticChange::new(
                SemanticEntityKind::Milestone,
                milestone.id().to_string(),
                vec![sync_field(
                    "status",
                    Some(milestone.status().as_str().to_owned()),
                )?],
            )?,
        )?;
        Ok(milestone)
    })
}
