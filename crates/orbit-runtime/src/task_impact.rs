//! Hypothetical, read-only completion impact for a single accepted task.
//!
//! Does not edit tasks, schedule, outbox, or server state. Compare a shared
//! revision against itself with exactly one accepted task status overridden.
use crate::{
    domain_commands::TaskRecord,
    task_dependencies::{TaskDependencyEdge, evaluate_readiness},
    task_next::next_actions,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompletionImpactRequest {
    pub task_id: String,
    pub now_ms: i64,
    #[serde(default = "default_limit")]
    pub next_limit: usize,
}
fn default_limit() -> usize {
    10
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImpactedTask {
    pub id: String,
    pub title: String,
    pub project_id: Option<String>,
    pub milestone_id: Option<String>,
    pub newly_actionable: bool,
    pub remaining_prerequisites: Vec<String>,
    pub previous_prerequisites: Vec<String>,
    pub explanation: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompletionImpact {
    pub revision: i64,
    pub task_id: String,
    pub task_title: String,
    pub already_complete: bool,
    pub evaluated_at_ms: i64,
    pub directly_unblocked_count: usize,
    pub newly_actionable_count: usize,
    pub affected: Vec<ImpactedTask>,
    pub next_action_ids_after: Vec<String>,
    pub warnings: Vec<String>,
}
/// Simulate one hypothetical status transition against a single accepted
/// snapshot. All blockers remain visible; clearing a dependency does not imply
/// a previously deferred task is ready or will fit the calendar.
pub fn preview_completion(
    revision: i64,
    tasks: &[TaskRecord],
    edges: &[TaskDependencyEdge],
    request: &CompletionImpactRequest,
) -> Result<CompletionImpact, &'static str> {
    if revision < 0 || request.now_ms < 0 {
        return Err("invalid impact revision or evaluation time");
    }
    if request.next_limit > 10_000 {
        return Err("impact next limit exceeds safety bound");
    }
    if request.task_id.trim().is_empty() {
        return Err("empty completion target");
    }
    let mut seen = BTreeSet::new();
    for task in tasks {
        if !seen.insert(task.id.as_str()) {
            return Err("duplicate accepted task identity");
        }
    }
    let original = tasks
        .iter()
        .find(|task| task.id == request.task_id)
        .ok_or("completion target absent")?;
    if original.status == "cancelled" {
        return Err("cancelled task cannot be hypothetically completed");
    }
    let already_complete = original.status == "done";
    let before = evaluate_readiness(revision, tasks, edges, request.now_ms)?;
    let mut after_tasks = tasks.to_vec();
    after_tasks
        .iter_mut()
        .find(|task| task.id == request.task_id)
        .ok_or("completion target absent")?
        .status = "done".into();
    let after = evaluate_readiness(revision, &after_tasks, edges, request.now_ms)?;
    let before_by_id: BTreeMap<_, _> = before
        .tasks
        .iter()
        .map(|task| (task.task_id.as_str(), task))
        .collect();
    let after_by_id: BTreeMap<_, _> = after
        .tasks
        .iter()
        .map(|task| (task.task_id.as_str(), task))
        .collect();
    let tasks_by_id: BTreeMap<_, _> = tasks.iter().map(|task| (task.id.as_str(), task)).collect();
    let mut affected = Vec::new();
    for (id, prior) in before_by_id {
        if id == request.task_id {
            continue;
        }
        // Include only tasks directly blocked by the target before completion.
        if !prior
            .blocking_task_ids
            .iter()
            .any(|blocker| blocker == &request.task_id)
        {
            continue;
        }
        let after = after_by_id
            .get(id)
            .ok_or("post-completion readiness missing")?;
        let task = tasks_by_id.get(id).ok_or("dependent task missing")?;
        if after.blocking_task_ids.len() >= prior.blocking_task_ids.len() {
            return Err("completion preview did not remove prerequisite blocker");
        }
        let newly_actionable = !prior.actionable && after.actionable;
        let explanation = if newly_actionable {
            "Completing the target would make this task actionable, subject to later changes".into()
        } else if !after.blocking_task_ids.is_empty() {
            format!(
                "Still blocked by {} unfinished prerequisite(s)",
                after.blocking_task_ids.len()
            )
        } else {
            "Dependency cleared, but task remains non-actionable due to status or start time".into()
        };
        affected.push(ImpactedTask {
            id: task.id.clone(),
            title: task.title.clone(),
            project_id: task.project_id.clone(),
            milestone_id: task.milestone_id.clone(),
            newly_actionable,
            remaining_prerequisites: after.blocking_task_ids.clone(),
            previous_prerequisites: prior.blocking_task_ids.clone(),
            explanation,
        });
    }
    affected.sort_by(|a, b| {
        b.newly_actionable
            .cmp(&a.newly_actionable)
            .then_with(|| a.id.cmp(&b.id))
    });
    let newly_actionable_count = affected.iter().filter(|task| task.newly_actionable).count();
    let next = next_actions(
        revision,
        &after_tasks,
        edges,
        request.now_ms,
        request.next_limit,
    )?;
    let mut warnings =
        vec!["Hypothetical preview only: no tasks or calendar blocks were modified".into()];
    if already_complete {
        warnings.push(
            "Target is already complete; no new prerequisite transitions are expected".into(),
        );
    }
    if !affected.is_empty() && newly_actionable_count == 0 {
        warnings.push("No dependent becomes immediately actionable; remaining blockers or scheduling may apply".into());
    }
    Ok(CompletionImpact {
        revision,
        task_id: request.task_id.clone(),
        task_title: original.title.clone(),
        already_complete,
        evaluated_at_ms: request.now_ms,
        directly_unblocked_count: affected.len(),
        newly_actionable_count,
        affected,
        next_action_ids_after: next.actions.into_iter().map(|a| a.task_id).collect(),
        warnings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn task(id: &str, status: &str) -> TaskRecord {
        TaskRecord {
            id: id.into(),
            title: id.into(),
            status: status.into(),
            project_id: None,
            milestone_id: None,
            estimated_minutes: Some(15),
            earliest_start_at_ms: None,
            deadline_at_ms: None,
            description: String::new(),
            priority: None,
            labels: Vec::new(),
        }
    }
    fn edge(task: &str, prerequisite: &str) -> TaskDependencyEdge {
        TaskDependencyEdge {
            task_id: task.into(),
            prerequisite_id: prerequisite.into(),
        }
    }
    fn req(id: &str) -> CompletionImpactRequest {
        CompletionImpactRequest {
            task_id: id.into(),
            now_ms: 1000,
            next_limit: 10,
        }
    }
    #[test]
    fn previews_immediate_unlock_without_mutating_accepted_tasks() {
        let tasks = [task("a", "ready"), task("b", "ready")];
        let before = tasks.clone();
        let report = preview_completion(4, &tasks, &[edge("b", "a")], &req("a")).unwrap();
        assert_eq!(tasks, before);
        assert_eq!(report.directly_unblocked_count, 1);
        assert_eq!(report.newly_actionable_count, 1);
        assert_eq!(report.affected[0].id, "b");
        assert_eq!(report.next_action_ids_after, ["b"]);
    }
    #[test]
    fn dependent_with_additional_blocker_is_not_claimed_ready() {
        let tasks = [
            task("a", "ready"),
            task("other", "ready"),
            task("b", "ready"),
        ];
        let report =
            preview_completion(2, &tasks, &[edge("b", "a"), edge("b", "other")], &req("a"))
                .unwrap();
        assert_eq!(report.directly_unblocked_count, 1);
        assert_eq!(report.newly_actionable_count, 0);
        assert_eq!(report.affected[0].remaining_prerequisites, ["other"]);
    }
    #[test]
    fn deferred_dependent_does_not_become_immediately_actionable() {
        let mut child = task("child", "ready");
        child.earliest_start_at_ms = Some(5_000);
        let tasks = [task("parent", "ready"), child];
        let report =
            preview_completion(2, &tasks, &[edge("child", "parent")], &req("parent")).unwrap();
        assert_eq!(report.affected.len(), 1);
        assert!(!report.affected[0].newly_actionable);
        assert!(report.affected[0].remaining_prerequisites.is_empty());
    }
    #[test]
    fn already_done_is_idempotent_and_reports_no_unlock() {
        let tasks = [task("a", "done"), task("b", "ready")];
        let report = preview_completion(2, &tasks, &[edge("b", "a")], &req("a")).unwrap();
        assert!(report.already_complete);
        assert!(report.affected.is_empty());
        assert_eq!(report.next_action_ids_after, ["b"]);
    }
    #[test]
    fn cancel_and_missing_tasks_fail_closed() {
        let tasks = [task("a", "cancelled"), task("b", "ready")];
        assert!(preview_completion(0, &tasks, &[], &req("a")).is_err());
        assert!(preview_completion(0, &tasks, &[], &req("missing")).is_err());
        assert!(
            preview_completion(0, &[task("b", "ready"), task("b", "ready")], &[], &req("b"))
                .is_err()
        );
    }
    #[test]
    fn graph_cycles_cannot_be_hidden_by_hypothetical_completion() {
        let tasks = [task("a", "ready"), task("b", "ready")];
        assert!(
            preview_completion(0, &tasks, &[edge("a", "b"), edge("b", "a")], &req("a")).is_err()
        );
    }
    #[test]
    fn next_action_limit_is_explicit_and_bounded() {
        let tasks = [task("a", "ready"), task("b", "ready")];
        let mut request = req("a");
        request.next_limit = 0;
        let report = preview_completion(0, &tasks, &[], &request).unwrap();
        assert!(report.next_action_ids_after.is_empty());
        request.next_limit = 10001;
        assert!(preview_completion(0, &tasks, &[], &request).is_err());
    }
}
