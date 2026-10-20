//! Dependency graph explanations for deterministic, local-first planning.
//!
//! All results are derived from accepted task and dependency snapshots. No
//! mutable cache is needed, and stable lexicographical ordering makes results
//! reproducible in CLI, MCP, offline clients, and the server.
use crate::domain_commands::TaskRecord;
use crate::task_dependencies::{TaskDependencyEdge, evaluate_readiness};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskGraphNode {
    pub task_id: String,
    pub title: String,
    pub status: String,
    /// Direct prerequisite IDs, lexicographically ordered.
    pub direct_prerequisites: Vec<String>,
    /// All reachable prerequisite IDs, with no duplicates.
    pub all_prerequisites: Vec<String>,
    /// Direct tasks that depend on this task.
    pub direct_dependents: Vec<String>,
    /// Every downstream task this task might eventually unblock.
    pub all_dependents: Vec<String>,
    /// The number of dependency edges on the longest prerequisite chain.
    pub prerequisite_depth: usize,
    /// Transitive prerequisites not currently in completed state.
    pub unfinished_prerequisites: Vec<String>,
    /// True only when the accepted task is ready and has no blockers.
    pub actionable: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskGraphReport {
    pub revision: i64,
    /// A prerequisite always appears before tasks depending on it.
    pub dependency_order: Vec<String>,
    pub roots: Vec<String>,
    pub leaves: Vec<String>,
    pub nodes: Vec<TaskGraphNode>,
    pub edge_count: usize,
}

/// Build a complete, audited graph. Reject duplicates, missing references and
/// cycles instead of guessing that corrupt offline snapshots are actionable.
pub fn analyze_task_graph(
    revision: i64,
    tasks: &[TaskRecord],
    edges: &[TaskDependencyEdge],
    now_ms: i64,
) -> Result<TaskGraphReport, &'static str> {
    let readiness = evaluate_readiness(revision, tasks, edges, now_ms)?;
    let mut by_id = BTreeMap::new();
    for task in tasks {
        by_id.insert(task.id.as_str(), task);
    }
    let mut forward: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    let mut reverse: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    for task in tasks {
        forward.insert(&task.id, BTreeSet::new());
        reverse.insert(&task.id, BTreeSet::new());
    }
    for edge in edges {
        forward
            .get_mut(edge.task_id.as_str())
            .ok_or("unknown dependent")?
            .insert(&edge.prerequisite_id);
        reverse
            .get_mut(edge.prerequisite_id.as_str())
            .ok_or("unknown prerequisite")?
            .insert(&edge.task_id);
    }
    fn reach<'a>(
        start: &'a str,
        connections: &BTreeMap<&'a str, BTreeSet<&'a str>>,
    ) -> BTreeSet<&'a str> {
        let mut visited = BTreeSet::new();
        let mut frontier: Vec<&str> = connections
            .get(start)
            .into_iter()
            .flat_map(|set| set.iter().copied())
            .collect();
        while let Some(id) = frontier.pop() {
            if visited.insert(id)
                && let Some(next) = connections.get(id)
            {
                frontier.extend(next.iter().copied());
            }
        }
        visited
    }
    // Kahn's algorithm, selecting lexicographically smallest available task.
    // Stable output is important for deterministic exports and agent planning.
    let mut unmet: BTreeMap<&str, usize> = forward
        .iter()
        .map(|(&id, parents)| (id, parents.len()))
        .collect();
    let mut queue: BTreeSet<&str> = unmet
        .iter()
        .filter_map(|(&id, &count)| (count == 0).then_some(id))
        .collect();
    let mut order = Vec::with_capacity(tasks.len());
    let mut depth: BTreeMap<&str, usize> = BTreeMap::new();
    while let Some(&next) = queue.iter().next() {
        queue.remove(next);
        order.push(next.to_string());
        let current_depth = *depth.get(next).unwrap_or(&0);
        for dependent in &reverse[next] {
            let entry = depth.entry(dependent).or_insert(0);
            *entry = (*entry).max(current_depth + 1);
            let remaining = unmet.get_mut(dependent).ok_or("missing dependency count")?;
            *remaining = remaining.checked_sub(1).ok_or("invalid dependency count")?;
            if *remaining == 0 {
                queue.insert(dependent);
            }
        }
    }
    if order.len() != tasks.len() {
        return Err("dependency cycle");
    }
    let roots = forward
        .iter()
        .filter(|(_, prereqs)| prereqs.is_empty())
        .map(|(&id, _)| id.to_owned())
        .collect();
    let leaves = reverse
        .iter()
        .filter(|(_, dependents)| dependents.is_empty())
        .map(|(&id, _)| id.to_owned())
        .collect();
    let actionability: BTreeMap<&str, bool> = readiness
        .tasks
        .iter()
        .map(|task| (task.task_id.as_str(), task.actionable))
        .collect();
    let nodes = tasks
        .iter()
        .map(|task| {
            let parents = reach(&task.id, &forward);
            let children = reach(&task.id, &reverse);
            let unfinished_prerequisites = parents
                .iter()
                .filter(|id| by_id.get(**id).is_some_and(|task| task.status != "done"))
                .map(|id| (*id).to_owned())
                .collect();
            TaskGraphNode {
                task_id: task.id.clone(),
                title: task.title.clone(),
                status: task.status.clone(),
                direct_prerequisites: forward[task.id.as_str()]
                    .iter()
                    .map(|id| (*id).to_owned())
                    .collect(),
                all_prerequisites: parents.iter().map(|id| (*id).to_owned()).collect(),
                direct_dependents: reverse[task.id.as_str()]
                    .iter()
                    .map(|id| (*id).to_owned())
                    .collect(),
                all_dependents: children.iter().map(|id| (*id).to_owned()).collect(),
                prerequisite_depth: *depth.get(task.id.as_str()).unwrap_or(&0),
                unfinished_prerequisites,
                actionable: *actionability.get(task.id.as_str()).unwrap_or(&false),
            }
        })
        .collect();
    Ok(TaskGraphReport {
        revision,
        dependency_order: order,
        roots,
        leaves,
        nodes,
        edge_count: edges.len(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn task(id: &str, status: &str) -> TaskRecord {
        TaskRecord {
            id: id.into(),
            title: format!("Work {id}"),
            status: status.into(),
            project_id: None,
            milestone_id: None,
            estimated_minutes: None,
            earliest_start_at_ms: None,
            deadline_at_ms: None,
        }
    }
    fn edge(task: &str, requires: &str) -> TaskDependencyEdge {
        TaskDependencyEdge {
            task_id: task.into(),
            prerequisite_id: requires.into(),
        }
    }
    #[test]
    fn diamond_graph_has_stable_order_and_transitive_impact() {
        let tasks = [
            task("a", "done"),
            task("b", "ready"),
            task("c", "ready"),
            task("d", "ready"),
        ];
        let edges = [
            edge("b", "a"),
            edge("c", "a"),
            edge("d", "b"),
            edge("d", "c"),
        ];
        let report = analyze_task_graph(4, &tasks, &edges, 100).unwrap();
        assert_eq!(report.dependency_order, ["a", "b", "c", "d"]);
        assert_eq!(report.roots, ["a"]);
        assert_eq!(report.leaves, ["d"]);
        assert_eq!(report.edge_count, 4);
        assert_eq!(report.nodes[0].all_dependents, ["b", "c", "d"]);
        assert_eq!(report.nodes[3].all_prerequisites, ["a", "b", "c"]);
        assert_eq!(report.nodes[3].unfinished_prerequisites, ["b", "c"]);
        assert_eq!(report.nodes[3].prerequisite_depth, 2);
        assert!(report.nodes[1].actionable);
        assert!(!report.nodes[3].actionable);
    }
    #[test]
    fn disconnected_tasks_are_topologically_stable() {
        let tasks = [task("z", "ready"), task("a", "ready"), task("m", "done")];
        let report = analyze_task_graph(0, &tasks, &[], 100).unwrap();
        assert_eq!(report.dependency_order, ["a", "m", "z"]);
        assert_eq!(report.roots, ["a", "m", "z"]);
        assert_eq!(report.leaves, ["a", "m", "z"]);
        assert_eq!(report.nodes[0].prerequisite_depth, 0);
    }
    #[test]
    fn graph_rejects_cycles_and_dangling_edges() {
        let tasks = [task("a", "ready"), task("b", "ready")];
        assert!(analyze_task_graph(1, &tasks, &[edge("a", "b"), edge("b", "a")], 0).is_err());
        assert!(analyze_task_graph(1, &tasks, &[edge("a", "unknown")], 0).is_err());
        assert!(analyze_task_graph(1, &tasks, &[edge("a", "a")], 0).is_err());
    }
    #[test]
    fn graph_is_order_independent() {
        let tasks = [task("c", "ready"), task("a", "ready"), task("b", "done")];
        let edges = [edge("c", "a"), edge("a", "b")];
        let mut reordered = tasks.clone();
        reordered.reverse();
        let mut reversed_edges = edges.clone();
        reversed_edges.reverse();
        let a = analyze_task_graph(2, &tasks, &edges, 0).unwrap();
        let b = analyze_task_graph(2, &reordered, &reversed_edges, 0).unwrap();
        assert_eq!(a.dependency_order, b.dependency_order);
        assert_eq!(a.edge_count, b.edge_count);
    }
}
