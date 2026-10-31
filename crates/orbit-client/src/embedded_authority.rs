//! In-process transport for a host-owned canonical Orbit runtime.
//!
//! This is the same authoritative implementation consumed by HTTP and MCP,
//! without a socket or a client-owned SQLite connection. The enclosing host
//! owns this object and must serialize its mutation requests.
use orbit_runtime::{
    AuthoritativeHost, AuthorityError,
    authority_evidence::{EvidenceCommand, EvidenceResult, EvidenceSnapshot},
    domain_commands::{
        ProjectCommand, ProjectRecord, ProjectResult, TaskCommand, TaskRecord, TaskResult,
    },
};
use std::path::Path;

/// Embedded transport: delegates requests to the canonical host, not to SQL.
/// `actor` is assigned by the trusted local host, not supplied per command.
pub struct EmbeddedAuthorityClient {
    host: AuthoritativeHost,
    actor: String,
}

impl EmbeddedAuthorityClient {
    /// Opens the authoritative runtime on this machine. The SQLite connection
    /// belongs to `AuthoritativeHost`, never to client application code.
    pub fn open(path: impl AsRef<Path>, actor: impl Into<String>) -> Result<Self, AuthorityError> {
        let actor = actor.into();
        if actor.trim().is_empty() {
            return Err(AuthorityError::Invalid("empty local actor"));
        }
        Ok(Self {
            host: AuthoritativeHost::open(path)?,
            actor,
        })
    }

    pub fn snapshot(&self) -> Result<orbit_runtime::AuthoritySnapshot, AuthorityError> {
        self.host.snapshot()
    }

    /// Borrow the host for richer facade commands that must use the same
    /// authoritative command path as the local `OrbitClient`.
    pub(crate) fn host(&self) -> &AuthoritativeHost {
        &self.host
    }

    /// Mutably borrow the host for accepted rich commands.
    pub(crate) fn host_mut(&mut self) -> &mut AuthoritativeHost {
        &mut self.host
    }

    /// Split the trusted actor from a mutable host borrow for one command.
    pub(crate) fn command_context(&mut self) -> (String, &mut AuthoritativeHost) {
        let actor = self.actor.clone();
        (actor, &mut self.host)
    }

    pub fn projects(&self) -> Result<(i64, Vec<ProjectRecord>), AuthorityError> {
        self.host.projects()
    }

    pub fn strategies(
        &self,
    ) -> Result<orbit_runtime::authority_strategy::StrategySnapshot, AuthorityError> {
        self.host.strategies()
    }
    pub fn strategy_command(
        &mut self,
        command: orbit_runtime::authority_strategy::StrategyCommand,
    ) -> Result<orbit_runtime::authority_strategy::StrategyResult, AuthorityError> {
        self.host.strategy_command(&self.actor, command)
    }

    pub fn milestones(
        &self,
    ) -> Result<orbit_runtime::authority_milestones::MilestoneSnapshot, AuthorityError> {
        self.host.milestones()
    }
    pub fn milestone_command(
        &mut self,
        command: orbit_runtime::authority_milestones::MilestoneCommand,
    ) -> Result<orbit_runtime::authority_milestones::MilestoneResult, AuthorityError> {
        self.host.milestone_command(&self.actor, command)
    }

    pub fn routines(
        &self,
    ) -> Result<orbit_runtime::authority_routines::RoutineSnapshot, AuthorityError> {
        self.host.routines()
    }

    pub fn routine_command(
        &mut self,
        command: orbit_runtime::authority_routines::RoutineCommand,
    ) -> Result<orbit_runtime::authority_routines::RoutineResult, AuthorityError> {
        self.host.routine_command(&self.actor, command)
    }

    pub fn schedule(
        &self,
    ) -> Result<orbit_runtime::authority_schedule::ScheduleSnapshot, AuthorityError> {
        self.host.schedule()
    }

    pub fn schedule_command(
        &mut self,
        command: orbit_runtime::authority_schedule::ScheduleCommand,
    ) -> Result<orbit_runtime::authority_schedule::ScheduleResult, AuthorityError> {
        self.host.schedule_command(&self.actor, command)
    }

    pub fn workload_insights(
        &self,
        now_ms: i64,
        horizon_ms: i64,
    ) -> Result<orbit_runtime::workload_insights::WorkloadInsights, AuthorityError> {
        self.host.workload_insights(now_ms, horizon_ms)
    }

    pub fn day_plan(
        &self,
        request: &orbit_runtime::daily_planning::DayPlanRequest,
    ) -> Result<orbit_runtime::daily_planning::DayPlan, AuthorityError> {
        self.host.day_plan(request)
    }

    pub fn next_actions(
        &self,
        now_ms: i64,
        limit: usize,
    ) -> Result<orbit_runtime::task_next::NextActionsReport, AuthorityError> {
        self.host.next_actions(now_ms, limit)
    }

    pub fn task_graph(
        &self,
        now_ms: i64,
    ) -> Result<orbit_runtime::task_graph::TaskGraphReport, AuthorityError> {
        self.host.task_graph(now_ms)
    }
    pub fn task_dependency_edges(
        &self,
    ) -> Result<Vec<orbit_runtime::task_dependencies::TaskDependencyEdge>, AuthorityError> {
        self.host.task_dependency_edges()
    }
    pub fn task_readiness(
        &self,
        now_ms: i64,
    ) -> Result<orbit_runtime::task_dependencies::TaskReadinessSnapshot, AuthorityError> {
        self.host.task_readiness(now_ms)
    }

    pub fn tasks(&self) -> Result<(i64, Vec<TaskRecord>), AuthorityError> {
        self.host.tasks()
    }

    pub fn evidence(&self) -> Result<EvidenceSnapshot, AuthorityError> {
        self.host.evidence()
    }

    pub fn create_project(
        &mut self,
        command: ProjectCommand,
    ) -> Result<ProjectResult, AuthorityError> {
        self.host.project_command(&self.actor, command)
    }

    pub fn task_command(&mut self, command: TaskCommand) -> Result<TaskResult, AuthorityError> {
        self.host.task_command(&self.actor, command)
    }

    pub fn evidence_command(
        &mut self,
        command: EvidenceCommand,
    ) -> Result<EvidenceResult, AuthorityError> {
        self.host.evidence_command(&self.actor, command)
    }

    /// File-authoritative creation/editing with SQLite revision and receipt acceptance.
    pub fn markdown_command(
        &mut self,
        command_id: &str,
        command: orbit_runtime::markdown_commands::MarkdownCommand,
    ) -> Result<orbit_runtime::markdown_commands::MarkdownReceipt, AuthorityError> {
        self.host
            .execute_markdown_command(&self.actor, command_id, command)
    }

    pub fn folder_create(
        &mut self,
        command_id: &str,
        command: orbit_runtime::folder_commands::FolderCreate,
    ) -> Result<orbit_runtime::folder_commands::FolderCreateReceipt, AuthorityError> {
        self.host
            .execute_folder_create(&self.actor, command_id, command)
    }

    pub fn folder_move(
        &mut self,
        command_id: &str,
        command: orbit_runtime::folder_commands::FolderMove,
    ) -> Result<orbit_runtime::folder_commands::FolderReceipt, AuthorityError> {
        self.host
            .execute_folder_move(&self.actor, command_id, command)
    }

    pub fn knowledge_tree(
        &self,
    ) -> Result<
        orbit_runtime::knowledge_files::KnowledgeNode,
        orbit_runtime::knowledge_files::VaultError,
    > {
        self.host.knowledge_tree()
    }

    pub fn markdown_document(
        &self,
        path: &str,
    ) -> Result<String, orbit_runtime::knowledge_files::VaultError> {
        self.host.markdown_document(path)
    }

    pub fn markdown_by_id(
        &self,
        id: &str,
    ) -> Result<
        Option<(
            orbit_runtime::knowledge_service::KnowledgeDocumentInfo,
            String,
        )>,
        orbit_runtime::knowledge_service::KnowledgeServiceError,
    > {
        self.host.markdown_by_id(id)
    }
    pub fn markdown_search(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<
        Vec<orbit_runtime::knowledge_service::KnowledgeDocumentInfo>,
        orbit_runtime::knowledge_service::KnowledgeServiceError,
    > {
        self.host.markdown_search(query, limit)
    }
    pub fn markdown_relations(
        &self,
        id: &str,
    ) -> Result<
        Option<orbit_runtime::knowledge_service::KnowledgeFileRelations>,
        orbit_runtime::knowledge_service::KnowledgeServiceError,
    > {
        self.host.markdown_relations(id)
    }

    pub fn history_prefix(&self, revision: i64) -> Result<String, AuthorityError> {
        self.host.history_prefix(revision)
    }

    pub fn schedule_plan(
        &self,
        request: &orbit_runtime::schedule_planning::SchedulePlanRequest,
    ) -> Result<orbit_runtime::schedule_planning::SchedulePlan, AuthorityError> {
        self.host.schedule_plan(request)
    }

    pub fn preview_task_completion(
        &self,
        request: &orbit_runtime::task_impact::CompletionImpactRequest,
    ) -> Result<orbit_runtime::task_impact::CompletionImpact, AuthorityError> {
        self.host.preview_task_completion(request)
    }

    pub fn work_brief(
        &self,
        request: &orbit_runtime::planning_brief::WorkBriefRequest,
    ) -> Result<orbit_runtime::planning_brief::WorkBrief, AuthorityError> {
        self.host.work_brief(request)
    }

    pub fn planning_focus(
        &self,
        brief: &orbit_runtime::planning_brief::WorkBrief,
        request: &orbit_runtime::planning_focus::FocusRequest,
    ) -> Result<orbit_runtime::planning_focus::TaskFocus, AuthorityError> {
        self.host.planning_focus(brief, request)
    }

    pub fn planning_group(
        &self,
        brief: &orbit_runtime::planning_brief::WorkBrief,
        request: &orbit_runtime::planning_focus::GroupFocusRequest,
    ) -> Result<orbit_runtime::planning_focus::GroupFocus, AuthorityError> {
        self.host.planning_group(brief, request)
    }

    pub fn planning_task_actions(
        &self,
        brief: &orbit_runtime::planning_brief::WorkBrief,
        request: &orbit_runtime::planning_actions::TaskActionRequest,
    ) -> Result<orbit_runtime::planning_actions::PlanningActionMenu, AuthorityError> {
        self.host.planning_task_actions(brief, request)
    }

    pub fn planning_group_actions(
        &self,
        brief: &orbit_runtime::planning_brief::WorkBrief,
        request: &orbit_runtime::planning_actions::GroupActionRequest,
    ) -> Result<orbit_runtime::planning_actions::PlanningActionMenu, AuthorityError> {
        self.host.planning_group_actions(brief, request)
    }

    pub fn resolve_planning_task_action(
        &self,
        brief: &orbit_runtime::planning_brief::WorkBrief,
        request: &orbit_runtime::planning_actions::TaskActionRequest,
        selection: &orbit_runtime::planning_actions::ActionSelection,
    ) -> Result<orbit_runtime::planning_actions::ResolvedAction, AuthorityError> {
        self.host
            .resolve_planning_task_action(brief, request, selection)
    }

    pub fn resolve_planning_group_action(
        &self,
        brief: &orbit_runtime::planning_brief::WorkBrief,
        request: &orbit_runtime::planning_actions::GroupActionRequest,
        selection: &orbit_runtime::planning_actions::ActionSelection,
    ) -> Result<orbit_runtime::planning_actions::ResolvedAction, AuthorityError> {
        self.host
            .resolve_planning_group_action(brief, request, selection)
    }

    pub fn planning_task_handoff(
        &self,
        brief: &orbit_runtime::planning_brief::WorkBrief,
        request: &orbit_runtime::planning_actions::TaskActionRequest,
        selection: &orbit_runtime::planning_actions::ActionSelection,
    ) -> Result<orbit_runtime::planning_handoff::PlanningHandoff, AuthorityError> {
        self.host.planning_task_handoff(brief, request, selection)
    }

    pub fn planning_group_handoff(
        &self,
        brief: &orbit_runtime::planning_brief::WorkBrief,
        request: &orbit_runtime::planning_actions::GroupActionRequest,
        selection: &orbit_runtime::planning_actions::ActionSelection,
    ) -> Result<orbit_runtime::planning_handoff::PlanningHandoff, AuthorityError> {
        self.host.planning_group_handoff(brief, request, selection)
    }

    pub fn project_dependency_report(
        &self,
        request: &orbit_runtime::project_dependency_planning::ProjectDependencyRequest,
    ) -> Result<orbit_runtime::project_dependency_planning::ProjectDependencyReport, AuthorityError>
    {
        self.host.project_dependency_report(request)
    }

    pub fn project_handoffs(
        &self,
        request: &orbit_runtime::project_handoffs::HandoffRequest,
    ) -> Result<orbit_runtime::project_handoffs::ProjectHandoffReport, AuthorityError> {
        self.host.project_handoffs(request)
    }

    pub fn portfolio_capacity_plan(
        &self,
        request: &orbit_runtime::portfolio_capacity::PortfolioCapacityRequest,
    ) -> Result<orbit_runtime::portfolio_capacity::PortfolioCapacityPlan, AuthorityError> {
        self.host.portfolio_capacity_plan(request)
    }

    pub fn compare_planning_scenarios(
        &self,
        request: &orbit_runtime::planning_scenarios::ScenarioCompareRequest,
    ) -> Result<orbit_runtime::planning_scenarios::ScenarioComparison, AuthorityError> {
        self.host.compare_planning_scenarios(request)
    }

    pub fn portfolio_actions(
        &self,
        request: &orbit_runtime::portfolio_actions::PortfolioActionRequest,
    ) -> Result<orbit_runtime::portfolio_actions::PortfolioActionReport, AuthorityError> {
        self.host.portfolio_actions(request)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use orbit_runtime::domain_commands::{ProjectCommand, TaskAction, TaskCommand};

    #[test]
    fn embedded_transport_executes_canonical_commands_and_replays_receipts() {
        let mut client = EmbeddedAuthorityClient::open(":memory:", "local-owner").unwrap();
        let empty = client.snapshot().unwrap();
        assert_eq!(empty.0, 0);
        let project = ProjectCommand {
            workflow_id: None,
            command_id: "p1".into(),
            expected_revision: 0,
            project_id: "project-1".into(),
            name: "Project".into(),
        };
        let accepted = client.create_project(project.clone()).unwrap();
        assert_eq!(accepted.revision, 1);
        assert!(client.create_project(project).unwrap().duplicate);
        let task = TaskCommand {
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
        };
        assert_eq!(client.task_command(task).unwrap().revision, 2);
        let snapshot = client.snapshot().unwrap();
        assert_eq!(snapshot.0, 2);
        assert_eq!(snapshot.2.len(), 1);
        assert_eq!(snapshot.3.len(), 1);
        assert_eq!(client.history_prefix(2).unwrap(), snapshot.1);
        assert!(matches!(
            client.task_command(TaskCommand {
                command_id: "stale".into(),
                expected_revision: 0,
                action: TaskAction::Create {
                    task_id: "task-2".into(),
                    title: "Late".into(),
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
                }
            }),
            Err(AuthorityError::StaleRevision)
        ));
    }

    #[test]
    fn local_host_rejects_blank_actor() {
        assert!(matches!(
            EmbeddedAuthorityClient::open(":memory:", "  "),
            Err(AuthorityError::Invalid(_))
        ));
    }
}
