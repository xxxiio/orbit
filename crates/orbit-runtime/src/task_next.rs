//! Explainable next actions based on accepted task state and dependency impact.
//!
//! This is a deterministic heuristic, not ML or AI. Dependency readiness is
//! authoritative: a task that is not executable is never ranked above one
//! that is. Sorting is stable even if the input rows arrive in a different
//! order from SQLite or a remote snapshot.
use crate::{
    domain_commands::TaskRecord,
    task_dependencies::{ReadinessReason, TaskDependencyEdge, evaluate_readiness},
    task_graph::analyze_task_graph,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NextAction {
    pub task_id: String,
    pub title: String,
    pub project_id: Option<String>,
    pub rank: usize,
    pub deadline_at_ms: Option<i64>,
    pub estimated_minutes: Option<i64>,
    pub unblocks_task_ids: Vec<String>,
    pub unblocks_count: usize,
    pub overdue: bool,
    pub explanation: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NextActionsReport {
    pub revision: i64,
    pub evaluated_at_ms: i64,
    pub actionable_total: usize,
    pub blocked_total: usize,
    pub actions: Vec<NextAction>,
}

/// Ranking precedence, before alphabetical ties:
/// 1. Tasks past their deadlines (earliest deadline first).
/// 2. Number of unfinished downstream tasks potentially unblocked.
/// 3. Tasks with a deadline, earliest first.
/// 4. Estimated minutes, shortest first when both supplied.
/// 5. Stable task ID.
///
/// This is a prioritization hint, not an automatic task mutation or an agent
/// authorization. Clients may show the explanation and let the human decide.
pub fn next_actions(
    revision: i64,
    tasks: &[TaskRecord],
    edges: &[TaskDependencyEdge],
    now_ms: i64,
    limit: usize,
) -> Result<NextActionsReport, &'static str> {
    if limit > 10_000 {
        return Err("next action limit exceeds safety bound");
    }
    let report = evaluate_readiness(revision, tasks, edges, now_ms)?;
    let graph = analyze_task_graph(revision, tasks, edges, now_ms)?;
    let statuses: std::collections::BTreeMap<&str, &str> = tasks
        .iter()
        .map(|task| (task.id.as_str(), task.status.as_str()))
        .collect();
    let mut actions = Vec::new();
    for task in &report.tasks {
        if !task.actionable {
            continue;
        }
        let graph_node = graph
            .nodes
            .iter()
            .find(|node| node.task_id == task.task_id)
            .ok_or("graph node missing")?;
        let unblocks_task_ids: Vec<String> = graph_node
            .all_dependents
            .iter()
            .filter(|id| {
                statuses
                    .get(id.as_str())
                    .is_some_and(|status| *status != "done" && *status != "cancelled")
            })
            .cloned()
            .collect();
        let unblocks_count = unblocks_task_ids.len();
        let overdue = task.reason == ReadinessReason::PastDeadline;
        let mut explanation =
            vec!["Accepted task is ready and its direct prerequisites are complete".to_string()];
        if overdue {
            explanation.push("Deadline has passed; task remains actionable".to_string());
        } else if task.deadline_at_ms.is_some() {
            explanation.push("Task has an upcoming deadline".to_string());
        }
        if unblocks_count > 0 {
            explanation.push(format!(
                "Completing this task could unblock {unblocks_count} downstream tasks"
            ));
        }
        if task.estimated_minutes.is_none() {
            explanation.push("Duration estimate not supplied".to_string());
        }
        actions.push(NextAction {
            task_id: task.task_id.clone(),
            title: task.title.clone(),
            project_id: task.project_id.clone(),
            rank: 0,
            deadline_at_ms: task.deadline_at_ms,
            estimated_minutes: task.estimated_minutes,
            unblocks_task_ids,
            unblocks_count,
            overdue,
            explanation,
        });
    }
    actions.sort_by(|a, b| {
        b.overdue
            .cmp(&a.overdue)
            .then_with(|| {
                if a.overdue && b.overdue {
                    a.deadline_at_ms.cmp(&b.deadline_at_ms)
                } else {
                    std::cmp::Ordering::Equal
                }
            })
            .then_with(|| b.unblocks_count.cmp(&a.unblocks_count))
            .then_with(|| match (a.deadline_at_ms, b.deadline_at_ms) {
                (Some(a), Some(b)) => a.cmp(&b),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => std::cmp::Ordering::Equal,
            })
            .then_with(|| a.estimated_minutes.cmp(&b.estimated_minutes))
            .then_with(|| a.task_id.cmp(&b.task_id))
    });
    let actionable_total = actions.len();
    actions.truncate(limit);
    for (index, action) in actions.iter_mut().enumerate() {
        action.rank = index + 1;
    }
    Ok(NextActionsReport {
        revision,
        evaluated_at_ms: now_ms,
        actionable_total,
        blocked_total: report.blocked_count,
        actions,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn task(id: &str, status: &str, deadline: Option<i64>, estimate: Option<i64>) -> TaskRecord {
        TaskRecord {
            id: id.into(),
            title: id.into(),
            status: status.into(),
            project_id: None,
            milestone_id: None,
            estimated_minutes: estimate,
            earliest_start_at_ms: None,
            deadline_at_ms: deadline,
        }
    }
    fn edge(task: &str, pre: &str) -> TaskDependencyEdge {
        TaskDependencyEdge {
            task_id: task.into(),
            prerequisite_id: pre.into(),
        }
    }
    #[test]
    fn overdue_and_unblocking_are_explainable_and_deterministic() {
        let tasks = [
            task("high-impact", "ready", None, Some(90)),
            task("overdue", "ready", Some(10), Some(30)),
            task("child", "ready", None, None),
            task("grandchild", "ready", None, None),
            task("ordinary", "ready", None, Some(10)),
        ];
        let edges = [edge("child", "high-impact"), edge("grandchild", "child")];
        let result = next_actions(4, &tasks, &edges, 20, 10).unwrap();
        assert_eq!(result.actions[0].task_id, "overdue");
        assert!(result.actions[0].overdue);
        assert_eq!(result.actions[1].task_id, "high-impact");
        assert_eq!(result.actions[1].unblocks_count, 2);
        assert_eq!(result.actions[1].unblocks_task_ids, ["child", "grandchild"]);
        assert_eq!(result.blocked_total, 2);
        assert_eq!(result.actionable_total, 3);
        assert!(
            result.actions[1]
                .explanation
                .iter()
                .any(|s| s.contains("downstream"))
        );
    }
    #[test]
    fn zero_limit_returns_count_without_results() {
        let tasks = [task("one", "ready", None, None)];
        let report = next_actions(0, &tasks, &[], 100, 0).unwrap();
        assert!(report.actions.is_empty());
        assert_eq!(report.actionable_total, 1);
    }
    #[test]
    fn pending_or_waiting_tasks_cannot_become_next_actions() {
        let tasks = [
            task("waiting", "waiting", None, Some(1)),
            task("ready", "ready", None, Some(2)),
        ];
        let report = next_actions(0, &tasks, &[], 100, 10).unwrap();
        assert_eq!(report.actions.len(), 1);
        assert_eq!(report.actions[0].task_id, "ready");
    }
    #[test]
    fn ranking_is_invariant_under_row_order() {
        let tasks = [
            task("z", "ready", None, Some(10)),
            task("a", "ready", None, Some(10)),
        ];
        let mut reversed = tasks.clone();
        reversed.reverse();
        let first = next_actions(0, &tasks, &[], 100, 10).unwrap();
        let second = next_actions(0, &reversed, &[], 100, 10).unwrap();
        assert_eq!(first.actions, second.actions);
        assert_eq!(first.actions[0].task_id, "a");
    }
}
