//! Connected MCP adapter. No local authoritative host or database access.
//! All writes are reviewed and queued through the shared AuthorityConnection.
use super::*;

/// Outcome of the exact command a connected rich mutation queued.
#[derive(Clone, Debug, PartialEq, Eq)]
enum MutationOutcome {
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
    fn accepted(&self) -> bool {
        matches!(self, Self::Accepted)
    }

    fn pending(&self) -> bool {
        matches!(self, Self::Pending { .. })
    }

    fn disposition(&self) -> &'static str {
        match self {
            Self::Accepted => "accepted",
            Self::Pending { .. } => "pending",
            Self::Conflict { .. } => "conflict",
        }
    }

    fn message(&self) -> Option<String> {
        match self {
            Self::Accepted => None,
            Self::Pending { command_id, state } => Some(format!(
                "queued as {} ({state}) in the durable outbox; server_state is the last accepted state only. Call authority.flush (developer mode) to submit it.",
                command_id.as_deref().unwrap_or("unknown")
            )),
            Self::Conflict {
                command_id,
                state,
                reason,
            } => Some(format!(
                "queued change {command_id} rejected ({state}): {reason}; inspect authority.unresolved or authority.conflict_review before retrying"
            )),
        }
    }
}

impl ConnectedMcp {
    // Connected job tools require a live server and bound job. Never stage
    // agent evidence into ordinary task-command outbox or local replica.
    fn agent_http(&self) -> Result<&orbit_client::authority::AuthorityHttp, String> {
        match &self.client {
            AuthorityConnection::Remote {
                http: Some(http), ..
            } => Ok(http),
            _ => Err("agent reporting needs an online authenticated authority".into()),
        }
    }
    fn agent_context(&self, job_id: &str) -> Result<Value, String> {
        let bound = std::env::var("ORBIT_MCP_JOB_ID")
            .map_err(|_| "ORBIT_MCP_JOB_ID is required for job-scoped tools".to_string())?;
        if bound.is_empty() || bound != job_id {
            return Err("job outside MCP session scope".into());
        }
        let encoded = job_id
            .bytes()
            .map(|b| {
                if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
                    char::from(b).to_string()
                } else {
                    format!("%{b:02X}")
                }
            })
            .collect::<String>();
        let context = self
            .agent_http()?
            .get(&format!(
                "/v1/authoritative/agent-jobs/resume-context?id={encoded}"
            ))
            .map_err(debug_err)?;
        let project = context
            .pointer("/job/project_id")
            .and_then(Value::as_str)
            .ok_or("job lacks authoritative project")?;
        if !self.policy.projects.is_empty() && !self.policy.projects.contains(project) {
            return Err("job outside agent project scope".into());
        }
        Ok(context)
    }
    fn read_family(&self, name: &str) -> Result<Value, String> {
        let records = match name {
            "projects" => self.client.projects(),
            "habits" => self.client.habits(),
            "habit_occurrences" => self.client.habit_occurrences(),
            "metrics" => self.client.metrics(),
            "metric_observations" => self.client.metric_observations(),
            "tasks" => self.client.tasks(),
            "schedule" => self.client.schedule(),
            "routines" => self.client.routines(),
            "milestones" => self.client.milestones(),
            "strategies" => self.client.strategies(),
            _ => return Err("unsupported collection".into()),
        }
        .map_err(debug_err)?;
        let filtered = records
            .into_iter()
            .filter(|record| {
                match name {
                    "projects" => record.get("id").and_then(Value::as_str).is_some_and(|id| {
                        self.policy.projects.is_empty() || self.policy.projects.contains(id)
                    }),
                    "strategies" | "milestones" => record
                        .get("project_id")
                        .and_then(Value::as_str)
                        .is_some_and(|id| {
                            self.policy.projects.is_empty() || self.policy.projects.contains(id)
                        }),
                    "tasks" => match record.get("project_id").and_then(Value::as_str) {
                        Some(id) => {
                            self.policy.projects.is_empty() || self.policy.projects.contains(id)
                        }
                        None => self.policy.allow_unscoped_execution,
                    },
                    // Unscoped schedules and routines can reveal user activities.
                    // Scoped agents are denied, rather than leaking references.
                    "schedule"
                    | "routines"
                    | "habits"
                    | "habit_occurrences"
                    | "metrics"
                    | "metric_observations" => self.policy.allow_unscoped_execution,
                    _ => false,
                }
            })
            .collect::<Vec<_>>();
        Ok(json!(filtered))
    }

    fn allow_project(&self, id: &str) -> Result<(), String> {
        if self.policy.projects.is_empty() || self.policy.projects.contains(id) {
            Ok(())
        } else {
            Err("project outside agent scope".into())
        }
    }

    /// Durable state of the command just queued by a rich mutation.
    ///
    /// Aggregate depth cannot attribute a pending row to this call, so follow
    /// the exact identity the client recorded. A missing identity or state is
    /// deliberately reported as pending: an unobserved command is never
    /// accepted.
    fn queued_outcome(&self, before: Option<String>) -> MutationOutcome {
        match &self.client {
            AuthorityConnection::Embedded(_) => MutationOutcome::Accepted,
            AuthorityConnection::Remote { .. } => {
                let Some(command_id) = self.client.last_queued_command_id() else {
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
                self.command_outcome(&command_id)
            }
        }
    }

    fn command_outcome(&self, command_id: &str) -> MutationOutcome {
        match self.client.command_state(command_id) {
            Ok(Some(state)) if state == "accepted" => MutationOutcome::Accepted,
            Ok(Some(state)) if state == "conflict" => {
                let reason = self
                    .client
                    .command_status(command_id)
                    .ok()
                    .flatten()
                    .and_then(|status| {
                        status
                            .pointer("/receipt/error")
                            .and_then(Value::as_str)
                            .map(str::to_owned)
                    })
                    .filter(|reason| !reason.trim().is_empty())
                    .unwrap_or_else(|| {
                        "command conflict requires explicit reconciliation; inspect the outbox"
                            .to_owned()
                    });
                MutationOutcome::Conflict {
                    command_id: command_id.to_owned(),
                    state,
                    reason,
                }
            }
            Ok(Some(state)) => MutationOutcome::Pending {
                command_id: Some(command_id.to_owned()),
                state,
            },
            Ok(None) | Err(_) => MutationOutcome::Pending {
                command_id: Some(command_id.to_owned()),
                state: "unavailable".into(),
            },
        }
    }

    /// One task projection shared by `task.get` and rich mutations: the
    /// accepted authority task (description, priority, labels, lifecycle,
    /// planning) plus, when available, its ordered acceptance criteria.
    ///
    /// `criteria: None` means the criteria read was unavailable, not that the
    /// task has no criteria. Mutation responses must never turn an unavailable
    /// read after a successful queue into an error, so absence is explicit
    /// (`acceptance_criteria: null` and `criteria_available: false`) rather than
    /// guessed.
    fn task_body(
        task: &orbit_client::TaskDto,
        criteria: Option<Vec<orbit_client::AcceptanceCriterionDto>>,
    ) -> Value {
        json!({
            "id": task.id,
            "title": task.title,
            "status": task.status,
            "project_id": task.project_id,
            "milestone_id": task.milestone_id,
            "planning": {
                "estimated_minutes": task.estimated_minutes,
                "earliest_start_at_ms": task.earliest_start_at_ms,
                "deadline_at_ms": task.deadline_at_ms,
            },
            "description": task.description,
            "priority": task.priority,
            "labels": task.labels,
            "acceptance_criteria": criteria.as_deref().map(criterion_rows),
            "criteria_available": criteria.is_some(),
        })
    }

    /// Read-time projection: both reads are required because a live read has
    /// no durable mutation to protect.
    fn task_json(&self, id: &str) -> Result<Value, String> {
        let task = self.client.task_detail(id).map_err(debug_err)?;
        let criteria = self.client.acceptance_criteria(id).map_err(debug_err)?;
        Ok(Self::task_body(&task, Some(criteria)))
    }

    fn criteria_json(&self, task_id: &str) -> Result<Value, String> {
        let criteria = self
            .client
            .acceptance_criteria(task_id)
            .map_err(debug_err)?;
        Ok(json!({"task_id": task_id, "criteria": criterion_rows(&criteria)}))
    }

    fn check_milestone_scope(&self, action: &MilestoneAction) -> Result<(), String> {
        match action {
            MilestoneAction::Create { project_id, .. } => self.allow_project(project_id),
            MilestoneAction::MoveProject {
                milestone_id,
                project_id,
            } => {
                self.allow_project(project_id)?;
                self.check_existing_milestone(milestone_id)
            }
            _ => self.check_existing_milestone(action.milestone_id()),
        }
    }
    fn check_existing_milestone(&self, id: &str) -> Result<(), String> {
        let rows = self.client.milestones().map_err(debug_err)?;
        let project = rows
            .iter()
            .find(|row| row.get("id").and_then(Value::as_str) == Some(id))
            .and_then(|row| row.get("project_id"))
            .and_then(Value::as_str)
            .ok_or("milestone absent from last accepted snapshot; refresh first")?;
        self.allow_project(project)
    }

    /// A task may change project ownership; both the accepted old project and
    /// destination must be authorized. Never trust the client's claimed scope.
    fn check_task_scope(&self, action: &TaskAction) -> Result<(), String> {
        match action {
            TaskAction::Create { project_id, .. } => {
                self.check_project_option(project_id.as_deref())
            }
            TaskAction::AssignProject {
                task_id,
                project_id,
            } => {
                self.check_existing_task(task_id)?;
                self.check_project_option(project_id.as_deref())
            }
            TaskAction::AssignMilestone {
                task_id,
                milestone_id,
            } => {
                self.check_existing_task(task_id)?;
                if let Some(id) = milestone_id {
                    self.check_existing_milestone(id)?;
                }
                Ok(())
            }
            TaskAction::ChangeStatus { task_id, .. }
            | TaskAction::Rename { task_id, .. }
            | TaskAction::SetPlanning { task_id, .. }
            | TaskAction::SetDetails { task_id, .. }
            | TaskAction::AcceptanceCriteria { task_id, .. } => self.check_existing_task(task_id),
            TaskAction::AddPrerequisite {
                task_id,
                prerequisite_id,
            }
            | TaskAction::RemovePrerequisite {
                task_id,
                prerequisite_id,
            } => {
                self.check_existing_task(task_id)?;
                self.check_existing_task(prerequisite_id)
            }
        }
    }
    fn check_project_option(&self, id: Option<&str>) -> Result<(), String> {
        match id {
            Some(project) => self.allow_project(project),
            None if self.policy.allow_unscoped_execution => Ok(()),
            None => Err("unassigned tasks require unscoped execution permission".into()),
        }
    }
    fn check_existing_task(&self, id: &str) -> Result<(), String> {
        let records = self.client.tasks().map_err(debug_err)?;
        if let Some(row) = records
            .iter()
            .find(|row| row.get("id").and_then(Value::as_str) == Some(id))
        {
            return self.check_project_option(row.get("project_id").and_then(Value::as_str));
        }
        // A read-only agent may never have refreshed this replica, and another
        // device may have created the task after the last accepted snapshot.
        // The live task-detail route is authoritative for scope decisions, so
        // fall back to it instead of denying a legitimate rich read.
        let task = self.client.task_detail(id).map_err(debug_err)?;
        self.check_project_option(task.project_id.as_deref())
    }

    /// Decode and authorize the ENTIRE workflow before queueing its first step.
    /// This avoids partial submission caused by invalid later steps. As with the
    /// normal authority workflow, runtime failures may still leave a committed
    /// or queued prefix; callers receive its exact dispositions.
    fn decode_workflow(&self, args: &Value) -> Result<Vec<DomainWorkflowStep>, String> {
        self.policy.write_allowed()?;
        let steps = args
            .get("steps")
            .and_then(Value::as_array)
            .ok_or("steps must be an array")?;
        if steps.is_empty() || steps.len() > 100 {
            return Err("workflow requires between 1 and 100 steps".into());
        }
        let mut ids = BTreeSet::new();
        let mut decoded = Vec::with_capacity(steps.len());
        for (index, step) in steps.iter().enumerate() {
            let family = step
                .get("family")
                .and_then(Value::as_str)
                .ok_or_else(|| format!("step {index}: family must be a string"))?;
            let payload = step
                .get("command")
                .cloned()
                .ok_or_else(|| format!("step {index}: command missing"))?;
            let item = match family {
                "project" => {
                    let cmd: ProjectCommand = serde_json::from_value(payload).map_err(debug_err)?;
                    self.allow_project(&cmd.project_id)?;
                    DomainWorkflowStep::Project(cmd)
                }
                "task" => {
                    let cmd: TaskCommand = serde_json::from_value(payload).map_err(debug_err)?;
                    self.check_task_scope(&cmd.action)?;
                    DomainWorkflowStep::Task(cmd)
                }
                "evidence" => {
                    if !self.policy.allow_unscoped_execution {
                        return Err("evidence requires unscoped permission".into());
                    }
                    DomainWorkflowStep::Evidence(
                        serde_json::from_value(payload).map_err(debug_err)?,
                    )
                }
                "schedule" => {
                    if !self.policy.allow_unscoped_execution {
                        return Err("schedule requires unscoped permission".into());
                    }
                    DomainWorkflowStep::Schedule(
                        serde_json::from_value(payload).map_err(debug_err)?,
                    )
                }
                "routine" => {
                    if !self.policy.allow_unscoped_execution {
                        return Err("routine requires unscoped permission".into());
                    }
                    DomainWorkflowStep::Routine(serde_json::from_value(payload).map_err(debug_err)?)
                }
                "milestone" => {
                    let cmd: MilestoneCommand =
                        serde_json::from_value(payload).map_err(debug_err)?;
                    self.check_milestone_scope(&cmd.action)?;
                    DomainWorkflowStep::Milestone(cmd)
                }
                "strategy" => {
                    let cmd: StrategyCommand =
                        serde_json::from_value(payload).map_err(debug_err)?;
                    self.allow_project(cmd.action.project_id())?;
                    DomainWorkflowStep::Strategy(cmd)
                }
                _ => return Err(format!("step {index}: unsupported family {family}")),
            };
            let id = item.command_id();
            if id.trim().is_empty() || !ids.insert(id.to_owned()) {
                return Err(format!(
                    "step {index}: command IDs must be nonempty and unique"
                ));
            }
            decoded.push(item);
        }
        Ok(decoded)
    }

    fn receipt(disposition: CommandDisposition) -> Value {
        match disposition {
            CommandDisposition::Accepted {
                revision,
                duplicate,
            } => json!({"disposition":"accepted","revision":revision,"duplicate":duplicate}),
            CommandDisposition::Pending => json!({"disposition":"pending","accepted":false}),
            CommandDisposition::AcceptedMarkdown { revision } => {
                json!({"disposition":"accepted","revision":revision})
            }
            CommandDisposition::AcceptedFolder => json!({"disposition":"accepted"}),
        }
    }

    pub(super) fn dispatch(&mut self, name: &str, args: &Value) -> Result<Value, String> {
        if name.starts_with("blueprint.") {
            return self.blueprint_dispatch(name, args);
        }
        match name {
            "planning.capabilities" => {
                let id = args.get("id").and_then(Value::as_str);
                let accepted_only = args
                    .get("accepted_only")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                serde_json::to_value(orbit_runtime::capabilities::discover(
                    id,
                    None,
                    accepted_only,
                ))
                .map_err(debug_err)
            }
            "task.dependencies.add" | "task.dependencies.remove" => {
                self.policy.write_allowed()?;
                let command_id = required_str(args, "command_id")?;
                let expected_revision = required_i64(args, "expected_revision")?;
                let task_id = required_str(args, "task_id")?;
                let prerequisite_id = required_str(args, "prerequisite_id")?;
                self.check_existing_task(&task_id)?;
                self.check_existing_task(&prerequisite_id)?;
                let action = if name.ends_with(".add") {
                    TaskAction::AddPrerequisite {
                        task_id,
                        prerequisite_id,
                    }
                } else {
                    TaskAction::RemovePrerequisite {
                        task_id,
                        prerequisite_id,
                    }
                };
                let command = TaskCommand {
                    command_id,
                    expected_revision,
                    action,
                };
                self.client
                    .task_command(command)
                    .map(Self::receipt)
                    .map_err(debug_err)
            }
            "task.dependencies.preview" => {
                if !self.policy.projects.is_empty() || !self.policy.allow_unscoped_execution {
                    return Err(
                        "dependency previews require unrestricted execution read access".into(),
                    );
                }
                let request: orbit_runtime::task_dependency_preview::DependencyPreviewRequest =
                    serde_json::from_value(args.clone()).map_err(debug_err)?;
                let report = self
                    .client
                    .preview_task_dependency(&request)
                    .map_err(debug_err)?;
                serde_json::to_value(report).map_err(debug_err)
            }
            "task.graph" => {
                if !self.policy.projects.is_empty() || !self.policy.allow_unscoped_execution {
                    return Err(
                        "dependency graph requires unrestricted execution read access".into(),
                    );
                }
                let now = required_i64(args, "now_ms")?;
                serde_json::to_value(self.client.task_graph(now).map_err(debug_err)?)
                    .map_err(debug_err)
            }
            "task.next" => {
                if !self.policy.projects.is_empty() || !self.policy.allow_unscoped_execution {
                    return Err("next actions require unrestricted execution read access".into());
                }
                let now = required_i64(args, "now_ms")?;
                let limit = args
                    .get("limit")
                    .and_then(Value::as_u64)
                    .unwrap_or(20)
                    .min(200) as usize;
                serde_json::to_value(self.client.next_actions(now, limit).map_err(debug_err)?)
                    .map_err(debug_err)
            }
            "planning.apply_approved_scenario" => {
                self.policy.write_allowed()?;
                if !self.policy.projects.is_empty() || !self.policy.allow_unscoped_execution {
                    return Err(
                        "calendar reservations require unrestricted workspace approval".into(),
                    );
                }
                if args.get("confirm") != Some(&serde_json::Value::Bool(true)) {
                    return Err("a separate explicit confirmation is required".into());
                }
                let request: orbit_runtime::planning_approval::ApprovedScheduleRequest =
                    serde_json::from_value(
                        args.get("request")
                            .cloned()
                            .ok_or("missing approved schedule request")?,
                    )
                    .map_err(debug_err)?;
                if !request.approval.approved
                    || request.approval.review.command_id != request.plan.command_id
                    || request.approval.review.expected_revision != request.plan.expected_revision
                {
                    return Err("approved schedule must match exact reviewed command".into());
                }
                serde_json::to_value(
                    self.client
                        .apply_approved_scenario_schedule(&request)
                        .map_err(debug_err)?,
                )
                .map_err(debug_err)
            }
            "planning.review_scenario_options" => {
                if !self.policy.projects.is_empty() || !self.policy.allow_unscoped_execution {
                    return Err("calendar option previews require full calendar visibility".into());
                }
                let request: orbit_runtime::scenario_execution::ScenarioExecutionRequest =
                    serde_json::from_value(
                        args.get("request")
                            .cloned()
                            .ok_or("missing calendar options request")?,
                    )
                    .map_err(debug_err)?;
                serde_json::to_value(
                    self.client
                        .review_scenario_options(&request)
                        .map_err(debug_err)?,
                )
                .map_err(debug_err)
            }
            "planning.review_scenario" => {
                // A calendar view discloses activities beyond a single project:
                // fail closed for restricted agents until a scoped-calendar
                // projection exists. Review is never permission to execute.
                if !self.policy.projects.is_empty() || !self.policy.allow_unscoped_execution {
                    return Err("scenario schedule review requires full calendar visibility".into());
                }
                let request: orbit_runtime::scenario_execution::ScenarioExecutionRequest =
                    serde_json::from_value(
                        args.get("request")
                            .cloned()
                            .ok_or("missing scenario execution request")?,
                    )
                    .map_err(debug_err)?;
                serde_json::to_value(
                    self.client
                        .review_scenario_execution(&request)
                        .map_err(debug_err)?,
                )
                .map_err(debug_err)
            }
            "planning.compare_scenarios" => {
                let request: orbit_runtime::planning_scenarios::ScenarioCompareRequest =
                    serde_json::from_value(
                        args.get("request")
                            .cloned()
                            .ok_or("missing scenario request")?,
                    )
                    .map_err(debug_err)?;
                let project = &request.baseline.project_id;
                if !self.policy.projects.is_empty() {
                    let Some(selected) = project else {
                        return Err("project-scoped agent must select an authorized project".into());
                    };
                    if !self.policy.projects.contains(selected) {
                        return Err("requested project is not authorized".into());
                    }
                } else if !self.policy.allow_unscoped_execution && project.is_none() {
                    return Err("unscoped planning comparison is not allowed".into());
                }
                serde_json::to_value(
                    self.client
                        .compare_planning_scenarios(&request)
                        .map_err(debug_err)?,
                )
                .map_err(debug_err)
            }
            "planning.portfolio_capacity" => {
                let now = required_i64(args, "now_ms")?;
                let available = required_i64(args, "available_minutes")?;
                let maximum = required_i64(args, "max_tasks")?;
                if !(0..=200).contains(&maximum) {
                    return Err("max_tasks must be between 0 and 200".into());
                }
                let project = args
                    .get("project_id")
                    .and_then(Value::as_str)
                    .map(str::to_owned);
                if !self.policy.projects.is_empty() {
                    let Some(ref requested) = project else {
                        return Err(
                            "project-scoped agents must request a specific authorized project"
                                .into(),
                        );
                    };
                    if !self.policy.projects.contains(requested) {
                        return Err("requested project is not authorized".into());
                    }
                } else if !self.policy.allow_unscoped_execution && project.is_none() {
                    return Err("unscoped portfolio planning is not allowed".into());
                }
                let default = args.get("default_minutes").and_then(Value::as_i64);
                let request = orbit_runtime::portfolio_capacity::PortfolioCapacityRequest {
                    now_ms: now,
                    available_minutes: available,
                    max_tasks: maximum as usize,
                    project_id: project,
                    include_unassigned: false,
                    include_unsized: default.is_some(),
                    default_minutes: default,
                };
                serde_json::to_value(
                    self.client
                        .portfolio_capacity_plan(&request)
                        .map_err(debug_err)?,
                )
                .map_err(debug_err)
            }
            "planning.portfolio_actions" => {
                let now = required_i64(args, "now_ms")?;
                let project = args
                    .get("project_id")
                    .and_then(Value::as_str)
                    .map(str::to_owned);
                if !self.policy.projects.is_empty() {
                    let Some(ref requested) = project else {
                        return Err(
                            "project-scoped agents must request a specific authorized project"
                                .into(),
                        );
                    };
                    if !self
                        .policy
                        .projects
                        .iter()
                        .any(|allowed| allowed == requested)
                    {
                        return Err("requested project is not authorized".into());
                    }
                } else if !self.policy.allow_unscoped_execution && project.is_none() {
                    return Err("unscoped portfolio planning is not allowed".into());
                }
                let limit = args
                    .get("limit")
                    .and_then(Value::as_u64)
                    .unwrap_or(20)
                    .min(200) as usize;
                let request = orbit_runtime::portfolio_actions::PortfolioActionRequest {
                    now_ms: now,
                    project_id: project,
                    limit,
                    include_unassigned: false,
                };
                serde_json::to_value(self.client.portfolio_actions(&request).map_err(debug_err)?)
                    .map_err(debug_err)
            }
            "task.readiness" => {
                if !self.policy.projects.is_empty() || !self.policy.allow_unscoped_execution {
                    return Err(
                        "workspace planning requires unrestricted execution read access".into(),
                    );
                }
                let now = required_i64(args, "now_ms")?;
                serde_json::to_value(self.client.task_readiness(now).map_err(debug_err)?)
                    .map_err(debug_err)
            }
            "planning.day" => {
                if !self.policy.projects.is_empty() || !self.policy.allow_unscoped_execution {
                    return Err(
                        "workspace planning requires unrestricted execution read access".into(),
                    );
                }
                let request: orbit_runtime::daily_planning::DayPlanRequest =
                    serde_json::from_value(args.clone()).map_err(debug_err)?;
                serde_json::to_value(self.client.day_plan(&request).map_err(debug_err)?)
                    .map_err(debug_err)
            }
            "planning.schedule" => {
                if !self.policy.projects.is_empty() || !self.policy.allow_unscoped_execution {
                    return Err(
                        "workspace planning requires unrestricted execution read access".into(),
                    );
                }
                let request: orbit_runtime::schedule_planning::SchedulePlanRequest =
                    serde_json::from_value(args.clone()).map_err(debug_err)?;
                serde_json::to_value(self.client.schedule_plan(&request).map_err(debug_err)?)
                    .map_err(debug_err)
            }
            "planning.workload" => {
                if !self.policy.projects.is_empty() || !self.policy.allow_unscoped_execution {
                    return Err(
                        "workspace planning requires unrestricted execution read access".into(),
                    );
                }
                let now = required_i64(args, "now_ms")?;
                let horizon = required_i64(args, "horizon_ms")?;
                serde_json::to_value(
                    self.client
                        .workload_insights(now, horizon)
                        .map_err(debug_err)?,
                )
                .map_err(debug_err)
            }
            "agent.job_context" => self.agent_context(&required_str(args, "job_id")?),
            "agent.handoff_submit" => {
                self.policy.write_allowed()?;
                let job_id = required_str(args, "job_id")?;
                self.agent_context(&job_id)?;
                let handoff_id = required_str(args, "handoff_id")?;
                let payload = args.get("payload").ok_or("missing payload")?;
                if !payload.is_object() {
                    return Err("handoff payload must be an object".into());
                }
                if serde_json::to_vec(payload).map_err(debug_err)?.len() > 64 * 1024 {
                    return Err("handoff exceeds 64 KiB".into());
                }
                let at_ms = required_i64(args, "at_ms")?;
                if at_ms < 0 {
                    return Err("handoff timestamp must be nonnegative".into());
                }
                let handoffs=self.agent_http()?.post("/v1/authoritative/agent-jobs/handoffs",
                    &json!({"job_id":job_id,"handoff_id":handoff_id,"payload":payload,"at_ms":at_ms}))
                    .map_err(debug_err)?;
                Ok(
                    json!({"job_id":job_id,"handoff_id":handoff_id,"handoffs":handoffs,
                    "task_completed":false}),
                )
            }
            "agent.inspect" => Ok(json!({
                "agent_id":self.policy.agent_id,"session_id":self.policy.session_id,
                "mode":if self.policy.mode == Mode::Developer {"developer"} else {"read_only"},
                "authority_mode":"connected","project_allowlist":self.policy.projects,
                "allow_unscoped_execution":self.policy.allow_unscoped_execution,
                "allow_unscoped_knowledge":self.policy.allow_unscoped_knowledge,
            })),
            "projects.list" => self.read_family("projects"),
            "tasks.list" => self.read_family("tasks"),
            // Rich per-task reads use the live task route (falling back from a
            // snapshot that lacks the task). Writes report the exact queued
            // command state and label the payload as last accepted server state.
            "task.get" => {
                let id = required_str(args, "id")?;
                self.check_existing_task(&id)?;
                self.task_json(&id)
            }
            "task.update" => {
                self.policy.write_allowed()?;
                let id = required_str(args, "id")?;
                self.check_existing_task(&id)?;
                if args.get("description").is_none()
                    && args.get("priority").is_none()
                    && args.get("labels").is_none()
                {
                    return Err(
                        "task.update requires at least one of description, priority or labels"
                            .into(),
                    );
                }
                let current = self.client.task_detail(&id).map_err(debug_err)?;
                let description = match args.get("description") {
                    Some(value) => value
                        .as_str()
                        .ok_or_else(|| "description must be a string".to_owned())?
                        .to_owned(),
                    None => current.description,
                };
                let priority = match args.get("priority") {
                    Some(value) => optional_priority(value)?,
                    None => current.priority,
                };
                let labels = match args.get("labels") {
                    Some(value) => labels_arg(value)?,
                    None => current.labels,
                };
                let before = self.client.last_queued_command_id();
                // The client returns the *last accepted* task it read before
                // queueing. Never perform a second, fallible read after the
                // command is durable: an error here would invite a retry that
                // queues a duplicate under a new command identity.
                let accepted = self
                    .client
                    .update_task_details(id.clone(), description, priority, labels)
                    .map_err(debug_err)?;
                let outcome = self.queued_outcome(before);
                // Criteria are advisory in a mutation response. A failed read
                // after queueing is reported as unavailable, not as failure.
                let criteria = self.client.acceptance_criteria(&id).ok();
                Ok(mutation_body(Self::task_body(&accepted, criteria), outcome))
            }
            "task.acceptance.list" => {
                let id = required_str(args, "task_id")?;
                self.check_existing_task(&id)?;
                self.criteria_json(&id)
            }
            "task.acceptance.add"
            | "task.acceptance.update"
            | "task.acceptance.check"
            | "task.acceptance.reorder"
            | "task.acceptance.remove" => {
                self.policy.write_allowed()?;
                let id = required_str(args, "task_id")?;
                self.check_existing_task(&id)?;
                let before = self.client.last_queued_command_id();
                let accepted = match name {
                    "task.acceptance.add" => {
                        let text = required_str(args, "text")?;
                        self.client.add_acceptance_criterion(id.clone(), text)
                    }
                    "task.acceptance.update" => {
                        let criterion_id = required_str(args, "criterion_id")?;
                        let text = required_str(args, "text")?;
                        self.client
                            .set_acceptance_criterion_text(id.clone(), criterion_id, text)
                    }
                    "task.acceptance.check" => {
                        let criterion_id = required_str(args, "criterion_id")?;
                        let completed = args
                            .get("completed")
                            .map(|value| {
                                value
                                    .as_bool()
                                    .ok_or_else(|| "completed must be a boolean".to_owned())
                            })
                            .transpose()?
                            .unwrap_or(true);
                        self.client.set_acceptance_criterion_completed(
                            id.clone(),
                            criterion_id,
                            completed,
                        )
                    }
                    "task.acceptance.reorder" => {
                        let criterion_id = required_str(args, "criterion_id")?;
                        let position = required_i64(args, "position")?;
                        if position < 0 {
                            return Err("position must be a non-negative zero-based index".into());
                        }
                        self.client.move_acceptance_criterion(
                            id.clone(),
                            criterion_id,
                            position as usize,
                        )
                    }
                    _ => {
                        let criterion_id = required_str(args, "criterion_id")?;
                        self.client
                            .remove_acceptance_criterion(id.clone(), criterion_id)
                    }
                }
                .map_err(debug_err)?;
                // The client returns the *accepted* list; the queued change is
                // only outbox state until `authority.flush` commits it.
                let outcome = self.queued_outcome(before);
                let server_state = json!({
                    "task_id": id,
                    "criteria": criterion_rows(&accepted),
                    "criteria_available": true,
                });
                Ok(mutation_body(server_state, outcome))
            }
            "task.activity" => {
                let id = required_str(args, "task_id")?;
                self.check_existing_task(&id)?;
                let limit = args
                    .get("limit")
                    .map(|value| {
                        value
                            .as_i64()
                            .ok_or_else(|| "limit must be an integer".to_owned())
                    })
                    .transpose()?
                    .unwrap_or(50);
                if !(0..=200).contains(&limit) {
                    return Err("limit must be between 0 and 200".into());
                }
                let activity = self.client.task_activity(&id).map_err(debug_err)?;
                Ok(json!(
                    activity
                        .into_iter()
                        .take(limit as usize)
                        .map(|row| json!({
                            "id": row.id,
                            "kind": row.kind,
                            "origin": row.origin,
                            "occurred_at_ms": row.occurred_at_ms,
                            "summary": row.summary,
                        }))
                        .collect::<Vec<_>>()
                ))
            }
            "planning.completion_impact" => {
                if !self.policy.projects.is_empty() || !self.policy.allow_unscoped_execution {
                    return Err(
                        "completion impact requires unrestricted execution read access".into(),
                    );
                }
                let request: orbit_runtime::task_impact::CompletionImpactRequest =
                    serde_json::from_value(args.clone()).map_err(debug_err)?;
                self.client
                    .preview_task_completion(&request)
                    .map(|report| json!(report))
                    .map_err(debug_err)
            }
            "planning.handoff_task" => {
                if !self.policy.projects.is_empty() || !self.policy.allow_unscoped_execution {
                    return Err("task handoff requires unrestricted planning access".into());
                }
                let request = orbit_runtime::planning_brief::WorkBriefRequest {
                    now_ms: required_i64(args, "now_ms")?,
                    ends_at_ms: required_i64(args, "ends_at_ms")?,
                    limit: args.get("limit").and_then(Value::as_u64).unwrap_or(100) as usize,
                    project_id: args
                        .get("project_id")
                        .and_then(Value::as_str)
                        .map(str::to_owned),
                    default_minutes: None,
                };
                let brief = self.client.work_brief(&request).map_err(debug_err)?;
                let revision = self
                    .client
                    .accepted_snapshot()
                    .map_err(debug_err)?
                    .and_then(|v| v.get("revision").and_then(Value::as_i64))
                    .ok_or("accepted revision unavailable")?;
                if brief.revision != revision {
                    return Err("accepted planning state changed".into());
                }
                let target = orbit_runtime::planning_actions::TaskActionRequest {
                    task_id: args
                        .get("task_id")
                        .and_then(Value::as_str)
                        .ok_or("task_id required")?
                        .into(),
                    authority_available: false,
                };
                let selection: orbit_runtime::planning_actions::ActionSelection =
                    serde_json::from_value(
                        args.get("selection").cloned().ok_or("selection required")?,
                    )
                    .map_err(debug_err)?;
                orbit_runtime::planning_handoff::dispatch_task(&brief, &target, &selection)
                    .map(|report| json!(report))
                    .map_err(str::to_owned)
            }
            "planning.handoff_group" => {
                if !self.policy.projects.is_empty() || !self.policy.allow_unscoped_execution {
                    return Err("group handoff requires unrestricted planning access".into());
                }
                let project_id = args
                    .get("project_id")
                    .and_then(Value::as_str)
                    .ok_or("project_id required")?
                    .to_owned();
                let request = orbit_runtime::planning_brief::WorkBriefRequest {
                    now_ms: required_i64(args, "now_ms")?,
                    ends_at_ms: required_i64(args, "ends_at_ms")?,
                    limit: args.get("limit").and_then(Value::as_u64).unwrap_or(100) as usize,
                    project_id: Some(project_id.clone()),
                    default_minutes: None,
                };
                let brief = self.client.work_brief(&request).map_err(debug_err)?;
                let revision = self
                    .client
                    .accepted_snapshot()
                    .map_err(debug_err)?
                    .and_then(|v| v.get("revision").and_then(Value::as_i64))
                    .ok_or("accepted revision unavailable")?;
                if brief.revision != revision {
                    return Err("accepted planning state changed".into());
                }
                let target = orbit_runtime::planning_actions::GroupActionRequest {
                    project_id,
                    milestone_id: args
                        .get("milestone_id")
                        .and_then(Value::as_str)
                        .map(str::to_owned),
                    authority_available: false,
                };
                let selection: orbit_runtime::planning_actions::ActionSelection =
                    serde_json::from_value(
                        args.get("selection").cloned().ok_or("selection required")?,
                    )
                    .map_err(debug_err)?;
                orbit_runtime::planning_handoff::dispatch_group(&brief, &target, &selection)
                    .map(|report| json!(report))
                    .map_err(str::to_owned)
            }
            "planning.resolve_task_action" => {
                if !self.policy.projects.is_empty() || !self.policy.allow_unscoped_execution {
                    return Err(
                        "task action resolution requires unrestricted planning access".into(),
                    );
                }
                let request = orbit_runtime::planning_brief::WorkBriefRequest {
                    now_ms: required_i64(args, "now_ms")?,
                    ends_at_ms: required_i64(args, "ends_at_ms")?,
                    limit: args.get("limit").and_then(Value::as_u64).unwrap_or(100) as usize,
                    project_id: args
                        .get("project_id")
                        .and_then(Value::as_str)
                        .map(str::to_owned),
                    default_minutes: None,
                };
                let brief = self.client.work_brief(&request).map_err(debug_err)?;
                let target = orbit_runtime::planning_actions::TaskActionRequest {
                    task_id: args
                        .get("task_id")
                        .and_then(Value::as_str)
                        .ok_or("task_id required")?
                        .into(),
                    authority_available: false,
                };
                let selection: orbit_runtime::planning_actions::ActionSelection =
                    serde_json::from_value(
                        args.get("selection").cloned().ok_or("selection required")?,
                    )
                    .map_err(debug_err)?;
                self.client
                    .resolve_planning_task_action(&brief, &target, &selection)
                    .map(|result| json!(result))
                    .map_err(debug_err)
            }
            "planning.resolve_group_action" => {
                if !self.policy.projects.is_empty() || !self.policy.allow_unscoped_execution {
                    return Err(
                        "project action resolution requires unrestricted planning access".into(),
                    );
                }
                let project_id = args
                    .get("project_id")
                    .and_then(Value::as_str)
                    .ok_or("project_id required")?
                    .to_owned();
                let request = orbit_runtime::planning_brief::WorkBriefRequest {
                    now_ms: required_i64(args, "now_ms")?,
                    ends_at_ms: required_i64(args, "ends_at_ms")?,
                    limit: args.get("limit").and_then(Value::as_u64).unwrap_or(100) as usize,
                    project_id: Some(project_id.clone()),
                    default_minutes: None,
                };
                let brief = self.client.work_brief(&request).map_err(debug_err)?;
                let target = orbit_runtime::planning_actions::GroupActionRequest {
                    project_id,
                    milestone_id: args
                        .get("milestone_id")
                        .and_then(Value::as_str)
                        .map(str::to_owned),
                    authority_available: false,
                };
                let selection: orbit_runtime::planning_actions::ActionSelection =
                    serde_json::from_value(
                        args.get("selection").cloned().ok_or("selection required")?,
                    )
                    .map_err(debug_err)?;
                self.client
                    .resolve_planning_group_action(&brief, &target, &selection)
                    .map(|result| json!(result))
                    .map_err(debug_err)
            }
            "planning.task_actions" => {
                if !self.policy.projects.is_empty() || !self.policy.allow_unscoped_execution {
                    return Err(
                        "task action discovery requires unrestricted accepted planning access"
                            .into(),
                    );
                }
                let request = orbit_runtime::planning_brief::WorkBriefRequest {
                    now_ms: required_i64(args, "now_ms")?,
                    ends_at_ms: required_i64(args, "ends_at_ms")?,
                    limit: args.get("limit").and_then(Value::as_u64).unwrap_or(100) as usize,
                    project_id: args
                        .get("project_id")
                        .and_then(Value::as_str)
                        .map(str::to_owned),
                    default_minutes: None,
                };
                let brief = self.client.work_brief(&request).map_err(debug_err)?;
                let menu = self
                    .client
                    .planning_task_actions(
                        &brief,
                        &orbit_runtime::planning_actions::TaskActionRequest {
                            task_id: args
                                .get("task_id")
                                .and_then(Value::as_str)
                                .ok_or("task_id required")?
                                .into(),
                            authority_available: false,
                        },
                    )
                    .map_err(debug_err)?;
                Ok(json!(menu))
            }
            "planning.group_actions" => {
                if !self.policy.projects.is_empty() || !self.policy.allow_unscoped_execution {
                    return Err(
                        "project action discovery requires unrestricted accepted planning access"
                            .into(),
                    );
                }
                let project_id = args
                    .get("project_id")
                    .and_then(Value::as_str)
                    .ok_or("project_id required")?
                    .to_owned();
                let request = orbit_runtime::planning_brief::WorkBriefRequest {
                    now_ms: required_i64(args, "now_ms")?,
                    ends_at_ms: required_i64(args, "ends_at_ms")?,
                    limit: args.get("limit").and_then(Value::as_u64).unwrap_or(100) as usize,
                    project_id: Some(project_id.clone()),
                    default_minutes: None,
                };
                let brief = self.client.work_brief(&request).map_err(debug_err)?;
                let menu = self
                    .client
                    .planning_group_actions(
                        &brief,
                        &orbit_runtime::planning_actions::GroupActionRequest {
                            project_id,
                            milestone_id: args
                                .get("milestone_id")
                                .and_then(Value::as_str)
                                .map(str::to_owned),
                            authority_available: false,
                        },
                    )
                    .map_err(debug_err)?;
                Ok(json!(menu))
            }
            "planning.task_focus" => {
                if !self.policy.projects.is_empty() || !self.policy.allow_unscoped_execution {
                    return Err("task navigation requires unrestricted planning read access".into());
                }
                let request = orbit_runtime::planning_brief::WorkBriefRequest {
                    now_ms: required_i64(args, "now_ms")?,
                    ends_at_ms: required_i64(args, "ends_at_ms")?,
                    limit: args.get("limit").and_then(Value::as_u64).unwrap_or(20) as usize,
                    project_id: args
                        .get("project_id")
                        .and_then(Value::as_str)
                        .map(str::to_owned),
                    default_minutes: None,
                };
                let focus = orbit_runtime::planning_focus::FocusRequest {
                    task_id: args
                        .get("task_id")
                        .and_then(Value::as_str)
                        .ok_or("task_id required")?
                        .into(),
                    max_neighbors: args
                        .get("max_neighbors")
                        .and_then(Value::as_u64)
                        .unwrap_or(12) as usize,
                };
                let brief = self.client.work_brief(&request).map_err(debug_err)?;
                self.client
                    .planning_focus(&brief, &focus)
                    .map(|report| json!(report))
                    .map_err(debug_err)
            }
            "planning.group_focus" => {
                if !self.policy.projects.is_empty() || !self.policy.allow_unscoped_execution {
                    return Err(
                        "project navigation requires unrestricted planning read access".into(),
                    );
                }
                let project_id = args
                    .get("project_id")
                    .and_then(Value::as_str)
                    .ok_or("project_id required")?
                    .to_owned();
                let request = orbit_runtime::planning_brief::WorkBriefRequest {
                    now_ms: required_i64(args, "now_ms")?,
                    ends_at_ms: required_i64(args, "ends_at_ms")?,
                    limit: args.get("limit").and_then(Value::as_u64).unwrap_or(100) as usize,
                    project_id: Some(project_id.clone()),
                    default_minutes: None,
                };
                let focus = orbit_runtime::planning_focus::GroupFocusRequest {
                    project_id,
                    milestone_id: args
                        .get("milestone_id")
                        .and_then(Value::as_str)
                        .map(str::to_owned),
                    max_tasks: args.get("max_tasks").and_then(Value::as_u64).unwrap_or(12) as usize,
                };
                let brief = self.client.work_brief(&request).map_err(debug_err)?;
                self.client
                    .planning_group(&brief, &focus)
                    .map(|report| json!(report))
                    .map_err(debug_err)
            }
            "planning.work_brief" => {
                if !self.policy.projects.is_empty() || !self.policy.allow_unscoped_execution {
                    return Err(
                        "planning overview requires unrestricted execution read access".into(),
                    );
                }
                let request: orbit_runtime::planning_brief::WorkBriefRequest =
                    serde_json::from_value(args.clone()).map_err(debug_err)?;
                self.client
                    .work_brief(&request)
                    .map(|report| json!(report))
                    .map_err(debug_err)
            }
            "habits.list" => self.read_family("habits"),
            "habit_occurrences.list" => self.read_family("habit_occurrences"),
            "metrics.list" => self.read_family("metrics"),
            "metric_observations.list" => self.read_family("metric_observations"),
            "schedule.list" => self.read_family("schedule"),
            "routines.list" => self.read_family("routines"),
            "milestones.list" => self.read_family("milestones"),
            "strategies.list" => self.read_family("strategies"),
            "authority.status" => {
                let snapshot = self.client.accepted_snapshot().map_err(debug_err)?;
                let unresolved = self.client.unresolved_commands().map_err(debug_err)?;
                Ok(
                    json!({"accepted_revision":snapshot.as_ref().and_then(|s|s.get("revision")),
                    "has_accepted_snapshot":snapshot.is_some(),"unresolved_count":unresolved.len()}),
                )
            }
            "authority.converge" => {
                self.policy.write_allowed()?;
                self.client
                    .converge()
                    .map(|cycle| json!(cycle))
                    .map_err(debug_err)
            }
            "authority.observe_convergence" => self
                .client
                .observe_convergence()
                .map(|report| json!(report))
                .map_err(debug_err),
            "authority.review_online" => {
                self.policy.write_allowed()?;
                let id = args
                    .get("command_id")
                    .and_then(Value::as_str)
                    .ok_or("missing command_id")?;
                self.client
                    .review_conflict_online(id)
                    .map(|review| json!(review))
                    .map_err(debug_err)
            }
            "authority.workspace_sync" => self
                .client
                .workspace_sync_report()
                .map(|report| json!(report))
                .map_err(debug_err),
            "authority.observe_workspace_sync" => self
                .client
                .observe_workspace_sync_report()
                .map(|report| json!(report))
                .map_err(debug_err),
            "authority.sync_health" => self
                .client
                .sync_health()
                .map(|status| json!(status))
                .map_err(debug_err),
            "authority.observe_sync_health" => self
                .client
                .observe_sync_health()
                .map(|status| json!(status))
                .map_err(debug_err),
            "authority.convergence" => self
                .client
                .convergence_status()
                .map(|status| json!(status))
                .map_err(debug_err),
            "authority.conflict_review" => {
                self.policy.write_allowed()?;
                let id = args
                    .get("command_id")
                    .and_then(Value::as_str)
                    .ok_or("missing command_id")?;
                self.client
                    .conflict_review(id)
                    .map(|review| json!(review))
                    .map_err(debug_err)
            }
            "authority.resolve_reviewed" | "authority.resolve" => {
                self.policy.write_allowed()?;
                let review: orbit_client::authority::ConflictReview =
                    serde_json::from_value(args.get("review").cloned().ok_or("missing review")?)
                        .map_err(debug_err)?;
                let replacement = args
                    .get("replacement_id")
                    .and_then(Value::as_str)
                    .ok_or("missing replacement_id")?;
                let family = args
                    .get("family")
                    .and_then(Value::as_str)
                    .ok_or("missing family")?;
                let action = args.get("action").ok_or("missing action")?;
                // Reuse existing scoped semantic permission checks before invoking
                // the snapshot-bound shared-client resolution.
                let kind = match family {
                    "project" => {
                        self.allow_project(
                            action
                                .get("project_id")
                                .and_then(Value::as_str)
                                .ok_or("missing project_id")?,
                        )?;
                        CommandKind::Project
                    }
                    "task" => {
                        let decoded: TaskAction =
                            serde_json::from_value(action.clone()).map_err(debug_err)?;
                        self.check_task_scope(&decoded)?;
                        CommandKind::Task
                    }
                    "evidence" => {
                        if !self.policy.allow_unscoped_execution {
                            return Err("unscoped permission required".into());
                        }
                        CommandKind::Evidence
                    }
                    "schedule" => {
                        if !self.policy.allow_unscoped_execution {
                            return Err("unscoped permission required".into());
                        }
                        CommandKind::Schedule
                    }
                    "routine" => {
                        if !self.policy.allow_unscoped_execution {
                            return Err("unscoped permission required".into());
                        }
                        CommandKind::Routine
                    }
                    "milestone" => {
                        let decoded: MilestoneAction =
                            serde_json::from_value(action.clone()).map_err(debug_err)?;
                        self.check_milestone_scope(&decoded)?;
                        CommandKind::Milestone
                    }
                    "strategy" => {
                        let decoded: orbit_runtime::authority_strategy::StrategyAction =
                            serde_json::from_value(action.clone()).map_err(debug_err)?;
                        self.allow_project(decoded.project_id())?;
                        CommandKind::Strategy
                    }
                    _ => return Err("unsupported resolution command family".into()),
                };
                self.client
                    .resolve_reviewed_conflict(&review, replacement, kind, action)
                    .map(Self::receipt)
                    .map_err(debug_err)
            }
            "authority.resolve_reviewed_batch" => {
                self.policy.write_allowed()?;
                let rows = args
                    .get("resolutions")
                    .and_then(Value::as_array)
                    .ok_or("resolutions array required")?;
                if rows.is_empty() || rows.len() > 100 {
                    return Err("reviewed batch must contain 1..=100 resolutions".into());
                }
                let mut decisions = Vec::with_capacity(rows.len());
                for (index, row) in rows.iter().enumerate() {
                    let review: orbit_client::authority::ConflictReview = serde_json::from_value(
                        row.get("review")
                            .cloned()
                            .ok_or_else(|| format!("resolution {index} missing review"))?,
                    )
                    .map_err(debug_err)?;
                    let replacement_id = row
                        .get("replacement_id")
                        .and_then(Value::as_str)
                        .ok_or_else(|| format!("resolution {index} missing replacement_id"))?;
                    let family = row
                        .get("family")
                        .and_then(Value::as_str)
                        .ok_or_else(|| format!("resolution {index} missing family"))?;
                    let action = row
                        .get("action")
                        .cloned()
                        .ok_or_else(|| format!("resolution {index} missing action"))?;
                    let kind = match family {
                        "project" => {
                            self.allow_project(
                                action
                                    .get("project_id")
                                    .and_then(Value::as_str)
                                    .ok_or("project_id required")?,
                            )?;
                            CommandKind::Project
                        }
                        "task" => {
                            let decoded: TaskAction =
                                serde_json::from_value(action.clone()).map_err(debug_err)?;
                            self.check_task_scope(&decoded)?;
                            CommandKind::Task
                        }
                        "evidence" => {
                            if !self.policy.allow_unscoped_execution {
                                return Err("evidence requires unscoped permission".into());
                            }
                            let _: EvidenceAction =
                                serde_json::from_value(action.clone()).map_err(debug_err)?;
                            CommandKind::Evidence
                        }
                        "schedule" => {
                            if !self.policy.allow_unscoped_execution {
                                return Err("schedule requires unscoped permission".into());
                            }
                            let _: orbit_runtime::authority_schedule::ScheduleAction =
                                serde_json::from_value(action.clone()).map_err(debug_err)?;
                            CommandKind::Schedule
                        }
                        "routine" => {
                            if !self.policy.allow_unscoped_execution {
                                return Err("routine requires unscoped permission".into());
                            }
                            let _: orbit_runtime::authority_routines::RoutineAction =
                                serde_json::from_value(action.clone()).map_err(debug_err)?;
                            CommandKind::Routine
                        }
                        "milestone" => {
                            let decoded: MilestoneAction =
                                serde_json::from_value(action.clone()).map_err(debug_err)?;
                            self.check_milestone_scope(&decoded)?;
                            CommandKind::Milestone
                        }
                        "strategy" => {
                            let decoded: orbit_runtime::authority_strategy::StrategyAction =
                                serde_json::from_value(action.clone()).map_err(debug_err)?;
                            self.allow_project(decoded.project_id())?;
                            CommandKind::Strategy
                        }
                        _ => return Err(format!("resolution {index} has unsupported family")),
                    };
                    decisions.push(orbit_client::authority::ReviewedReplacement {
                        review,
                        replacement_id: replacement_id.to_owned(),
                        kind,
                        action,
                    });
                }
                let staged = self
                    .client
                    .resolve_reviewed_batch(&decisions)
                    .map_err(debug_err)?;
                Ok(json!({"staged":staged,"accepted":false,
                    "atomic_local_staging":true,"server_acceptance_is_sequential":true}))
            }
            "authority.unresolved" => {
                self.policy.write_allowed()?;
                self.client
                    .unresolved_commands()
                    .map_err(debug_err)
                    .map(|rows| json!(rows))
            }
            "authority.refresh" => {
                // Refreshing the accepted cache is a local read: it queues and
                // accepts nothing. Read-only agents need it to make a fresh
                // connected session usable.
                self.client.refresh().map_err(debug_err)?;
                self.dispatch("authority.status", &json!({}))
            }
            "authority.flush" => {
                self.policy.write_allowed()?;
                let outcome = self.client.flush().map_err(debug_err)?;
                Ok(json!({"outcome":format!("{outcome:?}")}))
            }
            "authority.workflow" => {
                let steps = self.decode_workflow(args)?;
                match self.client.execute_workflow(steps) {
                    Ok(results) => Ok(
                        json!({"completed":results.into_iter().map(Self::receipt).collect::<Vec<_>>(),"failed_index":null}),
                    ),
                    Err(failure) => Ok(
                        json!({"completed":failure.completed.into_iter().map(Self::receipt).collect::<Vec<_>>(),"failed_index":failure.failed_index,"error":format!("{:?}",failure.cause)}),
                    ),
                }
            }
            "project.command" => {
                self.policy.write_allowed()?;
                let cmd: ProjectCommand =
                    serde_json::from_value(args.clone()).map_err(debug_err)?;
                self.allow_project(&cmd.project_id)?;
                self.client
                    .create_project(cmd)
                    .map(Self::receipt)
                    .map_err(debug_err)
            }
            "task.command" => {
                self.policy.write_allowed()?;
                let cmd: TaskCommand = serde_json::from_value(args.clone()).map_err(debug_err)?;
                self.check_task_scope(&cmd.action)?;
                self.client
                    .task_command(cmd)
                    .map(Self::receipt)
                    .map_err(debug_err)
            }
            "evidence.command" => {
                self.policy.write_allowed()?;
                if !self.policy.allow_unscoped_execution {
                    return Err(
                        "habit/metric evidence requires unscoped execution permission".into(),
                    );
                }
                let cmd: EvidenceCommand =
                    serde_json::from_value(args.clone()).map_err(debug_err)?;
                // Evidence identifiers are not project-scoped; permission is
                // checked before any command reaches the durable outbox.
                match &cmd.action {
                    EvidenceAction::CreateHabit { .. }
                    | EvidenceAction::RecordHabit { .. }
                    | EvidenceAction::SetMetricTarget { .. }
                    | EvidenceAction::RecordMetric { .. } => {}
                }
                self.client
                    .evidence_command(cmd)
                    .map(Self::receipt)
                    .map_err(debug_err)
            }
            "schedule.command" => {
                self.policy.write_allowed()?;
                if !self.policy.allow_unscoped_execution {
                    return Err("schedule commands require unscoped execution permission".into());
                }
                let cmd: ScheduleCommand =
                    serde_json::from_value(args.clone()).map_err(debug_err)?;
                self.client
                    .schedule_command(cmd)
                    .map(Self::receipt)
                    .map_err(debug_err)
            }
            "routine.command" => {
                self.policy.write_allowed()?;
                if !self.policy.allow_unscoped_execution {
                    return Err("routine commands require unscoped execution permission".into());
                }
                let cmd: RoutineCommand =
                    serde_json::from_value(args.clone()).map_err(debug_err)?;
                self.client
                    .routine_command(cmd)
                    .map(Self::receipt)
                    .map_err(debug_err)
            }
            "milestone.command" => {
                self.policy.write_allowed()?;
                let cmd: MilestoneCommand =
                    serde_json::from_value(args.clone()).map_err(debug_err)?;
                self.check_milestone_scope(&cmd.action)?;
                self.client
                    .milestone_command(cmd)
                    .map(Self::receipt)
                    .map_err(debug_err)
            }
            "strategy.command" => {
                self.policy.write_allowed()?;
                let cmd: StrategyCommand =
                    serde_json::from_value(args.clone()).map_err(debug_err)?;
                self.allow_project(cmd.action.project_id())?;
                self.client
                    .strategy_command(cmd)
                    .map(Self::receipt)
                    .map_err(debug_err)
            }
            _ => Err(format!(
                "tool '{name}' not supported by connected desktop MCP; use the canonical authority tools"
            )),
        }
    }
}

/// Wire shape for one ordered acceptance criterion. Positions come from the
/// authority list order; checking a criterion never implies a task transition.
fn criterion_rows(criteria: &[orbit_client::AcceptanceCriterionDto]) -> Vec<Value> {
    criteria
        .iter()
        .map(|criterion| {
            json!({
                "id": criterion.id,
                "text": criterion.text,
                "completed": criterion.completed,
                "completed_at_ms": criterion.completed_at_ms,
                "position": criterion.position,
            })
        })
        .collect()
}

/// `priority: null` or `"none"` clears the label; any other value is passed to
/// the client, which canonicalises and validates it before queueing.
fn optional_priority(value: &Value) -> Result<Option<String>, String> {
    match value {
        Value::Null => Ok(None),
        Value::String(text) => {
            let trimmed = text.trim();
            if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("none") {
                Ok(None)
            } else {
                Ok(Some(trimmed.to_owned()))
            }
        }
        _ => Err("priority must be a string or null".into()),
    }
}

fn labels_arg(value: &Value) -> Result<Vec<String>, String> {
    let Some(items) = value.as_array() else {
        return Err("labels must be an array of strings".into());
    };
    items
        .iter()
        .map(|item| {
            item.as_str()
                .map(str::to_owned)
                .ok_or_else(|| "labels must be an array of strings".to_owned())
        })
        .collect()
}

/// Wrap the payload in the explicit mutation response contract.
///
/// `server_state` is always the last accepted projection; it is never the
/// queued value. `accepted` means the exact command is already committed,
/// `pending` means it is still only durable local intent, and `conflict` means
/// the authority permanently rejected it.
fn mutation_body(server_state: Value, outcome: MutationOutcome) -> Value {
    let mut body = json!({
        "accepted": outcome.accepted(),
        "pending": outcome.pending(),
        "disposition": outcome.disposition(),
        "server_state": server_state,
    });
    if let Some(message) = outcome.message() {
        body.as_object_mut()
            .expect("mutation body is an object")
            .insert("message".into(), json!(message));
    }
    body
}
