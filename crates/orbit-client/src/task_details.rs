//! Rich task details, ordered acceptance criteria and workflow offers.
//!
//! Every mutation goes through the canonical accepted commands of the open
//! authoritative host: the caller reads the current revision, the command and
//! activity identities are generated per request, and the result is re-read
//! through application queries. No client-side state becomes authoritative.
//!
//! The `host_*` helpers take an already-open host so the unified
//! `AuthorityConnection` embedded transport and the `OrbitClient` facade share
//! exactly one implementation instead of forking the command path.
use orbit_application::{
    AcceptanceCriteriaChange, AcceptanceCriteriaCommand, UpdateTaskDetailsCommand,
};
use orbit_domain::{
    AcceptanceCriterionId, ActivityOrigin, ActivityRecord, ActivitySubject, ProjectId, TaskId,
    TaskPriority,
};
use orbit_runtime::AuthoritativeHost;

use crate::facade_conversions::{
    authority_client_error, generate_id, invalid_input, new_activity_id,
};
use crate::{
    AcceptanceCriterionDto, ActivityDto, ClientError, ClientErrorCode, OrbitClient, ProjectDto,
    ProjectWorkflowDto, TaskDto, WorkflowDescriptorDto, acceptance_criteria_dtos,
};

impl OrbitClient {
    /// Replace the rich task details (description, priority, labels).
    ///
    /// Priority is validated with the same domain parser every other adapter
    /// uses; an empty/blank string means "no priority". Labels are trimmed,
    /// de-duplicated and validated by the task aggregate. An unchanged update
    /// writes nothing and records no activity.
    pub fn update_task_details(
        &mut self,
        id: String,
        description: String,
        priority: Option<String>,
        labels: Vec<String>,
    ) -> Result<TaskDto, ClientError> {
        host_update_task_details(
            &mut self.runtime,
            "orbit-client",
            id,
            description,
            priority,
            labels,
        )
    }

    /// Ordered acceptance criteria for one task.
    pub fn acceptance_criteria(
        &self,
        task_id: &str,
    ) -> Result<Vec<AcceptanceCriterionDto>, ClientError> {
        host_acceptance_criteria(&self.runtime, task_id)
    }

    /// Append one criterion at the end of the ordered list.
    pub fn add_acceptance_criterion(
        &mut self,
        task_id: String,
        text: String,
    ) -> Result<Vec<AcceptanceCriterionDto>, ClientError> {
        host_add_acceptance_criterion(&mut self.runtime, "orbit-client", task_id, text)
    }

    /// Replace one criterion's text, preserving completion and position.
    pub fn set_acceptance_criterion_text(
        &mut self,
        task_id: String,
        criterion_id: String,
        text: String,
    ) -> Result<Vec<AcceptanceCriterionDto>, ClientError> {
        host_apply_acceptance_criteria(
            &mut self.runtime,
            "orbit-client",
            task_id,
            AcceptanceCriteriaChange::SetText {
                criterion_id: AcceptanceCriterionId::new(criterion_id).map_err(invalid_input)?,
                text,
            },
        )
    }

    /// Check or reopen one criterion. Completion never changes task status.
    pub fn set_acceptance_criterion_completed(
        &mut self,
        task_id: String,
        criterion_id: String,
        completed: bool,
    ) -> Result<Vec<AcceptanceCriterionDto>, ClientError> {
        host_apply_acceptance_criteria(
            &mut self.runtime,
            "orbit-client",
            task_id,
            AcceptanceCriteriaChange::SetCompleted {
                criterion_id: AcceptanceCriterionId::new(criterion_id).map_err(invalid_input)?,
                completed,
            },
        )
    }

    /// Move one criterion to a zero-based position, shifting the others.
    pub fn move_acceptance_criterion(
        &mut self,
        task_id: String,
        criterion_id: String,
        position: usize,
    ) -> Result<Vec<AcceptanceCriterionDto>, ClientError> {
        host_apply_acceptance_criteria(
            &mut self.runtime,
            "orbit-client",
            task_id,
            AcceptanceCriteriaChange::Reorder {
                criterion_id: AcceptanceCriterionId::new(criterion_id).map_err(invalid_input)?,
                position,
            },
        )
    }

    /// Remove one criterion from the ordered list.
    pub fn remove_acceptance_criterion(
        &mut self,
        task_id: String,
        criterion_id: String,
    ) -> Result<Vec<AcceptanceCriterionDto>, ClientError> {
        host_apply_acceptance_criteria(
            &mut self.runtime,
            "orbit-client",
            task_id,
            AcceptanceCriteriaChange::Remove {
                criterion_id: AcceptanceCriterionId::new(criterion_id).map_err(invalid_input)?,
            },
        )
    }

    /// Durable semantic activity for one task, newest first.
    pub fn task_activity(&self, task_id: &str) -> Result<Vec<ActivityDto>, ClientError> {
        host_task_activity(&self.runtime, task_id)
    }

    /// Workflows that ship with Orbit. Available without a connection because
    /// the descriptors are compiled into the binary.
    pub fn bundled_workflows(&self) -> Vec<WorkflowDescriptorDto> {
        bundled_workflows()
    }

    /// Pin a project to a workflow, then return the re-read project.
    ///
    /// The authority resolves the version (explicit, bundled descriptor, or
    /// latest published), installs a bundled manifest on demand and rejects
    /// unknown projects/workflows. A duplicate command id is idempotent.
    pub fn apply_workflow(
        &mut self,
        project_id: String,
        workflow_id: String,
        version: Option<u32>,
    ) -> Result<ProjectDto, ClientError> {
        host_apply_workflow(
            &mut self.runtime,
            "orbit-client",
            project_id,
            workflow_id,
            version,
        )
    }

    /// The workflow currently pinned to one project, if any.
    pub fn project_workflow(
        &self,
        project_id: &str,
    ) -> Result<Option<ProjectWorkflowDto>, ClientError> {
        host_project_workflow(&self.runtime, project_id)
    }

    /// Remove a project's workflow binding so it falls back to generated
    /// default views. Returns true when a binding existed. Explicit and
    /// auditable; never triggered implicitly.
    pub fn remove_project_workflow(&mut self, project_id: String) -> Result<bool, ClientError> {
        host_remove_project_workflow(&mut self.runtime, "orbit-client", project_id)
    }
}

/// Unpin a project's workflow through the durable command ledger.
pub(crate) fn host_remove_project_workflow(
    host: &mut AuthoritativeHost,
    actor: &str,
    project_id: String,
) -> Result<bool, ClientError> {
    let id = ProjectId::new(project_id).map_err(invalid_input)?;
    let revision = host.snapshot().map_err(authority_client_error)?.0;
    let (_, _, removed) = host
        .remove_project_workflow_accepted(actor, &generate_id("command"), revision, id)
        .map_err(authority_client_error)?;
    Ok(removed)
}

pub(crate) fn host_update_task_details(
    host: &mut AuthoritativeHost,
    actor: &str,
    id: String,
    description: String,
    priority: Option<String>,
    labels: Vec<String>,
) -> Result<TaskDto, ClientError> {
    let task_id = TaskId::new(id).map_err(invalid_input)?;
    let priority = parse_optional_priority(priority)?;
    let revision = host.snapshot().map_err(authority_client_error)?.0;
    host.update_task_details_accepted(
        actor,
        &generate_id("command"),
        revision,
        UpdateTaskDetailsCommand {
            task_id: task_id.clone(),
            description,
            priority,
            labels,
            activity_id: new_activity_id()?,
            origin: ActivityOrigin::Human,
        },
    )
    .map_err(authority_client_error)?;
    let task = host
        .application_queries()
        .task(&task_id)
        .map_err(ClientError::from)?;
    Ok(TaskDto::from(&task))
}

pub(crate) fn host_acceptance_criteria(
    host: &AuthoritativeHost,
    task_id: &str,
) -> Result<Vec<AcceptanceCriterionDto>, ClientError> {
    let task_id = TaskId::new(task_id.to_owned()).map_err(invalid_input)?;
    let criteria = host
        .acceptance_criteria(&task_id)
        .map_err(ClientError::from)?;
    Ok(acceptance_criteria_dtos(&criteria))
}

pub(crate) fn host_add_acceptance_criterion(
    host: &mut AuthoritativeHost,
    actor: &str,
    task_id: String,
    text: String,
) -> Result<Vec<AcceptanceCriterionDto>, ClientError> {
    let criterion_id =
        AcceptanceCriterionId::new(generate_id("criterion")).map_err(invalid_input)?;
    host_apply_acceptance_criteria(
        host,
        actor,
        task_id,
        AcceptanceCriteriaChange::Add { criterion_id, text },
    )
}

pub(crate) fn host_apply_acceptance_criteria(
    host: &mut AuthoritativeHost,
    actor: &str,
    task_id: String,
    change: AcceptanceCriteriaChange,
) -> Result<Vec<AcceptanceCriterionDto>, ClientError> {
    let task_id = TaskId::new(task_id).map_err(invalid_input)?;
    let command = AcceptanceCriteriaCommand {
        task_id: task_id.clone(),
        change,
        activity_id: new_activity_id()?,
        origin: ActivityOrigin::Human,
    };
    let revision = host.snapshot().map_err(authority_client_error)?.0;
    host.apply_acceptance_criteria_accepted(actor, &generate_id("command"), revision, command)
        .map_err(authority_client_error)?;
    host_acceptance_criteria(host, task_id.as_str())
}

pub(crate) fn host_task_activity(
    host: &AuthoritativeHost,
    task_id: &str,
) -> Result<Vec<ActivityDto>, ClientError> {
    let task_id = TaskId::new(task_id.to_owned()).map_err(invalid_input)?;
    let activities = host
        .application_queries()
        .activities()
        .map_err(ClientError::from)?;
    let mut records: Vec<&ActivityRecord> = activities
        .iter()
        .filter(|activity| match activity.subject() {
            ActivitySubject::Task(subject) => subject == &task_id,
            _ => false,
        })
        .collect();
    // The repository returns append order (sequence). Reverse it for newest
    // first so equal-millisecond events stay deterministic.
    records.reverse();
    Ok(records.into_iter().map(ActivityDto::from).collect())
}

/// Descriptors of workflows compiled into this build.
pub fn bundled_workflows() -> Vec<WorkflowDescriptorDto> {
    orbit_blueprint::bundled::BUNDLED_BLUEPRINTS
        .iter()
        .map(|descriptor| WorkflowDescriptorDto {
            id: descriptor.id.to_owned(),
            name: descriptor.name.to_owned(),
            description: descriptor.description.to_owned(),
            version: descriptor.version,
            bundled: true,
            is_default: descriptor.id == "basic",
        })
        .collect()
}

pub(crate) fn host_apply_workflow(
    host: &mut AuthoritativeHost,
    actor: &str,
    project_id: String,
    workflow_id: String,
    version: Option<u32>,
) -> Result<ProjectDto, ClientError> {
    let id = ProjectId::new(project_id).map_err(invalid_input)?;
    let revision = host.snapshot().map_err(authority_client_error)?.0;
    host.set_project_workflow_accepted(
        actor,
        &generate_id("command"),
        revision,
        id.clone(),
        workflow_id,
        version,
    )
    .map_err(authority_client_error)?;
    let project = host
        .application_queries()
        .project(&id)
        .map_err(ClientError::from)?;
    Ok(ProjectDto::from(&project))
}

pub(crate) fn host_project_workflow(
    host: &AuthoritativeHost,
    project_id: &str,
) -> Result<Option<ProjectWorkflowDto>, ClientError> {
    let binding = host
        .active_project_blueprint(project_id)
        .map_err(ClientError::from)?;
    Ok(binding.map(|binding| ProjectWorkflowDto {
        project_id: binding.project_id,
        workflow_id: binding.blueprint_id,
        version: binding.version,
    }))
}

/// Parse a caller-supplied priority. Blank means "no priority"; anything else
/// must be one of the domain values.
fn parse_optional_priority(priority: Option<String>) -> Result<Option<TaskPriority>, ClientError> {
    match priority {
        None => Ok(None),
        Some(value) if value.trim().is_empty() => Ok(None),
        Some(value) => orbit_application::parse_task_priority(&value)
            .map(Some)
            .map_err(|error| ClientError {
                code: ClientErrorCode::InvalidInput,
                message: format!(
                    "invalid task priority {value:?}: {error:?}; allowed values are low, medium, high, urgent"
                ),
            }),
    }
}

/// Parse one attachment owner kind with an actionable message on failure.
pub(crate) fn parse_attachment_owner(
    owner_kind: &str,
    owner_id: &str,
) -> Result<orbit_domain::AttachmentOwner, ClientError> {
    let kind = orbit_domain::AttachmentOwnerKind::parse(owner_kind.trim()).ok_or_else(|| {
        ClientError {
            code: ClientErrorCode::InvalidInput,
            message: format!(
                "unknown attachment owner kind {owner_kind:?}; allowed values are task, project, work_item, knowledge, agent_job"
            ),
        }
    })?;
    orbit_domain::AttachmentOwner::new(kind, owner_id).map_err(invalid_input)
}
