//! Rich task, attachment and workflow commands on the unified connection.
//!
//! The embedded transport has a real authoritative round-trip and reuses the
//! exact command helpers of `OrbitClient`. The connected transport stages
//! attachment bytes durably, uploads them with a validated receipt, and reads
//! accepted server projections back. Workflow binding changes remain refused.
use crate::{
    AcceptanceCriterionDto, ActivityDto, AttachmentDto, ClientError, PendingAttachmentDto,
    ProjectDto, ProjectWorkflowDto, TaskDto, WorkflowDescriptorDto,
};
use orbit_application::AcceptanceCriteriaChange;
use orbit_runtime::domain_commands::{
    AcceptanceCriteriaAction, TaskAction, TaskCommand, TaskRecord,
};
use serde::Deserialize;
use serde_json::Value;

use super::{AuthorityConnection, ClientConnectionError, encode_query};

/// Server-side attachment garbage collection is not exposed over HTTP, so the
/// connected build must not imply that a client purge is authoritative.
pub const PURGE_ATTACHMENTS_UNAVAILABLE_MESSAGE: &str = "connected mode cannot purge attachment content; the authoritative server owns garbage collection";
/// Accepted attachment reads need the server because the accepted snapshot
/// does not carry attachment metadata or bytes.
pub const ATTACHMENTS_OFFLINE_MESSAGE: &str = "offline: connected attachment reads require a server connection; uploads staged on this replica can still be listed and discarded";
/// Documented refusal for workflow binding changes in connected mode.
pub const PROJECT_WORKFLOW_UNAVAILABLE_MESSAGE: &str =
    "connected mode does not yet support changing a project workflow; use the local workspace";
/// Acceptance criteria and per-task activity are not part of the accepted
/// snapshot, so reading them requires a live server. Task detail is
/// snapshot-first and only falls back to the live route when absent.
pub const TASK_READS_OFFLINE_MESSAGE: &str = "offline: connected acceptance criteria and task activity require a server connection (task detail is read from the accepted snapshot)";
/// A connected task-detail or criteria mutation needs an accepted revision so
/// the server can detect a stale write.
pub const TASK_DETAILS_ACCEPTED_STATE_MESSAGE: &str =
    "offline: task detail mutations require an accepted authoritative snapshot";

/// One constructor for honest "not implemented in this build" failures so the
/// wording cannot drift between methods.
fn unavailable(message: &str) -> ClientConnectionError {
    ClientConnectionError::Unavailable(message.to_owned())
}

impl AuthorityConnection {
    /// Replace rich task details.
    ///
    /// Connected mode queues the canonical `SetDetails` task command and then
    /// returns the currently accepted projection. The queued value never
    /// appears in the accepted read before the server acknowledges it.
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
            Self::Remote { replica, .. } => {
                let details = canonical_details(&description, priority, &labels)?;
                let revision = replica.workspace()?.revision.ok_or_else(|| {
                    ClientConnectionError::Unsupported(TASK_DETAILS_ACCEPTED_STATE_MESSAGE)
                })?;
                // Read the accepted projection *before* queueing. If the read
                // fails the caller sees an error and nothing is queued, so a
                // retry cannot create a second command identity for the same
                // logical edit.
                let accepted = self.accepted_task(&id)?;
                self.task_command(TaskCommand {
                    command_id: crate::facade_conversions::generate_id("task-details"),
                    expected_revision: revision,
                    action: TaskAction::SetDetails {
                        task_id: id,
                        description: details.description().to_owned(),
                        priority: details.priority().map(|value| value.as_str().to_owned()),
                        labels: details.labels().to_vec(),
                    },
                })?;
                Ok(accepted)
            }
        }
    }

    /// Ordered acceptance criteria for one task, from authoritative state.
    pub fn acceptance_criteria(
        &self,
        task_id: &str,
    ) -> Result<Vec<AcceptanceCriterionDto>, ClientConnectionError> {
        match self {
            Self::Embedded(client) => {
                crate::task_details::host_acceptance_criteria(client.host(), task_id)
                    .map_err(ClientConnectionError::from)
            }
            Self::Remote { http, .. } => {
                let transport = http.as_ref().ok_or_else(|| {
                    ClientConnectionError::Unsupported(TASK_READS_OFFLINE_MESSAGE)
                })?;
                let values = transport.get_json(&format!(
                    "/v1/authoritative/tasks/{}/acceptance-criteria",
                    encode_query(task_id)
                ))?;
                let criteria: Vec<AcceptanceCriterionWire> = serde_json::from_value(values)?;
                Ok(criteria.into_iter().map(Into::into).collect())
            }
        }
    }

    /// Append one acceptance criterion.
    ///
    /// Connected mode queues the wire operation and returns the accepted list
    /// (which still excludes the pending addition).
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
            Self::Remote { .. } => {
                let criterion_id = orbit_domain::AcceptanceCriterionId::new(
                    crate::facade_conversions::generate_id("criterion"),
                )
                .map_err(crate::facade_conversions::invalid_input)?;
                validate_criterion_text(&criterion_id, &text)?;
                self.acceptance_change(
                    task_id,
                    AcceptanceCriteriaChange::Add { criterion_id, text },
                )
            }
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
            AcceptanceCriteriaChange::SetText {
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
            AcceptanceCriteriaChange::SetCompleted {
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
            AcceptanceCriteriaChange::Reorder {
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
            AcceptanceCriteriaChange::Remove {
                criterion_id: criterion_id_from(&criterion_id)?,
            },
        )
    }

    /// Per-task durable activity, newest first, from authoritative state.
    pub fn task_activity(&self, task_id: &str) -> Result<Vec<ActivityDto>, ClientConnectionError> {
        match self {
            Self::Embedded(client) => {
                crate::task_details::host_task_activity(client.host(), task_id)
                    .map_err(ClientConnectionError::from)
            }
            Self::Remote { http, .. } => {
                let transport = http.as_ref().ok_or_else(|| {
                    ClientConnectionError::Unsupported(TASK_READS_OFFLINE_MESSAGE)
                })?;
                let values = transport.get_json(&format!(
                    "/v1/authoritative/tasks/{}/activity",
                    encode_query(task_id)
                ))?;
                let activity: Vec<ActivityWire> = serde_json::from_value(values)?;
                Ok(activity.into_iter().map(Into::into).collect())
            }
        }
    }

    /// One task with its rich details.
    ///
    /// Connected mode reads the authoritative task-detail route so the caller
    /// sees the same projection embedded mode exposes. A 404 surfaces as a
    /// typed remote error naming the missing task.
    pub fn task_detail(&self, id: &str) -> Result<TaskDto, ClientConnectionError> {
        match self {
            Self::Embedded(client) => {
                let task_id = orbit_domain::TaskId::new(id.to_owned())
                    .map_err(crate::facade_conversions::invalid_input)?;
                let task = client
                    .host()
                    .application_queries()
                    .task(&task_id)
                    .map_err(ClientError::from)?;
                Ok(TaskDto::from(&task))
            }
            Self::Remote { .. } => self.remote_task_detail(id),
        }
    }

    /// Attachment metadata for one owner, from accepted server state.
    ///
    /// Pending local uploads are never merged into this list; use
    /// [`Self::staged_attachments`] for those.
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
            Self::Remote { replica, http } => {
                let owner = crate::task_details::parse_attachment_owner(owner_kind, owner_id)?;
                let transport = http
                    .as_ref()
                    .ok_or_else(|| unavailable(ATTACHMENTS_OFFLINE_MESSAGE))?;
                let value = transport
                    .get_json(&format!(
                        "/v1/authoritative/attachments?owner_kind={}&owner_id={}",
                        encode_query(owner.kind().as_str()),
                        encode_query(owner.id())
                    ))
                    .map_err(|error| match error {
                        crate::authority::AuthorityError::Transport(cause) => {
                            ClientConnectionError::Unavailable(format!(
                                "{ATTACHMENTS_OFFLINE_MESSAGE} ({cause})"
                            ))
                        }
                        other => other.into(),
                    })?;
                let attachments = crate::attachments_remote::dtos_from_wire(&value)?;
                // Accepted metadata is cached so a later content read can
                // verify the download against known metadata. This never
                // accepts a staged upload.
                replica.cache_accepted_attachments(&attachments)?;
                Ok(attachments)
            }
        }
    }

    /// Unacknowledged uploads for one owner, in staging order.
    ///
    /// These are durable local intent. Present them separately from
    /// [`Self::list_attachments`]; a staged upload does not exist on the server
    /// until a flush returns a validated receipt.
    pub fn staged_attachments(
        &self,
        owner_kind: &str,
        owner_id: &str,
    ) -> Result<Vec<PendingAttachmentDto>, ClientConnectionError> {
        match self {
            Self::Embedded(_) => Ok(Vec::new()),
            Self::Remote { replica, .. } => {
                let owner = crate::task_details::parse_attachment_owner(owner_kind, owner_id)?;
                Ok(replica.staged_attachments(owner.kind().as_str(), owner.id())?)
            }
        }
    }

    /// Every unacknowledged upload across owners, in creation order.
    pub fn pending_attachment_uploads(
        &self,
    ) -> Result<Vec<PendingAttachmentDto>, ClientConnectionError> {
        match self {
            Self::Embedded(_) => Ok(Vec::new()),
            Self::Remote { replica, .. } => Ok(replica.pending_attachment_uploads()?),
        }
    }

    /// True when this replica holds staged bytes the server has not accepted.
    pub fn has_pending_attachment_uploads(&self) -> Result<bool, ClientConnectionError> {
        match self {
            Self::Embedded(_) => Ok(false),
            Self::Remote { replica, .. } => Ok(replica.has_pending_attachment_uploads()?),
        }
    }

    /// Number of uploads the server has not accepted yet (pending plus
    /// rejected-until-corrected rows).
    pub fn pending_attachment_upload_count(&self) -> Result<i64, ClientConnectionError> {
        match self {
            Self::Embedded(_) => Ok(0),
            Self::Remote { replica, .. } => Ok(replica.pending_attachment_uploads()?.len() as i64),
        }
    }

    /// Import a local regular file as a new attachment.
    ///
    /// Connected mode validates and stages the bytes durably, then attempts one
    /// upload. A returned DTO is canonical server metadata only when the server
    /// accepted it; `has_pending_attachment_uploads` reports the rest. Symbolic
    /// links, non-regular files and files above `ORBIT_ATTACHMENT_MAX_BYTES`
    /// are rejected before any bytes are staged.
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
            Self::Remote { replica, http } => {
                let owner = crate::task_details::parse_attachment_owner(&owner_kind, &owner_id)?;
                let (filename, content_type, content) =
                    crate::attachments::read_attachment_source(&path)?;
                Ok(crate::attachments_remote::import_attachment(
                    replica.replica_mut(),
                    http.as_ref(),
                    owner.kind().as_str(),
                    owner.id(),
                    &filename,
                    &content_type,
                    &content,
                    description,
                )?)
            }
        }
    }

    /// Attachment metadata plus verified content.
    ///
    /// A staged id is read from this replica's verified staging file; an
    /// accepted id is downloaded and verified against `x-orbit-checksum`,
    /// `x-orbit-size` and cached accepted metadata.
    pub fn attachment_content(
        &self,
        id: &str,
    ) -> Result<(AttachmentDto, Vec<u8>), ClientConnectionError> {
        match self {
            Self::Embedded(client) => {
                crate::attachments::host_attachment_content(client.host(), id)
                    .map_err(ClientConnectionError::from)
            }
            Self::Remote { replica, http } => Ok(crate::attachments_remote::attachment_content(
                replica.replica(),
                http.as_ref(),
                id,
            )?),
        }
    }

    /// Copy verified attachment content to a new local file, never overwriting.
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
            Self::Remote { replica, http } => Ok(crate::attachments_remote::export_attachment(
                replica.replica(),
                http.as_ref(),
                id,
                &destination,
            )?),
        }
    }

    /// Copy verified attachment content into `directory`, using the validated
    /// accepted display filename. The filename is never treated as a path: a
    /// server-supplied `../evil` is rejected rather than joined, so an export
    /// can never escape the caller's directory.
    pub fn export_attachment_into(
        &self,
        id: &str,
        directory: String,
    ) -> Result<AttachmentDto, ClientConnectionError> {
        match self {
            Self::Embedded(client) => {
                let (metadata, bytes) =
                    crate::attachments::host_attachment_content(client.host(), id)?;
                let path = crate::attachments_remote::export_destination(
                    std::path::Path::new(&directory),
                    &metadata.filename,
                )?;
                crate::attachments_remote::write_new_export(&path, &bytes)?;
                Ok(metadata)
            }
            Self::Remote { replica, http } => {
                Ok(crate::attachments_remote::export_attachment_to_directory(
                    replica.replica(),
                    http.as_ref(),
                    id,
                    &directory,
                )?)
            }
        }
    }

    /// Remove one attachment association.
    ///
    /// A staged upload is discarded locally. An accepted attachment is removed
    /// online only: this build does not queue offline removals, so an
    /// unreachable server returns an actionable `Unavailable` error and removes
    /// nothing.
    pub fn remove_attachment(&mut self, id: String) -> Result<bool, ClientConnectionError> {
        match self {
            Self::Embedded(client) => {
                let (actor, host) = client.command_context();
                crate::attachments::host_remove_attachment(host, &actor, id)
                    .map_err(ClientConnectionError::from)
            }
            Self::Remote { replica, http } => Ok(crate::attachments_remote::remove_attachment(
                replica.replica_mut(),
                http.as_ref(),
                &id,
            )?),
        }
    }

    /// Remove unreferenced attachment content objects.
    ///
    /// The embedded host owns its content store, so it can purge; the connected
    /// build cannot, because the server never exposes garbage collection.
    pub fn purge_attachment_content(&mut self) -> Result<usize, ClientConnectionError> {
        match self {
            Self::Embedded(client) => {
                crate::attachments::host_purge_attachment_content(client.host_mut())
                    .map_err(ClientConnectionError::from)
            }
            Self::Remote { .. } => Err(unavailable(PURGE_ATTACHMENTS_UNAVAILABLE_MESSAGE)),
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
            Self::Remote { http, .. } => {
                let transport = http.as_ref().ok_or_else(|| {
                    ClientConnectionError::Unsupported(
                        "offline: reading a project workflow requires a server connection",
                    )
                })?;
                let value = transport.get_json(&format!(
                    "/v1/authoritative/blueprints/binding?project_id={}",
                    encode_query(project_id)
                ))?;
                let binding: Option<WorkflowBindingWire> = serde_json::from_value(value)?;
                Ok(binding.map(Into::into))
            }
        }
    }

    fn acceptance_change(
        &mut self,
        task_id: String,
        change: AcceptanceCriteriaChange,
    ) -> Result<Vec<AcceptanceCriterionDto>, ClientConnectionError> {
        match self {
            Self::Embedded(client) => {
                let (actor, host) = client.command_context();
                crate::task_details::host_apply_acceptance_criteria(host, &actor, task_id, change)
                    .map_err(ClientConnectionError::from)
            }
            Self::Remote { replica, .. } => {
                // Validate the domain-level text before touching the outbox: a
                // criterion the server is guaranteed to reject must not leave a
                // queued command (and a newly generated criterion id) behind.
                if let AcceptanceCriteriaChange::Add { criterion_id, text }
                | AcceptanceCriteriaChange::SetText {
                    criterion_id, text, ..
                } = &change
                {
                    validate_criterion_text(criterion_id, text)?;
                }
                if let AcceptanceCriteriaChange::Reorder { position, .. } = &change
                    && *position > orbit_domain::MAX_ACCEPTANCE_CRITERIA
                {
                    return Err(ClientConnectionError::from(ClientError {
                        code: crate::ClientErrorCode::InvalidInput,
                        message: format!(
                            "invalid acceptance criterion position {position}: a task has at most {} criteria",
                            orbit_domain::MAX_ACCEPTANCE_CRITERIA
                        ),
                    }));
                }
                let revision = replica.workspace()?.revision.ok_or_else(|| {
                    ClientConnectionError::Unsupported(TASK_DETAILS_ACCEPTED_STATE_MESSAGE)
                })?;
                // Read the accepted list first. A failed read must not queue a
                // mutation the caller may then retry under a new identity.
                let accepted = self.acceptance_criteria(&task_id)?;
                self.task_command(TaskCommand {
                    command_id: crate::facade_conversions::generate_id("acceptance-criteria"),
                    expected_revision: revision,
                    action: TaskAction::AcceptanceCriteria {
                        task_id,
                        change: acceptance_action(change),
                    },
                })?;
                // The pending operation is outbox state only; return the list
                // the server accepted before this mutation was queued.
                Ok(accepted)
            }
        }
    }

    /// Currently accepted task projection for one id.
    ///
    /// Prefers the last accepted server snapshot. If that snapshot does not
    /// contain the task (for example a different device created it after the
    /// last refresh), fall back to the authoritative task-detail route.
    fn accepted_task(&self, id: &str) -> Result<TaskDto, ClientConnectionError> {
        let Self::Remote { replica, .. } = self else {
            return Err(ClientConnectionError::Unsupported(
                "accepted task projection is only available in connected mode",
            ));
        };
        let accepted = replica
            .workspace()?
            .tasks
            .into_iter()
            .find(|task| task.get("id").and_then(Value::as_str) == Some(id));
        match accepted {
            Some(value) => task_dto_from_snapshot(value),
            None => self.remote_task_detail(id),
        }
    }

    fn remote_task_detail(&self, id: &str) -> Result<TaskDto, ClientConnectionError> {
        let Self::Remote { replica, http } = self else {
            return Err(ClientConnectionError::Unsupported(
                "task detail is only available in connected mode",
            ));
        };
        // The accepted snapshot is the offline-first source of truth and now
        // carries description/priority/labels, so connected detail editing can
        // load accepted values without a network round trip. Only a task the
        // snapshot genuinely lacks is fetched from its authoritative route.
        if let Some(value) = replica
            .workspace()?
            .tasks
            .into_iter()
            .find(|task| task.get("id").and_then(Value::as_str) == Some(id))
        {
            return task_dto_from_snapshot(value);
        }
        let transport = http
            .as_ref()
            .ok_or_else(|| ClientConnectionError::Unsupported(TASK_READS_OFFLINE_MESSAGE))?;
        let value = transport.get_json(&format!("/v1/authoritative/tasks/{}", encode_query(id)))?;
        task_dto_from_snapshot(value)
    }
}

/// Connected-mode pending signal: queued intentions never appear in accepted
/// reads, so a caller must consult this count (or the outbox) to distinguish
/// "accepted and unchanged" from "mutation still pending". Embedded authority
/// is synchronous and therefore always zero.
impl AuthorityConnection {
    pub fn pending_command_count(&self) -> Result<i64, ClientConnectionError> {
        match self {
            Self::Embedded(_) => Ok(0),
            // Staged attachment uploads are durable pending intentions too, so
            // a caller that only reads the accepted projection cannot conclude
            // the workspace is settled while an upload is waiting.
            Self::Remote { replica, .. } => Ok(replica.pending_command_count()?),
        }
    }

    /// The identity of the last command this connection durably queued.
    ///
    /// Together with [`Self::command_state`] this replaces aggregate-depth
    /// heuristics: a frontend can follow one specific mutation from `pending`
    /// to `accepted` (or `conflict`) instead of guessing.
    pub fn last_queued_command_id(&self) -> Option<String> {
        match self {
            Self::Embedded(_) => None,
            Self::Remote { replica, .. } => replica.last_queued_command_id(),
        }
    }

    /// Outbox state for one command identity: `pending`, `accepted`, `conflict`
    /// or `superseded`. `None` means the replica has never seen it. Embedded
    /// commands are committed synchronously and maintain no outbox.
    pub fn command_state(&self, id: &str) -> Result<Option<String>, ClientConnectionError> {
        match self {
            Self::Embedded(_) => Ok(None),
            Self::Remote { replica, .. } => Ok(replica.command_state(id)?),
        }
    }
}

/// Validate and canonicalise caller-supplied rich details with the same domain
/// rules the accepted authority applies, so a connected client rejects invalid
/// input before queueing a command the server is guaranteed to refuse. Labels
/// are trimmed and de-duplicated by the domain aggregate.
fn canonical_details(
    description: &str,
    priority: Option<String>,
    labels: &[String],
) -> Result<orbit_domain::TaskDetails, ClientConnectionError> {
    let priority = match priority {
        None => None,
        Some(value) if value.trim().is_empty() => None,
        Some(value) => Some(orbit_application::parse_task_priority(&value).map_err(|error| {
            ClientError {
                code: crate::ClientErrorCode::InvalidInput,
                message: format!(
                    "invalid task priority {value:?}: {error:?}; allowed values are low, medium, high, urgent"
                ),
            }
        })?),
    };
    orbit_domain::TaskDetails::new(description.to_owned(), priority, labels.to_vec()).map_err(
        |error| {
            ClientError {
                code: crate::ClientErrorCode::InvalidInput,
                message: format!(
                    "invalid task details: {error:?}; labels are trimmed, must be 1..=64 characters without control characters (at most 32), and the description is limited to 100000 characters"
                ),
            }
            .into()
        },
    )
}

/// Validate one criterion's text with the domain before it can be queued.
fn validate_criterion_text(
    criterion_id: &orbit_domain::AcceptanceCriterionId,
    text: &str,
) -> Result<(), ClientConnectionError> {
    orbit_domain::AcceptanceCriterion::new(criterion_id.clone(), text).map_err(|error| {
        ClientConnectionError::from(ClientError {
            code: crate::ClientErrorCode::InvalidInput,
            message: format!(
                "invalid acceptance criterion text {text:?}: {error:?}; text must be non-blank, at most 2000 characters, and contain no NUL"
            ),
        })
    })?;
    Ok(())
}

/// Convert the application-level criteria change into its stable wire shape.
fn acceptance_action(change: AcceptanceCriteriaChange) -> AcceptanceCriteriaAction {
    match change {
        AcceptanceCriteriaChange::Add { criterion_id, text } => AcceptanceCriteriaAction::Add {
            criterion_id: criterion_id.as_str().to_owned(),
            text,
        },
        AcceptanceCriteriaChange::SetText { criterion_id, text } => {
            AcceptanceCriteriaAction::SetText {
                criterion_id: criterion_id.as_str().to_owned(),
                text,
            }
        }
        AcceptanceCriteriaChange::SetCompleted {
            criterion_id,
            completed,
        } => AcceptanceCriteriaAction::SetCompleted {
            criterion_id: criterion_id.as_str().to_owned(),
            completed,
        },
        AcceptanceCriteriaChange::Reorder {
            criterion_id,
            position,
        } => AcceptanceCriteriaAction::Reorder {
            criterion_id: criterion_id.as_str().to_owned(),
            position,
        },
        AcceptanceCriteriaChange::Remove { criterion_id } => AcceptanceCriteriaAction::Remove {
            criterion_id: criterion_id.as_str().to_owned(),
        },
    }
}

fn task_dto_from_snapshot(value: Value) -> Result<TaskDto, ClientConnectionError> {
    // The authoritative snapshot stores the same wire shape as the task-detail
    // route, so one lenient reader covers both.
    let task: TaskRecord = serde_json::from_value(value)?;
    task_dto(task)
}

/// Map the authority task record onto the renderer-neutral DTO. The authority
/// emits an `i64` minute estimate; a value outside the DTO's `u32` range is a
/// contract mismatch, never silently truncated.
fn task_dto(task: TaskRecord) -> Result<TaskDto, ClientConnectionError> {
    let estimated_minutes = task
        .estimated_minutes
        .map(u32::try_from)
        .transpose()
        .map_err(|_| {
            ClientConnectionError::Unsupported("server task estimate is out of the DTO range")
        })?;
    Ok(TaskDto {
        id: task.id,
        title: task.title,
        status: task.status,
        project_id: task.project_id,
        milestone_id: task.milestone_id,
        estimated_minutes,
        earliest_start_at_ms: task.earliest_start_at_ms,
        deadline_at_ms: task.deadline_at_ms,
        description: task.description,
        priority: task.priority,
        labels: task.labels,
    })
}

#[derive(Deserialize)]
struct AcceptanceCriterionWire {
    id: String,
    text: String,
    completed: bool,
    #[serde(default)]
    completed_at_ms: Option<i64>,
    #[serde(default)]
    position: u32,
}

impl From<AcceptanceCriterionWire> for AcceptanceCriterionDto {
    fn from(criterion: AcceptanceCriterionWire) -> Self {
        Self {
            id: criterion.id,
            text: criterion.text,
            completed: criterion.completed,
            completed_at_ms: criterion.completed_at_ms,
            position: criterion.position,
        }
    }
}

#[derive(Deserialize)]
struct ActivityWire {
    id: String,
    kind: String,
    origin: String,
    occurred_at_ms: i64,
    summary: String,
}

impl From<ActivityWire> for ActivityDto {
    fn from(activity: ActivityWire) -> Self {
        Self {
            id: activity.id,
            kind: activity.kind,
            origin: activity.origin,
            occurred_at_ms: activity.occurred_at_ms,
            summary: activity.summary,
        }
    }
}

#[derive(Deserialize)]
struct WorkflowBindingWire {
    project_id: String,
    blueprint_id: String,
    version: u32,
}

impl From<WorkflowBindingWire> for ProjectWorkflowDto {
    fn from(binding: WorkflowBindingWire) -> Self {
        Self {
            project_id: binding.project_id,
            workflow_id: binding.blueprint_id,
            version: binding.version,
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
