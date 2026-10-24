//! Local/connected backend for the one project-centric TUI shell.
//!
//! The renderer uses one view model for both modes; the shell only branches for
//! capability/availability labels. Connected rich mutations return the last
//! accepted DTO while the exact queued command identity is followed through the
//! outbox. The shell reports `accepted`, `pending` or `conflict` for that
//! command; a missing identity or state fails closed as pending, never success.

use orbit_client::OrbitClient;
use orbit_client::unified::{AuthorityConnection, CommandDisposition};
use orbit_client::{
    AcceptanceCriterionDto, ActivityDto, AttachmentDto, ClientError, ClientErrorCode,
    ConfigureProjectStrategyRequest, CreateKnowledgeRequest, CreateProjectRequest,
    CreateTaskRequest, HabitDto, MetricTargetDto, PendingAttachmentDto, ProjectDto,
    ProjectEvidenceBindingDto, ProjectWorkflowDto, RecordHabitOccurrenceRequest,
    SetProjectStatusRequest, SetTaskPlanningRequest, SetTaskStatusRequest, TaskDto,
    TaskPlanProposalDto, WorkflowDescriptorDto,
};
use orbit_runtime::domain_commands::{ProjectCommand, TaskAction, TaskCommand};
use std::path::Path;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static ID_SEQUENCE: AtomicU64 = AtomicU64::new(1);

fn next_id(prefix: &str) -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    format!(
        "{prefix}-{}-{nanos}-{}",
        std::process::id(),
        ID_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    )
}

fn unavailable(message: impl Into<String>) -> ClientError {
    ClientError {
        code: ClientErrorCode::Unavailable,
        message: message.into(),
    }
}

fn mapped(error: impl std::fmt::Debug) -> ClientError {
    unavailable(format!("{error:?}"))
}

/// Map a connected-connection error into the shared typed client error so the
/// shell has one failure contract. Local facade errors keep their code.
fn connected_error(error: orbit_client::unified::ClientConnectionError) -> ClientError {
    use orbit_client::unified::ClientConnectionError;
    match error {
        ClientConnectionError::Local(error) => error,
        ClientConnectionError::Unavailable(message) => unavailable(message),
        other => mapped(other),
    }
}

/// Per-command outcome for the mutation the shell just submitted.
///
/// Aggregate outbox depth cannot tell "my command is pending" from "someone
/// else queued earlier", so connected rich mutations follow the exact command
/// identity recorded by the client. A missing identity or state fails closed as
/// [`MutationOutcome::Pending`] with an explicit `unavailable` state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MutationOutcome {
    Accepted,
    Pending {
        command_id: Option<String>,
        state: String,
    },
    Conflict {
        command_id: String,
        state: String,
        reason: String,
    },
}

impl MutationOutcome {
    pub fn is_accepted(&self) -> bool {
        matches!(self, Self::Accepted)
    }

    /// Anything that is not a committed acceptance keeps the pending signal on.
    pub fn needs_attention(&self) -> bool {
        !self.is_accepted()
    }
}

fn outcome_from_disposition(
    connection: &AuthorityConnection,
    disposition: &CommandDisposition,
) -> MutationOutcome {
    match disposition {
        CommandDisposition::Pending => MutationOutcome::Pending {
            command_id: connection.last_queued_command_id(),
            state: "pending".into(),
        },
        _ => MutationOutcome::Accepted,
    }
}

/// Translate one durable command state into an outcome. The caller must only
/// pass a state read for the command this mutation queued.
fn outcome_from_state(
    command_id: Option<String>,
    state: Option<&str>,
    conflict_reason: Option<String>,
) -> MutationOutcome {
    match state {
        Some("accepted") => MutationOutcome::Accepted,
        Some("conflict") => MutationOutcome::Conflict {
            command_id: command_id.unwrap_or_default(),
            state: "conflict".into(),
            reason: conflict_reason
                .filter(|reason| !reason.trim().is_empty())
                .unwrap_or_else(|| {
                    "command conflict requires explicit reconciliation; inspect the outbox".into()
                }),
        },
        // `pending`, `superseded` or an unexpected state is never success.
        Some(state) => MutationOutcome::Pending {
            command_id,
            state: state.to_owned(),
        },
        None => MutationOutcome::Pending {
            command_id,
            state: "unavailable".into(),
        },
    }
}

/// Read the conflict explanation recorded on the outbox row, if any.
fn conflict_reason(connection: &AuthorityConnection, command_id: &str) -> Option<String> {
    connection
        .command_status(command_id)
        .ok()
        .flatten()
        .and_then(|status| {
            status
                .pointer("/receipt/error")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned)
        })
}

/// Read the durable state of one command identity.
fn command_outcome(connection: &AuthorityConnection, command_id: &str) -> MutationOutcome {
    match connection.command_state(command_id) {
        Ok(state) => {
            let reason = (state.as_deref() == Some("conflict"))
                .then(|| conflict_reason(connection, command_id))
                .flatten();
            outcome_from_state(Some(command_id.to_owned()), state.as_deref(), reason)
        }
        Err(_) => MutationOutcome::Pending {
            command_id: Some(command_id.to_owned()),
            state: "unavailable".into(),
        },
    }
}

/// Follow the command queued by one rich mutation.
///
/// `before` is the last identity seen before the call. A new identity is the
/// only honest signal that this call queued something; if none appeared, the
/// outcome fails closed rather than treating "no visible growth" as success.
fn read_rich_outcome(connection: &AuthorityConnection, before: Option<String>) -> MutationOutcome {
    if matches!(connection, AuthorityConnection::Embedded(_)) {
        // Embedded authority commits synchronously and keeps no outbox.
        return MutationOutcome::Accepted;
    }
    let after = connection.last_queued_command_id();
    let Some(command_id) = after else {
        return MutationOutcome::Pending {
            command_id: None,
            state: "unavailable".into(),
        };
    };
    if Some(&command_id) == before.as_ref() {
        return MutationOutcome::Pending {
            command_id: Some(command_id),
            state: "unavailable".into(),
        };
    }
    command_outcome(connection, &command_id)
}

/// Process-scoped directory for connected attachment exports. The first call
/// creates it; later calls reuse the same path inside this process.
fn connected_export_dir() -> Result<std::path::PathBuf, ClientError> {
    static DIRECTORY: OnceLock<std::path::PathBuf> = OnceLock::new();
    if let Some(path) = DIRECTORY.get() {
        return Ok(path.clone());
    }
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    let path = std::env::temp_dir().join(format!(
        "orbit-tui-attachments-{}-{nanos}",
        std::process::id()
    ));
    let mut builder = std::fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(&path).map_err(|error| {
        unavailable(format!(
            "cannot create connected attachment export directory {}: {error}",
            path.display()
        ))
    })?;
    Ok(DIRECTORY.get_or_init(|| path).clone())
}

pub enum Backend {
    Local(OrbitClient),
    Connected {
        connection: AuthorityConnection,
        /// Outcome of the most recent connected mutation, consumed by the shell
        /// through [`Backend::take_outcome`].
        pending: Option<MutationOutcome>,
    },
}

impl Backend {
    pub fn open_local(path: impl AsRef<Path>) -> Result<Self, ClientError> {
        Ok(Self::Local(OrbitClient::open(path)?))
    }

    pub fn open_connected(replica: &Path, server: &str) -> Result<Self, ClientError> {
        let token = std::env::var("ORBIT_AUTHORITY_TOKEN").ok();
        let device = std::env::var("ORBIT_AUTHORITY_DEVICE").ok();
        let identity = match (token.as_deref(), device.as_deref()) {
            (Some(token), Some(device)) if !token.is_empty() && !device.is_empty() => {
                Some((device, token))
            }
            _ => None,
        };
        Self::open_connected_with_identity(replica, server, identity)
    }

    pub fn open_connected_with_identity(
        replica: &Path,
        server: &str,
        identity: Option<(&str, &str)>,
    ) -> Result<Self, ClientError> {
        let connection = AuthorityConnection::remote(replica, server, identity)
            .map_err(|error| unavailable(format!("connected client unavailable: {error:?}")))?;
        Ok(Self::Connected {
            connection,
            pending: None,
        })
    }

    pub fn is_connected(&self) -> bool {
        matches!(self, Self::Connected { .. })
    }

    /// Returns and clears the outcome of the last connected mutation.
    pub fn take_outcome(&mut self) -> Option<MutationOutcome> {
        match self {
            Self::Local(_) => None,
            Self::Connected { pending, .. } => pending.take(),
        }
    }

    /// True when the last mutation is queued, conflicted, or its state could
    /// not be established. Never inferred from aggregate outbox depth.
    pub fn take_pending(&mut self) -> bool {
        self.take_outcome()
            .is_some_and(|outcome| outcome.needs_attention())
    }

    pub fn refresh_accepted(&mut self) -> Result<(), ClientError> {
        match self {
            Self::Local(_) => Ok(()),
            Self::Connected { connection, .. } => connection.refresh().map(|_| ()).map_err(mapped),
        }
    }

    pub fn flush_pending(&mut self) -> Result<(String, bool), ClientError> {
        match self {
            Self::Local(_) => Ok(("Local authority has no outbox".into(), false)),
            Self::Connected { connection, .. } => {
                use orbit_client::authority::FlushOutcome;
                let (message, blocked) = match connection.flush().map_err(mapped)? {
                    FlushOutcome::Empty => ("No pending commands".to_owned(), false),
                    FlushOutcome::Accepted(count) => {
                        (format!("Server accepted {count} pending command(s)"), false)
                    }
                    FlushOutcome::Blocked {
                        accepted,
                        command_id,
                        reason,
                    } => (
                        format!(
                            "Flush blocked after accepting {accepted} command(s): {command_id} was rejected. {reason}. Inspect the outbox and resolve it with the CLI or legacy connected command surface."
                        ),
                        true,
                    ),
                };
                if let Err(error) = connection.refresh() {
                    return Ok((
                        format!("{message}; accepted-state refresh failed: {error:?}"),
                        true,
                    ));
                }
                Ok((message, blocked))
            }
        }
    }

    /// Pending work the user can see, including staged attachment uploads.
    /// The client owns this sum; the TUI must not re-derive it from outbox
    /// lengths, which omit attachment staging.
    pub fn pending_count(&self) -> usize {
        match self {
            Self::Local(_) => 0,
            Self::Connected { connection, .. } => connection
                .pending_command_count()
                .map(|count| count.max(0) as usize)
                .unwrap_or_else(|_| {
                    connection
                        .unresolved_commands()
                        .map(|rows| rows.len())
                        .unwrap_or(0)
                }),
        }
    }

    fn record_disposition(&mut self, disposition: &CommandDisposition) {
        if let Self::Connected {
            connection,
            pending,
        } = self
        {
            *pending = Some(outcome_from_disposition(connection, disposition));
        }
    }

    /// Follow the exact command a rich mutation queued. `before` is the last
    /// identity observed before the call, so a concurrent queue cannot be
    /// mistaken for this mutation.
    fn record_rich_outcome(&mut self, before: Option<String>) {
        let Self::Connected {
            connection,
            pending,
        } = self
        else {
            return;
        };
        *pending = Some(read_rich_outcome(connection, before));
    }

    pub fn workspace_view_data(&self) -> Result<orbit_ui::WorkspaceData, orbit_ui::ViewDataError> {
        let mut data = match self {
            Self::Local(client) => client.workspace_view_data()?,
            Self::Connected { connection, .. } => connection.workspace_view_data()?,
        };
        data.pending_commands = self.pending_count();
        Ok(data)
    }

    #[allow(dead_code)]
    pub fn project_view_data(
        &self,
        project_id: &str,
        observed_at_ms: i64,
    ) -> Result<orbit_ui::ProjectData, orbit_ui::ViewDataError> {
        let mut data = match self {
            Self::Local(client) => client.project_view_data(project_id, observed_at_ms)?,
            Self::Connected { connection, .. } => connection.project_view_data(project_id)?,
        };
        data.pending_commands = self.pending_count();
        Ok(data)
    }

    pub fn resolve_project_views(
        &self,
        project_id: &str,
        registry: &orbit_ui::ComponentRegistry,
        observed_at_ms: i64,
    ) -> Result<(orbit_ui::ProjectData, Vec<orbit_ui::ResolvedView>, bool), orbit_ui::ViewDataError>
    {
        match self {
            Self::Local(client) => {
                client.resolve_project_views(project_id, registry, observed_at_ms)
            }
            Self::Connected { connection, .. } => {
                connection.resolve_connected_project_views(project_id, registry)
            }
        }
    }

    pub fn list_projects(&self) -> Result<Vec<ProjectDto>, ClientError> {
        match self {
            Self::Local(client) => client.list_projects(),
            Self::Connected { .. } => Ok(self
                .workspace_view_data()
                .map_err(|error| unavailable(error.to_string()))?
                .projects
                .into_iter()
                .map(|project| ProjectDto {
                    id: project.id,
                    name: project.name,
                    status: project.status,
                    mode: project.mode,
                    objective: project.objective,
                    progress_signals: Vec::new(),
                    success_criteria: Vec::new(),
                    completion_ready: project.completion_ready,
                })
                .collect()),
        }
    }

    pub fn list_tasks(&self) -> Result<Vec<TaskDto>, ClientError> {
        match self {
            Self::Local(client) => client.list_tasks(),
            Self::Connected { .. } => Ok(self
                .workspace_view_data()
                .map_err(|error| unavailable(error.to_string()))?
                .tasks
                .into_iter()
                .map(|task| TaskDto {
                    description: String::new(),
                    priority: None,
                    labels: Vec::new(),
                    id: task.id,
                    title: task.title,
                    status: task.status,
                    project_id: task.project_id,
                    milestone_id: task.milestone_id,
                    estimated_minutes: task.estimated_minutes,
                    earliest_start_at_ms: task.earliest_start_at_ms,
                    deadline_at_ms: task.deadline_at_ms,
                })
                .collect()),
        }
    }

    pub fn agent_job_detail(
        &self,
        job_id: &str,
    ) -> Result<Option<orbit_client::view_data::AgentJobDetail>, ClientError> {
        match self {
            Self::Local(client) => client.agent_job_detail(job_id),
            Self::Connected { .. } => Ok(None),
        }
    }

    pub fn list_knowledge(&self) -> Result<Vec<orbit_client::KnowledgeItemDto>, ClientError> {
        match self {
            Self::Local(client) => client.list_knowledge(),
            Self::Connected { .. } => Err(unavailable(
                "connected mode note browsing uses the legacy command surface",
            )),
        }
    }

    pub fn task_allowed_statuses(&self, status: &str) -> Vec<String> {
        orbit_client::view_data::allowed_task_statuses(status)
    }

    pub fn create_project(
        &mut self,
        request: CreateProjectRequest,
    ) -> Result<ProjectDto, ClientError> {
        match self {
            Self::Local(client) => client.create_project(request),
            Self::Connected { connection, .. } => {
                let id = request.id.unwrap_or_else(|| next_id("project"));
                let revision = connection
                    .workspace_view_data()
                    .ok()
                    .and_then(|data| data.revision)
                    .unwrap_or(0);
                let disposition = connection
                    .create_project(ProjectCommand {
                        workflow_id: None,
                        command_id: next_id("command"),
                        expected_revision: revision,
                        project_id: id.clone(),
                        name: request.name.clone(),
                    })
                    .map_err(mapped)?;
                self.record_disposition(&disposition);
                Ok(ProjectDto {
                    id,
                    name: request.name,
                    status: "proposed".into(),
                    mode: "standard".into(),
                    objective: None,
                    progress_signals: Vec::new(),
                    success_criteria: Vec::new(),
                    completion_ready: false,
                })
            }
        }
    }

    pub fn create_task(&mut self, request: CreateTaskRequest) -> Result<TaskDto, ClientError> {
        match self {
            Self::Local(client) => client.create_task(request),
            Self::Connected {
                connection,
                pending,
            } => {
                let id = request.id.unwrap_or_else(|| next_id("task"));
                let milestone_id = request.milestone_id.clone();
                let revision = connection
                    .workspace_view_data()
                    .ok()
                    .and_then(|data| data.revision)
                    .unwrap_or(0);
                let disposition = connection
                    .task_command(TaskCommand {
                        command_id: next_id("command"),
                        expected_revision: revision,
                        action: TaskAction::Create {
                            task_id: id.clone(),
                            title: request.title.clone(),
                            project_id: request.project_id.clone(),
                        },
                    })
                    .map_err(mapped)?;
                *pending = Some(outcome_from_disposition(connection, &disposition));
                if let Some(milestone_id) = milestone_id.clone() {
                    let disposition = connection
                        .task_command(TaskCommand {
                            command_id: next_id("command"),
                            expected_revision: revision + 1,
                            action: TaskAction::AssignMilestone {
                                task_id: id.clone(),
                                milestone_id: Some(milestone_id),
                            },
                        })
                        .map_err(mapped)?;
                    *pending = Some(outcome_from_disposition(connection, &disposition));
                }
                Ok(TaskDto {
                    description: String::new(),
                    priority: None,
                    labels: Vec::new(),
                    id,
                    title: request.title,
                    status: "inbox".into(),
                    project_id: request.project_id,
                    milestone_id,
                    estimated_minutes: None,
                    earliest_start_at_ms: None,
                    deadline_at_ms: None,
                })
            }
        }
    }

    fn connected_task_command(&mut self, action: TaskAction) -> Result<(), ClientError> {
        match self {
            Self::Local(_) => Err(unavailable("local task command path")),
            Self::Connected { connection, .. } => {
                let revision = connection
                    .workspace_view_data()
                    .ok()
                    .and_then(|data| data.revision)
                    .unwrap_or(0);
                let disposition = connection
                    .task_command(TaskCommand {
                        command_id: next_id("command"),
                        expected_revision: revision,
                        action,
                    })
                    .map_err(mapped)?;
                self.record_disposition(&disposition);
                Ok(())
            }
        }
    }

    pub fn set_task_status(
        &mut self,
        request: SetTaskStatusRequest,
    ) -> Result<TaskDto, ClientError> {
        match self {
            Self::Local(client) => client.set_task_status(request),
            Self::Connected { .. } => {
                self.connected_task_command(TaskAction::ChangeStatus {
                    task_id: request.id.clone(),
                    status: request.status.clone(),
                })?;
                let title = self
                    .workspace_view_data()
                    .ok()
                    .and_then(|workspace| {
                        workspace
                            .tasks
                            .into_iter()
                            .find(|task| task.id == request.id)
                            .map(|task| task.title)
                    })
                    .unwrap_or_else(|| "Task".into());
                Ok(TaskDto {
                    description: String::new(),
                    priority: None,
                    labels: Vec::new(),
                    id: request.id,
                    title,
                    status: request.status,
                    project_id: None,
                    milestone_id: None,
                    estimated_minutes: None,
                    earliest_start_at_ms: None,
                    deadline_at_ms: None,
                })
            }
        }
    }

    pub fn rename_task(&mut self, id: String, title: String) -> Result<TaskDto, ClientError> {
        match self {
            Self::Local(client) => client.rename_task(id, title),
            Self::Connected { .. } => {
                self.connected_task_command(TaskAction::Rename {
                    task_id: id.clone(),
                    title,
                })?;
                Ok(TaskDto {
                    description: String::new(),
                    priority: None,
                    labels: Vec::new(),
                    id,
                    title: String::new(),
                    status: String::new(),
                    project_id: None,
                    milestone_id: None,
                    estimated_minutes: None,
                    earliest_start_at_ms: None,
                    deadline_at_ms: None,
                })
            }
        }
    }

    pub fn set_task_planning(
        &mut self,
        request: SetTaskPlanningRequest,
    ) -> Result<TaskDto, ClientError> {
        match self {
            Self::Local(client) => client.set_task_planning(request),
            Self::Connected { .. } => {
                self.connected_task_command(TaskAction::SetPlanning {
                    task_id: request.id.clone(),
                    estimated_minutes: request.estimated_minutes.map(i64::from),
                    earliest_start_at_ms: request.earliest_start_at_ms,
                    deadline_at_ms: request.deadline_at_ms,
                })?;
                Ok(TaskDto {
                    description: String::new(),
                    priority: None,
                    labels: Vec::new(),
                    id: request.id,
                    title: String::new(),
                    status: String::new(),
                    project_id: None,
                    milestone_id: None,
                    estimated_minutes: request.estimated_minutes,
                    earliest_start_at_ms: request.earliest_start_at_ms,
                    deadline_at_ms: request.deadline_at_ms,
                })
            }
        }
    }

    pub fn add_task_prerequisite(
        &mut self,
        task_id: String,
        prerequisite_id: String,
    ) -> Result<TaskDto, ClientError> {
        match self {
            Self::Local(client) => client.add_task_prerequisite(task_id, prerequisite_id),
            Self::Connected { .. } => {
                self.connected_task_command(TaskAction::AddPrerequisite {
                    task_id: task_id.clone(),
                    prerequisite_id,
                })?;
                Ok(TaskDto {
                    description: String::new(),
                    priority: None,
                    labels: Vec::new(),
                    id: task_id,
                    title: String::new(),
                    status: String::new(),
                    project_id: None,
                    milestone_id: None,
                    estimated_minutes: None,
                    earliest_start_at_ms: None,
                    deadline_at_ms: None,
                })
            }
        }
    }

    pub fn remove_task_prerequisite(
        &mut self,
        task_id: String,
        prerequisite_id: String,
    ) -> Result<TaskDto, ClientError> {
        match self {
            Self::Local(client) => client.remove_task_prerequisite(task_id, prerequisite_id),
            Self::Connected { .. } => {
                self.connected_task_command(TaskAction::RemovePrerequisite {
                    task_id: task_id.clone(),
                    prerequisite_id,
                })?;
                Ok(TaskDto {
                    description: String::new(),
                    priority: None,
                    labels: Vec::new(),
                    id: task_id,
                    title: String::new(),
                    status: String::new(),
                    project_id: None,
                    milestone_id: None,
                    estimated_minutes: None,
                    earliest_start_at_ms: None,
                    deadline_at_ms: None,
                })
            }
        }
    }

    pub fn assign_task_project(
        &mut self,
        task_id: String,
        project_id: Option<String>,
    ) -> Result<TaskDto, ClientError> {
        match self {
            Self::Local(client) => client.assign_task_project(task_id, project_id),
            Self::Connected { .. } => {
                self.connected_task_command(TaskAction::AssignProject {
                    task_id: task_id.clone(),
                    project_id,
                })?;
                Ok(TaskDto {
                    description: String::new(),
                    priority: None,
                    labels: Vec::new(),
                    id: task_id,
                    title: String::new(),
                    status: String::new(),
                    project_id: None,
                    milestone_id: None,
                    estimated_minutes: None,
                    earliest_start_at_ms: None,
                    deadline_at_ms: None,
                })
            }
        }
    }

    pub fn assign_task_milestone(
        &mut self,
        task_id: String,
        milestone_id: Option<String>,
    ) -> Result<TaskDto, ClientError> {
        match self {
            Self::Local(client) => client.assign_task_milestone(task_id, milestone_id),
            Self::Connected { .. } => {
                self.connected_task_command(TaskAction::AssignMilestone {
                    task_id: task_id.clone(),
                    milestone_id,
                })?;
                Ok(TaskDto {
                    description: String::new(),
                    priority: None,
                    labels: Vec::new(),
                    id: task_id,
                    title: String::new(),
                    status: String::new(),
                    project_id: None,
                    milestone_id: None,
                    estimated_minutes: None,
                    earliest_start_at_ms: None,
                    deadline_at_ms: None,
                })
            }
        }
    }

    pub fn configure_project_strategy(
        &mut self,
        request: ConfigureProjectStrategyRequest,
    ) -> Result<ProjectDto, ClientError> {
        match self {
            Self::Local(client) => client.configure_project_strategy(request),
            Self::Connected { .. } => Err(unavailable(
                "project strategy editing requires the connected command surface",
            )),
        }
    }

    pub fn set_project_status(
        &mut self,
        request: SetProjectStatusRequest,
    ) -> Result<ProjectDto, ClientError> {
        match self {
            Self::Local(client) => client.set_project_status(request),
            Self::Connected { .. } => Err(unavailable(
                "project status editing requires the connected command surface",
            )),
        }
    }

    pub fn list_project_evidence(
        &self,
        project_id: String,
    ) -> Result<Vec<ProjectEvidenceBindingDto>, ClientError> {
        match self {
            Self::Local(client) => client.list_project_evidence(project_id),
            Self::Connected { .. } => Err(unavailable(
                "connected mode does not project evidence bindings yet",
            )),
        }
    }

    pub fn change_project_evidence(
        &mut self,
        project_id: String,
        kind: String,
        evidence_id: String,
        bind: bool,
    ) -> Result<(), ClientError> {
        match self {
            Self::Local(client) => {
                client.change_project_evidence(project_id, kind, evidence_id, bind)
            }
            Self::Connected { .. } => Err(unavailable(
                "evidence editing requires the connected command surface",
            )),
        }
    }

    pub fn set_metric_target(&mut self, target: MetricTargetDto) -> Result<(), ClientError> {
        match self {
            Self::Local(client) => client.set_metric_target(target),
            Self::Connected { .. } => Err(unavailable(
                "metric targets require the connected command surface",
            )),
        }
    }

    pub fn record_metric_observation(
        &mut self,
        metric_id: String,
        observed_at_ms: i64,
        value: i64,
    ) -> Result<(), ClientError> {
        match self {
            Self::Local(client) => {
                client.record_metric_observation(metric_id, observed_at_ms, value)
            }
            Self::Connected { .. } => Err(unavailable(
                "metric observations require the connected command surface",
            )),
        }
    }

    pub fn create_knowledge(
        &mut self,
        request: CreateKnowledgeRequest,
    ) -> Result<orbit_client::KnowledgeItemDto, ClientError> {
        match self {
            Self::Local(client) => client.create_knowledge(request),
            Self::Connected { .. } => Err(unavailable(
                "note creation requires the connected command surface",
            )),
        }
    }

    pub fn record_habit_occurrence(
        &mut self,
        request: RecordHabitOccurrenceRequest,
    ) -> Result<orbit_client::HabitOccurrenceDto, ClientError> {
        match self {
            Self::Local(client) => client.record_habit_occurrence(request),
            Self::Connected { .. } => Err(unavailable(
                "habit recording requires the connected command surface",
            )),
        }
    }

    #[allow(dead_code)]
    pub fn create_habit(
        &mut self,
        request: orbit_client::CreateHabitRequest,
    ) -> Result<HabitDto, ClientError> {
        match self {
            Self::Local(client) => client.create_habit(request),
            Self::Connected { .. } => Err(unavailable(
                "habit creation requires the connected command surface",
            )),
        }
    }

    #[allow(dead_code)]
    pub fn habit_history(
        &self,
        habit_id: String,
    ) -> Result<Vec<orbit_client::HabitOccurrenceDto>, ClientError> {
        match self {
            Self::Local(client) => client.habit_history(habit_id),
            Self::Connected { .. } => Err(unavailable(
                "connected mode habit history uses the legacy command surface",
            )),
        }
    }

    pub fn install_bundled_blueprint(
        &mut self,
        id: &str,
    ) -> Result<orbit_blueprint::Blueprint, ClientError> {
        match self {
            Self::Local(client) => client.install_bundled_blueprint(id),
            Self::Connected { .. } => Err(unavailable(
                "blueprint installation requires the connected command surface",
            )),
        }
    }

    pub fn bind_project_blueprint(
        &mut self,
        project_id: &str,
        blueprint_id: &str,
        version: u32,
    ) -> Result<orbit_blueprint::BlueprintBinding, ClientError> {
        match self {
            Self::Local(client) => client.bind_project_blueprint(project_id, blueprint_id, version),
            Self::Connected { .. } => Err(unavailable(
                "blueprint binding requires the connected command surface",
            )),
        }
    }

    pub fn list_blueprint_versions(&self) -> Result<Vec<(String, u32)>, ClientError> {
        match self {
            Self::Local(client) => client.list_blueprint_versions(),
            Self::Connected { .. } => Err(unavailable(
                "connected mode does not expose published blueprint versions yet",
            )),
        }
    }

    pub fn project_blueprint_definition(
        &self,
        project_id: &str,
    ) -> Result<Option<orbit_blueprint::Blueprint>, ClientError> {
        match self {
            Self::Local(client) => client.project_blueprint_definition(project_id),
            Self::Connected { .. } => Err(unavailable(
                "connected mode does not project pinned blueprint definitions yet",
            )),
        }
    }

    /// Raw pinned binding; the shell prefers `project_workflow` for labels and
    /// keeps this for connected/legacy diagnostics.
    #[allow(dead_code)]
    pub fn project_blueprint_binding(
        &self,
        project_id: &str,
    ) -> Result<Option<orbit_blueprint::BlueprintBinding>, ClientError> {
        match self {
            Self::Local(client) => client.project_blueprint_binding(project_id),
            Self::Connected { .. } => Ok(None),
        }
    }

    pub fn list_blueprint_work_items(
        &self,
        project_id: &str,
    ) -> Result<Vec<orbit_blueprint::BlueprintWorkItem>, ClientError> {
        match self {
            Self::Local(client) => client.list_blueprint_work_items(project_id),
            Self::Connected { .. } => Err(unavailable(
                "connected mode does not project blueprint work items yet",
            )),
        }
    }

    pub fn create_blueprint_work_item(
        &mut self,
        project_id: &str,
        kind: &str,
        properties: std::collections::BTreeMap<String, serde_json::Value>,
    ) -> Result<orbit_blueprint::BlueprintWorkItem, ClientError> {
        match self {
            Self::Local(client) => client.create_blueprint_work_item(project_id, kind, properties),
            Self::Connected { .. } => Err(unavailable(
                "work-item creation requires the connected command surface",
            )),
        }
    }

    pub fn update_blueprint_work_item(
        &mut self,
        id: &str,
        expected_revision: u64,
        changes: std::collections::BTreeMap<String, serde_json::Value>,
    ) -> Result<orbit_blueprint::BlueprintWorkItem, ClientError> {
        match self {
            Self::Local(client) => {
                client.update_blueprint_work_item(id, expected_revision, changes)
            }
            Self::Connected { .. } => Err(unavailable(
                "work-item editing requires the connected command surface",
            )),
        }
    }

    pub fn transition_blueprint_work_item(
        &mut self,
        id: &str,
        to: &str,
        expected_revision: u64,
    ) -> Result<orbit_blueprint::BlueprintWorkItem, ClientError> {
        match self {
            Self::Local(client) => client.transition_blueprint_work_item(id, to, expected_revision),
            Self::Connected { .. } => Err(unavailable(
                "workflow transitions require the connected command surface",
            )),
        }
    }

    pub fn work_brief(
        &self,
        request: &orbit_runtime::planning_brief::WorkBriefRequest,
    ) -> Result<orbit_runtime::planning_brief::WorkBrief, ClientError> {
        match self {
            Self::Local(client) => client.work_brief(request),
            Self::Connected { connection, .. } => connection.work_brief(request).map_err(mapped),
        }
    }

    pub fn planning_focus(
        &self,
        brief: &orbit_runtime::planning_brief::WorkBrief,
        request: &orbit_runtime::planning_focus::FocusRequest,
    ) -> Result<orbit_runtime::planning_focus::TaskFocus, ClientError> {
        match self {
            Self::Local(client) => client.planning_focus(brief, request),
            Self::Connected { connection, .. } => {
                connection.planning_focus(brief, request).map_err(mapped)
            }
        }
    }

    pub fn planning_group(
        &self,
        brief: &orbit_runtime::planning_brief::WorkBrief,
        request: &orbit_runtime::planning_focus::GroupFocusRequest,
    ) -> Result<orbit_runtime::planning_focus::GroupFocus, ClientError> {
        match self {
            Self::Local(client) => client.planning_group(brief, request),
            Self::Connected { connection, .. } => {
                connection.planning_group(brief, request).map_err(mapped)
            }
        }
    }

    pub fn planning_task_actions(
        &self,
        brief: &orbit_runtime::planning_brief::WorkBrief,
        request: &orbit_runtime::planning_actions::TaskActionRequest,
    ) -> Result<orbit_runtime::planning_actions::PlanningActionMenu, ClientError> {
        match self {
            Self::Local(client) => client.planning_task_actions(brief, request),
            Self::Connected { connection, .. } => connection
                .planning_task_actions(brief, request)
                .map_err(mapped),
        }
    }

    pub fn planning_task_handoff(
        &self,
        brief: &orbit_runtime::planning_brief::WorkBrief,
        request: &orbit_runtime::planning_actions::TaskActionRequest,
        selection: &orbit_runtime::planning_actions::ActionSelection,
    ) -> Result<orbit_runtime::planning_handoff::PlanningHandoff, ClientError> {
        match self {
            Self::Local(client) => client.planning_task_handoff(brief, request, selection),
            Self::Connected { connection, .. } => connection
                .planning_task_handoff(brief, request, selection)
                .map_err(mapped),
        }
    }

    pub fn preview_task_dependency(
        &self,
        request: &orbit_runtime::task_dependency_preview::DependencyPreviewRequest,
    ) -> Result<orbit_runtime::task_dependency_preview::DependencyPreview, ClientError> {
        match self {
            Self::Local(client) => client.preview_task_dependency(request),
            Self::Connected { connection, .. } => {
                connection.preview_task_dependency(request).map_err(mapped)
            }
        }
    }

    pub fn schedule_plan(
        &self,
        request: &orbit_runtime::schedule_planning::SchedulePlanRequest,
    ) -> Result<orbit_runtime::schedule_planning::SchedulePlan, ClientError> {
        match self {
            Self::Local(client) => client.schedule_plan(request),
            Self::Connected { connection, .. } => connection.schedule_plan(request).map_err(mapped),
        }
    }

    pub fn completion_impact(
        &self,
        request: &orbit_runtime::task_impact::CompletionImpactRequest,
    ) -> Result<orbit_runtime::task_impact::CompletionImpact, ClientError> {
        match self {
            Self::Local(client) => client.completion_impact(request),
            Self::Connected { connection, .. } => {
                connection.preview_task_completion(request).map_err(mapped)
            }
        }
    }

    pub fn preview_task_plan(
        &self,
        _start_ms: i64,
        _end_ms: i64,
        _policy: orbit_client::PlanningPolicyDto,
    ) -> Result<TaskPlanProposalDto, ClientError> {
        match self {
            Self::Local(client) => client.preview_task_plan(_start_ms, _end_ms, _policy),
            Self::Connected { .. } => Err(unavailable(
                "task-plan preview requires the local authoritative planner",
            )),
        }
    }

    pub fn apply_task_plan(
        &mut self,
        _plan: TaskPlanProposalDto,
    ) -> Result<Vec<orbit_client::ScheduleBlockDto>, ClientError> {
        match self {
            Self::Local(client) => client.apply_task_plan(_plan),
            Self::Connected { .. } => Err(unavailable(
                "task-plan application requires the connected command surface",
            )),
        }
    }

    /// One task with its rich details.
    ///
    /// Local mode reads the authoritative projection; connected mode reads the
    /// authoritative task-detail route so the details editor sees description,
    /// priority and labels instead of the thin accepted snapshot.
    pub fn task_detail(&self, id: &str) -> Result<TaskDto, ClientError> {
        match self {
            Self::Local(client) => client.task_detail(id),
            Self::Connected { connection, .. } => {
                connection.task_detail(id).map_err(connected_error)
            }
        }
    }

    /// Rich task details (description, priority, labels).
    ///
    /// Connected mode queues the canonical `SetDetails` command and returns the
    /// currently accepted projection, so the caller must consult `take_outcome`
    /// before reporting success.
    pub fn update_task_details(
        &mut self,
        task_id: String,
        description: String,
        priority: Option<String>,
        labels: Vec<String>,
    ) -> Result<TaskDto, ClientError> {
        match self {
            Self::Local(client) => {
                client.update_task_details(task_id, description, priority, labels)
            }
            Self::Connected { connection, .. } => {
                let before = connection.last_queued_command_id();
                let result = connection
                    .update_task_details(task_id, description, priority, labels)
                    .map_err(connected_error);
                if result.is_ok() {
                    self.record_rich_outcome(before);
                }
                result
            }
        }
    }

    pub fn acceptance_criteria(
        &self,
        task_id: &str,
    ) -> Result<Vec<AcceptanceCriterionDto>, ClientError> {
        match self {
            Self::Local(client) => client.acceptance_criteria(task_id),
            Self::Connected { connection, .. } => connection
                .acceptance_criteria(task_id)
                .map_err(connected_error),
        }
    }

    pub fn add_acceptance_criterion(
        &mut self,
        task_id: String,
        text: String,
    ) -> Result<Vec<AcceptanceCriterionDto>, ClientError> {
        match self {
            Self::Local(client) => client.add_acceptance_criterion(task_id, text),
            Self::Connected { connection, .. } => {
                let before = connection.last_queued_command_id();
                let result = connection
                    .add_acceptance_criterion(task_id, text)
                    .map_err(connected_error);
                if result.is_ok() {
                    self.record_rich_outcome(before);
                }
                result
            }
        }
    }

    pub fn set_acceptance_criterion_text(
        &mut self,
        task_id: String,
        criterion_id: String,
        text: String,
    ) -> Result<Vec<AcceptanceCriterionDto>, ClientError> {
        match self {
            Self::Local(client) => {
                client.set_acceptance_criterion_text(task_id, criterion_id, text)
            }
            Self::Connected { connection, .. } => {
                let before = connection.last_queued_command_id();
                let result = connection
                    .set_acceptance_criterion_text(task_id, criterion_id, text)
                    .map_err(connected_error);
                if result.is_ok() {
                    self.record_rich_outcome(before);
                }
                result
            }
        }
    }

    pub fn set_acceptance_criterion_completed(
        &mut self,
        task_id: String,
        criterion_id: String,
        completed: bool,
    ) -> Result<Vec<AcceptanceCriterionDto>, ClientError> {
        match self {
            Self::Local(client) => {
                client.set_acceptance_criterion_completed(task_id, criterion_id, completed)
            }
            Self::Connected { connection, .. } => {
                let before = connection.last_queued_command_id();
                let result = connection
                    .set_acceptance_criterion_completed(task_id, criterion_id, completed)
                    .map_err(connected_error);
                if result.is_ok() {
                    self.record_rich_outcome(before);
                }
                result
            }
        }
    }

    pub fn move_acceptance_criterion(
        &mut self,
        task_id: String,
        criterion_id: String,
        position: usize,
    ) -> Result<Vec<AcceptanceCriterionDto>, ClientError> {
        match self {
            Self::Local(client) => {
                client.move_acceptance_criterion(task_id, criterion_id, position)
            }
            Self::Connected { connection, .. } => {
                let before = connection.last_queued_command_id();
                let result = connection
                    .move_acceptance_criterion(task_id, criterion_id, position)
                    .map_err(connected_error);
                if result.is_ok() {
                    self.record_rich_outcome(before);
                }
                result
            }
        }
    }

    pub fn remove_acceptance_criterion(
        &mut self,
        task_id: String,
        criterion_id: String,
    ) -> Result<Vec<AcceptanceCriterionDto>, ClientError> {
        match self {
            Self::Local(client) => client.remove_acceptance_criterion(task_id, criterion_id),
            Self::Connected { connection, .. } => {
                let before = connection.last_queued_command_id();
                let result = connection
                    .remove_acceptance_criterion(task_id, criterion_id)
                    .map_err(connected_error);
                if result.is_ok() {
                    self.record_rich_outcome(before);
                }
                result
            }
        }
    }

    pub fn task_activity(&self, task_id: &str) -> Result<Vec<ActivityDto>, ClientError> {
        match self {
            Self::Local(client) => client.task_activity(task_id),
            Self::Connected { connection, .. } => {
                connection.task_activity(task_id).map_err(connected_error)
            }
        }
    }

    pub fn list_attachments(
        &self,
        owner_kind: &str,
        owner_id: &str,
    ) -> Result<Vec<AttachmentDto>, ClientError> {
        match self {
            Self::Local(client) => client.list_attachments(owner_kind, owner_id),
            Self::Connected { connection, .. } => connection
                .list_attachments(owner_kind, owner_id)
                .map_err(connected_error),
        }
    }

    /// Unacknowledged uploads for one owner. Local mode has no staging queue,
    /// so it always returns an empty list; connected mode reads the replica.
    pub fn pending_attachments(
        &self,
        owner_kind: &str,
        owner_id: &str,
    ) -> Result<Vec<PendingAttachmentDto>, ClientError> {
        match self {
            Self::Local(_) => Ok(Vec::new()),
            Self::Connected { connection, .. } => connection
                .staged_attachments(owner_kind, owner_id)
                .map_err(connected_error),
        }
    }

    /// Number of staged uploads the server has not accepted yet.
    pub fn pending_attachment_count(&self) -> Result<i64, ClientError> {
        match self {
            Self::Local(_) => Ok(0),
            Self::Connected { connection, .. } => connection
                .pending_attachment_upload_count()
                .map_err(connected_error),
        }
    }

    /// One staged upload by attachment id, across owners.
    ///
    /// Preview uses this to label bytes that still live in local staging as
    /// pending instead of implying they are accepted server content.
    pub fn pending_attachment(
        &self,
        attachment_id: &str,
    ) -> Result<Option<PendingAttachmentDto>, ClientError> {
        match self {
            Self::Local(_) => Ok(None),
            Self::Connected { connection, .. } => Ok(connection
                .pending_attachment_uploads()
                .map_err(connected_error)?
                .into_iter()
                .find(|pending| pending.attachment.id == attachment_id)),
        }
    }

    /// Durable state of one queued command, with its conflict reason when the
    /// outbox recorded a permanent rejection.
    #[allow(dead_code)]
    pub fn command_outcome(&self, command_id: &str) -> MutationOutcome {
        match self {
            Self::Local(_) => MutationOutcome::Accepted,
            Self::Connected { connection, .. } => command_outcome(connection, command_id),
        }
    }

    /// Identity of the most recent command this connected process queued.
    #[allow(dead_code)]
    pub fn last_queued_command_id(&self) -> Option<String> {
        match self {
            Self::Local(_) => None,
            Self::Connected { connection, .. } => connection.last_queued_command_id(),
        }
    }

    pub fn import_attachment(
        &mut self,
        owner_kind: String,
        owner_id: String,
        path: String,
        description: Option<String>,
    ) -> Result<AttachmentDto, ClientError> {
        match self {
            Self::Local(client) => {
                client.import_attachment(owner_kind, owner_id, path, description)
            }
            Self::Connected { connection, .. } => connection
                .import_attachment(owner_kind, owner_id, path, description)
                .map_err(connected_error),
        }
    }

    pub fn attachment_content(&self, id: &str) -> Result<(AttachmentDto, Vec<u8>), ClientError> {
        match self {
            Self::Local(client) => client.attachment_content(id),
            Self::Connected { connection, .. } => {
                connection.attachment_content(id).map_err(connected_error)
            }
        }
    }

    pub fn export_attachment(
        &self,
        id: &str,
        destination: String,
    ) -> Result<AttachmentDto, ClientError> {
        match self {
            Self::Local(client) => client.export_attachment(id, destination),
            Self::Connected { connection, .. } => connection
                .export_attachment(id, destination)
                .map_err(connected_error),
        }
    }

    pub fn remove_attachment(&mut self, id: String) -> Result<bool, ClientError> {
        match self {
            Self::Local(client) => client.remove_attachment(id),
            Self::Connected { connection, .. } => {
                connection.remove_attachment(id).map_err(connected_error)
            }
        }
    }

    pub fn purge_attachment_content(&mut self) -> Result<usize, ClientError> {
        match self {
            Self::Local(client) => client.purge_attachment_content(),
            Self::Connected { connection, .. } => connection
                .purge_attachment_content()
                .map_err(connected_error),
        }
    }

    /// Workflows available offline. The descriptors are compiled into the
    /// binary, so this works in both deployment modes.
    pub fn bundled_workflows(&self) -> Vec<WorkflowDescriptorDto> {
        match self {
            Self::Local(client) => client.bundled_workflows(),
            Self::Connected { .. } => orbit_blueprint::bundled::BUNDLED_BLUEPRINTS
                .iter()
                .map(|descriptor| WorkflowDescriptorDto {
                    id: descriptor.id.to_owned(),
                    name: descriptor.name.to_owned(),
                    description: descriptor.description.to_owned(),
                    version: descriptor.version,
                    bundled: true,
                    is_default: descriptor.id == "basic",
                })
                .collect(),
        }
    }

    pub fn apply_workflow(
        &mut self,
        project_id: String,
        workflow_id: String,
        version: Option<u32>,
    ) -> Result<ProjectDto, ClientError> {
        match self {
            Self::Local(client) => client.apply_workflow(project_id, workflow_id, version),
            Self::Connected { .. } => Err(unavailable(
                "changing a project workflow requires the local authoritative workspace",
            )),
        }
    }

    pub fn project_workflow(
        &self,
        project_id: &str,
    ) -> Result<Option<ProjectWorkflowDto>, ClientError> {
        match self {
            Self::Local(client) => client.project_workflow(project_id),
            Self::Connected { .. } => Ok(None),
        }
    }

    /// Directory used for explicit attachment exports and viewer handoff.
    ///
    /// Connected mode has no server filesystem access, so exports use a
    /// process-scoped private temp directory. It is created with 0700 on Unix
    /// and never reused across processes.
    pub fn attachment_export_dir(&self) -> Result<std::path::PathBuf, ClientError> {
        match self {
            Self::Local(client) => Ok(client.attachment_store_root().join("exports")),
            Self::Connected { .. } => connected_export_dir(),
        }
    }

    /// Explicitly unpin a project's workflow (falls back to generated views).
    #[allow(dead_code)]
    pub fn remove_project_workflow(&mut self, project_id: String) -> Result<bool, ClientError> {
        match self {
            Self::Local(client) => client.remove_project_workflow(project_id),
            Self::Connected { .. } => Err(unavailable(
                "changing a project workflow requires the local authoritative workspace",
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_state_mapping_never_promotes_unknown_state_to_success() {
        assert_eq!(
            outcome_from_state(Some("cmd".into()), Some("accepted"), None),
            MutationOutcome::Accepted
        );
        assert_eq!(
            outcome_from_state(Some("cmd".into()), Some("pending"), None),
            MutationOutcome::Pending {
                command_id: Some("cmd".into()),
                state: "pending".into()
            }
        );
        assert!(matches!(
            outcome_from_state(
                Some("cmd".into()),
                Some("conflict"),
                Some("stale revision".into())
            ),
            MutationOutcome::Conflict { ref reason, .. } if reason == "stale revision"
        ));
        // Missing state and unexpected states fail closed as pending.
        assert_eq!(
            outcome_from_state(Some("cmd".into()), None, None),
            MutationOutcome::Pending {
                command_id: Some("cmd".into()),
                state: "unavailable".into()
            }
        );
        assert_eq!(
            outcome_from_state(Some("cmd".into()), Some("superseded"), None),
            MutationOutcome::Pending {
                command_id: Some("cmd".into()),
                state: "superseded".into()
            }
        );
    }

    #[test]
    fn connected_export_directory_is_process_scoped_and_private() {
        let first = super::connected_export_dir().unwrap();
        let second = super::connected_export_dir().unwrap();
        assert_eq!(first, second, "one directory per process");
        assert!(first.is_dir());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&first).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o700, "export directory must be private");
        }
    }
}
