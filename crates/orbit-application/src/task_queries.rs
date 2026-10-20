use orbit_domain::{Task, TaskId, TaskStatus};

use crate::{ApplicationError, TaskRepository};

pub fn get_task(repository: &impl TaskRepository, id: &TaskId) -> Result<Task, ApplicationError> {
    repository
        .get(id)?
        .ok_or_else(|| ApplicationError::NotFound {
            entity: "task",
            id: id.to_string(),
        })
}

pub fn list_tasks(repository: &impl TaskRepository) -> Result<Vec<Task>, ApplicationError> {
    Ok(repository.list()?)
}

pub fn list_tasks_by_status(
    repository: &impl TaskRepository,
    status: TaskStatus,
) -> Result<Vec<Task>, ApplicationError> {
    let tasks = repository
        .list()?
        .into_iter()
        .filter(|task| task.status() == status)
        .collect();

    Ok(tasks)
}
