use orbit_domain::{
    ActivityId, ActivityKind, ActivityOrigin, ActivityRecord, ActivitySubject, Task, TaskId,
    TaskPlanning, TaskScope, TaskStatus,
};

use crate::{
    ActivityRepository, ApplicationError, Clock, MilestoneRepository, PendingSemanticChange,
    ProjectRepository, SemanticEntityKind, TaskRepository, UnitOfWork, enqueue_if_sync_enabled,
    sync_field, validate_task_scope,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateTaskCommand {
    pub id: TaskId,
    pub title: String,
    pub scope: TaskScope,
}

pub fn create_task(
    tasks: &mut impl TaskRepository,
    projects: &impl ProjectRepository,
    milestones: &impl MilestoneRepository,
    command: CreateTaskCommand,
) -> Result<Task, ApplicationError> {
    if tasks.get(&command.id)?.is_some() {
        return Err(ApplicationError::AlreadyExists {
            entity: "task",
            id: command.id.to_string(),
        });
    }

    validate_task_scope(projects, milestones, &command.scope)?;
    let task = Task::new(command.id, command.title, command.scope)?;
    tasks.save(task.clone())?;
    Ok(task)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateTaskWithActivityCommand {
    pub task: CreateTaskCommand,
    pub activity_id: ActivityId,
    pub origin: ActivityOrigin,
    pub summary: String,
}

/// Create a task and its semantic creation activity atomically using a single
/// repository bundle. This is the composition-friendly counterpart to
/// `create_task`, which remains useful when repositories are supplied
/// independently in tests or alternate adapters.
pub fn create_task_with_activity(
    unit_of_work: &mut impl UnitOfWork,
    clock: &impl Clock,
    command: CreateTaskWithActivityCommand,
) -> Result<Task, ApplicationError> {
    unit_of_work.transact(|repositories| {
        if TaskRepository::get(repositories, &command.task.id)?.is_some() {
            return Err(ApplicationError::AlreadyExists {
                entity: "task",
                id: command.task.id.to_string(),
            });
        }
        if ActivityRepository::get(repositories, &command.activity_id)?.is_some() {
            return Err(ApplicationError::AlreadyExists {
                entity: "activity",
                id: command.activity_id.to_string(),
            });
        }

        validate_task_scope(repositories, repositories, &command.task.scope)?;
        let task = Task::new(
            command.task.id.clone(),
            command.task.title.clone(),
            command.task.scope.clone(),
        )?;
        TaskRepository::save(repositories, task.clone())?;
        let activity = ActivityRecord::new(
            command.activity_id.clone(),
            ActivitySubject::Task(task.id().clone()),
            ActivityKind::Created,
            command.origin.clone(),
            clock.now_ms(),
            command.summary.clone(),
        )?;
        ActivityRepository::append(repositories, activity)?;
        let now_ms = clock.now_ms();
        enqueue_if_sync_enabled(
            repositories,
            now_ms,
            PendingSemanticChange::new(
                SemanticEntityKind::Task,
                task.id().to_string(),
                vec![
                    sync_field("title", Some(task.title().to_owned()))?,
                    sync_field("status", Some(task.status().as_str().to_owned()))?,
                    sync_field(
                        "project_id",
                        task.scope().project_id().map(ToString::to_string),
                    )?,
                    sync_field(
                        "milestone_id",
                        task.scope().milestone_id().map(ToString::to_string),
                    )?,
                ],
            )?,
        )?;
        Ok(task)
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransitionTaskCommand {
    pub id: TaskId,
    pub status: TaskStatus,
}

pub fn transition_task(
    repository: &mut impl TaskRepository,
    command: TransitionTaskCommand,
) -> Result<Task, ApplicationError> {
    let mut task = repository
        .get(&command.id)?
        .ok_or_else(|| ApplicationError::NotFound {
            entity: "task",
            id: command.id.to_string(),
        })?;

    task.transition_to(command.status)?;
    repository.save(task.clone())?;
    Ok(task)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartTaskWithActivityCommand {
    pub id: TaskId,
    pub activity_id: ActivityId,
    pub origin: ActivityOrigin,
    pub summary: String,
}

/// Start a task without weakening the domain transition graph. Inbox tasks are
/// promoted through Ready before entering InProgress, but only the final state
/// is persisted and one semantic activity is appended atomically.
pub fn start_task_with_activity(
    unit_of_work: &mut impl UnitOfWork,
    clock: &impl Clock,
    command: StartTaskWithActivityCommand,
) -> Result<Task, ApplicationError> {
    unit_of_work.transact(|repositories| {
        if ActivityRepository::get(repositories, &command.activity_id)?.is_some() {
            return Err(ApplicationError::AlreadyExists {
                entity: "activity",
                id: command.activity_id.to_string(),
            });
        }

        let mut task = TaskRepository::get(repositories, &command.id)?.ok_or_else(|| {
            ApplicationError::NotFound {
                entity: "task",
                id: command.id.to_string(),
            }
        })?;

        if task.status() == TaskStatus::Inbox {
            task.transition_to(TaskStatus::Ready)?;
        }
        task.transition_to(TaskStatus::InProgress)?;
        TaskRepository::save(repositories, task.clone())?;

        let activity = ActivityRecord::new(
            command.activity_id.clone(),
            ActivitySubject::Task(task.id().clone()),
            ActivityKind::StatusChanged,
            command.origin.clone(),
            clock.now_ms(),
            command.summary.clone(),
        )?;
        ActivityRepository::append(repositories, activity)?;
        enqueue_if_sync_enabled(
            repositories,
            clock.now_ms(),
            PendingSemanticChange::new(
                SemanticEntityKind::Task,
                task.id().to_string(),
                vec![sync_field(
                    "status",
                    Some(task.status().as_str().to_owned()),
                )?],
            )?,
        )?;
        Ok(task)
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransitionTaskWithActivityCommand {
    pub task: TransitionTaskCommand,
    pub activity_id: ActivityId,
    pub origin: ActivityOrigin,
    pub summary: String,
}

/// Transition a task and append its semantic activity in one atomic unit of work.
///
/// This is the reference application contract for state mutation + audit history:
/// adapters must never expose a committed state transition without its matching
/// activity record, or vice versa.
pub fn transition_task_with_activity(
    unit_of_work: &mut impl UnitOfWork,
    clock: &impl Clock,
    command: TransitionTaskWithActivityCommand,
) -> Result<Task, ApplicationError> {
    unit_of_work.transact(|repositories| {
        if ActivityRepository::get(repositories, &command.activity_id)?.is_some() {
            return Err(ApplicationError::AlreadyExists {
                entity: "activity",
                id: command.activity_id.to_string(),
            });
        }

        let task = transition_task(repositories, command.task.clone())?;
        let activity = ActivityRecord::new(
            command.activity_id.clone(),
            ActivitySubject::Task(task.id().clone()),
            if task.status() == TaskStatus::Done {
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
                SemanticEntityKind::Task,
                task.id().to_string(),
                vec![sync_field(
                    "status",
                    Some(task.status().as_str().to_owned()),
                )?],
            )?,
        )?;
        Ok(task)
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetTaskPlanningCommand {
    pub id: TaskId,
    pub planning: TaskPlanning,
}

pub fn set_task_planning(
    repository: &mut impl TaskRepository,
    command: SetTaskPlanningCommand,
) -> Result<Task, ApplicationError> {
    let mut task = repository
        .get(&command.id)?
        .ok_or_else(|| ApplicationError::NotFound {
            entity: "task",
            id: command.id.to_string(),
        })?;
    task.set_planning(command.planning);
    repository.save(task.clone())?;
    Ok(task)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetTaskPlanningWithActivityCommand {
    pub task: SetTaskPlanningCommand,
    pub activity_id: ActivityId,
    pub origin: ActivityOrigin,
    pub summary: String,
}

pub fn set_task_planning_with_activity(
    unit_of_work: &mut impl UnitOfWork,
    clock: &impl Clock,
    command: SetTaskPlanningWithActivityCommand,
) -> Result<Task, ApplicationError> {
    unit_of_work.transact(|repositories| {
        if ActivityRepository::get(repositories, &command.activity_id)?.is_some() {
            return Err(ApplicationError::AlreadyExists {
                entity: "activity",
                id: command.activity_id.to_string(),
            });
        }
        let task = set_task_planning(repositories, command.task.clone())?;
        let activity = ActivityRecord::new(
            command.activity_id.clone(),
            ActivitySubject::Task(task.id().clone()),
            ActivityKind::Updated,
            command.origin.clone(),
            clock.now_ms(),
            command.summary.clone(),
        )?;
        ActivityRepository::append(repositories, activity)?;
        enqueue_if_sync_enabled(
            repositories,
            clock.now_ms(),
            PendingSemanticChange::new(
                SemanticEntityKind::Task,
                task.id().to_string(),
                vec![
                    sync_field(
                        "estimated_minutes",
                        task.planning().estimated_minutes().map(|v| v.to_string()),
                    )?,
                    sync_field(
                        "earliest_start_at_ms",
                        task.planning()
                            .earliest_start_at_ms()
                            .map(|v| v.to_string()),
                    )?,
                    sync_field(
                        "deadline_at_ms",
                        task.planning().deadline_at_ms().map(|v| v.to_string()),
                    )?,
                ],
            )?,
        )?;
        Ok(task)
    })
}
