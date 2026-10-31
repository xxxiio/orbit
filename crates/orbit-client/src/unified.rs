//! One canonical client entry point for a future Atomic App, CLI, or TUI.
//!
//! Both connections expose the same domain commands. Only the embedded host
//! can accept a command synchronously; remote/offline writes remain pending
//! until the authoritative server acknowledges them.
use crate::{
    authority::{AuthorityError, AuthorityHttp, CommandKind, FlushOutcome},
    connected::ConnectedOrbitClient,
    embedded_authority::EmbeddedAuthorityClient,
};
use orbit_runtime::{
    authority_evidence::EvidenceCommand,
    authority_milestones::MilestoneCommand,
    authority_routines::RoutineCommand,
    authority_schedule::ScheduleCommand,
    authority_strategy::StrategyCommand,
    domain_commands::{ProjectCommand, TaskCommand},
};
use serde_json::{Value, json};
use std::path::Path;

#[derive(Debug)]
pub enum ClientConnectionError {
    Embedded(orbit_runtime::AuthorityError),
    Remote(AuthorityError),
    Serialization(serde_json::Error),
    Unsupported(&'static str),
    /// A local facade command failed with the typed client error contract.
    Local(crate::ClientError),
    /// The requested capability is not implemented by this connected build.
    /// Never returned for a command the authority actually accepted.
    Unavailable(String),
    Vault(orbit_runtime::knowledge_files::VaultError),
}
impl From<crate::ClientError> for ClientConnectionError {
    fn from(error: crate::ClientError) -> Self {
        Self::Local(error)
    }
}
impl From<AuthorityError> for ClientConnectionError {
    fn from(error: AuthorityError) -> Self {
        Self::Remote(error)
    }
}
impl From<orbit_runtime::AuthorityError> for ClientConnectionError {
    fn from(error: orbit_runtime::AuthorityError) -> Self {
        Self::Embedded(error)
    }
}
impl From<serde_json::Error> for ClientConnectionError {
    fn from(error: serde_json::Error) -> Self {
        Self::Serialization(error)
    }
}

/// A command is only authoritative after the host commits it. A remote
/// `Pending` result never indicates that accepted state has changed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandDisposition {
    Accepted { revision: i64, duplicate: bool },
    AcceptedMarkdown { revision: i64 },
    AcceptedFolder,
    Pending,
}

fn now_ms_value() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or_default()
}

fn encode_query(input: &str) -> String {
    let mut encoded = String::new();
    for byte in input.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            encoded.push(char::from(byte));
        } else {
            use std::fmt::Write;
            write!(&mut encoded, "%{byte:02X}").expect("write to String");
        }
    }
    encoded
}

/// The same logical retry must not be reported as pending after its receipt
/// has already been persisted. A freshly queued intent is always pending.
fn remote_disposition(
    replica: &ConnectedOrbitClient,
    id: &str,
) -> Result<CommandDisposition, ClientConnectionError> {
    match replica.accepted_receipt(id)? {
        Some(receipt) => {
            let revision = receipt.get("revision").and_then(Value::as_i64).ok_or(
                ClientConnectionError::Unsupported("persisted receipt lacks revision"),
            )?;
            Ok(CommandDisposition::Accepted {
                revision,
                duplicate: true,
            })
        }
        None => Ok(CommandDisposition::Pending),
    }
}

mod workflow;
pub use workflow::*;
mod planning_queries;
mod rich_client;
pub use rich_client::*;

pub enum AuthorityConnection {
    Embedded(EmbeddedAuthorityClient),
    Remote {
        replica: ConnectedOrbitClient,
        http: Option<AuthorityHttp>,
    },
}

impl AuthorityConnection {
    /// Open host-local authority; only the host owns authoritative SQLite.
    pub fn embedded(path: impl AsRef<Path>, actor: &str) -> Result<Self, ClientConnectionError> {
        Ok(Self::Embedded(EmbeddedAuthorityClient::open(path, actor)?))
    }

    /// Open a remote client's replica and outbox. A missing identity/token
    /// permits offline use but not server communication.
    pub fn remote(
        replica_path: impl AsRef<Path>,
        server: &str,
        identity: Option<(&str, &str)>,
    ) -> Result<Self, ClientConnectionError> {
        let replica = ConnectedOrbitClient::open(replica_path, server)?;
        let http = identity
            .map(|(device, token)| AuthorityHttp::new(server, token, device))
            .transpose()?;
        Ok(Self::Remote { replica, http })
    }

    /// A uniform JSON workspace shape. Remote returns only accepted cached
    /// state; pending intentions are reported separately, never projected.
    pub fn workspace(&self) -> Result<Value, ClientConnectionError> {
        match self {
            Self::Embedded(host) => {
                let (revision, history_fingerprint, projects, tasks, evidence) = host.snapshot()?;
                let schedule = host.schedule()?;
                let routines = host.routines()?;
                let milestones = host.milestones()?;
                let strategies = host.strategies()?;
                if schedule.revision != revision
                    || routines.revision != revision
                    || milestones.revision != revision
                    || strategies.revision != revision
                {
                    return Err(ClientConnectionError::Unsupported(
                        "inconsistent embedded schedule revision",
                    ));
                }
                Ok(json!({
                    "revision": revision, "history_fingerprint": history_fingerprint,
                    "projects": projects, "tasks": tasks,
                    "task_dependencies": host.task_dependency_edges()?,
                    "habits": evidence.habits,
                    "habit_occurrences": evidence.habit_occurrences,
                    "metrics": evidence.metrics,
                    "metric_observations": evidence.metric_observations,
                    "schedule": schedule.blocks,
                    "routines": routines.routines,
                    "milestones": milestones.milestones,
                    "strategies": strategies.strategies,
                    "pending_count": 0,
                }))
            }
            Self::Remote { replica, .. } => Ok(serde_json::to_value(replica.workspace()?)?),
        }
    }

    /// A stable snapshot of the last accepted server state. The remote path
    /// never merges queued intentions into the accepted projection.
    /// `None` means this device has not yet received an accepted snapshot.
    pub fn accepted_snapshot(&self) -> Result<Option<Value>, ClientConnectionError> {
        match self {
            Self::Embedded(host) => {
                let (revision, history_fingerprint, projects, tasks, evidence) = host.snapshot()?;
                let schedule = host.schedule()?;
                let routines = host.routines()?;
                let milestones = host.milestones()?;
                let strategies = host.strategies()?;
                if schedule.revision != revision
                    || routines.revision != revision
                    || milestones.revision != revision
                    || strategies.revision != revision
                {
                    return Err(ClientConnectionError::Unsupported(
                        "inconsistent embedded schedule revision",
                    ));
                }
                Ok(Some(json!({
                    "revision": revision, "history_fingerprint": history_fingerprint,
                    "projects": projects, "tasks": tasks,
                    "task_dependencies": host.task_dependency_edges()?,
                    "habits": evidence.habits,
                    "habit_occurrences": evidence.habit_occurrences,
                    "metrics": evidence.metrics,
                    "metric_observations": evidence.metric_observations,
                    "schedule": schedule.blocks,
                    "routines": routines.routines,
                    "milestones": milestones.milestones,
                    "strategies": strategies.strategies,
                })))
            }
            Self::Remote { replica, .. } => {
                let workspace = replica.workspace()?;
                if workspace.revision.is_none() {
                    return Ok(None);
                }
                Ok(Some(json!({
                    "revision": workspace.revision,
                    "history_fingerprint": workspace.history_fingerprint,
                    "projects": workspace.projects, "tasks": workspace.tasks,
                    "task_dependencies": workspace.task_dependencies,
                    "habits": workspace.habits,
                    "habit_occurrences": workspace.habit_occurrences,
                    "metrics": workspace.metrics,
                    "metric_observations": workspace.metric_observations,
                    "schedule": workspace.schedule,
                    "routines": workspace.routines,
                    "milestones": workspace.milestones,
                    "strategies": workspace.strategies,
                })))
            }
        }
    }

    /// Identical accepted project projection in embedded and remote modes.
    /// Remote returns last refreshed accepted state, never pending writes.
    /// Renderer-neutral workspace projection over accepted snapshot data. A
    /// remote client without an accepted snapshot returns an empty workspace,
    /// never a pending-command approximation.
    pub fn workspace_view_data(&self) -> Result<orbit_ui::WorkspaceData, orbit_ui::ViewDataError> {
        let snapshot = self
            .accepted_snapshot()
            .map_err(|error| orbit_ui::ViewDataError::unavailable(format!("{error:?}")))?;
        let mut workspace = orbit_ui::workspace_from_snapshot(snapshot.as_ref(), now_ms_value())?;
        for task in &mut workspace.tasks {
            task.allowed_statuses = crate::view_data::allowed_task_statuses(&task.status);
        }
        for task in &mut workspace.inbox {
            task.allowed_statuses = crate::view_data::allowed_task_statuses(&task.status);
        }
        Ok(workspace)
    }

    /// Renderer-neutral project projection over the same accepted snapshot.
    pub fn project_view_data(
        &self,
        project_id: &str,
    ) -> Result<orbit_ui::ProjectData, orbit_ui::ViewDataError> {
        let snapshot = self
            .accepted_snapshot()
            .map_err(|error| orbit_ui::ViewDataError::unavailable(format!("{error:?}")))?;
        let mut data =
            orbit_ui::project_from_snapshot(snapshot.as_ref(), project_id, now_ms_value())?;
        for task in &mut data.tasks {
            task.allowed_statuses = crate::view_data::allowed_task_statuses(&task.status);
        }
        Ok(data)
    }

    /// Resolve generated views for connected clients. Blueprint work items are
    /// not present in the accepted snapshot yet; the adapter never invents them.
    pub fn resolve_connected_project_views(
        &self,
        project_id: &str,
        registry: &orbit_ui::ComponentRegistry,
    ) -> Result<(orbit_ui::ProjectData, Vec<orbit_ui::ResolvedView>, bool), orbit_ui::ViewDataError>
    {
        let data = self.project_view_data(project_id)?;
        let (views, generated) = orbit_ui::resolve_blueprint_views(None, &data, registry);
        Ok((data, views, generated))
    }

    pub fn projects(&self) -> Result<Vec<Value>, ClientConnectionError> {
        self.accepted_collection("projects")
    }

    pub fn strategies(&self) -> Result<Vec<Value>, ClientConnectionError> {
        self.accepted_collection("strategies")
    }
    pub fn strategy_command(
        &mut self,
        command: orbit_runtime::authority_strategy::StrategyCommand,
    ) -> Result<CommandDisposition, ClientConnectionError> {
        match self {
            Self::Embedded(host) => {
                let receipt = host.strategy_command(command)?;
                Ok(CommandDisposition::Accepted {
                    revision: receipt.revision,
                    duplicate: receipt.duplicate,
                })
            }
            Self::Remote { replica, .. } => {
                let id = command.command_id.clone();
                replica.queue_strategy_action(&id, &command.action)?;
                remote_disposition(replica, &id)
            }
        }
    }

    pub fn milestones(&self) -> Result<Vec<Value>, ClientConnectionError> {
        self.accepted_collection("milestones")
    }
    pub fn milestone_command(
        &mut self,
        command: orbit_runtime::authority_milestones::MilestoneCommand,
    ) -> Result<CommandDisposition, ClientConnectionError> {
        match self {
            Self::Embedded(host) => {
                let receipt = host.milestone_command(command)?;
                Ok(CommandDisposition::Accepted {
                    revision: receipt.revision,
                    duplicate: receipt.duplicate,
                })
            }
            Self::Remote { replica, .. } => {
                let id = command.command_id.clone();
                replica.queue_milestone_action(&id, &command.action)?;
                remote_disposition(replica, &id)
            }
        }
    }

    /// Last accepted routine compositions; never includes pending offline updates.
    pub fn routines(&self) -> Result<Vec<Value>, ClientConnectionError> {
        self.accepted_collection("routines")
    }

    /// One authoritative routine mutation (or a durable pending remote intent).
    pub fn routine_command(
        &mut self,
        command: orbit_runtime::authority_routines::RoutineCommand,
    ) -> Result<CommandDisposition, ClientConnectionError> {
        match self {
            Self::Embedded(host) => {
                let receipt = host.routine_command(command)?;
                Ok(CommandDisposition::Accepted {
                    revision: receipt.revision,
                    duplicate: receipt.duplicate,
                })
            }
            Self::Remote { replica, .. } => {
                let id = command.command_id.clone();
                replica.queue_routine_action(&id, &command.action)?;
                remote_disposition(replica, &id)
            }
        }
    }

    /// Accepted schedule projection only; remote offline writes remain pending.
    pub fn schedule(&self) -> Result<Vec<Value>, ClientConnectionError> {
        self.accepted_collection("schedule")
    }

    /// Schedule creates, edits and deletion share the task/project authority
    /// ledger and the server's optimistic revision fence.
    pub fn schedule_command(
        &mut self,
        command: orbit_runtime::authority_schedule::ScheduleCommand,
    ) -> Result<CommandDisposition, ClientConnectionError> {
        match self {
            Self::Embedded(host) => {
                let receipt = host.schedule_command(command)?;
                Ok(CommandDisposition::Accepted {
                    revision: receipt.revision,
                    duplicate: receipt.duplicate,
                })
            }
            Self::Remote { replica, .. } => {
                let id = command.command_id.clone();
                replica.queue_schedule_action(&id, &command.action)?;
                remote_disposition(replica, &id)
            }
        }
    }

    pub fn tasks(&self) -> Result<Vec<Value>, ClientConnectionError> {
        self.accepted_collection("tasks")
    }

    pub fn habits(&self) -> Result<Vec<Value>, ClientConnectionError> {
        self.accepted_collection("habits")
    }

    pub fn habit_occurrences(&self) -> Result<Vec<Value>, ClientConnectionError> {
        self.accepted_collection("habit_occurrences")
    }

    pub fn metrics(&self) -> Result<Vec<Value>, ClientConnectionError> {
        self.accepted_collection("metrics")
    }

    pub fn metric_observations(&self) -> Result<Vec<Value>, ClientConnectionError> {
        self.accepted_collection("metric_observations")
    }

    /// Tasks scoped to a project in the last accepted projection. Unaccepted
    /// offline writes never appear in the result. Unassigned tasks can be
    /// queried with `None`; `Some(id)` matches that exact project identity.
    pub fn tasks_for_project(
        &self,
        project_id: Option<&str>,
    ) -> Result<Vec<Value>, ClientConnectionError> {
        if project_id.is_some_and(|id| id.trim().is_empty()) {
            return Err(ClientConnectionError::Unsupported("empty project identity"));
        }
        Ok(self
            .tasks()?
            .into_iter()
            .filter(|task| match project_id {
                Some(id) => task.get("project_id").and_then(Value::as_str) == Some(id),
                None => task.get("project_id").is_none_or(Value::is_null),
            })
            .collect())
    }

    /// Accepted occurrences for a habit, suitable for a calendar/history view.
    /// This is a projection query, not an authoritative occurrence mutation.
    pub fn habit_occurrences_for(
        &self,
        habit_id: &str,
    ) -> Result<Vec<Value>, ClientConnectionError> {
        if habit_id.trim().is_empty() {
            return Err(ClientConnectionError::Unsupported("empty habit identity"));
        }
        Ok(self
            .habit_occurrences()?
            .into_iter()
            .filter(|row| row.get("habit_id").and_then(Value::as_str) == Some(habit_id))
            .collect())
    }

    /// Slice accepted tasks by their serialized status, without ever merging
    /// pending offline intents into a local projection.
    pub fn tasks_with_status(&self, status: &str) -> Result<Vec<Value>, ClientConnectionError> {
        if status.trim().is_empty() {
            return Err(ClientConnectionError::Unsupported("empty task status"));
        }
        Ok(self
            .tasks()?
            .into_iter()
            .filter(|task| task.get("status").and_then(Value::as_str) == Some(status))
            .collect())
    }

    /// A single accepted project detail projection for desktop, CLI and MCP.
    /// Tasks are resolved from the same snapshot as the project, so a refresh
    /// cannot mix revisions between parent and children. Pending offline
    /// changes are deliberately excluded from this read model.
    pub fn project_overview(
        &self,
        project_id: &str,
    ) -> Result<Option<Value>, ClientConnectionError> {
        if project_id.trim().is_empty() {
            return Err(ClientConnectionError::Unsupported("empty project identity"));
        }
        let Some(snapshot) = self.accepted_snapshot()? else {
            return Ok(None);
        };
        let projects = snapshot.get("projects").and_then(Value::as_array).ok_or(
            ClientConnectionError::Unsupported("accepted projection missing projects"),
        )?;
        let tasks = snapshot.get("tasks").and_then(Value::as_array).ok_or(
            ClientConnectionError::Unsupported("accepted projection missing tasks"),
        )?;
        let Some(project) = projects
            .iter()
            .find(|p| p.get("id").and_then(Value::as_str) == Some(project_id))
        else {
            return Ok(None);
        };
        let assigned: Vec<Value> = tasks
            .iter()
            .filter(|t| t.get("project_id").and_then(Value::as_str) == Some(project_id))
            .cloned()
            .collect();
        Ok(Some(json!({
            "revision": snapshot.get("revision"),
            "history_fingerprint": snapshot.get("history_fingerprint"),
            "project": project,
            "tasks": assigned,
        })))
    }

    /// Accepted status counts for project work or the unassigned inbox.
    /// The result uses exact serialized status names, retaining unfamiliar
    /// future statuses instead of silently dropping them.
    pub fn task_status_counts(
        &self,
        project_id: Option<&str>,
    ) -> Result<std::collections::BTreeMap<String, usize>, ClientConnectionError> {
        let mut counts = std::collections::BTreeMap::new();
        for task in self.tasks_for_project(project_id)? {
            let status = task.get("status").and_then(Value::as_str).ok_or(
                ClientConnectionError::Unsupported("accepted task missing status"),
            )?;
            *counts.entry(status.to_owned()).or_insert(0) += 1;
        }
        Ok(counts)
    }

    /// Read a stable, accepted-only history for a metric; unlike pending
    /// writes this can be displayed immediately in disconnected mode.
    pub fn metric_history(&self, metric_id: &str) -> Result<Vec<Value>, ClientConnectionError> {
        if metric_id.trim().is_empty() {
            return Err(ClientConnectionError::Unsupported("empty metric identity"));
        }
        self.metric_observations_for(metric_id)
    }

    /// Single-revision workspace dashboard suitable for CLI/TUI/MCP and
    /// future desktop clients. Never folds pending offline commands into
    /// accepted counts; `pending_count` is deliberately separate.
    pub fn dashboard_summary(&self) -> Result<Option<Value>, ClientConnectionError> {
        let Some(snapshot) = self.accepted_snapshot()? else {
            return Ok(None);
        };
        let collection =
            |key: &str| -> Result<&Vec<Value>, ClientConnectionError> {
                snapshot.get(key).and_then(Value::as_array).ok_or(
                    ClientConnectionError::Unsupported("accepted projection missing collection"),
                )
            };
        let projects = collection("projects")?;
        let tasks = collection("tasks")?;
        let habits = collection("habits")?;
        let occurrences = collection("habit_occurrences")?;
        let metrics = collection("metrics")?;
        let observations = collection("metric_observations")?;
        let mut statuses = std::collections::BTreeMap::<String, usize>::new();
        let mut unassigned = 0usize;
        for task in tasks {
            let status = task.get("status").and_then(Value::as_str).ok_or(
                ClientConnectionError::Unsupported("accepted task missing status"),
            )?;
            *statuses.entry(status.to_owned()).or_default() += 1;
            if task.get("project_id").is_none_or(Value::is_null) {
                unassigned += 1;
            }
        }
        let pending_count = match self {
            Self::Embedded(_) => 0,
            Self::Remote { replica, .. } => replica.workspace()?.pending_count,
        };
        Ok(Some(json!({
            "revision": snapshot.get("revision"),
            "history_fingerprint": snapshot.get("history_fingerprint"),
            "project_count": projects.len(), "task_count": tasks.len(),
            "unassigned_task_count": unassigned, "task_status_counts": statuses,
            "habit_count": habits.len(), "habit_occurrence_count": occurrences.len(),
            "metric_count": metrics.len(), "metric_observation_count": observations.len(),
            "pending_count": pending_count,
        })))
    }

    /// Query an individual entity from the accepted projection, not the
    /// unacknowledged outbox. Missing and not-yet-refreshed are distinct:
    /// callers can inspect `accepted_snapshot()` for the latter.
    pub fn project(&self, id: &str) -> Result<Option<Value>, ClientConnectionError> {
        self.accepted_by_id("projects", "id", id)
    }

    pub fn task(&self, id: &str) -> Result<Option<Value>, ClientConnectionError> {
        self.accepted_by_id("tasks", "id", id)
    }

    pub fn habit(&self, id: &str) -> Result<Option<Value>, ClientConnectionError> {
        self.accepted_by_id("habits", "id", id)
    }

    pub fn habit_occurrence(&self, id: &str) -> Result<Option<Value>, ClientConnectionError> {
        self.accepted_by_id("habit_occurrences", "id", id)
    }

    pub fn metric(&self, id: &str) -> Result<Option<Value>, ClientConnectionError> {
        self.accepted_by_id("metrics", "id", id)
    }

    /// Metric observations are not individually identified by the canonical
    /// schema. Return all accepted observations for the selected metric.
    pub fn metric_observations_for(&self, id: &str) -> Result<Vec<Value>, ClientConnectionError> {
        Ok(self
            .metric_observations()?
            .into_iter()
            .filter(|row| row.get("metric_id").and_then(Value::as_str) == Some(id))
            .collect())
    }

    fn accepted_by_id(
        &self,
        key: &str,
        id_key: &str,
        id: &str,
    ) -> Result<Option<Value>, ClientConnectionError> {
        if id.trim().is_empty() {
            return Err(ClientConnectionError::Unsupported("empty entity identity"));
        }
        Ok(self
            .accepted_collection(key)?
            .into_iter()
            .find(|row| row.get(id_key).and_then(Value::as_str) == Some(id)))
    }

    fn accepted_collection(&self, key: &str) -> Result<Vec<Value>, ClientConnectionError> {
        // An uninitialized remote cache has no authoritative rows.
        let Some(snapshot) = self.accepted_snapshot()? else {
            return Ok(Vec::new());
        };
        snapshot.get(key).and_then(Value::as_array).cloned().ok_or(
            ClientConnectionError::Unsupported("accepted projection missing collection"),
        )
    }

    /// Return the verified prefix hash for a global authority revision.
    /// Offline remote clients can prove only their most recent accepted tip;
    /// arbitrary historical prefix lookup requires a live server.
    pub fn history_prefix(&self, revision: i64) -> Result<String, ClientConnectionError> {
        if revision < 0 {
            return Err(ClientConnectionError::Unsupported("negative revision"));
        }
        match self {
            Self::Embedded(host) => Ok(host.history_prefix(revision)?),
            Self::Remote { replica, http } => {
                let cached = replica.workspace()?;
                if cached.revision == Some(revision) {
                    return cached
                        .history_fingerprint
                        .ok_or(ClientConnectionError::Unsupported(
                            "accepted fingerprint missing",
                        ));
                }
                let transport = http.as_ref().ok_or(ClientConnectionError::Unsupported(
                    "historical prefix requires remote connection",
                ))?;
                let result = transport.get(&format!(
                    "/v1/authoritative/history-prefix?revision={revision}"
                ))?;
                if result.get("revision").and_then(Value::as_i64) != Some(revision) {
                    return Err(ClientConnectionError::Unsupported(
                        "history prefix revision mismatch",
                    ));
                }
                result
                    .get("history_fingerprint")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
                    .ok_or(ClientConnectionError::Unsupported(
                        "missing history fingerprint",
                    ))
            }
        }
    }

    /// Execute typed commands in order using the same authority boundary in
    /// embedded and remote mode. This is intentionally non-atomic: a later
    /// error cannot undo already committed host commands or durable outbox
    /// entries. The result preserves the disposition of every completed step.
    ///
    /// Preflight prevents duplicate/empty command IDs within one workflow
    /// before any side effects. IDs remain stable across retries.
    pub fn execute_workflow(
        &mut self,
        steps: Vec<DomainWorkflowStep>,
    ) -> Result<Vec<CommandDisposition>, WorkflowFailure> {
        let mut ids = std::collections::HashSet::new();
        for (index, step) in steps.iter().enumerate() {
            let id = step.command_id();
            if id.trim().is_empty() || !ids.insert(id.to_owned()) {
                return Err(WorkflowFailure {
                    failed_index: index,
                    completed: Vec::new(),
                    cause: ClientConnectionError::Unsupported(
                        "workflow command IDs must be nonempty and unique",
                    ),
                });
            }
        }
        // Connected clients stage the entire reviewed workflow atomically in
        // the replica. This prevents a malformed or conflicting later command
        // from leaving an earlier intention unexpectedly pending. The server
        // still accepts the commands individually and in global queue order.
        if let Self::Remote { replica, .. } = self {
            let commands: Result<Vec<(String, CommandKind, Value)>, ClientConnectionError> = steps
                .iter()
                .map(|step| {
                    let (kind, action) = match step {
                        DomainWorkflowStep::Project(c) => {
                            (CommandKind::Project, serde_json::to_value(c)?)
                        }
                        DomainWorkflowStep::Task(c) => {
                            (CommandKind::Task, serde_json::to_value(&c.action)?)
                        }
                        DomainWorkflowStep::Evidence(c) => {
                            (CommandKind::Evidence, serde_json::to_value(&c.action)?)
                        }
                        DomainWorkflowStep::Schedule(c) => {
                            (CommandKind::Schedule, serde_json::to_value(&c.action)?)
                        }
                        DomainWorkflowStep::Routine(c) => {
                            (CommandKind::Routine, serde_json::to_value(&c.action)?)
                        }
                        DomainWorkflowStep::Milestone(c) => {
                            (CommandKind::Milestone, serde_json::to_value(&c.action)?)
                        }
                        DomainWorkflowStep::Strategy(c) => {
                            (CommandKind::Strategy, serde_json::to_value(&c.action)?)
                        }
                    };
                    // Project command actions are named fields, not a nested
                    // action. The outbox holds the endpoint JSON without the
                    // command envelope; flush supplies command_id/revision.
                    let action = if matches!(step, DomainWorkflowStep::Project(_)) {
                        serde_json::json!({"project_id":action["project_id"],"name":action["name"]})
                    } else {
                        action
                    };
                    Ok((step.command_id().to_string(), kind, action))
                })
                .collect();
            let commands = commands.map_err(|cause| WorkflowFailure {
                failed_index: 0,
                completed: Vec::new(),
                cause,
            })?;
            replica
                .queue_workflow(&commands)
                .map_err(|cause| WorkflowFailure {
                    failed_index: 0,
                    completed: Vec::new(),
                    cause: ClientConnectionError::Remote(cause),
                })?;
            return steps
                .iter()
                .map(|step| remote_disposition(replica, step.command_id()))
                .collect::<Result<Vec<_>, _>>()
                .map_err(|cause| WorkflowFailure {
                    failed_index: 0,
                    completed: Vec::new(),
                    cause,
                });
        }
        let mut completed = Vec::with_capacity(steps.len());
        for (index, step) in steps.into_iter().enumerate() {
            let result = match step {
                DomainWorkflowStep::Project(cmd) => self.create_project(cmd),
                DomainWorkflowStep::Task(cmd) => self.task_command(cmd),
                DomainWorkflowStep::Evidence(cmd) => self.evidence_command(cmd),
                DomainWorkflowStep::Schedule(cmd) => self.schedule_command(cmd),
                DomainWorkflowStep::Routine(cmd) => self.routine_command(cmd),
                DomainWorkflowStep::Milestone(cmd) => self.milestone_command(cmd),
                DomainWorkflowStep::Strategy(cmd) => self.strategy_command(cmd),
            };
            match result {
                Ok(disposition) => completed.push(disposition),
                Err(cause) => {
                    return Err(WorkflowFailure {
                        failed_index: index,
                        completed,
                        cause,
                    });
                }
            }
        }
        Ok(completed)
    }

    /// Bootstrap a project and all of its initial tasks with one client call.
    /// Errors include the accepted/queued prefix, which a caller must display
    /// before deciding whether to retry; the operation is not atomic.
    pub fn bootstrap_project(
        &mut self,
        manifest: ProjectBootstrap,
    ) -> Result<Vec<CommandDisposition>, WorkflowFailure> {
        let steps = manifest.into_workflow().map_err(|cause| WorkflowFailure {
            failed_index: 0,
            completed: Vec::new(),
            cause,
        })?;
        self.execute_workflow(steps)
    }

    /// Apply a full planning session to existing tasks. Validation is performed
    /// before the first durable write; partial execution failures are explicit.
    pub fn plan_tasks(
        &mut self,
        session: TaskPlanningSession,
    ) -> Result<Vec<CommandDisposition>, WorkflowFailure> {
        let steps = session.into_workflow().map_err(|cause| WorkflowFailure {
            failed_index: 0,
            completed: Vec::new(),
            cause,
        })?;
        self.execute_workflow(steps)
    }

    /// Apply a validated batch of task triage decisions through the same
    /// accepted command engine in embedded and remote modes. Partial failures
    /// expose the completed prefix for deterministic retries.
    pub fn refine_tasks(
        &mut self,
        manifest: TaskRefinement,
    ) -> Result<Vec<CommandDisposition>, WorkflowFailure> {
        let steps = manifest.into_workflow().map_err(|cause| WorkflowFailure {
            failed_index: 0,
            completed: Vec::new(),
            cause,
        })?;
        self.execute_workflow(steps)
    }

    pub fn create_project(
        &mut self,
        command: ProjectCommand,
    ) -> Result<CommandDisposition, ClientConnectionError> {
        match self {
            Self::Embedded(host) => {
                let receipt = host.create_project(command)?;
                Ok(CommandDisposition::Accepted {
                    revision: receipt.revision,
                    duplicate: receipt.duplicate,
                })
            }
            Self::Remote { replica, .. } => {
                replica.queue_project(&command.command_id, &command.project_id, &command.name)?;
                remote_disposition(replica, &command.command_id)
            }
        }
    }

    /// Plan or replan a task through the authoritative task command family.
    /// In connected mode this returns Pending until the server accepts it.
    pub fn set_task_planning(
        &mut self,
        command_id: String,
        expected_revision: i64,
        task_id: String,
        estimated_minutes: Option<i64>,
        earliest_start_at_ms: Option<i64>,
        deadline_at_ms: Option<i64>,
    ) -> Result<CommandDisposition, ClientConnectionError> {
        if command_id.trim().is_empty()
            || task_id.trim().is_empty()
            || expected_revision < 0
            || estimated_minutes.is_some_and(|v| v <= 0 || v > 525600)
            || earliest_start_at_ms.is_some_and(|v| v < 0)
            || deadline_at_ms.is_some_and(|v| v < 0)
            || matches!((earliest_start_at_ms, deadline_at_ms), (Some(start),Some(end)) if start > end)
        {
            return Err(ClientConnectionError::Unsupported(
                "invalid task planning request",
            ));
        }
        self.task_command(TaskCommand {
            command_id,
            expected_revision,
            action: orbit_runtime::domain_commands::TaskAction::SetPlanning {
                task_id,
                estimated_minutes,
                earliest_start_at_ms,
                deadline_at_ms,
            },
        })
    }

    pub fn add_task_prerequisite(
        &mut self,
        command_id: String,
        expected_revision: i64,
        task_id: String,
        prerequisite_id: String,
    ) -> Result<CommandDisposition, ClientConnectionError> {
        self.task_command(TaskCommand {
            command_id,
            expected_revision,
            action: orbit_runtime::domain_commands::TaskAction::AddPrerequisite {
                task_id,
                prerequisite_id,
            },
        })
    }

    pub fn remove_task_prerequisite(
        &mut self,
        command_id: String,
        expected_revision: i64,
        task_id: String,
        prerequisite_id: String,
    ) -> Result<CommandDisposition, ClientConnectionError> {
        self.task_command(TaskCommand {
            command_id,
            expected_revision,
            action: orbit_runtime::domain_commands::TaskAction::RemovePrerequisite {
                task_id,
                prerequisite_id,
            },
        })
    }

    pub fn task_command(
        &mut self,
        command: TaskCommand,
    ) -> Result<CommandDisposition, ClientConnectionError> {
        match self {
            Self::Embedded(host) => {
                let receipt = host.task_command(command)?;
                Ok(CommandDisposition::Accepted {
                    revision: receipt.revision,
                    duplicate: receipt.duplicate,
                })
            }
            Self::Remote { replica, .. } => {
                let id = command.command_id.clone();
                replica.queue_command(
                    &id,
                    CommandKind::Task,
                    &serde_json::to_value(command.action)?,
                )?;
                remote_disposition(replica, &id)
            }
        }
    }

    pub fn evidence_command(
        &mut self,
        command: EvidenceCommand,
    ) -> Result<CommandDisposition, ClientConnectionError> {
        match self {
            Self::Embedded(host) => {
                let receipt = host.evidence_command(command)?;
                Ok(CommandDisposition::Accepted {
                    revision: receipt.revision,
                    duplicate: receipt.duplicate,
                })
            }
            Self::Remote { replica, .. } => {
                let id = command.command_id.clone();
                replica.queue_command(
                    &id,
                    CommandKind::Evidence,
                    &serde_json::to_value(command.action)?,
                )?;
                remote_disposition(replica, &id)
            }
        }
    }

    /// The same logical command API in both deployment modes. The embedded
    /// host returns a committed receipt; a remote client durably queues the
    /// intention and returns Pending, even if an HTTP transport is configured.
    /// Call `flush()` to submit queued commands in their global order.
    pub fn markdown_command(
        &mut self,
        command_id: &str,
        command: orbit_runtime::markdown_commands::MarkdownCommand,
    ) -> Result<CommandDisposition, ClientConnectionError> {
        match self {
            Self::Embedded(host) => {
                let receipt = host.markdown_command(command_id, command)?;
                Ok(CommandDisposition::AcceptedMarkdown {
                    revision: receipt.revision,
                })
            }
            Self::Remote { replica, .. } => {
                replica.queue_markdown(command_id, &command)?;
                Ok(CommandDisposition::Pending)
            }
        }
    }

    /// The same file-backed folder operation in both modes. Remote clients
    /// cannot claim acceptance until the server returns a validated receipt.
    pub fn folder_create(
        &mut self,
        command_id: &str,
        command: orbit_runtime::folder_commands::FolderCreate,
    ) -> Result<CommandDisposition, ClientConnectionError> {
        match self {
            Self::Embedded(host) => {
                host.folder_create(command_id, command)?;
                Ok(CommandDisposition::AcceptedFolder)
            }
            Self::Remote { replica, .. } => {
                replica.queue_folder_create(command_id, &command)?;
                Ok(CommandDisposition::Pending)
            }
        }
    }

    pub fn folder_move(
        &mut self,
        command_id: &str,
        command: orbit_runtime::folder_commands::FolderMove,
    ) -> Result<CommandDisposition, ClientConnectionError> {
        match self {
            Self::Embedded(host) => {
                host.folder_move(command_id, command)?;
                Ok(CommandDisposition::AcceptedFolder)
            }
            Self::Remote { replica, .. } => {
                replica.queue_folder_move(command_id, &command)?;
                Ok(CommandDisposition::Pending)
            }
        }
    }

    /// Inspect a remote intention after restart without assuming a queued
    /// command is already accepted. Embedded commands are applied immediately
    /// and do not maintain an outbox.
    pub fn command_status(&self, id: &str) -> Result<Option<Value>, ClientConnectionError> {
        match self {
            Self::Embedded(_) => Err(ClientConnectionError::Unsupported(
                "embedded mode has no pending command outbox",
            )),
            Self::Remote { replica, .. } => Ok(replica.command_status(id)?),
        }
    }
    pub fn accepted_receipt(&self, id: &str) -> Result<Option<Value>, ClientConnectionError> {
        match self {
            Self::Embedded(_) => Err(ClientConnectionError::Unsupported(
                "embedded receipts are returned by command execution",
            )),
            Self::Remote { replica, .. } => Ok(replica.accepted_receipt(id)?),
        }
    }

    /// Inspect ordered offline intentions before deciding whether to refresh,
    /// retry or explicitly resolve a rejected command. This does not project
    /// intentions into accepted workspace state.
    pub fn unresolved_commands(&self) -> Result<Vec<Value>, ClientConnectionError> {
        match self {
            Self::Embedded(_) => Ok(Vec::new()),
            Self::Remote { replica, .. } => Ok(replica.unresolved()?),
        }
    }

    /// The last accepted snapshot, if this client has one. This is a thin alias
    /// for [`Self::accepted_snapshot`] used by frontends that only need to
    /// distinguish "accepted state observed" from "no snapshot yet".
    pub fn snapshot(&self) -> Result<Option<Value>, ClientConnectionError> {
        self.accepted_snapshot()
    }

    /// Durably queue one canonical intention in the connected replica.
    ///
    /// This is a direct passthrough to the replica outbox: embedded authority
    /// commits synchronously and therefore has no outbox to queue into. A
    /// successful return means the intention is pending locally, never that the
    /// server accepted it.
    pub fn queue_intent(
        &mut self,
        id: &str,
        kind: CommandKind,
        action: &Value,
    ) -> Result<(), ClientConnectionError> {
        match self {
            Self::Embedded(_) => Err(ClientConnectionError::Unsupported(
                "embedded authority commits commands immediately and keeps no outbox",
            )),
            Self::Remote { replica, .. } => {
                replica.queue_command(id, kind, action)?;
                Ok(())
            }
        }
    }

    /// Stage a complete user-reviewed workflow in one replica transaction.
    ///
    /// Staging is atomic locally; remote execution remains ordered and may
    /// commit a partial prefix. Nothing is accepted by staging alone.
    pub fn queue_intent_batch(
        &mut self,
        commands: &[(String, CommandKind, Value)],
    ) -> Result<usize, ClientConnectionError> {
        match self {
            Self::Embedded(_) => Err(ClientConnectionError::Unsupported(
                "embedded authority commits commands immediately and keeps no outbox",
            )),
            Self::Remote { replica, .. } => Ok(replica.queue_workflow(commands)?),
        }
    }

    pub fn converge(
        &mut self,
    ) -> Result<crate::authority::ConvergenceCycle, ClientConnectionError> {
        match self {
            Self::Embedded(_) => Err(ClientConnectionError::Unsupported(
                "embedded mode has no server peer",
            )),
            Self::Remote { replica, http } => {
                let transport = http.as_ref().ok_or(ClientConnectionError::Unsupported(
                    "offline: cannot synchronize with server",
                ))?;
                Ok(replica.converge(transport)?)
            }
        }
    }

    pub fn observe_convergence(
        &mut self,
    ) -> Result<crate::authority::ConvergenceObservation, ClientConnectionError> {
        match self {
            Self::Embedded(_) => Err(ClientConnectionError::Unsupported(
                "embedded mode has no server peer",
            )),
            Self::Remote { replica, http } => {
                let transport = http.as_ref().ok_or(ClientConnectionError::Unsupported(
                    "offline: server convergence cannot be verified",
                ))?;
                Ok(replica.observe_convergence(transport)?)
            }
        }
    }

    pub fn review_conflict_online(
        &mut self,
        id: &str,
    ) -> Result<crate::authority::ConflictReview, ClientConnectionError> {
        match self {
            Self::Embedded(_) => Err(ClientConnectionError::Unsupported(
                "no remote conflicts in embedded mode",
            )),
            Self::Remote { replica, http } => {
                let transport = http.as_ref().ok_or(ClientConnectionError::Unsupported(
                    "offline: cannot review latest accepted history",
                ))?;
                Ok(replica.review_conflict_online(transport, id)?)
            }
        }
    }

    /// Read-only replica status. In particular, offline state is never
    /// reported as synchronized with an unobserved live server.
    /// Full offline accepted-state and per-domain command summary.
    pub fn workspace_sync_report(
        &self,
    ) -> Result<crate::sync_health::WorkspaceSyncReport, ClientConnectionError> {
        match self {
            Self::Embedded(_) => Err(ClientConnectionError::Unsupported(
                "no replica in embedded mode",
            )),
            Self::Remote { replica, .. } => Ok(replica.workspace_sync_report()?),
        }
    }
    /// Compare the same domain report with a live authority observation.
    pub fn observe_workspace_sync_report(
        &mut self,
    ) -> Result<crate::sync_health::WorkspaceSyncReport, ClientConnectionError> {
        match self {
            Self::Embedded(_) => Err(ClientConnectionError::Unsupported(
                "no replica in embedded mode",
            )),
            Self::Remote { replica, http } => {
                let transport = http.as_ref().ok_or(ClientConnectionError::Unsupported(
                    "offline: live authority unavailable",
                ))?;
                Ok(replica.observe_workspace_sync_report(transport)?)
            }
        }
    }

    /// Unified machine-readable status for all connected frontends.
    pub fn sync_health(&self) -> Result<crate::sync_health::SyncHealth, ClientConnectionError> {
        match self {
            Self::Embedded(_) => Err(ClientConnectionError::Unsupported(
                "embedded mode has no remote replica",
            )),
            Self::Remote { replica, .. } => Ok(replica.sync_health()?),
        }
    }

    pub fn observe_sync_health(
        &mut self,
    ) -> Result<crate::sync_health::SyncHealth, ClientConnectionError> {
        match self {
            Self::Embedded(_) => Err(ClientConnectionError::Unsupported(
                "embedded mode has no remote replica",
            )),
            Self::Remote { replica, http, .. } => {
                let transport = http.as_ref().ok_or(ClientConnectionError::Unsupported(
                    "offline: live authority unavailable",
                ))?;
                Ok(replica.observe_sync_health(transport)?)
            }
        }
    }

    pub fn convergence_status(
        &self,
    ) -> Result<crate::authority::ConvergenceStatus, ClientConnectionError> {
        match self {
            Self::Embedded(_) => Err(ClientConnectionError::Unsupported(
                "embedded mode has no remote replica",
            )),
            Self::Remote { replica, .. } => Ok(replica.convergence_status()?),
        }
    }

    pub fn conflict_review(
        &self,
        id: &str,
    ) -> Result<crate::authority::ConflictReview, ClientConnectionError> {
        match self {
            Self::Embedded(_) => Err(ClientConnectionError::Unsupported(
                "no conflicts in embedded mode",
            )),
            Self::Remote { replica, .. } => Ok(replica.conflict_review(id)?),
        }
    }

    /// Refuse a stale conflict resolution when the server has advanced since
    /// the reviewer observed accepted history. Never auto-rebase user intent.
    pub fn resolve_reviewed_conflict(
        &mut self,
        review: &crate::authority::ConflictReview,
        replacement_id: &str,
        kind: CommandKind,
        action: &Value,
    ) -> Result<CommandDisposition, ClientConnectionError> {
        match self {
            Self::Embedded(_) => Err(ClientConnectionError::Unsupported(
                "no remote conflict in embedded mode",
            )),
            Self::Remote { replica, http } => {
                let transport = http.as_ref().ok_or(ClientConnectionError::Unsupported(
                    "must reconnect before applying a conflict resolution",
                ))?;
                replica.replace_reviewed_conflict_online(
                    transport,
                    review,
                    replacement_id,
                    kind,
                    action,
                )?;
                Ok(CommandDisposition::Pending)
            }
        }
    }

    /// Resolve a reviewed collection atomically in the connected replica.
    /// Individual remote commands are still dispatched and accepted in order,
    /// so callers must treat this as durable staging rather than server commit.
    pub fn resolve_reviewed_batch(
        &mut self,
        decisions: &[crate::authority::ReviewedReplacement],
    ) -> Result<usize, ClientConnectionError> {
        match self {
            Self::Embedded(_) => Err(ClientConnectionError::Unsupported(
                "embedded authority does not contain remote conflicts",
            )),
            Self::Remote { replica, http } => {
                let transport = http.as_ref().ok_or(ClientConnectionError::Unsupported(
                    "must reconnect before resolving reviewed conflicts",
                ))?;
                Ok(replica.replace_reviewed_batch_online(transport, decisions)?)
            }
        }
    }

    /// Read-only online calendar feasibility for baseline and alternatives.
    pub fn review_scenario_options(
        &self,
        request: &orbit_runtime::scenario_execution::ScenarioExecutionRequest,
    ) -> Result<orbit_runtime::scenario_execution::ScenarioCalendarOptions, ClientConnectionError>
    {
        match self {
            Self::Embedded(_) => Err(ClientConnectionError::Unsupported(
                "use the embedded authority to review local calendar options",
            )),
            Self::Remote { replica, http } => {
                let transport = http.as_ref().ok_or(ClientConnectionError::Unsupported(
                    "calendar option review requires an online authoritative server",
                ))?;
                Ok(replica.review_scenario_options_online(transport, request)?)
            }
        }
    }

    /// Online, read-only reconstruction of an accepted planning scenario as
    /// concrete calendar slots. The caller must separately approve a returned
    /// review; this method cannot enqueue or apply a schedule mutation.
    pub fn review_scenario_execution(
        &self,
        request: &orbit_runtime::scenario_execution::ScenarioExecutionRequest,
    ) -> Result<orbit_runtime::scenario_execution::ScenarioExecutionReview, ClientConnectionError>
    {
        match self {
            Self::Embedded(_) => Err(ClientConnectionError::Unsupported(
                "use the embedded authority planning review for local mode",
            )),
            Self::Remote { replica, http } => {
                let transport = http.as_ref().ok_or(ClientConnectionError::Unsupported(
                    "scenario review requires a live authoritative server",
                ))?;
                Ok(replica.review_scenario_execution_online(transport, request)?)
            }
        }
    }

    /// Execute only a separately reviewed and explicitly approved concrete
    /// schedule. Online-only in connected mode; never queued for offline replay.
    pub fn apply_approved_scenario_schedule(
        &mut self,
        request: &orbit_runtime::planning_approval::ApprovedScheduleRequest,
    ) -> Result<orbit_runtime::planning_approval::ApprovedScheduleReceipt, ClientConnectionError>
    {
        match self {
            Self::Embedded(_) => Err(ClientConnectionError::Unsupported(
                "use the embedded authoritative plan application for local mode",
            )),
            Self::Remote { replica, http } => {
                let transport = http.as_ref().ok_or(ClientConnectionError::Unsupported(
                    "approved plan requires a live authoritative server",
                ))?;
                Ok(replica.apply_approved_schedule_online(transport, request)?)
            }
        }
    }

    /// Compatibility aliases for callers that explicitly name queueing.
    /// These follow the identical transport-independent command semantics.
    pub fn queue_folder_create(
        &mut self,
        id: &str,
        command: orbit_runtime::folder_commands::FolderCreate,
    ) -> Result<CommandDisposition, ClientConnectionError> {
        self.folder_create(id, command)
    }
    pub fn queue_folder_move(
        &mut self,
        id: &str,
        command: orbit_runtime::folder_commands::FolderMove,
    ) -> Result<CommandDisposition, ClientConnectionError> {
        self.folder_move(id, command)
    }
    pub fn queue_markdown_command(
        &mut self,
        id: &str,
        command: orbit_runtime::markdown_commands::MarkdownCommand,
    ) -> Result<CommandDisposition, ClientConnectionError> {
        self.markdown_command(id, command)
    }
    pub fn markdown_unresolved(&self) -> Result<Vec<Value>, ClientConnectionError> {
        match self {
            Self::Embedded(_) => Ok(Vec::new()),
            Self::Remote { replica, .. } => Ok(replica.markdown_unresolved()?),
        }
    }
    /// Queue one blueprint mutation in the canonical ordered outbox.
    ///
    /// Connected blueprint writes have no direct transport path: the frozen
    /// route and payload are durable local intent until the same global flush
    /// that carries every other command family acknowledges them. Embedded
    /// authority applies blueprint mutations synchronously and keeps no outbox.
    pub fn queue_blueprint_transport(
        &mut self,
        id: &str,
        route: &str,
        body: &Value,
    ) -> Result<CommandDisposition, ClientConnectionError> {
        match self {
            Self::Embedded(_) => Err(ClientConnectionError::Unsupported(
                "embedded authority commits blueprint mutations immediately and keeps no outbox",
            )),
            Self::Remote { replica, .. } => {
                replica.queue_blueprint_transport(id, route, body)?;
                Ok(CommandDisposition::Pending)
            }
        }
    }
    /// Unresolved blueprint intentions, in global queue order. An entry here is
    /// durable local intent only, never evidence of server acceptance.
    pub fn blueprint_pending(&self) -> Result<Value, ClientConnectionError> {
        match self {
            Self::Embedded(_) => Ok(json!({"accepted":false,"queued":0,"commands":[]})),
            Self::Remote { replica, .. } => Ok(replica.blueprint_pending()?),
        }
    }
    pub fn flush_markdown(&mut self) -> Result<FlushOutcome, ClientConnectionError> {
        match self {
            Self::Embedded(_) => Ok(FlushOutcome::Empty),
            Self::Remote { replica, http } => {
                let transport = http.as_ref().ok_or(ClientConnectionError::Unsupported(
                    "offline: no remote credentials",
                ))?;
                Ok(replica.flush_markdown(transport)?)
            }
        }
    }

    /// Embedded reads are canonical. Remote reads require valid credentials and
    /// fetch accepted server state; knowledge is not included in the six-family snapshot.
    /// Folder navigation mirrors the host filesystem, including empty folders.
    /// Remote navigation is live; offline replica folder caches remain future work.
    pub fn knowledge_tree(
        &self,
    ) -> Result<orbit_runtime::knowledge_files::KnowledgeNode, ClientConnectionError> {
        match self {
            Self::Embedded(host) => host.knowledge_tree().map_err(ClientConnectionError::Vault),
            Self::Remote { http, .. } => {
                let transport = http.as_ref().ok_or(ClientConnectionError::Unsupported(
                    "offline folder tree requires a server connection",
                ))?;
                Ok(serde_json::from_value(
                    transport.get("/v1/authoritative/knowledge-tree")?,
                )?)
            }
        }
    }

    /// Content for a selected filesystem-tree note, from a real .md file.
    /// This deliberately does not read legacy SQL-stored knowledge bodies.
    /// Remote access requires an authenticated connection; no stale cache is substituted.
    pub fn markdown_document(&self, path: &str) -> Result<String, ClientConnectionError> {
        match self {
            Self::Embedded(host) => host
                .markdown_document(path)
                .map_err(ClientConnectionError::Vault),
            Self::Remote { http, .. } => {
                let transport = http.as_ref().ok_or(ClientConnectionError::Unsupported(
                    "offline Markdown file reads are not cached",
                ))?;
                // Encode the entire relative path as query data rather than URL structure.
                let mut encoded = String::new();
                for byte in path.bytes() {
                    if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
                        encoded.push(char::from(byte));
                    } else {
                        use std::fmt::Write;
                        write!(&mut encoded, "%{byte:02X}").expect("write into String");
                    }
                }
                let payload = transport.get(&format!(
                    "/v1/authoritative/markdown-document?path={encoded}"
                ))?;
                payload
                    .get("body")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
                    .ok_or(ClientConnectionError::Unsupported(
                        "invalid Markdown file response",
                    ))
            }
        }
    }

    /// Read a Markdown note by immutable ID, regardless of folder moves.
    /// Remote reads are accepted server state; offline caches are never silently substituted.
    pub fn markdown_by_id(
        &self,
        id: &str,
    ) -> Result<
        Option<(
            orbit_runtime::knowledge_service::KnowledgeDocumentInfo,
            String,
        )>,
        ClientConnectionError,
    > {
        match self {
            Self::Embedded(host) => host
                .markdown_by_id(id)
                .map_err(|_| ClientConnectionError::Unsupported("invalid Markdown identity")),
            Self::Remote { http, .. } => {
                let transport = http.as_ref().ok_or(ClientConnectionError::Unsupported(
                    "offline Markdown reads are not cached",
                ))?;
                let response = transport.get(&format!(
                    "/v1/authoritative/markdown-by-id?id={}",
                    encode_query(id)
                ))?;
                let note = serde_json::from_value(
                    response
                        .get("note")
                        .cloned()
                        .ok_or(ClientConnectionError::Unsupported("missing note"))?,
                )?;
                let body = response
                    .get("body")
                    .and_then(Value::as_str)
                    .ok_or(ClientConnectionError::Unsupported("missing body"))?
                    .to_owned();
                Ok(Some((note, body)))
            }
        }
    }

    /// Explicitly use the last accepted Markdown copy for offline display.
    /// Returning cached data is not a promise that it is currently fresh.
    pub fn cached_markdown_by_id(
        &self,
        id: &str,
    ) -> Result<
        Option<(
            orbit_runtime::knowledge_service::KnowledgeDocumentInfo,
            String,
        )>,
        ClientConnectionError,
    > {
        match self {
            Self::Embedded(host) => host
                .markdown_by_id(id)
                .map_err(|_| ClientConnectionError::Unsupported("invalid Markdown ID")),
            Self::Remote { replica, .. } => Ok(replica.cached_markdown(id)?),
        }
    }

    /// Search the locally cached accepted Markdown mirror. Results may be
    /// stale and never include pending outbox edits.
    pub fn search_cached_markdown(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<orbit_runtime::knowledge_service::KnowledgeDocumentInfo>, ClientConnectionError>
    {
        match self {
            Self::Embedded(_) => Err(ClientConnectionError::Unsupported(
                "embedded mode reads canonical Markdown directly; there is no mirror cache",
            )),
            Self::Remote { replica, .. } => Ok(replica.search_cached_markdown(query, limit)?),
        }
    }

    /// Every note in the locally cached accepted Markdown mirror. Completeness
    /// is reported separately by [`Self::markdown_mirror_status`].
    pub fn cached_markdown_inventory(
        &self,
        limit: usize,
    ) -> Result<Vec<orbit_runtime::knowledge_service::KnowledgeDocumentInfo>, ClientConnectionError>
    {
        match self {
            Self::Embedded(_) => Err(ClientConnectionError::Unsupported(
                "embedded mode reads canonical Markdown directly; there is no mirror cache",
            )),
            Self::Remote { replica, .. } => Ok(replica.cached_markdown_inventory(limit)?),
        }
    }

    /// Fetch the authoritative document and persist it for disconnected reads.
    /// The server remains authoritative; pending outbox edits are not projected.
    pub fn refresh_markdown_by_id(
        &mut self,
        id: &str,
    ) -> Result<
        (
            orbit_runtime::knowledge_service::KnowledgeDocumentInfo,
            String,
        ),
        ClientConnectionError,
    > {
        match self {
            Self::Embedded(host) => host
                .markdown_by_id(id)
                .map_err(|_| ClientConnectionError::Unsupported("invalid Markdown ID"))?
                .ok_or(ClientConnectionError::Unsupported(
                    "Markdown note not found",
                )),
            Self::Remote { replica, http } => {
                let transport = http.as_ref().ok_or(ClientConnectionError::Unsupported(
                    "offline: cannot refresh Markdown",
                ))?;
                Ok(replica.refresh_markdown(id, transport)?)
            }
        }
    }

    /// Inspect offline Markdown mirror provenance without contacting the
    /// server or mistaking an individually cached note for a complete vault.
    pub fn markdown_mirror_status(
        &self,
    ) -> Result<crate::authority::MarkdownMirrorStatus, ClientConnectionError> {
        match self {
            Self::Embedded(_) => Err(ClientConnectionError::Unsupported(
                "offline mirror only applies to connected clients",
            )),
            Self::Remote { replica, .. } => Ok(replica.markdown_mirror_status()?),
        }
    }

    /// Populate the connected offline Markdown mirror from all server-accepted
    /// note identities and bodies. No partial download is published locally.
    pub fn sync_markdown_mirror(&mut self) -> Result<usize, ClientConnectionError> {
        match self {
            Self::Embedded(_) => Err(ClientConnectionError::Unsupported(
                "offline mirror only applies to connected clients",
            )),
            Self::Remote { replica, http } => {
                let transport = http.as_ref().ok_or(ClientConnectionError::Unsupported(
                    "offline: cannot synchronize Markdown",
                ))?;
                Ok(replica.sync_markdown_mirror(transport)?)
            }
        }
    }

    /// Return exact reused/downloaded counts for a complete accepted mirror
    /// refresh. Offline/pending commands never contribute to these totals.
    pub fn sync_markdown_mirror_detailed(
        &mut self,
    ) -> Result<crate::authority::MarkdownMirrorSyncReport, ClientConnectionError> {
        match self {
            Self::Embedded(_) => Err(ClientConnectionError::Unsupported(
                "offline mirror only applies to connected clients",
            )),
            Self::Remote { replica, http } => {
                let transport = http.as_ref().ok_or(ClientConnectionError::Unsupported(
                    "offline: cannot synchronize Markdown",
                ))?;
                Ok(replica.sync_markdown_mirror_detailed(transport)?)
            }
        }
    }

    /// Search actual Markdown files; never the retired SQL body table.
    pub fn markdown_search(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<orbit_runtime::knowledge_service::KnowledgeDocumentInfo>, ClientConnectionError>
    {
        match self {
            Self::Embedded(host) => host
                .markdown_search(query, limit)
                .map_err(|_| ClientConnectionError::Unsupported("invalid Markdown search")),
            Self::Remote { http, .. } => {
                let transport = http.as_ref().ok_or(ClientConnectionError::Unsupported(
                    "offline Markdown search is not cached",
                ))?;
                Ok(serde_json::from_value(transport.post(
                    "/v1/authoritative/markdown-search",
                    &json!({"query":query,"limit":limit}),
                )?)?)
            }
        }
    }

    /// Forward links and backlinks use immutable IDs and survive folder moves.
    pub fn markdown_relations(
        &self,
        id: &str,
    ) -> Result<
        Option<orbit_runtime::knowledge_service::KnowledgeFileRelations>,
        ClientConnectionError,
    > {
        match self {
            Self::Embedded(host) => host
                .markdown_relations(id)
                .map_err(|_| ClientConnectionError::Unsupported("invalid Markdown relations")),
            Self::Remote { http, .. } => {
                let transport = http.as_ref().ok_or(ClientConnectionError::Unsupported(
                    "offline Markdown relations are not cached",
                ))?;
                Ok(Some(serde_json::from_value(transport.get(&format!(
                    "/v1/authoritative/markdown-relations?id={}",
                    encode_query(id)
                ))?)?))
            }
        }
    }

    pub fn refresh(&mut self) -> Result<Value, ClientConnectionError> {
        match self {
            Self::Embedded(_) => self.workspace(),
            Self::Remote { replica, http } => {
                let transport = http.as_ref().ok_or(ClientConnectionError::Unsupported(
                    "offline: no remote credentials",
                ))?;
                replica.refresh(transport)?;
                Ok(serde_json::to_value(replica.workspace()?)?)
            }
        }
    }

    /// Reconnect a remote client with ordered replay, accepted refresh, and
    /// offline Markdown mirror update. Local-only clients have no remote peer.
    pub fn reconnect(
        &mut self,
    ) -> Result<crate::authority::ReconnectReport, ClientConnectionError> {
        match self {
            Self::Embedded(_) => Err(ClientConnectionError::Unsupported(
                "reconnect is only available in connected mode",
            )),
            Self::Remote { replica, http } => {
                let transport = http.as_ref().ok_or(ClientConnectionError::Unsupported(
                    "offline: no remote credentials",
                ))?;
                Ok(replica.reconnect(transport)?)
            }
        }
    }

    pub fn flush(&mut self) -> Result<FlushOutcome, ClientConnectionError> {
        match self {
            Self::Embedded(_) => Ok(FlushOutcome::Empty),
            Self::Remote { replica, http } => {
                let transport = http.as_ref().ok_or(ClientConnectionError::Unsupported(
                    "offline: no remote credentials",
                ))?;
                Ok(replica.flush(transport)?)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use orbit_runtime::domain_commands::{TaskAction, TaskCommand};
    #[test]
    fn file_authoritative_unified_create_and_edit() {
        use orbit_runtime::markdown_commands::MarkdownCommand;
        let mut client = AuthorityConnection::embedded(":memory:", "owner").unwrap();
        let create = MarkdownCommand::Create {
            note_id: "note-137".into(),
            path: "first.md".into(),
            body: "# First".into(),
        };
        let receipt = client
            .markdown_command("create-137", create.clone())
            .unwrap();
        assert_eq!(
            receipt,
            CommandDisposition::AcceptedMarkdown { revision: 1 }
        );
        assert_eq!(
            client.markdown_command("create-137", create).unwrap(),
            CommandDisposition::AcceptedMarkdown { revision: 1 }
        );
        let expected = "---\norbit_id: note-137\n---\n# First";
        let updated = "---\norbit_id: note-137\n---\n# Updated";
        let receipt = client
            .markdown_command(
                "edit-137",
                MarkdownCommand::Update {
                    note_id: "note-137".into(),
                    expected: expected.into(),
                    replacement: updated.into(),
                },
            )
            .unwrap();
        assert_eq!(
            receipt,
            CommandDisposition::AcceptedMarkdown { revision: 2 }
        );
        assert_eq!(client.markdown_document("first.md").unwrap(), updated);
    }

    #[test]
    fn remote_file_commands_use_one_ordered_outbox_and_never_commit_offline() {
        use orbit_runtime::{
            folder_commands::{FolderCreate, FolderMove},
            markdown_commands::MarkdownCommand,
        };
        let mut remote =
            AuthorityConnection::remote(":memory:", "https://example.org", None).unwrap();
        assert_eq!(
            remote
                .folder_create(
                    "mkdir",
                    FolderCreate {
                        path: "Work".into()
                    }
                )
                .unwrap(),
            CommandDisposition::Pending
        );
        assert_eq!(
            remote
                .markdown_command(
                    "create",
                    MarkdownCommand::Create {
                        note_id: "note-one".into(),
                        path: "Work/one.md".into(),
                        body: "# One".into()
                    }
                )
                .unwrap(),
            CommandDisposition::Pending
        );
        assert_eq!(
            remote
                .folder_move(
                    "move",
                    FolderMove {
                        from: "Work".into(),
                        to: "Projects".into()
                    }
                )
                .unwrap(),
            CommandDisposition::Pending
        );
        assert_eq!(remote.workspace().unwrap()["pending_count"], 3);
        let unresolved = match &remote {
            AuthorityConnection::Remote { replica, .. } => replica.unresolved().unwrap(),
            _ => unreachable!(),
        };
        assert_eq!(
            unresolved
                .iter()
                .map(|row| row["command_id"].as_str().unwrap())
                .collect::<Vec<_>>(),
            vec!["mkdir", "create", "move"]
        );
        assert!(remote.flush().is_err());
        assert_eq!(remote.workspace().unwrap()["pending_count"], 3);
    }

    #[test]
    fn remote_unified_commands_are_idempotent_before_and_after_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("remote.db");
        let command = ProjectCommand {
            workflow_id: None,
            command_id: "same-project".into(),
            expected_revision: 0,
            project_id: "project-x".into(),
            name: "Example".into(),
        };
        {
            let mut client =
                AuthorityConnection::remote(&path, "https://example.org", None).unwrap();
            assert_eq!(
                client.create_project(command.clone()).unwrap(),
                CommandDisposition::Pending
            );
            assert_eq!(
                client.create_project(command.clone()).unwrap(),
                CommandDisposition::Pending
            );
            assert_eq!(client.workspace().unwrap()["pending_count"], 1);
            assert_eq!(
                client.command_status("same-project").unwrap().unwrap()["state"],
                "pending"
            );
        }
        let mut reopened = AuthorityConnection::remote(&path, "https://example.org", None).unwrap();
        assert_eq!(
            reopened.create_project(command.clone()).unwrap(),
            CommandDisposition::Pending
        );
        assert_eq!(reopened.workspace().unwrap()["pending_count"], 1);
        let changed = ProjectCommand {
            workflow_id: None,
            name: "Changed".into(),
            ..command
        };
        assert!(reopened.create_project(changed).is_err());
        assert!(reopened.accepted_receipt("same-project").unwrap().is_none());
    }

    #[test]
    fn accepted_read_contract_is_the_same_for_embedded_and_refreshed_remote() {
        let mut local = AuthorityConnection::embedded(":memory:", "owner").unwrap();
        assert_eq!(local.projects().unwrap().len(), 0);
        assert_eq!(local.habits().unwrap().len(), 0);
        let cmd = ProjectCommand {
            workflow_id: None,
            command_id: "accept-read-p".into(),
            expected_revision: 0,
            project_id: "read-project".into(),
            name: "Read contract".into(),
        };
        local.create_project(cmd).unwrap();
        let snapshot = local.accepted_snapshot().unwrap().unwrap();
        assert_eq!(
            local.projects().unwrap(),
            snapshot["projects"].as_array().unwrap().clone()
        );
        assert!(local.project("read-project").unwrap().is_some());
        assert!(local.project("missing-project").unwrap().is_none());
        assert_eq!(
            local.history_prefix(1).unwrap(),
            snapshot["history_fingerprint"].as_str().unwrap()
        );

        let mut remote =
            AuthorityConnection::remote(":memory:", "https://example.org", None).unwrap();
        assert!(remote.accepted_snapshot().unwrap().is_none());
        assert!(remote.projects().unwrap().is_empty());
        match &mut remote {
            AuthorityConnection::Remote { replica, .. } => {
                replica.accept_snapshot_for_testing(&snapshot).unwrap();
            }
            _ => unreachable!(),
        }
        assert_eq!(remote.accepted_snapshot().unwrap(), Some(snapshot.clone()));
        assert_eq!(remote.projects().unwrap(), local.projects().unwrap());
        assert_eq!(
            remote.history_prefix(1).unwrap(),
            local.history_prefix(1).unwrap()
        );
        assert!(remote.history_prefix(0).is_err());
    }

    #[test]
    fn task_filters_have_embedded_remote_parity_and_exclude_pending() {
        let mut local = AuthorityConnection::embedded(":memory:", "owner").unwrap();
        local
            .create_project(ProjectCommand {
                workflow_id: None,
                command_id: "filtered-p".into(),
                expected_revision: 0,
                project_id: "p".into(),
                name: "Project".into(),
            })
            .unwrap();
        local
            .task_command(TaskCommand {
                command_id: "filtered-t1".into(),
                expected_revision: 1,
                action: TaskAction::Create {
                    task_id: "assigned".into(),
                    title: "Assigned".into(),
                    project_id: Some("p".into()),
                    milestone_id: None,
                    description: String::new(),
                    status: None,
                    priority: None,
                    estimated_minutes: None,
                    deadline_at_ms: None,
                    earliest_start_at_ms: None,
                    acceptance_criteria: Vec::new(),
                    prerequisite_ids: Vec::new(),
                },
            })
            .unwrap();
        local
            .task_command(TaskCommand {
                command_id: "filtered-t2".into(),
                expected_revision: 2,
                action: TaskAction::Create {
                    task_id: "unassigned".into(),
                    title: "Unassigned".into(),
                    project_id: None,
                    milestone_id: None,
                    description: String::new(),
                    status: None,
                    priority: None,
                    estimated_minutes: None,
                    deadline_at_ms: None,
                    earliest_start_at_ms: None,
                    acceptance_criteria: Vec::new(),
                    prerequisite_ids: Vec::new(),
                },
            })
            .unwrap();
        assert_eq!(local.tasks_for_project(Some("p")).unwrap().len(), 1);
        assert_eq!(local.tasks_for_project(None).unwrap().len(), 1);
        assert!(local.tasks_for_project(Some(" ")).is_err());
        assert!(local.habit_occurrences_for("").is_err());
        assert!(local.tasks_with_status("").is_err());
        let snapshot = local.accepted_snapshot().unwrap().unwrap();
        let mut remote =
            AuthorityConnection::remote(":memory:", "https://example.org", None).unwrap();
        if let AuthorityConnection::Remote { replica, .. } = &mut remote {
            replica.accept_snapshot_for_testing(&snapshot).unwrap();
        }
        assert_eq!(
            local.tasks_for_project(Some("p")).unwrap(),
            remote.tasks_for_project(Some("p")).unwrap()
        );
        assert_eq!(
            local.tasks_for_project(None).unwrap(),
            remote.tasks_for_project(None).unwrap()
        );
        remote
            .task_command(TaskCommand {
                command_id: "pending-t".into(),
                expected_revision: 3,
                action: TaskAction::Create {
                    task_id: "pending".into(),
                    title: "Pending".into(),
                    project_id: Some("p".into()),
                    milestone_id: None,
                    description: String::new(),
                    status: None,
                    priority: None,
                    estimated_minutes: None,
                    deadline_at_ms: None,
                    earliest_start_at_ms: None,
                    acceptance_criteria: Vec::new(),
                    prerequisite_ids: Vec::new(),
                },
            })
            .unwrap();
        assert_eq!(remote.tasks_for_project(Some("p")).unwrap().len(), 1);
        assert_eq!(remote.workspace().unwrap()["pending_count"], 1);
    }

    #[test]
    fn dashboard_is_one_accepted_revision_and_pending_is_separate() {
        let mut embedded = AuthorityConnection::embedded(":memory:", "owner").unwrap();
        embedded
            .create_project(ProjectCommand {
                workflow_id: None,
                command_id: "summary-project".into(),
                expected_revision: 0,
                project_id: "summary".into(),
                name: "Summary".into(),
            })
            .unwrap();
        embedded
            .task_command(TaskCommand {
                command_id: "summary-task".into(),
                expected_revision: 1,
                action: TaskAction::Create {
                    task_id: "t".into(),
                    title: "Task".into(),
                    project_id: None,
                    milestone_id: None,
                    description: String::new(),
                    status: None,
                    priority: None,
                    estimated_minutes: None,
                    deadline_at_ms: None,
                    earliest_start_at_ms: None,
                    acceptance_criteria: Vec::new(),
                    prerequisite_ids: Vec::new(),
                },
            })
            .unwrap();
        let snapshot = embedded.accepted_snapshot().unwrap().unwrap();
        let embedded_summary = embedded.dashboard_summary().unwrap().unwrap();
        assert_eq!(embedded_summary["revision"], 2);
        assert_eq!(embedded_summary["project_count"], 1);
        assert_eq!(embedded_summary["task_count"], 1);
        assert_eq!(embedded_summary["unassigned_task_count"], 1);
        assert_eq!(embedded_summary["pending_count"], 0);
        let mut remote =
            AuthorityConnection::remote(":memory:", "https://example.org", None).unwrap();
        assert!(remote.dashboard_summary().unwrap().is_none());
        if let AuthorityConnection::Remote { replica, .. } = &mut remote {
            replica.accept_snapshot_for_testing(&snapshot).unwrap();
        }
        assert_eq!(
            remote.dashboard_summary().unwrap(),
            Some(embedded_summary.clone())
        );
        remote
            .task_command(TaskCommand {
                command_id: "summary-pending".into(),
                expected_revision: 2,
                action: TaskAction::Create {
                    task_id: "pending".into(),
                    title: "Pending".into(),
                    project_id: None,
                    milestone_id: None,
                    description: String::new(),
                    status: None,
                    priority: None,
                    estimated_minutes: None,
                    deadline_at_ms: None,
                    earliest_start_at_ms: None,
                    acceptance_criteria: Vec::new(),
                    prerequisite_ids: Vec::new(),
                },
            })
            .unwrap();
        let summary = remote.dashboard_summary().unwrap().unwrap();
        assert_eq!(summary["pending_count"], 1);
        assert_eq!(summary["task_count"], 1);
        assert_eq!(summary["revision"], embedded_summary["revision"]);
        assert_eq!(
            summary["history_fingerprint"],
            embedded_summary["history_fingerprint"]
        );
    }

    #[test]
    fn project_overview_and_status_counts_are_snapshot_consistent() {
        let mut embedded = AuthorityConnection::embedded(":memory:", "owner").unwrap();
        embedded
            .create_project(ProjectCommand {
                workflow_id: None,
                command_id: "dashboard-p".into(),
                expected_revision: 0,
                project_id: "dashboard".into(),
                name: "Dashboard".into(),
            })
            .unwrap();
        embedded
            .task_command(TaskCommand {
                command_id: "dashboard-task".into(),
                expected_revision: 1,
                action: TaskAction::Create {
                    task_id: "child".into(),
                    title: "Child task".into(),
                    project_id: Some("dashboard".into()),
                    milestone_id: None,
                    description: String::new(),
                    status: None,
                    priority: None,
                    estimated_minutes: None,
                    deadline_at_ms: None,
                    earliest_start_at_ms: None,
                    acceptance_criteria: Vec::new(),
                    prerequisite_ids: Vec::new(),
                },
            })
            .unwrap();
        let overview = embedded.project_overview("dashboard").unwrap().unwrap();
        assert_eq!(overview["tasks"].as_array().unwrap().len(), 1);
        assert_eq!(overview["project"]["id"], "dashboard");
        assert_eq!(overview["revision"], 2);
        assert_eq!(
            embedded
                .task_status_counts(Some("dashboard"))
                .unwrap()
                .values()
                .sum::<usize>(),
            1
        );
        assert!(embedded.project_overview("missing").unwrap().is_none());
        assert!(embedded.project_overview(" ").is_err());
        let snapshot = embedded.accepted_snapshot().unwrap().unwrap();
        let mut remote =
            AuthorityConnection::remote(":memory:", "https://example.org", None).unwrap();
        if let AuthorityConnection::Remote { replica, .. } = &mut remote {
            replica.accept_snapshot_for_testing(&snapshot).unwrap();
        }
        assert_eq!(
            remote.project_overview("dashboard").unwrap(),
            Some(overview.clone())
        );
        assert_eq!(
            remote.task_status_counts(Some("dashboard")).unwrap(),
            embedded.task_status_counts(Some("dashboard")).unwrap()
        );
        remote
            .task_command(TaskCommand {
                command_id: "pending-child".into(),
                expected_revision: 2,
                action: TaskAction::Create {
                    task_id: "pending-child".into(),
                    title: "Pending".into(),
                    project_id: Some("dashboard".into()),
                    milestone_id: None,
                    description: String::new(),
                    status: None,
                    priority: None,
                    estimated_minutes: None,
                    deadline_at_ms: None,
                    earliest_start_at_ms: None,
                    acceptance_criteria: Vec::new(),
                    prerequisite_ids: Vec::new(),
                },
            })
            .unwrap();
        assert_eq!(
            remote.project_overview("dashboard").unwrap(),
            Some(overview)
        );
        assert_eq!(
            remote
                .task_status_counts(Some("dashboard"))
                .unwrap()
                .values()
                .sum::<usize>(),
            1
        );
        assert!(remote.metric_history(" ").is_err());
        assert!(remote.metric_history("not-created").unwrap().is_empty());
    }

    #[test]
    fn queued_writes_are_never_exposed_as_accepted_reads() {
        let mut remote =
            AuthorityConnection::remote(":memory:", "https://example.org", None).unwrap();
        remote
            .create_project(ProjectCommand {
                workflow_id: None,
                command_id: "pending-read".into(),
                expected_revision: 0,
                project_id: "p".into(),
                name: "Pending only".into(),
            })
            .unwrap();
        assert!(remote.projects().unwrap().is_empty());
        assert!(remote.project("p").unwrap().is_none());
        assert!(remote.accepted_snapshot().unwrap().is_none());
        assert_eq!(remote.workspace().unwrap()["pending_count"], 1);
    }

    #[test]
    fn embedded_and_remote_share_commands_but_not_acceptance_semantics() {
        let mut local = AuthorityConnection::embedded(":memory:", "owner").unwrap();
        let command = ProjectCommand {
            workflow_id: None,
            command_id: "p1".into(),
            expected_revision: 0,
            project_id: "project-1".into(),
            name: "Personal".into(),
        };
        assert_eq!(
            local.create_project(command.clone()).unwrap(),
            CommandDisposition::Accepted {
                revision: 1,
                duplicate: false
            }
        );
        assert_eq!(
            local.create_project(command.clone()).unwrap(),
            CommandDisposition::Accepted {
                revision: 1,
                duplicate: true
            }
        );
        assert_eq!(
            local.workspace().unwrap()["projects"]
                .as_array()
                .unwrap()
                .len(),
            1
        );

        let mut remote =
            AuthorityConnection::remote(":memory:", "https://example.org", None).unwrap();
        assert_eq!(
            remote.create_project(command).unwrap(),
            CommandDisposition::Pending
        );
        assert_eq!(remote.workspace().unwrap()["revision"], Value::Null);
        assert_eq!(
            remote.workspace().unwrap()["projects"]
                .as_array()
                .unwrap()
                .len(),
            0
        );
        assert_eq!(remote.workspace().unwrap()["pending_count"], 1);
        let result = remote
            .task_command(TaskCommand {
                command_id: "t1".into(),
                expected_revision: 1,
                action: TaskAction::Create {
                    task_id: "task-1".into(),
                    title: "Task".into(),
                    project_id: Some("project-1".into()),
                    milestone_id: None,
                    description: String::new(),
                    status: None,
                    priority: None,
                    estimated_minutes: None,
                    deadline_at_ms: None,
                    earliest_start_at_ms: None,
                    acceptance_criteria: Vec::new(),
                    prerequisite_ids: Vec::new(),
                },
            })
            .unwrap();
        assert_eq!(result, CommandDisposition::Pending);
        assert_eq!(remote.workspace().unwrap()["pending_count"], 2);
        assert!(remote.flush().is_err());
    }
}

#[cfg(test)]
mod folder_client_parity_tests {
    use super::*;
    use orbit_runtime::folder_commands::{FolderCreate, FolderMove};
    #[test]
    fn embedded_creates_and_moves_real_folders_remote_only_queues() {
        let mut local = AuthorityConnection::embedded(":memory:", "owner").unwrap();
        assert_eq!(
            local
                .queue_folder_create(
                    "mkdir-work",
                    FolderCreate {
                        path: "Work".into()
                    }
                )
                .unwrap(),
            CommandDisposition::AcceptedFolder
        );
        assert_eq!(
            local
                .queue_folder_create(
                    "mkdir-work",
                    FolderCreate {
                        path: "Work".into()
                    }
                )
                .unwrap(),
            CommandDisposition::AcceptedFolder
        );
        assert_eq!(
            local
                .queue_folder_move(
                    "rename-work",
                    FolderMove {
                        from: "Work".into(),
                        to: "Projects".into()
                    }
                )
                .unwrap(),
            CommandDisposition::AcceptedFolder
        );
        let tree = serde_json::to_value(local.knowledge_tree().unwrap()).unwrap();
        assert!(tree.to_string().contains("Projects"));
        assert!(!tree.to_string().contains("\"name\":\"Work\""));

        let mut remote =
            AuthorityConnection::remote(":memory:", "https://example.org", None).unwrap();
        assert_eq!(
            remote
                .queue_folder_create(
                    "mkdir-work",
                    FolderCreate {
                        path: "Work".into()
                    }
                )
                .unwrap(),
            CommandDisposition::Pending
        );
        assert_eq!(
            remote
                .queue_folder_move(
                    "rename-work",
                    FolderMove {
                        from: "Work".into(),
                        to: "Projects".into()
                    }
                )
                .unwrap(),
            CommandDisposition::Pending
        );
        assert_eq!(remote.workspace().unwrap()["pending_count"], 2);
        assert!(remote.flush().is_err());
    }
}

#[cfg(test)]
mod conflict_workflow_tests {
    use super::*;

    #[test]
    fn unresolved_commands_are_visible_but_never_accepted() {
        let mut remote =
            AuthorityConnection::remote(":memory:", "https://example.org", None).unwrap();
        let command = TaskCommand {
            command_id: "create-1".into(),
            expected_revision: 0,
            action: orbit_runtime::domain_commands::TaskAction::Create {
                task_id: "task-1".into(),
                title: "Write plan".into(),
                project_id: None,
                milestone_id: None,
                description: String::new(),
                status: None,
                priority: None,
                estimated_minutes: None,
                deadline_at_ms: None,
                earliest_start_at_ms: None,
                acceptance_criteria: Vec::new(),
                prerequisite_ids: Vec::new(),
            },
        };
        assert_eq!(
            remote.task_command(command).unwrap(),
            CommandDisposition::Pending
        );
        let unresolved = remote.unresolved_commands().unwrap();
        assert_eq!(unresolved.len(), 1);
        assert_eq!(unresolved[0]["command_id"], "create-1");
        assert_eq!(unresolved[0]["state"], "pending");
        assert!(remote.tasks().unwrap().is_empty());
        assert!(remote.conflict_review("create-1").is_err());
        assert_eq!(remote.unresolved_commands().unwrap().len(), 1);
        let embedded = AuthorityConnection::embedded(":memory:", "owner").unwrap();
        assert!(embedded.unresolved_commands().unwrap().is_empty());
    }
}

#[cfg(test)]
mod typed_workflow_tests {
    use super::*;
    use orbit_runtime::domain_commands::TaskAction;

    fn project(id: &str) -> DomainWorkflowStep {
        DomainWorkflowStep::Project(ProjectCommand {
            workflow_id: None,
            command_id: id.into(),
            expected_revision: 0,
            project_id: "p1".into(),
            name: "Demo".into(),
        })
    }

    #[test]
    fn preflight_duplicate_ids_has_no_side_effects() {
        let mut host = AuthorityConnection::embedded(":memory:", "owner").unwrap();
        let failure = host
            .execute_workflow(vec![project("same"), project("same")])
            .unwrap_err();
        assert_eq!(failure.failed_index, 1);
        assert!(failure.completed.is_empty());
        assert!(host.projects().unwrap().is_empty());
        let mut remote =
            AuthorityConnection::remote(":memory:", "https://example.org", None).unwrap();
        assert!(
            remote
                .execute_workflow(vec![project(""), project("other")])
                .is_err()
        );
        assert_eq!(remote.workspace().unwrap()["pending_count"], 0);
    }

    #[test]
    fn embedded_workflow_commits_in_order() {
        let mut host = AuthorityConnection::embedded(":memory:", "owner").unwrap();
        let results = host
            .execute_workflow(vec![
                project("create-project"),
                DomainWorkflowStep::Task(TaskCommand {
                    command_id: "create-task".into(),
                    expected_revision: 1,
                    action: TaskAction::Create {
                        task_id: "t1".into(),
                        title: "Plan".into(),
                        project_id: Some("p1".into()),
                        milestone_id: None,
                        description: String::new(),
                        status: None,
                        priority: None,
                        estimated_minutes: None,
                        deadline_at_ms: None,
                        earliest_start_at_ms: None,
                        acceptance_criteria: Vec::new(),
                        prerequisite_ids: Vec::new(),
                    },
                }),
            ])
            .unwrap();
        assert_eq!(
            results,
            vec![
                CommandDisposition::Accepted {
                    revision: 1,
                    duplicate: false
                },
                CommandDisposition::Accepted {
                    revision: 2,
                    duplicate: false
                },
            ]
        );
        assert_eq!(host.tasks_for_project(Some("p1")).unwrap().len(), 1);
    }

    #[test]
    fn remote_workflow_queues_without_projecting_pending_writes() {
        let mut remote =
            AuthorityConnection::remote(":memory:", "https://example.org", None).unwrap();
        let result = remote
            .execute_workflow(vec![
                project("create-project"),
                DomainWorkflowStep::Task(TaskCommand {
                    command_id: "create-task".into(),
                    expected_revision: 1,
                    action: TaskAction::Create {
                        task_id: "t1".into(),
                        title: "Plan".into(),
                        project_id: Some("p1".into()),
                        milestone_id: None,
                        description: String::new(),
                        status: None,
                        priority: None,
                        estimated_minutes: None,
                        deadline_at_ms: None,
                        earliest_start_at_ms: None,
                        acceptance_criteria: Vec::new(),
                        prerequisite_ids: Vec::new(),
                    },
                }),
            ])
            .unwrap();
        assert_eq!(
            result,
            vec![CommandDisposition::Pending, CommandDisposition::Pending]
        );
        assert_eq!(remote.workspace().unwrap()["pending_count"], 2);
        assert!(remote.projects().unwrap().is_empty());
        assert!(remote.tasks().unwrap().is_empty());
    }

    #[test]
    fn failure_reports_accepted_prefix_without_rollback() {
        let mut host = AuthorityConnection::embedded(":memory:", "owner").unwrap();
        let error = host
            .execute_workflow(vec![
                project("create-project"),
                DomainWorkflowStep::Task(TaskCommand {
                    command_id: "bad-task".into(),
                    expected_revision: 0,
                    action: TaskAction::Create {
                        task_id: "t1".into(),
                        title: "Plan".into(),
                        project_id: Some("p1".into()),
                        milestone_id: None,
                        description: String::new(),
                        status: None,
                        priority: None,
                        estimated_minutes: None,
                        deadline_at_ms: None,
                        earliest_start_at_ms: None,
                        acceptance_criteria: Vec::new(),
                        prerequisite_ids: Vec::new(),
                    },
                }),
            ])
            .unwrap_err();
        assert_eq!(error.failed_index, 1);
        assert_eq!(error.completed.len(), 1);
        assert_eq!(host.projects().unwrap().len(), 1);
    }
}

#[cfg(test)]
mod project_bootstrap_tests {
    use super::*;
    fn manifest() -> ProjectBootstrap {
        ProjectBootstrap {
            command_prefix: "launch".into(),
            base_revision: 0,
            project_id: "roadmap".into(),
            project_name: "Roadmap".into(),
            tasks: vec![
                BootstrapTask {
                    task_id: "design".into(),
                    title: "Design".into(),
                },
                BootstrapTask {
                    task_id: "ship".into(),
                    title: "Ship".into(),
                },
            ],
        }
    }
    #[test]
    fn project_launch_commits_complete_project_in_revision_order() {
        let mut host = AuthorityConnection::embedded(":memory:", "owner").unwrap();
        let receipts = host.bootstrap_project(manifest()).unwrap();
        assert_eq!(
            receipts,
            vec![
                CommandDisposition::Accepted {
                    revision: 1,
                    duplicate: false
                },
                CommandDisposition::Accepted {
                    revision: 2,
                    duplicate: false
                },
                CommandDisposition::Accepted {
                    revision: 3,
                    duplicate: false
                },
            ]
        );
        assert_eq!(host.projects().unwrap().len(), 1);
        assert_eq!(host.tasks_for_project(Some("roadmap")).unwrap().len(), 2);
        assert_eq!(host.dashboard_summary().unwrap().unwrap()["revision"], 3);
        let retry = host.bootstrap_project(manifest()).unwrap();
        assert!(retry.iter().all(|r| matches!(
            r,
            CommandDisposition::Accepted {
                duplicate: true,
                ..
            }
        )));
        assert_eq!(host.dashboard_summary().unwrap().unwrap()["revision"], 3);
    }
    #[test]
    fn invalid_manifest_has_no_embedded_or_remote_side_effects() {
        let mut bad = manifest();
        bad.tasks[1].task_id = "design".into();
        let mut host = AuthorityConnection::embedded(":memory:", "owner").unwrap();
        assert!(host.bootstrap_project(bad.clone()).is_err());
        assert!(host.projects().unwrap().is_empty());
        let mut remote =
            AuthorityConnection::remote(":memory:", "https://example.org", None).unwrap();
        assert!(remote.bootstrap_project(bad).is_err());
        assert_eq!(remote.workspace().unwrap()["pending_count"], 0);
    }
    #[test]
    fn remote_launch_is_ordered_but_not_accepted() {
        let mut remote =
            AuthorityConnection::remote(":memory:", "https://example.org", None).unwrap();
        assert_eq!(
            remote.bootstrap_project(manifest()).unwrap(),
            vec![CommandDisposition::Pending; 3]
        );
        assert_eq!(remote.workspace().unwrap()["pending_count"], 3);
        assert!(remote.projects().unwrap().is_empty());
        assert!(remote.tasks().unwrap().is_empty());
        assert_eq!(
            remote.bootstrap_project(manifest()).unwrap(),
            vec![CommandDisposition::Pending; 3]
        );
        assert_eq!(remote.workspace().unwrap()["pending_count"], 3);
    }
    #[test]
    fn failed_project_launch_reports_committed_prefix() {
        let mut host = AuthorityConnection::embedded(":memory:", "owner").unwrap();
        let mut bad = manifest();
        bad.tasks[1].task_id = "design".into();
        assert!(bad.clone().into_workflow().is_err());
        let mut attempted = manifest();
        attempted.base_revision = 1;
        let failure = host.bootstrap_project(attempted).unwrap_err();
        assert_eq!(failure.failed_index, 0);
        assert!(failure.completed.is_empty());
    }
}

#[cfg(test)]
mod task_refinement_tests {
    use super::*;
    fn manifest() -> TaskRefinement {
        TaskRefinement {
            command_prefix: "review-oct".into(),
            base_revision: 2,
            edits: vec![TaskRefinementEdit {
                task_id: "task-a".into(),
                title: Some("Revised".into()),
                project_id: Some(Some("project-a".into())),
                status: Some("planned".into()),
            }],
        }
    }
    #[test]
    fn preflight_rejects_all_invalid_edits_without_any_write() {
        let mut remote =
            AuthorityConnection::remote(":memory:", "https://example.org", None).unwrap();
        let mut invalid = manifest();
        invalid.edits.push(TaskRefinementEdit {
            task_id: "task-a".into(),
            title: Some("Duplicate".into()),
            project_id: None,
            status: None,
        });
        let failure = remote.refine_tasks(invalid).unwrap_err();
        assert_eq!(failure.failed_index, 0);
        assert!(failure.completed.is_empty());
        assert!(remote.unresolved_commands().unwrap().is_empty());
    }
    #[test]
    fn stable_ordered_commands_and_revision_fencing() {
        let first = manifest().into_workflow().unwrap();
        let second = manifest().into_workflow().unwrap();
        assert_eq!(first.len(), 3);
        for (index, (left, right)) in first.iter().zip(second.iter()).enumerate() {
            assert_eq!(left.command_id(), right.command_id());
            let DomainWorkflowStep::Task(cmd) = left else {
                panic!("expected task command")
            };
            assert_eq!(cmd.expected_revision, 2 + index as i64);
        }
    }
    #[test]
    fn remote_refinement_queues_but_does_not_project_pending_changes() {
        let mut remote =
            AuthorityConnection::remote(":memory:", "https://example.org", None).unwrap();
        assert_eq!(
            remote.refine_tasks(manifest()).unwrap(),
            vec![CommandDisposition::Pending; 3]
        );
        assert_eq!(remote.unresolved_commands().unwrap().len(), 3);
        assert!(remote.tasks().unwrap().is_empty());
        assert_eq!(
            remote.refine_tasks(manifest()).unwrap(),
            vec![CommandDisposition::Pending; 3]
        );
        assert_eq!(remote.unresolved_commands().unwrap().len(), 3);
    }
    #[test]
    fn embedded_partial_failure_is_explicit() {
        let mut host = AuthorityConnection::embedded(":memory:", "owner").unwrap();
        let failure = host.refine_tasks(manifest()).unwrap_err();
        assert_eq!(failure.failed_index, 0);
        assert!(failure.completed.is_empty());
    }
}

#[cfg(test)]
mod m20_planning_client_acceptance {
    use super::*;
    use orbit_runtime::domain_commands::{TaskAction, TaskCommand};
    #[test]
    fn embedded_planning_changes_accepted_snapshot_and_remote_keeps_pending_separate() {
        let mut embedded = AuthorityConnection::embedded(":memory:", "owner").unwrap();
        embedded
            .task_command(TaskCommand {
                command_id: "create".into(),
                expected_revision: 0,
                action: TaskAction::Create {
                    task_id: "t".into(),
                    title: "Task".into(),
                    project_id: None,
                    milestone_id: None,
                    description: String::new(),
                    status: None,
                    priority: None,
                    estimated_minutes: None,
                    deadline_at_ms: None,
                    earliest_start_at_ms: None,
                    acceptance_criteria: Vec::new(),
                    prerequisite_ids: Vec::new(),
                },
            })
            .unwrap();
        assert_eq!(
            embedded
                .set_task_planning("plan".into(), 1, "t".into(), Some(30), Some(50), Some(100))
                .unwrap(),
            CommandDisposition::Accepted {
                revision: 2,
                duplicate: false
            }
        );
        let tasks = embedded.tasks().unwrap();
        assert_eq!(tasks[0]["estimated_minutes"], 30);
        assert_eq!(tasks[0]["deadline_at_ms"], 100);
        let replica_path =
            std::env::temp_dir().join(format!("orbit-planning-{}.sqlite", std::process::id()));
        let _ = std::fs::remove_file(&replica_path);
        let mut remote =
            AuthorityConnection::remote(&replica_path, "http://127.0.0.1:9", None).unwrap();
        assert_eq!(
            remote
                .set_task_planning("queued".into(), 0, "t".into(), Some(30), None, None)
                .unwrap(),
            CommandDisposition::Pending
        );
        assert!(remote.accepted_snapshot().unwrap().is_none());
        drop(remote);
        let _ = std::fs::remove_file(replica_path);
    }
}

#[cfg(test)]
mod m20_planning_session_tests {
    use super::*;
    use orbit_runtime::domain_commands::{TaskAction, TaskCommand};
    fn session() -> TaskPlanningSession {
        TaskPlanningSession {
            command_prefix: "sprint-one".into(),
            base_revision: 2,
            plans: vec![
                PlannedTaskWindow {
                    task_id: "task-one".into(),
                    estimated_minutes: Some(30),
                    earliest_start_at_ms: Some(100_000),
                    deadline_at_ms: Some(2_000_000),
                },
                PlannedTaskWindow {
                    task_id: "task-two".into(),
                    estimated_minutes: Some(60),
                    earliest_start_at_ms: None,
                    deadline_at_ms: Some(8_000_000),
                },
            ],
        }
    }
    #[test]
    fn bad_second_window_prevents_any_remote_outbox_entries() {
        let mut remote =
            AuthorityConnection::remote(":memory:", "https://example.org", None).unwrap();
        let mut s = session();
        s.plans[1].estimated_minutes = Some(-5);
        let failed = remote.plan_tasks(s).unwrap_err();
        assert_eq!(failed.failed_index, 0);
        assert!(failed.completed.is_empty());
        assert!(remote.unresolved_commands().unwrap().is_empty());
    }
    #[test]
    fn repeated_id_and_impossible_duration_are_rejected() {
        let mut duplicate = session();
        duplicate.plans[1].task_id = "task-one".into();
        assert!(duplicate.into_workflow().is_err());
        let mut impossible = session();
        impossible.plans[0].deadline_at_ms = Some(101_000);
        assert!(impossible.into_workflow().is_err());
    }
    #[test]
    fn embedded_session_persists_both_windows_and_preserves_retry_receipts() {
        let mut host = AuthorityConnection::embedded(":memory:", "owner").unwrap();
        for (revision, (id, title)) in [("task-one", "One"), ("task-two", "Two")]
            .into_iter()
            .enumerate()
        {
            host.task_command(TaskCommand {
                command_id: format!("create-{revision}"),
                expected_revision: revision as i64,
                action: TaskAction::Create {
                    task_id: id.into(),
                    title: title.into(),
                    project_id: None,
                    milestone_id: None,
                    description: String::new(),
                    status: None,
                    priority: None,
                    estimated_minutes: None,
                    deadline_at_ms: None,
                    earliest_start_at_ms: None,
                    acceptance_criteria: Vec::new(),
                    prerequisite_ids: Vec::new(),
                },
            })
            .unwrap();
        }
        let first = host.plan_tasks(session()).unwrap();
        assert_eq!(
            first,
            vec![
                CommandDisposition::Accepted {
                    revision: 3,
                    duplicate: false
                },
                CommandDisposition::Accepted {
                    revision: 4,
                    duplicate: false
                }
            ]
        );
        assert_eq!(host.tasks().unwrap().len(), 2);
        assert_eq!(
            host.task("task-one").unwrap().unwrap()["estimated_minutes"],
            30
        );
        assert_eq!(
            host.task("task-two").unwrap().unwrap()["deadline_at_ms"],
            8_000_000
        );
        let replay = host.plan_tasks(session()).unwrap();
        assert_eq!(
            replay,
            vec![
                CommandDisposition::Accepted {
                    revision: 3,
                    duplicate: true
                },
                CommandDisposition::Accepted {
                    revision: 4,
                    duplicate: true
                }
            ]
        );
    }
    #[test]
    fn offline_session_is_durable_and_does_not_mutate_accepted_state() {
        let mut remote =
            AuthorityConnection::remote(":memory:", "https://example.org", None).unwrap();
        assert_eq!(
            remote.plan_tasks(session()).unwrap(),
            vec![CommandDisposition::Pending; 2]
        );
        assert_eq!(remote.unresolved_commands().unwrap().len(), 2);
        assert!(remote.tasks().unwrap().is_empty());
        assert_eq!(
            remote.plan_tasks(session()).unwrap(),
            vec![CommandDisposition::Pending; 2]
        );
        assert_eq!(remote.unresolved_commands().unwrap().len(), 2);
    }
    #[test]
    fn routine_and_milestone_domains_share_one_global_authority_revision() {
        use orbit_runtime::authority_milestones::{MilestoneAction, MilestoneCommand};
        use orbit_runtime::authority_routines::{RoutineAction, RoutineCommand};
        let mut host = AuthorityConnection::embedded(":memory:", "owner").unwrap();
        host.create_project(ProjectCommand {
            workflow_id: None,
            command_id: "project-a".into(),
            expected_revision: 0,
            project_id: "project".into(),
            name: "Project".into(),
        })
        .unwrap();
        host.evidence_command(EvidenceCommand {
            command_id: "habit-a".into(),
            expected_revision: 1,
            action: orbit_runtime::authority_evidence::EvidenceAction::CreateHabit {
                habit_id: "walk".into(),
                name: "Walk".into(),
                cadence: "daily".into(),
            },
        })
        .unwrap();
        let routine = RoutineCommand {
            command_id: "routine-a".into(),
            expected_revision: 2,
            action: RoutineAction::Create {
                routine_id: "morning".into(),
                name: "Morning".into(),
                habit_ids: vec!["walk".into()],
            },
        };
        let milestone = MilestoneCommand {
            command_id: "milestone-a".into(),
            expected_revision: 3,
            action: MilestoneAction::Create {
                milestone_id: "launch".into(),
                project_id: "project".into(),
                name: "Launch".into(),
                description: String::new(),
                priority: None,
                owner: None,
                start_at_ms: None,
                target_at_ms: None,
            },
        };
        assert_eq!(
            host.routine_command(routine.clone()).unwrap(),
            CommandDisposition::Accepted {
                revision: 3,
                duplicate: false
            }
        );
        assert_eq!(
            host.milestone_command(milestone.clone()).unwrap(),
            CommandDisposition::Accepted {
                revision: 4,
                duplicate: false
            }
        );
        assert_eq!(
            host.routine_command(routine).unwrap(),
            CommandDisposition::Accepted {
                revision: 3,
                duplicate: true
            }
        );
        assert_eq!(
            host.milestone_command(milestone).unwrap(),
            CommandDisposition::Accepted {
                revision: 4,
                duplicate: true
            }
        );
        let accepted = host.accepted_snapshot().unwrap().unwrap();
        assert_eq!(accepted["revision"], 4);
        assert_eq!(accepted["routines"][0]["name"], "Morning");
        assert_eq!(accepted["routines"][0]["habit_ids"][0], "walk");
        assert_eq!(accepted["milestones"][0]["name"], "Launch");
        assert_eq!(accepted["milestones"][0]["status"], "planned");
        assert_eq!(host.routines().unwrap().len(), 1);
        assert_eq!(host.milestones().unwrap().len(), 1);
    }

    #[test]
    fn remote_routine_and_milestone_commands_are_only_pending_until_acknowledged() {
        use orbit_runtime::authority_milestones::{MilestoneAction, MilestoneCommand};
        use orbit_runtime::authority_routines::{RoutineAction, RoutineCommand};
        let local = AuthorityConnection::embedded(":memory:", "owner").unwrap();
        let original = local.accepted_snapshot().unwrap().unwrap();
        let mut remote =
            AuthorityConnection::remote(":memory:", "https://example.org", None).unwrap();
        assert!(remote.accepted_snapshot().unwrap().is_none());
        if let AuthorityConnection::Remote { replica, .. } = &mut remote {
            replica.accept_snapshot_for_testing(&original).unwrap();
        }
        let routine = RoutineCommand {
            command_id: "intent-routine".into(),
            expected_revision: 0,
            action: RoutineAction::Create {
                routine_id: "routine".into(),
                name: "Routine".into(),
                habit_ids: vec![],
            },
        };
        let milestone = MilestoneCommand {
            command_id: "intent-milestone".into(),
            expected_revision: 1,
            action: MilestoneAction::Create {
                milestone_id: "milestone".into(),
                project_id: "future".into(),
                name: "Plan".into(),
                description: String::new(),
                priority: None,
                owner: None,
                start_at_ms: None,
                target_at_ms: None,
            },
        };
        assert_eq!(
            remote.routine_command(routine.clone()).unwrap(),
            CommandDisposition::Pending
        );
        assert_eq!(
            remote.milestone_command(milestone.clone()).unwrap(),
            CommandDisposition::Pending
        );
        assert_eq!(
            remote.routine_command(routine).unwrap(),
            CommandDisposition::Pending
        );
        assert_eq!(
            remote.milestone_command(milestone).unwrap(),
            CommandDisposition::Pending
        );
        assert_eq!(remote.workspace().unwrap()["pending_count"], 2);
        assert_eq!(remote.accepted_snapshot().unwrap().unwrap(), original);
        assert!(remote.routines().unwrap().is_empty());
        assert!(remote.milestones().unwrap().is_empty());
        assert_eq!(remote.unresolved_commands().unwrap().len(), 2);
    }

    #[test]
    fn accepted_routine_and_milestone_snapshots_match_remote_projection() {
        use orbit_runtime::authority_milestones::{MilestoneAction, MilestoneCommand};
        use orbit_runtime::authority_routines::{RoutineAction, RoutineCommand};
        let mut local = AuthorityConnection::embedded(":memory:", "owner").unwrap();
        local
            .create_project(ProjectCommand {
                workflow_id: None,
                command_id: "project".into(),
                expected_revision: 0,
                project_id: "p".into(),
                name: "Project".into(),
            })
            .unwrap();
        local
            .routine_command(RoutineCommand {
                command_id: "routine".into(),
                expected_revision: 1,
                action: RoutineAction::Create {
                    routine_id: "r".into(),
                    name: "Review".into(),
                    habit_ids: vec![],
                },
            })
            .unwrap();
        local
            .milestone_command(MilestoneCommand {
                command_id: "milestone".into(),
                expected_revision: 2,
                action: MilestoneAction::Create {
                    milestone_id: "m".into(),
                    project_id: "p".into(),
                    name: "Review".into(),
                    description: String::new(),
                    priority: None,
                    owner: None,
                    start_at_ms: None,
                    target_at_ms: None,
                },
            })
            .unwrap();
        let snapshot = local.accepted_snapshot().unwrap().unwrap();
        let mut remote =
            AuthorityConnection::remote(":memory:", "https://example.org", None).unwrap();
        if let AuthorityConnection::Remote { replica, .. } = &mut remote {
            replica.accept_snapshot_for_testing(&snapshot).unwrap();
        }
        assert_eq!(local.routines().unwrap(), remote.routines().unwrap());
        assert_eq!(local.milestones().unwrap(), remote.milestones().unwrap());
        assert_eq!(remote.accepted_snapshot().unwrap().unwrap(), snapshot);
    }
}

#[cfg(test)]
mod strategy_parity_tests {
    use super::*;
    use orbit_runtime::authority_strategy::{CriterionInput, StrategyAction, StrategyCommand};

    fn configure(id: &str, revision: i64) -> StrategyCommand {
        StrategyCommand {
            command_id: id.into(),
            expected_revision: revision,
            action: StrategyAction::Configure {
                project_id: "project".into(),
                mode: "custom".into(),
                objective: Some("Ship release".into()),
                progress_signals: vec!["milestones".into(), "tasks".into()],
                criteria: vec![CriterionInput {
                    key: "release".into(),
                    description: "Publish binary".into(),
                    required: true,
                }],
            },
        }
    }
    #[test]
    fn strategy_command_is_accepted_in_embedded_mode() {
        let mut client = AuthorityConnection::embedded(":memory:", "owner").unwrap();
        client
            .create_project(ProjectCommand {
                workflow_id: None,
                command_id: "project-create".into(),
                expected_revision: 0,
                project_id: "project".into(),
                name: "Release".into(),
            })
            .unwrap();
        let result = client.strategy_command(configure("strategy", 1)).unwrap();
        assert_eq!(
            result,
            CommandDisposition::Accepted {
                revision: 2,
                duplicate: false
            }
        );
        let strategies = client.strategies().unwrap();
        assert_eq!(strategies.len(), 1);
        assert_eq!(strategies[0]["mode"], "custom");
        assert_eq!(strategies[0]["objective"], "Ship release");
        assert_eq!(strategies[0]["criteria"][0]["key"], "release");
    }
    #[test]
    fn accepted_local_retry_does_not_advance_revision() {
        let mut client = AuthorityConnection::embedded(":memory:", "owner").unwrap();
        client
            .create_project(ProjectCommand {
                workflow_id: None,
                command_id: "create".into(),
                expected_revision: 0,
                project_id: "project".into(),
                name: "Release".into(),
            })
            .unwrap();
        client.strategy_command(configure("strategy", 1)).unwrap();
        let result = client.strategy_command(configure("strategy", 1)).unwrap();
        assert_eq!(
            result,
            CommandDisposition::Accepted {
                revision: 2,
                duplicate: true
            }
        );
        assert_eq!(client.strategies().unwrap().len(), 1);
    }
    #[test]
    fn remote_strategy_is_pending_until_server_acceptance() {
        let mut client =
            AuthorityConnection::remote(":memory:", "https://example.org", None).unwrap();
        let queued = client.strategy_command(configure("strategy", 1)).unwrap();
        assert_eq!(queued, CommandDisposition::Pending);
        assert!(client.strategies().is_err() || client.strategies().unwrap().is_empty());
        let pending = client.unresolved_commands().unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0]["kind"], "strategy");
    }
    #[test]
    fn remote_retry_does_not_duplicate_outbox_intent() {
        let mut client =
            AuthorityConnection::remote(":memory:", "https://example.org", None).unwrap();
        client.strategy_command(configure("strategy", 1)).unwrap();
        client.strategy_command(configure("strategy", 1)).unwrap();
        assert_eq!(client.unresolved_commands().unwrap().len(), 1);
    }
    #[test]
    fn remote_conflicting_reuse_of_identity_fails() {
        let mut client =
            AuthorityConnection::remote(":memory:", "https://example.org", None).unwrap();
        client.strategy_command(configure("strategy", 1)).unwrap();
        let mut changed = configure("strategy", 1);
        if let StrategyAction::Configure { objective, .. } = &mut changed.action {
            *objective = Some("Different".into());
        }
        assert!(client.strategy_command(changed).is_err());
        assert_eq!(client.unresolved_commands().unwrap().len(), 1);
    }
}

#[cfg(test)]
mod unified_remote_atomic_workflow_tests {
    use super::*;
    use orbit_runtime::domain_commands::TaskAction;

    fn project(id: &str) -> DomainWorkflowStep {
        DomainWorkflowStep::Project(ProjectCommand {
            workflow_id: None,
            command_id: id.to_owned(),
            expected_revision: 0,
            project_id: "proj".to_owned(),
            name: "Project".to_owned(),
        })
    }
    fn task(id: &str) -> DomainWorkflowStep {
        DomainWorkflowStep::Task(TaskCommand {
            command_id: id.to_owned(),
            expected_revision: 1,
            action: TaskAction::Create {
                task_id: "task".to_owned(),
                title: "Task".to_owned(),
                project_id: Some("proj".to_owned()),
                milestone_id: None,
                description: String::new(),
                status: None,
                priority: None,
                estimated_minutes: None,
                deadline_at_ms: None,
                earliest_start_at_ms: None,
                acceptance_criteria: Vec::new(),
                prerequisite_ids: Vec::new(),
            },
        })
    }

    #[test]
    fn remote_stages_entire_project_task_workflow_without_accepted_projection() {
        let mut client =
            AuthorityConnection::remote(":memory:", "https://example.org", None).unwrap();
        assert_eq!(
            client
                .execute_workflow(vec![project("p1"), task("t1")])
                .unwrap(),
            vec![CommandDisposition::Pending, CommandDisposition::Pending]
        );
        let unresolved = client.unresolved_commands().unwrap();
        assert_eq!(unresolved.len(), 2);
        assert_eq!(unresolved[0]["kind"], "project");
        assert_eq!(unresolved[0]["action"]["project_id"], "proj");
        assert_eq!(unresolved[1]["kind"], "task");
        assert_eq!(unresolved[1]["action"]["task_id"], "task");
        assert_eq!(client.projects().unwrap().len(), 0);
        assert_eq!(client.tasks().unwrap().len(), 0);
    }

    #[test]
    fn conflicting_last_step_does_not_stage_new_prefix() {
        let mut client =
            AuthorityConnection::remote(":memory:", "https://example.org", None).unwrap();
        client.execute_workflow(vec![task("taken")]).unwrap();
        let changed = DomainWorkflowStep::Task(TaskCommand {
            command_id: "taken".to_owned(),
            expected_revision: 2,
            action: TaskAction::ChangeStatus {
                task_id: "task".to_owned(),
                status: "done".to_owned(),
            },
        });
        assert!(
            client
                .execute_workflow(vec![project("new-project"), changed])
                .is_err()
        );
        let pending = client.unresolved_commands().unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0]["command_id"], "taken");
    }

    #[test]
    fn retrying_same_workflow_does_not_duplicate_intentions() {
        let mut client =
            AuthorityConnection::remote(":memory:", "https://example.org", None).unwrap();
        for _ in 0..3 {
            assert_eq!(
                client
                    .execute_workflow(vec![project("p1"), task("t1")])
                    .unwrap(),
                vec![CommandDisposition::Pending, CommandDisposition::Pending]
            );
        }
        assert_eq!(client.unresolved_commands().unwrap().len(), 2);
    }

    #[test]
    fn duplicate_ids_fail_without_any_queue_side_effects() {
        let mut client =
            AuthorityConnection::remote(":memory:", "https://example.org", None).unwrap();
        let result = client.execute_workflow(vec![project("same"), task("same")]);
        assert!(result.is_err());
        assert!(client.unresolved_commands().unwrap().is_empty());
    }
}

#[cfg(test)]
mod m22_dependency_client_contracts {
    use super::*;
    use orbit_runtime::domain_commands::{TaskAction, TaskCommand};

    fn create(client: &mut AuthorityConnection, id: &str, revision: i64) {
        let result = client
            .task_command(TaskCommand {
                command_id: format!("create-{id}"),
                expected_revision: revision,
                action: TaskAction::Create {
                    task_id: id.into(),
                    title: format!("Title {id}"),
                    project_id: None,
                    milestone_id: None,
                    description: String::new(),
                    status: None,
                    priority: None,
                    estimated_minutes: None,
                    deadline_at_ms: None,
                    earliest_start_at_ms: None,
                    acceptance_criteria: Vec::new(),
                    prerequisite_ids: Vec::new(),
                },
            })
            .unwrap();
        assert_eq!(
            result,
            CommandDisposition::Accepted {
                revision: revision + 1,
                duplicate: false
            }
        );
    }

    #[test]
    fn embedded_client_exposes_revisioned_graph_and_readiness() {
        let mut client = AuthorityConnection::embedded(":memory:", "owner").unwrap();
        create(&mut client, "alpha", 0);
        create(&mut client, "beta", 1);
        let changed = client
            .add_task_prerequisite("edge".into(), 2, "beta".into(), "alpha".into())
            .unwrap();
        assert_eq!(
            changed,
            CommandDisposition::Accepted {
                revision: 3,
                duplicate: false
            }
        );
        let graph = client.task_graph(1000).unwrap();
        assert_eq!(graph.revision, 3);
        assert_eq!(graph.dependency_order, ["alpha", "beta"]);
        assert_eq!(
            graph
                .nodes
                .iter()
                .find(|node| node.task_id == "alpha")
                .unwrap()
                .all_dependents,
            ["beta"]
        );
        let report = client.task_readiness(1000).unwrap();
        assert_eq!(report.blocked_count, 1);
        assert_eq!(
            report
                .tasks
                .iter()
                .find(|task| task.task_id == "beta")
                .unwrap()
                .blocking_task_ids,
            ["alpha"]
        );
        let workspace = client.workspace().unwrap();
        assert_eq!(workspace["task_dependencies"][0]["task_id"], "beta");
        let remove = client
            .remove_task_prerequisite("remove".into(), 3, "beta".into(), "alpha".into())
            .unwrap();
        assert_eq!(
            remove,
            CommandDisposition::Accepted {
                revision: 4,
                duplicate: false
            }
        );
        assert_eq!(client.task_graph(1000).unwrap().edge_count, 0);
    }

    #[test]
    fn embedded_client_does_not_publish_rejected_cycle() {
        let mut client = AuthorityConnection::embedded(":memory:", "owner").unwrap();
        create(&mut client, "a", 0);
        create(&mut client, "b", 1);
        client
            .add_task_prerequisite("ab".into(), 2, "a".into(), "b".into())
            .unwrap();
        assert!(
            client
                .add_task_prerequisite("ba".into(), 3, "b".into(), "a".into())
                .is_err()
        );
        assert_eq!(client.task_graph(0).unwrap().edge_count, 1);
        assert_eq!(client.workspace().unwrap()["revision"], 3);
    }

    #[test]
    fn offline_readiness_requires_an_accepted_snapshot() {
        let mut client =
            AuthorityConnection::remote(":memory:", "http://localhost:9999", None).unwrap();
        assert!(client.task_readiness(1000).is_err());
        assert!(client.task_graph(1000).is_err());
        let disposition = client
            .task_command(TaskCommand {
                command_id: "offline-create".into(),
                expected_revision: 0,
                action: TaskAction::Create {
                    task_id: "later".into(),
                    title: "Later".into(),
                    project_id: None,
                    milestone_id: None,
                    description: String::new(),
                    status: None,
                    priority: None,
                    estimated_minutes: None,
                    deadline_at_ms: None,
                    earliest_start_at_ms: None,
                    acceptance_criteria: Vec::new(),
                    prerequisite_ids: Vec::new(),
                },
            })
            .unwrap();
        assert_eq!(disposition, CommandDisposition::Pending);
        // Outbox intention must never appear as an accepted action.
        assert!(client.task_readiness(1000).is_err());
    }
}

#[cfg(test)]
mod m22_dependency_preview_client_contracts {
    use super::*;
    use orbit_runtime::{
        domain_commands::{TaskAction, TaskCommand},
        task_dependency_preview::{DependencyPreviewRequest, ProposedDependencyEdit},
    };
    fn add(client: &mut AuthorityConnection, id: &str, revision: i64, status: &str) {
        assert_eq!(
            client
                .task_command(TaskCommand {
                    command_id: format!("create-{id}"),
                    expected_revision: revision,
                    action: TaskAction::Create {
                        task_id: id.into(),
                        title: id.into(),
                        project_id: None,
                        milestone_id: None,
                        description: String::new(),
                        status: None,
                        priority: None,
                        estimated_minutes: None,
                        deadline_at_ms: None,
                        earliest_start_at_ms: None,
                        acceptance_criteria: Vec::new(),
                        prerequisite_ids: Vec::new(),
                    },
                })
                .unwrap(),
            CommandDisposition::Accepted {
                revision: revision + 1,
                duplicate: false
            }
        );
        if status != "inbox" {
            client
                .task_command(TaskCommand {
                    command_id: format!("status-{id}"),
                    expected_revision: revision + 1,
                    action: TaskAction::ChangeStatus {
                        task_id: id.into(),
                        status: status.into(),
                    },
                })
                .unwrap();
        }
    }
    fn preview(
        client: &AuthorityConnection,
        task: &str,
        prior: &str,
        edit: ProposedDependencyEdit,
    ) -> Result<orbit_runtime::task_dependency_preview::DependencyPreview, ClientConnectionError>
    {
        client.preview_task_dependency(&DependencyPreviewRequest {
            task_id: task.into(),
            prerequisite_id: prior.into(),
            edit,
            now_ms: 1000,
        })
    }
    #[test]
    fn embedded_preview_is_read_only_and_revision_bound() {
        let mut client = AuthorityConnection::embedded(":memory:", "owner").unwrap();
        add(&mut client, "task-a", 0, "inbox");
        add(&mut client, "task-b", 1, "inbox");
        let before = client.workspace().unwrap();
        let result = preview(&client, "task-b", "task-a", ProposedDependencyEdit::Add).unwrap();
        assert_eq!(result.accepted_revision, 2);
        assert!(result.changes_graph);
        assert_eq!((result.before_edge_count, result.after_edge_count), (0, 1));
        assert_eq!(client.workspace().unwrap(), before);
        assert!(client.unresolved_commands().unwrap().is_empty());
    }
    #[test]
    fn embedded_preview_reflects_accepted_edge_changes() {
        let mut client = AuthorityConnection::embedded(":memory:", "owner").unwrap();
        add(&mut client, "task-a", 0, "inbox");
        add(&mut client, "task-b", 1, "inbox");
        let first = preview(&client, "task-b", "task-a", ProposedDependencyEdit::Add).unwrap();
        client
            .add_task_prerequisite("edge".into(), 2, "task-b".into(), "task-a".into())
            .unwrap();
        let second = preview(&client, "task-b", "task-a", ProposedDependencyEdit::Add).unwrap();
        assert!(!second.changes_graph);
        assert_eq!(second.accepted_revision, first.accepted_revision + 1);
        assert_eq!(second.before_edge_count, 1);
        assert_eq!(
            preview(&client, "task-b", "task-a", ProposedDependencyEdit::Remove)
                .unwrap()
                .after_edge_count,
            0
        );
    }
    #[test]
    fn connected_preview_requires_accepted_snapshot() {
        let mut client =
            AuthorityConnection::remote(":memory:", "https://example.org", None).unwrap();
        assert!(preview(&client, "a", "b", ProposedDependencyEdit::Add).is_err());
        assert_eq!(
            client
                .add_task_prerequisite("queued".into(), 0, "a".into(), "b".into())
                .unwrap(),
            CommandDisposition::Pending
        );
        // Queuing a hypothetical dependency does not manufacture an accepted
        // snapshot or allow offline what-if previews to inspect pending data.
        assert!(preview(&client, "a", "b", ProposedDependencyEdit::Add).is_err());
        assert_eq!(client.unresolved_commands().unwrap().len(), 1);
    }
    #[test]
    fn embedded_preview_rejects_a_cycle_without_staging_any_change() {
        let mut client = AuthorityConnection::embedded(":memory:", "owner").unwrap();
        add(&mut client, "a", 0, "inbox");
        add(&mut client, "b", 1, "inbox");
        client
            .add_task_prerequisite("edge".into(), 2, "b".into(), "a".into())
            .unwrap();
        let before = client.workspace().unwrap();
        assert!(preview(&client, "a", "b", ProposedDependencyEdit::Add).is_err());
        assert_eq!(client.workspace().unwrap(), before);
    }
}

#[cfg(test)]
mod renderer_contract_tests {
    use super::*;
    use orbit_runtime::domain_commands::{ProjectCommand, TaskAction, TaskCommand};

    #[test]
    fn embedded_and_connected_clients_expose_the_shared_view_contract() {
        let mut client = AuthorityConnection::embedded(":memory:", "owner").unwrap();
        assert_eq!(
            client
                .create_project(ProjectCommand {
                    workflow_id: None,
                    command_id: "project-view".into(),
                    expected_revision: 0,
                    project_id: "p1".into(),
                    name: "Connected project".into(),
                })
                .unwrap(),
            CommandDisposition::Accepted {
                revision: 1,
                duplicate: false,
            }
        );
        assert_eq!(
            client
                .task_command(TaskCommand {
                    command_id: "task-view".into(),
                    expected_revision: 1,
                    action: TaskAction::Create {
                        task_id: "t1".into(),
                        title: "View task".into(),
                        project_id: Some("p1".into()),
                        milestone_id: None,
                        description: String::new(),
                        status: None,
                        priority: None,
                        estimated_minutes: None,
                        deadline_at_ms: None,
                        earliest_start_at_ms: None,
                        acceptance_criteria: Vec::new(),
                        prerequisite_ids: Vec::new(),
                    },
                })
                .unwrap(),
            CommandDisposition::Accepted {
                revision: 2,
                duplicate: false,
            }
        );
        client
            .task_command(TaskCommand {
                command_id: "task-ready".into(),
                expected_revision: 2,
                action: TaskAction::ChangeStatus {
                    task_id: "t1".into(),
                    status: "ready".into(),
                },
            })
            .unwrap();
        let workspace = client.workspace_view_data().unwrap();
        assert_eq!(workspace.projects.len(), 1);
        assert!(workspace.tasks.iter().any(|task| task.id == "t1"));
        let (data, views, generated) = client
            .resolve_connected_project_views("p1", &orbit_ui::ComponentRegistry::builtin())
            .unwrap();
        assert_eq!(data.project.name, "Connected project");
        assert!(generated);
        assert!(views.iter().any(|view| view.id == "overview"));
        assert!(workspace.inbox.iter().all(|task| task.status == "inbox"));
    }

    #[test]
    fn remote_view_contract_reports_no_accepted_snapshot() {
        let client = AuthorityConnection::remote(":memory:", "https://example.org", None).unwrap();
        assert!(client.workspace_view_data().unwrap().projects.is_empty());
        assert!(client.project_view_data("p1").is_err());
    }
}
