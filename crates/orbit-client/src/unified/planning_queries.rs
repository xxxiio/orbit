//! Read-only planning and dependency projections shared by local and connected clients.
//! Connected reads use server-accepted state, never speculative offline changes.
use super::*;

impl AuthorityConnection {
    /// Read-only workload diagnostics; remote mode uses accepted state only.
    pub fn workload_insights(
        &self,
        now_ms: i64,
        horizon_ms: i64,
    ) -> Result<orbit_runtime::workload_insights::WorkloadInsights, ClientConnectionError> {
        match self {
            Self::Embedded(host) => Ok(host.workload_insights(now_ms, horizon_ms)?),
            Self::Remote { .. } => {
                let readiness = self.task_readiness(now_ms)?;
                let snapshot =
                    self.accepted_snapshot()?
                        .ok_or(ClientConnectionError::Unsupported(
                            "workload insights require accepted snapshot",
                        ))?;
                let tasks: Vec<orbit_runtime::domain_commands::TaskRecord> =
                    serde_json::from_value(
                        snapshot
                            .get("tasks")
                            .cloned()
                            .ok_or(ClientConnectionError::Unsupported("missing tasks"))?,
                    )?;
                orbit_runtime::workload_insights::analyze_workload(
                    readiness.revision,
                    &tasks,
                    &readiness.edges,
                    now_ms,
                    horizon_ms,
                )
                .map_err(ClientConnectionError::Unsupported)
            }
        }
    }

    /// Plan the day from accepted state only; queued remote commands cannot
    /// change priority or unblock prerequisites until authority acceptance.
    pub fn day_plan(
        &self,
        request: &orbit_runtime::daily_planning::DayPlanRequest,
    ) -> Result<orbit_runtime::daily_planning::DayPlan, ClientConnectionError> {
        match self {
            Self::Embedded(host) => Ok(host.day_plan(request)?),
            Self::Remote { .. } => {
                let readiness = self.task_readiness(request.now_ms)?;
                let snapshot =
                    self.accepted_snapshot()?
                        .ok_or(ClientConnectionError::Unsupported(
                            "day plan requires accepted snapshot",
                        ))?;
                let tasks: Vec<orbit_runtime::domain_commands::TaskRecord> =
                    serde_json::from_value(
                        snapshot
                            .get("tasks")
                            .cloned()
                            .ok_or(ClientConnectionError::Unsupported("missing tasks"))?,
                    )?;
                orbit_runtime::daily_planning::plan_day(
                    readiness.revision,
                    &tasks,
                    &readiness.edges,
                    request,
                )
                .map_err(ClientConnectionError::Unsupported)
            }
        }
    }

    /// Propose time slots using only accepted tasks, dependencies and schedule.
    /// Pending connected-mode edits never reduce apparent busy time or unblock work.
    pub fn schedule_plan(
        &self,
        request: &orbit_runtime::schedule_planning::SchedulePlanRequest,
    ) -> Result<orbit_runtime::schedule_planning::SchedulePlan, ClientConnectionError> {
        match self {
            Self::Embedded(host) => Ok(host.schedule_plan(request)?),
            Self::Remote { .. } => {
                let snapshot =
                    self.accepted_snapshot()?
                        .ok_or(ClientConnectionError::Unsupported(
                            "schedule planning requires accepted snapshot",
                        ))?;
                let revision = snapshot.get("revision").and_then(Value::as_i64).ok_or(
                    ClientConnectionError::Unsupported("accepted revision missing"),
                )?;
                let tasks: Vec<orbit_runtime::domain_commands::TaskRecord> =
                    serde_json::from_value(
                        snapshot
                            .get("tasks")
                            .cloned()
                            .ok_or(ClientConnectionError::Unsupported("accepted tasks missing"))?,
                    )?;
                let edges: Vec<orbit_runtime::task_dependencies::TaskDependencyEdge> =
                    serde_json::from_value(snapshot.get("task_dependencies").cloned().ok_or(
                        ClientConnectionError::Unsupported("accepted dependencies missing"),
                    )?)?;
                let blocks: Vec<orbit_runtime::authority_schedule::ScheduleRecord> =
                    serde_json::from_value(snapshot.get("schedule").cloned().ok_or(
                        ClientConnectionError::Unsupported("accepted schedule missing"),
                    )?)?;
                orbit_runtime::schedule_planning::plan_schedule(
                    revision, revision, &tasks, &edges, &blocks, request,
                )
                .map_err(ClientConnectionError::Unsupported)
            }
        }
    }

    /// Accepted-only planning overview. Connected mode never applies pending
    /// outbox intentions to readiness, deadline, or calendar calculations.
    /// Simulate completion against last accepted tasks, never pending intentions.
    pub fn preview_task_completion(
        &self,
        request: &orbit_runtime::task_impact::CompletionImpactRequest,
    ) -> Result<orbit_runtime::task_impact::CompletionImpact, ClientConnectionError> {
        match self {
            Self::Embedded(host) => Ok(host.preview_task_completion(request)?),
            Self::Remote { .. } => {
                let readiness = self.task_readiness(request.now_ms)?;
                let snapshot =
                    self.accepted_snapshot()?
                        .ok_or(ClientConnectionError::Unsupported(
                            "completion preview requires accepted snapshot",
                        ))?;
                let tasks: Vec<orbit_runtime::domain_commands::TaskRecord> =
                    serde_json::from_value(
                        snapshot
                            .get("tasks")
                            .cloned()
                            .ok_or(ClientConnectionError::Unsupported("accepted tasks missing"))?,
                    )?;
                orbit_runtime::task_impact::preview_completion(
                    readiness.revision,
                    &tasks,
                    &readiness.edges,
                    request,
                )
                .map_err(ClientConnectionError::Unsupported)
            }
        }
    }

    pub fn work_brief(
        &self,
        request: &orbit_runtime::planning_brief::WorkBriefRequest,
    ) -> Result<orbit_runtime::planning_brief::WorkBrief, ClientConnectionError> {
        match self {
            Self::Embedded(host) => Ok(host.work_brief(request)?),
            Self::Remote { .. } => {
                let snapshot =
                    self.accepted_snapshot()?
                        .ok_or(ClientConnectionError::Unsupported(
                            "work brief requires accepted snapshot",
                        ))?;
                let revision = snapshot.get("revision").and_then(Value::as_i64).ok_or(
                    ClientConnectionError::Unsupported("accepted revision missing"),
                )?;
                let tasks: Vec<orbit_runtime::domain_commands::TaskRecord> =
                    serde_json::from_value(
                        snapshot
                            .get("tasks")
                            .cloned()
                            .ok_or(ClientConnectionError::Unsupported("accepted tasks missing"))?,
                    )?;
                let edges: Vec<orbit_runtime::task_dependencies::TaskDependencyEdge> =
                    serde_json::from_value(snapshot.get("task_dependencies").cloned().ok_or(
                        ClientConnectionError::Unsupported("accepted dependencies missing"),
                    )?)?;
                let blocks: Vec<orbit_runtime::authority_schedule::ScheduleRecord> =
                    serde_json::from_value(snapshot.get("schedule").cloned().ok_or(
                        ClientConnectionError::Unsupported("accepted schedule missing"),
                    )?)?;
                orbit_runtime::planning_brief::build_work_brief(
                    revision, &tasks, &edges, &blocks, request,
                )
                .map_err(ClientConnectionError::Unsupported)
            }
        }
    }

    /// Read-only task drilldown, including fully offline accepted snapshots.
    pub fn planning_focus(
        &self,
        brief: &orbit_runtime::planning_brief::WorkBrief,
        request: &orbit_runtime::planning_focus::FocusRequest,
    ) -> Result<orbit_runtime::planning_focus::TaskFocus, ClientConnectionError> {
        match self {
            Self::Embedded(host) => Ok(host.planning_focus(brief, request)?),
            Self::Remote { .. } => {
                let snapshot =
                    self.accepted_snapshot()?
                        .ok_or(ClientConnectionError::Unsupported(
                            "accepted snapshot unavailable",
                        ))?;
                let revision = snapshot.get("revision").and_then(Value::as_i64).ok_or(
                    ClientConnectionError::Unsupported("accepted revision missing"),
                )?;
                if revision != brief.revision {
                    return Err(ClientConnectionError::Unsupported(
                        "planning brief revision is stale",
                    ));
                }
                orbit_runtime::planning_focus::focus_task(brief, request)
                    .map_err(ClientConnectionError::Unsupported)
            }
        }
    }

    /// Offline-capable project/milestone context over the accepted brief.
    pub fn planning_group(
        &self,
        brief: &orbit_runtime::planning_brief::WorkBrief,
        request: &orbit_runtime::planning_focus::GroupFocusRequest,
    ) -> Result<orbit_runtime::planning_focus::GroupFocus, ClientConnectionError> {
        match self {
            Self::Embedded(host) => Ok(host.planning_group(brief, request)?),
            Self::Remote { .. } => {
                let snapshot =
                    self.accepted_snapshot()?
                        .ok_or(ClientConnectionError::Unsupported(
                            "accepted snapshot unavailable",
                        ))?;
                let revision = snapshot.get("revision").and_then(Value::as_i64).ok_or(
                    ClientConnectionError::Unsupported("accepted revision missing"),
                )?;
                if revision != brief.revision {
                    return Err(ClientConnectionError::Unsupported(
                        "planning brief revision is stale",
                    ));
                }
                orbit_runtime::planning_focus::focus_group(brief, request)
                    .map_err(ClientConnectionError::Unsupported)
            }
        }
    }

    /// Action discovery is read-only; connected clients use accepted snapshot state.
    pub fn planning_task_actions(
        &self,
        brief: &orbit_runtime::planning_brief::WorkBrief,
        request: &orbit_runtime::planning_actions::TaskActionRequest,
    ) -> Result<orbit_runtime::planning_actions::PlanningActionMenu, ClientConnectionError> {
        match self {
            Self::Embedded(host) => Ok(host.planning_task_actions(brief, request)?),
            Self::Remote { .. } => {
                let snapshot =
                    self.accepted_snapshot()?
                        .ok_or(ClientConnectionError::Unsupported(
                            "accepted snapshot unavailable",
                        ))?;
                let revision = snapshot.get("revision").and_then(Value::as_i64).ok_or(
                    ClientConnectionError::Unsupported("accepted revision missing"),
                )?;
                if revision != brief.revision {
                    return Err(ClientConnectionError::Unsupported(
                        "planning brief revision is stale",
                    ));
                }
                orbit_runtime::planning_actions::task_actions(brief, request)
                    .map_err(ClientConnectionError::Unsupported)
            }
        }
    }
    pub fn planning_group_actions(
        &self,
        brief: &orbit_runtime::planning_brief::WorkBrief,
        request: &orbit_runtime::planning_actions::GroupActionRequest,
    ) -> Result<orbit_runtime::planning_actions::PlanningActionMenu, ClientConnectionError> {
        match self {
            Self::Embedded(host) => Ok(host.planning_group_actions(brief, request)?),
            Self::Remote { .. } => {
                let snapshot =
                    self.accepted_snapshot()?
                        .ok_or(ClientConnectionError::Unsupported(
                            "accepted snapshot unavailable",
                        ))?;
                let revision = snapshot.get("revision").and_then(Value::as_i64).ok_or(
                    ClientConnectionError::Unsupported("accepted revision missing"),
                )?;
                if revision != brief.revision {
                    return Err(ClientConnectionError::Unsupported(
                        "planning brief revision is stale",
                    ));
                }
                orbit_runtime::planning_actions::group_actions(brief, request)
                    .map_err(ClientConnectionError::Unsupported)
            }
        }
    }

    /// Revalidates an action selection against the currently accepted revision;
    /// no write is performed and no receipt is generated.
    pub fn resolve_planning_task_action(
        &self,
        brief: &orbit_runtime::planning_brief::WorkBrief,
        request: &orbit_runtime::planning_actions::TaskActionRequest,
        selection: &orbit_runtime::planning_actions::ActionSelection,
    ) -> Result<orbit_runtime::planning_actions::ResolvedAction, ClientConnectionError> {
        match self {
            Self::Embedded(host) => {
                Ok(host.resolve_planning_task_action(brief, request, selection)?)
            }
            Self::Remote { .. } => {
                let snapshot =
                    self.accepted_snapshot()?
                        .ok_or(ClientConnectionError::Unsupported(
                            "accepted snapshot unavailable",
                        ))?;
                let revision = snapshot.get("revision").and_then(Value::as_i64).ok_or(
                    ClientConnectionError::Unsupported("accepted revision missing"),
                )?;
                if revision != brief.revision {
                    return Err(ClientConnectionError::Unsupported(
                        "planning brief revision is stale",
                    ));
                }
                orbit_runtime::planning_actions::resolve_task_action(brief, request, selection)
                    .map_err(ClientConnectionError::Unsupported)
            }
        }
    }
    pub fn resolve_planning_group_action(
        &self,
        brief: &orbit_runtime::planning_brief::WorkBrief,
        request: &orbit_runtime::planning_actions::GroupActionRequest,
        selection: &orbit_runtime::planning_actions::ActionSelection,
    ) -> Result<orbit_runtime::planning_actions::ResolvedAction, ClientConnectionError> {
        match self {
            Self::Embedded(host) => {
                Ok(host.resolve_planning_group_action(brief, request, selection)?)
            }
            Self::Remote { .. } => {
                let snapshot =
                    self.accepted_snapshot()?
                        .ok_or(ClientConnectionError::Unsupported(
                            "accepted snapshot unavailable",
                        ))?;
                let revision = snapshot.get("revision").and_then(Value::as_i64).ok_or(
                    ClientConnectionError::Unsupported("accepted revision missing"),
                )?;
                if revision != brief.revision {
                    return Err(ClientConnectionError::Unsupported(
                        "planning brief revision is stale",
                    ));
                }
                orbit_runtime::planning_actions::resolve_group_action(brief, request, selection)
                    .map_err(ClientConnectionError::Unsupported)
            }
        }
    }

    /// Read-only navigation handoff. Remote mode validates against the local
    /// accepted snapshot; pending outbox commands cannot change destinations.
    pub fn planning_task_handoff(
        &self,
        brief: &orbit_runtime::planning_brief::WorkBrief,
        request: &orbit_runtime::planning_actions::TaskActionRequest,
        selection: &orbit_runtime::planning_actions::ActionSelection,
    ) -> Result<orbit_runtime::planning_handoff::PlanningHandoff, ClientConnectionError> {
        match self {
            Self::Embedded(host) => Ok(host.planning_task_handoff(brief, request, selection)?),
            Self::Remote { .. } => {
                let accepted =
                    self.accepted_snapshot()?
                        .ok_or(ClientConnectionError::Unsupported(
                            "accepted snapshot unavailable",
                        ))?;
                let revision = accepted.get("revision").and_then(Value::as_i64).ok_or(
                    ClientConnectionError::Unsupported("accepted revision missing"),
                )?;
                if revision != brief.revision {
                    return Err(ClientConnectionError::Unsupported(
                        "planning handoff revision is stale",
                    ));
                }
                orbit_runtime::planning_handoff::dispatch_task(brief, request, selection)
                    .map_err(ClientConnectionError::Unsupported)
            }
        }
    }
    pub fn planning_group_handoff(
        &self,
        brief: &orbit_runtime::planning_brief::WorkBrief,
        request: &orbit_runtime::planning_actions::GroupActionRequest,
        selection: &orbit_runtime::planning_actions::ActionSelection,
    ) -> Result<orbit_runtime::planning_handoff::PlanningHandoff, ClientConnectionError> {
        match self {
            Self::Embedded(host) => Ok(host.planning_group_handoff(brief, request, selection)?),
            Self::Remote { .. } => {
                let accepted =
                    self.accepted_snapshot()?
                        .ok_or(ClientConnectionError::Unsupported(
                            "accepted snapshot unavailable",
                        ))?;
                let revision = accepted.get("revision").and_then(Value::as_i64).ok_or(
                    ClientConnectionError::Unsupported("accepted revision missing"),
                )?;
                if revision != brief.revision {
                    return Err(ClientConnectionError::Unsupported(
                        "planning handoff revision is stale",
                    ));
                }
                orbit_runtime::planning_handoff::dispatch_group(brief, request, selection)
                    .map_err(ClientConnectionError::Unsupported)
            }
        }
    }

    pub fn next_actions(
        &self,
        now_ms: i64,
        limit: usize,
    ) -> Result<orbit_runtime::task_next::NextActionsReport, ClientConnectionError> {
        match self {
            Self::Embedded(host) => Ok(host.next_actions(now_ms, limit)?),
            Self::Remote { .. } => {
                let readiness = self.task_readiness(now_ms)?;
                let snapshot =
                    self.accepted_snapshot()?
                        .ok_or(ClientConnectionError::Unsupported(
                            "next actions require an accepted snapshot",
                        ))?;
                let tasks: Vec<orbit_runtime::domain_commands::TaskRecord> =
                    serde_json::from_value(
                        snapshot
                            .get("tasks")
                            .cloned()
                            .ok_or(ClientConnectionError::Unsupported("missing tasks"))?,
                    )?;
                orbit_runtime::task_next::next_actions(
                    readiness.revision,
                    &tasks,
                    &readiness.edges,
                    now_ms,
                    limit,
                )
                .map_err(ClientConnectionError::Unsupported)
            }
        }
    }

    /// Graph planning from accepted state. No pending remote command may
    /// retroactively unblock a task before the server accepts that change.
    /// Same accepted-state report in embedded and disconnected connected mode.
    /// The caller must authorize the selected project at its adapter boundary.
    pub fn project_dependency_report(
        &self,
        request: &orbit_runtime::project_dependency_planning::ProjectDependencyRequest,
    ) -> Result<
        orbit_runtime::project_dependency_planning::ProjectDependencyReport,
        ClientConnectionError,
    > {
        match self {
            Self::Embedded(host) => Ok(host.project_dependency_report(request)?),
            Self::Remote { .. } => {
                let readiness = self.task_readiness(request.now_ms)?;
                let snapshot =
                    self.accepted_snapshot()?
                        .ok_or(ClientConnectionError::Unsupported(
                            "project diagnostics require accepted state",
                        ))?;
                let revision = snapshot.get("revision").and_then(Value::as_i64).ok_or(
                    ClientConnectionError::Unsupported("missing accepted revision"),
                )?;
                if revision != readiness.revision {
                    return Err(ClientConnectionError::Unsupported(
                        "accepted revision mismatch",
                    ));
                }
                let tasks: Vec<orbit_runtime::domain_commands::TaskRecord> =
                    serde_json::from_value(
                        snapshot
                            .get("tasks")
                            .cloned()
                            .ok_or(ClientConnectionError::Unsupported("missing tasks"))?,
                    )?;
                orbit_runtime::project_dependency_planning::project_dependency_report(
                    revision,
                    &tasks,
                    &readiness.edges,
                    request,
                )
                .map_err(ClientConnectionError::Unsupported)
            }
        }
    }

    pub fn project_handoffs(
        &self,
        request: &orbit_runtime::project_handoffs::HandoffRequest,
    ) -> Result<orbit_runtime::project_handoffs::ProjectHandoffReport, ClientConnectionError> {
        match self {
            Self::Embedded(host) => Ok(host.project_handoffs(request)?),
            Self::Remote { .. } => {
                let readiness = self.task_readiness(request.now_ms)?;
                let snapshot =
                    self.accepted_snapshot()?
                        .ok_or(ClientConnectionError::Unsupported(
                            "project handoffs require accepted snapshot",
                        ))?;
                let revision = snapshot.get("revision").and_then(Value::as_i64).ok_or(
                    ClientConnectionError::Unsupported("missing accepted revision"),
                )?;
                if revision != readiness.revision {
                    return Err(ClientConnectionError::Unsupported(
                        "accepted revision mismatch",
                    ));
                }
                let tasks: Vec<orbit_runtime::domain_commands::TaskRecord> =
                    serde_json::from_value(
                        snapshot
                            .get("tasks")
                            .cloned()
                            .ok_or(ClientConnectionError::Unsupported("missing tasks"))?,
                    )?;
                orbit_runtime::project_handoffs::analyze_handoffs(
                    revision,
                    &tasks,
                    &readiness.edges,
                    request,
                )
                .map_err(ClientConnectionError::Unsupported)
            }
        }
    }

    /// Capacity plan based only on the last accepted snapshot (or embedded authority).
    pub fn portfolio_capacity_plan(
        &self,
        request: &orbit_runtime::portfolio_capacity::PortfolioCapacityRequest,
    ) -> Result<orbit_runtime::portfolio_capacity::PortfolioCapacityPlan, ClientConnectionError>
    {
        match self {
            Self::Embedded(host) => Ok(host.portfolio_capacity_plan(request)?),
            Self::Remote { .. } => {
                let readiness = self.task_readiness(request.now_ms)?;
                let snapshot =
                    self.accepted_snapshot()?
                        .ok_or(ClientConnectionError::Unsupported(
                            "portfolio capacity planning requires accepted state",
                        ))?;
                let revision = snapshot.get("revision").and_then(Value::as_i64).ok_or(
                    ClientConnectionError::Unsupported("missing accepted revision"),
                )?;
                if revision != readiness.revision {
                    return Err(ClientConnectionError::Unsupported(
                        "accepted revision mismatch",
                    ));
                }
                let tasks: Vec<orbit_runtime::domain_commands::TaskRecord> =
                    serde_json::from_value(
                        snapshot
                            .get("tasks")
                            .cloned()
                            .ok_or(ClientConnectionError::Unsupported("missing tasks"))?,
                    )?;
                orbit_runtime::portfolio_capacity::portfolio_capacity_plan(
                    revision,
                    &tasks,
                    &readiness.edges,
                    request,
                )
                .map_err(ClientConnectionError::Unsupported)
            }
        }
    }

    /// Read-only scenario comparison, pinned to the last accepted revision.
    pub fn compare_planning_scenarios(
        &self,
        request: &orbit_runtime::planning_scenarios::ScenarioCompareRequest,
    ) -> Result<orbit_runtime::planning_scenarios::ScenarioComparison, ClientConnectionError> {
        match self {
            Self::Embedded(host) => Ok(host.compare_planning_scenarios(request)?),
            Self::Remote { .. } => {
                let readiness = self.task_readiness(request.baseline.now_ms)?;
                let snapshot =
                    self.accepted_snapshot()?
                        .ok_or(ClientConnectionError::Unsupported(
                            "scenario comparison requires an accepted snapshot",
                        ))?;
                let revision = snapshot.get("revision").and_then(Value::as_i64).ok_or(
                    ClientConnectionError::Unsupported("missing accepted revision"),
                )?;
                if revision != readiness.revision {
                    return Err(ClientConnectionError::Unsupported(
                        "accepted revision mismatch",
                    ));
                }
                let tasks: Vec<orbit_runtime::domain_commands::TaskRecord> =
                    serde_json::from_value(
                        snapshot
                            .get("tasks")
                            .cloned()
                            .ok_or(ClientConnectionError::Unsupported("missing tasks"))?,
                    )?;
                orbit_runtime::planning_scenarios::compare_scenarios(
                    revision,
                    &tasks,
                    &readiness.edges,
                    request,
                )
                .map_err(ClientConnectionError::Unsupported)
            }
        }
    }

    /// Offline-safe, accepted-state-only portfolio recommendations.
    pub fn portfolio_actions(
        &self,
        request: &orbit_runtime::portfolio_actions::PortfolioActionRequest,
    ) -> Result<orbit_runtime::portfolio_actions::PortfolioActionReport, ClientConnectionError>
    {
        match self {
            Self::Embedded(host) => Ok(host.portfolio_actions(request)?),
            Self::Remote { .. } => {
                let readiness = self.task_readiness(request.now_ms)?;
                let snapshot =
                    self.accepted_snapshot()?
                        .ok_or(ClientConnectionError::Unsupported(
                            "portfolio planning requires an accepted snapshot",
                        ))?;
                let revision = snapshot.get("revision").and_then(Value::as_i64).ok_or(
                    ClientConnectionError::Unsupported("missing accepted revision"),
                )?;
                if revision != readiness.revision {
                    return Err(ClientConnectionError::Unsupported(
                        "accepted revision mismatch",
                    ));
                }
                let tasks: Vec<orbit_runtime::domain_commands::TaskRecord> =
                    serde_json::from_value(
                        snapshot
                            .get("tasks")
                            .cloned()
                            .ok_or(ClientConnectionError::Unsupported("missing tasks"))?,
                    )?;
                orbit_runtime::portfolio_actions::recommend_portfolio_actions(
                    revision,
                    &tasks,
                    &readiness.edges,
                    request,
                )
                .map_err(ClientConnectionError::Unsupported)
            }
        }
    }

    pub fn task_graph(
        &self,
        now_ms: i64,
    ) -> Result<orbit_runtime::task_graph::TaskGraphReport, ClientConnectionError> {
        match self {
            Self::Embedded(host) => Ok(host.task_graph(now_ms)?),
            Self::Remote { .. } => {
                let readiness = self.task_readiness(now_ms)?;
                let snapshot =
                    self.accepted_snapshot()?
                        .ok_or(ClientConnectionError::Unsupported(
                            "graph requires an accepted snapshot",
                        ))?;
                let tasks: Vec<orbit_runtime::domain_commands::TaskRecord> =
                    serde_json::from_value(
                        snapshot
                            .get("tasks")
                            .cloned()
                            .ok_or(ClientConnectionError::Unsupported("missing tasks"))?,
                    )?;
                orbit_runtime::task_graph::analyze_task_graph(
                    readiness.revision,
                    &tasks,
                    &readiness.edges,
                    now_ms,
                )
                .map_err(ClientConnectionError::Unsupported)
            }
        }
    }

    /// Deterministic readiness over accepted local state only. Remote queued
    /// intentions are deliberately not projected as completed prerequisites.
    /// Preview a task dependency edit against accepted state only. In connected
    /// mode this deliberately ignores all pending intentions in the outbox.
    pub fn preview_task_dependency(
        &self,
        request: &orbit_runtime::task_dependency_preview::DependencyPreviewRequest,
    ) -> Result<orbit_runtime::task_dependency_preview::DependencyPreview, ClientConnectionError>
    {
        let (revision, tasks, edges) = match self {
            Self::Embedded(host) => {
                let (revision, tasks) = host.tasks()?;
                (revision, tasks, host.task_dependency_edges()?)
            }
            Self::Remote { replica, .. } => {
                let cache = replica.workspace()?;
                let revision = cache.revision.ok_or(ClientConnectionError::Unsupported(
                    "dependency preview requires accepted authoritative state",
                ))?;
                let tasks: Vec<orbit_runtime::domain_commands::TaskRecord> = cache
                    .tasks
                    .into_iter()
                    .map(serde_json::from_value)
                    .collect::<Result<_, _>>()?;
                (revision, tasks, cache.task_dependencies)
            }
        };
        orbit_runtime::task_dependency_preview::preview_dependency(
            revision, &tasks, &edges, request,
        )
        .map_err(ClientConnectionError::Unsupported)
    }

    pub fn task_readiness(
        &self,
        now_ms: i64,
    ) -> Result<orbit_runtime::task_dependencies::TaskReadinessSnapshot, ClientConnectionError>
    {
        if now_ms < 0 {
            return Err(ClientConnectionError::Unsupported("invalid planning time"));
        }
        match self {
            Self::Embedded(host) => Ok(host.task_readiness(now_ms)?),
            Self::Remote { replica, .. } => {
                let cached = replica.workspace()?;
                let revision = cached.revision.ok_or(ClientConnectionError::Unsupported(
                    "offline readiness requires an accepted server snapshot",
                ))?;
                let tasks: Vec<orbit_runtime::domain_commands::TaskRecord> = cached
                    .tasks
                    .into_iter()
                    .map(serde_json::from_value)
                    .collect::<Result<_, _>>()?;
                orbit_runtime::task_dependencies::evaluate_readiness(
                    revision,
                    &tasks,
                    &cached.task_dependencies,
                    now_ms,
                )
                .map_err(ClientConnectionError::Unsupported)
            }
        }
    }
}
