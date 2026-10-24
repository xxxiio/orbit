//! Explainable, accepted-state portfolio recommendations across project boundaries.
//!
//! Scores are *advisory*: no operation here mutates tasks or authorizes an agent
//! to act. Scoped requests never disclose identifiers of foreign tasks/projects.
use crate::{
    domain_commands::TaskRecord,
    task_dependencies::{TaskDependencyEdge, evaluate_readiness},
    task_graph::analyze_task_graph,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PortfolioActionRequest {
    pub now_ms: i64,
    #[serde(default)]
    pub project_id: Option<String>,
    #[serde(default = "default_limit")]
    pub limit: usize,
    #[serde(default)]
    pub include_unassigned: bool,
}
fn default_limit() -> usize {
    20
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PortfolioAction {
    pub task_id: String,
    pub title: String,
    pub project_id: Option<String>,
    pub milestone_id: Option<String>,
    pub rank: usize,
    pub deadline_at_ms: Option<i64>,
    pub estimated_minutes: Option<i64>,
    pub overdue: bool,
    /// Number of unfinished tasks downstream of this action, including indirect descendants.
    pub unfinished_downstream: usize,
    pub same_project_downstream: usize,
    /// Opaque count only in scoped reports. Foreign IDs are never included.
    pub other_project_downstream: usize,
    pub downstream_project_count: usize,
    pub immediately_unblocked: usize,
    pub known_downstream_minutes: i64,
    pub unknown_downstream_estimates: usize,
    /// Only includes visible descendants when a project filter is supplied.
    pub visible_downstream_task_ids: Vec<String>,
    pub reasons: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PortfolioActionReport {
    pub revision: i64,
    pub evaluated_at_ms: i64,
    pub project_filter: Option<String>,
    pub actionable_total: usize,
    pub blocked_total: usize,
    pub recommended: Vec<PortfolioAction>,
    pub warnings: Vec<String>,
}

fn unfinished(task: &TaskRecord) -> bool {
    task.status != "done" && task.status != "cancelled"
}

/// Sort precedence: overdue first, immediate unlocks, total unfinished
/// descendants, cross-project breadth, upcoming deadline and stable task ID.
/// Counting descendants is intentionally not additive to 'immediately_unblocked':
/// diamond paths are deduplicated by the graph model.
pub fn recommend_portfolio_actions(
    revision: i64,
    tasks: &[TaskRecord],
    edges: &[TaskDependencyEdge],
    request: &PortfolioActionRequest,
) -> Result<PortfolioActionReport, &'static str> {
    if revision < 0 || request.now_ms < 0 {
        return Err("negative revision or evaluation time");
    }
    if request.limit > 10_000 {
        return Err("portfolio recommendation limit exceeds safety bound");
    }
    if request
        .project_id
        .as_ref()
        .is_some_and(|id| id.trim().is_empty())
    {
        return Err("empty project identifier");
    }
    let readiness = evaluate_readiness(revision, tasks, edges, request.now_ms)?;
    let graph = analyze_task_graph(revision, tasks, edges, request.now_ms)?;
    let by_id: BTreeMap<&str, &TaskRecord> =
        tasks.iter().map(|task| (task.id.as_str(), task)).collect();
    let nodes: BTreeMap<&str, _> = graph
        .nodes
        .iter()
        .map(|node| (node.task_id.as_str(), node))
        .collect();
    let ready: BTreeMap<&str, _> = readiness
        .tasks
        .iter()
        .map(|row| (row.task_id.as_str(), row))
        .collect();
    let visible = |task: &TaskRecord| match &request.project_id {
        Some(project) => task.project_id.as_deref() == Some(project.as_str()),
        None => request.include_unassigned || task.project_id.is_some(),
    };
    let blocked_total = readiness
        .tasks
        .iter()
        .filter(|row| !row.actionable)
        .filter(|row| {
            by_id
                .get(row.task_id.as_str())
                .is_some_and(|task| visible(task) && unfinished(task))
        })
        .count();
    let mut actions = Vec::new();
    for task in tasks {
        if !visible(task) || !unfinished(task) {
            continue;
        }
        let Some(status) = ready.get(task.id.as_str()) else {
            return Err("readiness row missing");
        };
        if !status.actionable {
            continue;
        }
        let node = nodes
            .get(task.id.as_str())
            .ok_or("task graph row missing")?;
        let descendants: Vec<&TaskRecord> = node
            .all_dependents
            .iter()
            .filter_map(|id| by_id.get(id.as_str()).copied())
            .filter(|child| unfinished(child))
            .collect();
        let same_project_downstream = descendants
            .iter()
            .filter(|child| child.project_id == task.project_id)
            .count();
        let other_project_downstream = descendants.len() - same_project_downstream;
        let downstream_project_count = descendants
            .iter()
            .filter(|child| child.project_id != task.project_id)
            .map(|child| child.project_id.clone())
            .collect::<BTreeSet<_>>()
            .len();
        let visible_downstream_task_ids: Vec<String> = descendants
            .iter()
            .filter(|child| visible(child))
            .map(|child| child.id.clone())
            .collect();
        let known_downstream_minutes = descendants
            .iter()
            .filter_map(|child| child.estimated_minutes)
            .fold(0i64, |sum, minutes| sum.saturating_add(minutes.max(0)));
        let unknown_downstream_estimates = descendants
            .iter()
            .filter(|child| child.estimated_minutes.is_none())
            .count();
        // An immediate unlock must have *all* other unfinished prerequisites
        // already complete; otherwise this action is only a partial unblock.
        let immediately_unblocked = descendants
            .iter()
            .filter(|child| {
                edges
                    .iter()
                    .any(|edge| edge.task_id == child.id && edge.prerequisite_id == task.id)
                    && edges
                        .iter()
                        .filter(|edge| edge.task_id == child.id && edge.prerequisite_id != task.id)
                        .all(|edge| {
                            by_id
                                .get(edge.prerequisite_id.as_str())
                                .is_some_and(|pre| pre.status == "done")
                        })
                    && child
                        .earliest_start_at_ms
                        .is_none_or(|start| start <= request.now_ms)
                    && child.status == "ready"
            })
            .count();
        let overdue = task
            .deadline_at_ms
            .is_some_and(|deadline| deadline < request.now_ms);
        let mut reasons =
            vec!["All accepted prerequisites are complete; task is actionable".to_string()];
        if overdue {
            reasons.push("Task is overdue".into());
        }
        if immediately_unblocked > 0 {
            reasons.push(format!(
                "Completing this task would immediately unblock {immediately_unblocked} task(s)"
            ));
        }
        if other_project_downstream > 0 {
            reasons.push(format!("Downstream impact reaches {other_project_downstream} task(s) in {downstream_project_count} other project(s)"));
        }
        if same_project_downstream > 0 {
            reasons.push(format!(
                "There are {same_project_downstream} unfinished downstream tasks in this project"
            ));
        }
        if unknown_downstream_estimates > 0 {
            reasons.push(format!(
                "{unknown_downstream_estimates} downstream task(s) have no effort estimate"
            ));
        }
        actions.push(PortfolioAction {
            task_id: task.id.clone(),
            title: task.title.clone(),
            project_id: task.project_id.clone(),
            milestone_id: task.milestone_id.clone(),
            rank: 0,
            deadline_at_ms: task.deadline_at_ms,
            estimated_minutes: task.estimated_minutes,
            overdue,
            unfinished_downstream: descendants.len(),
            same_project_downstream,
            other_project_downstream,
            downstream_project_count,
            immediately_unblocked,
            known_downstream_minutes,
            unknown_downstream_estimates,
            visible_downstream_task_ids,
            reasons,
        });
    }
    actions.sort_by(|a, b| {
        b.overdue
            .cmp(&a.overdue)
            .then_with(|| b.immediately_unblocked.cmp(&a.immediately_unblocked))
            .then_with(|| b.unfinished_downstream.cmp(&a.unfinished_downstream))
            .then_with(|| b.downstream_project_count.cmp(&a.downstream_project_count))
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
    actions.truncate(request.limit);
    for (index, action) in actions.iter_mut().enumerate() {
        action.rank = index + 1;
    }
    let mut warnings = Vec::new();
    if request.project_id.is_some() {
        warnings.push("Foreign project task identifiers and titles are excluded; aggregate cross-project impact is not an access grant".into());
    }
    Ok(PortfolioActionReport {
        revision,
        evaluated_at_ms: request.now_ms,
        project_filter: request.project_id.clone(),
        actionable_total,
        blocked_total,
        recommended: actions,
        warnings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn task(id: &str, project: &str, status: &str) -> TaskRecord {
        TaskRecord {
            id: id.into(),
            title: format!("title {id}"),
            status: status.into(),
            project_id: Some(project.into()),
            milestone_id: None,
            estimated_minutes: Some(15),
            earliest_start_at_ms: None,
            deadline_at_ms: None,
            description: String::new(),
            priority: None,
            labels: Vec::new(),
        }
    }
    fn edge(t: &str, p: &str) -> TaskDependencyEdge {
        TaskDependencyEdge {
            task_id: t.into(),
            prerequisite_id: p.into(),
        }
    }
    fn req(project: Option<&str>) -> PortfolioActionRequest {
        PortfolioActionRequest {
            now_ms: 100,
            project_id: project.map(str::to_owned),
            limit: 20,
            include_unassigned: false,
        }
    }
    #[test]
    fn prioritizes_cross_project_unlocks_and_deduplicates_diamonds() {
        let tasks = vec![
            task("root", "platform", "ready"),
            task("other", "platform", "ready"),
            task("web", "web", "ready"),
            task("mobile", "mobile", "ready"),
            task("shared", "web", "ready"),
        ];
        let edges = vec![
            edge("web", "root"),
            edge("mobile", "root"),
            edge("shared", "web"),
            edge("shared", "mobile"),
        ];
        let report = recommend_portfolio_actions(4, &tasks, &edges, &req(None)).unwrap();
        assert_eq!(report.recommended[0].task_id, "root");
        assert_eq!(report.recommended[0].unfinished_downstream, 3);
        assert_eq!(report.recommended[0].downstream_project_count, 2);
        assert_eq!(report.recommended[0].immediately_unblocked, 2);
        assert_eq!(report.recommended[0].known_downstream_minutes, 45);
    }
    #[test]
    fn scoped_response_never_leaks_foreign_task_names_or_ids() {
        let tasks = vec![
            task("platform", "platform", "ready"),
            task("secret_web", "secret", "planned"),
        ];
        let report = recommend_portfolio_actions(
            2,
            &tasks,
            &[edge("secret_web", "platform")],
            &req(Some("platform")),
        )
        .unwrap();
        let json = serde_json::to_string(&report).unwrap();
        assert!(!json.contains("secret_web"));
        assert!(!json.contains("secret"));
        assert_eq!(report.recommended[0].other_project_downstream, 1);
    }
    #[test]
    fn cannot_claim_immediate_unlock_if_another_prerequisite_is_open() {
        let tasks = vec![
            task("a", "p", "ready"),
            task("b", "p", "ready"),
            task("c", "q", "ready"),
        ];
        let result =
            recommend_portfolio_actions(1, &tasks, &[edge("c", "a"), edge("c", "b")], &req(None))
                .unwrap();
        assert_eq!(result.recommended[0].immediately_unblocked, 0);
    }
    #[test]
    fn completed_descendants_do_not_count_as_bottlenecks() {
        let tasks = vec![task("a", "p", "ready"), task("c", "q", "done")];
        let result = recommend_portfolio_actions(1, &tasks, &[edge("c", "a")], &req(None)).unwrap();
        assert_eq!(result.recommended[0].unfinished_downstream, 0);
    }
    #[test]
    fn source_order_is_immaterial() {
        let mut tasks = vec![
            task("a", "p", "ready"),
            task("b", "p", "ready"),
            task("c", "q", "ready"),
        ];
        let mut edges = vec![edge("c", "b")];
        let first = recommend_portfolio_actions(1, &tasks, &edges, &req(None)).unwrap();
        tasks.reverse();
        edges.reverse();
        let second = recommend_portfolio_actions(1, &tasks, &edges, &req(None)).unwrap();
        assert_eq!(first, second);
    }
    #[test]
    fn rejects_invalid_requests_and_limits() {
        let mut r = req(None);
        r.limit = 10_001;
        assert!(recommend_portfolio_actions(1, &[], &[], &r).is_err());
        r.limit = 1;
        r.project_id = Some("  ".into());
        assert!(recommend_portfolio_actions(1, &[], &[], &r).is_err());
        r.project_id = None;
        r.now_ms = -1;
        assert!(recommend_portfolio_actions(1, &[], &[], &r).is_err());
    }
    #[test]
    fn upcoming_deadline_and_stable_task_id_tiebreak() {
        let mut a = task("a", "p", "ready");
        a.deadline_at_ms = Some(500);
        let b = task("b", "p", "ready");
        let first =
            recommend_portfolio_actions(1, &[b.clone(), a.clone()], &[], &req(None)).unwrap();
        assert_eq!(first.recommended[0].task_id, "a");
        a.deadline_at_ms = Some(50);
        let second = recommend_portfolio_actions(1, &[a, b], &[], &req(None)).unwrap();
        assert!(second.recommended[0].overdue);
    }
    #[test]
    fn bound_and_zero_limit_return_consistent_total() {
        let tasks = vec![task("a", "p", "ready"), task("b", "p", "ready")];
        let mut r = req(None);
        r.limit = 0;
        let result = recommend_portfolio_actions(1, &tasks, &[], &r).unwrap();
        assert_eq!(result.actionable_total, 2);
        assert!(result.recommended.is_empty());
    }
}

#[cfg(test)]
mod portfolio_edge_cases {
    use super::*;
    fn task(id: &str, project: Option<&str>, status: &str, estimate: Option<i64>) -> TaskRecord {
        TaskRecord {
            id: id.into(),
            title: format!("Task {id}"),
            status: status.into(),
            project_id: project.map(str::to_owned),
            milestone_id: Some("m1".into()),
            estimated_minutes: estimate,
            earliest_start_at_ms: None,
            deadline_at_ms: None,
            description: String::new(),
            priority: None,
            labels: Vec::new(),
        }
    }
    fn edge(dependent: &str, pre: &str) -> TaskDependencyEdge {
        TaskDependencyEdge {
            task_id: dependent.into(),
            prerequisite_id: pre.into(),
        }
    }
    fn request(project: Option<&str>) -> PortfolioActionRequest {
        PortfolioActionRequest {
            now_ms: 100,
            project_id: project.map(str::to_owned),
            limit: 20,
            include_unassigned: false,
        }
    }
    #[test]
    fn inactive_tasks_are_never_returned_as_actionable_recommendations() {
        for status in [
            "inbox",
            "planned",
            "waiting",
            "blocked",
            "deferred",
            "in_progress",
            "review",
            "done",
            "cancelled",
        ] {
            let report = recommend_portfolio_actions(
                1,
                &[task("a", Some("p"), status, Some(10))],
                &[],
                &request(None),
            )
            .unwrap();
            assert!(
                report.recommended.is_empty(),
                "unexpected recommendation for {status}"
            );
        }
    }
    #[test]
    fn pending_prerequisites_take_precedence_over_ready_status_labels() {
        let tasks = vec![
            task("pre", Some("p"), "ready", Some(10)),
            task("dependent", Some("p"), "ready", Some(10)),
        ];
        let report =
            recommend_portfolio_actions(1, &tasks, &[edge("dependent", "pre")], &request(None))
                .unwrap();
        assert_eq!(report.actionable_total, 1);
        assert_eq!(report.blocked_total, 1);
        assert_eq!(report.recommended[0].task_id, "pre");
    }
    #[test]
    fn only_completed_other_prerequisites_make_an_immediate_unlock() {
        let tasks = vec![
            task("a", Some("p"), "ready", Some(10)),
            task("finished", Some("p"), "done", Some(10)),
            task("dependent", Some("p"), "ready", Some(10)),
        ];
        let report = recommend_portfolio_actions(
            1,
            &tasks,
            &[edge("dependent", "a"), edge("dependent", "finished")],
            &request(None),
        )
        .unwrap();
        assert_eq!(report.recommended[0].immediately_unblocked, 1);
        assert_eq!(report.recommended[0].unfinished_downstream, 1);
    }
    #[test]
    fn earliest_start_in_future_is_not_immediately_unblocked() {
        let mut dependent = task("dependent", Some("p"), "ready", Some(10));
        dependent.earliest_start_at_ms = Some(101);
        let tasks = vec![task("pre", Some("p"), "ready", Some(10)), dependent];
        let report =
            recommend_portfolio_actions(1, &tasks, &[edge("dependent", "pre")], &request(None))
                .unwrap();
        assert_eq!(report.recommended[0].immediately_unblocked, 0);
        assert_eq!(report.recommended[0].unfinished_downstream, 1);
    }
    #[test]
    fn planned_downstream_is_long_term_impact_not_immediate_unlock() {
        let tasks = vec![
            task("pre", Some("p"), "ready", Some(10)),
            task("planned", Some("q"), "planned", Some(10)),
        ];
        let report =
            recommend_portfolio_actions(1, &tasks, &[edge("planned", "pre")], &request(None))
                .unwrap();
        assert_eq!(report.recommended[0].other_project_downstream, 1);
        assert_eq!(report.recommended[0].immediately_unblocked, 0);
    }
    #[test]
    fn cancelled_descendants_are_not_pending_impact() {
        let tasks = vec![
            task("pre", Some("p"), "ready", Some(10)),
            task("cancelled", Some("q"), "cancelled", Some(10)),
        ];
        let report =
            recommend_portfolio_actions(1, &tasks, &[edge("cancelled", "pre")], &request(None))
                .unwrap();
        assert_eq!(report.recommended[0].unfinished_downstream, 0);
        assert_eq!(report.recommended[0].other_project_downstream, 0);
    }
    #[test]
    fn missing_effort_is_reported_separately_from_known_minutes() {
        let tasks = vec![
            task("a", Some("p"), "ready", Some(5)),
            task("b", Some("q"), "ready", None),
            task("c", Some("q"), "ready", Some(25)),
        ];
        let report = recommend_portfolio_actions(
            1,
            &tasks,
            &[edge("b", "a"), edge("c", "b")],
            &request(None),
        )
        .unwrap();
        let top = &report.recommended[0];
        assert_eq!(top.known_downstream_minutes, 25);
        assert_eq!(top.unknown_downstream_estimates, 1);
        assert!(
            top.reasons
                .iter()
                .any(|reason| reason.contains("no effort estimate"))
        );
    }
    #[test]
    fn unassigned_tasks_are_explicitly_opt_in_for_global_views() {
        let tasks = vec![
            task("assigned", Some("p"), "ready", Some(10)),
            task("unassigned", None, "ready", Some(10)),
        ];
        let default = recommend_portfolio_actions(1, &tasks, &[], &request(None)).unwrap();
        assert_eq!(default.actionable_total, 1);
        let mut all = request(None);
        all.include_unassigned = true;
        let result = recommend_portfolio_actions(1, &tasks, &[], &all).unwrap();
        assert_eq!(result.actionable_total, 2);
        let scoped = recommend_portfolio_actions(1, &tasks, &[], &request(Some("p"))).unwrap();
        assert_eq!(scoped.actionable_total, 1);
    }
    #[test]
    fn fail_closed_on_dangling_and_cyclic_dependencies() {
        let tasks = vec![
            task("a", Some("p"), "ready", None),
            task("b", Some("p"), "ready", None),
        ];
        assert!(
            recommend_portfolio_actions(1, &tasks, &[edge("a", "missing")], &request(None))
                .is_err()
        );
        assert!(
            recommend_portfolio_actions(
                1,
                &tasks,
                &[edge("a", "b"), edge("b", "a")],
                &request(None)
            )
            .is_err()
        );
    }
    #[test]
    fn scoped_outgoing_impact_is_stable_after_foreign_title_changes() {
        let mut tasks = vec![
            task("a", Some("p"), "ready", Some(10)),
            task("b", Some("secret"), "ready", Some(10)),
        ];
        let edges = vec![edge("b", "a")];
        let first = recommend_portfolio_actions(1, &tasks, &edges, &request(Some("p"))).unwrap();
        tasks[1].title = "different foreign title".into();
        let second = recommend_portfolio_actions(1, &tasks, &edges, &request(Some("p"))).unwrap();
        assert_eq!(first, second);
        assert!(!serde_json::to_string(&first).unwrap().contains("secret"));
    }
    #[test]
    fn large_downstream_effort_uses_saturating_arithmetic() {
        let tasks = vec![
            task("root", Some("p"), "ready", Some(1)),
            task("a", Some("p"), "ready", Some(i64::MAX)),
            task("b", Some("p"), "ready", Some(i64::MAX)),
        ];
        let result = recommend_portfolio_actions(
            1,
            &tasks,
            &[edge("a", "root"), edge("b", "root")],
            &request(None),
        )
        .unwrap();
        assert_eq!(result.recommended[0].known_downstream_minutes, i64::MAX);
    }
    #[test]
    fn project_filter_never_returns_foreign_actionable_tasks() {
        let tasks = vec![
            task("one", Some("p"), "ready", Some(1)),
            task("two", Some("q"), "ready", Some(1)),
            task("three", None, "ready", Some(1)),
        ];
        let report = recommend_portfolio_actions(1, &tasks, &[], &request(Some("p"))).unwrap();
        assert_eq!(report.recommended.len(), 1);
        assert_eq!(report.recommended[0].task_id, "one");
        assert!(report.recommended[0].visible_downstream_task_ids.is_empty());
    }
}

#[cfg(test)]
mod portfolio_wire_contract {
    use super::*;
    #[test]
    fn wire_request_defaults_are_documented_and_bounded() {
        let request: PortfolioActionRequest =
            serde_json::from_value(serde_json::json!({"now_ms":20})).unwrap();
        assert_eq!(request.limit, 20);
        assert!(request.project_id.is_none());
        assert!(!request.include_unassigned);
        let value = serde_json::to_value(request).unwrap();
        assert_eq!(value["limit"], 20);
        assert_eq!(value["include_unassigned"], false);
    }
}
