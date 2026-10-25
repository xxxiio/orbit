//! Deterministic daily planning over an accepted task/dependency snapshot.
//!
//! This module never mutates tasks, reserves calendar slots, or treats proposed
//! offline changes as accepted. The caller supplies time and capacity. Outputs
//! are replayable and contain explicit reasons for every selection or exclusion.
use crate::{
    domain_commands::TaskRecord,
    task_dependencies::{ReadinessReason, TaskDependencyEdge, evaluate_readiness},
    task_next::next_actions,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DayPlanRequest {
    pub now_ms: i64,
    pub available_minutes: i64,
    pub max_tasks: usize,
    #[serde(default)]
    pub project_id: Option<String>,
    #[serde(default)]
    pub include_unsized: bool,
    #[serde(default)]
    pub default_minutes: Option<i64>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanningDisposition {
    Selected,
    OutsideProject,
    NotActionable,
    InsufficientCapacity,
    MissingEstimate,
    TaskLimit,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlannedTask {
    pub task_id: String,
    pub title: String,
    pub project_id: Option<String>,
    pub disposition: PlanningDisposition,
    pub estimated_minutes: Option<i64>,
    pub allocated_minutes: i64,
    pub start_offset_minutes: Option<i64>,
    pub blocking_task_ids: Vec<String>,
    pub explanation: Vec<String>,
    pub rank: Option<usize>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectDaySummary {
    pub project_id: Option<String>,
    pub selected: usize,
    pub actionable: usize,
    pub blocked: usize,
    pub allocated_minutes: i64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DayPlan {
    pub revision: i64,
    pub now_ms: i64,
    pub available_minutes: i64,
    pub allocated_minutes: i64,
    pub remaining_minutes: i64,
    pub selected_count: usize,
    pub actionable_count: usize,
    pub blocked_count: usize,
    pub tasks: Vec<PlannedTask>,
    pub projects: Vec<ProjectDaySummary>,
    pub warnings: Vec<String>,
}

/// Deterministic greedy capacity allocation in the already-established
/// `next_actions` priority order. A task that will not fit cannot crowd out a
/// smaller, lower ranked task; no artificial splitting of work is performed.
/// Non-actionable tasks remain in the report so users can diagnose blockers.
pub fn plan_day(
    revision: i64,
    tasks: &[TaskRecord],
    edges: &[TaskDependencyEdge],
    request: &DayPlanRequest,
) -> Result<DayPlan, &'static str> {
    if revision < 0 || request.now_ms < 0 {
        return Err("invalid plan revision or time");
    }
    if request.available_minutes < 0 || request.available_minutes > 10080 {
        return Err("capacity outside weekly bound");
    }
    if request.max_tasks > 10000 {
        return Err("task limit exceeds safety bound");
    }
    if request.default_minutes.is_some_and(|m| m <= 0 || m > 10080) {
        return Err("invalid default duration");
    }
    if request
        .project_id
        .as_ref()
        .is_some_and(|p| p.trim().is_empty())
    {
        return Err("empty project filter");
    }
    let readiness = evaluate_readiness(revision, tasks, edges, request.now_ms)?;
    let next = next_actions(revision, tasks, edges, request.now_ms, tasks.len())?;
    let by_id: BTreeMap<&str, &TaskRecord> = tasks.iter().map(|t| (t.id.as_str(), t)).collect();
    let _by_readiness: BTreeMap<&str, _> = readiness
        .tasks
        .iter()
        .map(|r| (r.task_id.as_str(), r))
        .collect();
    let priority: BTreeMap<&str, usize> = next
        .actions
        .iter()
        .enumerate()
        .map(|(index, a)| (a.task_id.as_str(), index))
        .collect();
    let mut selected = BTreeSet::new();
    let mut allocated = 0i64;
    let mut records = BTreeMap::<String, PlannedTask>::new();
    let mut warnings = Vec::new();
    for action in &next.actions {
        let task = by_id
            .get(action.task_id.as_str())
            .ok_or("action is missing task")?;
        let in_scope = request
            .project_id
            .as_ref()
            .is_none_or(|p| task.project_id.as_ref() == Some(p));
        if !in_scope {
            continue;
        }
        let effective = task.estimated_minutes.or(request.default_minutes);
        let remaining = request.available_minutes - allocated;
        let disposition = if selected.len() >= request.max_tasks {
            PlanningDisposition::TaskLimit
        } else if effective.is_none() && !request.include_unsized {
            PlanningDisposition::MissingEstimate
        } else if effective.unwrap_or(0) > remaining {
            PlanningDisposition::InsufficientCapacity
        } else {
            PlanningDisposition::Selected
        };
        let mut explanation = action.explanation.clone();
        let mut offset = None;
        let mut charged = 0;
        match disposition {
            PlanningDisposition::Selected => {
                charged = effective.unwrap_or(0);
                offset = Some(allocated);
                allocated = allocated
                    .checked_add(charged)
                    .ok_or("allocation overflow")?;
                selected.insert(action.task_id.clone());
                if task.estimated_minutes.is_none() {
                    if request.default_minutes.is_some() {
                        explanation.push("Used configured default duration".into());
                    } else {
                        explanation
                            .push("Included unsized task without capacity allocation".into());
                    }
                }
                explanation.push(format!(
                    "Selected at minute {} of available {}",
                    offset.unwrap_or(0),
                    request.available_minutes
                ));
            }
            PlanningDisposition::MissingEstimate => {
                explanation.push("Needs an estimate or explicit unsized-task permission".into())
            }
            PlanningDisposition::InsufficientCapacity => explanation.push(format!(
                "Requires {} minutes but only {} remain",
                effective.unwrap_or(0),
                remaining
            )),
            PlanningDisposition::TaskLimit => {
                explanation.push("Maximum selected-task count reached".into())
            }
            _ => {}
        }
        records.insert(
            action.task_id.clone(),
            PlannedTask {
                task_id: task.id.clone(),
                title: task.title.clone(),
                project_id: task.project_id.clone(),
                disposition,
                estimated_minutes: task.estimated_minutes,
                allocated_minutes: charged,
                start_offset_minutes: offset,
                blocking_task_ids: Vec::new(),
                explanation,
                rank: Some(priority[action.task_id.as_str()] + 1),
            },
        );
    }
    // Include excluded tasks explicitly. This is a diagnostic report, not just
    // a to-do list, and it makes stale workflow statuses inspectable.
    for ready in &readiness.tasks {
        if records.contains_key(&ready.task_id) {
            continue;
        }
        let in_scope = request
            .project_id
            .as_ref()
            .is_none_or(|p| ready.project_id.as_ref() == Some(p));
        let disposition = if in_scope {
            PlanningDisposition::NotActionable
        } else {
            PlanningDisposition::OutsideProject
        };
        let mut explanation = Vec::new();
        if !in_scope {
            explanation.push("Task is outside the requested project".into());
        } else {
            explanation.push(format!("Task is not actionable: {}", ready.reason));
            if !ready.blocking_task_ids.is_empty() {
                explanation.push(format!(
                    "Incomplete prerequisites: {}",
                    ready.blocking_task_ids.join(", ")
                ));
            }
            if ready.reason == ReadinessReason::ScheduledForLater {
                explanation.push("Earliest start time has not arrived".into());
            }
        }
        records.insert(
            ready.task_id.clone(),
            PlannedTask {
                task_id: ready.task_id.clone(),
                title: ready.title.clone(),
                project_id: ready.project_id.clone(),
                disposition,
                estimated_minutes: ready.estimated_minutes,
                allocated_minutes: 0,
                start_offset_minutes: None,
                blocking_task_ids: ready.blocking_task_ids.clone(),
                explanation,
                rank: None,
            },
        );
    }
    // Selected tasks first, then actionable but excluded, then non-actionable;
    // stable task IDs guarantee identical results for shuffled SQLite rows.
    let mut rows: Vec<_> = records.into_values().collect();
    fn bucket(disposition: &PlanningDisposition) -> u8 {
        match disposition {
            PlanningDisposition::Selected => 0,
            PlanningDisposition::InsufficientCapacity
            | PlanningDisposition::MissingEstimate
            | PlanningDisposition::TaskLimit => 1,
            PlanningDisposition::NotActionable => 2,
            PlanningDisposition::OutsideProject => 3,
        }
    }
    rows.sort_by(|a, b| {
        bucket(&a.disposition)
            .cmp(&bucket(&b.disposition))
            .then_with(|| a.rank.cmp(&b.rank))
            .then_with(|| a.task_id.cmp(&b.task_id))
    });
    let mut project_map: BTreeMap<Option<String>, ProjectDaySummary> = BTreeMap::new();
    for row in &rows {
        if row.disposition == PlanningDisposition::OutsideProject {
            continue;
        }
        let group = project_map
            .entry(row.project_id.clone())
            .or_insert_with(|| ProjectDaySummary {
                project_id: row.project_id.clone(),
                selected: 0,
                actionable: 0,
                blocked: 0,
                allocated_minutes: 0,
            });
        if row.disposition == PlanningDisposition::Selected {
            group.selected += 1;
        }
        if row.rank.is_some() {
            group.actionable += 1;
        }
        if !row.blocking_task_ids.is_empty() {
            group.blocked += 1;
        }
        group.allocated_minutes += row.allocated_minutes;
    }
    if rows
        .iter()
        .any(|r| r.disposition == PlanningDisposition::MissingEstimate)
    {
        warnings.push("Some actionable tasks have no duration estimate".into());
    }
    if rows
        .iter()
        .any(|r| r.disposition == PlanningDisposition::InsufficientCapacity)
    {
        warnings.push("Some actionable tasks do not fit remaining capacity".into());
    }
    if rows
        .iter()
        .any(|r| r.disposition == PlanningDisposition::TaskLimit)
    {
        warnings.push("Task count limit excluded additional actionable work".into());
    }
    if request.include_unsized
        && request.default_minutes.is_none()
        && rows.iter().any(|r| {
            r.disposition == PlanningDisposition::Selected && r.estimated_minutes.is_none()
        })
    {
        warnings.push("Unsized selected tasks do not consume capacity; plan may overcommit".into());
    }
    let selected_count = selected.len();
    let actionable_count = rows
        .iter()
        .filter(|r| r.rank.is_some() && r.disposition != PlanningDisposition::OutsideProject)
        .count();
    let blocked_count = rows
        .iter()
        .filter(|r| {
            !r.blocking_task_ids.is_empty() && r.disposition != PlanningDisposition::OutsideProject
        })
        .count();
    Ok(DayPlan {
        revision,
        now_ms: request.now_ms,
        available_minutes: request.available_minutes,
        allocated_minutes: allocated,
        remaining_minutes: request.available_minutes - allocated,
        selected_count,
        actionable_count,
        blocked_count,
        tasks: rows,
        projects: project_map.into_values().collect(),
        warnings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn task(
        id: &str,
        project: Option<&str>,
        status: &str,
        minutes: Option<i64>,
        deadline: Option<i64>,
    ) -> TaskRecord {
        TaskRecord {
            id: id.into(),
            title: format!("Task {id}"),
            status: status.into(),
            project_id: project.map(str::to_string),
            milestone_id: None,
            estimated_minutes: minutes,
            earliest_start_at_ms: None,
            deadline_at_ms: deadline,
            description: String::new(),
            priority: None,
            labels: Vec::new(),
        }
    }
    fn dep(id: &str, prior: &str) -> TaskDependencyEdge {
        TaskDependencyEdge {
            task_id: id.into(),
            prerequisite_id: prior.into(),
        }
    }
    fn req(cap: i64) -> DayPlanRequest {
        DayPlanRequest {
            now_ms: 100,
            available_minutes: cap,
            max_tasks: 10,
            project_id: None,
            include_unsized: false,
            default_minutes: None,
        }
    }
    #[test]
    fn chooses_fitting_tasks_without_discarding_later_options() {
        let tasks = [
            task("a", Some("p"), "ready", Some(60), Some(10)),
            task("b", Some("p"), "ready", Some(20), None),
            task("c", Some("p"), "ready", Some(10), None),
        ];
        let plan = plan_day(2, &tasks, &[], &req(30)).unwrap();
        assert_eq!(plan.selected_count, 2);
        assert_eq!(plan.allocated_minutes, 30);
        assert_eq!(plan.remaining_minutes, 0);
        assert_eq!(plan.tasks[0].task_id, "c");
        assert_eq!(plan.tasks[1].task_id, "b");
        assert_eq!(
            plan.tasks[2].disposition,
            PlanningDisposition::InsufficientCapacity
        );
        assert_eq!(plan.projects[0].allocated_minutes, 30);
    }
    #[test]
    fn dependency_blocker_is_reported_and_not_allocated() {
        let tasks = [
            task("build", Some("p"), "ready", Some(20), None),
            task("design", Some("p"), "ready", Some(15), None),
        ];
        let plan = plan_day(3, &tasks, &[dep("build", "design")], &req(100)).unwrap();
        assert_eq!(plan.blocked_count, 1);
        let blocked = plan.tasks.iter().find(|r| r.task_id == "build").unwrap();
        assert_eq!(blocked.disposition, PlanningDisposition::NotActionable);
        assert_eq!(blocked.blocking_task_ids, ["design"]);
        assert_eq!(blocked.allocated_minutes, 0);
        assert!(blocked.explanation.iter().any(|e| e.contains("design")));
    }
    #[test]
    fn default_estimates_and_unsized_policy_are_explicit() {
        let tasks = [task("a", None, "ready", None, None)];
        let strict = plan_day(1, &tasks, &[], &req(60)).unwrap();
        assert_eq!(strict.selected_count, 0);
        assert_eq!(
            strict.tasks[0].disposition,
            PlanningDisposition::MissingEstimate
        );
        let with_default = DayPlanRequest {
            default_minutes: Some(25),
            ..req(60)
        };
        let plan = plan_day(1, &tasks, &[], &with_default).unwrap();
        assert_eq!(plan.allocated_minutes, 25);
        assert_eq!(plan.tasks[0].estimated_minutes, None);
        let with_unsized = DayPlanRequest {
            include_unsized: true,
            ..req(60)
        };
        let plan = plan_day(1, &tasks, &[], &with_unsized).unwrap();
        assert_eq!(plan.selected_count, 1);
        assert_eq!(plan.allocated_minutes, 0);
        assert!(!plan.warnings.is_empty());
    }
    #[test]
    fn project_filter_and_summary_are_deterministic() {
        let tasks = [
            task("p1", Some("p"), "ready", Some(20), None),
            task("q", Some("q"), "ready", Some(30), None),
            task("p2", Some("p"), "waiting", Some(5), None),
        ];
        let request = DayPlanRequest {
            project_id: Some("p".into()),
            ..req(40)
        };
        let first = plan_day(5, &tasks, &[], &request).unwrap();
        let mut shuffled = tasks.to_vec();
        shuffled.reverse();
        let second = plan_day(5, &shuffled, &[], &request).unwrap();
        assert_eq!(first, second);
        assert_eq!(first.selected_count, 1);
        assert_eq!(first.projects.len(), 1);
        assert_eq!(first.projects[0].project_id, Some("p".into()));
        assert_eq!(
            first
                .tasks
                .iter()
                .find(|t| t.task_id == "q")
                .unwrap()
                .disposition,
            PlanningDisposition::OutsideProject
        );
    }
    #[test]
    fn overdue_remains_actionable_and_limited_count_is_visible() {
        let tasks = [
            task("a", None, "ready", Some(2), Some(1)),
            task("b", None, "ready", Some(2), None),
        ];
        let request = DayPlanRequest {
            max_tasks: 1,
            ..req(10)
        };
        let plan = plan_day(5, &tasks, &[], &request).unwrap();
        assert_eq!(plan.selected_count, 1);
        assert_eq!(plan.tasks[0].task_id, "a");
        assert_eq!(plan.tasks[1].disposition, PlanningDisposition::TaskLimit);
    }
    #[test]
    fn explanations_use_human_readable_reason_wording() {
        // CamelCase tokens are the fingerprint of a `Debug`-formatted Rust
        // enum; the web client quotes these strings verbatim.
        fn has_debug_identifier(text: &str) -> bool {
            text.split(|c: char| !c.is_ascii_alphanumeric())
                .any(|token| {
                    token.chars().next().is_some_and(|c| c.is_ascii_uppercase())
                        && token.chars().skip(1).any(|c| c.is_ascii_uppercase())
                })
        }
        let tasks = [
            task("build", Some("p"), "ready", Some(20), None),
            task("design", Some("p"), "ready", Some(15), None),
            task("active", Some("p"), "in_progress", Some(10), None),
        ];
        let plan = plan_day(4, &tasks, &[dep("build", "design")], &req(100)).unwrap();
        let blocked = plan.tasks.iter().find(|r| r.task_id == "build").unwrap();
        assert!(
            blocked
                .explanation
                .iter()
                .any(|line| line == "Task is not actionable: unfinished prerequisites")
        );
        let active = plan.tasks.iter().find(|r| r.task_id == "active").unwrap();
        assert!(
            active
                .explanation
                .iter()
                .any(|line| line == "Task is not actionable: in progress")
        );
        for row in &plan.tasks {
            for line in &row.explanation {
                assert!(
                    !has_debug_identifier(line),
                    "explanation leaks a Debug identifier: {line}"
                );
            }
        }
        for warning in &plan.warnings {
            assert!(
                !has_debug_identifier(warning),
                "warning leaks a Debug identifier: {warning}"
            );
        }
    }
    #[test]
    fn invalid_inputs_and_corrupted_edges_fail_closed() {
        let tasks = [task("a", None, "ready", Some(1), None)];
        assert!(plan_day(1, &tasks, &[], &req(-1)).is_err());
        assert!(plan_day(-1, &tasks, &[], &req(10)).is_err());
        assert!(
            plan_day(
                1,
                &tasks,
                &[],
                &DayPlanRequest {
                    default_minutes: Some(0),
                    ..req(10)
                }
            )
            .is_err()
        );
        assert!(plan_day(1, &tasks, &[dep("a", "missing")], &req(10)).is_err());
        assert!(
            plan_day(
                1,
                &tasks,
                &[],
                &DayPlanRequest {
                    project_id: Some(" ".into()),
                    ..req(10)
                }
            )
            .is_err()
        );
    }
}
