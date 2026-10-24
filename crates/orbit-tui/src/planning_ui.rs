//! Read-only terminal presentation for the authoritative planning brief.
//! The business decisions come entirely from orbit-runtime::planning_brief.
use orbit_runtime::planning_brief::{WorkBrief, WorkBriefRequest, WorkItem, WorkItemState};

pub fn request(
    now: i64,
    hours: i64,
    limit: usize,
    project: Option<String>,
) -> Result<WorkBriefRequest, String> {
    if now < 0 || !(1..=168).contains(&hours) || !(1..=500).contains(&limit) {
        return Err("plan.today requires hours 1..168 and limit 1..500".into());
    }
    let span = hours
        .checked_mul(3_600_000)
        .ok_or("invalid planning horizon")?;
    let end = now.checked_add(span).ok_or("planning horizon overflow")?;
    if project.as_ref().is_some_and(|p| p.trim().is_empty()) {
        return Err("project ID cannot be empty".into());
    }
    Ok(WorkBriefRequest {
        now_ms: now,
        ends_at_ms: end,
        limit,
        project_id: project,
        default_minutes: Some(30),
    })
}

fn state_label(state: &WorkItemState) -> &'static str {
    match state {
        WorkItemState::Scheduled => "SCHEDULED",
        WorkItemState::Ready => "READY",
        WorkItemState::Blocked => "BLOCKED",
        WorkItemState::Deferred => "DEFERRED",
        WorkItemState::Completed => "COMPLETED",
    }
}

fn compact(text: &str, limit: usize) -> String {
    let clean = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut chars = clean.chars();
    let shortened = chars.by_ref().take(limit).collect::<String>();
    if chars.next().is_some() {
        format!("{shortened}…")
    } else {
        shortened
    }
}

fn describe_item(item: &WorkItem) -> Vec<String> {
    let mut lines = vec![format!(
        "{}  {}  {}",
        state_label(&item.state),
        item.id,
        compact(&item.title, 75)
    )];
    if let (Some(start), Some(end)) = (item.proposed_start_ms, item.proposed_end_ms) {
        lines.push(format!(
            "    Proposed slot: {start} → {end} (milliseconds UTC)"
        ));
    }
    if let Some(project) = &item.project_id {
        lines.push(format!("    Project: {project}"));
    }
    if let Some(milestone) = &item.milestone_id {
        lines.push(format!("    Milestone: {milestone}"));
    }
    if let Some(deadline) = item.deadline_at_ms {
        lines.push(format!("    Deadline: {deadline}"));
    }
    if !item.blocked_by.is_empty() {
        lines.push(format!("    Waiting on: {}", item.blocked_by.join(", ")));
    }
    if !item.unblocks.is_empty() {
        lines.push(format!("    Unlocks: {}", item.unblocks.join(", ")));
    }
    if let Some(rank) = item.portfolio_rank {
        lines.push(format!("    Portfolio #{} • immediately unlocks {} • same-project downstream {} • cross-project downstream {}",
            rank,item.immediately_unblocked,item.same_project_downstream,item.other_project_downstream));
        for reason in item.portfolio_reasons.iter().filter(|reason| {
            reason.contains("unblock")
                || reason.contains("Downstream")
                || reason.contains("overdue")
        }) {
            lines.push(format!("    Portfolio: {}", compact(reason, 120)));
        }
    }
    for explanation in &item.explanation {
        lines.push(format!("    Why: {}", compact(explanation, 120)));
    }
    lines
}

/// Render a validated handoff. This view cannot write to the ledger.
pub fn render_handoff(handoff: &orbit_runtime::planning_handoff::PlanningHandoff) -> Vec<String> {
    use orbit_runtime::planning_handoff::HandoffView;
    let mut lines = vec![
        format!("PLANNING HANDOFF • accepted revision {}", handoff.revision),
        "READ ONLY • no task or calendar changes have been made".to_owned(),
        String::new(),
    ];
    match &handoff.view {
        HandoffView::Task(focus) => lines.extend(render_focus(focus)),
        HandoffView::Project(group) => lines.extend(render_group_focus(group)),
        HandoffView::Preview(preview) => {
            lines.push(format!("Next step: {:?}", preview.operation));
            lines.push(format!("Task: {}", preview.task_id));
            if let Some(prerequisite) = &preview.prerequisite_id {
                lines.push(format!("Visible prerequisite: {prerequisite}"));
            }
            lines.push(preview.explanation.clone());
            if preview.requires_explicit_approval {
                lines.push("Any write requires a separate reviewed, authorized command".into());
            }
            if matches!(
                preview.operation,
                orbit_runtime::planning_handoff::PreviewOperation::DependencyAddition
            ) {
                lines.push(
                    "Select a visible prerequisite before requesting a dependency preview".into(),
                );
            }
            if matches!(
                preview.operation,
                orbit_runtime::planning_handoff::PreviewOperation::CalendarReview
            ) {
                lines.push("Use plan.review to prepare exact slots, then plan.approve --confirm separately".into());
            }
        }
    }
    lines.extend(handoff.notes.iter().map(|note| format!("Note: {note}")));
    lines
}

/// Render an actual accepted-state dependency simulation. A preview must never
/// be confused with a successful authoritative mutation.
pub fn render_dependency_preview(
    report: &orbit_runtime::task_dependency_preview::DependencyPreview,
) -> Vec<String> {
    let mut lines = vec![
        format!(
            "DEPENDENCY WHAT-IF • accepted revision {}",
            report.accepted_revision
        ),
        "READ ONLY • no dependency edge or task state was changed".into(),
        format!(
            "Proposed {:?}: {} depends on {}",
            report.edit, report.task_id, report.prerequisite_id
        ),
        format!(
            "Graph would change: {} • edges {} → {}",
            report.changes_graph, report.before_edge_count, report.after_edge_count
        ),
        format!(
            "Actionable tasks {} → {} • blocked tasks {} → {}",
            report.actionable_before,
            report.actionable_after,
            report.blocked_before,
            report.blocked_after
        ),
    ];
    for task in &report.changed_tasks {
        lines.push(format!(
            "Task {}: actionable {} → {}",
            task.task_id, task.actionable_before, task.actionable_after
        ));
        lines.push(format!(
            "    Before blockers: {}",
            task.blocking_before.join(", ")
        ));
        lines.push(format!(
            "    After blockers: {}",
            task.blocking_after.join(", ")
        ));
    }
    for note in &report.notes {
        lines.push(format!("Note: {note}"));
    }
    lines.push("Any actual change requires a separate revision-fenced command".into());
    lines
}

/// Only a reviewed, visible dependency-removal handoff can become an immediate
/// what-if simulation. Adding an edge still requires selecting a distinct,
/// visible prerequisite; never infer one from hidden accepted relationships.
pub fn dependency_request_from_handoff(
    handoff: &orbit_runtime::planning_handoff::PlanningHandoff,
) -> Result<Option<orbit_runtime::task_dependency_preview::DependencyPreviewRequest>, String> {
    use orbit_runtime::planning_handoff::{HandoffView, PreviewOperation};
    use orbit_runtime::task_dependency_preview::{
        DependencyPreviewRequest, ProposedDependencyEdit,
    };
    if handoff.authoritative_write_performed {
        return Err("handoff incorrectly reports a write".into());
    }
    let HandoffView::Preview(preview) = &handoff.view else {
        return Ok(None);
    };
    if preview.revision != handoff.revision {
        return Err("handoff revision mismatch".into());
    }
    if preview.operation != PreviewOperation::DependencyRemoval {
        return Ok(None);
    };
    let prerequisite = preview
        .prerequisite_id
        .clone()
        .ok_or("missing accepted prerequisite")?;
    if prerequisite.trim().is_empty() || prerequisite == preview.task_id {
        return Err("invalid accepted prerequisite".into());
    }
    Ok(Some(DependencyPreviewRequest {
        task_id: preview.task_id.clone(),
        prerequisite_id: prerequisite,
        edit: ProposedDependencyEdit::Remove,
        now_ms: preview.evaluated_at_ms,
    }))
}

/// A calendar action can show a real, accepted-state proposal for its task.
/// The schedule service remains the sole allocator; never interpret a brief's
/// proposed timestamp as a reserved calendar block.
pub fn calendar_request_from_handoff(
    handoff: &orbit_runtime::planning_handoff::PlanningHandoff,
) -> Result<Option<orbit_runtime::schedule_planning::SchedulePlanRequest>, String> {
    use orbit_runtime::planning_handoff::{HandoffView, PreviewOperation};
    if handoff.authoritative_write_performed {
        return Err("handoff incorrectly reports a write".into());
    }
    let HandoffView::Preview(preview) = &handoff.view else {
        return Ok(None);
    };
    if preview.operation != PreviewOperation::CalendarSlots {
        return Ok(None);
    };
    if preview.revision != handoff.revision || preview.window_end_ms <= preview.evaluated_at_ms {
        return Err("calendar handoff no longer matches the accepted planning window".into());
    }
    if preview.task_id.trim().is_empty() {
        return Err("missing calendar preview target".into());
    }
    Ok(Some(
        orbit_runtime::schedule_planning::SchedulePlanRequest {
            starts_at_ms: preview.evaluated_at_ms,
            ends_at_ms: preview.window_end_ms,
            max_tasks: 100,
            project_id: None,
            default_minutes: Some(30),
            include_unsized: true,
        },
    ))
}

/// Render the selected task only. Whole-workspace plans may contain unrelated
/// tasks, which must not be returned by a task-specific handoff.
pub fn render_task_calendar_preview(
    handoff: &orbit_runtime::planning_handoff::PlanningHandoff,
    plan: &orbit_runtime::schedule_planning::SchedulePlan,
) -> Result<Vec<String>, String> {
    use orbit_runtime::planning_handoff::{HandoffView, PreviewOperation};
    let HandoffView::Preview(preview) = &handoff.view else {
        return Err("not a calendar preview".into());
    };
    if preview.operation != PreviewOperation::CalendarSlots || handoff.authoritative_write_performed
    {
        return Err("not a read-only calendar preview".into());
    }
    if preview.revision != handoff.revision {
        return Err("stale calendar handoff".into());
    }
    if plan.task_revision != handoff.revision || plan.schedule_revision != handoff.revision {
        return Err("calendar proposal uses a different accepted revision".into());
    }
    let mut lines = vec![
        format!("CALENDAR WHAT-IF • accepted revision {}", handoff.revision),
        "READ ONLY • no calendar reservation has been created".into(),
        format!("Task: {}", preview.task_id),
    ];
    if let Some(slot) = plan.slots.iter().find(|s| s.task_id == preview.task_id) {
        lines.push(format!(
            "Proposed slot: {} → {} ({} minutes)",
            slot.starts_at_ms, slot.ends_at_ms, slot.minutes
        ));
        lines.extend(
            slot.explanation
                .iter()
                .map(|reason| format!("Why: {reason}")),
        );
    } else {
        lines.push("No uninterrupted proposed slot for this task in the current plan".into());
        for excluded in plan
            .unplaced
            .iter()
            .filter(|s| s.task_id == preview.task_id)
        {
            lines.push(format!("Reason: {}", excluded.reason));
        }
    }
    lines.push(
        "To reserve time, separately review exact slots and approve an authoritative write".into(),
    );
    Ok(lines)
}

pub fn render_focus(focus: &orbit_runtime::planning_focus::TaskFocus) -> Vec<String> {
    let mut lines = vec![
        format!("TASK FOCUS • accepted rev {}", focus.revision),
        format!("{} • {}", focus.task.id, focus.task.title),
        format!("State: {:?}", focus.task.state),
        format!(
            "Project: {} • Milestone: {}",
            focus.task.project_id.as_deref().unwrap_or("unassigned"),
            focus.task.milestone_id.as_deref().unwrap_or("unassigned")
        ),
        format!(
            "Visible project tasks: {} • milestone tasks: {}",
            focus.project_task_count, focus.milestone_task_count
        ),
    ];
    for (title, rows) in [
        ("PREREQUISITES", &focus.visible_prerequisites),
        ("DOWNSTREAM DEPENDENTS", &focus.visible_dependents),
        ("PROJECT TASKS", &focus.project_peers),
        ("MILESTONE TASKS", &focus.milestone_peers),
    ] {
        lines.push(String::new());
        lines.push(title.into());
        if rows.is_empty() {
            lines.push("  None visible".into());
        }
        for row in rows {
            lines.push(format!(
                "  {} • {} • {:?}",
                row.task_id, row.title, row.state
            ));
        }
    }
    if !focus.prerequisite_trails.is_empty() {
        lines.push(String::new());
        lines.push("VISIBLE PREREQUISITE PATHS".into());
        for trail in &focus.prerequisite_trails {
            lines.push(format!(
                "  {}{}",
                trail.task_ids.join(" → "),
                if trail.unresolved_leaf {
                    " (truncated)"
                } else {
                    ""
                }
            ));
        }
    }
    if !focus.dependent_trails.is_empty() {
        lines.push(String::new());
        lines.push("VISIBLE DOWNSTREAM PATHS".into());
        for trail in &focus.dependent_trails {
            lines.push(format!(
                "  {}{}",
                trail.task_ids.join(" → "),
                if trail.unresolved_leaf {
                    " (truncated)"
                } else {
                    ""
                }
            ));
        }
    }
    if let Some(next) = &focus.immediate_next {
        lines.push(format!("Next in Today: {}", next.task_id));
    }
    if let Some(prev) = &focus.previous_in_today {
        lines.push(format!("Previous in Today: {}", prev.task_id));
    }
    for note in &focus.navigation_notes {
        lines.push(format!("Note: {note}"));
    }
    lines
}

pub fn render_group_focus(group: &orbit_runtime::planning_focus::GroupFocus) -> Vec<String> {
    let mut lines = vec![
        format!("PROJECT FOCUS • accepted rev {}", group.revision),
        format!(
            "Project: {}",
            group.project.project_id.as_deref().unwrap_or("unassigned")
        ),
        format!(
            "Total {} • scheduled {} • ready {} • blocked {} • deferred {} • completed {}",
            group.project.task_count,
            group.project.scheduled,
            group.project.actionable,
            group.project.blocked,
            group.project.deferred,
            group.project.completed
        ),
        format!(
            "Proposed {} minutes • overdue {}",
            group.project.proposed_minutes, group.project.overdue
        ),
        "TOP ACTIONS".into(),
    ];
    for row in &group.project.top_actions {
        lines.push(format!(
            "  {} • {} • portfolio {:?}",
            row.task_id, row.title, row.portfolio_rank
        ));
    }
    if group.project.top_actions.is_empty() {
        lines.push("  None available".into());
    }
    lines.push(String::new());
    lines.push("VISIBLE BOTTLENECKS".into());
    for row in &group.project.bottlenecks {
        lines.push(format!(
            "  {} blocks {} task(s)",
            row.prerequisite_id, row.visible_waiters
        ));
        for waiter in &row.waiting_task_ids {
            lines.push(format!("    → {waiter}"));
        }
    }
    if group.project.bottlenecks.is_empty() {
        lines.push("  None visible".into());
    }
    lines.push(String::new());
    lines.push("MILESTONES".into());
    for summary in &group.milestones {
        lines.push(format!(
            "  {} • {} tasks • {} ready • {} blocked • {} overdue",
            summary.milestone_id.as_deref().unwrap_or("unassigned"),
            summary.task_count,
            summary.actionable,
            summary.blocked,
            summary.overdue
        ));
    }
    for note in &group.notes {
        lines.push(format!("Note: {note}"));
    }
    lines
}

pub fn render_brief(brief: &WorkBrief) -> Vec<String> {
    let mut result=vec![
        format!("TODAY / NEXT  •  accepted revision {}  •  evaluated {}",brief.revision,brief.evaluated_at_ms),
        format!("Capacity {}min  •  proposed {}min  •  schedule {}  ready {}  blocked {}  deferred {}  completed {}",
            brief.available_minutes,brief.proposed_minutes,brief.scheduled_count,brief.ready_count,
            brief.blocked_count,brief.deferred_count,brief.completed_count),
        format!("PORTFOLIO • {} actionable accepted task(s) • {} visible task(s) with cross-project leverage",
            brief.portfolio_actionable_total,brief.cross_project_leverage_count),
        "READ-ONLY PROPOSAL • pending offline commands are not included • times shown as epoch milliseconds".into(),
        String::new(),
    ];
    for item in &brief.items {
        result.extend(describe_item(item));
        result.push(String::new());
    }
    if brief.items.is_empty() {
        result.push("No accepted tasks match this planning window.".into());
    }
    if !brief.milestones.is_empty() {
        result.push("MILESTONE BOTTLENECKS".into());
        for m in &brief.milestones {
            result.push(format!(
                "  project={} milestone={} | scheduled={} ready={} blocked={} overdue={}",
                m.project_id.as_deref().unwrap_or("-"),
                m.milestone_id.as_deref().unwrap_or("-"),
                m.scheduled,
                m.ready,
                m.blocked,
                m.overdue_task_ids.len()
            ));
            for action in m.leverage_actions.iter().take(5) {
                result.push(format!(
                    "    PRIORITY #{} {} | immediate unlocks {} | cross-project {} | downstream {}",
                    action.portfolio_rank,
                    action.task_id,
                    action.immediately_unblocked,
                    action.cross_project_downstream,
                    action.downstream_total
                ));
            }
            for blocker in m.prerequisite_bottlenecks.iter().take(10) {
                result.push(format!(
                    "    {} blocks {} task(s)",
                    blocker.prerequisite_id, blocker.waiting_count
                ));
            }
        }
    }
    for warning in &brief.warnings {
        result.push(format!("WARNING: {warning}"));
    }
    result
}

pub fn render_impact(report: &orbit_runtime::task_impact::CompletionImpact) -> Vec<String> {
    let mut lines = vec![
        format!("COMPLETION IMPACT • accepted revision {}", report.revision),
        format!(
            "If completed: {} — {}",
            report.task_id,
            compact(&report.task_title, 80)
        ),
        format!(
            "Dependent tasks affected: {} • newly actionable: {}",
            report.directly_unblocked_count, report.newly_actionable_count
        ),
        "SIMULATION ONLY • no task status, schedule, or pending command changed".into(),
        String::new(),
    ];
    for task in &report.affected {
        lines.push(format!(
            "{} {} — {}",
            if task.newly_actionable {
                "READY"
            } else {
                "STILL BLOCKED"
            },
            task.id,
            compact(&task.title, 80)
        ));
        if !task.remaining_prerequisites.is_empty() {
            lines.push(format!(
                "    Remaining prerequisites: {}",
                task.remaining_prerequisites.join(", ")
            ));
        }
        lines.push(format!("    {}", task.explanation));
    }
    if report.affected.is_empty() {
        lines.push("No dependent tasks would change.".into());
    }
    lines.push(String::new());
    lines.push(format!(
        "Suggested next actions: {}",
        report.next_action_ids_after.join(", ")
    ));
    for warning in &report.warnings {
        lines.push(format!("Note: {warning}"));
    }
    lines
}

/// A read-only list of selectable tasks in the brief. Positions are line offsets
/// within the rendered text; selecting a task never mutates its status.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlanningDocument {
    pub lines: Vec<String>,
    pub task_anchors: Vec<(usize, String)>,
}
impl PlanningDocument {
    pub fn from_brief(brief: &WorkBrief) -> Self {
        let lines = render_brief(brief);
        let mut anchors = Vec::new();
        // Extract anchors by matching exact generated header prefixes. This is
        // display metadata, not an input parser or a source of domain truth.
        for item in &brief.items {
            let prefix = format!("{}  {}  ", state_label(&item.state), item.id);
            if let Some((index, _)) = lines
                .iter()
                .enumerate()
                .find(|(_, line)| line.starts_with(&prefix))
            {
                anchors.push((index, item.id.clone()));
            }
        }
        Self {
            lines,
            task_anchors: anchors,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn request_is_bounded_and_explicit() {
        let brief = request(100, 4, 10, Some("alpha".into())).unwrap();
        assert_eq!(brief.project_id.as_deref(), Some("alpha"));
        assert_eq!(brief.ends_at_ms, 100 + 4 * 3_600_000);
        for (now, hours, limit) in [(0, 0, 10), (0, 169, 10), (0, 12, 0), (0, 12, 501)] {
            assert!(request(now, hours, limit, None).is_err(), "{hours}/{limit}");
        }
        assert!(request(i64::MAX, 12, 10, None).is_err());
        assert!(request(1, 12, 10, Some("  ".into())).is_err());
    }

    fn item(id: &str, state: WorkItemState) -> WorkItem {
        WorkItem {
            id: id.into(),
            title: format!("Task {id}"),
            project_id: None,
            milestone_id: None,
            state,
            recommended_rank: Some(1),
            proposed_start_ms: None,
            proposed_end_ms: None,
            deadline_at_ms: None,
            blocked_by: vec![],
            unblocks: vec![],
            explanation: vec!["Ready".into()],
            portfolio_rank: None,
            same_project_downstream: 0,
            other_project_downstream: 0,
            immediately_unblocked: 0,
            portfolio_reasons: vec![],
        }
    }

    #[test]
    fn navigation_indexes_only_real_task_headers() {
        let brief = WorkBrief {
            revision: 7,
            evaluated_at_ms: 100,
            ends_at_ms: 200,
            available_minutes: 10,
            proposed_minutes: 0,
            scheduled_count: 0,
            ready_count: 2,
            blocked_count: 0,
            deferred_count: 0,
            completed_count: 0,
            items: vec![
                item("a", WorkItemState::Ready),
                item("b", WorkItemState::Ready),
            ],
            projects: vec![],
            milestones: vec![],
            warnings: vec![],
            portfolio_actionable_total: 0,
            cross_project_leverage_count: 0,
        };
        let doc = PlanningDocument::from_brief(&brief);
        assert_eq!(doc.task_anchors.len(), 2);
        assert_eq!(doc.task_anchors[0].1, "a");
        assert_eq!(doc.task_anchors[1].1, "b");
        for (row, id) in &doc.task_anchors {
            assert!(doc.lines[*row].contains(id), "{row} {id}");
        }
    }
    #[test]
    fn labels_cover_every_planner_state() {
        assert_eq!(state_label(&WorkItemState::Scheduled), "SCHEDULED");
        assert_eq!(state_label(&WorkItemState::Ready), "READY");
        assert_eq!(state_label(&WorkItemState::Blocked), "BLOCKED");
        assert_eq!(state_label(&WorkItemState::Deferred), "DEFERRED");
        assert_eq!(state_label(&WorkItemState::Completed), "COMPLETED");
    }
    #[test]
    fn compact_removes_newlines_without_truncating_unicode_mid_byte() {
        assert_eq!(compact("one\n  two\tthree", 100), "one two three");
        assert_eq!(compact("日本語で説明", 3), "日本語…");
    }
}

/// Parse the read-only portfolio query used by connected terminal clients.
pub fn parse_portfolio(
    source: &str,
    now_ms: i64,
) -> Result<Option<orbit_runtime::portfolio_actions::PortfolioActionRequest>, String> {
    let mut words = source.split_whitespace();
    if words.next() != Some("plan.portfolio") {
        return Ok(None);
    }
    let mut limit = 20usize;
    let mut project = None;
    while let Some(option) = words.next() {
        match option {
            "--limit" => {
                limit = words
                    .next()
                    .ok_or("--limit needs a value")?
                    .parse()
                    .map_err(|_| "--limit must be a nonnegative integer")?
            }
            "--project" => project = Some(words.next().ok_or("--project needs an ID")?.to_string()),
            _ => return Err(format!("unknown plan.portfolio argument: {option}")),
        }
    }
    if limit > 200 {
        return Err("portfolio limit exceeds terminal maximum of 200".into());
    }
    if project
        .as_ref()
        .is_some_and(|p: &String| p.trim().is_empty())
    {
        return Err("project ID cannot be empty".into());
    }
    Ok(Some(
        orbit_runtime::portfolio_actions::PortfolioActionRequest {
            now_ms,
            project_id: project,
            limit,
            include_unassigned: false,
        },
    ))
}

pub fn render_portfolio(
    report: &orbit_runtime::portfolio_actions::PortfolioActionReport,
) -> Vec<String> {
    let mut lines = vec![
        format!(
            "PROJECT-AWARE NEXT ACTIONS • accepted revision {}",
            report.revision
        ),
        format!(
            "Actionable: {} • Blocked: {} • Showing: {}",
            report.actionable_total,
            report.blocked_total,
            report.recommended.len()
        ),
        "Read-only recommendations • no pending connected edits included".into(),
        String::new(),
    ];
    for row in &report.recommended {
        lines.push(format!("{}. {} — {}", row.rank, row.task_id, row.title));
        lines.push(format!(
            "   Project: {} • immediate unlocks: {} • future impact: {}",
            row.project_id.as_deref().unwrap_or("unassigned"),
            row.immediately_unblocked,
            row.unfinished_downstream
        ));
        lines.push(format!(
            "   Own-project impact: {} • other-project impact: {} across {} projects",
            row.same_project_downstream, row.other_project_downstream, row.downstream_project_count
        ));
        if let Some(deadline) = row.deadline_at_ms {
            lines.push(format!(
                "   Deadline: {deadline}{}",
                if row.overdue { " (overdue)" } else { "" }
            ));
        }
        for reason in &row.reasons {
            lines.push(format!("   Why: {reason}"));
        }
        lines.push(String::new());
    }
    if report.recommended.is_empty() {
        lines.push("No actionable tasks for the selected project.".into());
    }
    for warning in &report.warnings {
        lines.push(format!("Scope: {warning}"));
    }
    lines
}

#[cfg(test)]
mod portfolio_tests {
    use super::*;
    #[test]
    fn parses_bounded_project_portfolio_requests() {
        let request = parse_portfolio("plan.portfolio --limit 7 --project p", 100)
            .unwrap()
            .unwrap();
        assert_eq!(request.limit, 7);
        assert_eq!(request.project_id.as_deref(), Some("p"));
        assert_eq!(request.now_ms, 100);
        assert!(parse_portfolio("plan.today", 100).unwrap().is_none());
        for invalid in [
            "plan.portfolio --limit 201",
            "plan.portfolio --limit nope",
            "plan.portfolio --project",
            "plan.portfolio --unknown",
        ] {
            assert!(parse_portfolio(invalid, 100).is_err());
        }
    }
}

#[cfg(test)]
mod portfolio_display_tests {
    use super::*;
    #[test]
    fn document_renders_leverage_without_reclassifying_task_status() {
        let task = WorkItem {
            id: "platform-api".into(),
            title: "Ship API".into(),
            project_id: Some("platform".into()),
            milestone_id: Some("release".into()),
            state: WorkItemState::Ready,
            recommended_rank: Some(2),
            proposed_start_ms: None,
            proposed_end_ms: None,
            deadline_at_ms: None,
            blocked_by: vec![],
            unblocks: vec![],
            explanation: vec!["Ready".into()],
            portfolio_rank: Some(1),
            same_project_downstream: 1,
            other_project_downstream: 3,
            immediately_unblocked: 2,
            portfolio_reasons: vec![
                "Completing this task would immediately unblock 2 task(s)".into(),
            ],
        };
        let brief = WorkBrief {
            revision: 4,
            evaluated_at_ms: 0,
            ends_at_ms: 60_000,
            available_minutes: 1,
            proposed_minutes: 0,
            scheduled_count: 0,
            ready_count: 1,
            blocked_count: 0,
            deferred_count: 0,
            completed_count: 0,
            items: vec![task],
            projects: vec![],
            milestones: vec![],
            warnings: vec![],
            portfolio_actionable_total: 1,
            cross_project_leverage_count: 1,
        };
        let document = PlanningDocument::from_brief(&brief);
        assert!(
            document
                .lines
                .iter()
                .any(|line| line.contains("Portfolio #1"))
        );
        assert!(
            document
                .lines
                .iter()
                .any(|line| line.contains("immediately unlocks 2"))
        );
        assert!(
            document
                .lines
                .iter()
                .any(|line| line.contains("cross-project leverage"))
        );
        assert_eq!(brief.items[0].state, WorkItemState::Ready);
    }
}

/// One interactive alternative against the deterministic portfolio baseline.
/// It is deliberately read-only: approval must use normal task/schedule commands.
pub fn parse_scenario(
    source: &str,
    now_ms: i64,
    revision: i64,
) -> Result<Option<orbit_runtime::planning_scenarios::ScenarioCompareRequest>, String> {
    let mut words = source.split_whitespace();
    if words.next() != Some("plan.compare") {
        return Ok(None);
    }
    let mut project = None;
    let mut budget = 120;
    let mut maximum = 5usize;
    let mut pins = Vec::new();
    let mut excludes = Vec::new();
    let mut alternative_budget = None;
    let mut alternative_max = None;
    while let Some(flag) = words.next() {
        match flag {
            "--project" => {
                project = Some(words.next().ok_or("--project requires an ID")?.to_owned())
            }
            "--minutes" => {
                budget = words
                    .next()
                    .ok_or("--minutes requires a value")?
                    .parse()
                    .map_err(|_| "invalid baseline budget")?
            }
            "--limit" => {
                maximum = words
                    .next()
                    .ok_or("--limit requires a value")?
                    .parse()
                    .map_err(|_| "invalid baseline task limit")?
            }
            "--pin" => pins.push(words.next().ok_or("--pin requires a task ID")?.to_owned()),
            "--exclude" => excludes.push(
                words
                    .next()
                    .ok_or("--exclude requires a task ID")?
                    .to_owned(),
            ),
            "--alt-minutes" => {
                alternative_budget = Some(
                    words
                        .next()
                        .ok_or("--alt-minutes requires a value")?
                        .parse()
                        .map_err(|_| "invalid alternate budget")?,
                )
            }
            "--alt-limit" => {
                alternative_max = Some(
                    words
                        .next()
                        .ok_or("--alt-limit requires a value")?
                        .parse()
                        .map_err(|_| "invalid alternate task limit")?,
                )
            }
            _ => return Err(format!("unrecognized comparison argument: {flag}")),
        }
    }
    if !(0..=525_600).contains(&budget) || maximum > 200 {
        return Err("baseline capacity out of range".into());
    }
    if project
        .as_ref()
        .is_some_and(|v: &String| v.trim().is_empty())
    {
        return Err("project ID cannot be empty".into());
    }
    Ok(Some(
        orbit_runtime::planning_scenarios::ScenarioCompareRequest {
            expected_revision: revision,
            baseline: orbit_runtime::portfolio_capacity::PortfolioCapacityRequest {
                now_ms,
                available_minutes: budget,
                max_tasks: maximum,
                project_id: project,
                include_unassigned: false,
                include_unsized: false,
                default_minutes: None,
            },
            alternatives: vec![orbit_runtime::planning_scenarios::ScenarioChoice {
                name: "what-if".into(),
                pinned_task_ids: pins,
                excluded_task_ids: excludes,
                available_minutes: alternative_budget,
                max_tasks: alternative_max,
            }],
        },
    ))
}

/// Connected TUI review command. Produces an immutable, read-only request;
/// only the separately confirmed `plan.approve --confirm` command can write.
/// Flags are intentionally the same as `plan.compare`, plus `--hours` and
/// `--baseline` to choose the baseline rather than the named what-if plan.
pub fn parse_scenario_review(
    source: &str,
    now_ms: i64,
    revision: i64,
    unique_id: &str,
) -> Result<Option<orbit_runtime::scenario_execution::ScenarioExecutionRequest>, String> {
    let mut words = source.split_whitespace();
    if words.next() != Some("plan.review") {
        return Ok(None);
    }
    if now_ms < 0 || revision < 0 {
        return Err("accepted revision and start time are required".into());
    }
    let mut hours = 8i64;
    let mut baseline = false;
    let mut comparison_args = vec!["plan.compare".to_owned()];
    while let Some(token) = words.next() {
        match token {
            "--hours" => {
                hours = words
                    .next()
                    .ok_or("--hours requires a value")?
                    .parse()
                    .map_err(|_| "invalid planning horizon")?
            }
            "--baseline" => baseline = true,
            "--pin" | "--exclude" | "--project" | "--minutes" | "--limit" | "--alt-minutes"
            | "--alt-limit" => {
                comparison_args.push(token.to_owned());
                comparison_args.push(
                    words
                        .next()
                        .ok_or("planning option requires a value")?
                        .to_owned(),
                );
            }
            _ => return Err(format!("unknown scenario review argument: {token}")),
        }
    }
    if !(1..=168).contains(&hours) {
        return Err("scenario horizon must be 1–168 hours".into());
    }
    let starts_at_ms = now_ms - (now_ms % 60_000);
    let ends_at_ms = starts_at_ms
        .checked_add(hours * 3_600_000)
        .ok_or("scenario horizon overflow")?;
    let comparison = parse_scenario(&comparison_args.join(" "), starts_at_ms, revision)?
        .ok_or("invalid scenario comparison arguments")?;
    if unique_id.trim().is_empty()
        || unique_id.len() > 80
        || !unique_id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
    {
        return Err("invalid review identity".into());
    }
    Ok(Some(
        orbit_runtime::scenario_execution::ScenarioExecutionRequest {
            comparison,
            scenario_name: if baseline { "baseline" } else { "what-if" }.into(),
            starts_at_ms,
            ends_at_ms,
            command_id: format!("{unique_id}-schedule"),
            identity_prefix: format!("{unique_id}-slot"),
        },
    ))
}

pub fn render_scenario_review(
    review: &orbit_runtime::scenario_execution::ScenarioExecutionReview,
) -> Vec<String> {
    let mut lines = vec![
        format!(
            "SCHEDULE REVIEW • {} • revision {}",
            review.selected_scenario, review.review.expected_revision
        ),
        format!(
            "{} tasks • {} reserved minutes • {} – {}",
            review.review.scheduled_tasks.len(),
            review.review.total_reserved_minutes,
            review.plan.window_start_ms,
            review.plan.window_end_ms
        ),
        String::new(),
        "READ-ONLY • no reservations have been created".into(),
        "Review EVERY calendar slot before approving. Esc closes this view.".into(),
        String::new(),
    ];
    for item in &review.review.scheduled_slots {
        lines.push(format!(
            "  {} • {} → {} ({} min) • block {}",
            item.task_id,
            item.starts_at_ms,
            item.ends_at_ms,
            item.duration_minutes,
            item.schedule_block_id
        ));
    }
    if !review.warnings.is_empty() {
        lines.push(String::new());
        lines.extend(
            review
                .warnings
                .iter()
                .map(|warning| format!("WARNING • {warning}")),
        );
    }
    lines.push(String::new());
    lines.push("To execute these exact slots: plan.approve --confirm".into());
    lines.push("Any server revision change requires a new review".into());
    lines
}

pub fn render_scenarios(
    report: &orbit_runtime::planning_scenarios::ScenarioComparison,
) -> Vec<String> {
    let mut lines = vec![
        format!(
            "WHAT-IF PLANNING • accepted revision {}",
            report.accepted_revision
        ),
        "READ-ONLY • no task changes or calendar reservations were applied".into(),
    ];
    for scenario in std::iter::once(&report.baseline).chain(report.alternatives.iter()) {
        lines.push(String::new());
        lines.push(format!(
            "{}: {} of {} minutes allocated ({} remain)",
            scenario.name,
            scenario.allocated_minutes,
            scenario.capacity_minutes,
            scenario.remaining_minutes
        ));
        for item in &scenario.chosen {
            lines.push(format!(
                "  #{}  {} — {} minutes",
                item.portfolio_rank, item.title, item.minutes
            ));
        }
        for item in &scenario.excluded {
            lines.push(format!("  excluded {}: {}", item.task_id, item.reason));
        }
        if !scenario.newly_selected.is_empty() {
            lines.push(format!("  Added: {}", scenario.newly_selected.join(", ")));
        }
        if !scenario.displaced.is_empty() {
            lines.push(format!("  Displaced: {}", scenario.displaced.join(", ")));
        }
    }
    lines
}

#[cfg(test)]
mod scenario_terminal_tests {
    use super::*;
    #[test]
    fn parses_revision_bound_scenario() {
        let req=parse_scenario("plan.compare --minutes 90 --limit 3 --pin t1 --exclude t2 --alt-minutes 60 --project p",12,5)
            .unwrap().unwrap();
        assert_eq!(req.expected_revision, 5);
        assert_eq!(req.baseline.available_minutes, 90);
        assert_eq!(req.alternatives[0].pinned_task_ids, vec!["t1"]);
        assert_eq!(req.alternatives[0].excluded_task_ids, vec!["t2"]);
        assert_eq!(req.alternatives[0].available_minutes, Some(60));
        assert_eq!(req.baseline.project_id.as_deref(), Some("p"));
    }
    #[test]
    fn rejects_invalid_scenario_arguments() {
        for command in [
            "plan.compare --pin",
            "plan.compare --limit -1",
            "plan.compare --minutes -2",
            "plan.compare --unknown xyz",
            "plan.compare --alt-minutes wrong",
        ] {
            assert!(parse_scenario(command, 1, 1).is_err(), "{command}");
        }
        assert!(parse_scenario("plan.today", 1, 1).unwrap().is_none());
    }
}

#[cfg(test)]
mod scenario_execution_ui_tests {
    use super::*;
    #[test]
    fn review_command_requires_nonempty_stable_identity() {
        assert!(parse_scenario_review("plan.review", 0, 3, "").is_err());
        assert!(parse_scenario_review("plan.review", 0, 3, "invalid/id").is_err());
    }
    #[test]
    fn parser_reuses_comparison_flags_and_selects_alternative() {
        let result = parse_scenario_review(
            "plan.review --hours 6 --minutes 90 --pin task-a --exclude task-b",
            65_000,
            3,
            "session-1",
        )
        .unwrap()
        .unwrap();
        assert_eq!(result.starts_at_ms, 60_000);
        assert_eq!(result.ends_at_ms, 60_000 + 6 * 3_600_000);
        assert_eq!(result.scenario_name, "what-if");
        assert_eq!(result.comparison.baseline.now_ms, 60_000);
        assert_eq!(
            result.comparison.alternatives[0].pinned_task_ids,
            vec!["task-a"]
        );
        assert_eq!(
            result.comparison.alternatives[0].excluded_task_ids,
            vec!["task-b"]
        );
    }
    #[test]
    fn parser_can_select_unchanged_baseline() {
        let result = parse_scenario_review("plan.review --baseline --hours 2", 0, 2, "session")
            .unwrap()
            .unwrap();
        assert_eq!(result.scenario_name, "baseline");
        assert_eq!(result.ends_at_ms, 7_200_000);
    }
    #[test]
    fn rejects_unbounded_review_horizon() {
        assert!(parse_scenario_review("plan.review --hours 0", 0, 2, "session").is_err());
        assert!(parse_scenario_review("plan.review --hours 169", 0, 2, "session").is_err());
        assert!(parse_scenario_review("plan.review --hours unknown", 0, 2, "session").is_err());
    }
    #[test]
    fn rejects_unknown_option() {
        assert!(parse_scenario_review("plan.review --approve", 0, 2, "session").is_err());
    }
}

#[cfg(test)]
mod action_handoff_terminal_tests {
    use super::*;
    use orbit_runtime::planning_actions::ActionDestination;
    use orbit_runtime::planning_handoff::{
        HandoffView, PlanningHandoff, PreviewInstruction, PreviewOperation,
    };

    fn preview(operation: PreviewOperation, confirmation: bool) -> PlanningHandoff {
        PlanningHandoff {
            revision: 11,
            destination: ActionDestination::CalendarReview,
            view: HandoffView::Preview(PreviewInstruction {
                revision: 11,
                operation,
                task_id: "t".into(),
                prerequisite_id: None,
                evaluated_at_ms: 1_000,
                window_end_ms: 3_601_000,
                explanation: "This is a preview".into(),
                requires_explicit_approval: confirmation,
            }),
            authoritative_write_performed: false,
            notes: vec!["No authoritative write occurred".into()],
        }
    }
    #[test]
    fn calendar_approval_handoff_is_visibly_non_executing() {
        let lines = render_handoff(&preview(PreviewOperation::CalendarReview, true)).join("\n");
        assert!(lines.contains("READ ONLY"));
        assert!(lines.contains("plan.review"));
        assert!(lines.contains("plan.approve --confirm"));
        assert!(lines.contains("separate reviewed, authorized command"));
        assert!(lines.contains("No authoritative write occurred"));
    }
    #[test]
    fn dependency_creation_does_not_choose_a_hidden_prerequisite() {
        let lines = render_handoff(&preview(PreviewOperation::DependencyAddition, true)).join("\n");
        assert!(lines.contains("Select a visible prerequisite"));
        assert!(!lines.contains("secret-task"));
    }
    #[test]
    fn read_only_completion_preview_does_not_claim_approval() {
        let lines = render_handoff(&preview(PreviewOperation::CompletionImpact, false)).join("\n");
        assert!(lines.contains("READ ONLY"));
        assert!(!lines.contains("Any write requires"));
        assert!(lines.contains("This is a preview"));
    }
}

#[cfg(test)]
mod m22_229_handoff_preview_tests {
    use super::*;
    use orbit_runtime::planning_actions::ActionDestination;
    use orbit_runtime::planning_handoff::{
        HandoffView, PlanningHandoff, PreviewInstruction, PreviewOperation,
    };
    use orbit_runtime::schedule_planning::{PlannedSlot, SchedulePlan, UnplacedTask};
    use orbit_runtime::task_dependency_preview::{
        DependencyPreview, DependencyTaskDelta, ProposedDependencyEdit,
    };

    fn handoff(operation: PreviewOperation, parent: Option<&str>) -> PlanningHandoff {
        PlanningHandoff {
            revision: 7,
            destination: match operation {
                PreviewOperation::DependencyAddition | PreviewOperation::DependencyRemoval => {
                    ActionDestination::DependencyPreview
                }
                PreviewOperation::CalendarSlots => ActionDestination::CalendarPreview,
                PreviewOperation::CalendarReview => ActionDestination::CalendarReview,
                PreviewOperation::CompletionImpact => ActionDestination::CompletionPreview,
            },
            view: HandoffView::Preview(PreviewInstruction {
                revision: 7,
                operation,
                task_id: "task".into(),
                prerequisite_id: parent.map(str::to_owned),
                evaluated_at_ms: 60_000,
                window_end_ms: 3_660_000,
                explanation: "Read-only".into(),
                requires_explicit_approval: true,
            }),
            authoritative_write_performed: false,
            notes: vec![],
        }
    }
    fn plan() -> SchedulePlan {
        SchedulePlan {
            task_revision: 7,
            schedule_revision: 7,
            starts_at_ms: 60_000,
            ends_at_ms: 3_660_000,
            available_minutes: 60,
            allocated_minutes: 30,
            occupied_minutes: 0,
            free_windows: vec![],
            slots: vec![
                PlannedSlot {
                    task_id: "task".into(),
                    title: "Task".into(),
                    project_id: None,
                    starts_at_ms: 60_000,
                    ends_at_ms: 1_860_000,
                    minutes: 30,
                    ranking: 1,
                    explanation: vec!["fits".into()],
                },
                PlannedSlot {
                    task_id: "foreign".into(),
                    title: "Hidden".into(),
                    project_id: Some("other".into()),
                    starts_at_ms: 1_860_000,
                    ends_at_ms: 3_660_000,
                    minutes: 30,
                    ranking: 2,
                    explanation: vec!["other".into()],
                },
            ],
            unplaced: vec![],
            warnings: vec![],
        }
    }
    #[test]
    fn removal_dispatches_only_a_specific_reviewed_prerequisite() {
        let h = handoff(PreviewOperation::DependencyRemoval, Some("parent"));
        let request = dependency_request_from_handoff(&h).unwrap().unwrap();
        assert_eq!(request.task_id, "task");
        assert_eq!(request.prerequisite_id, "parent");
        assert_eq!(request.edit, ProposedDependencyEdit::Remove);
        assert_eq!(request.now_ms, 60_000);
    }
    #[test]
    fn adding_dependency_requires_separate_prerequisite_selection() {
        let h = handoff(PreviewOperation::DependencyAddition, None);
        assert!(dependency_request_from_handoff(&h).unwrap().is_none());
    }
    #[test]
    fn missing_or_self_prerequisite_fails_closed() {
        assert!(
            dependency_request_from_handoff(&handoff(PreviewOperation::DependencyRemoval, None))
                .is_err()
        );
        assert!(
            dependency_request_from_handoff(&handoff(
                PreviewOperation::DependencyRemoval,
                Some("task")
            ))
            .is_err()
        );
    }
    #[test]
    fn mutated_handoff_revision_is_rejected() {
        let mut h = handoff(PreviewOperation::DependencyRemoval, Some("parent"));
        h.revision = 8;
        assert!(dependency_request_from_handoff(&h).is_err());
    }
    #[test]
    fn calendar_request_reuses_reviewed_window_without_mutation() {
        let request =
            calendar_request_from_handoff(&handoff(PreviewOperation::CalendarSlots, None))
                .unwrap()
                .unwrap();
        assert_eq!(request.starts_at_ms, 60_000);
        assert_eq!(request.ends_at_ms, 3_660_000);
        assert_eq!(request.project_id, None);
    }
    #[test]
    fn calendar_review_is_not_automatically_executed() {
        assert!(
            calendar_request_from_handoff(&handoff(PreviewOperation::CalendarReview, None))
                .unwrap()
                .is_none()
        );
    }
    #[test]
    fn calendar_view_only_shows_the_selected_task() {
        let lines =
            render_task_calendar_preview(&handoff(PreviewOperation::CalendarSlots, None), &plan())
                .unwrap()
                .join("\n");
        assert!(lines.contains("Proposed slot"));
        assert!(!lines.contains("foreign"));
        assert!(!lines.contains("Hidden"));
        assert!(!lines.contains("other project task"));
        assert!(lines.contains("no calendar reservation"));
    }
    #[test]
    fn schedule_revision_change_must_reject_preview() {
        let mut plan = plan();
        plan.schedule_revision = 8;
        assert!(
            render_task_calendar_preview(&handoff(PreviewOperation::CalendarSlots, None), &plan)
                .is_err()
        );
    }
    #[test]
    fn task_revision_change_must_reject_preview() {
        let mut plan = plan();
        plan.task_revision = 8;
        assert!(
            render_task_calendar_preview(&handoff(PreviewOperation::CalendarSlots, None), &plan)
                .is_err()
        );
    }
    #[test]
    fn missing_slot_reports_only_its_own_exclusion() {
        let mut plan = plan();
        plan.slots.clear();
        plan.unplaced = vec![
            UnplacedTask {
                task_id: "task".into(),
                reason: "No continuous window".into(),
            },
            UnplacedTask {
                task_id: "foreign".into(),
                reason: "Hidden reason".into(),
            },
        ];
        let text =
            render_task_calendar_preview(&handoff(PreviewOperation::CalendarSlots, None), &plan)
                .unwrap()
                .join("\n");
        assert!(text.contains("No continuous window"));
        assert!(!text.contains("Hidden reason"));
    }
    #[test]
    fn dependency_preview_presents_before_and_after_without_claiming_commit() {
        let report = DependencyPreview {
            accepted_revision: 7,
            edit: ProposedDependencyEdit::Remove,
            task_id: "task".into(),
            prerequisite_id: "parent".into(),
            changes_graph: true,
            requires_revision_check: true,
            before_edge_count: 1,
            after_edge_count: 0,
            actionable_before: 1,
            actionable_after: 2,
            blocked_before: 1,
            blocked_after: 0,
            changed_tasks: vec![DependencyTaskDelta {
                task_id: "task".into(),
                actionable_before: false,
                actionable_after: true,
                blocking_before: vec!["parent".into()],
                blocking_after: vec![],
            }],
            potentially_affected_task_ids: vec!["task".into()],
            notes: vec![],
        };
        let text = render_dependency_preview(&report).join("\n");
        assert!(text.contains("READ ONLY"));
        assert!(text.contains("actionable false → true"));
        assert!(text.contains("separate revision-fenced command"));
    }
}
