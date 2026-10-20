//! Task use cases through the authoritative host and accepted commands.
use super::*;

pub(super) fn run_task_dependency(
    host: &mut orbit_runtime::AuthoritativeHost,
    args: TaskDependencyArgs,
    add: bool,
) -> Result<Response, CliError> {
    let revision = host.snapshot().map_err(CliError::repository)?.0;
    let action = if add {
        orbit_runtime::domain_commands::TaskAction::AddPrerequisite {
            task_id: args.id,
            prerequisite_id: args.prerequisite,
        }
    } else {
        orbit_runtime::domain_commands::TaskAction::RemovePrerequisite {
            task_id: args.id,
            prerequisite_id: args.prerequisite,
        }
    };
    let result = host
        .task_command(
            "orbit-cli",
            orbit_runtime::domain_commands::TaskCommand {
                command_id: generate_id("command"),
                expected_revision: revision,
                action,
            },
        )
        .map_err(CliError::repository)?;
    Ok(Response::Authority {
        result: serde_json::to_value(result).map_err(|e| CliError::Serialization(e.to_string()))?,
    })
}

pub(super) fn run_task(
    host: &mut orbit_runtime::AuthoritativeHost,
    command: TaskSubcommand,
) -> Result<Response, CliError> {
    match command {
        TaskSubcommand::DependsOn(args) => run_task_dependency(host, args, true),
        TaskSubcommand::Unblock(args) => run_task_dependency(host, args, false),
        TaskSubcommand::ReviewSchedule(args) => {
            let raw = fs::read_to_string(&args.input)
                .map_err(|err| CliError::Io(format!("cannot read schedule request: {err}")))?;
            let request: orbit_runtime::planning_approval::ScheduleApprovalRequest =
                serde_json::from_str(&raw).map_err(|err| {
                    CliError::Serialization(format!("invalid schedule request: {err}"))
                })?;
            let review = host
                .review_schedule_request("orbit-cli", &request)
                .map_err(CliError::repository)?;
            Ok(Response::Authority {
                result: serde_json::to_value(review)
                    .map_err(|err| CliError::Serialization(err.to_string()))?,
            })
        }
        TaskSubcommand::ApplyReviewedSchedule(args) => {
            if !args.confirm {
                return Err(CliError::Usage(
                    "explicit --confirm required to create approved calendar blocks".into(),
                ));
            }
            let raw = fs::read_to_string(&args.input)
                .map_err(|err| CliError::Io(format!("cannot read approved schedule: {err}")))?;
            let request: orbit_runtime::planning_approval::ApprovedScheduleRequest =
                serde_json::from_str(&raw).map_err(|err| {
                    CliError::Serialization(format!("invalid approved schedule: {err}"))
                })?;
            let receipt = host
                .execute_schedule_request("orbit-cli", request)
                .map_err(CliError::repository)?;
            Ok(Response::Authority {
                result: serde_json::to_value(receipt)
                    .map_err(|err| CliError::Serialization(err.to_string()))?,
            })
        }
        TaskSubcommand::Workload(args) => {
            let report = host
                .workload_insights(args.now_ms, args.horizon_ms)
                .map_err(CliError::repository)?;
            Ok(Response::Authority {
                result: serde_json::to_value(report)
                    .map_err(|e| CliError::Serialization(e.to_string()))?,
            })
        }
        TaskSubcommand::SchedulePlan(args) => {
            let request = orbit_runtime::schedule_planning::SchedulePlanRequest {
                starts_at_ms: args.starts_at_ms,
                ends_at_ms: args.ends_at_ms,
                max_tasks: args.max_tasks,
                project_id: args.project,
                default_minutes: args.default_minutes,
                include_unsized: false,
            };
            let report = host.schedule_plan(&request).map_err(CliError::repository)?;
            Ok(Response::Authority {
                result: serde_json::to_value(report)
                    .map_err(|e| CliError::Serialization(e.to_string()))?,
            })
        }
        TaskSubcommand::DayPlan(args) => {
            let request = orbit_runtime::daily_planning::DayPlanRequest {
                now_ms: args.now_ms,
                available_minutes: args.available_minutes,
                max_tasks: args.max_tasks,
                project_id: args.project,
                include_unsized: args.include_unsized,
                default_minutes: args.default_minutes,
            };
            let report = host.day_plan(&request).map_err(CliError::repository)?;
            Ok(Response::Authority {
                result: serde_json::to_value(report)
                    .map_err(|e| CliError::Serialization(e.to_string()))?,
            })
        }
        TaskSubcommand::PreviewDependency(args) => {
            let (revision, tasks) = host.tasks().map_err(CliError::repository)?;
            let edges = host.task_dependency_edges().map_err(CliError::repository)?;
            let request = orbit_runtime::task_dependency_preview::DependencyPreviewRequest {
                task_id: args.id,
                prerequisite_id: args.prerequisite,
                edit: if args.remove {
                    orbit_runtime::task_dependency_preview::ProposedDependencyEdit::Remove
                } else {
                    orbit_runtime::task_dependency_preview::ProposedDependencyEdit::Add
                },
                now_ms: args.now_ms,
            };
            let preview = orbit_runtime::task_dependency_preview::preview_dependency(
                revision, &tasks, &edges, &request,
            )
            .map_err(|message| CliError::Serialization(message.to_owned()))?;
            Ok(Response::Authority {
                result: serde_json::to_value(preview)
                    .map_err(|error| CliError::Serialization(error.to_string()))?,
            })
        }
        TaskSubcommand::CompletionImpact(args) => {
            let request = orbit_runtime::task_impact::CompletionImpactRequest {
                task_id: args.id,
                now_ms: args.now_ms,
                next_limit: args.limit,
            };
            let result = host
                .preview_task_completion(&request)
                .map_err(CliError::repository)?;
            Ok(Response::Authority {
                result: serde_json::to_value(result)
                    .map_err(|e| CliError::Serialization(e.to_string()))?,
            })
        }
        TaskSubcommand::PlanningGroup(args) => {
            let brief = host
                .work_brief(&orbit_runtime::planning_brief::WorkBriefRequest {
                    now_ms: args.now_ms,
                    ends_at_ms: args.ends_at_ms,
                    limit: args.limit,
                    project_id: Some(args.project.clone()),
                    default_minutes: None,
                })
                .map_err(CliError::repository)?;
            let result = host
                .planning_group(
                    &brief,
                    &orbit_runtime::planning_focus::GroupFocusRequest {
                        project_id: args.project,
                        milestone_id: args.milestone,
                        max_tasks: args.max_tasks,
                    },
                )
                .map_err(CliError::repository)?;
            Ok(Response::Authority {
                result: serde_json::to_value(result)
                    .map_err(|e| CliError::Serialization(e.to_string()))?,
            })
        }
        TaskSubcommand::HandoffPlanningAction(args) => {
            let raw = fs::read_to_string(&args.selection).map_err(|error| {
                CliError::Io(format!("cannot read planning selection: {error}"))
            })?;
            let selection: orbit_runtime::planning_actions::ActionSelection =
                serde_json::from_str(&raw).map_err(|error| {
                    CliError::Serialization(format!("invalid planning selection: {error}"))
                })?;
            let brief = host
                .work_brief(&orbit_runtime::planning_brief::WorkBriefRequest {
                    now_ms: args.now_ms,
                    ends_at_ms: args.ends_at_ms,
                    limit: args.limit,
                    project_id: args.project,
                    default_minutes: None,
                })
                .map_err(CliError::repository)?;
            let result = orbit_runtime::planning_handoff::dispatch_task(
                &brief,
                &orbit_runtime::planning_actions::TaskActionRequest {
                    task_id: args.id,
                    authority_available: true,
                },
                &selection,
            )
            .map_err(|message| CliError::Io(message.to_owned()))?;
            Ok(Response::Authority {
                result: serde_json::to_value(result)
                    .map_err(|error| CliError::Serialization(error.to_string()))?,
            })
        }
        TaskSubcommand::ResolvePlanningAction(args) => {
            let raw = fs::read_to_string(&args.selection).map_err(|error| {
                CliError::Io(format!("cannot read planning selection: {error}"))
            })?;
            let selection: orbit_runtime::planning_actions::ActionSelection =
                serde_json::from_str(&raw).map_err(|error| {
                    CliError::Serialization(format!("invalid planning selection: {error}"))
                })?;
            let brief = host
                .work_brief(&orbit_runtime::planning_brief::WorkBriefRequest {
                    now_ms: args.now_ms,
                    ends_at_ms: args.ends_at_ms,
                    limit: args.limit,
                    project_id: args.project,
                    default_minutes: None,
                })
                .map_err(CliError::repository)?;
            let result = host
                .resolve_planning_task_action(
                    &brief,
                    &orbit_runtime::planning_actions::TaskActionRequest {
                        task_id: args.id,
                        authority_available: true,
                    },
                    &selection,
                )
                .map_err(CliError::repository)?;
            Ok(Response::Authority {
                result: serde_json::to_value(result)
                    .map_err(|error| CliError::Serialization(error.to_string()))?,
            })
        }
        TaskSubcommand::HandoffPlanningGroupAction(args) => {
            let raw = fs::read_to_string(&args.selection).map_err(|error| {
                CliError::Io(format!("cannot read group action selection: {error}"))
            })?;
            let selection: orbit_runtime::planning_actions::ActionSelection =
                serde_json::from_str(&raw).map_err(|error| {
                    CliError::Serialization(format!("invalid group action selection: {error}"))
                })?;
            let brief = host
                .work_brief(&orbit_runtime::planning_brief::WorkBriefRequest {
                    now_ms: args.now_ms,
                    ends_at_ms: args.ends_at_ms,
                    limit: args.limit,
                    project_id: Some(args.project.clone()),
                    default_minutes: None,
                })
                .map_err(CliError::repository)?;
            let result = orbit_runtime::planning_handoff::dispatch_group(
                &brief,
                &orbit_runtime::planning_actions::GroupActionRequest {
                    project_id: args.project,
                    milestone_id: args.milestone,
                    authority_available: true,
                },
                &selection,
            )
            .map_err(|message| CliError::Io(message.to_owned()))?;
            Ok(Response::Authority {
                result: serde_json::to_value(result)
                    .map_err(|error| CliError::Serialization(error.to_string()))?,
            })
        }
        TaskSubcommand::ResolvePlanningGroupAction(args) => {
            let raw = fs::read_to_string(&args.selection).map_err(|error| {
                CliError::Io(format!("cannot read group action selection: {error}"))
            })?;
            let selection: orbit_runtime::planning_actions::ActionSelection =
                serde_json::from_str(&raw).map_err(|error| {
                    CliError::Serialization(format!("invalid group action selection: {error}"))
                })?;
            let brief = host
                .work_brief(&orbit_runtime::planning_brief::WorkBriefRequest {
                    now_ms: args.now_ms,
                    ends_at_ms: args.ends_at_ms,
                    limit: args.limit,
                    project_id: Some(args.project.clone()),
                    default_minutes: None,
                })
                .map_err(CliError::repository)?;
            let result = host
                .resolve_planning_group_action(
                    &brief,
                    &orbit_runtime::planning_actions::GroupActionRequest {
                        project_id: args.project,
                        milestone_id: args.milestone,
                        authority_available: true,
                    },
                    &selection,
                )
                .map_err(CliError::repository)?;
            Ok(Response::Authority {
                result: serde_json::to_value(result)
                    .map_err(|error| CliError::Serialization(error.to_string()))?,
            })
        }
        TaskSubcommand::PlanningActions(args) => {
            let brief = host
                .work_brief(&orbit_runtime::planning_brief::WorkBriefRequest {
                    now_ms: args.now_ms,
                    ends_at_ms: args.ends_at_ms,
                    limit: args.limit,
                    project_id: args.project,
                    default_minutes: None,
                })
                .map_err(CliError::repository)?;
            let menu = host
                .planning_task_actions(
                    &brief,
                    &orbit_runtime::planning_actions::TaskActionRequest {
                        task_id: args.id,
                        authority_available: true,
                    },
                )
                .map_err(CliError::repository)?;
            Ok(Response::Authority {
                result: serde_json::to_value(menu)
                    .map_err(|e| CliError::Serialization(e.to_string()))?,
            })
        }
        TaskSubcommand::PlanningGroupActions(args) => {
            let brief = host
                .work_brief(&orbit_runtime::planning_brief::WorkBriefRequest {
                    now_ms: args.now_ms,
                    ends_at_ms: args.ends_at_ms,
                    limit: args.limit,
                    project_id: Some(args.project.clone()),
                    default_minutes: None,
                })
                .map_err(CliError::repository)?;
            let menu = host
                .planning_group_actions(
                    &brief,
                    &orbit_runtime::planning_actions::GroupActionRequest {
                        project_id: args.project,
                        milestone_id: args.milestone,
                        authority_available: true,
                    },
                )
                .map_err(CliError::repository)?;
            Ok(Response::Authority {
                result: serde_json::to_value(menu)
                    .map_err(|e| CliError::Serialization(e.to_string()))?,
            })
        }
        TaskSubcommand::PlanningFocus(args) => {
            let brief = host
                .work_brief(&orbit_runtime::planning_brief::WorkBriefRequest {
                    now_ms: args.now_ms,
                    ends_at_ms: args.ends_at_ms,
                    limit: args.limit,
                    project_id: args.project,
                    default_minutes: None,
                })
                .map_err(CliError::repository)?;
            let result = host
                .planning_focus(
                    &brief,
                    &orbit_runtime::planning_focus::FocusRequest {
                        task_id: args.id,
                        max_neighbors: args.max_neighbors,
                    },
                )
                .map_err(CliError::repository)?;
            Ok(Response::Authority {
                result: serde_json::to_value(result)
                    .map_err(|e| CliError::Serialization(e.to_string()))?,
            })
        }
        TaskSubcommand::WorkBrief(args) => {
            let request = orbit_runtime::planning_brief::WorkBriefRequest {
                now_ms: args.now_ms,
                ends_at_ms: args.ends_at_ms,
                limit: args.limit,
                project_id: args.project,
                default_minutes: args.default_minutes,
            };
            let result = host.work_brief(&request).map_err(CliError::repository)?;
            Ok(Response::Authority {
                result: serde_json::to_value(result)
                    .map_err(|e| CliError::Serialization(e.to_string()))?,
            })
        }
        TaskSubcommand::Next(args) => {
            let report = host
                .next_actions(args.now_ms, args.limit)
                .map_err(CliError::repository)?;
            Ok(Response::Authority {
                result: serde_json::to_value(report)
                    .map_err(|e| CliError::Serialization(e.to_string()))?,
            })
        }
        TaskSubcommand::ReviewScenarioOptions(args) => {
            let source = std::fs::read_to_string(&args.input).map_err(|e| {
                CliError::Serialization(format!("scenario options unavailable: {e}"))
            })?;
            let request: orbit_runtime::scenario_execution::ScenarioExecutionRequest =
                serde_json::from_str(&source).map_err(|e| {
                    CliError::Serialization(format!("invalid scenario options JSON: {e}"))
                })?;
            let report = host
                .review_scenario_options("orbit-cli", &request)
                .map_err(CliError::repository)?;
            Ok(Response::Authority {
                result: serde_json::to_value(report)
                    .map_err(|e| CliError::Serialization(e.to_string()))?,
            })
        }
        TaskSubcommand::ReviewScenario(args) => {
            let source = std::fs::read_to_string(&args.input).map_err(|e| {
                CliError::Serialization(format!("scenario review file unavailable: {e}"))
            })?;
            let request: orbit_runtime::scenario_execution::ScenarioExecutionRequest =
                serde_json::from_str(&source).map_err(|e| {
                    CliError::Serialization(format!("invalid scenario execution JSON: {e}"))
                })?;
            let result = host
                .review_scenario_execution("orbit-cli", &request)
                .map_err(CliError::repository)?;
            Ok(Response::Authority {
                result: serde_json::to_value(result)
                    .map_err(|e| CliError::Serialization(e.to_string()))?,
            })
        }
        TaskSubcommand::CompareScenarios(args) => {
            let source = std::fs::read_to_string(&args.input)
                .map_err(|e| CliError::Serialization(format!("scenario file unavailable: {e}")))?;
            let request: orbit_runtime::planning_scenarios::ScenarioCompareRequest =
                serde_json::from_str(&source)
                    .map_err(|e| CliError::Serialization(format!("invalid scenario JSON: {e}")))?;
            let result = host
                .compare_planning_scenarios(&request)
                .map_err(CliError::repository)?;
            Ok(Response::Authority {
                result: serde_json::to_value(result)
                    .map_err(|e| CliError::Serialization(e.to_string()))?,
            })
        }
        TaskSubcommand::PortfolioCapacity(args) => {
            let report = host
                .portfolio_capacity_plan(
                    &orbit_runtime::portfolio_capacity::PortfolioCapacityRequest {
                        now_ms: args.now_ms,
                        available_minutes: args.available_minutes,
                        max_tasks: args.max_tasks,
                        project_id: args.project,
                        include_unassigned: args.include_unassigned,
                        include_unsized: args.include_unsized,
                        default_minutes: args.default_minutes,
                    },
                )
                .map_err(CliError::repository)?;
            Ok(Response::Authority {
                result: serde_json::to_value(report)
                    .map_err(|e| CliError::Serialization(e.to_string()))?,
            })
        }
        TaskSubcommand::PortfolioActions(args) => {
            let report = host
                .portfolio_actions(&orbit_runtime::portfolio_actions::PortfolioActionRequest {
                    now_ms: args.now_ms,
                    project_id: args.project,
                    limit: args.limit,
                    include_unassigned: args.include_unassigned,
                })
                .map_err(CliError::repository)?;
            Ok(Response::Authority {
                result: serde_json::to_value(report)
                    .map_err(|e| CliError::Serialization(e.to_string()))?,
            })
        }
        TaskSubcommand::ProjectDependencies(args) => {
            let report = host
                .project_dependency_report(
                    &orbit_runtime::project_dependency_planning::ProjectDependencyRequest {
                        now_ms: args.now_ms,
                        project_id: args.project,
                        include_unassigned: args.include_unassigned,
                    },
                )
                .map_err(CliError::repository)?;
            Ok(Response::Authority {
                result: serde_json::to_value(report)
                    .map_err(|e| CliError::Serialization(e.to_string()))?,
            })
        }
        TaskSubcommand::ProjectHandoffs(args) => {
            let report = host
                .project_handoffs(&orbit_runtime::project_handoffs::HandoffRequest {
                    now_ms: args.now_ms,
                    project_id: args.project,
                    include_completed: args.include_completed,
                })
                .map_err(CliError::repository)?;
            Ok(Response::Authority {
                result: serde_json::to_value(report)
                    .map_err(|e| CliError::Serialization(e.to_string()))?,
            })
        }
        TaskSubcommand::Graph(args) => {
            let result = host.task_graph(args.now_ms).map_err(CliError::repository)?;
            Ok(Response::Authority {
                result: serde_json::to_value(result)
                    .map_err(|e| CliError::Serialization(e.to_string()))?,
            })
        }
        TaskSubcommand::Ready(args) => {
            let mut result = host
                .task_readiness(args.now_ms)
                .map_err(CliError::repository)?;
            if !args.all {
                result.tasks.retain(|task| task.actionable);
            }
            Ok(Response::Authority {
                result: serde_json::to_value(result)
                    .map_err(|e| CliError::Serialization(e.to_string()))?,
            })
        }
        TaskSubcommand::Add(args) => {
            let scope = task_scope(args.project, args.milestone)?;
            let task_id = TaskId::new(args.id.unwrap_or_else(|| generate_id("task")))
                .map_err(CliError::domain)?;
            let expected_revision = host.snapshot().map_err(CliError::repository)?.0;
            let (_, _, task) = host
                .create_task_accepted(
                    "orbit-cli",
                    &generate_id("command"),
                    expected_revision,
                    CreateTaskWithActivityCommand {
                        task: CreateTaskCommand {
                            id: task_id.clone(),
                            title: args.title,
                            scope,
                        },
                        activity_id: ActivityId::new(generate_id("activity"))
                            .map_err(CliError::domain)?,
                        origin: ActivityOrigin::Human,
                        summary: format!("Task {} created", task_id),
                    },
                )
                .map_err(CliError::application)?;
            Ok(Response::Task {
                task: TaskView::from(&task),
                action: "created",
            })
        }
        TaskSubcommand::List(args) => {
            let mut tasks = host
                .application_queries()
                .tasks()
                .map_err(CliError::application)?;
            if let Some(status) = args.status {
                let status: TaskStatus = status.into();
                tasks.retain(|task| task.status() == status);
            }
            if let Some(project) = args.project {
                let project = ProjectId::new(project).map_err(CliError::domain)?;
                tasks.retain(|task| task.scope().project_id() == Some(&project));
            }
            if let Some(query) = normalized_filter(args.query)? {
                tasks.retain(|task| text_matches(&query, task.id().as_str(), task.title()));
            }
            tasks.sort_by(|left, right| left.id().as_str().cmp(right.id().as_str()));
            Ok(Response::Tasks {
                tasks: tasks.iter().map(TaskView::from).collect(),
            })
        }
        TaskSubcommand::Start(args) => start_task_cli(host, args.id),
        TaskSubcommand::Complete(args) => {
            transition_task_cli(host, args.id, TaskStatus::Done, "completed")
        }
        TaskSubcommand::Status(args) => {
            let status: TaskStatus = args.status.into();
            transition_task_cli(host, args.id, status, "status changed")
        }
        TaskSubcommand::Planning(args) => {
            let id = TaskId::new(args.id).map_err(CliError::domain)?;
            let planning = TaskPlanning::new(
                args.estimate_minutes,
                args.earliest_start_ms,
                args.deadline_ms,
            )
            .map_err(CliError::domain)?;
            let task = {
                let revision = host.snapshot().map_err(CliError::repository)?.0;
                let (_, _, _accepted_id) = host
                    .set_task_planning_accepted(
                        "orbit-cli",
                        &generate_id("command"),
                        revision,
                        SetTaskPlanningWithActivityCommand {
                            task: SetTaskPlanningCommand {
                                id: id.clone(),
                                planning,
                            },
                            activity_id: ActivityId::new(generate_id("activity"))
                                .map_err(CliError::domain)?,
                            origin: ActivityOrigin::Human,
                            summary: format!("Task {} planning metadata updated", id),
                        },
                    )
                    .map_err(CliError::application)?;
                host.application_queries()
                    .task(&id)
                    .map_err(CliError::application)?
            };
            Ok(Response::Task {
                task: TaskView::from(&task),
                action: "planning updated",
            })
        }
    }
}

fn start_task_cli(
    host: &mut orbit_runtime::AuthoritativeHost,
    raw_id: String,
) -> Result<Response, CliError> {
    let id = TaskId::new(raw_id).map_err(CliError::domain)?;
    let expected_revision = host.snapshot().map_err(CliError::repository)?.0;
    let (_, _, task) = host
        .start_task_accepted(
            "orbit-cli",
            &generate_id("command"),
            expected_revision,
            StartTaskWithActivityCommand {
                id: id.clone(),
                activity_id: ActivityId::new(generate_id("activity")).map_err(CliError::domain)?,
                origin: ActivityOrigin::Human,
                summary: format!("Task {} started", id),
            },
        )
        .map_err(CliError::application)?;
    Ok(Response::Task {
        task: TaskView::from(&task),
        action: "started",
    })
}

fn transition_task_cli(
    host: &mut orbit_runtime::AuthoritativeHost,
    raw_id: String,
    status: TaskStatus,
    action: &'static str,
) -> Result<Response, CliError> {
    let id = TaskId::new(raw_id).map_err(CliError::domain)?;
    let expected_revision = host.snapshot().map_err(CliError::repository)?.0;
    let (_, _, task) = host
        .transition_task_accepted(
            "orbit-cli",
            &generate_id("command"),
            expected_revision,
            TransitionTaskWithActivityCommand {
                task: TransitionTaskCommand {
                    id: id.clone(),
                    status,
                },
                activity_id: ActivityId::new(generate_id("activity")).map_err(CliError::domain)?,
                origin: ActivityOrigin::Human,
                summary: format!("Task {} {}", id, action),
            },
        )
        .map_err(CliError::application)?;
    Ok(Response::Task {
        task: TaskView::from(&task),
        action,
    })
}
