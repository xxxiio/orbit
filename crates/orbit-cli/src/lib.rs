mod agent_commands;
mod authority_http;
mod blueprint_commands;
mod connected_agent;
mod connected_blueprint;
mod doctor_command;
mod habit_routine_commands;
mod init_command;
mod project_commands;
mod schedule_commands;
mod task_commands;
#[cfg(test)]
use task_commands::run_task;
mod authority_replica;
mod markdown_assets;
mod markdown_cli;
mod markdown_discovery;
mod markdown_metadata;
mod replica_workflow;
mod update_command;
use markdown_cli::{FolderCommand, NoteCommand, run_folder_host, run_note_host};
use std::{
    fs,
    path::{Path, PathBuf},
    process,
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use clap::{Args, Parser, Subcommand, ValueEnum};
use orbit_application::{
    ChangeProjectEvidenceCommand, ConfigureProjectStrategyCommand,
    ConfigureProjectStrategyWithActivityCommand, CreateHabitCommand,
    CreateHabitWithActivityCommand, CreateProjectCommand, CreateProjectWithActivityCommand,
    CreateRoutineWithActivityCommand, CreateScheduleBlockCommand,
    CreateScheduleBlockWithActivityCommand, CreateTaskCommand, CreateTaskWithActivityCommand,
    MetricObservation, MetricTargetRecord, ProjectEvidenceBinding, ProjectHealthSnapshot,
    RecordHabitOccurrenceWithActivityCommand, RescheduleBlockWithActivityCommand,
    SetHabitActiveWithActivityCommand, SetProjectCriterionStatusWithActivityCommand,
    SetRoutineHabitMembershipWithActivityCommand, SetTaskPlanningCommand,
    SetTaskPlanningWithActivityCommand, StartTaskWithActivityCommand, TransitionProjectCommand,
    TransitionProjectWithActivityCommand, TransitionTaskCommand, TransitionTaskWithActivityCommand,
};
use orbit_domain::{
    ActivityId, ActivityOrigin, Habit, HabitCadence, HabitId, HabitOccurrence, HabitOccurrenceId,
    HabitPeriod, MilestoneId, ProjectId, ProjectMode, ProjectProgressSignal, ProjectStatus,
    ProjectSuccessCriterion, Routine, RoutineId, ScheduleBlock, ScheduleBlockId,
    ScheduleFlexibility, ScheduleTarget, SuccessCriterionStatus, Task, TaskId, TaskPlanning,
    TaskScope, TaskStatus,
};
use orbit_progress::{
    HealthPolicy, MetricTargetDirection, ProjectHealthReason, ProjectHealthState,
};

use serde::Serialize;

static ID_SEQUENCE: AtomicU64 = AtomicU64::new(0);

mod commands;
pub use commands::*;

pub fn run(cli: Cli) -> Result<String, CliError> {
    let json = cli.json;
    let db_override = cli.db.clone();
    let config_override = cli.config.clone();
    let command = match cli.command {
        // Operational setup and diagnostics must work before a deployment or
        // workspace is opened; neither command opens a database for writes.
        Command::Init(args) => {
            return init_command::run(
                args,
                db_override.as_deref(),
                config_override.as_deref(),
                json,
            );
        }
        Command::Doctor(args) => {
            return doctor_command::run(
                args,
                db_override.as_deref(),
                config_override.as_deref(),
                json,
            );
        }
        // Updates must not open (or migrate) the workspace database. The
        // command inspects and backs it up read-only before replacing binaries.
        Command::Update(args) => {
            return update_command::run(
                args,
                db_override.as_deref(),
                config_override.as_deref(),
                json,
            );
        }
        command => command,
    };

    let resolved =
        match orbit_client::config::resolve(db_override.as_deref(), config_override.as_deref()) {
            Ok(resolved) => Some(resolved),
            // A process without a home directory keeps the historical relative
            // fallback for ordinary commands. Init and doctor fail earlier.
            Err(orbit_client::config::ConfigError::NoHome) => None,
            Err(error) => return Err(CliError::Usage(error.to_string())),
        };
    let deployment = match &resolved {
        Some(resolved) => resolved.deployment.clone(),
        None => orbit_client::deployment::resolve_deployment(
            db_override.as_deref(),
            config_override.as_deref(),
        )
        .map_err(|error| CliError::Usage(error.to_string()))?,
    };
    let db_path = match &deployment {
        orbit_client::deployment::Deployment::Local { database } => database.clone(),
        orbit_client::deployment::Deployment::Connected { replica, .. } => replica.clone(),
    };
    let default_agent_provider = resolved
        .as_ref()
        .and_then(|resolved| resolved.config.agent.as_ref())
        .and_then(|agent| agent.default_provider.clone());
    if let Command::Blueprint(BlueprintCommand {
        offline: false,
        require_online: false,
        command: BlueprintSubcommand::Validate { file },
    }) = &command
    {
        let response = blueprint_commands::validate_file(file)?;
        return if json {
            serde_json::to_string_pretty(&response)
                .map_err(|error| CliError::Serialization(error.to_string()))
        } else {
            Ok(response.human())
        };
    }
    let response = match command {
        Command::Capabilities(args) => Response::Authority {
            result: serde_json::to_value(orbit_runtime::capabilities::discover(
                args.id.as_deref(),
                None,
                args.accepted_only,
            ))
            .map_err(|error| CliError::Serialization(error.to_string()))?,
        },
        Command::Sync(command) => {
            // Connected mode uses its own native replica, never the local-only store.
            run_sync(&db_path, command.command)?
        }
        command => {
            // The local CLI is never a fallback for an explicitly connected
            // deployment. Use `sync replica` to queue remote intentions.
            // Prevent accidental writes to a second authority database.
            match deployment {
                orbit_client::deployment::Deployment::Connected { server, .. } => {
                    if let Command::Blueprint(blueprint) = command {
                        let response = connected_blueprint::run(
                            &server,
                            &db_path,
                            blueprint.command,
                            blueprint.offline,
                            blueprint.require_online,
                        )?;
                        return if json {
                            serde_json::to_string_pretty(&response)
                                .map_err(|e| CliError::Serialization(e.to_string()))
                        } else {
                            Ok(response.to_string())
                        };
                    }
                    if let Command::Agent(agent) = command {
                        let response = connected_agent::run(
                            &server,
                            &db_path,
                            agent.command,
                            default_agent_provider.as_deref(),
                        )?;
                        return if json {
                            serde_json::to_string_pretty(&response)
                                .map_err(|e| CliError::Serialization(e.to_string()))
                        } else {
                            Ok(response.to_string())
                        };
                    } else {
                        return Err(CliError::Usage(
                            "connected deployment: use `orbit sync replica` for accepted reads and queued writes".into()));
                    }
                }
                orbit_client::deployment::Deployment::Local { .. } => {}
            }
            ensure_parent(&db_path)?;
            let mut host = orbit_runtime::AuthoritativeHost::open(&db_path).map_err(|error| {
                CliError::Serialization(format!("authority unavailable: {error:?}"))
            })?;
            if let Command::Note(note) = command {
                run_note_host(&mut host, &db_path, note.command)?
            } else if let Command::Folder(folder) = command {
                run_folder_host(&mut host, folder.command)?
            } else if let Command::Routine(routine) = command {
                habit_routine_commands::run_routine_host(&mut host, routine.command)?
            } else if let Command::Habit(habit) = command {
                habit_routine_commands::run_habit_host(&mut host, habit.command)?
            } else if let Command::Blueprint(blueprint) = command {
                {
                    if blueprint.offline {
                        return Err(CliError::Usage("--offline requires connected mode".into()));
                    }
                    blueprint_commands::run_blueprint(&mut host, blueprint.command)?
                }
            } else if let Command::Agent(agent) = command {
                agent_commands::run_agent(
                    &mut host,
                    agent.command,
                    default_agent_provider.as_deref(),
                )?
            } else if let Command::Task(task) = command {
                task_commands::run_task(&mut host, task.command)?
            } else if let Command::Project(project) = command {
                project_commands::run_project_host(&mut host, project.command)?
            } else if let Command::Schedule(schedule) = command {
                schedule_commands::run_schedule_host(&mut host, schedule.command)?
            } else if let Command::Today = command {
                today_host(&host)?
            } else if let Command::Status = command {
                workspace_status_host(&host, &db_path)?
            } else if let Command::Search(args) = command {
                search_host(&host, args)?
            } else {
                unreachable!("all local CLI commands are handled through the authoritative host")
            }
        }
    };

    if json {
        serde_json::to_string_pretty(&response)
            .map_err(|error| CliError::Serialization(error.to_string()))
    } else {
        Ok(response.human())
    }
}

fn run_sync(_db_path: &Path, command: SyncSubcommand) -> Result<Response, CliError> {
    let value = match command {
        SyncSubcommand::Replica(args) => authority_replica::invoke(args),
        SyncSubcommand::Authority(args) => authority_http::invoke(args),
    }
    .map_err(CliError::sync)?;
    Ok(Response::Authority { result: value })
}

fn workspace_status_host(
    host: &orbit_runtime::AuthoritativeHost,
    db_path: &Path,
) -> Result<Response, CliError> {
    let diagnostics = host
        .application_queries()
        .diagnostics()
        .map_err(CliError::application)?;
    // Markdown files are authoritative; do not count legacy SQLite knowledge bodies.
    let knowledge_items = host
        .markdown_knowledge_index()
        .map_err(|err| CliError::Serialization(format!("knowledge index unavailable: {err:?}")))?
        .len();
    Ok(Response::WorkspaceStatus {
        database: db_path.display().to_string(),
        schema_version: diagnostics.schema_version,
        healthy: diagnostics.integrity.is_healthy(),
        issues: diagnostics
            .integrity
            .issues
            .into_iter()
            .map(|issue| WorkspaceIssueView {
                area: issue.area.to_owned(),
                detail: issue.detail,
            })
            .collect(),
        wal_enabled: diagnostics.concurrency.wal_enabled,
        foreign_keys_enabled: diagnostics.concurrency.foreign_keys_enabled,
        busy_timeout_ms: diagnostics.concurrency.busy_timeout_ms,
        projects: diagnostics.projects,
        tasks: diagnostics.tasks,
        habits: diagnostics.habits,
        routines: diagnostics.routines,
        knowledge_items,
    })
}

fn search_host(
    host: &orbit_runtime::AuthoritativeHost,
    args: SearchArgs,
) -> Result<Response, CliError> {
    let query = args.query.trim().to_lowercase();
    if query.is_empty() {
        return Err(CliError::Usage("search query must not be blank".to_owned()));
    }

    let queries = host.application_queries();
    let mut results = Vec::new();
    for project in queries.projects().map_err(CliError::application)? {
        if text_matches(&query, project.id().as_str(), project.name()) {
            results.push(SearchResultView::Project(ProjectView::from(&project)));
        }
    }
    for task in queries.tasks().map_err(CliError::application)? {
        if text_matches(&query, task.id().as_str(), task.title()) {
            results.push(SearchResultView::Task(TaskView::from(&task)));
        }
    }
    for habit in queries.habits().map_err(CliError::application)? {
        if text_matches(&query, habit.id().as_str(), habit.name()) {
            results.push(SearchResultView::Habit(HabitView::from(&habit)));
        }
    }
    for routine in queries.routines().map_err(CliError::application)? {
        if text_matches(&query, routine.id().as_str(), routine.name()) {
            results.push(SearchResultView::Routine(RoutineView::from(&routine)));
        }
    }
    // Content and folder organization are authoritative in Markdown files.
    // A search must not quietly read an obsolete SQL knowledge body table.
    for document in host
        .markdown_search(&query, 100)
        .map_err(|err| CliError::Serialization(format!("Markdown search unavailable: {err:?}")))?
    {
        results.push(SearchResultView::Markdown(document));
    }
    if let (Some(from_ms), Some(to_ms)) = (args.from_ms, args.to_ms) {
        if to_ms <= from_ms {
            return Err(CliError::Usage(
                "--to-ms must be greater than --from-ms".to_owned(),
            ));
        }
        for block in queries
            .schedule_between(from_ms, to_ms)
            .map_err(CliError::application)?
        {
            if text_matches(&query, block.id().as_str(), block.title()) {
                results.push(SearchResultView::ScheduleBlock(ScheduleBlockView::from(
                    &block,
                )));
            }
        }
    }
    results.sort_by(|left, right| left.sort_key().cmp(&right.sort_key()));
    Ok(Response::Search {
        query: args.query,
        results,
    })
}

fn text_matches(query: &str, id: &str, label: &str) -> bool {
    id.to_lowercase().contains(query) || label.to_lowercase().contains(query)
}

fn normalized_filter(query: Option<String>) -> Result<Option<String>, CliError> {
    match query {
        Some(query) if query.trim().is_empty() => {
            Err(CliError::Usage("--query must not be blank".to_owned()))
        }
        Some(query) => Ok(Some(query.trim().to_lowercase())),
        None => Ok(None),
    }
}

fn today_host(host: &orbit_runtime::AuthoritativeHost) -> Result<Response, CliError> {
    let mut tasks = host
        .application_queries()
        .tasks()
        .map_err(CliError::application)?;
    tasks.retain(|task| {
        matches!(
            task.status(),
            TaskStatus::InProgress | TaskStatus::Ready | TaskStatus::Planned
        )
    });
    tasks.sort_by(|left, right| {
        let rank = |status| match status {
            TaskStatus::InProgress => 0,
            TaskStatus::Ready => 1,
            TaskStatus::Planned => 2,
            _ => 3,
        };
        rank(left.status())
            .cmp(&rank(right.status()))
            .then_with(|| left.id().as_str().cmp(right.id().as_str()))
    });
    Ok(Response::Today {
        tasks: tasks.iter().map(TaskView::from).collect(),
        provisional: true,
    })
}

fn task_scope(project: Option<String>, milestone: Option<String>) -> Result<TaskScope, CliError> {
    match (project, milestone) {
        (None, None) => Ok(TaskScope::standalone()),
        (Some(project), None) => Ok(TaskScope::in_project(
            ProjectId::new(project).map_err(CliError::domain)?,
        )),
        (Some(project), Some(milestone)) => Ok(TaskScope::in_milestone(
            ProjectId::new(project).map_err(CliError::domain)?,
            MilestoneId::new(milestone).map_err(CliError::domain)?,
        )),
        (None, Some(_)) => Err(CliError::Usage("--milestone requires --project".to_owned())),
    }
}

fn schedule_target(
    task: Option<String>,
    habit: Option<String>,
) -> Result<ScheduleTarget, CliError> {
    match (task, habit) {
        (None, None) => Ok(ScheduleTarget::Standalone),
        (Some(task), None) => Ok(ScheduleTarget::Task(
            TaskId::new(task).map_err(CliError::domain)?,
        )),
        (None, Some(habit)) => Ok(ScheduleTarget::Habit(
            HabitId::new(habit).map_err(CliError::domain)?,
        )),
        (Some(_), Some(_)) => Err(CliError::Usage(
            "schedule target accepts only one of --task or --habit".to_owned(),
        )),
    }
}

#[derive(Debug, Serialize)]
struct EvidenceBindingView {
    kind: &'static str,
    id: String,
}
#[derive(Debug, Serialize)]
struct EvidenceHabitView {
    id: String,
    available: bool,
    fulfilled: u32,
    evaluated: u32,
    progress_bp: Option<u16>,
}
#[derive(Debug, Serialize)]
struct EvidenceMetricView {
    id: String,
    progress_bp: Option<u16>,
    reached: Option<bool>,
}

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Response {
    Authority {
        result: serde_json::Value,
    },
    Task {
        task: TaskView,
        action: &'static str,
    },
    Tasks {
        tasks: Vec<TaskView>,
    },
    Project {
        project: ProjectView,
        action: &'static str,
    },
    Projects {
        projects: Vec<ProjectView>,
    },
    ProjectHealth {
        health: ProjectHealthView,
    },
    ProjectEvidence {
        project_id: String,
        bindings: Vec<EvidenceBindingView>,
    },
    ProjectEvidenceReport {
        project_id: String,
        progress_bp: Option<u16>,
        habits: Vec<EvidenceHabitView>,
        metrics: Vec<EvidenceMetricView>,
    },
    MetricUpdated {
        metric_id: String,
        action: &'static str,
    },
    Habit {
        habit: HabitView,
        action: &'static str,
    },
    Habits {
        habits: Vec<HabitView>,
    },
    Routine {
        routine: RoutineView,
        action: &'static str,
    },
    Routines {
        routines: Vec<RoutineView>,
    },
    HabitOccurrence {
        occurrence: HabitOccurrenceView,
        action: &'static str,
    },
    HabitOccurrences {
        habit_id: String,
        occurrences: Vec<HabitOccurrenceView>,
    },
    ScheduleBlock {
        block: ScheduleBlockView,
        action: &'static str,
    },
    ScheduleBlocks {
        blocks: Vec<ScheduleBlockView>,
    },
    Search {
        query: String,
        results: Vec<SearchResultView>,
    },
    WorkspaceStatus {
        database: String,
        schema_version: i64,
        healthy: bool,
        issues: Vec<WorkspaceIssueView>,
        wal_enabled: bool,
        foreign_keys_enabled: bool,
        busy_timeout_ms: u64,
        projects: usize,
        tasks: usize,
        habits: usize,
        routines: usize,
        knowledge_items: usize,
    },
    Today {
        tasks: Vec<TaskView>,
        provisional: bool,
    },
}

impl Response {
    fn human(&self) -> String {
        match self {
            Self::Task { task, action } => {
                format!(
                    "{} task {}: {} [{}]",
                    action, task.id, task.title, task.status
                )
            }
            Self::Tasks { tasks } => render_tasks(tasks),
            Self::Project { project, action } => format!(
                "{} project {}: {} [{}]",
                action, project.id, project.name, project.status
            ),
            Self::Projects { projects } => {
                if projects.is_empty() {
                    return "No projects.".to_owned();
                }
                projects
                    .iter()
                    .map(|project| {
                        format!("{}  {:<12}  {}", project.id, project.status, project.name)
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            }
            Self::MetricUpdated { metric_id, action } => format!("Metric {metric_id}: {action}"),
            Self::ProjectEvidenceReport {
                project_id,
                progress_bp,
                habits,
                metrics,
            } => {
                let format_progress = |value: Option<u16>| {
                    value
                        .map(|v| format!("{:.1}%", f64::from(v) / 100.0))
                        .unwrap_or_else(|| "unknown".into())
                };
                let mut lines = vec![format!(
                    "Project {project_id} evidence: {}",
                    format_progress(*progress_bp)
                )];
                for habit in habits {
                    lines.push(format!(
                        "habit {}: {} ({}/{})",
                        habit.id,
                        format_progress(habit.progress_bp),
                        habit.fulfilled,
                        habit.evaluated
                    ));
                }
                for metric in metrics {
                    lines.push(format!(
                        "metric {}: {}",
                        metric.id,
                        format_progress(metric.progress_bp)
                    ));
                }
                lines.join("\n")
            }
            Self::ProjectEvidence {
                project_id,
                bindings,
            } => {
                if bindings.is_empty() {
                    return format!("Project {project_id}: no evidence bindings");
                }
                format!(
                    "Project {project_id} evidence:\n{}",
                    bindings
                        .iter()
                        .map(|b| format!("{}: {}", b.kind, b.id))
                        .collect::<Vec<_>>()
                        .join("\n")
                )
            }
            Self::ProjectHealth { health } => {
                let reasons = if health.reasons.is_empty() {
                    "none".to_owned()
                } else {
                    health.reasons.join(", ")
                };
                format!(
                    "Project {} health: {}\nTask progress: {:.1}%\nMode progress: {}\nBound evidence: {}\nMissing signals: {}\nReasons: {}\nBlocked: tasks={} milestones={}\nRecent activity: count={} last_age_minutes={}",
                    health.project_id,
                    health.state,
                    f64::from(health.progress_bp) / 100.0,
                    health
                        .mode_progress_bp
                        .map(|p| format!("{:.1}%", f64::from(p) / 100.0))
                        .unwrap_or_else(|| "unknown".into()),
                    health
                        .evidence_progress_bp
                        .map(|p| format!("{:.1}%", f64::from(p) / 100.0))
                        .unwrap_or_else(|| "unknown".into()),
                    health.unavailable_signals.join(", "),
                    reasons,
                    health.blocked_tasks,
                    health.blocked_milestones,
                    health.recent_activity_count,
                    health
                        .last_activity_age_minutes
                        .map(|value| value.to_string())
                        .unwrap_or_else(|| "none".to_owned()),
                )
            }
            Self::Habit { habit, action } => format!(
                "{} habit {}: {} [{}; {}]",
                action, habit.id, habit.name, habit.cadence, habit.state
            ),
            Self::Habits { habits } => {
                if habits.is_empty() {
                    return "No habits.".to_owned();
                }
                habits
                    .iter()
                    .map(|habit| {
                        format!(
                            "{}  {:<8}  {:<12}  {}",
                            habit.id, habit.state, habit.cadence, habit.name
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            }
            Self::Routine { routine, action } => format!(
                "{} routine {}: {} [{} habits]",
                action,
                routine.id,
                routine.name,
                routine.habit_ids.len()
            ),
            Self::Routines { routines } => {
                if routines.is_empty() {
                    return "No routines.".to_owned();
                }
                routines
                    .iter()
                    .map(|routine| {
                        format!(
                            "{}  {:>2} habits  {}",
                            routine.id,
                            routine.habit_ids.len(),
                            routine.name
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            }
            Self::HabitOccurrence { occurrence, action } => format!(
                "{} habit occurrence {} habit={} at={} duration={}m",
                action,
                occurrence.id,
                occurrence.habit_id,
                occurrence.occurred_at_ms,
                occurrence
                    .duration_minutes
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "-".to_owned())
            ),
            Self::HabitOccurrences {
                habit_id,
                occurrences,
            } => {
                if occurrences.is_empty() {
                    return format!("No occurrences for habit {habit_id}.");
                }
                occurrences
                    .iter()
                    .map(|occurrence| {
                        format!(
                            "{}  {}  duration={}m",
                            occurrence.id,
                            occurrence.occurred_at_ms,
                            occurrence
                                .duration_minutes
                                .map(|v| v.to_string())
                                .unwrap_or_else(|| "-".to_owned())
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            }
            Self::ScheduleBlock { block, action } => format!(
                "{} schedule {}: {} start={} duration={}m [{}; {}]",
                action,
                block.id,
                block.title,
                block.starts_at_ms,
                block.duration_minutes,
                block.flexibility,
                block.target
            ),
            Self::ScheduleBlocks { blocks } => {
                if blocks.is_empty() {
                    return "No schedule blocks in range.".to_owned();
                }
                blocks
                    .iter()
                    .map(|block| {
                        format!(
                            "{}  {}  {:>4}m  {:<7}  {:<20}  {}",
                            block.id,
                            block.starts_at_ms,
                            block.duration_minutes,
                            block.flexibility,
                            block.target,
                            block.title
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            }
            Self::Search { query, results } => {
                if results.is_empty() {
                    return format!("No matches for {query:?}.");
                }
                results
                    .iter()
                    .map(SearchResultView::human)
                    .collect::<Vec<_>>()
                    .join("\n")
            }
            Self::Authority { result } => serde_json::to_string_pretty(result)
                .unwrap_or_else(|_| "invalid server response".into()),
            Self::WorkspaceStatus {
                database,
                schema_version,
                healthy,
                issues,
                wal_enabled,
                foreign_keys_enabled,
                busy_timeout_ms,
                projects,
                tasks,
                habits,
                routines,
                knowledge_items,
            } => {
                let mut lines = vec![
                    format!("Workspace: {database}"),
                    format!("Schema: {schema_version}"),
                    format!(
                        "Health: {}",
                        if *healthy {
                            "healthy"
                        } else {
                            "issues detected"
                        }
                    ),
                    format!(
                        "SQLite: wal={} foreign_keys={} busy_timeout_ms={}",
                        wal_enabled, foreign_keys_enabled, busy_timeout_ms
                    ),
                    format!(
                        "Entities: projects={} tasks={} habits={} routines={} knowledge={}",
                        projects, tasks, habits, routines, knowledge_items
                    ),
                ];
                for issue in issues {
                    lines.push(format!("Issue [{}]: {}", issue.area, issue.detail));
                }
                lines.join("\n")
            }
            Self::Today { tasks, .. } => {
                if tasks.is_empty() {
                    "Today: no planned, ready, or in-progress tasks.".to_owned()
                } else {
                    format!("Today\n{}", render_tasks(tasks))
                }
            }
        }
    }
}

fn render_tasks(tasks: &[TaskView]) -> String {
    if tasks.is_empty() {
        return "No tasks.".to_owned();
    }
    tasks
        .iter()
        .map(|task| {
            let scope = match (&task.project_id, &task.milestone_id) {
                (Some(project), Some(milestone)) => {
                    format!(" project={project} milestone={milestone}")
                }
                (Some(project), None) => format!(" project={project}"),
                _ => String::new(),
            };
            format!("{}  {:<12}  {}{}", task.id, task.status, task.title, scope)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[derive(Debug, Serialize)]
struct WorkspaceIssueView {
    area: String,
    detail: String,
}

#[derive(Debug, Serialize)]
struct TaskView {
    id: String,
    title: String,
    status: String,
    project_id: Option<String>,
    milestone_id: Option<String>,
    estimated_minutes: Option<u32>,
    earliest_start_ms: Option<i64>,
    deadline_ms: Option<i64>,
}

impl From<&Task> for TaskView {
    fn from(task: &Task) -> Self {
        Self {
            id: task.id().to_string(),
            title: task.title().to_owned(),
            status: task.status().as_str().to_owned(),
            project_id: task.scope().project_id().map(ToString::to_string),
            milestone_id: task.scope().milestone_id().map(ToString::to_string),
            estimated_minutes: task.planning().estimated_minutes(),
            earliest_start_ms: task.planning().earliest_start_at_ms(),
            deadline_ms: task.planning().deadline_at_ms(),
        }
    }
}

#[derive(Debug, Serialize)]
struct MilestoneHealthView {
    id: String,
    progress_bp: Option<u16>,
    included_tasks: u32,
}

#[derive(Debug, Serialize)]
struct ProjectHealthView {
    project_id: String,
    mode: String,
    tracked_signals: Vec<String>,
    completion_ready: bool,
    required_criteria_total: u32,
    required_criteria_satisfied: u32,
    state: &'static str,
    reasons: Vec<&'static str>,
    progress_bp: u16,
    mode_progress_bp: Option<u16>,
    evidence_progress_bp: Option<u16>,
    available_signals: Vec<String>,
    unavailable_signals: Vec<String>,
    blocked_tasks: u32,
    blocked_milestones: u32,
    recent_activity_count: u32,
    last_activity_age_minutes: Option<u64>,
    status_derived_progress: bool,
    milestones: Vec<MilestoneHealthView>,
}

impl From<&ProjectHealthSnapshot> for ProjectHealthView {
    fn from(snapshot: &ProjectHealthSnapshot) -> Self {
        Self {
            project_id: snapshot.project_id.to_string(),
            mode: snapshot.mode.as_str().to_owned(),
            tracked_signals: snapshot
                .tracked_signals
                .iter()
                .map(|signal| signal.as_str().to_owned())
                .collect(),
            completion_ready: snapshot.completion_ready,
            required_criteria_total: snapshot.required_criteria_total,
            required_criteria_satisfied: snapshot.required_criteria_satisfied,
            state: match snapshot.health.state {
                ProjectHealthState::OnTrack => "on_track",
                ProjectHealthState::NeedsAttention => "needs_attention",
                ProjectHealthState::AtRisk => "at_risk",
            },
            reasons: snapshot
                .health
                .reasons
                .iter()
                .map(|reason| match reason {
                    ProjectHealthReason::BehindPlan => "behind_plan",
                    ProjectHealthReason::ProjectedLate => "projected_late",
                    ProjectHealthReason::OpenBlockers => "open_blockers",
                    ProjectHealthReason::BlockedEntities => "blocked_entities",
                    ProjectHealthReason::EffortOverrun => "effort_overrun",
                    ProjectHealthReason::NoRecentActivity => "no_recent_activity",
                    ProjectHealthReason::ProgressStalled => "progress_stalled",
                })
                .collect(),
            progress_bp: snapshot.rollup.progress.value(),
            mode_progress_bp: None,
            evidence_progress_bp: None,
            available_signals: Vec::new(),
            unavailable_signals: Vec::new(),
            blocked_tasks: snapshot.evidence.blocker_exposure.blocked_tasks,
            blocked_milestones: snapshot.evidence.blocker_exposure.blocked_milestones,
            recent_activity_count: snapshot.recent_activity.activity_count,
            last_activity_age_minutes: snapshot.recent_activity.last_activity_age_minutes,
            status_derived_progress: snapshot.status_derived_progress,
            milestones: snapshot
                .milestones
                .iter()
                .map(|milestone| MilestoneHealthView {
                    id: milestone.milestone_id.to_string(),
                    progress_bp: milestone.progress.map(|value| value.value()),
                    included_tasks: milestone.included_tasks,
                })
                .collect(),
        }
    }
}

#[derive(Debug, Serialize)]
struct ProjectCriterionView {
    key: String,
    description: String,
    required: bool,
    status: String,
}

#[derive(Debug, Serialize)]
struct ProjectView {
    id: String,
    name: String,
    status: String,
    mode: String,
    objective: Option<String>,
    progress_signals: Vec<String>,
    success_criteria: Vec<ProjectCriterionView>,
    completion_ready: bool,
}

impl From<&orbit_domain::Project> for ProjectView {
    fn from(project: &orbit_domain::Project) -> Self {
        Self {
            id: project.id().to_string(),
            name: project.name().to_owned(),
            status: project.status().as_str().to_owned(),
            mode: project.mode().as_str().to_owned(),
            objective: project.objective().map(ToOwned::to_owned),
            progress_signals: project
                .progress_signals()
                .iter()
                .map(|signal| signal.as_str().to_owned())
                .collect(),
            success_criteria: project
                .success_criteria()
                .iter()
                .map(|criterion| ProjectCriterionView {
                    key: criterion.key().to_owned(),
                    description: criterion.description().to_owned(),
                    required: criterion.required(),
                    status: criterion.status().as_str().to_owned(),
                })
                .collect(),
            completion_ready: project.required_completion_ready(),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(tag = "kind", content = "item", rename_all = "snake_case")]
enum SearchResultView {
    Project(ProjectView),
    Task(TaskView),
    Habit(HabitView),
    Routine(RoutineView),
    Markdown(orbit_runtime::knowledge_service::KnowledgeDocumentInfo),
    ScheduleBlock(ScheduleBlockView),
}

impl SearchResultView {
    fn sort_key(&self) -> (&'static str, &str) {
        match self {
            Self::Project(item) => ("project", &item.id),
            Self::Task(item) => ("task", &item.id),
            Self::Habit(item) => ("habit", &item.id),
            Self::Routine(item) => ("routine", &item.id),
            Self::Markdown(item) => ("knowledge", &item.id),
            Self::ScheduleBlock(item) => ("schedule_block", &item.id),
        }
    }

    fn human(&self) -> String {
        match self {
            Self::Project(item) => format!(
                "project        {}  {}  [{}]",
                item.id, item.name, item.status
            ),
            Self::Task(item) => format!(
                "task           {}  {}  [{}]",
                item.id, item.title, item.status
            ),
            Self::Habit(item) => format!(
                "habit          {}  {}  [{}]",
                item.id, item.name, item.state
            ),
            Self::Routine(item) => format!(
                "routine        {}  {}  [{} habits]",
                item.id,
                item.name,
                item.habit_ids.len()
            ),
            Self::Markdown(item) => format!(
                "knowledge      {}  {}  [{}]",
                item.id, item.title, item.path
            ),
            Self::ScheduleBlock(item) => format!(
                "schedule_block {}  {}  [{}]",
                item.id, item.title, item.flexibility
            ),
        }
    }
}

#[derive(Debug, Serialize)]
struct HabitOccurrenceView {
    id: String,
    habit_id: String,
    occurred_at_ms: i64,
    duration_minutes: Option<u32>,
}

impl From<&HabitOccurrence> for HabitOccurrenceView {
    fn from(occurrence: &HabitOccurrence) -> Self {
        Self {
            id: occurrence.id().to_string(),
            habit_id: occurrence.habit_id().to_string(),
            occurred_at_ms: occurrence.occurred_at_ms(),
            duration_minutes: occurrence.duration_minutes(),
        }
    }
}

#[derive(Debug, Serialize)]
struct HabitView {
    id: String,
    name: String,
    cadence: String,
    state: &'static str,
}

impl From<&Habit> for HabitView {
    fn from(habit: &Habit) -> Self {
        let cadence = match habit.cadence() {
            HabitCadence::Daily => "daily".to_owned(),
            HabitCadence::Weekly { target_per_week } => format!("{target_per_week}/week"),
        };
        Self {
            id: habit.id().to_string(),
            name: habit.name().to_owned(),
            cadence,
            state: if habit.is_active() {
                "active"
            } else {
                "paused"
            },
        }
    }
}

#[derive(Debug, Serialize)]
struct RoutineView {
    id: String,
    name: String,
    habit_ids: Vec<String>,
}

impl From<&Routine> for RoutineView {
    fn from(routine: &Routine) -> Self {
        Self {
            id: routine.id().to_string(),
            name: routine.name().to_owned(),
            habit_ids: routine
                .habit_ids()
                .iter()
                .map(ToString::to_string)
                .collect(),
        }
    }
}

#[derive(Debug, Serialize)]
struct ScheduleBlockView {
    id: String,
    title: String,
    starts_at_ms: i64,
    ends_at_ms: i64,
    duration_minutes: u32,
    flexibility: &'static str,
    target: String,
}

impl From<&ScheduleBlock> for ScheduleBlockView {
    fn from(block: &ScheduleBlock) -> Self {
        let flexibility = match block.flexibility() {
            ScheduleFlexibility::Fixed => "fixed",
            ScheduleFlexibility::Movable => "movable",
        };
        let target = match block.target() {
            ScheduleTarget::Standalone => "standalone".to_owned(),
            ScheduleTarget::Task(id) => format!("task:{id}"),
            ScheduleTarget::Habit(id) => format!("habit:{id}"),
        };
        Self {
            id: block.id().to_string(),
            title: block.title().to_owned(),
            starts_at_ms: block.starts_at_ms(),
            ends_at_ms: block.ends_at_ms(),
            duration_minutes: block.duration_minutes(),
            flexibility,
            target,
        }
    }
}

#[derive(Debug)]
pub enum CliError {
    Usage(String),
    Io(String),
    Domain(String),
    Application(String),
    Repository(String),
    Sync(String),
    Serialization(String),
    /// An already-rendered diagnostic report that must exit non-zero without
    /// being wrapped in the usual `orbit:` error prefix.
    Diagnostics(String),
}

impl CliError {
    fn domain(error: impl std::fmt::Debug) -> Self {
        Self::Domain(format!("{error:?}"))
    }
    fn application(error: impl std::fmt::Debug) -> Self {
        Self::Application(format!("{error:?}"))
    }
    fn repository(error: impl std::fmt::Debug) -> Self {
        Self::Repository(format!("{error:?}"))
    }
    fn sync(error: impl std::fmt::Debug) -> Self {
        Self::Sync(format!("{error:?}"))
    }
}

pub fn render_error_json(error: &CliError) -> String {
    #[derive(Serialize)]
    struct ErrorResponse<'a> {
        r#type: &'static str,
        code: &'static str,
        message: String,
        recoverable: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        hint: Option<&'a str>,
    }

    let response = ErrorResponse {
        r#type: "error",
        code: error.code(),
        message: error.to_string(),
        recoverable: !matches!(error, CliError::Serialization(_)),
        hint: error.hint(),
    };
    serde_json::to_string_pretty(&response).unwrap_or_else(|_| {
        "{\"type\":\"error\",\"code\":\"serialization_error\",\"message\":\"failed to serialize CLI error\",\"recoverable\":false}".to_owned()
    })
}

impl CliError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Usage(_) => "usage_error",
            Self::Io(_) => "io_error",
            Self::Domain(_) => "domain_error",
            Self::Application(_) => "application_error",
            Self::Repository(_) => "repository_error",
            Self::Sync(_) => "sync_error",
            Self::Serialization(_) => "serialization_error",
            Self::Diagnostics(_) => "diagnostics_failed",
        }
    }

    pub fn exit_code(&self) -> i32 {
        match self {
            Self::Usage(_) | Self::Domain(_) => 2,
            Self::Application(_) => 3,
            Self::Io(_) | Self::Repository(_) => 4,
            Self::Sync(_) => 5,
            Self::Serialization(_) => 70,
            Self::Diagnostics(_) => 1,
        }
    }

    pub fn hint(&self) -> Option<&'static str> {
        match self {
            Self::Usage(_) => {
                Some("Run `orbit --help` or the subcommand help for valid arguments.")
            }
            Self::Repository(_) => {
                Some("Check the workspace path and run storage diagnostics when available.")
            }
            Self::Sync(_) => Some("Inspect `orbit sync replica status` and pending commands."),
            _ => None,
        }
    }
}

impl std::fmt::Display for CliError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Usage(message)
            | Self::Io(message)
            | Self::Domain(message)
            | Self::Application(message)
            | Self::Repository(message)
            | Self::Sync(message)
            | Self::Serialization(message)
            | Self::Diagnostics(message) => formatter.write_str(message),
        }
    }
}

impl std::error::Error for CliError {}

fn generate_id(prefix: &str) -> String {
    let sequence = ID_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    format!("{prefix}-{}-{}-{sequence}", now_ms(), process::id())
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(i64::MAX as u128) as i64
}

fn ensure_parent(path: &Path) -> Result<(), CliError> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent).map_err(|error| CliError::Io(error.to_string()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schedule_approval_cli_requires_double_confirmation() {
        let mut host = orbit_runtime::AuthoritativeHost::open(":memory:").unwrap();
        let unavailable = std::env::temp_dir().join(format!(
            "orbit-missing-approved-plan-{}-{}.json",
            std::process::id(),
            now_ms()
        ));
        // No approval file is even read until --confirm is asserted.
        let result = run_task(
            &mut host,
            TaskSubcommand::ApplyReviewedSchedule(TaskApplyReviewedScheduleArgs {
                input: unavailable,
                confirm: false,
            }),
        );
        assert!(matches!(result, Err(CliError::Usage(_))));
        assert_eq!(host.snapshot().unwrap().0, 0);
    }

    #[test]
    fn schedule_review_cli_never_creates_calendar_blocks() {
        use orbit_runtime::domain_commands::{TaskAction, TaskCommand as DomainTaskCommand};
        let mut host = orbit_runtime::AuthoritativeHost::open(":memory:").unwrap();
        host.task_command(
            "orbit-cli",
            DomainTaskCommand {
                command_id: "create-cli-task".into(),
                expected_revision: 0,
                action: TaskAction::Create {
                    task_id: "cli-approved-task".into(),
                    title: "Review the proposed action".into(),
                    project_id: None,
                },
            },
        )
        .unwrap();
        host.task_command(
            "orbit-cli",
            DomainTaskCommand {
                command_id: "estimate-cli-task".into(),
                expected_revision: 1,
                action: TaskAction::SetPlanning {
                    task_id: "cli-approved-task".into(),
                    estimated_minutes: Some(30),
                    earliest_start_at_ms: None,
                    deadline_at_ms: None,
                },
            },
        )
        .unwrap();
        host.task_command(
            "orbit-cli",
            DomainTaskCommand {
                command_id: "ready-cli-task".into(),
                expected_revision: 2,
                action: TaskAction::ChangeStatus {
                    task_id: "cli-approved-task".into(),
                    status: "ready".into(),
                },
            },
        )
        .unwrap();
        let input = std::env::temp_dir().join(format!(
            "orbit-review-cli-{}-{}.json",
            std::process::id(),
            now_ms()
        ));
        let request = orbit_runtime::planning_approval::ScheduleApprovalRequest {
            command_id: "plan-cli-task".into(),
            expected_revision: 3,
            window_start_ms: 0,
            window_end_ms: 7_200_000,
            items: vec![orbit_runtime::planning_approval::ScheduleApprovalItem {
                task_id: "cli-approved-task".into(),
                starts_at_ms: 0,
                ends_at_ms: 1_800_000,
                schedule_block_id: "cli-block".into(),
                activity_id: "cli-activity".into(),
            }],
        };
        std::fs::write(&input, serde_json::to_vec(&request).unwrap()).unwrap();
        let result = run_task(
            &mut host,
            TaskSubcommand::ReviewSchedule(TaskReviewScheduleArgs {
                input: input.clone(),
            }),
        )
        .unwrap();
        let Response::Authority {
            result: review_value,
        } = result
        else {
            panic!("expected review")
        };
        let review: orbit_runtime::planning_approval::PlanReview =
            serde_json::from_value(review_value).unwrap();
        assert_eq!(review.scheduled_slots[0].duration_minutes, 30);
        assert_eq!(host.snapshot().unwrap().0, 3);
        assert!(
            host.application_queries()
                .schedule_between(0, 7_200_000)
                .unwrap()
                .is_empty()
        );
        let approved_file = std::env::temp_dir().join(format!(
            "orbit-approved-cli-{}-{}.json",
            std::process::id(),
            now_ms()
        ));
        let approved = orbit_runtime::planning_approval::ApprovedScheduleRequest {
            plan: request,
            approval: orbit_runtime::planning_approval::PlanApproval {
                review,
                approved: true,
            },
        };
        std::fs::write(&approved_file, serde_json::to_vec(&approved).unwrap()).unwrap();
        let result = run_task(
            &mut host,
            TaskSubcommand::ApplyReviewedSchedule(TaskApplyReviewedScheduleArgs {
                input: approved_file.clone(),
                confirm: true,
            }),
        )
        .unwrap();
        let Response::Authority {
            result: receipt_value,
        } = result
        else {
            panic!("expected receipt")
        };
        let receipt: orbit_runtime::planning_approval::ApprovedScheduleReceipt =
            serde_json::from_value(receipt_value).unwrap();
        assert_eq!(receipt.schedule_block_ids, vec!["cli-block"]);
        assert_eq!(host.snapshot().unwrap().0, 4);
        assert_eq!(
            host.application_queries()
                .schedule_between(0, 7_200_000)
                .unwrap()
                .len(),
            1
        );
        let retried = run_task(
            &mut host,
            TaskSubcommand::ApplyReviewedSchedule(TaskApplyReviewedScheduleArgs {
                input: approved_file.clone(),
                confirm: true,
            }),
        )
        .unwrap();
        let Response::Authority {
            result: retry_value,
        } = retried
        else {
            panic!("expected retry")
        };
        assert!(retry_value["replayed"].as_bool().unwrap());
        assert_eq!(host.snapshot().unwrap().0, 4);
        let _ = std::fs::remove_file(input);
        let _ = std::fs::remove_file(approved_file);
    }

    #[test]
    fn schedule_approval_cli_rejects_incomplete_approval_document() {
        let mut host = orbit_runtime::AuthoritativeHost::open(":memory:").unwrap();
        let input = std::env::temp_dir().join(format!(
            "orbit-bad-approval-cli-{}-{}.json",
            std::process::id(),
            now_ms()
        ));
        std::fs::write(&input, r#"{"approved":true}"#).unwrap();
        let result = run_task(
            &mut host,
            TaskSubcommand::ApplyReviewedSchedule(TaskApplyReviewedScheduleArgs {
                input: input.clone(),
                confirm: true,
            }),
        );
        assert!(matches!(result, Err(CliError::Serialization(_))));
        assert_eq!(host.snapshot().unwrap().0, 0);
        let _ = std::fs::remove_file(input);
    }

    #[test]
    fn milestone_requires_project_at_cli_boundary() {
        let result = task_scope(None, Some("m-1".to_owned()));
        assert!(matches!(result, Err(CliError::Usage(_))));
    }

    #[test]
    fn dependency_preview_cli_has_explicit_time_and_optional_remove_switch() {
        use clap::Parser;
        let parsed = Cli::try_parse_from([
            "orbit",
            "task",
            "preview-dependency",
            "dependent",
            "prerequisite",
            "--now-ms",
            "1000",
        ])
        .unwrap();
        match parsed.command {
            Command::Task(TaskCommand {
                command: TaskSubcommand::PreviewDependency(args),
            }) => {
                assert_eq!(args.id, "dependent");
                assert_eq!(args.prerequisite, "prerequisite");
                assert_eq!(args.now_ms, 1000);
                assert!(!args.remove);
            }
            _ => panic!("preview command resolved to unexpected CLI branch"),
        }
        let parsed = Cli::try_parse_from([
            "orbit",
            "task",
            "preview-dependency",
            "dependent",
            "prerequisite",
            "--now-ms",
            "1000",
            "--remove",
        ])
        .unwrap();
        match parsed.command {
            Command::Task(TaskCommand {
                command: TaskSubcommand::PreviewDependency(args),
            }) => assert!(args.remove),
            _ => panic!("remove preview did not resolve"),
        }
    }

    #[test]
    fn dependency_preview_cli_requires_explicit_evaluation_time() {
        use clap::Parser;
        assert!(
            Cli::try_parse_from([
                "orbit",
                "task",
                "preview-dependency",
                "dependent",
                "prerequisite"
            ])
            .is_err()
        );
        assert!(Cli::try_parse_from(["orbit", "task", "preview-dependency", "dependent"]).is_err());
    }

    #[test]
    fn dependency_preview_cli_rejects_cycles_without_committing() {
        let mut host = orbit_runtime::AuthoritativeHost::open(":memory:").unwrap();
        for id in ["a", "b"] {
            let revision = host.snapshot().unwrap().0;
            host.task_command(
                "setup",
                orbit_runtime::domain_commands::TaskCommand {
                    command_id: format!("create-{id}"),
                    expected_revision: revision,
                    action: orbit_runtime::domain_commands::TaskAction::Create {
                        task_id: id.into(),
                        title: id.into(),
                        project_id: None,
                    },
                },
            )
            .unwrap();
        }
        let before = host.snapshot().unwrap().0;
        let result = run_task(
            &mut host,
            TaskSubcommand::PreviewDependency(TaskPreviewDependencyArgs {
                id: "b".into(),
                prerequisite: "a".into(),
                remove: false,
                now_ms: 100,
            }),
        )
        .unwrap();
        let encoded = serde_json::to_value(result).unwrap();
        assert_eq!(encoded["result"]["before_edge_count"], 0);
        assert_eq!(encoded["result"]["after_edge_count"], 1);
        assert_eq!(host.snapshot().unwrap().0, before);
        assert!(host.task_dependency_edges().unwrap().is_empty());
    }

    #[test]
    fn task_json_contract_uses_stable_fields() {
        let mut task = Task::new(
            TaskId::new("task-1").expect("id"),
            "Renew passport",
            TaskScope::standalone(),
        )
        .expect("task");
        task.set_planning(TaskPlanning::new(Some(45), Some(100), Some(500)).expect("planning"));
        let response = Response::Task {
            task: TaskView::from(&task),
            action: "created",
        };
        let json = serde_json::to_value(response).expect("serialize response");
        assert_eq!(json["type"], "task");
        assert_eq!(json["action"], "created");
        assert_eq!(json["task"]["id"], "task-1");
        assert_eq!(json["task"]["status"], "inbox");
        assert_eq!(json["task"]["estimated_minutes"], 45);
        assert_eq!(json["task"]["earliest_start_ms"], 100);
        assert_eq!(json["task"]["deadline_ms"], 500);
    }

    #[test]
    fn habit_json_contract_exposes_cadence_and_active_state() {
        let habit = Habit::new(
            HabitId::new("habit-1").expect("id"),
            "Run",
            HabitCadence::Weekly { target_per_week: 3 },
        )
        .expect("habit");
        let response = Response::Habit {
            habit: HabitView::from(&habit),
            action: "created",
        };
        let json = serde_json::to_value(response).expect("serialize response");
        assert_eq!(json["habit"]["cadence"], "3/week");
        assert_eq!(json["habit"]["state"], "active");
    }

    #[test]
    fn schedule_json_contract_exposes_fixed_vs_movable() {
        let block = ScheduleBlock::new(
            ScheduleBlockId::new("schedule-1").expect("id"),
            "Appointment",
            1_000,
            30,
            ScheduleFlexibility::Fixed,
            ScheduleTarget::Standalone,
        )
        .expect("block");
        let response = Response::ScheduleBlock {
            block: ScheduleBlockView::from(&block),
            action: "created",
        };
        let json = serde_json::to_value(response).expect("serialize response");
        assert_eq!(json["block"]["flexibility"], "fixed");
        assert_eq!(json["block"]["target"], "standalone");
    }

    #[test]
    fn schedule_parser_rejects_task_and_habit_target_together() {
        let parsed = Cli::try_parse_from([
            "orbit",
            "schedule",
            "add",
            "Focus",
            "--start-ms",
            "1000",
            "--duration-minutes",
            "30",
            "--task",
            "task-1",
            "--habit",
            "habit-1",
        ]);
        assert!(parsed.is_err());
    }

    #[test]
    fn today_human_output_is_explicit_when_empty() {
        let response = Response::Today {
            tasks: vec![],
            provisional: true,
        };
        assert_eq!(
            response.human(),
            "Today: no planned, ready, or in-progress tasks."
        );
    }

    #[test]
    fn parser_accepts_global_json_after_subcommand() {
        let cli = Cli::try_parse_from(["orbit", "task", "list", "--json"]).expect("valid CLI");
        assert!(cli.json);
        assert!(matches!(cli.command, Command::Task(_)));
    }

    #[test]
    fn json_error_contract_has_stable_type_and_code() {
        let value: serde_json::Value =
            serde_json::from_str(&render_error_json(&CliError::Usage("bad input".to_owned())))
                .expect("valid JSON");
        assert_eq!(value["type"], "error");
        assert_eq!(value["code"], "usage_error");
        assert_eq!(value["recoverable"], true);
    }

    #[test]
    fn parser_accepts_arbitrary_task_status_transition_command() {
        let cli = Cli::try_parse_from(["orbit", "task", "status", "task-1", "blocked"])
            .expect("valid CLI");
        assert!(matches!(cli.command, Command::Task(_)));
    }

    #[test]
    fn parser_accepts_project_status_transition_command() {
        let cli = Cli::try_parse_from(["orbit", "project", "status", "project-1", "on-hold"])
            .expect("valid CLI");
        assert!(matches!(cli.command, Command::Project(_)));
    }

    #[test]
    fn parser_accepts_project_health_command() {
        let cli =
            Cli::try_parse_from(["orbit", "project", "health", "project-1"]).expect("valid CLI");
        assert!(matches!(cli.command, Command::Project(_)));
    }

    #[test]
    fn parser_accepts_list_filters() {
        let cli = Cli::try_parse_from([
            "orbit",
            "task",
            "list",
            "--status",
            "ready",
            "--project",
            "project-1",
            "--query",
            "passport",
        ])
        .expect("valid CLI");
        assert!(matches!(cli.command, Command::Task(_)));
    }

    #[test]
    fn search_json_contract_is_typed_and_stable() {
        let response = Response::Search {
            query: "orbit".to_owned(),
            results: vec![SearchResultView::Project(ProjectView {
                id: "project-1".to_owned(),
                name: "Orbit Core".to_owned(),
                status: "active".to_owned(),
                mode: "target".to_owned(),
                objective: None,
                progress_signals: vec![],
                success_criteria: vec![],
                completion_ready: false,
            })],
        };
        let json = serde_json::to_value(response).expect("serialize response");
        assert_eq!(json["type"], "search");
        assert_eq!(json["results"][0]["kind"], "project");
        assert_eq!(json["results"][0]["item"]["id"], "project-1");
    }

    #[test]
    fn exit_code_contract_is_stable() {
        assert_eq!(CliError::Usage("bad".to_owned()).exit_code(), 2);
        assert_eq!(CliError::Domain("bad".to_owned()).exit_code(), 2);
        assert_eq!(CliError::Application("bad".to_owned()).exit_code(), 3);
        assert_eq!(CliError::Repository("bad".to_owned()).exit_code(), 4);
        assert_eq!(CliError::Io("bad".to_owned()).exit_code(), 4);
        assert_eq!(CliError::Serialization("bad".to_owned()).exit_code(), 70);
    }

    #[test]
    fn status_json_contract_exposes_workspace_diagnostics() {
        let response = Response::WorkspaceStatus {
            database: "/tmp/orbit.db".to_owned(),
            schema_version: 1,
            healthy: true,
            issues: vec![],
            wal_enabled: true,
            foreign_keys_enabled: true,
            busy_timeout_ms: 5_000,
            projects: 1,
            tasks: 2,
            habits: 3,
            routines: 1,
            knowledge_items: 0,
        };
        let json = serde_json::to_value(response).expect("serialize response");
        assert_eq!(json["type"], "workspace_status");
        assert_eq!(json["healthy"], true);
        assert_eq!(json["busy_timeout_ms"], 5_000);
        assert_eq!(json["tasks"], 2);
    }

    #[test]
    fn source_level_acceptance_create_list_transition_search_json() {
        let root = std::env::temp_dir().join(generate_id("orbit-cli-acceptance"));
        fs::create_dir_all(&root).expect("temp workspace");
        let db = root.join("orbit.db");
        let db_text = db.to_string_lossy().into_owned();

        let add = Cli::try_parse_from([
            "orbit",
            "--db",
            db_text.as_str(),
            "--json",
            "task",
            "add",
            "Acceptance task",
        ])
        .expect("parse add");
        let created: serde_json::Value =
            serde_json::from_str(&run(add).expect("add task")).expect("created json");
        let task_id = created["task"]["id"].as_str().expect("task id").to_owned();
        assert_eq!(created["task"]["status"], "inbox");

        let list = Cli::try_parse_from([
            "orbit",
            "--db",
            db_text.as_str(),
            "--json",
            "task",
            "list",
            "--query",
            "Acceptance",
        ])
        .expect("parse list");
        let listed: serde_json::Value =
            serde_json::from_str(&run(list).expect("list tasks")).expect("list json");
        assert_eq!(listed["tasks"].as_array().expect("tasks").len(), 1);

        let start = Cli::try_parse_from([
            "orbit",
            "--db",
            db_text.as_str(),
            "--json",
            "task",
            "start",
            task_id.as_str(),
        ])
        .expect("parse start");
        let started: serde_json::Value =
            serde_json::from_str(&run(start).expect("start task")).expect("start json");
        assert_eq!(started["task"]["status"], "in_progress");

        let search = Cli::try_parse_from([
            "orbit",
            "--db",
            db_text.as_str(),
            "--json",
            "search",
            "Acceptance",
        ])
        .expect("parse search");
        let searched: serde_json::Value =
            serde_json::from_str(&run(search).expect("search")).expect("search json");
        assert_eq!(searched["results"].as_array().expect("results").len(), 1);
        assert_eq!(searched["results"][0]["kind"], "task");

        let status = Cli::try_parse_from(["orbit", "--db", db_text.as_str(), "--json", "status"])
            .expect("parse status");
        let status_json: serde_json::Value =
            serde_json::from_str(&run(status).expect("status")).expect("status json");
        assert_eq!(status_json["healthy"], true);
        assert_eq!(status_json["tasks"], 1);

        drop(status_json);
        let _ = fs::remove_file(&db);
        let _ = fs::remove_file(db.with_extension("db-wal"));
        let _ = fs::remove_file(db.with_extension("db-shm"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn parser_rejects_removed_legacy_sync_commands() {
        for action in ["init", "status", "pull", "push", "http"] {
            assert!(Cli::try_parse_from(["orbit", "sync", action]).is_err());
        }
        assert!(Cli::try_parse_from(["orbit", "sync", "replica", "--help"]).is_err());
    }
}

/// Resolve a worktree by its exact persisted launch-derived branch, not by
/// guessing a path or reading the main checkout. Remote repos fail closed in
/// the authority API; evidence must be collected by a trusted remote executor.
fn git_evidence_for_job(
    repo: &str,
    branch_ref: Option<&str>,
    max_commits: usize,
) -> Result<serde_json::Value, CliError> {
    let Some(branch_ref) = branch_ref else {
        return git_evidence(repo, max_commits);
    };
    let root = Path::new(repo);
    if !root.is_absolute() || !root.is_dir() {
        return Err(CliError::Usage(
            "repository must be a local absolute directory".into(),
        ));
    }
    let output = process::Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["worktree", "list", "--porcelain"])
        .output()
        .map_err(|e| CliError::Usage(format!("cannot inspect Git worktrees: {e}")))?;
    if !output.status.success() {
        return Err(CliError::Usage("Git worktree discovery failed".into()));
    }
    let listing = String::from_utf8(output.stdout)
        .map_err(|_| CliError::Usage("invalid UTF-8 from Git worktree listing".into()))?;
    let mut path = None::<String>;
    let mut matches = Vec::<String>::new();
    for line in listing.lines().chain(std::iter::once("")) {
        if line.is_empty() {
            path = None;
        } else if let Some(p) = line.strip_prefix("worktree ") {
            path = Some(p.to_string());
        } else if line.strip_prefix("branch ") == Some(branch_ref)
            && let Some(p) = &path
        {
            matches.push(p.clone());
        }
    }
    if matches.len() != 1 {
        return Err(CliError::Usage(format!(
            "expected one worktree for {branch_ref}, found {}",
            matches.len()
        )));
    }
    let target = Path::new(&matches[0])
        .canonicalize()
        .map_err(|e| CliError::Usage(format!("worktree unavailable: {e}")))?;
    let mut evidence = git_evidence(&target.to_string_lossy(), max_commits)?;
    if evidence["branch"].as_str() != branch_ref.strip_prefix("refs/heads/") {
        return Err(CliError::Usage(
            "worktree branch changed during evidence collection".into(),
        ));
    }
    evidence["scope"] =
        serde_json::json!("isolated worktree snapshot, agent attribution not proven");
    evidence["worktree_path"] = serde_json::json!(target.to_string_lossy().to_string());
    Ok(evidence)
}

/// Read-only bounded Git inspection. No shell, no checkout, no mutable Git operations.
fn git_evidence(repo: &str, max_commits: usize) -> Result<serde_json::Value, CliError> {
    use std::process::Command;
    let path = Path::new(repo);
    if !path.is_absolute() || !path.is_dir() {
        return Err(CliError::Usage(
            "repository is not a local absolute directory".into(),
        ));
    }
    let git = |args: &[&str]| -> Result<String, CliError> {
        let output = Command::new("git")
            .arg("-C")
            .arg(path)
            .args(args)
            .output()
            .map_err(|e| CliError::Usage(format!("Git inspection unavailable: {e}")))?;
        if !output.status.success() {
            return Err(CliError::Usage(format!(
                "Git inspection failed: {}",
                String::from_utf8_lossy(&output.stderr)
            )));
        }
        Ok(String::from_utf8_lossy(&output.stdout)
            .trim_end()
            .to_string())
    };
    if git(&["rev-parse", "--show-toplevel"])?
        != path
            .canonicalize()
            .map_err(|e| CliError::Usage(e.to_string()))?
            .to_string_lossy()
    {
        return Err(CliError::Usage(
            "repository must point to Git worktree root".into(),
        ));
    }
    let head = git(&["rev-parse", "HEAD"])?;
    let branch = git(&["branch", "--show-current"])?;
    let status = git(&["status", "--porcelain=v1", "--untracked-files=no"])?;
    let limit = max_commits.to_string();
    let commits = git(&["log", "--format=%H", "-n", &limit])?;
    Ok(serde_json::json!({"head":head,"branch":branch,
        "modified_tracked_files":status.lines().take(300).collect::<Vec<_>>(),
        "recent_commits":commits.lines().collect::<Vec<_>>(),
        "scope":"repository snapshot, not necessarily attributable to this agent"}))
}

#[cfg(test)]
mod agent_worktree_evidence_tests {
    use super::*;
    #[test]
    fn git_evidence_is_a_bounded_read_only_snapshot() {
        if process::Command::new("git")
            .arg("--version")
            .output()
            .is_err()
        {
            eprintln!("git unavailable; repository evidence acceptance not executed");
            return;
        }
        let directory = tempfile::tempdir().unwrap();
        let repo = directory.path().canonicalize().unwrap();
        let run = |args: &[&str]| {
            let output = process::Command::new("git")
                .arg("-C")
                .arg(&repo)
                .args(args)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
        };
        run(&["init", "-q"]);
        run(&["config", "user.email", "evidence@example.invalid"]);
        run(&["config", "user.name", "Evidence Test"]);
        fs::write(repo.join("one.txt"), "one").unwrap();
        run(&["add", "one.txt"]);
        run(&["commit", "-q", "-m", "one"]);
        fs::write(repo.join("two.txt"), "two").unwrap();
        run(&["add", "two.txt"]);
        run(&["commit", "-q", "-m", "two"]);
        let evidence = git_evidence(repo.to_str().unwrap(), 1).unwrap();
        assert_eq!(evidence["recent_commits"].as_array().unwrap().len(), 1);
        assert!(
            evidence["head"]
                .as_str()
                .is_some_and(|head| head.len() >= 40)
        );
        assert!(
            evidence["modified_tracked_files"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        assert!(evidence["scope"].as_str().is_some());
    }

    #[test]
    fn missing_branch_is_not_silently_replaced_with_main_checkout() {
        let repo = std::env::temp_dir();
        let result =
            git_evidence_for_job(&repo.to_string_lossy(), Some("refs/heads/orbit/missing"), 5);
        assert!(result.is_err());
    }
    #[test]
    fn missing_repository_does_not_capture_evidence() {
        let result = git_evidence_for_job("/nonexistent-orbit-evidence-repository", None, 5);
        assert!(result.is_err());
    }
}
