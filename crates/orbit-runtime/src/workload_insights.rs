//! Read-only workload diagnostics for a project or the entire workspace.
//!
//! Unlike automated scheduling, these diagnostics never pretend that an
//! unfinished dependency has been completed. The report explains planning
//! constraints and can be derived identically from an offline accepted replica.
use crate::{
    domain_commands::TaskRecord,
    task_dependencies::{ReadinessReason, TaskDependencyEdge, evaluate_readiness},
    task_graph::analyze_task_graph,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttentionReason {
    Overdue,
    DueSoon,
    MissingEstimate,
    UnfinishedPrerequisites,
    FutureStart,
    WaitingWorkflow,
    UnblocksOthers,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskAttention {
    pub task_id: String,
    pub title: String,
    pub project_id: Option<String>,
    pub reasons: Vec<AttentionReason>,
    pub details: Vec<String>,
    pub blocking_ids: Vec<String>,
    pub downstream_count: usize,
    pub estimated_minutes: Option<i64>,
    pub deadline_at_ms: Option<i64>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectWorkload {
    pub project_id: Option<String>,
    pub total: usize,
    pub completed: usize,
    pub actionable: usize,
    pub blocked_by_prerequisites: usize,
    pub waiting_or_deferred: usize,
    pub overdue: usize,
    pub missing_estimates: usize,
    pub known_remaining_minutes: i64,
    pub attention: Vec<TaskAttention>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkloadInsights {
    pub revision: i64,
    pub evaluated_at_ms: i64,
    pub horizon_end_ms: i64,
    pub total_tasks: usize,
    pub unfinished_tasks: usize,
    pub total_known_remaining_minutes: i64,
    pub projects: Vec<ProjectWorkload>,
}

/// Highlight tasks due within `horizon_ms` and obstacles to productive work.
/// A nonnegative explicit horizon makes offline reports reproducible.
/// Known workload ignores tasks without estimates; reports those separately.
pub fn analyze_workload(
    revision: i64,
    tasks: &[TaskRecord],
    edges: &[TaskDependencyEdge],
    now_ms: i64,
    horizon_ms: i64,
) -> Result<WorkloadInsights, &'static str> {
    if now_ms < 0
        || !(0..=366 * 86_400_000_i64).contains(&horizon_ms)
        || now_ms.checked_add(horizon_ms).is_none()
    {
        return Err("invalid workload planning horizon");
    }
    let readiness = evaluate_readiness(revision, tasks, edges, now_ms)?;
    let graph = analyze_task_graph(revision, tasks, edges, now_ms)?;
    let by_readiness: BTreeMap<&str, _> = readiness
        .tasks
        .iter()
        .map(|r| (r.task_id.as_str(), r))
        .collect();
    let by_graph: BTreeMap<&str, _> = graph
        .nodes
        .iter()
        .map(|r| (r.task_id.as_str(), r))
        .collect();
    let unfinished: BTreeSet<&str> = tasks
        .iter()
        .filter(|t| !matches!(t.status.as_str(), "done" | "cancelled"))
        .map(|t| t.id.as_str())
        .collect();
    let mut groups = BTreeMap::<Option<String>, ProjectWorkload>::new();
    let mut total_known_remaining_minutes = 0_i64;
    for task in tasks {
        let status = by_readiness
            .get(task.id.as_str())
            .ok_or("missing readiness record")?;
        let graph_node = by_graph.get(task.id.as_str()).ok_or("missing graph node")?;
        let project = groups
            .entry(task.project_id.clone())
            .or_insert_with(|| ProjectWorkload {
                project_id: task.project_id.clone(),
                total: 0,
                completed: 0,
                actionable: 0,
                blocked_by_prerequisites: 0,
                waiting_or_deferred: 0,
                overdue: 0,
                missing_estimates: 0,
                known_remaining_minutes: 0,
                attention: Vec::new(),
            });
        project.total += 1;
        if !unfinished.contains(task.id.as_str()) {
            project.completed += 1;
            continue;
        }
        if status.actionable {
            project.actionable += 1;
        }
        if !status.blocking_task_ids.is_empty() {
            project.blocked_by_prerequisites += 1;
        }
        if matches!(
            status.reason,
            ReadinessReason::Waiting
                | ReadinessReason::Deferred
                | ReadinessReason::Planned
                | ReadinessReason::Blocked
        ) {
            project.waiting_or_deferred += 1;
        }
        let overdue = task.deadline_at_ms.is_some_and(|d| d < now_ms);
        if overdue {
            project.overdue += 1;
        }
        if task.estimated_minutes.is_none() {
            project.missing_estimates += 1;
        }
        if let Some(minutes) = task.estimated_minutes {
            if minutes < 0 {
                return Err("negative remaining task estimate");
            }
            project.known_remaining_minutes = project
                .known_remaining_minutes
                .checked_add(minutes)
                .ok_or("project workload overflow")?;
            total_known_remaining_minutes = total_known_remaining_minutes
                .checked_add(minutes)
                .ok_or("total workload overflow")?;
        }
        let downstream_count = graph_node
            .all_dependents
            .iter()
            .filter(|id| unfinished.contains(id.as_str()))
            .count();
        let mut reasons = Vec::new();
        let mut details = Vec::new();
        if overdue {
            reasons.push(AttentionReason::Overdue);
            details.push("Deadline passed; work is not automatically cancelled".into());
        } else if task
            .deadline_at_ms
            .is_some_and(|d| d <= now_ms + horizon_ms)
        {
            reasons.push(AttentionReason::DueSoon);
            details.push("Deadline falls within the selected planning horizon".into());
        }
        if task.estimated_minutes.is_none() {
            reasons.push(AttentionReason::MissingEstimate);
            details.push("No duration estimate: known workload is a lower bound".into());
        }
        if !status.blocking_task_ids.is_empty() {
            reasons.push(AttentionReason::UnfinishedPrerequisites);
            details.push(format!(
                "Waiting for {} unfinished prerequisites",
                status.blocking_task_ids.len()
            ));
        }
        if status.reason == ReadinessReason::ScheduledForLater {
            reasons.push(AttentionReason::FutureStart);
            details.push("Task has a future earliest start".into());
        }
        if matches!(
            status.reason,
            ReadinessReason::Waiting | ReadinessReason::Deferred | ReadinessReason::Blocked
        ) {
            reasons.push(AttentionReason::WaitingWorkflow);
            details.push(format!("Workflow status {} is not ready", status.reason));
        }
        if downstream_count > 0 {
            reasons.push(AttentionReason::UnblocksOthers);
            details.push(format!(
                "Completing this task may unlock {downstream_count} downstream tasks"
            ));
        }
        if !reasons.is_empty() {
            project.attention.push(TaskAttention {
                task_id: task.id.clone(),
                title: task.title.clone(),
                project_id: task.project_id.clone(),
                reasons,
                details,
                blocking_ids: status.blocking_task_ids.clone(),
                downstream_count,
                estimated_minutes: task.estimated_minutes,
                deadline_at_ms: task.deadline_at_ms,
            });
        }
    }
    let mut projects: Vec<ProjectWorkload> = groups.into_values().collect();
    for project in &mut projects {
        project.attention.sort_by(|a, b| {
            let overdue_a = a.reasons.contains(&AttentionReason::Overdue);
            let overdue_b = b.reasons.contains(&AttentionReason::Overdue);
            overdue_b
                .cmp(&overdue_a)
                .then_with(|| b.downstream_count.cmp(&a.downstream_count))
                .then_with(|| a.deadline_at_ms.cmp(&b.deadline_at_ms))
                .then_with(|| a.task_id.cmp(&b.task_id))
        });
    }
    Ok(WorkloadInsights {
        revision,
        evaluated_at_ms: now_ms,
        horizon_end_ms: now_ms + horizon_ms,
        total_tasks: tasks.len(),
        unfinished_tasks: unfinished.len(),
        total_known_remaining_minutes,
        projects,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn task(
        id: &str,
        project: &str,
        status: &str,
        minutes: Option<i64>,
        deadline: Option<i64>,
    ) -> TaskRecord {
        TaskRecord {
            id: id.into(),
            title: id.into(),
            status: status.into(),
            project_id: Some(project.into()),
            milestone_id: None,
            estimated_minutes: minutes,
            earliest_start_at_ms: None,
            deadline_at_ms: deadline,
            description: String::new(),
            priority: None,
            labels: Vec::new(),
        }
    }
    fn edge(a: &str, b: &str) -> TaskDependencyEdge {
        TaskDependencyEdge {
            task_id: a.into(),
            prerequisite_id: b.into(),
        }
    }
    #[test]
    fn project_summaries_explain_known_and_unknown_work() {
        let tasks = [
            task("a", "p", "ready", Some(20), Some(90)),
            task("b", "p", "ready", None, None),
            task("c", "q", "done", Some(300), None),
            task("d", "q", "waiting", Some(10), None),
        ];
        let report = analyze_workload(6, &tasks, &[edge("b", "a")], 100, 300).unwrap();
        assert_eq!(report.unfinished_tasks, 3);
        assert_eq!(report.total_known_remaining_minutes, 30);
        let p = report
            .projects
            .iter()
            .find(|p| p.project_id.as_deref() == Some("p"))
            .unwrap();
        assert_eq!(p.known_remaining_minutes, 20);
        assert_eq!(p.overdue, 1);
        assert_eq!(p.missing_estimates, 1);
        assert_eq!(p.blocked_by_prerequisites, 1);
        assert_eq!(p.attention[0].task_id, "a");
        assert!(
            p.attention[0]
                .reasons
                .contains(&AttentionReason::UnblocksOthers)
        );
        let q = report
            .projects
            .iter()
            .find(|p| p.project_id.as_deref() == Some("q"))
            .unwrap();
        assert_eq!(q.completed, 1);
        assert_eq!(q.known_remaining_minutes, 10);
    }
    #[test]
    fn stable_when_task_order_changes() {
        let tasks = [
            task("z", "p", "ready", Some(5), Some(120)),
            task("a", "p", "ready", Some(10), Some(110)),
        ];
        let mut reversed = tasks.to_vec();
        reversed.reverse();
        let left = analyze_workload(2, &tasks, &[], 100, 30).unwrap();
        let right = analyze_workload(2, &reversed, &[], 100, 30).unwrap();
        assert_eq!(left, right);
        assert_eq!(left.projects[0].attention[0].task_id, "a");
    }
    #[test]
    fn horizon_cannot_overflow_or_roll_back() {
        assert!(analyze_workload(0, &[], &[], -1, 30).is_err());
        assert!(analyze_workload(0, &[], &[], 0, -1).is_err());
        assert!(analyze_workload(0, &[], &[], i64::MAX, 30).is_err());
        assert!(analyze_workload(0, &[], &[], 0, 367 * 86_400_000).is_err());
    }
    #[test]
    fn future_due_is_not_overdue() {
        let tasks = [task("a", "p", "ready", Some(2), Some(130))];
        let report = analyze_workload(1, &tasks, &[], 100, 40).unwrap();
        assert_eq!(report.projects[0].overdue, 0);
        assert!(
            report.projects[0].attention[0]
                .reasons
                .contains(&AttentionReason::DueSoon)
        );
    }
    #[test]
    fn waiting_workflow_detail_is_human_readable() {
        let tasks = [task("a", "p", "waiting", Some(10), None)];
        let report = analyze_workload(3, &tasks, &[], 100, 40).unwrap();
        let details = &report.projects[0].attention[0].details;
        assert!(
            details
                .iter()
                .any(|line| line == "Workflow status waiting is not ready")
        );
        for line in details {
            let camel = line
                .split(|c: char| !c.is_ascii_alphanumeric())
                .any(|token| {
                    token.chars().next().is_some_and(|c| c.is_ascii_uppercase())
                        && token.chars().skip(1).any(|c| c.is_ascii_uppercase())
                });
            assert!(!camel, "detail leaks a Debug identifier: {line}");
        }
    }
    #[test]
    fn duplicate_and_dangling_edges_are_rejected() {
        let tasks = [task("a", "p", "ready", Some(2), None)];
        assert!(analyze_workload(1, &tasks, &[edge("a", "absent")], 100, 40).is_err());
        assert!(analyze_workload(1, &tasks, &[edge("a", "a")], 100, 40).is_err());
    }
}
