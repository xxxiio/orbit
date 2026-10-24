//! Read-only what-if analysis for dependency edits.
//!
//! The accepted task graph is immutable input. Preview never stages a write,
//! modifies a task, claims a revision, or converts pending offline commands
//! into accepted prerequisites. Callers must still submit an authoritative,
//! revision-fenced task command after reviewing this result.
use crate::{
    domain_commands::TaskRecord,
    task_dependencies::{TaskDependencyEdge, evaluate_readiness},
    task_graph::analyze_task_graph,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProposedDependencyEdit {
    Add,
    Remove,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DependencyPreviewRequest {
    pub task_id: String,
    pub prerequisite_id: String,
    pub edit: ProposedDependencyEdit,
    pub now_ms: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DependencyTaskDelta {
    pub task_id: String,
    pub actionable_before: bool,
    pub actionable_after: bool,
    pub blocking_before: Vec<String>,
    pub blocking_after: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DependencyPreview {
    pub accepted_revision: i64,
    pub edit: ProposedDependencyEdit,
    pub task_id: String,
    pub prerequisite_id: String,
    /// False when the proposed edge is already in its requested state.
    pub changes_graph: bool,
    /// A later authoritative mutation must still use this revision as its fence.
    pub requires_revision_check: bool,
    pub before_edge_count: usize,
    pub after_edge_count: usize,
    pub actionable_before: usize,
    pub actionable_after: usize,
    pub blocked_before: usize,
    pub blocked_after: usize,
    /// Only include task deltas changed by this proposal, in stable ID order.
    pub changed_tasks: Vec<DependencyTaskDelta>,
    /// Every accepted downstream task that might be affected by the changed node.
    pub potentially_affected_task_ids: Vec<String>,
    pub notes: Vec<String>,
}

/// Compute the effect of one edge operation over a validated accepted graph.
/// Fail closed on missing IDs or a cycle; never output a misleading preview.
/// This operation works identically on an embedded host or an accepted remote
/// snapshot, with the same explicit clock and stable output order.
pub fn preview_dependency(
    revision: i64,
    tasks: &[TaskRecord],
    edges: &[TaskDependencyEdge],
    request: &DependencyPreviewRequest,
) -> Result<DependencyPreview, &'static str> {
    if request.task_id.trim().is_empty() || request.prerequisite_id.trim().is_empty() {
        return Err("task and prerequisite IDs cannot be blank");
    }
    if request.task_id == request.prerequisite_id {
        return Err("a task cannot depend on itself");
    }
    // Validate the existing graph before considering the hypothetical edit.
    let before = evaluate_readiness(revision, tasks, edges, request.now_ms)?;
    let graph = analyze_task_graph(revision, tasks, edges, request.now_ms)?;
    if !tasks.iter().any(|t| t.id == request.task_id) {
        return Err("dependent task absent from accepted state");
    }
    if !tasks.iter().any(|t| t.id == request.prerequisite_id) {
        return Err("prerequisite task absent from accepted state");
    }
    let exists = edges.iter().any(|edge| {
        edge.task_id == request.task_id && edge.prerequisite_id == request.prerequisite_id
    });
    let changes_graph = match request.edit {
        ProposedDependencyEdit::Add => !exists,
        ProposedDependencyEdit::Remove => exists,
    };
    let mut proposed = edges.to_vec();
    match request.edit {
        ProposedDependencyEdit::Add if !exists => proposed.push(TaskDependencyEdge {
            task_id: request.task_id.clone(),
            prerequisite_id: request.prerequisite_id.clone(),
        }),
        ProposedDependencyEdit::Remove if exists => proposed.retain(|edge| {
            edge.task_id != request.task_id || edge.prerequisite_id != request.prerequisite_id
        }),
        _ => {}
    }
    // This also performs transitive cycle detection. Never infer that one
    // successful local simulation guarantees the future server revision.
    let after = evaluate_readiness(revision, tasks, &proposed, request.now_ms)?;
    analyze_task_graph(revision, tasks, &proposed, request.now_ms)?;
    let mut changed_tasks = Vec::new();
    for original in &before.tasks {
        let updated = after
            .tasks
            .iter()
            .find(|t| t.task_id == original.task_id)
            .ok_or("task disappeared from hypothetical graph")?;
        if original.actionable != updated.actionable
            || original.blocking_task_ids != updated.blocking_task_ids
        {
            changed_tasks.push(DependencyTaskDelta {
                task_id: original.task_id.clone(),
                actionable_before: original.actionable,
                actionable_after: updated.actionable,
                blocking_before: original.blocking_task_ids.clone(),
                blocking_after: updated.blocking_task_ids.clone(),
            });
        }
    }
    // The prerequisite's existing descendants and the dependent node's
    // descendants can change their readiness explanations after an edge edit.
    let mut affected: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    affected.insert(request.task_id.clone());
    if let Some(node) = graph
        .nodes
        .iter()
        .find(|node| node.task_id == request.task_id)
    {
        affected.extend(node.all_dependents.iter().cloned());
    }
    let mut notes = Vec::new();
    if !changes_graph {
        notes.push("The accepted graph already has the requested edge state".into());
    }
    notes.push("Read-only preview; no dependency command has been queued or accepted".into());
    notes.push("Submit a separate task command fenced by accepted_revision".into());
    Ok(DependencyPreview {
        accepted_revision: revision,
        edit: request.edit,
        task_id: request.task_id.clone(),
        prerequisite_id: request.prerequisite_id.clone(),
        changes_graph,
        requires_revision_check: true,
        before_edge_count: edges.len(),
        after_edge_count: proposed.len(),
        actionable_before: before.actionable_count,
        actionable_after: after.actionable_count,
        blocked_before: before.blocked_count,
        blocked_after: after.blocked_count,
        changed_tasks,
        potentially_affected_task_ids: affected.into_iter().collect(),
        notes,
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
            estimated_minutes: None,
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
    fn request(task: &str, prior: &str, edit: ProposedDependencyEdit) -> DependencyPreviewRequest {
        DependencyPreviewRequest {
            task_id: task.into(),
            prerequisite_id: prior.into(),
            edit,
            now_ms: 100,
        }
    }
    #[test]
    fn adding_unfinished_prerequisite_removes_actionable_task_without_mutation() {
        let tasks = [task("a", "ready"), task("b", "ready")];
        let edges = vec![];
        let report = preview_dependency(
            7,
            &tasks,
            &edges,
            &request("b", "a", ProposedDependencyEdit::Add),
        )
        .unwrap();
        assert_eq!(edges.len(), 0);
        assert_eq!(report.accepted_revision, 7);
        assert_eq!((report.before_edge_count, report.after_edge_count), (0, 1));
        assert_eq!((report.actionable_before, report.actionable_after), (2, 1));
        assert_eq!(report.changed_tasks[0].blocking_after, vec!["a"]);
        assert_eq!(report.changed_tasks[0].task_id, "b");
    }
    #[test]
    fn adding_completed_prerequisite_is_not_a_blocker() {
        let tasks = [task("a", "done"), task("b", "ready")];
        let report = preview_dependency(
            3,
            &tasks,
            &[],
            &request("b", "a", ProposedDependencyEdit::Add),
        )
        .unwrap();
        assert_eq!(report.actionable_before, report.actionable_after);
        assert!(report.changed_tasks.is_empty());
        assert_eq!(report.after_edge_count, 1);
    }
    #[test]
    fn removing_an_unfinished_prerequisite_unblocks_dependent() {
        let tasks = [task("a", "planned"), task("b", "ready")];
        let edges = [edge("b", "a")];
        let report = preview_dependency(
            4,
            &tasks,
            &edges,
            &request("b", "a", ProposedDependencyEdit::Remove),
        )
        .unwrap();
        assert!(!report.changed_tasks[0].actionable_before);
        assert!(report.changed_tasks[0].actionable_after);
        assert_eq!(report.changed_tasks[0].blocking_before, vec!["a"]);
        assert!(report.changed_tasks[0].blocking_after.is_empty());
        assert_eq!(edges.len(), 1);
    }
    #[test]
    fn duplicate_add_and_missing_remove_are_idempotent_previews() {
        let tasks = [task("a", "ready"), task("b", "ready")];
        let edges = [edge("b", "a")];
        for (edit, expected) in [
            (ProposedDependencyEdit::Add, 1),
            (ProposedDependencyEdit::Remove, 0),
        ] {
            let accepted = if edit == ProposedDependencyEdit::Add {
                &edges[..]
            } else {
                &[][..]
            };
            let report = preview_dependency(5, &tasks, accepted, &request("b", "a", edit)).unwrap();
            assert!(!report.changes_graph);
            assert_eq!(report.after_edge_count, expected);
        }
    }
    #[test]
    fn transitive_cycles_are_rejected_without_changing_accepted_graph() {
        let tasks = [task("a", "ready"), task("b", "ready"), task("c", "ready")];
        let edges = [edge("b", "a"), edge("c", "b")];
        assert!(
            preview_dependency(
                5,
                &tasks,
                &edges,
                &request("a", "c", ProposedDependencyEdit::Add)
            )
            .is_err()
        );
        assert_eq!(edges.len(), 2);
    }
    #[test]
    fn missing_nodes_and_self_edges_are_rejected() {
        let tasks = [task("a", "ready"), task("b", "ready")];
        for (id, prior) in [("missing", "a"), ("a", "missing"), ("a", "a"), ("", "a")] {
            assert!(
                preview_dependency(
                    0,
                    &tasks,
                    &[],
                    &request(id, prior, ProposedDependencyEdit::Add)
                )
                .is_err()
            );
        }
    }
    #[test]
    fn invalid_accepted_graph_fails_closed() {
        let tasks = [task("a", "ready"), task("b", "ready")];
        let corrupt = [edge("b", "a"), edge("a", "b")];
        assert!(
            preview_dependency(
                0,
                &tasks,
                &corrupt,
                &request("a", "b", ProposedDependencyEdit::Remove)
            )
            .is_err()
        );
    }
    #[test]
    fn preview_is_deterministic_under_input_row_reordering() {
        let tasks = [task("a", "done"), task("b", "ready"), task("c", "ready")];
        let mut reordered = tasks.clone();
        reordered.reverse();
        let edges = [edge("c", "b")];
        let request = request("b", "a", ProposedDependencyEdit::Add);
        assert_eq!(
            preview_dependency(4, &tasks, &edges, &request).unwrap(),
            preview_dependency(4, &reordered, &edges, &request).unwrap()
        );
    }
    #[test]
    fn error_on_negative_clock_or_revision() {
        let tasks = [task("a", "ready"), task("b", "ready")];
        let mut request = request("b", "a", ProposedDependencyEdit::Add);
        request.now_ms = -1;
        assert!(preview_dependency(0, &tasks, &[], &request).is_err());
        request.now_ms = 100;
        assert!(preview_dependency(-1, &tasks, &[], &request).is_err());
    }
}
