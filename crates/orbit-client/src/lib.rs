//! Stable embedded-client facade for GUI/FFI consumers.
//!
//! This crate intentionally depends on Orbit application/storage contracts but
//! has no Flutter, Dart, MCP, or TUI dependency. AI provider access is routed through the optional Orbit AI integration contracts rather than implemented in Flutter. Generated FFI
//! bindings should wrap this facade instead of exposing repository/domain
//! internals directly.

pub mod config;
#[cfg(test)]
mod convergence_tests;
pub mod deployment;
#[cfg(test)]
mod mirror_consistency_tests;
pub mod outbox_audit;
pub mod sync_health;

use std::{
    path::Path,
    process,
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

#[cfg(test)]
use orbit_ai::{
    AiAccessPolicy, AiAuthMode, AiCapabilities, AiContextFragment, AiDataCategory, AiDestination,
    AiProjectAccessMode, AiProjectPolicy, AiProviderConfig, AiProviderKind, AiSecretRef,
    DirectHttpAiProvider, ProjectAgentPlaybook, ReqwestAiHttpTransport, build_disclosure,
    invoke_project_completion_review_with_disclosure, invoke_project_roadmap_with_disclosure,
    invoke_with_disclosure, parse_project_completion_assessment, parse_project_roadmap_proposal,
};
use orbit_application::{
    ApplicationError, ApplyValidatedHabitPlanCommand, ApplyValidatedPlanCommand,
    ConfigureProjectStrategyCommand, ConfigureProjectStrategyWithActivityCommand,
    CreateHabitCommand, CreateHabitWithActivityCommand, CreateProjectCommand,
    CreateProjectWithActivityCommand, CreateScheduleBlockCommand,
    CreateScheduleBlockWithActivityCommand, CreateTaskCommand, CreateTaskWithActivityCommand,
    HabitPlanApplicationIdentity, PlanApplicationIdentity,
    RecordHabitOccurrenceWithActivityCommand, RepositoryError, RescheduleBlockWithActivityCommand,
    SetHabitActiveWithActivityCommand, SetProjectCriterionStatusWithActivityCommand,
    SetTaskPlanningCommand, SetTaskPlanningWithActivityCommand, TransitionProjectCommand,
    TransitionProjectWithActivityCommand, TransitionTaskCommand, TransitionTaskWithActivityCommand,
};
use orbit_domain::{
    ActivityId, ActivityKind, ActivityOrigin, ActivityRecord, ActivitySubject, Habit, HabitCadence,
    HabitId, HabitOccurrence, HabitOccurrenceId, HabitPeriod, KnowledgeItem, KnowledgeItemId,
    KnowledgeKind, KnowledgeProvenance, KnowledgeStatus, Project, ProjectId, ProjectMode,
    ProjectProgressSignal, ProjectStatus, ProjectSuccessCriterion, ScheduleBlock, ScheduleBlockId,
    ScheduleFlexibility, ScheduleTarget, SuccessCriterionStatus, Task, TaskId, TaskPlanning,
    TaskScope, TaskStatus,
};

use orbit_planner::{
    AvailabilitySlot, CapacityPolicy, FixedBlockBufferPolicy, HabitPlanProposal, HabitPlanRequest,
    PlanProposal, PlanningWindow, ProposedHabitBlock, ProposedTaskBlock, UnscheduledHabit,
    UnscheduledHabitReason, UnscheduledReason, UnscheduledTask,
    propose_habits_with_availability_and_buffers, propose_tasks_with_availability_and_buffers,
    validate_habit_proposal_with_buffers, validate_proposal_with_buffers,
};

pub const ORBIT_CLIENT_API_VERSION: u32 = 4;

static CLIENT_ID_SEQUENCE: AtomicU64 = AtomicU64::new(1);

mod attachments;
mod attachments_remote;
pub mod authority;
pub mod connected;
pub mod embedded_authority;
mod file_knowledge_bridge;
mod task_details;
pub mod unified;
pub mod view_data;
pub use embedded_authority::EmbeddedAuthorityClient;

mod contract;
pub use contract::*;

pub struct OrbitClient {
    runtime: orbit_runtime::AuthoritativeHost,
}
/// Opaque review artifact. Callers can inspect `review` but cannot mutate
/// its private validated plan command before the explicit approval step.
pub struct PreparedTaskPlan {
    pub review: orbit_runtime::planning_approval::PlanReview,
    command: ApplyValidatedPlanCommand,
}

impl OrbitClient {
    /// Read note references from the already-open authoritative runtime.
    /// A client must never initialize a second host to perform a query.
    pub fn markdown_term_references(
        &self,
        term: &str,
        limit: usize,
    ) -> Result<Vec<orbit_application::KnowledgeTermOccurrence>, ClientError> {
        self.runtime
            .markdown_term_locations(term, limit)
            .map_err(|error| ClientError {
                code: ClientErrorCode::Unavailable,
                message: format!("{error:?}"),
            })
    }

    /// Shared embedded project summary; the terminal does not reopen SQLite
    /// for every project or reproduce business calculations in its renderer.
    pub fn project_evidence_summary(
        &self,
        raw_id: &str,
        now_ms: i64,
    ) -> Result<String, ClientError> {
        let id = ProjectId::new(raw_id.to_owned()).map_err(|error| ClientError {
            code: ClientErrorCode::InvalidInput,
            message: format!("{error:?}"),
        })?;
        let health = self
            .runtime
            .application_queries()
            .project_health(&id, 10080, orbit_progress::HealthPolicy::default())
            .map_err(ClientError::from)?;
        let start = now_ms.saturating_sub(7 * 24 * 60 * 60 * 1000);
        let period = HabitPeriod::new(start, now_ms).map_err(|error| ClientError {
            code: ClientErrorCode::InvalidInput,
            message: format!("{error:?}"),
        })?;
        let evidence = self
            .runtime
            .application_queries()
            .project_evidence_report(&id, &[period], now_ms)
            .map_err(ClientError::from)?;
        let signal = evidence
            .evidence_progress
            .map(|p| format!("{}%", p.value() / 100))
            .unwrap_or_else(|| "unknown".into());
        Ok(format!(
            "task:{}% evidence:{} health:{:?}",
            health.rollup.progress.value() / 100,
            signal,
            health.health.state
        ))
    }
}

impl OrbitClient {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, ClientError> {
        let path = path.as_ref();
        let runtime = orbit_runtime::AuthoritativeHost::open(path).map_err(|e| ClientError {
            code: ClientErrorCode::Unavailable,
            message: format!("{e:?}"),
        })?;
        Ok(Self { runtime })
    }

    /// Read-only deterministic Today/Next brief from the same authoritative host
    /// that owns local task mutations. This must not open a second database.
    pub fn work_brief(
        &self,
        request: &orbit_runtime::planning_brief::WorkBriefRequest,
    ) -> Result<orbit_runtime::planning_brief::WorkBrief, ClientError> {
        self.runtime
            .work_brief(request)
            .map_err(|error| ClientError {
                code: ClientErrorCode::Unavailable,
                message: format!("{error:?}"),
            })
    }

    /// Drill into an accepted Today/Next task without exposing invisible
    /// cross-project identities or changing authoritative state.
    pub fn planning_focus(
        &self,
        brief: &orbit_runtime::planning_brief::WorkBrief,
        request: &orbit_runtime::planning_focus::FocusRequest,
    ) -> Result<orbit_runtime::planning_focus::TaskFocus, ClientError> {
        self.runtime
            .planning_focus(brief, request)
            .map_err(|error| ClientError {
                code: ClientErrorCode::Unavailable,
                message: format!("{error:?}"),
            })
    }

    /// Accepted project and milestone drilldown for daily planning clients.
    pub fn planning_task_actions(
        &self,
        brief: &orbit_runtime::planning_brief::WorkBrief,
        request: &orbit_runtime::planning_actions::TaskActionRequest,
    ) -> Result<orbit_runtime::planning_actions::PlanningActionMenu, ClientError> {
        self.runtime
            .planning_task_actions(brief, request)
            .map_err(|error| ClientError {
                code: ClientErrorCode::Unavailable,
                message: format!("{error:?}"),
            })
    }
    pub fn planning_group_actions(
        &self,
        brief: &orbit_runtime::planning_brief::WorkBrief,
        request: &orbit_runtime::planning_actions::GroupActionRequest,
    ) -> Result<orbit_runtime::planning_actions::PlanningActionMenu, ClientError> {
        self.runtime
            .planning_group_actions(brief, request)
            .map_err(|error| ClientError {
                code: ClientErrorCode::Unavailable,
                message: format!("{error:?}"),
            })
    }

    pub fn resolve_planning_task_action(
        &self,
        brief: &orbit_runtime::planning_brief::WorkBrief,
        request: &orbit_runtime::planning_actions::TaskActionRequest,
        selection: &orbit_runtime::planning_actions::ActionSelection,
    ) -> Result<orbit_runtime::planning_actions::ResolvedAction, ClientError> {
        self.runtime
            .resolve_planning_task_action(brief, request, selection)
            .map_err(|error| ClientError {
                code: ClientErrorCode::Unavailable,
                message: format!("{error:?}"),
            })
    }
    pub fn resolve_planning_group_action(
        &self,
        brief: &orbit_runtime::planning_brief::WorkBrief,
        request: &orbit_runtime::planning_actions::GroupActionRequest,
        selection: &orbit_runtime::planning_actions::ActionSelection,
    ) -> Result<orbit_runtime::planning_actions::ResolvedAction, ClientError> {
        self.runtime
            .resolve_planning_group_action(brief, request, selection)
            .map_err(|error| ClientError {
                code: ClientErrorCode::Unavailable,
                message: format!("{error:?}"),
            })
    }

    /// Dispatch a reviewed action to its non-executable accepted planning view.
    pub fn planning_task_handoff(
        &self,
        brief: &orbit_runtime::planning_brief::WorkBrief,
        request: &orbit_runtime::planning_actions::TaskActionRequest,
        selection: &orbit_runtime::planning_actions::ActionSelection,
    ) -> Result<orbit_runtime::planning_handoff::PlanningHandoff, ClientError> {
        self.runtime
            .planning_task_handoff(brief, request, selection)
            .map_err(|error| ClientError {
                code: ClientErrorCode::Unavailable,
                message: format!("{error:?}"),
            })
    }
    pub fn planning_group_handoff(
        &self,
        brief: &orbit_runtime::planning_brief::WorkBrief,
        request: &orbit_runtime::planning_actions::GroupActionRequest,
        selection: &orbit_runtime::planning_actions::ActionSelection,
    ) -> Result<orbit_runtime::planning_handoff::PlanningHandoff, ClientError> {
        self.runtime
            .planning_group_handoff(brief, request, selection)
            .map_err(|error| ClientError {
                code: ClientErrorCode::Unavailable,
                message: format!("{error:?}"),
            })
    }

    pub fn planning_group(
        &self,
        brief: &orbit_runtime::planning_brief::WorkBrief,
        request: &orbit_runtime::planning_focus::GroupFocusRequest,
    ) -> Result<orbit_runtime::planning_focus::GroupFocus, ClientError> {
        self.runtime
            .planning_group(brief, request)
            .map_err(|error| ClientError {
                code: ClientErrorCode::Unavailable,
                message: format!("{error:?}"),
            })
    }

    /// Explain downstream impact without modifying task status or schedule.
    pub fn completion_impact(
        &self,
        request: &orbit_runtime::task_impact::CompletionImpactRequest,
    ) -> Result<orbit_runtime::task_impact::CompletionImpact, ClientError> {
        self.runtime
            .preview_task_completion(request)
            .map_err(|error| ClientError {
                code: ClientErrorCode::Unavailable,
                message: format!("{error:?}"),
            })
    }

    /// Read-only dependency preview over the currently accepted graph.
    pub fn preview_task_dependency(
        &self,
        request: &orbit_runtime::task_dependency_preview::DependencyPreviewRequest,
    ) -> Result<orbit_runtime::task_dependency_preview::DependencyPreview, ClientError> {
        let (revision, tasks) = self.runtime.tasks().map_err(|error| ClientError {
            code: ClientErrorCode::Unavailable,
            message: format!("{error:?}"),
        })?;
        let edges = self
            .runtime
            .task_dependency_edges()
            .map_err(|error| ClientError {
                code: ClientErrorCode::Unavailable,
                message: format!("{error:?}"),
            })?;
        orbit_runtime::task_dependency_preview::preview_dependency(
            revision, &tasks, &edges, request,
        )
        .map_err(|message| ClientError {
            code: ClientErrorCode::InvalidInput,
            message: message.to_owned(),
        })
    }

    /// Read-only schedule proposal over the currently accepted state.
    pub fn schedule_plan(
        &self,
        request: &orbit_runtime::schedule_planning::SchedulePlanRequest,
    ) -> Result<orbit_runtime::schedule_planning::SchedulePlan, ClientError> {
        self.runtime
            .schedule_plan(request)
            .map_err(|error| ClientError {
                code: ClientErrorCode::Unavailable,
                message: format!("{error:?}"),
            })
    }

    pub fn capabilities(&self) -> ClientCapabilities {
        ClientCapabilities::default()
    }

    pub fn bridge_contract(&self) -> ClientBridgeContract {
        ClientBridgeContract::default()
    }

    pub fn platform_capabilities(&self, platform: ClientPlatform) -> ClientPlatformCapabilities {
        ClientPlatformCapabilities::for_platform(platform)
    }

    pub fn workspace_summary(&self) -> Result<WorkspaceSummaryDto, ClientError> {
        let projects = self
            .runtime
            .application_queries()
            .projects()
            .map_err(ClientError::from)?;
        let tasks = self
            .runtime
            .application_queries()
            .tasks()
            .map_err(ClientError::from)?;
        let habits = self
            .runtime
            .application_queries()
            .habits()
            .map_err(ClientError::from)?;
        let routines = self
            .runtime
            .application_queries()
            .routines()
            .map_err(ClientError::from)?;
        let knowledge = self.list_knowledge()?;
        // Legacy SQLite resource foreign keys cannot index real Markdown
        // files. File-backed attachments are tracked separately.
        let knowledge_resources = 0usize;
        Ok(WorkspaceSummaryDto {
            schema_version: self
                .runtime
                .application_schema_version()
                .map_err(ClientError::from)?,
            projects: projects.len(),
            tasks: tasks.len(),
            habits: habits.len(),
            routines: routines.len(),
            knowledge_items: knowledge.len(),
            knowledge_resources,
        })
    }

    pub fn list_projects(&self) -> Result<Vec<ProjectDto>, ClientError> {
        Ok(self
            .runtime
            .application_queries()
            .projects()
            .map_err(ClientError::from)?
            .iter()
            .map(ProjectDto::from)
            .collect())
    }

    pub fn list_tasks(&self) -> Result<Vec<TaskDto>, ClientError> {
        Ok(self
            .runtime
            .application_queries()
            .tasks()
            .map_err(ClientError::from)?
            .iter()
            .map(TaskDto::from)
            .collect())
    }

    /// One task with its rich details.
    ///
    /// Reads through the same local projection as `list_tasks`, so callers can
    /// use one uniform method in local and connected mode. An unknown or
    /// malformed id is a typed error rather than an empty task.
    pub fn task_detail(&self, id: &str) -> Result<TaskDto, ClientError> {
        let task_id = TaskId::new(id.to_owned()).map_err(invalid_input)?;
        self.list_tasks()?
            .into_iter()
            .find(|task| task.id == task_id.as_str())
            .ok_or_else(|| ClientError {
                code: ClientErrorCode::NotFound,
                message: format!("task {id} not found"),
            })
    }

    pub fn list_knowledge(&self) -> Result<Vec<KnowledgeItemDto>, ClientError> {
        let notes = self
            .runtime
            .markdown_knowledge_index()
            .map_err(markdown_client_error)?;
        notes
            .iter()
            .map(|info| {
                let (_, body) = self
                    .runtime
                    .markdown_by_id(&info.id)
                    .map_err(markdown_client_error)?
                    .ok_or_else(|| {
                        markdown_client_error("Markdown note disappeared during discovery")
                    })?;
                file_knowledge_bridge::dto(info, &body)
            })
            .collect()
    }

    pub fn create_project(
        &mut self,
        request: CreateProjectRequest,
    ) -> Result<ProjectDto, ClientError> {
        let id = ProjectId::new(request.id.unwrap_or_else(|| generate_id("project")))
            .map_err(invalid_input)?;
        let project = {
            let revision = self.runtime.snapshot().map_err(authority_client_error)?.0;
            let (_, _, _accepted_id) = self
                .runtime
                .create_project_accepted(
                    "orbit-client",
                    &generate_id("command"),
                    revision,
                    CreateProjectWithActivityCommand {
                        project: CreateProjectCommand {
                            id: id.clone(),
                            name: request.name,
                        },
                        activity_id: new_activity_id()?,
                        origin: ActivityOrigin::Human,
                        summary: format!("Project {} created", id),
                    },
                )
                .map_err(authority_client_error)?;
            self.runtime
                .application_queries()
                .project(&id)
                .map_err(ClientError::from)?
        };
        Ok(ProjectDto::from(&project))
    }

    pub fn set_project_status(
        &mut self,
        request: SetProjectStatusRequest,
    ) -> Result<ProjectDto, ClientError> {
        let id = ProjectId::new(request.id).map_err(invalid_input)?;
        let status = parse_project_status(&request.status)?;
        let project = {
            let revision = self.runtime.snapshot().map_err(authority_client_error)?.0;
            let (_, _, _accepted_id) = self
                .runtime
                .transition_project_accepted(
                    "orbit-client",
                    &generate_id("command"),
                    revision,
                    TransitionProjectWithActivityCommand {
                        project: TransitionProjectCommand {
                            id: id.clone(),
                            status,
                        },
                        activity_id: new_activity_id()?,
                        origin: ActivityOrigin::Human,
                        summary: format!("Project {} status changed to {}", id, status.as_str()),
                    },
                )
                .map_err(authority_client_error)?;
            self.runtime
                .application_queries()
                .project(&id)
                .map_err(ClientError::from)?
        };
        Ok(ProjectDto::from(&project))
    }

    pub fn configure_project_strategy(
        &mut self,
        request: ConfigureProjectStrategyRequest,
    ) -> Result<ProjectDto, ClientError> {
        let id = ProjectId::new(request.id).map_err(invalid_input)?;
        let mode = ProjectMode::parse(&request.mode).map_err(invalid_input)?;
        let signals = request
            .progress_signals
            .into_iter()
            .map(|value| ProjectProgressSignal::parse(&value).map_err(invalid_input))
            .collect::<Result<Vec<_>, _>>()?;
        let criteria = request
            .success_criteria
            .into_iter()
            .map(|criterion| {
                let status =
                    SuccessCriterionStatus::parse(&criterion.status).map_err(invalid_input)?;
                let mut created = ProjectSuccessCriterion::new(
                    criterion.key,
                    criterion.description,
                    criterion.required,
                )
                .map_err(invalid_input)?;
                created.set_status(status);
                Ok::<_, ClientError>(created)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let project = {
            let revision = self.runtime.snapshot().map_err(authority_client_error)?.0;
            let (_, _, _accepted_id) = self
                .runtime
                .configure_project_strategy_accepted(
                    "orbit-client",
                    &generate_id("command"),
                    revision,
                    ConfigureProjectStrategyWithActivityCommand {
                        project: ConfigureProjectStrategyCommand {
                            id: id.clone(),
                            mode,
                            objective: request.objective,
                            progress_signals: signals,
                            success_criteria: criteria,
                        },
                        activity_id: new_activity_id()?,
                        origin: ActivityOrigin::Human,
                        summary: format!("Project {} strategy configured as {}", id, mode.as_str()),
                    },
                )
                .map_err(authority_client_error)?;
            self.runtime
                .application_queries()
                .project(&id)
                .map_err(ClientError::from)?
        };
        Ok(ProjectDto::from(&project))
    }

    pub fn set_project_criterion_status(
        &mut self,
        request: SetProjectCriterionStatusRequest,
    ) -> Result<ProjectDto, ClientError> {
        let project_id = ProjectId::new(request.project_id).map_err(invalid_input)?;
        let status = SuccessCriterionStatus::parse(&request.status).map_err(invalid_input)?;
        let project = {
            let revision = self.runtime.snapshot().map_err(authority_client_error)?.0;
            let (_, _, _accepted_id) = self
                .runtime
                .set_project_criterion_status_accepted(
                    "orbit-client",
                    &generate_id("command"),
                    revision,
                    SetProjectCriterionStatusWithActivityCommand {
                        project_id: project_id.clone(),
                        criterion_key: request.criterion_key.clone(),
                        status,
                        activity_id: new_activity_id()?,
                        origin: ActivityOrigin::Human,
                        summary: format!(
                            "Project {} criterion {} changed to {}",
                            project_id,
                            request.criterion_key,
                            status.as_str()
                        ),
                    },
                )
                .map_err(authority_client_error)?;
            self.runtime
                .application_queries()
                .project(&project_id)
                .map_err(ClientError::from)?
        };
        Ok(ProjectDto::from(&project))
    }

    pub fn list_project_evidence(
        &self,
        project_id: String,
    ) -> Result<Vec<ProjectEvidenceBindingDto>, ClientError> {
        let project_id = ProjectId::new(project_id).map_err(invalid_input)?;
        let bindings = self
            .runtime
            .application_queries()
            .project_evidence(&project_id)?;
        Ok(bindings
            .into_iter()
            .map(|b| match b {
                orbit_application::ProjectEvidenceBinding::Habit(id) => ProjectEvidenceBindingDto {
                    kind: "habit".into(),
                    id: id.to_string(),
                },
                orbit_application::ProjectEvidenceBinding::Metric(id) => {
                    ProjectEvidenceBindingDto {
                        kind: "metric".into(),
                        id,
                    }
                }
            })
            .collect())
    }

    pub fn change_project_evidence(
        &mut self,
        project_id: String,
        kind: String,
        evidence_id: String,
        bind: bool,
    ) -> Result<(), ClientError> {
        let project_id = ProjectId::new(project_id).map_err(invalid_input)?;
        let binding = match kind.as_str() {
            "habit" => orbit_application::ProjectEvidenceBinding::Habit(
                HabitId::new(evidence_id).map_err(invalid_input)?,
            ),
            "metric" => orbit_application::ProjectEvidenceBinding::Metric(evidence_id),
            _ => {
                return Err(ClientError {
                    code: ClientErrorCode::InvalidInput,
                    message: "kind must be habit or metric".into(),
                });
            }
        };
        let cmd = orbit_application::ChangeProjectEvidenceCommand {
            project_id,
            binding,
        };
        let revision = self.runtime.snapshot().map_err(authority_client_error)?.0;
        self.runtime
            .change_project_evidence_accepted(
                "orbit-client",
                &generate_id("command"),
                revision,
                bind,
                cmd,
            )
            .map_err(authority_client_error)?;
        Ok(())
    }

    pub fn set_metric_target(&mut self, target: MetricTargetDto) -> Result<(), ClientError> {
        let direction = match target.direction.as_str() {
            "at_least" => orbit_progress::MetricTargetDirection::AtLeast,
            "at_most" => orbit_progress::MetricTargetDirection::AtMost,
            _ => {
                return Err(ClientError {
                    code: ClientErrorCode::InvalidInput,
                    message: "invalid metric direction".into(),
                });
            }
        };
        {
            let revision = self.runtime.snapshot().map_err(authority_client_error)?.0;
            self.runtime
                .set_metric_target_accepted(
                    "orbit-client",
                    &generate_id("command"),
                    revision,
                    orbit_application::MetricTargetRecord {
                        metric_id: target.metric_id,
                        baseline: target.baseline,
                        target: target.target,
                        direction,
                    },
                )
                .map_err(authority_client_error)?;
        };
        Ok(())
    }

    pub fn record_metric_observation(
        &mut self,
        metric_id: String,
        observed_at_ms: i64,
        value: i64,
    ) -> Result<(), ClientError> {
        {
            let revision = self.runtime.snapshot().map_err(authority_client_error)?.0;
            self.runtime
                .record_metric_observation_accepted(
                    "orbit-client",
                    &generate_id("command"),
                    revision,
                    orbit_application::MetricObservation {
                        metric_id,
                        observed_at_ms,
                        value,
                    },
                )
                .map_err(authority_client_error)?;
        };
        Ok(())
    }

    pub fn project_evidence_progress(
        &self,
        project_id: String,
        start_ms: i64,
        end_ms: i64,
        as_of_ms: i64,
    ) -> Result<ProjectEvidenceProgressDto, ClientError> {
        let id = ProjectId::new(project_id).map_err(invalid_input)?;
        let period = orbit_domain::HabitPeriod::new(start_ms, end_ms).map_err(invalid_input)?;
        let result = self
            .runtime
            .application_queries()
            .mode_aware_project_health(
                &id,
                &[period],
                as_of_ms,
                10080,
                orbit_progress::HealthPolicy::default(),
            )?;
        Ok(ProjectEvidenceProgressDto {
            project_id: id.to_string(),
            task_progress_bp: result.canonical.rollup.progress.value(),
            mode_progress_bp: result.selected_progress.map(|v| v.value()),
            evidence_progress_bp: result.bound_evidence.evidence_progress.map(|v| v.value()),
            habit_progress: result
                .bound_evidence
                .habits
                .habits
                .into_iter()
                .map(|h| (h.habit_id.to_string(), h.consistency.map(|p| p.value())))
                .collect(),
            metric_progress: result
                .bound_evidence
                .metrics
                .into_iter()
                .map(|m| (m.metric_id, m.evaluation.map(|e| e.progress.value())))
                .collect(),
            available_signals: result
                .available_signals
                .iter()
                .map(|s| s.as_str().into())
                .collect(),
            unavailable_signals: result
                .unavailable_signals
                .iter()
                .map(|s| s.as_str().into())
                .collect(),
            completion_ready: result.canonical.completion_ready,
        })
    }

    pub fn create_task(&mut self, request: CreateTaskRequest) -> Result<TaskDto, ClientError> {
        let id = TaskId::new(request.id.unwrap_or_else(|| generate_id("task")))
            .map_err(invalid_input)?;
        let project_id = request
            .project_id
            .map(ProjectId::new)
            .transpose()
            .map_err(invalid_input)?;
        let milestone_id = request
            .milestone_id
            .map(orbit_domain::MilestoneId::new)
            .transpose()
            .map_err(invalid_input)?;
        let scope = TaskScope::try_new(project_id, milestone_id).map_err(invalid_input)?;
        let task = {
            let revision = self.runtime.snapshot().map_err(authority_client_error)?.0;
            let (_, _, _accepted_id) = self
                .runtime
                .create_task_accepted(
                    "orbit-client",
                    &generate_id("command"),
                    revision,
                    CreateTaskWithActivityCommand {
                        task: CreateTaskCommand {
                            id: id.clone(),
                            title: request.title,
                            scope,
                        },
                        activity_id: new_activity_id()?,
                        origin: ActivityOrigin::Human,
                        summary: format!("Task {} created", id),
                    },
                )
                .map_err(authority_client_error)?;
            self.runtime
                .application_queries()
                .task(&id)
                .map_err(ClientError::from)?
        };
        Ok(TaskDto::from(&task))
    }

    pub fn set_task_status(
        &mut self,
        request: SetTaskStatusRequest,
    ) -> Result<TaskDto, ClientError> {
        let id = TaskId::new(request.id).map_err(invalid_input)?;
        let status = parse_task_status(&request.status)?;
        let task = {
            let revision = self.runtime.snapshot().map_err(authority_client_error)?.0;
            let (_, _, _accepted_id) = self
                .runtime
                .transition_task_accepted(
                    "orbit-client",
                    &generate_id("command"),
                    revision,
                    TransitionTaskWithActivityCommand {
                        task: TransitionTaskCommand {
                            id: id.clone(),
                            status,
                        },
                        activity_id: new_activity_id()?,
                        origin: ActivityOrigin::Human,
                        summary: format!("Task {} status changed to {}", id, status.as_str()),
                    },
                )
                .map_err(authority_client_error)?;
            self.runtime
                .application_queries()
                .task(&id)
                .map_err(ClientError::from)?
        };
        Ok(TaskDto::from(&task))
    }

    /// Rename a task through the canonical task command family.
    pub fn rename_task(&mut self, id: String, title: String) -> Result<TaskDto, ClientError> {
        if title.trim().is_empty() || title.len() > 4096 {
            return Err(invalid_input(
                "task title must be non-empty and at most 4096 bytes",
            ));
        }
        self.apply_task_command(orbit_runtime::domain_commands::TaskAction::Rename {
            task_id: id,
            title,
        })
    }

    /// Add a prerequisite edge. The authority rejects self/cyclic/missing edges.
    pub fn add_task_prerequisite(
        &mut self,
        task_id: String,
        prerequisite_id: String,
    ) -> Result<TaskDto, ClientError> {
        self.apply_task_command(
            orbit_runtime::domain_commands::TaskAction::AddPrerequisite {
                task_id,
                prerequisite_id,
            },
        )
    }

    pub fn assign_task_project(
        &mut self,
        task_id: String,
        project_id: Option<String>,
    ) -> Result<TaskDto, ClientError> {
        self.apply_task_command(orbit_runtime::domain_commands::TaskAction::AssignProject {
            task_id,
            project_id,
        })
    }

    pub fn assign_task_milestone(
        &mut self,
        task_id: String,
        milestone_id: Option<String>,
    ) -> Result<TaskDto, ClientError> {
        self.apply_task_command(
            orbit_runtime::domain_commands::TaskAction::AssignMilestone {
                task_id,
                milestone_id,
            },
        )
    }

    pub fn remove_task_prerequisite(
        &mut self,
        task_id: String,
        prerequisite_id: String,
    ) -> Result<TaskDto, ClientError> {
        self.apply_task_command(
            orbit_runtime::domain_commands::TaskAction::RemovePrerequisite {
                task_id,
                prerequisite_id,
            },
        )
    }

    fn apply_task_command(
        &mut self,
        action: orbit_runtime::domain_commands::TaskAction,
    ) -> Result<TaskDto, ClientError> {
        let revision = self.runtime.snapshot().map_err(authority_client_error)?.0;
        let result = self
            .runtime
            .task_command(
                "orbit-client",
                orbit_runtime::domain_commands::TaskCommand {
                    command_id: generate_id("command"),
                    expected_revision: revision,
                    action,
                },
            )
            .map_err(authority_client_error)?;
        let id = TaskId::new(result.task.id).map_err(invalid_input)?;
        let task = self
            .runtime
            .application_queries()
            .task(&id)
            .map_err(ClientError::from)?;
        Ok(TaskDto::from(&task))
    }

    pub fn set_task_planning(
        &mut self,
        request: SetTaskPlanningRequest,
    ) -> Result<TaskDto, ClientError> {
        let id = TaskId::new(request.id).map_err(invalid_input)?;
        let planning = TaskPlanning::new(
            request.estimated_minutes,
            request.earliest_start_at_ms,
            request.deadline_at_ms,
        )
        .map_err(invalid_input)?;
        let task = {
            let revision = self.runtime.snapshot().map_err(authority_client_error)?.0;
            let (_, _, _accepted_id) = self
                .runtime
                .set_task_planning_accepted(
                    "orbit-client",
                    &generate_id("command"),
                    revision,
                    SetTaskPlanningWithActivityCommand {
                        task: SetTaskPlanningCommand {
                            id: id.clone(),
                            planning,
                        },
                        activity_id: new_activity_id()?,
                        origin: ActivityOrigin::Human,
                        summary: format!("Task {} planning updated", id),
                    },
                )
                .map_err(authority_client_error)?;
            self.runtime
                .application_queries()
                .task(&id)
                .map_err(ClientError::from)?
        };
        Ok(TaskDto::from(&task))
    }

    /// Legacy DTO-facing APIs delegate to the file-authoritative knowledge
    /// command path. They do not write `knowledge_items` in SqliteStore.
    pub fn create_knowledge(
        &mut self,
        request: CreateKnowledgeRequest,
    ) -> Result<KnowledgeItemDto, ClientError> {
        let id = KnowledgeItemId::new(request.id.unwrap_or_else(|| generate_id("knowledge")))
            .map_err(invalid_input)?;
        let kind = parse_knowledge_kind(&request.kind)?;
        let item = KnowledgeItemDto {
            id: id.as_str().to_owned(),
            kind: kind.as_str().to_owned(),
            title: request.title,
            body_markdown: request.body_markdown,
            status: "active".into(),
            provenance: "human".into(),
            updated_at_ms: now_ms(),
        };
        self.runtime
            .execute_folder_create(
                "local",
                "folder-default-inbox",
                orbit_runtime::folder_commands::FolderCreate {
                    path: "Inbox".into(),
                },
            )
            .map_err(markdown_client_error)?;
        let markdown = file_knowledge_bridge::render(&item, None);
        self.runtime
            .execute_markdown_command(
                "local",
                &generate_id("create-note"),
                orbit_runtime::markdown_commands::MarkdownCommand::Create {
                    note_id: item.id.clone(),
                    path: format!("Inbox/{}.md", item.id),
                    body: markdown,
                },
            )
            .map_err(markdown_client_error)?;
        Ok(item)
    }

    pub fn update_knowledge(
        &mut self,
        request: UpdateKnowledgeRequest,
    ) -> Result<KnowledgeItemDto, ClientError> {
        let (info, existing) = self
            .runtime
            .markdown_by_id(&request.id)
            .map_err(markdown_client_error)?
            .ok_or_else(|| ClientError {
                code: ClientErrorCode::NotFound,
                message: "Markdown note not found".into(),
            })?;
        let mut item = file_knowledge_bridge::dto(&info, &existing)?;
        item.title = request.title;
        item.body_markdown = request.body_markdown;
        item.updated_at_ms = now_ms();
        let updated = file_knowledge_bridge::replace_preserving_metadata(&existing, &item)?;
        self.runtime
            .execute_markdown_command(
                "local",
                &generate_id("edit-note"),
                orbit_runtime::markdown_commands::MarkdownCommand::Update {
                    note_id: item.id.clone(),
                    expected: existing,
                    replacement: updated,
                },
            )
            .map_err(markdown_client_error)?;
        Ok(item)
    }

    pub fn set_knowledge_status(
        &mut self,
        request: SetKnowledgeStatusRequest,
    ) -> Result<KnowledgeItemDto, ClientError> {
        let next = parse_knowledge_status(&request.status)?;
        let (info, existing) = self
            .runtime
            .markdown_by_id(&request.id)
            .map_err(markdown_client_error)?
            .ok_or_else(|| ClientError {
                code: ClientErrorCode::NotFound,
                message: "Markdown note not found".into(),
            })?;
        let mut item = file_knowledge_bridge::dto(&info, &existing)?;
        let current = parse_knowledge_status(&item.status)?;
        if !current.can_transition_to(next) {
            return Err(ClientError {
                code: ClientErrorCode::InvalidInput,
                message: "invalid knowledge status transition".into(),
            });
        }
        item.status = next.as_str().into();
        item.updated_at_ms = now_ms();
        let updated = file_knowledge_bridge::replace_preserving_metadata(&existing, &item)?;
        self.runtime
            .execute_markdown_command(
                "local",
                &generate_id("status-note"),
                orbit_runtime::markdown_commands::MarkdownCommand::Update {
                    note_id: item.id.clone(),
                    expected: existing,
                    replacement: updated,
                },
            )
            .map_err(markdown_client_error)?;
        Ok(item)
    }

    pub fn list_habits(&self) -> Result<Vec<HabitDto>, ClientError> {
        Ok(self
            .runtime
            .application_queries()
            .habits()
            .map_err(ClientError::from)?
            .iter()
            .map(HabitDto::from)
            .collect())
    }

    pub fn create_habit(&mut self, request: CreateHabitRequest) -> Result<HabitDto, ClientError> {
        let id = HabitId::new(request.id.unwrap_or_else(|| generate_id("habit")))
            .map_err(invalid_input)?;
        let cadence = parse_habit_cadence(&request.cadence)?;
        let habit = {
            let revision = self.runtime.snapshot().map_err(authority_client_error)?.0;
            let (_, _, _accepted_id) = self
                .runtime
                .create_habit_accepted(
                    "orbit-client",
                    &generate_id("command"),
                    revision,
                    CreateHabitWithActivityCommand {
                        habit: CreateHabitCommand {
                            id: id.clone(),
                            name: request.name,
                            cadence,
                        },
                        activity_id: new_activity_id()?,
                        origin: ActivityOrigin::Human,
                        summary: format!("Habit {} created", id),
                    },
                )
                .map_err(authority_client_error)?;
            self.runtime
                .application_queries()
                .habit(&id)
                .map_err(ClientError::from)?
        };
        Ok(HabitDto::from(&habit))
    }

    pub fn set_habit_active(
        &mut self,
        request: SetHabitActiveRequest,
    ) -> Result<HabitDto, ClientError> {
        let id = HabitId::new(request.id).map_err(invalid_input)?;
        let habit = {
            let revision = self.runtime.snapshot().map_err(authority_client_error)?.0;
            let (_, _, _accepted_id) = self
                .runtime
                .set_habit_active_accepted(
                    "orbit-client",
                    &generate_id("command"),
                    revision,
                    SetHabitActiveWithActivityCommand {
                        id: id.clone(),
                        active: request.active,
                        activity_id: new_activity_id()?,
                        origin: ActivityOrigin::Human,
                        summary: format!(
                            "Habit {} {}",
                            id,
                            if request.active { "resumed" } else { "paused" }
                        ),
                    },
                )
                .map_err(authority_client_error)?;
            self.runtime
                .application_queries()
                .habit(&id)
                .map_err(ClientError::from)?
        };
        Ok(HabitDto::from(&habit))
    }

    pub fn record_habit_occurrence(
        &mut self,
        request: RecordHabitOccurrenceRequest,
    ) -> Result<HabitOccurrenceDto, ClientError> {
        let habit_id = HabitId::new(request.habit_id).map_err(invalid_input)?;
        let id = HabitOccurrenceId::new(
            request
                .id
                .unwrap_or_else(|| generate_id("habit-occurrence")),
        )
        .map_err(invalid_input)?;
        let occurrence = {
            let revision = self.runtime.snapshot().map_err(authority_client_error)?.0;
            let (_, _, accepted_id) = self
                .runtime
                .record_habit_occurrence_accepted(
                    "orbit-client",
                    &generate_id("command"),
                    revision,
                    RecordHabitOccurrenceWithActivityCommand {
                        id: id.clone(),
                        habit_id: habit_id.clone(),
                        occurred_at_ms: request.occurred_at_ms,
                        duration_minutes: request.duration_minutes,
                        activity_id: new_activity_id()?,
                        origin: ActivityOrigin::Human,
                        summary: format!("Habit {} completed", habit_id),
                    },
                )
                .map_err(authority_client_error)?;
            self.runtime
                .application_queries()
                .habit_occurrences(&habit_id)
                .map_err(ClientError::from)?
                .into_iter()
                .find(|item| item.id().as_str() == accepted_id.as_str())
                .ok_or_else(|| ClientError {
                    code: ClientErrorCode::NotFound,
                    message: format!("accepted habit occurrence {accepted_id} not found"),
                })?
        };
        Ok(HabitOccurrenceDto::from(&occurrence))
    }

    pub fn habit_history(&self, habit_id: String) -> Result<Vec<HabitOccurrenceDto>, ClientError> {
        let habit_id = HabitId::new(habit_id).map_err(invalid_input)?;
        Ok(self
            .runtime
            .application_queries()
            .habit_occurrences(&habit_id)
            .map_err(ClientError::from)?
            .iter()
            .map(HabitOccurrenceDto::from)
            .collect())
    }

    pub fn list_schedule(
        &self,
        start_ms: i64,
        end_ms: i64,
    ) -> Result<Vec<ScheduleBlockDto>, ClientError> {
        if end_ms <= start_ms {
            return Err(ClientError {
                code: ClientErrorCode::InvalidInput,
                message: "schedule end must be after start".to_owned(),
            });
        }
        Ok(self
            .runtime
            .application_queries()
            .schedule_between(start_ms, end_ms)
            .map_err(ClientError::from)?
            .iter()
            .map(ScheduleBlockDto::from)
            .collect())
    }

    pub fn create_schedule_block(
        &mut self,
        request: CreateScheduleBlockRequest,
    ) -> Result<ScheduleBlockDto, ClientError> {
        let id = ScheduleBlockId::new(request.id.unwrap_or_else(|| generate_id("schedule")))
            .map_err(invalid_input)?;
        let flexibility = parse_schedule_flexibility(&request.flexibility)?;
        let target = parse_schedule_target(&request.target_kind, request.target_id)?;
        let block = {
            let revision = self.runtime.snapshot().map_err(authority_client_error)?.0;
            let (_, _, _accepted_id) = self
                .runtime
                .create_schedule_block_accepted(
                    "orbit-client",
                    &generate_id("command"),
                    revision,
                    CreateScheduleBlockWithActivityCommand {
                        block: CreateScheduleBlockCommand {
                            id: id.clone(),
                            title: request.title,
                            starts_at_ms: request.starts_at_ms,
                            duration_minutes: request.duration_minutes,
                            flexibility,
                            target,
                        },
                        activity_id: new_activity_id()?,
                        origin: ActivityOrigin::Human,
                        summary: format!("Schedule block {} created", id),
                    },
                )
                .map_err(authority_client_error)?;
            self.runtime
                .application_queries()
                .schedule_block(&id)
                .map_err(ClientError::from)?
        };
        Ok(ScheduleBlockDto::from(&block))
    }

    /// Authoritatively move an existing movable block. The application service
    /// enforces fixed-block immutability and emits semantic activity atomically.
    pub fn reschedule_schedule_block(
        &mut self,
        id: String,
        starts_at_ms: i64,
    ) -> Result<ScheduleBlockDto, ClientError> {
        let id = ScheduleBlockId::new(id).map_err(invalid_input)?;
        let current = self
            .runtime
            .application_queries()
            .schedule_block(&id)
            .map_err(ClientError::from)?;
        let end = starts_at_ms
            .checked_add(i64::from(current.duration_minutes()) * 60_000)
            .ok_or(ClientError {
                code: ClientErrorCode::InvalidInput,
                message: "rescheduled block end overflows timestamp range".into(),
            })?;
        if current.flexibility() == ScheduleFlexibility::Fixed {
            return Err(ClientError {
                code: ClientErrorCode::InvalidInput,
                message: "fixed blocks cannot be moved".into(),
            });
        }
        if !self
            .schedule_conflicts(starts_at_ms, end, Some(id.as_str()))?
            .is_empty()
        {
            return Err(ClientError {
                code: ClientErrorCode::InvalidInput,
                message: "the proposed schedule overlaps an existing block".into(),
            });
        }
        let revision = self.runtime.snapshot().map_err(authority_client_error)?.0;
        self.runtime
            .reschedule_block_accepted(
                "orbit-client",
                &generate_id("command"),
                revision,
                RescheduleBlockWithActivityCommand {
                    id: id.clone(),
                    starts_at_ms,
                    activity_id: new_activity_id()?,
                    origin: ActivityOrigin::Human,
                    summary: format!("Schedule block {} rescheduled", id),
                },
            )
            .map_err(authority_client_error)?;
        let changed = self
            .runtime
            .application_queries()
            .schedule_block(&id)
            .map_err(ClientError::from)?;
        Ok(ScheduleBlockDto::from(&changed))
    }

    /// Find blocks that intersect [start_ms, end_ms). Back-to-back allocations
    /// are not conflicts; the optional ID excludes the block being moved.
    pub fn schedule_conflicts(
        &self,
        start_ms: i64,
        end_ms: i64,
        exclude_id: Option<&str>,
    ) -> Result<Vec<ScheduleBlockDto>, ClientError> {
        if end_ms <= start_ms {
            return Err(ClientError {
                code: ClientErrorCode::InvalidInput,
                message: "schedule interval must have positive length".into(),
            });
        }
        let blocks = self
            .runtime
            .application_queries()
            .schedule_between(start_ms, end_ms)
            .map_err(ClientError::from)?;
        Ok(blocks
            .iter()
            .filter(|b| {
                b.id().as_str() != exclude_id.unwrap_or("")
                    && b.starts_at_ms() < end_ms
                    && b.ends_at_ms() > start_ms
            })
            .map(ScheduleBlockDto::from)
            .collect())
    }

    /// Safe create rejects an occupied interval before entering the accepted
    /// application command path. This is a host-local convenience; the
    /// authoritative transaction still owns the final state validation.
    pub fn create_schedule_block_without_conflict(
        &mut self,
        request: CreateScheduleBlockRequest,
    ) -> Result<ScheduleBlockDto, ClientError> {
        if request.duration_minutes == 0 {
            return Err(ClientError {
                code: ClientErrorCode::InvalidInput,
                message: "duration must be positive".into(),
            });
        }
        let end = request
            .starts_at_ms
            .checked_add(i64::from(request.duration_minutes) * 60_000)
            .ok_or(ClientError {
                code: ClientErrorCode::InvalidInput,
                message: "schedule end overflows timestamp range".into(),
            })?;
        if !self
            .schedule_conflicts(request.starts_at_ms, end, None)?
            .is_empty()
        {
            return Err(ClientError {
                code: ClientErrorCode::InvalidInput,
                message: "the proposed schedule overlaps an existing block".into(),
            });
        }
        self.create_schedule_block(request)
    }

    pub fn today_snapshot(
        &self,
        start_ms: i64,
        end_ms: i64,
    ) -> Result<TodaySnapshotDto, ClientError> {
        if end_ms <= start_ms {
            return Err(ClientError {
                code: ClientErrorCode::InvalidInput,
                message: "today window end must be after start".to_owned(),
            });
        }
        let schedule = self.list_schedule(start_ms, end_ms)?;
        let tasks = self.list_tasks()?;
        let habits = self.list_habits()?;
        Ok(TodaySnapshotDto {
            start_ms,
            end_ms,
            schedule,
            tasks,
            habits,
        })
    }

    pub fn preview_task_plan(
        &self,
        start_ms: i64,
        end_ms: i64,
        policy: PlanningPolicyDto,
    ) -> Result<TaskPlanProposalDto, ClientError> {
        let window = PlanningWindow::new(start_ms, end_ms).map_err(invalid_input)?;
        let capacity = capacity_policy(&policy)?;
        let buffers = buffer_policy(&policy);
        let blocks = self
            .runtime
            .application_queries()
            .schedule_between(start_ms, end_ms)
            .map_err(ClientError::from)?;
        let tasks = self
            .runtime
            .application_queries()
            .tasks()
            .map_err(ClientError::from)?;
        let availability = [AvailabilitySlot { start_ms, end_ms }];
        let proposal = propose_tasks_with_availability_and_buffers(
            window,
            &blocks,
            &tasks,
            capacity,
            &availability,
            buffers,
        );
        Ok(task_plan_dto(start_ms, end_ms, policy, proposal))
    }

    /// Review a proposed plan and freeze its concrete schedule identities.
    /// No schedule blocks or activities are created until `approve_task_plan`.
    pub fn review_task_plan(
        &self,
        dto: TaskPlanProposalDto,
    ) -> Result<PreparedTaskPlan, ClientError> {
        let window = PlanningWindow::new(dto.start_ms, dto.end_ms).map_err(invalid_input)?;
        let policy = capacity_policy(&dto.policy)?;
        let buffers = buffer_policy(&dto.policy);
        let blocks = self
            .runtime
            .application_queries()
            .schedule_between(dto.start_ms, dto.end_ms)
            .map_err(ClientError::from)?;
        let tasks = self
            .runtime
            .application_queries()
            .tasks()
            .map_err(ClientError::from)?;
        let validated = validate_proposal_with_buffers(
            window,
            &blocks,
            &tasks,
            policy,
            buffers,
            task_plan_from_dto(&dto)?,
        )
        .map_err(|violations| ClientError {
            code: ClientErrorCode::InvalidInput,
            message: format!("plan rejected: {violations:?}"),
        })?;
        let identities = validated
            .proposal()
            .scheduled
            .iter()
            .map(|item| {
                Ok(PlanApplicationIdentity {
                    task_id: item.task_id.clone(),
                    schedule_block_id: ScheduleBlockId::new(generate_id("schedule"))
                        .map_err(invalid_input)?,
                    activity_id: new_activity_id()?,
                })
            })
            .collect::<Result<Vec<_>, ClientError>>()?;
        let command = ApplyValidatedPlanCommand {
            window,
            policy,
            fixed_buffers: buffers,
            proposal: validated,
            identities,
            origin: ActivityOrigin::Human,
        };
        let actor = "orbit-client".to_owned();
        let command_id = generate_id("command");
        let revision = self.runtime.snapshot().map_err(authority_client_error)?.0;
        let review = self
            .runtime
            .review_task_plan_approval(&actor, &command_id, revision, &command)
            .map_err(authority_client_error)?;
        Ok(PreparedTaskPlan { review, command })
    }

    /// Execute the exact reviewed plan only after explicit user approval.
    /// Consuming the prepared plan prevents accidental alteration of the
    /// schedule command between review and submission.
    pub fn approve_task_plan(
        &mut self,
        prepared: PreparedTaskPlan,
        approved: bool,
    ) -> Result<Vec<ScheduleBlockDto>, ClientError> {
        let actor = prepared.review.actor.clone();
        let command_id = prepared.review.command_id.clone();
        let revision = prepared.review.expected_revision;
        let approval = orbit_runtime::planning_approval::PlanApproval {
            review: prepared.review,
            approved,
        };
        let (_, _, ids) = self
            .runtime
            .apply_approved_task_plan_accepted(
                approval,
                &actor,
                &command_id,
                revision,
                prepared.command,
            )
            .map_err(authority_client_error)?;
        ids.into_iter()
            .map(|raw| {
                let id = ScheduleBlockId::new(raw).map_err(invalid_input)?;
                let block = self.runtime.application_queries().schedule_block(&id)?;
                Ok(ScheduleBlockDto::from(&block))
            })
            .collect()
    }

    pub fn apply_task_plan(
        &mut self,
        dto: TaskPlanProposalDto,
    ) -> Result<Vec<ScheduleBlockDto>, ClientError> {
        let window = PlanningWindow::new(dto.start_ms, dto.end_ms).map_err(invalid_input)?;
        let capacity = capacity_policy(&dto.policy)?;
        let buffers = buffer_policy(&dto.policy);
        let blocks = self
            .runtime
            .application_queries()
            .schedule_between(dto.start_ms, dto.end_ms)
            .map_err(ClientError::from)?;
        let tasks = self
            .runtime
            .application_queries()
            .tasks()
            .map_err(ClientError::from)?;
        let proposal = task_plan_from_dto(&dto)?;
        let validated =
            validate_proposal_with_buffers(window, &blocks, &tasks, capacity, buffers, proposal)
                .map_err(|violations| ClientError {
                    code: ClientErrorCode::InvalidInput,
                    message: format!("plan rejected: {violations:?}"),
                })?;
        let identities = validated
            .proposal()
            .scheduled
            .iter()
            .map(|item| {
                Ok(PlanApplicationIdentity {
                    task_id: item.task_id.clone(),
                    schedule_block_id: ScheduleBlockId::new(generate_id("schedule"))
                        .map_err(invalid_input)?,
                    activity_id: new_activity_id()?,
                })
            })
            .collect::<Result<Vec<_>, ClientError>>()?;
        let command = ApplyValidatedPlanCommand {
            window,
            policy: capacity,
            fixed_buffers: buffers,
            proposal: validated,
            identities,
            origin: ActivityOrigin::Human,
        };
        let revision = self.runtime.snapshot().map_err(authority_client_error)?.0;
        let (_, _, block_ids) = self
            .runtime
            .apply_task_plan_accepted("orbit-client", &generate_id("command"), revision, command)
            .map_err(authority_client_error)?;
        let mut created = Vec::with_capacity(block_ids.len());
        for id in block_ids {
            let block_id = ScheduleBlockId::new(id).map_err(invalid_input)?;
            created.push(
                self.runtime
                    .application_queries()
                    .schedule_block(&block_id)?,
            );
        }
        Ok(created.iter().map(ScheduleBlockDto::from).collect())
    }

    pub fn preview_habit_plan(
        &self,
        start_ms: i64,
        end_ms: i64,
        period_start_ms: i64,
        period_end_ms: i64,
        inputs: Vec<HabitPlanInputDto>,
        policy: PlanningPolicyDto,
    ) -> Result<HabitPlanProposalDto, ClientError> {
        if period_end_ms <= period_start_ms {
            return Err(ClientError {
                code: ClientErrorCode::InvalidInput,
                message: "habit period end must be after start".to_owned(),
            });
        }
        let window = PlanningWindow::new(start_ms, end_ms).map_err(invalid_input)?;
        let capacity = capacity_policy(&policy)?;
        let buffers = buffer_policy(&policy);
        let blocks = self
            .runtime
            .application_queries()
            .schedule_between(start_ms, end_ms)
            .map_err(ClientError::from)?;
        let habits = self
            .runtime
            .application_queries()
            .habits()
            .map_err(ClientError::from)?;
        let habit_map: std::collections::BTreeMap<_, _> =
            habits.iter().map(|h| (h.id().to_string(), h)).collect();
        let requests = inputs
            .iter()
            .map(|input| {
                let habit = habit_map.get(&input.habit_id).ok_or_else(|| ClientError {
                    code: ClientErrorCode::NotFound,
                    message: format!("habit not found: {}", input.habit_id),
                })?;
                Ok(HabitPlanRequest {
                    habit_id: habit.id().clone(),
                    cadence: habit.cadence(),
                    active: habit.is_active(),
                    completed_in_period: input.completed_in_period,
                    duration_per_occurrence_minutes: input.duration_per_occurrence_minutes,
                })
            })
            .collect::<Result<Vec<_>, ClientError>>()?;
        let availability = [AvailabilitySlot { start_ms, end_ms }];
        let proposal = propose_habits_with_availability_and_buffers(
            window,
            &blocks,
            &requests,
            capacity,
            &availability,
            buffers,
        );
        Ok(habit_plan_dto(
            start_ms,
            end_ms,
            period_start_ms,
            period_end_ms,
            policy,
            inputs,
            proposal,
        ))
    }

    pub fn apply_habit_plan(
        &mut self,
        dto: HabitPlanProposalDto,
    ) -> Result<Vec<ScheduleBlockDto>, ClientError> {
        let window = PlanningWindow::new(dto.start_ms, dto.end_ms).map_err(invalid_input)?;
        let capacity = capacity_policy(&dto.policy)?;
        let buffers = buffer_policy(&dto.policy);
        let blocks = self
            .runtime
            .application_queries()
            .schedule_between(dto.start_ms, dto.end_ms)
            .map_err(ClientError::from)?;
        let habits = self
            .runtime
            .application_queries()
            .habits()
            .map_err(ClientError::from)?;
        let habit_map: std::collections::BTreeMap<_, _> =
            habits.iter().map(|h| (h.id().to_string(), h)).collect();
        let requests = dto
            .requests
            .iter()
            .map(|input| {
                let habit = habit_map.get(&input.habit_id).ok_or_else(|| ClientError {
                    code: ClientErrorCode::NotFound,
                    message: format!("habit not found: {}", input.habit_id),
                })?;
                Ok(HabitPlanRequest {
                    habit_id: habit.id().clone(),
                    cadence: habit.cadence(),
                    active: habit.is_active(),
                    completed_in_period: input.completed_in_period,
                    duration_per_occurrence_minutes: input.duration_per_occurrence_minutes,
                })
            })
            .collect::<Result<Vec<_>, ClientError>>()?;
        let proposal = habit_plan_from_dto(&dto)?;
        let validated = validate_habit_proposal_with_buffers(
            window, &blocks, &requests, capacity, buffers, proposal,
        )
        .map_err(|violations| ClientError {
            code: ClientErrorCode::InvalidInput,
            message: format!("habit plan rejected: {violations:?}"),
        })?;
        let identities = validated
            .proposal()
            .scheduled
            .iter()
            .map(|item| {
                Ok(HabitPlanApplicationIdentity {
                    habit_id: item.habit_id.clone(),
                    occurrence_index: item.occurrence_index,
                    schedule_block_id: ScheduleBlockId::new(generate_id("schedule"))
                        .map_err(invalid_input)?,
                    activity_id: new_activity_id()?,
                })
            })
            .collect::<Result<Vec<_>, ClientError>>()?;
        let command = ApplyValidatedHabitPlanCommand {
            window,
            period_start_ms: dto.period_start_ms,
            period_end_ms: dto.period_end_ms,
            policy: capacity,
            fixed_buffers: buffers,
            requests,
            proposal: validated,
            identities,
            origin: ActivityOrigin::Human,
        };
        let revision = self.runtime.snapshot().map_err(authority_client_error)?.0;
        let (_, _, block_ids) = self
            .runtime
            .apply_habit_plan_accepted("orbit-client", &generate_id("command"), revision, command)
            .map_err(authority_client_error)?;
        let mut created = Vec::with_capacity(block_ids.len());
        for id in block_ids {
            let block_id = ScheduleBlockId::new(id).map_err(invalid_input)?;
            created.push(
                self.runtime
                    .application_queries()
                    .schedule_block(&block_id)?,
            );
        }
        Ok(created.iter().map(ScheduleBlockDto::from).collect())
    }

    pub fn current_change_cursor(&self) -> Result<ClientChangeCursor, ClientError> {
        let count = self
            .runtime
            .application_queries()
            .activities()
            .map_err(ClientError::from)?
            .len() as u64;
        Ok(ClientChangeCursor {
            activity_count: count,
        })
    }

    /// Returns semantic changes appended after `cursor` and a cursor pointing at
    /// the current end of the durable activity journal. Generated Flutter
    /// bindings may expose this as polling or wrap repeated calls in a stream.
    pub fn poll_changes(
        &self,
        cursor: ClientChangeCursor,
    ) -> Result<ClientChangeBatch, ClientError> {
        let activities = self
            .runtime
            .application_queries()
            .activities()
            .map_err(ClientError::from)?;
        let start = usize::try_from(cursor.activity_count).unwrap_or(usize::MAX);
        if start > activities.len() {
            return Err(ClientError {
                code: ClientErrorCode::InvalidInput,
                message: "change cursor is ahead of the current activity journal".to_owned(),
            });
        }
        let changes = activities[start..]
            .iter()
            .map(ClientChangeDto::from)
            .collect();
        Ok(ClientChangeBatch {
            cursor: ClientChangeCursor {
                activity_count: activities.len() as u64,
            },
            changes,
        })
    }
}

mod facade_conversions;
use facade_conversions::*;

#[cfg(test)]
#[path = "client_facade_tests.rs"]
mod client_facade_tests;

#[cfg(test)]
#[path = "rich_client_tests.rs"]
mod rich_client_tests;

#[cfg(test)]
#[path = "attachment_staging_tests.rs"]
mod attachment_staging_tests;

#[cfg(test)]
#[path = "connected_flush_tests.rs"]
mod connected_flush_tests;
