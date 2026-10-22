//! Rich task, attachment and workflow commands on the unified connection.
//!
//! The embedded transport has a real authoritative round-trip and reuses the
//! exact command helpers of `OrbitClient`. The connected transport does not:
//! the server accepts neither rich task commands nor attachment uploads yet, so
//! every such method fails closed with a typed `Unavailable` failure instead of
//! queuing a command the server is guaranteed to reject.
use crate::{
    AcceptanceCriterionDto, ActivityDto, AttachmentDto, ClientError, ProjectDto,
    ProjectWorkflowDto, TaskDto, WorkflowDescriptorDto,
};

use super::{AuthorityConnection, ClientConnectionError};

/// Documented refusal for attachment operations in connected mode.
pub const ATTACHMENTS_UNAVAILABLE_MESSAGE: &str = "attachments are not available in connected mode in this build; the server does not yet accept attachment uploads";
/// Documented refusal for rich task commands in connected mode.
pub const TASK_DETAILS_UNAVAILABLE_MESSAGE: &str = "connected mode does not yet support task descriptions or acceptance criteria; use the local workspace";
/// Documented refusal for per-task activity history in connected mode.
pub const TASK_ACTIVITY_UNAVAILABLE_MESSAGE: &str =
    "connected mode does not yet project per-task activity history; use the local workspace";
/// Documented refusal for workflow binding changes in connected mode.
pub const PROJECT_WORKFLOW_UNAVAILABLE_MESSAGE: &str =
    "connected mode does not yet support changing a project workflow; use the local workspace";

/// One constructor for honest "not implemented in this build" failures so the
/// wording cannot drift between methods.
fn unavailable(message: &str) -> ClientConnectionError {
    ClientConnectionError::Unavailable(message.to_owned())
}

impl AuthorityConnection {
    /// Replace rich task details. Only the embedded transport can accept this.
    pub fn update_task_details(
        &mut self,
        id: String,
        description: String,
        priority: Option<String>,
        labels: Vec<String>,
    ) -> Result<TaskDto, ClientConnectionError> {
        match self {
            Self::Embedded(client) => {
                let (actor, host) = client.command_context();
                crate::task_details::host_update_task_details(
                    host,
                    &actor,
                    id,
                    description,
                    priority,
                    labels,
                )
                .map_err(ClientConnectionError::from)
            }
            Self::Remote { .. } => Err(unavailable(TASK_DETAILS_UNAVAILABLE_MESSAGE)),
        }
    }

    /// Ordered acceptance criteria for one task.
    pub fn acceptance_criteria(
        &self,
        task_id: &str,
    ) -> Result<Vec<AcceptanceCriterionDto>, ClientConnectionError> {
        match self {
            Self::Embedded(client) => {
                crate::task_details::host_acceptance_criteria(client.host(), task_id)
                    .map_err(ClientConnectionError::from)
            }
            Self::Remote { .. } => Err(unavailable(TASK_DETAILS_UNAVAILABLE_MESSAGE)),
        }
    }

    /// Append one acceptance criterion.
    pub fn add_acceptance_criterion(
        &mut self,
        task_id: String,
        text: String,
    ) -> Result<Vec<AcceptanceCriterionDto>, ClientConnectionError> {
        match self {
            Self::Embedded(client) => {
                let (actor, host) = client.command_context();
                crate::task_details::host_add_acceptance_criterion(host, &actor, task_id, text)
                    .map_err(ClientConnectionError::from)
            }
            Self::Remote { .. } => Err(unavailable(TASK_DETAILS_UNAVAILABLE_MESSAGE)),
        }
    }

    /// Replace one criterion's text.
    pub fn set_acceptance_criterion_text(
        &mut self,
        task_id: String,
        criterion_id: String,
        text: String,
    ) -> Result<Vec<AcceptanceCriterionDto>, ClientConnectionError> {
        self.acceptance_change(
            task_id,
            orbit_application::AcceptanceCriteriaChange::SetText {
                criterion_id: criterion_id_from(&criterion_id)?,
                text,
            },
        )
    }

    /// Check or reopen one criterion.
    pub fn set_acceptance_criterion_completed(
        &mut self,
        task_id: String,
        criterion_id: String,
        completed: bool,
    ) -> Result<Vec<AcceptanceCriterionDto>, ClientConnectionError> {
        self.acceptance_change(
            task_id,
            orbit_application::AcceptanceCriteriaChange::SetCompleted {
                criterion_id: criterion_id_from(&criterion_id)?,
                completed,
            },
        )
    }

    /// Move one criterion to a zero-based position.
    pub fn move_acceptance_criterion(
        &mut self,
        task_id: String,
        criterion_id: String,
        position: usize,
    ) -> Result<Vec<AcceptanceCriterionDto>, ClientConnectionError> {
        self.acceptance_change(
            task_id,
            orbit_application::AcceptanceCriteriaChange::Reorder {
                criterion_id: criterion_id_from(&criterion_id)?,
                position,
            },
        )
    }

    /// Remove one criterion.
    pub fn remove_acceptance_criterion(
        &mut self,
        task_id: String,
        criterion_id: String,
    ) -> Result<Vec<AcceptanceCriterionDto>, ClientConnectionError> {
        self.acceptance_change(
            task_id,
            orbit_application::AcceptanceCriteriaChange::Remove {
                criterion_id: criterion_id_from(&criterion_id)?,
            },
        )
    }

    /// Per-task durable activity, newest first.
    pub fn task_activity(&self, task_id: &str) -> Result<Vec<ActivityDto>, ClientConnectionError> {
        match self {
            Self::Embedded(client) => {
                crate::task_details::host_task_activity(client.host(), task_id)
                    .map_err(ClientConnectionError::from)
            }
            Self::Remote { .. } => Err(unavailable(TASK_ACTIVITY_UNAVAILABLE_MESSAGE)),
        }
    }

    /// Attachment metadata for one owner.
    pub fn list_attachments(
        &self,
        owner_kind: &str,
        owner_id: &str,
    ) -> Result<Vec<AttachmentDto>, ClientConnectionError> {
        match self {
            Self::Embedded(client) => {
                crate::attachments::host_list_attachments(client.host(), owner_kind, owner_id)
                    .map_err(ClientConnectionError::from)
            }
            Self::Remote { .. } => Err(unavailable(ATTACHMENTS_UNAVAILABLE_MESSAGE)),
        }
    }

    /// Import a local regular file as a new attachment.
    pub fn import_attachment(
        &mut self,
        owner_kind: String,
        owner_id: String,
        path: String,
        description: Option<String>,
    ) -> Result<AttachmentDto, ClientConnectionError> {
        match self {
            Self::Embedded(client) => {
                let (actor, host) = client.command_context();
                crate::attachments::host_import_attachment(
                    host,
                    &actor,
                    owner_kind,
                    owner_id,
                    path,
                    description,
                )
                .map_err(ClientConnectionError::from)
            }
            Self::Remote { .. } => Err(unavailable(ATTACHMENTS_UNAVAILABLE_MESSAGE)),
        }
    }

    /// Attachment metadata plus verified content.
    pub fn attachment_content(
        &self,
        id: &str,
    ) -> Result<(AttachmentDto, Vec<u8>), ClientConnectionError> {
        match self {
            Self::Embedded(client) => {
                crate::attachments::host_attachment_content(client.host(), id)
                    .map_err(ClientConnectionError::from)
            }
            Self::Remote { .. } => Err(unavailable(ATTACHMENTS_UNAVAILABLE_MESSAGE)),
        }
    }

    /// Copy attachment content to a new local file, never overwriting.
    pub fn export_attachment(
        &self,
        id: &str,
        destination: String,
    ) -> Result<AttachmentDto, ClientConnectionError> {
        match self {
            Self::Embedded(client) => {
                crate::attachments::host_export_attachment(client.host(), id, destination)
                    .map_err(ClientConnectionError::from)
            }
            Self::Remote { .. } => Err(unavailable(ATTACHMENTS_UNAVAILABLE_MESSAGE)),
        }
    }

    /// Remove one attachment association.
    pub fn remove_attachment(&mut self, id: String) -> Result<bool, ClientConnectionError> {
        match self {
            Self::Embedded(client) => {
                let (actor, host) = client.command_context();
                crate::attachments::host_remove_attachment(host, &actor, id)
                    .map_err(ClientConnectionError::from)
            }
            Self::Remote { .. } => Err(unavailable(ATTACHMENTS_UNAVAILABLE_MESSAGE)),
        }
    }

    /// Remove unreferenced attachment content objects.
    pub fn purge_attachment_content(&mut self) -> Result<usize, ClientConnectionError> {
        match self {
            Self::Embedded(client) => {
                crate::attachments::host_purge_attachment_content(client.host_mut())
                    .map_err(ClientConnectionError::from)
            }
            Self::Remote { .. } => Err(unavailable(ATTACHMENTS_UNAVAILABLE_MESSAGE)),
        }
    }

    /// Workflows compiled into this build. This is a static descriptor read, so
    /// it works identically offline, embedded, and connected.
    pub fn bundled_workflows(&self) -> Vec<WorkflowDescriptorDto> {
        crate::task_details::bundled_workflows()
    }

    /// Pin a project to a workflow.
    pub fn apply_workflow(
        &mut self,
        project_id: String,
        workflow_id: String,
        version: Option<u32>,
    ) -> Result<ProjectDto, ClientConnectionError> {
        match self {
            Self::Embedded(client) => {
                let (actor, host) = client.command_context();
                crate::task_details::host_apply_workflow(
                    host,
                    &actor,
                    project_id,
                    workflow_id,
                    version,
                )
                .map_err(ClientConnectionError::from)
            }
            Self::Remote { .. } => Err(unavailable(PROJECT_WORKFLOW_UNAVAILABLE_MESSAGE)),
        }
    }

    /// The workflow pinned to a project, when a projection is available.
    pub fn project_workflow(
        &self,
        project_id: &str,
    ) -> Result<Option<ProjectWorkflowDto>, ClientConnectionError> {
        match self {
            Self::Embedded(client) => {
                crate::task_details::host_project_workflow(client.host(), project_id)
                    .map_err(ClientConnectionError::from)
            }
            // The accepted snapshot does not carry workflow bindings yet, and
            // fabricating one from a bundled default would be a guess.
            Self::Remote { .. } => Err(unavailable(PROJECT_WORKFLOW_UNAVAILABLE_MESSAGE)),
        }
    }

    fn acceptance_change(
        &mut self,
        task_id: String,
        change: orbit_application::AcceptanceCriteriaChange,
    ) -> Result<Vec<AcceptanceCriterionDto>, ClientConnectionError> {
        match self {
            Self::Embedded(client) => {
                let (actor, host) = client.command_context();
                crate::task_details::host_apply_acceptance_criteria(host, &actor, task_id, change)
                    .map_err(ClientConnectionError::from)
            }
            Self::Remote { .. } => Err(unavailable(TASK_DETAILS_UNAVAILABLE_MESSAGE)),
        }
    }
}

fn criterion_id_from(
    criterion_id: &str,
) -> Result<orbit_domain::AcceptanceCriterionId, ClientConnectionError> {
    orbit_domain::AcceptanceCriterionId::new(criterion_id.to_owned()).map_err(|error| {
        ClientError {
            code: crate::ClientErrorCode::InvalidInput,
            message: format!("{error:?}"),
        }
        .into()
    })
}
