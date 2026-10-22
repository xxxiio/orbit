//! Task detail and acceptance-criteria use cases.
//!
//! Details (description, priority, labels) are part of the task aggregate.
//! Acceptance criteria are a separate ordered aggregate with stable identities,
//! so checking them never mutates the task lifecycle implicitly.
use orbit_domain::{
    AcceptanceCriteria, AcceptanceCriterionId, ActivityId, ActivityKind, ActivityOrigin,
    ActivityRecord, ActivitySubject, DomainError, Task, TaskDetails, TaskId, TaskPriority,
};

use crate::{
    AcceptanceCriteriaRepository, ActivityRepository, ApplicationError, Clock, TaskRepository,
    UnitOfWork,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateTaskDetailsCommand {
    pub task_id: TaskId,
    pub description: String,
    pub priority: Option<TaskPriority>,
    pub labels: Vec<String>,
    pub activity_id: ActivityId,
    pub origin: ActivityOrigin,
}

/// Replace the rich task details and record one meaningful activity when the
/// value actually changed. A no-op update writes nothing, avoiding activity
/// noise from refreshes or repeated submissions.
pub fn update_task_details_with_activity(
    unit_of_work: &mut impl UnitOfWork,
    clock: &impl Clock,
    command: UpdateTaskDetailsCommand,
) -> Result<Task, ApplicationError> {
    unit_of_work.transact(|repositories| {
        let mut task = TaskRepository::get(repositories, &command.task_id)?.ok_or_else(|| {
            ApplicationError::NotFound {
                entity: "task",
                id: command.task_id.to_string(),
            }
        })?;
        let previous = task.details().clone();
        let details = TaskDetails::new(
            command.description,
            command.priority,
            command.labels.clone(),
        )?;
        if details == previous {
            return Ok(task);
        }

        let mut changed = Vec::new();
        if details.description() != previous.description() {
            changed.push("description");
        }
        if details.priority() != previous.priority() {
            changed.push("priority");
        }
        if details.labels() != previous.labels() {
            changed.push("labels");
        }
        task.set_details(details);
        TaskRepository::save(repositories, task.clone())?;

        if ActivityRepository::get(repositories, &command.activity_id)?.is_some() {
            return Err(ApplicationError::AlreadyExists {
                entity: "activity",
                id: command.activity_id.to_string(),
            });
        }
        let summary = format!("Task {} {} updated", task.id(), changed.join(", "));
        let activity = ActivityRecord::new(
            command.activity_id.clone(),
            ActivitySubject::Task(task.id().clone()),
            ActivityKind::Updated,
            command.origin.clone(),
            clock.now_ms(),
            summary,
        )?;
        ActivityRepository::append(repositories, activity)?;
        Ok(task)
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AcceptanceCriteriaChange {
    Add {
        criterion_id: AcceptanceCriterionId,
        text: String,
    },
    SetText {
        criterion_id: AcceptanceCriterionId,
        text: String,
    },
    SetCompleted {
        criterion_id: AcceptanceCriterionId,
        completed: bool,
    },
    Reorder {
        criterion_id: AcceptanceCriterionId,
        position: usize,
    },
    Remove {
        criterion_id: AcceptanceCriterionId,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcceptanceCriteriaCommand {
    pub task_id: TaskId,
    pub change: AcceptanceCriteriaChange,
    pub activity_id: ActivityId,
    pub origin: ActivityOrigin,
}

/// Apply one explicit acceptance-criteria operation. The whole ordered list is
/// replaced atomically so positions can never tear.
pub fn apply_acceptance_criteria_change(
    unit_of_work: &mut impl UnitOfWork,
    clock: &impl Clock,
    command: AcceptanceCriteriaCommand,
) -> Result<AcceptanceCriteria, ApplicationError> {
    unit_of_work.transact(|repositories| {
        let exists = TaskRepository::get(repositories, &command.task_id)?.is_some();
        if !exists {
            return Err(ApplicationError::NotFound {
                entity: "task",
                id: command.task_id.to_string(),
            });
        }
        let items =
            AcceptanceCriteriaRepository::list_acceptance_criteria(repositories, &command.task_id)?;
        let mut criteria = AcceptanceCriteria::from_ordered(items)?;

        let summary = match &command.change {
            AcceptanceCriteriaChange::Add { criterion_id, text } => {
                criteria.add(criterion_id.clone(), text.clone())?;
                format!("Acceptance criterion added: {}", truncate(text))
            }
            AcceptanceCriteriaChange::SetText { criterion_id, text } => {
                criteria.set_text(criterion_id, text.clone())?;
                format!("Acceptance criterion updated: {}", truncate(text))
            }
            AcceptanceCriteriaChange::SetCompleted {
                criterion_id,
                completed,
            } => {
                if *completed {
                    criteria.set_completed(criterion_id, Some(clock.now_ms()))?;
                    "Acceptance criterion checked".to_owned()
                } else {
                    criteria.set_completed(criterion_id, None)?;
                    "Acceptance criterion reopened".to_owned()
                }
            }
            AcceptanceCriteriaChange::Reorder {
                criterion_id,
                position,
            } => {
                criteria.reorder(criterion_id, *position)?;
                "Acceptance criteria reordered".to_owned()
            }
            AcceptanceCriteriaChange::Remove { criterion_id } => {
                criteria.remove(criterion_id)?;
                "Acceptance criterion removed".to_owned()
            }
        };

        AcceptanceCriteriaRepository::replace_acceptance_criteria(
            repositories,
            &command.task_id,
            &criteria,
        )?;
        if ActivityRepository::get(repositories, &command.activity_id)?.is_some() {
            return Err(ApplicationError::AlreadyExists {
                entity: "activity",
                id: command.activity_id.to_string(),
            });
        }
        let activity = ActivityRecord::new(
            command.activity_id.clone(),
            ActivitySubject::Task(command.task_id.clone()),
            ActivityKind::Updated,
            command.origin.clone(),
            clock.now_ms(),
            summary,
        )?;
        ActivityRepository::append(repositories, activity)?;
        Ok(criteria)
    })
}

/// Read the ordered criteria without starting a write transaction.
pub fn list_acceptance_criteria(
    repository: &impl AcceptanceCriteriaRepository,
    task_id: &TaskId,
) -> Result<AcceptanceCriteria, ApplicationError> {
    let items = repository.list_acceptance_criteria(task_id)?;
    Ok(AcceptanceCriteria::from_ordered(items)?)
}

fn truncate(text: &str) -> String {
    const LIMIT: usize = 80;
    let mut value: String = text.chars().take(LIMIT).collect();
    if text.chars().count() > LIMIT {
        value.push_str("...");
    }
    value
}

/// Reject a priority string with the domain error used by every adapter.
pub fn parse_task_priority(value: &str) -> Result<TaskPriority, DomainError> {
    TaskPriority::from_str_checked(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summaries_are_truncated_without_splitting_characters() {
        let long = "é".repeat(200);
        let summary = truncate(&long);
        assert!(summary.ends_with("..."));
        assert_eq!(summary.chars().count(), 83);
    }
}
