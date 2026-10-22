//! Embedded MCP adapter. One host-owned runtime executes domain rules.
//! Never opens an independent repository for an MCP tool invocation.
use super::*;

impl Server {
    // Agent job reports must be bound to the launched job, not merely an MCP
    // session that happens to have general task write permissions.
    fn authorize_agent_job(&self, job_id: &str) -> Result<(), String> {
        let bound = std::env::var("ORBIT_MCP_JOB_ID")
            .map_err(|_| "ORBIT_MCP_JOB_ID is required for job-scoped tools".to_string())?;
        if bound.is_empty() || bound != job_id {
            return Err("job outside MCP session scope".into());
        }
        let context = self.authority.local_agent_resume_context(job_id)?;
        let project = context
            .get("job")
            .and_then(|v| v.get("project_id"))
            .and_then(Value::as_str)
            .ok_or("job has no project")?;
        let project = ProjectId::new(project.to_owned()).map_err(debug_err)?;
        if !self.policy.project_allowed(&project) {
            return Err("job outside agent project scope".into());
        }
        Ok(())
    }
    pub(super) fn dispatch(&mut self, name: &str, args: &Value) -> Result<Value, String> {
        if name.starts_with("blueprint.") {
            return self.blueprint_dispatch(name, args);
        }
        if let Some(result) = self.dispatch_rich(name, args) {
            return result;
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
                let (_, tasks) = self.authority.tasks().map_err(debug_err)?;
                for id in [&task_id, &prerequisite_id] {
                    let row = tasks
                        .iter()
                        .find(|row| &row.id == id)
                        .ok_or("task absent from accepted authoritative state")?;
                    match &row.project_id {
                        Some(project) => {
                            let project = ProjectId::new(project.clone()).map_err(debug_err)?;
                            if !self.policy.project_allowed(&project) {
                                return Err("task outside agent project scope".into());
                            }
                        }
                        None if !self.policy.allow_unscoped_execution => {
                            return Err("unscoped task requires execution permission".into());
                        }
                        None => {}
                    }
                }
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
                let actor = format!("mcp_{}_{}", self.policy.agent_id, self.policy.session_id)
                    .replace(
                        |c: char| !(c.is_ascii_alphanumeric() || c == '-' || c == '_'),
                        "_",
                    );
                serde_json::to_value(
                    self.authority
                        .task_command(&actor, command)
                        .map_err(debug_err)?,
                )
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
                let (_, tasks) = self.authority.tasks().map_err(debug_err)?;
                let edges = self.authority.task_dependency_edges().map_err(debug_err)?;
                let revision = self.authority.snapshot().map_err(debug_err)?.0;
                let report = orbit_runtime::task_dependency_preview::preview_dependency(
                    revision, &tasks, &edges, &request,
                )
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
                serde_json::to_value(self.authority.task_graph(now).map_err(debug_err)?)
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
                serde_json::to_value(self.authority.next_actions(now, limit).map_err(debug_err)?)
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
                let actor = format!("mcp_{}_{}", self.policy.agent_id, self.policy.session_id)
                    .replace(
                        |c: char| !(c.is_ascii_alphanumeric() || c == '-' || c == '_'),
                        "_",
                    );
                serde_json::to_value(
                    self.authority
                        .execute_schedule_request(&actor, request)
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
                let actor = format!("mcp_{}_{}", self.policy.agent_id, self.policy.session_id)
                    .replace(
                        |c: char| !(c.is_ascii_alphanumeric() || c == '-' || c == '_'),
                        "_",
                    );
                serde_json::to_value(
                    self.authority
                        .review_scenario_options(&actor, &request)
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
                let actor = format!("mcp_{}_{}", self.policy.agent_id, self.policy.session_id)
                    .replace(
                        |c: char| !(c.is_ascii_alphanumeric() || c == '-' || c == '_'),
                        "_",
                    );
                serde_json::to_value(
                    self.authority
                        .review_scenario_execution(&actor, &request)
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
                    self.authority
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
                    self.authority
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
                serde_json::to_value(
                    self.authority
                        .portfolio_actions(&request)
                        .map_err(debug_err)?,
                )
                .map_err(debug_err)
            }
            "task.readiness" => {
                if !self.policy.projects.is_empty() || !self.policy.allow_unscoped_execution {
                    return Err(
                        "workspace planning requires unrestricted execution read access".into(),
                    );
                }
                let now = required_i64(args, "now_ms")?;
                serde_json::to_value(self.authority.task_readiness(now).map_err(debug_err)?)
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
                serde_json::to_value(self.authority.day_plan(&request).map_err(debug_err)?)
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
                serde_json::to_value(self.authority.schedule_plan(&request).map_err(debug_err)?)
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
                    self.authority
                        .workload_insights(now, horizon)
                        .map_err(debug_err)?,
                )
                .map_err(debug_err)
            }
            "agent.job_context" => {
                let job_id = required_str(args, "job_id")?;
                self.authorize_agent_job(&job_id)?;
                self.authority.local_agent_resume_context(&job_id)
            }
            "agent.handoff_submit" => {
                self.policy.write_allowed()?;
                let job_id = required_str(args, "job_id")?;
                self.authorize_agent_job(&job_id)?;
                let handoff_id = required_str(args, "handoff_id")?;
                let payload = args.get("payload").ok_or("missing payload")?;
                if !payload.is_object() {
                    return Err("handoff payload must be an object".into());
                }
                let size = serde_json::to_vec(payload).map_err(debug_err)?.len();
                if size > 64 * 1024 {
                    return Err("handoff exceeds 64 KiB".into());
                }
                let at_ms = required_i64(args, "at_ms")?;
                if at_ms < 0 {
                    return Err("handoff timestamp must be nonnegative".into());
                }
                self.authority
                    .record_local_agent_handoff(&job_id, &handoff_id, payload, at_ms)?;
                Ok(
                    json!({"job_id":job_id,"handoff_id":handoff_id,"recorded":true,
                    "task_completed":false}),
                )
            }
            "agent.inspect" => Ok(json!({
                "agent_id": self.policy.agent_id.clone(),
                "session_id": self.policy.session_id.clone(),
                "mode": if self.policy.mode == Mode::Developer { "developer" } else { "read_only" },
                "project_allowlist": self.policy.projects.iter().cloned().collect::<Vec<_>>(),
                "allow_unscoped_execution": self.policy.allow_unscoped_execution,
                "allow_unscoped_knowledge": self.policy.allow_unscoped_knowledge,
            })),
            "projects.list" => self.projects_list(),
            "project.evidence_report" => self.project_evidence_report(args),
            "project.health" => self.project_health(args),
            "project.evidence_list" => self.project_evidence_list(args),
            "project.evidence_bind" => self.project_evidence_change(args, true),
            "project.evidence_unbind" => self.project_evidence_change(args, false),
            "project.metric_set_target" => self.project_metric_target(args),
            "project.metric_observe" => self.project_metric_observe(args),
            "tasks.list" => self.tasks_list(),
            "planning.handoff_task" => self.planning_handoff_task(args),
            "planning.handoff_group" => self.planning_handoff_group(args),
            "planning.resolve_task_action" => self.planning_resolve_task_action(args),
            "planning.resolve_group_action" => self.planning_resolve_group_action(args),
            "planning.task_actions" => self.planning_task_actions(args),
            "planning.group_actions" => self.planning_group_actions(args),
            "planning.task_focus" => self.planning_task_focus(args),
            "planning.group_focus" => self.planning_group_focus(args),
            "planning.work_brief" => self.planning_work_brief(args),
            "planning.completion_impact" => self.planning_completion_impact(args),
            "milestones.list" => self.milestones_list(args),
            "habits.list" => self.habits_list(),
            "priorities.list" => self.priorities_list(),
            "knowledge.tree" => self.markdown_tree(),
            "knowledge.read" => self.markdown_read(args),
            "knowledge.search" => self.markdown_search(args),
            "knowledge.relations" => self.markdown_relations(args),
            "knowledge.command" => self.markdown_command(args),
            "knowledge.folder_move" => self.markdown_folder_move(args),
            "knowledge.folder_create" => self.markdown_folder_create(args),
            "task.create" => self.task_create(args),
            "task.set_status" => self.task_set_status(args),
            "project.set_status" => self.project_set_status(args),
            "project.configure_strategy" => self.project_configure_strategy(args),
            "project.set_criterion_status" => self.project_set_criterion_status(args),
            "milestone.set_status" => self.milestone_set_status(args),
            "habit.set_active" => self.habit_set_active(args),
            _ => Err(format!("unknown tool: {name}")),
        }
    }

    fn planning_completion_impact(&self, args: &Value) -> Result<Value, String> {
        if !self.policy.projects.is_empty() || !self.policy.allow_unscoped_execution {
            return Err("completion impact requires unrestricted execution read access".into());
        }
        let request: orbit_runtime::task_impact::CompletionImpactRequest =
            serde_json::from_value(args.clone()).map_err(debug_err)?;
        serde_json::to_value(
            self.authority
                .preview_task_completion(&request)
                .map_err(debug_err)?,
        )
        .map_err(debug_err)
    }

    fn planning_work_brief(&self, args: &Value) -> Result<Value, String> {
        // A dependency graph can expose task identities from other projects.
        // Deny restricted agents instead of returning partially redacted edges.
        if !self.policy.projects.is_empty() || !self.policy.allow_unscoped_execution {
            return Err("planning overview requires unrestricted execution read access".into());
        }
        let request: orbit_runtime::planning_brief::WorkBriefRequest =
            serde_json::from_value(args.clone()).map_err(debug_err)?;
        serde_json::to_value(self.authority.work_brief(&request).map_err(debug_err)?)
            .map_err(debug_err)
    }

    fn planning_handoff_task(&self, args: &Value) -> Result<Value, String> {
        if !self.policy.projects.is_empty() || !self.policy.allow_unscoped_execution {
            return Err("planning handoff requires unrestricted accepted planning access".into());
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
        let brief = self.authority.work_brief(&request).map_err(debug_err)?;
        let selection: orbit_runtime::planning_actions::ActionSelection =
            serde_json::from_value(args.get("selection").cloned().ok_or("selection required")?)
                .map_err(debug_err)?;
        let target = orbit_runtime::planning_actions::TaskActionRequest {
            task_id: args
                .get("task_id")
                .and_then(Value::as_str)
                .ok_or("task_id required")?
                .into(),
            authority_available: true,
        };
        serde_json::to_value(
            orbit_runtime::planning_handoff::dispatch_task(&brief, &target, &selection)
                .map_err(str::to_owned)?,
        )
        .map_err(debug_err)
    }
    fn planning_handoff_group(&self, args: &Value) -> Result<Value, String> {
        if !self.policy.projects.is_empty() || !self.policy.allow_unscoped_execution {
            return Err("planning handoff requires unrestricted accepted planning access".into());
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
        let brief = self.authority.work_brief(&request).map_err(debug_err)?;
        let selection: orbit_runtime::planning_actions::ActionSelection =
            serde_json::from_value(args.get("selection").cloned().ok_or("selection required")?)
                .map_err(debug_err)?;
        let target = orbit_runtime::planning_actions::GroupActionRequest {
            project_id,
            milestone_id: args
                .get("milestone_id")
                .and_then(Value::as_str)
                .map(str::to_owned),
            authority_available: true,
        };
        serde_json::to_value(
            orbit_runtime::planning_handoff::dispatch_group(&brief, &target, &selection)
                .map_err(str::to_owned)?,
        )
        .map_err(debug_err)
    }
    fn planning_resolve_task_action(&self, args: &Value) -> Result<Value, String> {
        if !self.policy.projects.is_empty() || !self.policy.allow_unscoped_execution {
            return Err(
                "task action resolution requires unrestricted accepted planning access".into(),
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
        let brief = self.authority.work_brief(&request).map_err(debug_err)?;
        let target = orbit_runtime::planning_actions::TaskActionRequest {
            task_id: args
                .get("task_id")
                .and_then(Value::as_str)
                .ok_or("task_id required")?
                .into(),
            authority_available: true,
        };
        let selection: orbit_runtime::planning_actions::ActionSelection =
            serde_json::from_value(args.get("selection").cloned().ok_or("selection required")?)
                .map_err(debug_err)?;
        serde_json::to_value(
            self.authority
                .resolve_planning_task_action(&brief, &target, &selection)
                .map_err(debug_err)?,
        )
        .map_err(debug_err)
    }
    fn planning_resolve_group_action(&self, args: &Value) -> Result<Value, String> {
        if !self.policy.projects.is_empty() || !self.policy.allow_unscoped_execution {
            return Err(
                "project action resolution requires unrestricted accepted planning access".into(),
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
        let brief = self.authority.work_brief(&request).map_err(debug_err)?;
        let target = orbit_runtime::planning_actions::GroupActionRequest {
            project_id,
            milestone_id: args
                .get("milestone_id")
                .and_then(Value::as_str)
                .map(str::to_owned),
            authority_available: true,
        };
        let selection: orbit_runtime::planning_actions::ActionSelection =
            serde_json::from_value(args.get("selection").cloned().ok_or("selection required")?)
                .map_err(debug_err)?;
        serde_json::to_value(
            self.authority
                .resolve_planning_group_action(&brief, &target, &selection)
                .map_err(debug_err)?,
        )
        .map_err(debug_err)
    }
    fn planning_task_actions(&self, args: &Value) -> Result<Value, String> {
        if !self.policy.projects.is_empty() || !self.policy.allow_unscoped_execution {
            return Err(
                "task action discovery requires unrestricted accepted planning access".into(),
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
        let brief = self.authority.work_brief(&request).map_err(debug_err)?;
        let task_id = args
            .get("task_id")
            .and_then(Value::as_str)
            .ok_or("task_id required")?
            .to_owned();
        let menu = self
            .authority
            .planning_task_actions(
                &brief,
                &orbit_runtime::planning_actions::TaskActionRequest {
                    task_id,
                    authority_available: true,
                },
            )
            .map_err(debug_err)?;
        serde_json::to_value(menu).map_err(debug_err)
    }
    fn planning_group_actions(&self, args: &Value) -> Result<Value, String> {
        if !self.policy.projects.is_empty() || !self.policy.allow_unscoped_execution {
            return Err(
                "project action discovery requires unrestricted accepted planning access".into(),
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
        let brief = self.authority.work_brief(&request).map_err(debug_err)?;
        let menu = self
            .authority
            .planning_group_actions(
                &brief,
                &orbit_runtime::planning_actions::GroupActionRequest {
                    project_id,
                    milestone_id: args
                        .get("milestone_id")
                        .and_then(Value::as_str)
                        .map(str::to_owned),
                    authority_available: true,
                },
            )
            .map_err(debug_err)?;
        serde_json::to_value(menu).map_err(debug_err)
    }
    fn planning_task_focus(&self, args: &Value) -> Result<Value, String> {
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
        let brief = self.authority.work_brief(&request).map_err(debug_err)?;
        serde_json::to_value(
            self.authority
                .planning_focus(&brief, &focus)
                .map_err(debug_err)?,
        )
        .map_err(debug_err)
    }
    fn planning_group_focus(&self, args: &Value) -> Result<Value, String> {
        if !self.policy.projects.is_empty() || !self.policy.allow_unscoped_execution {
            return Err("project navigation requires unrestricted planning read access".into());
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
        let brief = self.authority.work_brief(&request).map_err(debug_err)?;
        serde_json::to_value(
            self.authority
                .planning_group(&brief, &focus)
                .map_err(debug_err)?,
        )
        .map_err(debug_err)
    }

    // Knowledge is a physical Markdown vault. Legacy SqliteStore knowledge
    // repository operations are intentionally not reachable from MCP.
    // Project-scoped grants do not imply permission to inspect unscoped notes.
    fn markdown_authority(&self) -> Result<&AuthoritativeHost, String> {
        if !self.policy.allow_unscoped_knowledge {
            return Err("unscoped Markdown vault access is not authorized".into());
        }
        Ok(&self.authority)
    }
    fn markdown_authority_mut(&mut self) -> Result<&mut AuthoritativeHost, String> {
        if !self.policy.allow_unscoped_knowledge {
            return Err("unscoped Markdown vault access is not authorized".into());
        }
        Ok(&mut self.authority)
    }
    fn markdown_tree(&self) -> Result<Value, String> {
        serde_json::to_value(
            self.markdown_authority()?
                .knowledge_tree()
                .map_err(debug_err)?,
        )
        .map_err(debug_err)
    }
    fn markdown_read(&self, args: &Value) -> Result<Value, String> {
        let id = required_str(args, "note_id")?;
        let entry = self
            .markdown_authority()?
            .markdown_by_id(&id)
            .map_err(debug_err)?;
        Ok(match entry {
            Some((info, body)) => json!({"note": info, "markdown": body}),
            None => Value::Null,
        })
    }
    fn markdown_search(&self, args: &Value) -> Result<Value, String> {
        let q = required_str(args, "text")?;
        let limit = args
            .get("limit")
            .and_then(Value::as_u64)
            .unwrap_or(50)
            .min(200) as usize;
        serde_json::to_value(
            self.markdown_authority()?
                .markdown_search(&q, limit)
                .map_err(debug_err)?,
        )
        .map_err(debug_err)
    }
    fn markdown_relations(&self, args: &Value) -> Result<Value, String> {
        let id = required_str(args, "note_id")?;
        serde_json::to_value(
            self.markdown_authority()?
                .markdown_relations(&id)
                .map_err(debug_err)?,
        )
        .map_err(debug_err)
    }
    fn markdown_command(&mut self, args: &Value) -> Result<Value, String> {
        self.policy.write_allowed()?;
        // The caller supplies a stable command ID to permit safe retries.
        let command_id = required_str(args, "command_id")?;
        let command: MarkdownCommand =
            serde_json::from_value(args.get("command").cloned().ok_or("missing command")?)
                .map_err(debug_err)?;
        let actor = format!("mcp_{}_{}", self.policy.agent_id, self.policy.session_id).replace(
            |c: char| !(c.is_ascii_alphanumeric() || c == '-' || c == '_'),
            "_",
        );
        let receipt = self
            .markdown_authority_mut()?
            .execute_markdown_command(&actor, &command_id, command)
            .map_err(debug_err)?;
        serde_json::to_value(receipt).map_err(debug_err)
    }
    fn markdown_folder_create(&mut self, args: &Value) -> Result<Value, String> {
        self.policy.write_allowed()?;
        let command_id = required_str(args, "command_id")?;
        let command: FolderCreate =
            serde_json::from_value(args.get("command").cloned().ok_or("missing command")?)
                .map_err(debug_err)?;
        let actor = format!("mcp_{}_{}", self.policy.agent_id, self.policy.session_id).replace(
            |c: char| !(c.is_ascii_alphanumeric() || c == '-' || c == '_'),
            "_",
        );
        let receipt = self
            .markdown_authority_mut()?
            .execute_folder_create(&actor, &command_id, command)
            .map_err(debug_err)?;
        serde_json::to_value(receipt).map_err(debug_err)
    }
    fn markdown_folder_move(&mut self, args: &Value) -> Result<Value, String> {
        self.policy.write_allowed()?;
        let command_id = required_str(args, "command_id")?;
        let mv: FolderMove =
            serde_json::from_value(args.get("move").cloned().ok_or("missing move")?)
                .map_err(debug_err)?;
        let actor = format!("mcp_{}_{}", self.policy.agent_id, self.policy.session_id).replace(
            |c: char| !(c.is_ascii_alphanumeric() || c == '-' || c == '_'),
            "_",
        );
        let receipt = self
            .markdown_authority_mut()?
            .execute_folder_move(&actor, &command_id, mv)
            .map_err(debug_err)?;
        serde_json::to_value(receipt).map_err(debug_err)
    }

    fn scoped_project(&self, args: &Value) -> Result<ProjectId, String> {
        let id = ProjectId::new(required_str(args, "project_id")?).map_err(debug_err)?;
        if !self.policy.project_allowed(&id) {
            return Err("project outside agent scope".into());
        }
        Ok(id)
    }

    fn project_evidence_report(&self, args: &Value) -> Result<Value, String> {
        let id = self.scoped_project(args)?;
        let start = required_i64(args, "start_ms")?;
        let end = required_i64(args, "end_ms")?;
        let as_of = required_i64(args, "as_of_ms")?;
        if end > as_of {
            return Err("report end must not exceed as_of_ms".into());
        }
        let period = HabitPeriod::new(start, end).map_err(debug_err)?;
        let report = self
            .authority
            .application_queries()
            .project_evidence_report(&id, &[period], as_of)
            .map_err(debug_err)?;
        Ok(
            json!({"project_id": id.as_str(), "progress_bp": report.evidence_progress.map(|p| p.value()),
            "habits": report.habits.habits.iter().map(|h| json!({"id": h.habit_id.as_str(), "available": h.available,
                "fulfilled_periods": h.fulfilled_periods, "evaluated_periods": h.evaluated_periods,
                "consistency_bp": h.consistency.map(|p| p.value())})).collect::<Vec<_>>(),
            "metrics": report.metrics.iter().map(|m| json!({"id": m.metric_id,
                "progress_bp": m.evaluation.map(|e| e.progress.value()),
                "reached": m.evaluation.map(|e| e.reached)})).collect::<Vec<_>>() }),
        )
    }

    fn project_health(&self, args: &Value) -> Result<Value, String> {
        let id = self.scoped_project(args)?;
        let lookback = args
            .get("lookback_minutes")
            .and_then(Value::as_u64)
            .unwrap_or(10080);
        let as_of = now_ms();
        let window = HabitPeriod::new(as_of.saturating_sub(7 * 24 * 60 * 60 * 1000), as_of)
            .map_err(debug_err)?;
        let result = self
            .authority
            .application_queries()
            .mode_aware_project_health(&id, &[window], as_of, lookback, HealthPolicy::default())
            .map_err(debug_err)?;
        let snapshot = &result.canonical;
        Ok(
            json!({"project_id": id.as_str(), "mode": snapshot.mode.as_str(),
            "completion_ready": snapshot.completion_ready,
            "required_criteria_total": snapshot.required_criteria_total,
            "required_criteria_satisfied": snapshot.required_criteria_satisfied,
            "task_progress_bp": snapshot.rollup.progress.value(),
            "mode_progress_bp": result.selected_progress.map(|p|p.value()),
            "evidence_progress_bp": result.bound_evidence.evidence_progress.map(|p|p.value()),
            "available_signals":result.available_signals.iter().map(|s|s.as_str()).collect::<Vec<_>>(),
            "unavailable_signals":result.unavailable_signals.iter().map(|s|s.as_str()).collect::<Vec<_>>(),
            "health_state": format!("{:?}", snapshot.health.state).to_lowercase(),
            "health_reasons": snapshot.health.reasons.iter().map(|r| format!("{:?}", r)).collect::<Vec<_>>() }),
        )
    }

    fn project_evidence_list(&self, args: &Value) -> Result<Value, String> {
        let id = self.scoped_project(args)?;
        let bindings = self
            .authority
            .application_queries()
            .project_evidence(&id)
            .map_err(debug_err)?;
        Ok(json!(
            bindings
                .iter()
                .map(|b| match b {
                    ProjectEvidenceBinding::Habit(h) => json!({"kind":"habit", "id":h.as_str()}),
                    ProjectEvidenceBinding::Metric(m) => json!({"kind":"metric", "id":m}),
                })
                .collect::<Vec<_>>()
        ))
    }

    fn project_evidence_change(&mut self, args: &Value, add: bool) -> Result<Value, String> {
        self.policy.write_allowed()?;
        let id = self.scoped_project(args)?;
        let evidence_id = required_str(args, "evidence_id")?;
        let binding = match required_str(args, "kind")?.as_str() {
            "habit" => ProjectEvidenceBinding::Habit(HabitId::new(evidence_id).map_err(debug_err)?),
            "metric" => ProjectEvidenceBinding::Metric(evidence_id),
            _ => return Err("kind must be habit or metric".into()),
        };
        let command = ChangeProjectEvidenceCommand {
            project_id: id.clone(),
            binding,
        };
        let revision = self.authority.snapshot().map_err(debug_err)?.0;
        self.authority
            .change_project_evidence_accepted(
                "orbit-mcp",
                &mcp_command_id(args),
                revision,
                add,
                command,
            )
            .map_err(debug_err)?;
        Ok(json!({"project_id":id.as_str(), "action": if add {"bound"} else {"unbound"}}))
    }

    fn scoped_bound_metric(&self, args: &Value) -> Result<String, String> {
        let id = self.scoped_project(args)?;
        let metric_id = required_str(args, "metric_id")?;
        let bindings = self
            .authority
            .application_queries()
            .project_evidence(&id)
            .map_err(debug_err)?;
        if !bindings.contains(&ProjectEvidenceBinding::Metric(metric_id.clone())) {
            return Err("metric is not bound to this project".into());
        }
        Ok(metric_id)
    }

    fn project_metric_target(&mut self, args: &Value) -> Result<Value, String> {
        self.policy.write_allowed()?;
        let metric_id = self.scoped_bound_metric(args)?;
        let direction = match required_str(args, "direction")?.as_str() {
            "at_least" => orbit_progress::MetricTargetDirection::AtLeast,
            "at_most" => orbit_progress::MetricTargetDirection::AtMost,
            _ => return Err("direction must be at_least or at_most".into()),
        };
        {
            let revision = self.authority.snapshot().map_err(debug_err)?.0;
            self.authority
                .set_metric_target_accepted(
                    "orbit-mcp",
                    &mcp_command_id(args),
                    revision,
                    orbit_application::MetricTargetRecord {
                        metric_id: metric_id.clone(),
                        baseline: required_i64(args, "baseline")?,
                        target: required_i64(args, "target")?,
                        direction,
                    },
                )
                .map_err(debug_err)?;
        };
        Ok(json!({"metric_id":metric_id,"updated":true}))
    }

    fn project_metric_observe(&mut self, args: &Value) -> Result<Value, String> {
        self.policy.write_allowed()?;
        let metric_id = self.scoped_bound_metric(args)?;
        {
            let revision = self.authority.snapshot().map_err(debug_err)?.0;
            self.authority
                .record_metric_observation_accepted(
                    "orbit-mcp",
                    &mcp_command_id(args),
                    revision,
                    orbit_application::MetricObservation {
                        metric_id: metric_id.clone(),
                        observed_at_ms: required_i64(args, "observed_at_ms")?,
                        value: required_i64(args, "value")?,
                    },
                )
                .map_err(debug_err)?;
        };
        Ok(json!({"metric_id":metric_id,"recorded":true}))
    }

    fn projects_list(&self) -> Result<Value, String> {
        let rows = self.authority.application_queries().projects().map_err(debug_err)?.into_iter()
            .filter(|p| self.policy.project_allowed(p.id()))
            .map(|p| json!({
                "id":p.id().as_str(),"name":p.name(),"status":p.status().as_str(),"mode":p.mode().as_str(),
                "objective":p.objective(),"progress_signals":p.progress_signals().iter().map(|s|s.as_str()).collect::<Vec<_>>(),
                "success_criteria":p.success_criteria().iter().map(|c|json!({"key":c.key(),"description":c.description(),"required":c.required(),"status":c.status().as_str()})).collect::<Vec<_>>(),
                "completion_ready":p.required_completion_ready()
            })).collect::<Vec<_>>();
        Ok(json!(rows))
    }

    fn tasks_list(&self) -> Result<Value, String> {
        let rows = self.authority.application_queries().tasks().map_err(debug_err)?.into_iter().filter(|t| {
            match t.scope().project_id() {
                Some(id) => self.policy.project_allowed(id),
                None => self.policy.allow_unscoped_execution,
            }
        }).map(|t| json!({
            "id":t.id().as_str(),"title":t.title(),"status":t.status().as_str(),
            "project_id":t.scope().project_id().map(|x|x.as_str()),
            "milestone_id":t.scope().milestone_id().map(|x|x.as_str()),
            "estimated_minutes":t.planning().estimated_minutes(),"deadline_at_ms":t.planning().deadline_at_ms()
        })).collect::<Vec<_>>();
        Ok(json!(rows))
    }

    fn milestones_list(&self, args: &Value) -> Result<Value, String> {
        let project = ProjectId::new(required_str(args, "project_id")?).map_err(debug_err)?;
        if !self.policy.project_allowed(&project) {
            return Err("project outside agent scope".into());
        }
        let rows = self.authority.application_queries().project_milestones(&project).map_err(debug_err)?.into_iter()
            .map(|m|json!({"id":m.id().as_str(),"project_id":m.project_id().as_str(),"name":m.name(),"status":m.status().as_str()})).collect::<Vec<_>>();
        Ok(json!(rows))
    }

    fn habits_list(&self) -> Result<Value, String> {
        if !self.policy.allow_unscoped_execution {
            return Err("unscoped habits are not visible to this agent".into());
        }
        let rows = self.authority.application_queries().habits().map_err(debug_err)?.into_iter()
            .map(|h|json!({"id":h.id().as_str(),"name":h.name(),"cadence":format!("{:?}",h.cadence()).to_lowercase(),"active":h.is_active()})).collect::<Vec<_>>();
        Ok(json!(rows))
    }

    fn priorities_list(&self) -> Result<Value, String> {
        let tasks = self
            .authority
            .application_queries()
            .tasks()
            .map_err(debug_err)?;
        let rows = tasks.into_iter().filter(|t| {
            let visible = match t.scope().project_id() { Some(id)=>self.policy.project_allowed(id), None=>self.policy.allow_unscoped_execution };
            visible && matches!(t.status(), TaskStatus::InProgress|TaskStatus::Ready|TaskStatus::Planned|TaskStatus::Blocked)
        }).map(|t|json!({"id":t.id().as_str(),"title":t.title(),"status":t.status().as_str(),"project_id":t.scope().project_id().map(|x|x.as_str()),"deadline_at_ms":t.planning().deadline_at_ms()})).collect::<Vec<_>>();
        Ok(json!(rows))
    }

    fn task_create(&mut self, args: &Value) -> Result<Value, String> {
        self.policy.write_allowed()?;
        let project_id = args
            .get("project_id")
            .and_then(Value::as_str)
            .map(|v| ProjectId::new(v.to_owned()))
            .transpose()
            .map_err(debug_err)?;
        if let Some(p) = &project_id {
            if !self.policy.project_allowed(p) {
                return Err("project outside agent scope".into());
            }
        } else if !self.policy.allow_unscoped_execution {
            return Err("standalone task creation is not allowed".into());
        }
        let milestone_id = args
            .get("milestone_id")
            .and_then(Value::as_str)
            .map(|v| MilestoneId::new(v.to_owned()))
            .transpose()
            .map_err(debug_err)?;
        let scope = TaskScope::try_new(project_id, milestone_id).map_err(debug_err)?;
        let id = TaskId::new(
            args.get("id")
                .and_then(Value::as_str)
                .map(str::to_owned)
                .unwrap_or_else(|| mcp_entity_id(args, "task")),
        )
        .map_err(debug_err)?;
        let task = {
            let revision = self.authority.snapshot().map_err(debug_err)?.0;
            let (_, _, _accepted_id) = self
                .authority
                .create_task_accepted(
                    "orbit-mcp",
                    &mcp_command_id(args),
                    revision,
                    CreateTaskWithActivityCommand {
                        task: CreateTaskCommand {
                            id: id.clone(),
                            title: required_str(args, "title")?,
                            scope,
                        },
                        activity_id: mcp_activity_id(args)?,
                        origin: self.policy.origin(),
                        summary: format!("Agent created task {id}"),
                    },
                )
                .map_err(debug_err)?;
            self.authority
                .application_queries()
                .task(&id)
                .map_err(debug_err)?
        };
        Ok(json!({"id":task.id().as_str(),"title":task.title(),"status":task.status().as_str()}))
    }

    fn task_set_status(&mut self, args: &Value) -> Result<Value, String> {
        self.policy.write_allowed()?;
        let id = TaskId::new(required_str(args, "id")?).map_err(debug_err)?;
        let task = self
            .authority
            .application_queries()
            .task(&id)
            .map_err(debug_err)?;
        self.ensure_task_visible(&task)?;
        let status = parse_task_status(&required_str(args, "status")?)?;
        let task = {
            let revision = self.authority.snapshot().map_err(debug_err)?.0;
            let (_, _, _accepted_id) = self
                .authority
                .transition_task_accepted(
                    "orbit-mcp",
                    &mcp_command_id(args),
                    revision,
                    TransitionTaskWithActivityCommand {
                        task: TransitionTaskCommand {
                            id: id.clone(),
                            status,
                        },
                        activity_id: mcp_activity_id(args)?,
                        origin: self.policy.origin(),
                        summary: format!("Agent changed task {id} to {}", status.as_str()),
                    },
                )
                .map_err(debug_err)?;
            self.authority
                .application_queries()
                .task(&id)
                .map_err(debug_err)?
        };
        Ok(json!({"id":task.id().as_str(),"status":task.status().as_str()}))
    }

    fn project_set_status(&mut self, args: &Value) -> Result<Value, String> {
        self.policy.write_allowed()?;
        let id = ProjectId::new(required_str(args, "id")?).map_err(debug_err)?;
        if !self.policy.project_allowed(&id) {
            return Err("project outside agent scope".into());
        }
        let status = parse_project_status(&required_str(args, "status")?)?;
        let p = {
            let revision = self.authority.snapshot().map_err(debug_err)?.0;
            let (_, _, _accepted_id) = self
                .authority
                .transition_project_accepted(
                    "orbit-mcp",
                    &mcp_command_id(args),
                    revision,
                    TransitionProjectWithActivityCommand {
                        project: TransitionProjectCommand {
                            id: id.clone(),
                            status,
                        },
                        activity_id: mcp_activity_id(args)?,
                        origin: self.policy.origin(),
                        summary: format!("Agent changed project {id} to {}", status.as_str()),
                    },
                )
                .map_err(debug_err)?;
            self.authority
                .application_queries()
                .project(&id)
                .map_err(debug_err)?
        };
        Ok(json!({"id":p.id().as_str(),"status":p.status().as_str()}))
    }

    fn project_configure_strategy(&mut self, args: &Value) -> Result<Value, String> {
        self.policy.write_allowed()?;
        let id = ProjectId::new(required_str(args, "id")?).map_err(debug_err)?;
        if !self.policy.project_allowed(&id) {
            return Err("project outside agent scope".into());
        }
        let mode = ProjectMode::parse(&required_str(args, "mode")?).map_err(debug_err)?;
        let objective = args
            .get("objective")
            .and_then(Value::as_str)
            .map(str::to_owned);
        let progress_signals = args
            .get("progress_signals")
            .and_then(Value::as_array)
            .ok_or_else(|| "missing progress_signals array".to_owned())?
            .iter()
            .map(|v| {
                v.as_str()
                    .ok_or_else(|| "invalid progress signal".to_owned())
                    .and_then(|v| ProjectProgressSignal::parse(v).map_err(debug_err))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let criteria = match args.get("success_criteria").and_then(Value::as_array) {
            Some(values) => values
                .iter()
                .map(|value| {
                    let key = required_str(value, "key")?;
                    let description = required_str(value, "description")?;
                    let required = value
                        .get("required")
                        .and_then(Value::as_bool)
                        .unwrap_or(true);
                    ProjectSuccessCriterion::new(key, description, required).map_err(debug_err)
                })
                .collect::<Result<Vec<_>, _>>()?,
            None => Vec::new(),
        };
        let project = {
            let revision = self.authority.snapshot().map_err(debug_err)?.0;
            let (_, _, _accepted_id) = self
                .authority
                .configure_project_strategy_accepted(
                    "orbit-mcp",
                    &mcp_command_id(args),
                    revision,
                    ConfigureProjectStrategyWithActivityCommand {
                        project: ConfigureProjectStrategyCommand {
                            id: id.clone(),
                            mode,
                            objective,
                            progress_signals,
                            success_criteria: criteria,
                        },
                        activity_id: mcp_activity_id(args)?,
                        origin: self.policy.origin(),
                        summary: format!(
                            "Agent configured project {id} strategy as {}",
                            mode.as_str()
                        ),
                    },
                )
                .map_err(debug_err)?;
            self.authority
                .application_queries()
                .project(&id)
                .map_err(debug_err)?
        };
        Ok(
            json!({"id":project.id().as_str(),"mode":project.mode().as_str(),"completion_ready":project.required_completion_ready()}),
        )
    }

    fn project_set_criterion_status(&mut self, args: &Value) -> Result<Value, String> {
        self.policy.write_allowed()?;
        let project_id = ProjectId::new(required_str(args, "project_id")?).map_err(debug_err)?;
        if !self.policy.project_allowed(&project_id) {
            return Err("project outside agent scope".into());
        }
        let key = required_str(args, "criterion_key")?;
        let status =
            SuccessCriterionStatus::parse(&required_str(args, "status")?).map_err(debug_err)?;
        let project = {
            let revision = self.authority.snapshot().map_err(debug_err)?.0;
            let (_, _, _accepted_id) = self
                .authority
                .set_project_criterion_status_accepted(
                    "orbit-mcp",
                    &mcp_command_id(args),
                    revision,
                    SetProjectCriterionStatusWithActivityCommand {
                        project_id: project_id.clone(),
                        criterion_key: key.clone(),
                        status,
                        activity_id: mcp_activity_id(args)?,
                        origin: self.policy.origin(),
                        summary: format!(
                            "Agent changed project {project_id} criterion {key} to {}",
                            status.as_str()
                        ),
                    },
                )
                .map_err(debug_err)?;
            self.authority
                .application_queries()
                .project(&project_id)
                .map_err(debug_err)?
        };
        Ok(
            json!({"project_id":project.id().as_str(),"criterion_key":key,"status":status.as_str(),"completion_ready":project.required_completion_ready()}),
        )
    }

    fn milestone_set_status(&mut self, args: &Value) -> Result<Value, String> {
        self.policy.write_allowed()?;
        let id = MilestoneId::new(required_str(args, "id")?).map_err(debug_err)?;
        let m = self
            .authority
            .application_queries()
            .milestone(&id)
            .map_err(debug_err)?;
        if !self.policy.project_allowed(m.project_id()) {
            return Err("milestone project outside agent scope".into());
        }
        let status = parse_milestone_status(&required_str(args, "status")?)?;
        let m = {
            let revision = self.authority.snapshot().map_err(debug_err)?.0;
            let (_, _, _accepted_id) = self
                .authority
                .transition_milestone_accepted(
                    "orbit-mcp",
                    &mcp_command_id(args),
                    revision,
                    TransitionMilestoneWithActivityCommand {
                        milestone: TransitionMilestoneCommand {
                            id: id.clone(),
                            status,
                        },
                        activity_id: mcp_activity_id(args)?,
                        origin: self.policy.origin(),
                        summary: format!("Agent changed milestone {id} to {}", status.as_str()),
                    },
                )
                .map_err(debug_err)?;
            self.authority
                .application_queries()
                .milestone(&id)
                .map_err(debug_err)?
        };
        Ok(json!({"id":m.id().as_str(),"status":m.status().as_str()}))
    }

    fn habit_set_active(&mut self, args: &Value) -> Result<Value, String> {
        self.policy.write_allowed()?;
        if !self.policy.allow_unscoped_execution {
            return Err("habit access not allowed".into());
        }
        let id = HabitId::new(required_str(args, "id")?).map_err(debug_err)?;
        let active = required_bool(args, "active")?;
        let h = {
            let revision = self.authority.snapshot().map_err(debug_err)?.0;
            let (_, _, _accepted_id) = self
                .authority
                .set_habit_active_accepted(
                    "orbit-mcp",
                    &mcp_command_id(args),
                    revision,
                    SetHabitActiveWithActivityCommand {
                        id: id.clone(),
                        active,
                        activity_id: mcp_activity_id(args)?,
                        origin: self.policy.origin(),
                        summary: format!("Agent set habit {id} active={active}"),
                    },
                )
                .map_err(debug_err)?;
            self.authority
                .application_queries()
                .habit(&id)
                .map_err(debug_err)?
        };
        Ok(json!({"id":h.id().as_str(),"active":h.is_active()}))
    }

    pub(super) fn ensure_task_visible(&self, t: &orbit_domain::Task) -> Result<(), String> {
        match t.scope().project_id() {
            Some(p) if self.policy.project_allowed(p) => Ok(()),
            None if self.policy.allow_unscoped_execution => Ok(()),
            _ => Err("task outside agent scope".into()),
        }
    }
}
