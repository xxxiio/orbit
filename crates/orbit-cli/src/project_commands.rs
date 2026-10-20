//! Capability-specific CLI adapter; delegates mutations to the authoritative host.
use super::*;

/// Project operations with a shared host implementation. Advanced planning,
/// evidence, and project-mode queries use a transitional repository boundary
/// until their typed query contracts are available in ApplicationQueries.
pub(super) fn run_project_host(
    host: &mut orbit_runtime::AuthoritativeHost,
    command: ProjectSubcommand,
) -> Result<Response, CliError> {
    match command {
        ProjectSubcommand::Add(args) => {
            let id = ProjectId::new(generate_id("project")).map_err(CliError::domain)?;
            let expected_revision = host.snapshot().map_err(CliError::repository)?.0;
            let (_, _, project) = host
                .create_project_accepted(
                    "orbit-cli",
                    &generate_id("command"),
                    expected_revision,
                    CreateProjectWithActivityCommand {
                        project: CreateProjectCommand {
                            id: id.clone(),
                            name: args.name,
                        },
                        activity_id: ActivityId::new(generate_id("activity"))
                            .map_err(CliError::domain)?,
                        origin: ActivityOrigin::Human,
                        summary: format!("Project {} created", id),
                    },
                )
                .map_err(CliError::application)?;
            Ok(Response::Project {
                project: ProjectView::from(&project),
                action: "created",
            })
        }
        ProjectSubcommand::List(args) => {
            let mut projects = host
                .application_queries()
                .projects()
                .map_err(CliError::application)?;
            if let Some(status) = args.status {
                let status: ProjectStatus = status.into();
                projects.retain(|project| project.status() == status);
            }
            if let Some(query) = normalized_filter(args.query)? {
                projects
                    .retain(|project| text_matches(&query, project.id().as_str(), project.name()));
            }
            projects.sort_by(|left, right| left.id().as_str().cmp(right.id().as_str()));
            Ok(Response::Projects {
                projects: projects.iter().map(ProjectView::from).collect(),
            })
        }
        ProjectSubcommand::Status(args) => {
            let id = ProjectId::new(args.id).map_err(CliError::domain)?;
            let status: ProjectStatus = args.status.into();
            let expected_revision = host.snapshot().map_err(CliError::repository)?.0;
            let (_, _, project) = host
                .transition_project_accepted(
                    "orbit-cli",
                    &generate_id("command"),
                    expected_revision,
                    TransitionProjectWithActivityCommand {
                        project: TransitionProjectCommand {
                            id: id.clone(),
                            status,
                        },
                        activity_id: ActivityId::new(generate_id("activity"))
                            .map_err(CliError::domain)?,
                        origin: ActivityOrigin::Human,
                        summary: format!("Project {} status changed to {}", id, status.as_str()),
                    },
                )
                .map_err(CliError::application)?;
            Ok(Response::Project {
                project: ProjectView::from(&project),
                action: "status changed",
            })
        }
        ProjectSubcommand::Configure(args) => {
            let id = ProjectId::new(args.id).map_err(CliError::domain)?;
            let mode: ProjectMode = args.mode.into();
            let progress_signals = if args.signals.is_empty() {
                mode.default_signals()
            } else {
                args.signals
                    .into_iter()
                    .map(ProjectProgressSignal::from)
                    .collect()
            };
            let mut criteria = Vec::new();
            for raw in args.criteria {
                criteria.push(parse_project_criterion(&raw, true)?);
            }
            for raw in args.optional_criteria {
                criteria.push(parse_project_criterion(&raw, false)?);
            }
            let project = {
                let revision = host.snapshot().map_err(CliError::repository)?.0;
                let (_, _, _accepted_id) = host
                    .configure_project_strategy_accepted(
                        "orbit-cli",
                        &generate_id("command"),
                        revision,
                        ConfigureProjectStrategyWithActivityCommand {
                            project: ConfigureProjectStrategyCommand {
                                id: id.clone(),
                                mode,
                                objective: args.objective,
                                progress_signals,
                                success_criteria: criteria,
                            },
                            activity_id: ActivityId::new(generate_id("activity"))
                                .map_err(CliError::domain)?,
                            origin: ActivityOrigin::Human,
                            summary: format!("Project {} configured as {}", id, mode.as_str()),
                        },
                    )
                    .map_err(CliError::application)?;
                host.application_queries()
                    .project(&id)
                    .map_err(CliError::application)?
            };
            Ok(Response::Project {
                project: ProjectView::from(&project),
                action: "configured",
            })
        }
        ProjectSubcommand::Criterion(args) => {
            let id = ProjectId::new(args.id).map_err(CliError::domain)?;
            let status: SuccessCriterionStatus = args.status.into();
            let project = {
                let revision = host.snapshot().map_err(CliError::repository)?.0;
                let (_, _, _accepted_id) = host
                    .set_project_criterion_status_accepted(
                        "orbit-cli",
                        &generate_id("command"),
                        revision,
                        SetProjectCriterionStatusWithActivityCommand {
                            project_id: id.clone(),
                            criterion_key: args.key.clone(),
                            status,
                            activity_id: ActivityId::new(generate_id("activity"))
                                .map_err(CliError::domain)?,
                            origin: ActivityOrigin::Human,
                            summary: format!(
                                "Project {} criterion {} changed to {}",
                                id,
                                args.key,
                                status.as_str()
                            ),
                        },
                    )
                    .map_err(CliError::application)?;
                host.application_queries()
                    .project(&id)
                    .map_err(CliError::application)?
            };
            Ok(Response::Project {
                project: ProjectView::from(&project),
                action: "criterion changed",
            })
        }
        ProjectSubcommand::Metric(args) => match args.command {
            ProjectMetricSubcommand::Target {
                metric_id,
                baseline,
                target,
                direction,
            } => {
                let direction = match direction {
                    CliMetricDirection::AtLeast => MetricTargetDirection::AtLeast,
                    CliMetricDirection::AtMost => MetricTargetDirection::AtMost,
                };
                let expected_revision = host.snapshot().map_err(CliError::repository)?.0;
                host.set_metric_target_accepted(
                    "orbit-cli",
                    &generate_id("command"),
                    expected_revision,
                    MetricTargetRecord {
                        metric_id: metric_id.clone(),
                        baseline,
                        target,
                        direction,
                    },
                )
                .map_err(CliError::application)?;
                Ok(Response::MetricUpdated {
                    metric_id,
                    action: "target saved and queued for sync",
                })
            }
            ProjectMetricSubcommand::Observe {
                metric_id,
                value,
                observed_at_ms,
            } => {
                let expected_revision = host.snapshot().map_err(CliError::repository)?.0;
                host.record_metric_observation_accepted(
                    "orbit-cli",
                    &generate_id("command"),
                    expected_revision,
                    MetricObservation {
                        metric_id: metric_id.clone(),
                        value,
                        observed_at_ms,
                    },
                )
                .map_err(CliError::application)?;
                Ok(Response::MetricUpdated {
                    metric_id,
                    action: "observation recorded and queued for sync",
                })
            }
        },
        ProjectSubcommand::Evidence(args) => {
            let (project_id, change) = match args.command {
                ProjectEvidenceSubcommand::Report {
                    project_id,
                    start_ms,
                    end_ms,
                    as_of_ms,
                } => {
                    let id = ProjectId::new(project_id).map_err(CliError::domain)?;
                    if end_ms > as_of_ms {
                        return Err(CliError::Usage(
                            "report window must end at or before as-of time".into(),
                        ));
                    }
                    let period = HabitPeriod::new(start_ms, end_ms).map_err(CliError::domain)?;
                    let report = host
                        .application_queries()
                        .project_evidence_report(&id, &[period], as_of_ms)
                        .map_err(CliError::application)?;
                    return Ok(Response::ProjectEvidenceReport {
                        project_id: id.to_string(),
                        progress_bp: report.evidence_progress.map(|v| v.value()),
                        habits: report
                            .habits
                            .habits
                            .into_iter()
                            .map(|h| EvidenceHabitView {
                                id: h.habit_id.to_string(),
                                available: h.available,
                                fulfilled: h.fulfilled_periods,
                                evaluated: h.evaluated_periods,
                                progress_bp: h.consistency.map(|v| v.value()),
                            })
                            .collect(),
                        metrics: report
                            .metrics
                            .into_iter()
                            .map(|m| EvidenceMetricView {
                                id: m.metric_id,
                                progress_bp: m.evaluation.map(|e| e.progress.value()),
                                reached: m.evaluation.map(|e| e.reached),
                            })
                            .collect(),
                    });
                }
                ProjectEvidenceSubcommand::List { project_id } => (project_id, None),
                ProjectEvidenceSubcommand::Bind {
                    project_id,
                    kind,
                    evidence_id,
                } => (project_id, Some((true, kind, evidence_id))),
                ProjectEvidenceSubcommand::Unbind {
                    project_id,
                    kind,
                    evidence_id,
                } => (project_id, Some((false, kind, evidence_id))),
            };
            let id = ProjectId::new(project_id).map_err(CliError::domain)?;
            if let Some((bind, kind, evidence_id)) = change {
                let binding = match kind {
                    CliEvidenceKind::Habit => ProjectEvidenceBinding::Habit(
                        HabitId::new(evidence_id).map_err(CliError::domain)?,
                    ),
                    CliEvidenceKind::Metric => ProjectEvidenceBinding::Metric(evidence_id),
                };
                let expected_revision = host.snapshot().map_err(CliError::repository)?.0;
                host.change_project_evidence_accepted(
                    "orbit-cli",
                    &generate_id("command"),
                    expected_revision,
                    bind,
                    ChangeProjectEvidenceCommand {
                        project_id: id.clone(),
                        binding,
                    },
                )
                .map_err(CliError::application)?;
            }
            let bindings = host
                .application_queries()
                .project_evidence(&id)
                .map_err(CliError::application)?;
            Ok(Response::ProjectEvidence {
                project_id: id.to_string(),
                bindings: bindings
                    .into_iter()
                    .map(|binding| match binding {
                        ProjectEvidenceBinding::Habit(id) => EvidenceBindingView {
                            kind: "habit",
                            id: id.to_string(),
                        },
                        ProjectEvidenceBinding::Metric(id) => {
                            EvidenceBindingView { kind: "metric", id }
                        }
                    })
                    .collect(),
            })
        }
        ProjectSubcommand::Health(args) => {
            let id = ProjectId::new(args.id).map_err(CliError::domain)?;
            let as_of_ms = now_ms();
            let window =
                HabitPeriod::new(as_of_ms.saturating_sub(7 * 24 * 60 * 60 * 1000), as_of_ms)
                    .map_err(CliError::domain)?;
            let report = host
                .application_queries()
                .mode_aware_project_health(
                    &id,
                    &[window],
                    as_of_ms,
                    args.lookback_minutes,
                    HealthPolicy::default(),
                )
                .map_err(CliError::application)?;
            let mut health = ProjectHealthView::from(&report.canonical);
            health.mode_progress_bp = report.selected_progress.map(|p| p.value());
            health.evidence_progress_bp =
                report.bound_evidence.evidence_progress.map(|p| p.value());
            health.available_signals = report
                .available_signals
                .iter()
                .map(|s| s.as_str().to_owned())
                .collect();
            health.unavailable_signals = report
                .unavailable_signals
                .iter()
                .map(|s| s.as_str().to_owned())
                .collect();
            Ok(Response::ProjectHealth { health })
        }
    }
}

fn parse_project_criterion(raw: &str, required: bool) -> Result<ProjectSuccessCriterion, CliError> {
    let (key, description) = raw
        .split_once('=')
        .ok_or_else(|| CliError::Usage("project criterion must use key=description".to_owned()))?;
    ProjectSuccessCriterion::new(
        key.trim().to_owned(),
        description.trim().to_owned(),
        required,
    )
    .map_err(CliError::domain)
}
