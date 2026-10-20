use orbit_domain::{
    ActivityId, ActivityKind, ActivityOrigin, ActivityRecord, ActivitySubject, Project, ProjectId,
    ProjectStatus,
};

use crate::{
    ActivityRepository, ApplicationError, Clock, PendingSemanticChange, ProjectRepository,
    SemanticEntityKind, UnitOfWork, enqueue_if_sync_enabled, sync_field,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateProjectCommand {
    pub id: ProjectId,
    pub name: String,
}

pub fn create_project(
    repository: &mut impl ProjectRepository,
    command: CreateProjectCommand,
) -> Result<Project, ApplicationError> {
    if repository.get(&command.id)?.is_some() {
        return Err(ApplicationError::AlreadyExists {
            entity: "project",
            id: command.id.to_string(),
        });
    }

    let project = Project::new(command.id, command.name)?;
    repository.save(project.clone())?;
    Ok(project)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateProjectWithActivityCommand {
    pub project: CreateProjectCommand,
    pub activity_id: ActivityId,
    pub origin: ActivityOrigin,
    pub summary: String,
}

pub fn create_project_with_activity(
    unit_of_work: &mut impl UnitOfWork,
    clock: &impl Clock,
    command: CreateProjectWithActivityCommand,
) -> Result<Project, ApplicationError> {
    unit_of_work.transact(|repositories| {
        if ProjectRepository::get(repositories, &command.project.id)?.is_some() {
            return Err(ApplicationError::AlreadyExists {
                entity: "project",
                id: command.project.id.to_string(),
            });
        }
        if ActivityRepository::get(repositories, &command.activity_id)?.is_some() {
            return Err(ApplicationError::AlreadyExists {
                entity: "activity",
                id: command.activity_id.to_string(),
            });
        }
        let project = Project::new(command.project.id.clone(), command.project.name.clone())?;
        ProjectRepository::save(repositories, project.clone())?;
        let activity = ActivityRecord::new(
            command.activity_id.clone(),
            ActivitySubject::Project(project.id().clone()),
            ActivityKind::Created,
            command.origin.clone(),
            clock.now_ms(),
            command.summary.clone(),
        )?;
        ActivityRepository::append(repositories, activity)?;
        enqueue_if_sync_enabled(
            repositories,
            clock.now_ms(),
            PendingSemanticChange::new(
                SemanticEntityKind::Project,
                project.id().to_string(),
                vec![
                    sync_field("name", Some(project.name().to_owned()))?,
                    sync_field("status", Some(project.status().as_str().to_owned()))?,
                    sync_field("mode", Some(project.mode().as_str().to_owned()))?,
                    sync_field("objective", project.objective().map(ToOwned::to_owned))?,
                    sync_field(
                        "progress_signals",
                        Some(
                            project
                                .progress_signals()
                                .iter()
                                .map(|signal| signal.as_str())
                                .collect::<Vec<_>>()
                                .join(","),
                        ),
                    )?,
                ],
            )?,
        )?;
        Ok(project)
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransitionProjectCommand {
    pub id: ProjectId,
    pub status: ProjectStatus,
}

pub fn transition_project(
    repository: &mut impl ProjectRepository,
    command: TransitionProjectCommand,
) -> Result<Project, ApplicationError> {
    let mut project = repository
        .get(&command.id)?
        .ok_or_else(|| ApplicationError::NotFound {
            entity: "project",
            id: command.id.to_string(),
        })?;

    project.transition_to(command.status)?;
    repository.save(project.clone())?;
    Ok(project)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransitionProjectWithActivityCommand {
    pub project: TransitionProjectCommand,
    pub activity_id: ActivityId,
    pub origin: ActivityOrigin,
    pub summary: String,
}

pub fn transition_project_with_activity(
    unit_of_work: &mut impl UnitOfWork,
    clock: &impl Clock,
    command: TransitionProjectWithActivityCommand,
) -> Result<Project, ApplicationError> {
    unit_of_work.transact(|repositories| {
        if ActivityRepository::get(repositories, &command.activity_id)?.is_some() {
            return Err(ApplicationError::AlreadyExists {
                entity: "activity",
                id: command.activity_id.to_string(),
            });
        }

        let project = transition_project(repositories, command.project.clone())?;
        let activity = ActivityRecord::new(
            command.activity_id.clone(),
            ActivitySubject::Project(project.id().clone()),
            if project.status() == ProjectStatus::Completed {
                ActivityKind::Completed
            } else {
                ActivityKind::StatusChanged
            },
            command.origin.clone(),
            clock.now_ms(),
            command.summary.clone(),
        )?;
        ActivityRepository::append(repositories, activity)?;
        enqueue_if_sync_enabled(
            repositories,
            clock.now_ms(),
            PendingSemanticChange::new(
                SemanticEntityKind::Project,
                project.id().to_string(),
                vec![sync_field(
                    "status",
                    Some(project.status().as_str().to_owned()),
                )?],
            )?,
        )?;
        Ok(project)
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigureProjectStrategyCommand {
    pub id: ProjectId,
    pub mode: orbit_domain::ProjectMode,
    pub objective: Option<String>,
    pub progress_signals: Vec<orbit_domain::ProjectProgressSignal>,
    pub success_criteria: Vec<orbit_domain::ProjectSuccessCriterion>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigureProjectStrategyWithActivityCommand {
    pub project: ConfigureProjectStrategyCommand,
    pub activity_id: ActivityId,
    pub origin: ActivityOrigin,
    pub summary: String,
}

pub fn configure_project_strategy_with_activity(
    unit_of_work: &mut impl UnitOfWork,
    clock: &impl Clock,
    command: ConfigureProjectStrategyWithActivityCommand,
) -> Result<Project, ApplicationError> {
    unit_of_work.transact(|repositories| {
        if ActivityRepository::get(repositories, &command.activity_id)?.is_some() {
            return Err(ApplicationError::AlreadyExists {
                entity: "activity",
                id: command.activity_id.to_string(),
            });
        }
        let mut project =
            ProjectRepository::get(repositories, &command.project.id)?.ok_or_else(|| {
                ApplicationError::NotFound {
                    entity: "project",
                    id: command.project.id.to_string(),
                }
            })?;
        let old_criterion_keys = project
            .success_criteria()
            .iter()
            .map(|criterion| criterion.key().to_owned())
            .collect::<Vec<_>>();
        project.set_mode(command.project.mode, false);
        project.set_objective(command.project.objective.clone())?;
        project.set_progress_signals(command.project.progress_signals.clone())?;
        project.replace_success_criteria(command.project.success_criteria.clone())?;
        ProjectRepository::save(repositories, project.clone())?;
        ActivityRepository::append(
            repositories,
            ActivityRecord::new(
                command.activity_id.clone(),
                ActivitySubject::Project(project.id().clone()),
                ActivityKind::Updated,
                command.origin.clone(),
                clock.now_ms(),
                command.summary.clone(),
            )?,
        )?;

        let mut mutations = vec![
            sync_field("mode", Some(project.mode().as_str().to_owned()))?,
            sync_field("objective", project.objective().map(ToOwned::to_owned))?,
            sync_field(
                "progress_signals",
                Some(
                    project
                        .progress_signals()
                        .iter()
                        .map(|signal| signal.as_str())
                        .collect::<Vec<_>>()
                        .join(","),
                ),
            )?,
        ];
        for key in old_criterion_keys {
            if !project
                .success_criteria()
                .iter()
                .any(|criterion| criterion.key() == key)
            {
                mutations.push(sync_field(format!("criterion:{key}"), None)?);
            }
        }
        for criterion in project.success_criteria() {
            mutations.push(sync_field(
                format!("criterion:{}", criterion.key()),
                Some(encode_project_criterion(criterion)),
            )?);
        }
        enqueue_if_sync_enabled(
            repositories,
            clock.now_ms(),
            PendingSemanticChange::new(
                SemanticEntityKind::Project,
                project.id().to_string(),
                mutations,
            )?,
        )?;
        Ok(project)
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetProjectCriterionStatusWithActivityCommand {
    pub project_id: ProjectId,
    pub criterion_key: String,
    pub status: orbit_domain::SuccessCriterionStatus,
    pub activity_id: ActivityId,
    pub origin: ActivityOrigin,
    pub summary: String,
}

pub fn set_project_criterion_status_with_activity(
    unit_of_work: &mut impl UnitOfWork,
    clock: &impl Clock,
    command: SetProjectCriterionStatusWithActivityCommand,
) -> Result<Project, ApplicationError> {
    unit_of_work.transact(|repositories| {
        if ActivityRepository::get(repositories, &command.activity_id)?.is_some() {
            return Err(ApplicationError::AlreadyExists {
                entity: "activity",
                id: command.activity_id.to_string(),
            });
        }
        let mut project =
            ProjectRepository::get(repositories, &command.project_id)?.ok_or_else(|| {
                ApplicationError::NotFound {
                    entity: "project",
                    id: command.project_id.to_string(),
                }
            })?;
        project.set_success_criterion_status(&command.criterion_key, command.status)?;
        let criterion = project
            .success_criteria()
            .iter()
            .find(|criterion| criterion.key() == command.criterion_key)
            .expect("criterion just updated")
            .clone();
        ProjectRepository::save(repositories, project.clone())?;
        ActivityRepository::append(
            repositories,
            ActivityRecord::new(
                command.activity_id,
                ActivitySubject::Project(project.id().clone()),
                ActivityKind::Updated,
                command.origin,
                clock.now_ms(),
                command.summary,
            )?,
        )?;
        enqueue_if_sync_enabled(
            repositories,
            clock.now_ms(),
            PendingSemanticChange::new(
                SemanticEntityKind::Project,
                project.id().to_string(),
                vec![sync_field(
                    format!("criterion:{}", criterion.key()),
                    Some(encode_project_criterion(&criterion)),
                )?],
            )?,
        )?;
        Ok(project)
    })
}

fn encode_project_criterion(criterion: &orbit_domain::ProjectSuccessCriterion) -> String {
    let escaped = criterion
        .description()
        .replace('%', "%25")
        .replace('|', "%7C")
        .replace('\n', "%0A");
    format!(
        "{}|{}|{}",
        if criterion.required() { "1" } else { "0" },
        criterion.status().as_str(),
        escaped
    )
}
