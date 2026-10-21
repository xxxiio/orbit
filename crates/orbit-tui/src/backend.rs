//! Local/connected backend for the one project-centric TUI shell.
//!
//! The renderer uses one view model for both modes; the shell only branches for
//! capability/availability labels. Connected mutations return queued DTOs but
//! expose whether the command is still pending (never accepted) through
//! `take_pending`.

use orbit_client::OrbitClient;
use orbit_client::unified::{AuthorityConnection, CommandDisposition};
use orbit_client::{
    ClientError, ClientErrorCode, ConfigureProjectStrategyRequest, CreateKnowledgeRequest,
    CreateProjectRequest, CreateTaskRequest, HabitDto, MetricTargetDto, ProjectDto,
    ProjectEvidenceBindingDto, RecordHabitOccurrenceRequest, SetProjectStatusRequest,
    SetTaskPlanningRequest, SetTaskStatusRequest, TaskDto, TaskPlanProposalDto,
};
use orbit_runtime::domain_commands::{ProjectCommand, TaskAction, TaskCommand};
use std::path::Path;
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

pub enum Backend {
    Local(OrbitClient),
    Connected {
        connection: AuthorityConnection,
        pending: bool,
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
            pending: false,
        })
    }

    pub fn is_connected(&self) -> bool {
        matches!(self, Self::Connected { .. })
    }

    /// Returns and clears the pending flag left by the last mutation.
    pub fn take_pending(&mut self) -> bool {
        match self {
            Self::Local(_) => false,
            Self::Connected { pending, .. } => std::mem::take(pending),
        }
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
                            "Accepted {accepted}; {command_id} remains blocked: {reason}. Resolve the conflict with the CLI or legacy connected command surface."
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

    pub fn pending_count(&self) -> usize {
        match self {
            Self::Local(_) => 0,
            Self::Connected { connection, .. } => connection
                .unresolved_commands()
                .map(|rows| rows.len())
                .unwrap_or(0),
        }
    }

    fn record_disposition(&mut self, disposition: &CommandDisposition) {
        if let Self::Connected { pending, .. } = self {
            *pending |= matches!(disposition, CommandDisposition::Pending);
        }
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
                *pending |= matches!(disposition, CommandDisposition::Pending);
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
                    *pending |= matches!(disposition, CommandDisposition::Pending);
                }
                Ok(TaskDto {
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
}
