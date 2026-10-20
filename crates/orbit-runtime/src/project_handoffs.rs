//! Project handoffs on top of the accepted task prerequisite graph.
//!
//! Project dependencies are not separately writable state: they are derived
//! from the actual task graph at the accepted revision. Unlike task graph
//! cycles, project-level back-and-forth handoffs are allowed and reported as
//! coordination risks, not rejected as corrupt data.
use crate::domain_commands::TaskRecord;
use crate::task_dependencies::TaskDependencyEdge;
use crate::task_graph::analyze_task_graph;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HandoffRequest {
    pub now_ms: i64,
    #[serde(default)]
    pub project_id: Option<String>,
    #[serde(default)]
    pub include_completed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HandoffEdge {
    pub from_project: Option<String>,
    pub to_project: Option<String>,
    /// Direct task-level dependency count, including completed prerequisites
    /// when requested by the caller.
    pub direct_dependencies: usize,
    pub outstanding_dependencies: usize,
    pub blocked_tasks: usize,
    pub dependent_estimated_minutes: i64,
    pub unknown_estimate_tasks: usize,
    /// Task identifiers are only returned when the full graph is authorized.
    pub prerequisite_task_ids: Vec<String>,
    pub dependent_task_ids: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HandoffProject {
    pub project_id: Option<String>,
    pub waiting_on_projects: usize,
    pub serves_projects: usize,
    pub blocked_task_count: usize,
    pub actionable_task_count: usize,
    pub downstream_project_count: usize,
    /// A project can have task-level work ready despite an incoming handoff.
    pub completely_blocked: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectHandoffReport {
    pub revision: i64,
    pub evaluated_at_ms: i64,
    pub project_filter: Option<String>,
    pub edges: Vec<HandoffEdge>,
    pub projects: Vec<HandoffProject>,
    /// Set of projects engaged in a round-trip cross-project coordination
    /// cycle. Such a cycle does NOT imply a cycle in the task DAG.
    pub round_trip_projects: Vec<Option<String>>,
    pub warnings: Vec<String>,
}

/// Derive cross-project handoffs from task edges, with deterministic ordering.
/// For project-scoped use, all foreign task identifiers are omitted and only
/// edges involving that project are returned. Authorization of the selected
/// project is the responsibility of the transport before calling this API.
pub fn analyze_handoffs(
    revision: i64,
    tasks: &[TaskRecord],
    edges: &[TaskDependencyEdge],
    request: &HandoffRequest,
) -> Result<ProjectHandoffReport, &'static str> {
    if revision < 0 || request.now_ms < 0 {
        return Err("invalid handoff revision or time");
    }
    if request
        .project_id
        .as_ref()
        .is_some_and(|id| id.trim().is_empty())
    {
        return Err("empty project filter");
    }
    let graph = analyze_task_graph(revision, tasks, edges, request.now_ms)?;
    let lookup: BTreeMap<&str, &TaskRecord> = tasks.iter().map(|t| (t.id.as_str(), t)).collect();
    type Key = (Option<String>, Option<String>);
    let mut grouped: BTreeMap<Key, Vec<(&TaskRecord, &TaskRecord)>> = BTreeMap::new();
    for edge in edges {
        let dependent = lookup[edge.task_id.as_str()];
        let prerequisite = lookup[edge.prerequisite_id.as_str()];
        if dependent.project_id == prerequisite.project_id {
            continue;
        }
        if !request.include_completed && dependent.status == "done" {
            continue;
        }
        if request.project_id.as_ref().is_some_and(|id| {
            dependent.project_id.as_ref() != Some(id)
                && prerequisite.project_id.as_ref() != Some(id)
        }) {
            continue;
        }
        grouped
            .entry((
                prerequisite.project_id.clone(),
                dependent.project_id.clone(),
            ))
            .or_default()
            .push((prerequisite, dependent));
    }
    let mut result_edges = Vec::new();
    for ((from_project, to_project), pairs) in grouped {
        let mut prerequisites = BTreeSet::new();
        let mut dependents = BTreeSet::new();
        let mut blocked = BTreeSet::new();
        let mut estimated = BTreeMap::<&str, Option<i64>>::new();
        let mut outstanding = 0;
        for (prerequisite, dependent) in &pairs {
            prerequisites.insert(prerequisite.id.clone());
            dependents.insert(dependent.id.clone());
            if prerequisite.status != "done" {
                outstanding += 1;
                if dependent.status != "done" {
                    blocked.insert(dependent.id.as_str());
                }
            }
            estimated.insert(dependent.id.as_str(), dependent.estimated_minutes);
        }
        let mut dependent_estimated_minutes = 0_i64;
        let mut unknown_estimate_tasks = 0;
        for estimate in estimated.values() {
            match estimate {
                Some(minutes) if *minutes >= 0 => {
                    dependent_estimated_minutes =
                        dependent_estimated_minutes.saturating_add(*minutes);
                }
                _ => unknown_estimate_tasks += 1,
            }
        }
        let scoped = request.project_id.is_some();
        result_edges.push(HandoffEdge {
            from_project,
            to_project,
            direct_dependencies: pairs.len(),
            outstanding_dependencies: outstanding,
            blocked_tasks: blocked.len(),
            dependent_estimated_minutes,
            unknown_estimate_tasks,
            prerequisite_task_ids: if scoped {
                Vec::new()
            } else {
                prerequisites.into_iter().collect()
            },
            dependent_task_ids: if scoped {
                Vec::new()
            } else {
                dependents.into_iter().collect()
            },
        });
    }
    let mut ids = BTreeSet::new();
    for task in tasks {
        if request
            .project_id
            .as_ref()
            .is_none_or(|id| task.project_id.as_ref() == Some(id))
        {
            ids.insert(task.project_id.clone());
        }
    }
    let mut projects = Vec::new();
    for id in ids {
        let mut wait_on = BTreeSet::new();
        let mut serves = BTreeSet::new();
        let mut blocked = 0;
        let mut actionable = 0;
        for task in tasks
            .iter()
            .filter(|task| task.project_id == id && task.status != "done")
        {
            let node = graph
                .nodes
                .iter()
                .find(|node| node.task_id == task.id)
                .ok_or("missing accepted task graph node")?;
            if node.actionable {
                actionable += 1;
            }
            if !node.unfinished_prerequisites.is_empty() {
                blocked += 1;
            }
        }
        for edge in &result_edges {
            if edge.to_project == id && edge.outstanding_dependencies > 0 {
                wait_on.insert(edge.from_project.clone());
            }
            if edge.from_project == id {
                serves.insert(edge.to_project.clone());
            }
        }
        let mut downstream = BTreeSet::new();
        let mut queue: VecDeque<_> = serves.iter().cloned().collect();
        while let Some(next) = queue.pop_front() {
            if next == id {
                continue;
            }
            if downstream.insert(next.clone()) {
                for edge in &result_edges {
                    if edge.from_project == next {
                        queue.push_back(edge.to_project.clone());
                    }
                }
            }
        }
        projects.push(HandoffProject {
            project_id: id,
            waiting_on_projects: wait_on.len(),
            serves_projects: serves.len(),
            blocked_task_count: blocked,
            actionable_task_count: actionable,
            downstream_project_count: downstream.len(),
            completely_blocked: blocked > 0 && actionable == 0,
        });
    }
    // A two-way project coordination cycle can exist without violating the
    // task dependency DAG, so report a warning rather than rejecting it.
    let mut round_trip = BTreeSet::new();
    for edge in &result_edges {
        if result_edges.iter().any(|other| {
            other.from_project == edge.to_project && other.to_project == edge.from_project
        }) {
            round_trip.insert(edge.from_project.clone());
            round_trip.insert(edge.to_project.clone());
        }
    }
    // Project-scoped adapters may authorize only the selected project. Redact
    // foreign project identities *after* deriving graph counts so the report
    // remains useful without exposing another project's stable identifier.
    if let Some(allowed) = request.project_id.as_ref() {
        round_trip = round_trip
            .into_iter()
            .map(|id| {
                if id.as_ref() == Some(allowed) {
                    id
                } else {
                    Some("[external project]".to_owned())
                }
            })
            .collect();
        for edge in &mut result_edges {
            if edge.from_project.as_ref() != Some(allowed) {
                edge.from_project = Some("[external project]".to_owned());
            }
            if edge.to_project.as_ref() != Some(allowed) {
                edge.to_project = Some("[external project]".to_owned());
            }
        }
    }
    let mut warnings = Vec::new();
    if !round_trip.is_empty() {
        warnings
            .push("Two-way project handoffs require coordination; task DAG remains valid".into());
    }
    if request.project_id.is_some() && !result_edges.is_empty() {
        warnings
            .push("Scoped handoff report omits task identities for all cross-project edges".into());
    }
    Ok(ProjectHandoffReport {
        revision,
        evaluated_at_ms: request.now_ms,
        project_filter: request.project_id.clone(),
        edges: result_edges,
        projects,
        round_trip_projects: round_trip.into_iter().collect(),
        warnings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn task(id: &str, project: &str, status: &str, minutes: Option<i64>) -> TaskRecord {
        TaskRecord {
            id: id.into(),
            title: format!("sensitive {id}"),
            status: status.into(),
            project_id: Some(project.into()),
            milestone_id: None,
            estimated_minutes: minutes,
            earliest_start_at_ms: None,
            deadline_at_ms: None,
        }
    }
    fn edge(t: &str, p: &str) -> TaskDependencyEdge {
        TaskDependencyEdge {
            task_id: t.into(),
            prerequisite_id: p.into(),
        }
    }
    fn request(project: Option<&str>) -> HandoffRequest {
        HandoffRequest {
            now_ms: 10,
            project_id: project.map(str::to_owned),
            include_completed: false,
        }
    }
    #[test]
    fn cross_project_handoff_tracks_unique_blocked_tasks_and_minutes() {
        let tasks = [
            task("a", "platform", "ready", Some(5)),
            task("b", "frontend", "ready", Some(30)),
            task("c", "frontend", "ready", None),
            task("d", "frontend", "done", Some(8)),
        ];
        let edges = [edge("b", "a"), edge("c", "a"), edge("d", "a")];
        let report = analyze_handoffs(1, &tasks, &edges, &request(None)).unwrap();
        assert_eq!(report.edges.len(), 1);
        let edge = &report.edges[0];
        assert_eq!(edge.direct_dependencies, 2);
        assert_eq!(edge.blocked_tasks, 2);
        assert_eq!(edge.outstanding_dependencies, 2);
        assert_eq!(edge.dependent_estimated_minutes, 30);
        assert_eq!(edge.unknown_estimate_tasks, 1);
        let frontend = report
            .projects
            .iter()
            .find(|p| p.project_id.as_deref() == Some("frontend"))
            .unwrap();
        assert!(frontend.completely_blocked);
    }
    #[test]
    fn completed_prerequisite_no_longer_blocks_handoff() {
        let tasks = [
            task("a", "platform", "done", Some(5)),
            task("b", "frontend", "ready", Some(30)),
        ];
        let result = analyze_handoffs(1, &tasks, &[edge("b", "a")], &request(None)).unwrap();
        assert_eq!(result.edges[0].outstanding_dependencies, 0);
        let frontend = result
            .projects
            .iter()
            .find(|p| p.project_id.as_deref() == Some("frontend"))
            .unwrap();
        assert_eq!(frontend.actionable_task_count, 1);
        assert!(!frontend.completely_blocked);
    }
    #[test]
    fn project_scoped_report_redacts_all_task_ids() {
        let tasks = [
            task("secret", "platform", "ready", Some(5)),
            task("public", "frontend", "ready", Some(30)),
        ];
        let result = analyze_handoffs(
            1,
            &tasks,
            &[edge("public", "secret")],
            &request(Some("frontend")),
        )
        .unwrap();
        assert!(result.edges[0].prerequisite_task_ids.is_empty());
        assert!(result.edges[0].dependent_task_ids.is_empty());
        let serialized = serde_json::to_string(&result).unwrap();
        assert!(!serialized.contains("secret"));
        assert!(!serialized.contains("public"));
        assert_eq!(result.projects.len(), 1);
    }
    #[test]
    fn project_round_trip_is_not_task_cycle() {
        let tasks = [
            task("a", "one", "ready", Some(10)),
            task("b", "two", "ready", Some(10)),
            task("c", "one", "ready", Some(10)),
        ];
        let report =
            analyze_handoffs(1, &tasks, &[edge("b", "a"), edge("c", "b")], &request(None)).unwrap();
        assert_eq!(
            report.round_trip_projects,
            [Some("one".into()), Some("two".into())]
        );
        assert!(!report.warnings.is_empty());
    }
    #[test]
    fn invalid_graph_and_input_fail_closed() {
        let tasks = [
            task("a", "one", "ready", None),
            task("b", "two", "ready", None),
        ];
        assert!(
            analyze_handoffs(1, &tasks, &[edge("a", "b"), edge("b", "a")], &request(None)).is_err()
        );
        let mut bad = request(None);
        bad.now_ms = -1;
        assert!(analyze_handoffs(1, &tasks, &[], &bad).is_err());
        assert!(analyze_handoffs(1, &tasks, &[edge("a", "missing")], &request(None)).is_err());
    }
}

#[cfg(test)]
mod additional_handoff_contract_tests {
    use super::*;
    fn task(id: &str, project: &str, status: &str, minutes: Option<i64>) -> TaskRecord {
        TaskRecord {
            id: id.into(),
            title: format!("private {id}"),
            status: status.into(),
            project_id: Some(project.into()),
            milestone_id: None,
            estimated_minutes: minutes,
            earliest_start_at_ms: None,
            deadline_at_ms: None,
        }
    }
    fn edge(t: &str, p: &str) -> TaskDependencyEdge {
        TaskDependencyEdge {
            task_id: t.into(),
            prerequisite_id: p.into(),
        }
    }
    fn req(project: Option<&str>, completed: bool) -> HandoffRequest {
        HandoffRequest {
            now_ms: 100,
            project_id: project.map(str::to_owned),
            include_completed: completed,
        }
    }
    #[test]
    fn multiple_dependencies_on_one_task_count_one_blocked_task() {
        let tasks = [
            task("a", "infra", "ready", Some(10)),
            task("b", "infra", "ready", Some(20)),
            task("c", "app", "ready", Some(30)),
        ];
        let report = analyze_handoffs(
            1,
            &tasks,
            &[edge("c", "a"), edge("c", "b")],
            &req(None, false),
        )
        .unwrap();
        let e = &report.edges[0];
        assert_eq!(e.direct_dependencies, 2);
        assert_eq!(e.outstanding_dependencies, 2);
        assert_eq!(e.blocked_tasks, 1);
        assert_eq!(e.dependent_estimated_minutes, 30);
        assert_eq!(e.dependent_task_ids, ["c"]);
    }
    #[test]
    fn completed_dependents_included_only_if_requested() {
        let tasks = [
            task("a", "infra", "ready", None),
            task("b", "app", "done", Some(40)),
        ];
        let edges = [edge("b", "a")];
        assert!(
            analyze_handoffs(1, &tasks, &edges, &req(None, false))
                .unwrap()
                .edges
                .is_empty()
        );
        let report = analyze_handoffs(1, &tasks, &edges, &req(None, true)).unwrap();
        assert_eq!(report.edges[0].direct_dependencies, 1);
        assert_eq!(report.edges[0].blocked_tasks, 0);
    }
    #[test]
    fn unrelated_projects_do_not_leak_in_scoped_response() {
        let tasks = [
            task("a", "allowed", "ready", None),
            task("b", "partner", "ready", None),
            task("secret", "secret-project", "ready", None),
            task("hidden", "hidden-project", "ready", None),
        ];
        let result = analyze_handoffs(
            1,
            &tasks,
            &[edge("a", "b"), edge("secret", "hidden")],
            &req(Some("allowed"), false),
        )
        .unwrap();
        assert_eq!(result.edges.len(), 1);
        assert_eq!(result.projects.len(), 1);
        let json = serde_json::to_string(&result).unwrap();
        assert!(!json.contains("partner"));
        assert!(!json.contains("secret-project"));
        assert!(!json.contains("hidden-project"));
        assert!(!json.contains("secret"));
        assert!(!json.contains("hidden"));
    }
    #[test]
    fn scoped_partner_is_redacted_in_both_directions() {
        let tasks = [
            task("a", "allowed", "ready", None),
            task("b", "foreign", "ready", None),
            task("c", "allowed", "ready", None),
        ];
        let result = analyze_handoffs(
            1,
            &tasks,
            &[edge("a", "b"), edge("b", "c")],
            &req(Some("allowed"), false),
        )
        .unwrap();
        assert_eq!(result.edges.len(), 2);
        for edge in &result.edges {
            assert!(edge.from_project.as_deref() != Some("foreign"));
            assert!(edge.to_project.as_deref() != Some("foreign"));
            assert!(edge.prerequisite_task_ids.is_empty());
            assert!(edge.dependent_task_ids.is_empty());
        }
    }
    #[test]
    fn order_does_not_depend_on_sqlite_row_order() {
        let tasks = [
            task("a", "alpha", "ready", Some(10)),
            task("b", "beta", "ready", Some(20)),
            task("c", "gamma", "ready", Some(30)),
        ];
        let edges = [edge("b", "a"), edge("c", "b")];
        let expected = analyze_handoffs(1, &tasks, &edges, &req(None, false)).unwrap();
        let mut reordered = tasks.to_vec();
        reordered.reverse();
        let mut reversed = edges.to_vec();
        reversed.reverse();
        let actual = analyze_handoffs(1, &reordered, &reversed, &req(None, false)).unwrap();
        assert_eq!(expected, actual);
    }
    #[test]
    fn chain_reaches_multiple_downstream_projects() {
        let tasks = [
            task("a", "alpha", "ready", None),
            task("b", "beta", "ready", None),
            task("c", "gamma", "ready", None),
            task("d", "delta", "ready", None),
        ];
        let edges = [edge("b", "a"), edge("c", "b"), edge("d", "c")];
        let result = analyze_handoffs(1, &tasks, &edges, &req(None, false)).unwrap();
        let alpha = result
            .projects
            .iter()
            .find(|p| p.project_id.as_deref() == Some("alpha"))
            .unwrap();
        assert_eq!(alpha.downstream_project_count, 3);
        assert_eq!(alpha.serves_projects, 1);
    }
    #[test]
    fn missing_estimates_do_not_cause_negative_total() {
        let tasks = [
            task("a", "alpha", "ready", None),
            task("b", "beta", "ready", Some(-10)),
            task("c", "beta", "ready", Some(30)),
        ];
        let result = analyze_handoffs(
            1,
            &tasks,
            &[edge("b", "a"), edge("c", "a")],
            &req(None, false),
        )
        .unwrap();
        assert_eq!(result.edges[0].dependent_estimated_minutes, 30);
        assert_eq!(result.edges[0].unknown_estimate_tasks, 1);
    }
}

#[cfg(test)]
mod serialized_contract_tests {
    use super::*;
    fn task(id: &str, project: &str) -> TaskRecord {
        TaskRecord {
            id: id.into(),
            title: format!("title:{id}"),
            status: "ready".into(),
            project_id: Some(project.into()),
            milestone_id: None,
            estimated_minutes: Some(10),
            earliest_start_at_ms: None,
            deadline_at_ms: None,
        }
    }
    #[test]
    fn scoped_round_trip_does_not_disclose_partner_project_identity() {
        let tasks = [
            task("alpha", "allowed"),
            task("beta", "confidential"),
            task("gamma", "allowed"),
        ];
        let edges = [
            TaskDependencyEdge {
                task_id: "beta".into(),
                prerequisite_id: "alpha".into(),
            },
            TaskDependencyEdge {
                task_id: "gamma".into(),
                prerequisite_id: "beta".into(),
            },
        ];
        let result = analyze_handoffs(
            3,
            &tasks,
            &edges,
            &HandoffRequest {
                now_ms: 100,
                project_id: Some("allowed".into()),
                include_completed: false,
            },
        )
        .unwrap();
        assert!(!result.round_trip_projects.is_empty());
        let encoded = serde_json::to_string(&result).unwrap();
        assert!(!encoded.contains("confidential"));
        assert!(!encoded.contains("title:beta"));
        assert!(!encoded.contains("\"beta\""));
    }
    #[test]
    fn full_report_retains_task_ids_for_authorized_workspace_views() {
        let tasks = [task("dependent", "app"), task("prerequisite", "core")];
        let edges = [TaskDependencyEdge {
            task_id: "dependent".into(),
            prerequisite_id: "prerequisite".into(),
        }];
        let result = analyze_handoffs(
            3,
            &tasks,
            &edges,
            &HandoffRequest {
                now_ms: 100,
                project_id: None,
                include_completed: false,
            },
        )
        .unwrap();
        assert_eq!(result.edges[0].prerequisite_task_ids, ["prerequisite"]);
        assert_eq!(result.edges[0].dependent_task_ids, ["dependent"]);
    }
    #[test]
    fn read_only_contract_preserves_inputs() {
        let tasks = [task("dependent", "app"), task("prerequisite", "core")];
        let edges = [TaskDependencyEdge {
            task_id: "dependent".into(),
            prerequisite_id: "prerequisite".into(),
        }];
        let snapshot = tasks.clone();
        let edges_before = edges.clone();
        let request = HandoffRequest {
            now_ms: 100,
            project_id: None,
            include_completed: false,
        };
        let _ = analyze_handoffs(3, &tasks, &edges, &request).unwrap();
        assert_eq!(tasks, snapshot);
        assert_eq!(edges, edges_before);
    }
}
