//! Internal SQLite persistence implementation; not a public API boundary.
use super::*;

pub(super) fn task_from_row(row: &Row<'_>) -> rusqlite::Result<Task> {
    let id: String = row.get(0)?;
    let title: String = row.get(1)?;
    let status: String = row.get(2)?;
    let project_id: Option<String> = row.get(3)?;
    let milestone_id: Option<String> = row.get(4)?;
    let estimated_minutes_raw: Option<i64> = row.get(5)?;
    let earliest_start_at_ms: Option<i64> = row.get(6)?;
    let deadline_at_ms: Option<i64> = row.get(7)?;
    let description: Option<String> = row.get(8)?;
    let priority: Option<String> = row.get(9)?;

    let project_id = project_id
        .map(project_id_from_string)
        .transpose()
        .map_err(to_sql_error)?;
    let milestone_id = milestone_id
        .map(milestone_id_from_string)
        .transpose()
        .map_err(to_sql_error)?;
    let scope = TaskScope::try_new(project_id, milestone_id).map_err(to_sql_error)?;
    let mut task = Task::new(task_id_from_string(id).map_err(to_sql_error)?, title, scope)
        .map_err(to_sql_error)?;
    let estimated_minutes = estimated_minutes_raw
        .map(|value| u32::try_from(value).map_err(|_| to_sql_error("invalid task estimate")))
        .transpose()?;
    let planning = TaskPlanning::new(estimated_minutes, earliest_start_at_ms, deadline_at_ms)
        .map_err(to_sql_error)?;
    task.set_planning(planning);
    let mut details = orbit_domain::TaskDetails::default();
    details
        .set_description(description.unwrap_or_default())
        .map_err(to_sql_error)?;
    if let Some(priority) = priority {
        details.set_priority(Some(
            orbit_domain::TaskPriority::parse(&priority)
                .ok_or_else(|| to_sql_error("invalid task priority"))?,
        ));
    }
    task.set_details(details);
    restore_task_status(&mut task, &status).map_err(to_sql_error)?;
    Ok(task)
}

pub(super) fn project_from_row(row: &Row<'_>) -> rusqlite::Result<Project> {
    let id: String = row.get(0)?;
    let name: String = row.get(1)?;
    let status: String = row.get(2)?;
    let mode: String = row.get(3)?;
    let objective: Option<String> = row.get(4)?;
    let signals: String = row.get(5)?;
    let mut project = Project::new(project_id_from_string(id).map_err(to_sql_error)?, name)
        .map_err(to_sql_error)?;
    project.set_mode(ProjectMode::parse(&mode).map_err(to_sql_error)?, false);
    project.set_objective(objective).map_err(to_sql_error)?;
    let parsed_signals = signals
        .split(',')
        .filter(|value| !value.is_empty())
        .map(ProjectProgressSignal::parse)
        .collect::<Result<Vec<_>, _>>()
        .map_err(to_sql_error)?;
    project
        .set_progress_signals(parsed_signals)
        .map_err(to_sql_error)?;
    restore_project_status(&mut project, &status).map_err(to_sql_error)?;
    Ok(project)
}

pub(super) fn milestone_from_row(row: &Row<'_>) -> rusqlite::Result<Milestone> {
    let id: String = row.get(0)?;
    let project_id: String = row.get(1)?;
    let name: String = row.get(2)?;
    let status: String = row.get(3)?;
    let mut milestone = Milestone::new(
        milestone_id_from_string(id).map_err(to_sql_error)?,
        project_id_from_string(project_id).map_err(to_sql_error)?,
        name,
    )
    .map_err(to_sql_error)?;
    restore_milestone_status(&mut milestone, &status).map_err(to_sql_error)?;
    Ok(milestone)
}

pub(super) fn habit_from_row(row: &Row<'_>) -> rusqlite::Result<Habit> {
    let id: String = row.get(0)?;
    let name: String = row.get(1)?;
    let cadence_kind: String = row.get(2)?;
    let target_per_week_raw: Option<i64> = row.get(3)?;
    let active: i64 = row.get(4)?;

    let target_per_week = target_per_week_raw
        .map(|value| u8::try_from(value).map_err(|_| to_sql_error("invalid habit target")))
        .transpose()?;
    let cadence = match (cadence_kind.as_str(), target_per_week) {
        ("daily", None) => HabitCadence::Daily,
        ("weekly", Some(target_per_week)) => HabitCadence::Weekly { target_per_week },
        _ => return Err(to_sql_error("invalid habit cadence")),
    };
    let mut habit = Habit::new(
        habit_id_from_string(id).map_err(to_sql_error)?,
        name,
        cadence,
    )
    .map_err(to_sql_error)?;
    if active == 0 {
        habit.pause();
    } else if active != 1 {
        return Err(to_sql_error("invalid habit active flag"));
    }
    Ok(habit)
}

pub(super) fn habit_occurrence_from_row(row: &Row<'_>) -> rusqlite::Result<HabitOccurrence> {
    let id: String = row.get(0)?;
    let habit_id: String = row.get(1)?;
    let occurred_at_ms: i64 = row.get(2)?;
    let duration_raw: Option<i64> = row.get(3)?;
    let duration_minutes = duration_raw
        .map(|value| {
            u32::try_from(value).map_err(|_| to_sql_error("invalid habit occurrence duration"))
        })
        .transpose()?;
    HabitOccurrence::new(
        HabitOccurrenceId::new(id).map_err(to_sql_error)?,
        habit_id_from_string(habit_id).map_err(to_sql_error)?,
        occurred_at_ms,
        duration_minutes,
    )
    .map_err(to_sql_error)
}

pub(super) fn schedule_from_row(row: &Row<'_>) -> rusqlite::Result<ScheduleBlock> {
    let id: String = row.get(0)?;
    let title: String = row.get(1)?;
    let starts_at_ms: i64 = row.get(2)?;
    let duration_minutes: u32 = row.get(3)?;
    let flexibility: String = row.get(4)?;
    let target_kind: String = row.get(5)?;
    let target_id: Option<String> = row.get(6)?;

    let flexibility = match flexibility.as_str() {
        "fixed" => ScheduleFlexibility::Fixed,
        "movable" => ScheduleFlexibility::Movable,
        _ => return Err(to_sql_error("invalid schedule flexibility")),
    };
    let target = match (target_kind.as_str(), target_id) {
        ("standalone", None) => ScheduleTarget::Standalone,
        ("task", Some(id)) => ScheduleTarget::Task(task_id_from_string(id).map_err(to_sql_error)?),
        ("habit", Some(id)) => {
            ScheduleTarget::Habit(habit_id_from_string(id).map_err(to_sql_error)?)
        }
        _ => return Err(to_sql_error("invalid schedule target")),
    };
    ScheduleBlock::new(
        schedule_id_from_string(id).map_err(to_sql_error)?,
        title,
        starts_at_ms,
        duration_minutes,
        flexibility,
        target,
    )
    .map_err(to_sql_error)
}

pub(super) fn activity_from_row(row: &Row<'_>) -> rusqlite::Result<ActivityRecord> {
    let id: String = row.get(0)?;
    let subject_kind: String = row.get(1)?;
    let subject_id: String = row.get(2)?;
    let activity_kind: String = row.get(3)?;
    let activity_kind_value: Option<String> = row.get(4)?;
    let origin_kind: String = row.get(5)?;
    let origin_value: Option<String> = row.get(6)?;
    let occurred_at_ms: i64 = row.get(7)?;
    let summary: String = row.get(8)?;

    ActivityRecord::new(
        activity_id_from_string(id).map_err(to_sql_error)?,
        decode_subject(&subject_kind, subject_id).map_err(to_sql_error)?,
        decode_activity_kind(&activity_kind, activity_kind_value).map_err(to_sql_error)?,
        decode_origin(&origin_kind, origin_value).map_err(to_sql_error)?,
        occurred_at_ms,
        summary,
    )
    .map_err(to_sql_error)
}

pub(super) fn restore_task_status(task: &mut Task, status: &str) -> Result<(), &'static str> {
    match status {
        "inbox" => Ok(()),
        "planned" => task
            .transition_to(TaskStatus::Planned)
            .map_err(|_| "invalid task status"),
        "ready" => task
            .transition_to(TaskStatus::Ready)
            .map_err(|_| "invalid task status"),
        "in_progress" => {
            task.transition_to(TaskStatus::Ready)
                .map_err(|_| "invalid task status")?;
            task.transition_to(TaskStatus::InProgress)
                .map_err(|_| "invalid task status")
        }
        "waiting" => {
            task.transition_to(TaskStatus::Ready)
                .map_err(|_| "invalid task status")?;
            task.transition_to(TaskStatus::InProgress)
                .map_err(|_| "invalid task status")?;
            task.transition_to(TaskStatus::Waiting)
                .map_err(|_| "invalid task status")
        }
        "blocked" => {
            task.transition_to(TaskStatus::Ready)
                .map_err(|_| "invalid task status")?;
            task.transition_to(TaskStatus::Blocked)
                .map_err(|_| "invalid task status")
        }
        "review" => {
            task.transition_to(TaskStatus::Ready)
                .map_err(|_| "invalid task status")?;
            task.transition_to(TaskStatus::InProgress)
                .map_err(|_| "invalid task status")?;
            task.transition_to(TaskStatus::Review)
                .map_err(|_| "invalid task status")
        }
        "deferred" => task
            .transition_to(TaskStatus::Deferred)
            .map_err(|_| "invalid task status"),
        "done" => {
            task.transition_to(TaskStatus::Ready)
                .map_err(|_| "invalid task status")?;
            task.transition_to(TaskStatus::InProgress)
                .map_err(|_| "invalid task status")?;
            task.transition_to(TaskStatus::Done)
                .map_err(|_| "invalid task status")
        }
        "cancelled" => task
            .transition_to(TaskStatus::Cancelled)
            .map_err(|_| "invalid task status"),
        _ => Err("unknown task status"),
    }
}

pub(super) fn restore_project_status(
    project: &mut Project,
    status: &str,
) -> Result<(), &'static str> {
    match status {
        "proposed" => Ok(()),
        "active" => project
            .transition_to(ProjectStatus::Active)
            .map_err(|_| "invalid project status"),
        "on_hold" => {
            project
                .transition_to(ProjectStatus::Active)
                .map_err(|_| "invalid project status")?;
            project
                .transition_to(ProjectStatus::OnHold)
                .map_err(|_| "invalid project status")
        }
        "completed" => {
            project
                .transition_to(ProjectStatus::Active)
                .map_err(|_| "invalid project status")?;
            project
                .transition_to(ProjectStatus::Completed)
                .map_err(|_| "invalid project status")
        }
        "cancelled" => project
            .transition_to(ProjectStatus::Cancelled)
            .map_err(|_| "invalid project status"),
        "archived" => project
            .transition_to(ProjectStatus::Archived)
            .map_err(|_| "invalid project status"),
        _ => Err("unknown project status"),
    }
}

pub(super) fn restore_milestone_status(
    milestone: &mut Milestone,
    status: &str,
) -> Result<(), &'static str> {
    match status {
        "planned" => Ok(()),
        "in_progress" => milestone
            .transition_to(MilestoneStatus::InProgress)
            .map_err(|_| "invalid milestone status"),
        "blocked" => milestone
            .transition_to(MilestoneStatus::Blocked)
            .map_err(|_| "invalid milestone status"),
        "completed" => {
            milestone
                .transition_to(MilestoneStatus::InProgress)
                .map_err(|_| "invalid milestone status")?;
            milestone
                .transition_to(MilestoneStatus::Completed)
                .map_err(|_| "invalid milestone status")
        }
        "cancelled" => milestone
            .transition_to(MilestoneStatus::Cancelled)
            .map_err(|_| "invalid milestone status"),
        _ => Err("unknown milestone status"),
    }
}

pub(super) fn encode_subject(subject: &ActivitySubject) -> (&'static str, &str) {
    match subject {
        ActivitySubject::Project(id) => ("project", id.as_str()),
        ActivitySubject::Milestone(id) => ("milestone", id.as_str()),
        ActivitySubject::Task(id) => ("task", id.as_str()),
        ActivitySubject::Habit(id) => ("habit", id.as_str()),
        ActivitySubject::Knowledge(id) => ("knowledge", id.as_str()),
        ActivitySubject::Routine(id) => ("routine", id.as_str()),
        ActivitySubject::ScheduleBlock(id) => ("schedule_block", id.as_str()),
    }
}

pub(super) fn decode_subject(kind: &str, id: String) -> Result<ActivitySubject, &'static str> {
    match kind {
        "project" => Ok(ActivitySubject::Project(project_id_from_string(id)?)),
        "milestone" => Ok(ActivitySubject::Milestone(milestone_id_from_string(id)?)),
        "task" => Ok(ActivitySubject::Task(task_id_from_string(id)?)),
        "habit" => Ok(ActivitySubject::Habit(habit_id_from_string(id)?)),
        "knowledge" => Ok(ActivitySubject::Knowledge(
            KnowledgeItemId::new(id).map_err(|_| "invalid knowledge item id")?,
        )),
        "routine" => Ok(ActivitySubject::Routine(routine_id_from_string(id)?)),
        "schedule_block" => Ok(ActivitySubject::ScheduleBlock(schedule_id_from_string(id)?)),
        _ => Err("unknown activity subject"),
    }
}

pub(super) fn encode_activity_kind(kind: &ActivityKind) -> (&'static str, Option<&str>) {
    match kind {
        ActivityKind::Created => ("created", None),
        ActivityKind::Updated => ("updated", None),
        ActivityKind::StatusChanged => ("status_changed", None),
        ActivityKind::ProgressUpdated => ("progress_updated", None),
        ActivityKind::BlockerOpened => ("blocker_opened", None),
        ActivityKind::BlockerResolved => ("blocker_resolved", None),
        ActivityKind::Scheduled => ("scheduled", None),
        ActivityKind::Rescheduled => ("rescheduled", None),
        ActivityKind::Completed => ("completed", None),
        ActivityKind::AttachmentAdded => ("attachment_added", None),
        ActivityKind::AttachmentRemoved => ("attachment_removed", None),
        ActivityKind::Custom(value) => ("custom", Some(value.as_str())),
    }
}

pub(super) fn decode_activity_kind(
    kind: &str,
    value: Option<String>,
) -> Result<ActivityKind, &'static str> {
    match (kind, value) {
        ("created", None) => Ok(ActivityKind::Created),
        ("updated", None) => Ok(ActivityKind::Updated),
        ("status_changed", None) => Ok(ActivityKind::StatusChanged),
        ("progress_updated", None) => Ok(ActivityKind::ProgressUpdated),
        ("blocker_opened", None) => Ok(ActivityKind::BlockerOpened),
        ("blocker_resolved", None) => Ok(ActivityKind::BlockerResolved),
        ("scheduled", None) => Ok(ActivityKind::Scheduled),
        ("rescheduled", None) => Ok(ActivityKind::Rescheduled),
        ("completed", None) => Ok(ActivityKind::Completed),
        ("attachment_added", None) => Ok(ActivityKind::AttachmentAdded),
        ("attachment_removed", None) => Ok(ActivityKind::AttachmentRemoved),
        ("custom", Some(value)) => Ok(ActivityKind::Custom(value)),
        _ => Err("invalid activity kind"),
    }
}

pub(super) fn encode_origin(origin: &ActivityOrigin) -> (&'static str, Option<&str>) {
    match origin {
        ActivityOrigin::Human => ("human", None),
        ActivityOrigin::System => ("system", None),
        ActivityOrigin::Agent(value) => ("agent", Some(value.as_str())),
        ActivityOrigin::External(value) => ("external", Some(value.as_str())),
    }
}

pub(super) fn decode_origin(
    kind: &str,
    value: Option<String>,
) -> Result<ActivityOrigin, &'static str> {
    match (kind, value) {
        ("human", None) => Ok(ActivityOrigin::Human),
        ("system", None) => Ok(ActivityOrigin::System),
        ("agent", Some(value)) => Ok(ActivityOrigin::Agent(value)),
        ("external", Some(value)) => Ok(ActivityOrigin::External(value)),
        _ => Err("invalid activity origin"),
    }
}

pub(super) fn task_id_from_string(value: String) -> Result<TaskId, &'static str> {
    TaskId::new(value).map_err(|_| "invalid task id")
}
pub(super) fn project_id_from_string(value: String) -> Result<ProjectId, &'static str> {
    ProjectId::new(value).map_err(|_| "invalid project id")
}
pub(super) fn milestone_id_from_string(value: String) -> Result<MilestoneId, &'static str> {
    MilestoneId::new(value).map_err(|_| "invalid milestone id")
}
pub(super) fn habit_id_from_string(value: String) -> Result<HabitId, &'static str> {
    HabitId::new(value).map_err(|_| "invalid habit id")
}
pub(super) fn routine_id_from_string(value: String) -> Result<RoutineId, &'static str> {
    RoutineId::new(value).map_err(|_| "invalid routine id")
}
pub(super) fn schedule_id_from_string(value: String) -> Result<ScheduleBlockId, &'static str> {
    ScheduleBlockId::new(value).map_err(|_| "invalid schedule id")
}
pub(super) fn activity_id_from_string(value: String) -> Result<ActivityId, &'static str> {
    ActivityId::new(value).map_err(|_| "invalid activity id")
}

pub(super) fn collect_rows<T, F>(
    rows: rusqlite::MappedRows<'_, F>,
) -> Result<Vec<T>, RepositoryError>
where
    F: FnMut(&Row<'_>) -> rusqlite::Result<T>,
{
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(repo_error)
}
