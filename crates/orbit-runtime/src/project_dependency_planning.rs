//! Accepted-state project dependency planning and cross-project boundary analysis.
//!
//! This module is deliberately deterministic and read-only. It never infers
//! acceptance from a queued connected command, never alters a task status,
//! and does not silently drop an inaccessible prerequisite from the graph.
use crate::domain_commands::TaskRecord;
use crate::task_dependencies::{TaskDependencyEdge, evaluate_readiness};
use crate::task_graph::analyze_task_graph;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectDependencyRequest {
    pub now_ms: i64,
    /// Restrict detailed task output to this project; blockers from other
    /// projects are summarized as opaque counters in the scoped view.
    #[serde(default)]
    pub project_id: Option<String>,
    #[serde(default)]
    pub include_unassigned: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectDependencySummary {
    pub project_id: Option<String>,
    pub total: usize,
    pub actionable: usize,
    pub blocked: usize,
    pub completed: usize,
    pub external_prerequisite_count: usize,
    pub external_blocked_task_count: usize,
    pub outgoing_dependency_count: usize,
    pub critical_task_ids: Vec<String>,
    pub ready_task_ids: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectDependencyBottleneck {
    pub task_id: String,
    pub project_id: Option<String>,
    pub waiting_direct: usize,
    pub waiting_transitive: usize,
    pub waiting_project_count: usize,
    pub cross_project_waiting: usize,
    pub pending_estimated_minutes: i64,
    pub missing_estimate_count: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectDependencyTask {
    pub task_id: String,
    pub project_id: Option<String>,
    pub milestone_id: Option<String>,
    pub title: String,
    pub actionable: bool,
    pub blocking_internal: Vec<String>,
    /// Count only: a scoped report must not expose foreign task identifiers.
    pub external_blocker_count: usize,
    pub downstream_in_project: usize,
    pub downstream_other_projects: usize,
    pub longest_prerequisite_chain: usize,
    pub deadline_at_ms: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectDependencyReport {
    pub revision: i64,
    pub evaluated_at_ms: i64,
    pub project_filter: Option<String>,
    pub summaries: Vec<ProjectDependencySummary>,
    pub tasks: Vec<ProjectDependencyTask>,
    pub bottlenecks: Vec<ProjectDependencyBottleneck>,
    pub cross_project_edge_count: usize,
    pub warnings: Vec<String>,
}

/// Analyze the complete graph and only then redact task-level details. A
/// caller must authorize its requested project before invoking this function.
/// Scoped reports omit individual foreign task identifiers and titles.
pub fn project_dependency_report(
    revision: i64,
    tasks: &[TaskRecord],
    edges: &[TaskDependencyEdge],
    request: &ProjectDependencyRequest,
) -> Result<ProjectDependencyReport, &'static str> {
    if request.now_ms < 0 || revision < 0 {
        return Err("invalid accepted revision or time");
    }
    if request
        .project_id
        .as_ref()
        .is_some_and(|id| id.trim().is_empty())
    {
        return Err("empty project identifier");
    }
    let graph = analyze_task_graph(revision, tasks, edges, request.now_ms)?;
    let readiness = evaluate_readiness(revision, tasks, edges, request.now_ms)?;
    let by_id: BTreeMap<&str, &TaskRecord> =
        tasks.iter().map(|task| (task.id.as_str(), task)).collect();
    let graph_by_id: BTreeMap<&str, _> = graph
        .nodes
        .iter()
        .map(|node| (node.task_id.as_str(), node))
        .collect();
    let ready_by_id: BTreeMap<&str, _> = readiness
        .tasks
        .iter()
        .map(|task| (task.task_id.as_str(), task))
        .collect();
    let visible = |task: &TaskRecord| {
        request.project_id.as_ref().map_or(
            request.include_unassigned || task.project_id.is_some(),
            |id| task.project_id.as_ref() == Some(id),
        )
    };

    let mut summaries: BTreeMap<Option<String>, ProjectDependencySummary> = BTreeMap::new();
    let mut task_rows = Vec::new();
    let mut cross_project_edge_count = 0;
    let mut external_prerequisites: BTreeMap<Option<String>, BTreeSet<&str>> = BTreeMap::new();
    let mut external_blocked: BTreeMap<Option<String>, BTreeSet<&str>> = BTreeMap::new();
    let mut outgoing: BTreeMap<Option<String>, usize> = BTreeMap::new();
    for edge in edges {
        let dependent = by_id[edge.task_id.as_str()];
        let prerequisite = by_id[edge.prerequisite_id.as_str()];
        if dependent.project_id != prerequisite.project_id {
            if visible(dependent) || visible(prerequisite) {
                cross_project_edge_count += 1;
            }
            if visible(dependent) {
                external_prerequisites
                    .entry(dependent.project_id.clone())
                    .or_default()
                    .insert(edge.prerequisite_id.as_str());
                if prerequisite.status != "done" {
                    external_blocked
                        .entry(dependent.project_id.clone())
                        .or_default()
                        .insert(edge.task_id.as_str());
                }
            }
            if visible(prerequisite) {
                *outgoing.entry(prerequisite.project_id.clone()).or_default() += 1;
            }
        }
    }
    for task in tasks {
        if !visible(task) {
            continue;
        }
        let node = graph_by_id[task.id.as_str()];
        let readiness = ready_by_id[task.id.as_str()];
        let key = task.project_id.clone();
        let summary = summaries
            .entry(key.clone())
            .or_insert_with(|| ProjectDependencySummary {
                project_id: key.clone(),
                total: 0,
                actionable: 0,
                blocked: 0,
                completed: 0,
                external_prerequisite_count: 0,
                external_blocked_task_count: 0,
                outgoing_dependency_count: 0,
                critical_task_ids: Vec::new(),
                ready_task_ids: Vec::new(),
            });
        summary.total += 1;
        if readiness.actionable {
            summary.actionable += 1;
            summary.ready_task_ids.push(task.id.clone());
        }
        if !readiness.blocking_task_ids.is_empty() {
            summary.blocked += 1;
        }
        if task.status == "done" {
            summary.completed += 1;
        }
        let blocking_internal = readiness
            .blocking_task_ids
            .iter()
            .filter(|id| by_id[id.as_str()].project_id == key)
            .cloned()
            .collect::<Vec<_>>();
        let external_blocker_count = readiness.blocking_task_ids.len() - blocking_internal.len();
        let downstream_in_project = node
            .all_dependents
            .iter()
            .filter(|id| by_id[id.as_str()].project_id == key)
            .count();
        let downstream_other_projects = node.all_dependents.len() - downstream_in_project;
        if task.status != "done" && !node.all_dependents.is_empty() {
            summary.critical_task_ids.push(task.id.clone());
        }
        task_rows.push(ProjectDependencyTask {
            task_id: task.id.clone(),
            project_id: key,
            milestone_id: task.milestone_id.clone(),
            title: task.title.clone(),
            actionable: readiness.actionable,
            blocking_internal,
            external_blocker_count,
            downstream_in_project,
            downstream_other_projects,
            longest_prerequisite_chain: node.prerequisite_depth,
            deadline_at_ms: task.deadline_at_ms,
        });
    }
    for (key, summary) in &mut summaries {
        summary.external_prerequisite_count =
            external_prerequisites.get(key).map_or(0, BTreeSet::len);
        summary.external_blocked_task_count = external_blocked.get(key).map_or(0, BTreeSet::len);
        summary.outgoing_dependency_count = *outgoing.get(key).unwrap_or(&0);
        summary.critical_task_ids.sort_by(|a, b| {
            graph_by_id[b.as_str()]
                .all_dependents
                .len()
                .cmp(&graph_by_id[a.as_str()].all_dependents.len())
                .then_with(|| a.cmp(b))
        });
        summary.ready_task_ids.sort();
    }
    let mut bottlenecks = Vec::new();
    for task in tasks
        .iter()
        .filter(|task| visible(task) && task.status != "done")
    {
        let node = graph_by_id[task.id.as_str()];
        let waiting = node
            .all_dependents
            .iter()
            .filter_map(|id| {
                let dependent = by_id[id.as_str()];
                (dependent.status != "done").then_some(dependent)
            })
            .collect::<Vec<_>>();
        if waiting.is_empty() {
            continue;
        }
        let mut projects = BTreeSet::new();
        let mut total_minutes = 0_i64;
        let mut missing_estimate_count = 0;
        let mut cross_project_waiting = 0;
        for dependent in &waiting {
            projects.insert(dependent.project_id.clone());
            if dependent.project_id != task.project_id {
                cross_project_waiting += 1;
            }
            match dependent.estimated_minutes {
                Some(minutes) if minutes >= 0 => {
                    total_minutes = total_minutes.saturating_add(minutes);
                }
                _ => missing_estimate_count += 1,
            }
        }
        bottlenecks.push(ProjectDependencyBottleneck {
            task_id: task.id.clone(),
            project_id: task.project_id.clone(),
            waiting_direct: node
                .direct_dependents
                .iter()
                .filter(|id| by_id[id.as_str()].status != "done")
                .count(),
            waiting_transitive: waiting.len(),
            waiting_project_count: projects.len(),
            cross_project_waiting,
            pending_estimated_minutes: total_minutes,
            missing_estimate_count,
        });
    }
    bottlenecks.sort_by(|a, b| {
        b.waiting_transitive
            .cmp(&a.waiting_transitive)
            .then_with(|| b.cross_project_waiting.cmp(&a.cross_project_waiting))
            .then_with(|| a.task_id.cmp(&b.task_id))
    });
    task_rows.sort_by(|a, b| {
        a.project_id
            .cmp(&b.project_id)
            .then_with(|| a.task_id.cmp(&b.task_id))
    });
    let warnings = if request.project_id.is_some()
        && task_rows.iter().any(|task| task.external_blocker_count > 0)
    {
        vec!["Some blockers belong to another project; foreign task details are omitted".to_owned()]
    } else {
        Vec::new()
    };
    Ok(ProjectDependencyReport {
        revision,
        evaluated_at_ms: request.now_ms,
        project_filter: request.project_id.clone(),
        summaries: summaries.into_values().collect(),
        tasks: task_rows,
        bottlenecks,
        cross_project_edge_count,
        warnings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn task(id: &str, project: Option<&str>, status: &str, minutes: Option<i64>) -> TaskRecord {
        TaskRecord {
            id: id.into(),
            title: format!("title {id}"),
            status: status.into(),
            project_id: project.map(str::to_owned),
            milestone_id: Some("milestone".into()),
            estimated_minutes: minutes,
            earliest_start_at_ms: None,
            deadline_at_ms: None,
            description: String::new(),
            priority: None,
            labels: Vec::new(),
        }
    }
    fn edge(task: &str, requires: &str) -> TaskDependencyEdge {
        TaskDependencyEdge {
            task_id: task.into(),
            prerequisite_id: requires.into(),
        }
    }
    fn fixture() -> (Vec<TaskRecord>, Vec<TaskDependencyEdge>) {
        (
            vec![
                task("foundation", Some("platform"), "ready", Some(20)),
                task("api", Some("app"), "ready", Some(40)),
                task("ui", Some("app"), "ready", Some(30)),
                task("docs", Some("docs"), "ready", None),
                task("released", Some("app"), "done", Some(10)),
            ],
            vec![
                edge("api", "foundation"),
                edge("ui", "api"),
                edge("docs", "foundation"),
            ],
        )
    }
    fn request(project: Option<&str>) -> ProjectDependencyRequest {
        ProjectDependencyRequest {
            now_ms: 1000,
            project_id: project.map(str::to_owned),
            include_unassigned: true,
        }
    }
    #[test]
    fn reports_cross_project_dependencies_and_transitive_bottlenecks() {
        let (tasks, edges) = fixture();
        let result = project_dependency_report(5, &tasks, &edges, &request(None)).unwrap();
        assert_eq!(result.cross_project_edge_count, 2);
        assert_eq!(result.summaries.len(), 3);
        let platform = result
            .summaries
            .iter()
            .find(|s| s.project_id.as_deref() == Some("platform"))
            .unwrap();
        assert_eq!(platform.outgoing_dependency_count, 2);
        let app = result
            .summaries
            .iter()
            .find(|s| s.project_id.as_deref() == Some("app"))
            .unwrap();
        assert_eq!(app.external_prerequisite_count, 1);
        assert_eq!(app.external_blocked_task_count, 1);
        let bottleneck = &result.bottlenecks[0];
        assert_eq!(bottleneck.task_id, "foundation");
        assert_eq!(bottleneck.waiting_transitive, 3);
        assert_eq!(bottleneck.waiting_project_count, 2);
        assert_eq!(bottleneck.missing_estimate_count, 1);
        assert_eq!(bottleneck.pending_estimated_minutes, 70);
    }
    #[test]
    fn project_scope_does_not_disclose_foreign_task_titles_or_identifiers() {
        let (tasks, edges) = fixture();
        let result = project_dependency_report(5, &tasks, &edges, &request(Some("app"))).unwrap();
        assert_eq!(result.tasks.len(), 3);
        assert_eq!(result.summaries.len(), 1);
        let api = result
            .tasks
            .iter()
            .find(|task| task.task_id == "api")
            .unwrap();
        assert!(api.blocking_internal.is_empty());
        assert_eq!(api.external_blocker_count, 1);
        assert_eq!(
            result
                .bottlenecks
                .iter()
                .filter(|b| b.task_id == "foundation")
                .count(),
            0
        );
        let serialized = serde_json::to_string(&result).unwrap();
        assert!(!serialized.contains("foundation"));
        assert!(!serialized.contains("title docs"));
        assert!(!result.warnings.is_empty());
    }
    #[test]
    fn accepted_completion_unblocks_project_without_mutations() {
        let (mut tasks, edges) = fixture();
        let before = project_dependency_report(1, &tasks, &edges, &request(Some("app"))).unwrap();
        assert!(
            !before
                .tasks
                .iter()
                .any(|task| task.task_id == "api" && task.actionable)
        );
        tasks
            .iter_mut()
            .find(|task| task.id == "foundation")
            .unwrap()
            .status = "done".into();
        let after = project_dependency_report(2, &tasks, &edges, &request(Some("app"))).unwrap();
        assert!(
            after
                .tasks
                .iter()
                .any(|task| task.task_id == "api" && task.actionable)
        );
    }
    #[test]
    fn rejects_cycles_missing_tasks_duplicate_edges_and_bad_requests() {
        let (tasks, edges) = fixture();
        let mut bad = request(None);
        bad.now_ms = -1;
        assert!(project_dependency_report(1, &tasks, &edges, &bad).is_err());
        bad = request(Some(" "));
        assert!(project_dependency_report(1, &tasks, &edges, &bad).is_err());
        assert!(
            project_dependency_report(
                1,
                &tasks,
                &[edge("api", "ui"), edge("ui", "api")],
                &request(None)
            )
            .is_err()
        );
        assert!(
            project_dependency_report(1, &tasks, &[edge("api", "absent")], &request(None)).is_err()
        );
        let mut duplicated = edges.clone();
        duplicated.push(edges[0].clone());
        assert!(project_dependency_report(1, &tasks, &duplicated, &request(None)).is_err());
    }
    #[test]
    fn stable_with_reordered_tasks_and_edges() {
        let (tasks, edges) = fixture();
        let expected = project_dependency_report(1, &tasks, &edges, &request(None)).unwrap();
        let mut tasks = tasks;
        tasks.reverse();
        let mut edges = edges;
        edges.reverse();
        let actual = project_dependency_report(1, &tasks, &edges, &request(None)).unwrap();
        assert_eq!(expected, actual);
    }
    #[test]
    fn project_without_tasks_is_empty_but_does_not_fail() {
        let (tasks, edges) = fixture();
        let report = project_dependency_report(0, &tasks, &edges, &request(Some("empty"))).unwrap();
        assert!(report.tasks.is_empty());
        assert!(report.summaries.is_empty());
    }
    #[test]
    fn unassigned_scope_is_opt_in_for_unfiltered_report() {
        let (mut tasks, edges) = fixture();
        tasks.push(task("misc", None, "ready", None));
        let mut r = request(None);
        r.include_unassigned = false;
        let result = project_dependency_report(0, &tasks, &edges, &r).unwrap();
        assert!(!result.tasks.iter().any(|task| task.task_id == "misc"));
        r.include_unassigned = true;
        let result = project_dependency_report(0, &tasks, &edges, &r).unwrap();
        assert!(result.tasks.iter().any(|task| task.task_id == "misc"));
    }
}

#[cfg(test)]
mod additional_contract_tests {
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
            title: format!("internal {id}"),
            status: status.into(),
            project_id: project.map(str::to_owned),
            milestone_id: Some("milestone".into()),
            estimated_minutes: minutes,
            earliest_start_at_ms: None,
            deadline_at_ms: deadline,
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
    fn request(project: Option<&str>) -> ProjectDependencyRequest {
        ProjectDependencyRequest {
            now_ms: 100,
            project_id: project.map(str::to_owned),
            include_unassigned: true,
        }
    }
    #[test]
    fn diamond_graph_counts_one_transitive_dependent_once() {
        let tasks = [
            task("root", Some("a"), "ready", Some(5), None),
            task("left", Some("b"), "ready", Some(10), None),
            task("right", Some("b"), "ready", Some(20), None),
            task("leaf", Some("c"), "ready", Some(30), None),
        ];
        let edges = [
            edge("left", "root"),
            edge("right", "root"),
            edge("leaf", "left"),
            edge("leaf", "right"),
        ];
        let report = project_dependency_report(3, &tasks, &edges, &request(None)).unwrap();
        let root = report
            .bottlenecks
            .iter()
            .find(|row| row.task_id == "root")
            .unwrap();
        assert_eq!(root.waiting_transitive, 3);
        assert_eq!(root.waiting_direct, 2);
        assert_eq!(root.pending_estimated_minutes, 60);
        assert_eq!(root.waiting_project_count, 2);
        let leaf = report
            .tasks
            .iter()
            .find(|row| row.task_id == "leaf")
            .unwrap();
        assert_eq!(leaf.longest_prerequisite_chain, 2);
        assert_eq!(leaf.external_blocker_count, 2);
    }
    #[test]
    fn direct_blockers_are_not_replaced_by_transitive_ancestors() {
        let tasks = [
            task("a", Some("infra"), "ready", None, None),
            task("b", Some("backend"), "ready", None, None),
            task("c", Some("frontend"), "ready", None, None),
        ];
        let report = project_dependency_report(
            0,
            &tasks,
            &[edge("b", "a"), edge("c", "b")],
            &request(Some("frontend")),
        )
        .unwrap();
        let c = &report.tasks[0];
        assert_eq!(c.external_blocker_count, 1);
        assert!(c.blocking_internal.is_empty());
        assert_eq!(c.longest_prerequisite_chain, 2);
    }
    #[test]
    fn already_completed_dependents_do_not_enter_bottleneck_effort() {
        let tasks = [
            task("root", Some("x"), "ready", None, None),
            task("done", Some("x"), "done", Some(500), None),
            task("later", Some("x"), "ready", Some(30), None),
        ];
        let result = project_dependency_report(
            5,
            &tasks,
            &[edge("done", "root"), edge("later", "root")],
            &request(None),
        )
        .unwrap();
        let root = result
            .bottlenecks
            .iter()
            .find(|row| row.task_id == "root")
            .unwrap();
        assert_eq!(root.waiting_transitive, 1);
        assert_eq!(root.pending_estimated_minutes, 30);
        assert_eq!(root.waiting_direct, 1);
    }
    #[test]
    fn overdue_tasks_can_be_actionable_with_no_prerequisites() {
        let tasks = [task("overdue", Some("x"), "ready", Some(30), Some(2))];
        let result = project_dependency_report(5, &tasks, &[], &request(Some("x"))).unwrap();
        assert!(result.tasks[0].actionable);
        assert_eq!(result.summaries[0].ready_task_ids, ["overdue"]);
    }
    #[test]
    fn completed_prerequisites_have_no_blocker_count() {
        let tasks = [
            task("done", Some("x"), "done", Some(30), None),
            task("dependent", Some("y"), "ready", Some(30), None),
        ];
        let result =
            project_dependency_report(2, &tasks, &[edge("dependent", "done")], &request(Some("y")))
                .unwrap();
        assert_eq!(result.tasks[0].external_blocker_count, 0);
        assert!(result.tasks[0].actionable);
        assert_eq!(result.summaries[0].external_prerequisite_count, 1);
        assert_eq!(result.summaries[0].external_blocked_task_count, 0);
    }
    #[test]
    fn global_counts_are_not_leaked_into_scoped_report() {
        let tasks = [
            task("a", Some("allowed"), "ready", None, None),
            task("b", Some("external"), "ready", None, None),
            task("c", Some("third"), "ready", None, None),
            task("d", Some("fourth"), "ready", None, None),
        ];
        let edges = [edge("a", "b"), edge("c", "d")];
        let global = project_dependency_report(2, &tasks, &edges, &request(None)).unwrap();
        let scoped =
            project_dependency_report(2, &tasks, &edges, &request(Some("allowed"))).unwrap();
        assert_eq!(global.cross_project_edge_count, 2);
        assert_eq!(scoped.cross_project_edge_count, 1);
        assert!(
            scoped
                .tasks
                .iter()
                .all(|row| row.project_id.as_deref() == Some("allowed"))
        );
        let encoded = serde_json::to_string(&scoped).unwrap();
        assert!(!encoded.contains("internal b"));
        assert!(!encoded.contains("third"));
        assert!(!encoded.contains("fourth"));
    }
    #[test]
    fn negative_estimates_are_treated_as_unknown_not_negative_effort() {
        let tasks = [
            task("root", Some("a"), "ready", None, None),
            task("negative", Some("b"), "ready", Some(-5), None),
            task("positive", Some("b"), "ready", Some(15), None),
        ];
        let result = project_dependency_report(
            2,
            &tasks,
            &[edge("negative", "root"), edge("positive", "root")],
            &request(None),
        )
        .unwrap();
        let row = result
            .bottlenecks
            .iter()
            .find(|row| row.task_id == "root")
            .unwrap();
        assert_eq!(row.pending_estimated_minutes, 15);
        assert_eq!(row.missing_estimate_count, 1);
    }
    #[test]
    fn duplicate_task_id_is_rejected_before_projection() {
        let task = task("duplicate", Some("a"), "ready", None, None);
        assert!(project_dependency_report(1, &[task.clone(), task], &[], &request(None)).is_err());
    }
    #[test]
    fn no_edges_produces_no_bottlenecks() {
        let tasks = [
            task("a", Some("alpha"), "ready", None, None),
            task("b", Some("beta"), "waiting", None, None),
        ];
        let result = project_dependency_report(0, &tasks, &[], &request(None)).unwrap();
        assert!(result.bottlenecks.is_empty());
        assert_eq!(result.cross_project_edge_count, 0);
        assert_eq!(result.summaries.len(), 2);
        assert_eq!(result.summaries.iter().map(|s| s.blocked).sum::<usize>(), 0);
    }
    #[test]
    fn sorted_bottlenecks_use_stable_tiebreaks() {
        let tasks = [
            task("a", Some("a"), "ready", None, None),
            task("b", Some("a"), "ready", None, None),
            task("c", Some("a"), "ready", None, None),
            task("d", Some("a"), "ready", None, None),
        ];
        let report =
            project_dependency_report(0, &tasks, &[edge("c", "a"), edge("d", "b")], &request(None))
                .unwrap();
        assert_eq!(
            report
                .bottlenecks
                .iter()
                .map(|b| b.task_id.as_str())
                .collect::<Vec<_>>(),
            ["a", "b"]
        );
    }
    #[test]
    fn disconnected_unassigned_work_is_visible_only_on_request() {
        let tasks = [task("orphan", None, "ready", Some(20), None)];
        let mut req = request(None);
        req.include_unassigned = false;
        assert!(
            project_dependency_report(0, &tasks, &[], &req)
                .unwrap()
                .tasks
                .is_empty()
        );
        req.include_unassigned = true;
        let report = project_dependency_report(0, &tasks, &[], &req).unwrap();
        assert_eq!(report.tasks[0].project_id, None);
        assert_eq!(report.summaries[0].ready_task_ids, ["orphan"]);
    }
}

// This boundary remains pure so the same accepted snapshot can be evaluated
// locally, by a disconnected replica, or in the self-hosted server without
// transport-specific policy decisions or an extra derived-state database.
