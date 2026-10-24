//! Deterministic time-budget selection from portfolio-aware action rankings.
//!
//! This is a proposal, not a schedule reservation. It respects the same
//! accepted-state graph and project visibility as portfolio recommendations.
use crate::{
    domain_commands::TaskRecord,
    portfolio_actions::{
        PortfolioActionReport, PortfolioActionRequest, recommend_portfolio_actions,
    },
    task_dependencies::TaskDependencyEdge,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PortfolioCapacityRequest {
    pub now_ms: i64,
    pub available_minutes: i64,
    pub max_tasks: usize,
    #[serde(default)]
    pub project_id: Option<String>,
    #[serde(default)]
    pub include_unassigned: bool,
    #[serde(default)]
    pub include_unsized: bool,
    #[serde(default)]
    pub default_minutes: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PortfolioCapacityItem {
    pub task_id: String,
    pub title: String,
    pub project_id: Option<String>,
    pub minutes: i64,
    pub portfolio_rank: usize,
    pub immediate_unlocks: usize,
    pub cross_project_downstream: usize,
    pub rationale: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PortfolioCapacityExclusion {
    pub task_id: String,
    pub reason: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PortfolioCapacityPlan {
    pub revision: i64,
    pub evaluated_at_ms: i64,
    pub project_filter: Option<String>,
    pub capacity_minutes: i64,
    pub assigned_minutes: i64,
    pub unassigned_minutes: i64,
    pub actionable_total: usize,
    pub blocked_total: usize,
    pub chosen: Vec<PortfolioCapacityItem>,
    pub excluded: Vec<PortfolioCapacityExclusion>,
    pub warnings: Vec<String>,
}

/// Greedy bounded packing in portfolio rank order. The greedy policy is
/// deliberate and predictable: it won't reorder tasks purely to fill every
/// remaining minute and never treats a future unlock as currently actionable.
pub fn portfolio_capacity_plan(
    revision: i64,
    tasks: &[TaskRecord],
    edges: &[TaskDependencyEdge],
    request: &PortfolioCapacityRequest,
) -> Result<PortfolioCapacityPlan, &'static str> {
    if request.available_minutes < 0 {
        return Err("negative portfolio time budget");
    }
    if request.max_tasks > 10_000 {
        return Err("portfolio task count exceeds safety bound");
    }
    if request.default_minutes.is_some_and(|minutes| minutes <= 0) {
        return Err("default task duration must be positive");
    }
    let query = PortfolioActionRequest {
        now_ms: request.now_ms,
        project_id: request.project_id.clone(),
        limit: 10_000,
        include_unassigned: request.include_unassigned,
    };
    let PortfolioActionReport {
        actionable_total,
        blocked_total,
        recommended,
        warnings,
        ..
    } = recommend_portfolio_actions(revision, tasks, edges, &query)?;
    let mut chosen = Vec::new();
    let mut excluded = Vec::new();
    let mut remaining = request.available_minutes;
    for action in recommended {
        let minutes = match action.estimated_minutes {
            Some(value) if value > 0 => value,
            Some(_) => {
                excluded.push(PortfolioCapacityExclusion {
                    task_id: action.task_id,
                    reason: "Task duration must be positive".into(),
                });
                continue;
            }
            None => match (request.include_unsized, request.default_minutes) {
                (true, Some(value)) => value,
                _ => {
                    excluded.push(PortfolioCapacityExclusion {
                        task_id:action.task_id,reason:"No duration estimate; provide a positive default and enable unsized tasks".into(),
                    });
                    continue;
                }
            },
        };
        if chosen.len() >= request.max_tasks {
            excluded.push(PortfolioCapacityExclusion {
                task_id: action.task_id,
                reason: "Task count limit reached".into(),
            });
            continue;
        }
        if minutes > remaining {
            excluded.push(PortfolioCapacityExclusion {
                task_id: action.task_id,
                reason: format!(
                    "Needs {minutes} uninterrupted minutes but only {remaining} remain"
                ),
            });
            continue;
        }
        remaining -= minutes;
        chosen.push(PortfolioCapacityItem {
            task_id: action.task_id,
            title: action.title,
            project_id: action.project_id,
            minutes,
            portfolio_rank: action.rank,
            immediate_unlocks: action.immediately_unblocked,
            cross_project_downstream: action.other_project_downstream,
            rationale: action.reasons,
        });
    }
    Ok(PortfolioCapacityPlan {
        revision,
        evaluated_at_ms: request.now_ms,
        project_filter: request.project_id.clone(),
        capacity_minutes: request.available_minutes,
        assigned_minutes: request.available_minutes - remaining,
        unassigned_minutes: remaining,
        actionable_total,
        blocked_total,
        chosen,
        excluded,
        warnings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn task(id: &str, project: &str, estimate: Option<i64>) -> TaskRecord {
        TaskRecord {
            id: id.into(),
            title: id.into(),
            status: "ready".into(),
            project_id: Some(project.into()),
            milestone_id: None,
            estimated_minutes: estimate,
            earliest_start_at_ms: None,
            deadline_at_ms: None,
            description: String::new(),
            priority: None,
            labels: Vec::new(),
        }
    }
    fn edge(id: &str, pre: &str) -> TaskDependencyEdge {
        TaskDependencyEdge {
            task_id: id.into(),
            prerequisite_id: pre.into(),
        }
    }
    fn request(capacity: i64) -> PortfolioCapacityRequest {
        PortfolioCapacityRequest {
            now_ms: 100,
            available_minutes: capacity,
            max_tasks: 10,
            project_id: None,
            include_unassigned: false,
            include_unsized: false,
            default_minutes: None,
        }
    }
    #[test]
    fn fills_capacity_without_violating_global_precedence() {
        let tasks = vec![
            task("root", "platform", Some(60)),
            task("short", "web", Some(20)),
            task("dependent", "mobile", Some(40)),
        ];
        let result =
            portfolio_capacity_plan(1, &tasks, &[edge("dependent", "root")], &request(50)).unwrap();
        assert_eq!(result.chosen.len(), 1);
        assert_eq!(result.chosen[0].task_id, "short");
        assert_eq!(result.assigned_minutes, 20);
        assert_eq!(result.unassigned_minutes, 30);
        assert!(result.excluded.iter().any(|row| row.task_id == "root"));
        assert!(!result.chosen.iter().any(|row| row.task_id == "dependent"));
    }
    #[test]
    fn unsized_work_requires_explicit_opt_in_and_positive_fallback() {
        let tasks = vec![task("unknown", "p", None)];
        let report = portfolio_capacity_plan(1, &tasks, &[], &request(30)).unwrap();
        assert_eq!(report.excluded.len(), 1);
        let mut request = request(30);
        request.include_unsized = true;
        request.default_minutes = Some(15);
        let report = portfolio_capacity_plan(1, &tasks, &[], &request).unwrap();
        assert_eq!(report.chosen.len(), 1);
        assert_eq!(report.assigned_minutes, 15);
        request.default_minutes = Some(0);
        assert!(portfolio_capacity_plan(1, &tasks, &[], &request).is_err());
    }
    #[test]
    fn scoped_plan_has_no_foreign_identifiers_in_exclusions() {
        let tasks = vec![
            task("visible", "p", Some(40)),
            task("private", "other", Some(30)),
        ];
        let mut request = request(20);
        request.project_id = Some("p".into());
        let report = portfolio_capacity_plan(1, &tasks, &[], &request).unwrap();
        assert!(!serde_json::to_string(&report).unwrap().contains("private"));
        assert_eq!(report.excluded.len(), 1);
    }
    #[test]
    fn limits_are_strict_even_at_zero_capacity() {
        let tasks = vec![task("a", "p", Some(10))];
        let report = portfolio_capacity_plan(1, &tasks, &[], &request(0)).unwrap();
        assert!(report.chosen.is_empty());
        assert_eq!(report.unassigned_minutes, 0);
        let mut request = request(-1);
        assert!(portfolio_capacity_plan(1, &tasks, &[], &request).is_err());
        request.available_minutes = 10;
        request.max_tasks = 10_001;
        assert!(portfolio_capacity_plan(1, &tasks, &[], &request).is_err());
    }
    #[test]
    fn selection_is_stable_under_input_permutation() {
        let mut tasks = vec![task("a", "p", Some(10)), task("b", "q", Some(20))];
        let first = portfolio_capacity_plan(1, &tasks, &[], &request(20)).unwrap();
        tasks.reverse();
        let second = portfolio_capacity_plan(1, &tasks, &[], &request(20)).unwrap();
        assert_eq!(first, second);
    }
    #[test]
    fn explicit_task_limit_remains_an_explained_exclusion() {
        let tasks = vec![task("a", "p", Some(10)), task("b", "p", Some(10))];
        let mut request = request(50);
        request.max_tasks = 1;
        let result = portfolio_capacity_plan(1, &tasks, &[], &request).unwrap();
        assert_eq!(result.chosen.len(), 1);
        assert_eq!(result.excluded[0].reason, "Task count limit reached");
    }
}

#[cfg(test)]
mod portfolio_allocation_edges {
    use super::*;
    fn task(id: &str, status: &str, minutes: Option<i64>) -> TaskRecord {
        TaskRecord {
            id: id.into(),
            title: id.into(),
            status: status.into(),
            project_id: Some("p".into()),
            milestone_id: None,
            estimated_minutes: minutes,
            earliest_start_at_ms: None,
            deadline_at_ms: None,
            description: String::new(),
            priority: None,
            labels: Vec::new(),
        }
    }
    fn req(minutes: i64) -> PortfolioCapacityRequest {
        PortfolioCapacityRequest {
            now_ms: 100,
            available_minutes: minutes,
            max_tasks: 20,
            project_id: None,
            include_unassigned: false,
            include_unsized: false,
            default_minutes: None,
        }
    }
    #[test]
    fn blocked_tasks_never_consume_capacity() {
        let tasks = vec![
            task("root", "ready", Some(30)),
            task("next", "ready", Some(15)),
        ];
        let edge = TaskDependencyEdge {
            task_id: "next".into(),
            prerequisite_id: "root".into(),
        };
        let report = portfolio_capacity_plan(1, &tasks, &[edge], &req(60)).unwrap();
        assert_eq!(report.chosen.len(), 1);
        assert_eq!(report.chosen[0].task_id, "root");
        assert_eq!(report.blocked_total, 1);
    }
    #[test]
    fn invalid_stored_duration_is_an_explicit_exclusion() {
        let report =
            portfolio_capacity_plan(1, &[task("a", "ready", Some(-2))], &[], &req(20)).unwrap();
        assert_eq!(report.excluded.len(), 1);
        assert!(report.excluded[0].reason.contains("positive"));
    }
    #[test]
    fn cannot_exceed_task_count_even_with_zero_duration_budget() {
        let tasks = vec![task("a", "ready", Some(1)), task("b", "ready", Some(1))];
        let mut request = req(100);
        request.max_tasks = 0;
        let report = portfolio_capacity_plan(1, &tasks, &[], &request).unwrap();
        assert!(report.chosen.is_empty());
        assert_eq!(report.excluded.len(), 2);
        assert_eq!(report.assigned_minutes, 0);
    }
    #[test]
    fn overdue_task_is_preferred_unless_it_cannot_fit() {
        let mut late = task("late", "ready", Some(40));
        late.deadline_at_ms = Some(10);
        let short = task("short", "ready", Some(10));
        let result = portfolio_capacity_plan(1, &[short, late], &[], &req(20)).unwrap();
        assert_eq!(result.chosen[0].task_id, "short");
        assert_eq!(result.excluded[0].task_id, "late");
    }
    #[test]
    fn rejected_input_does_not_modify_provided_task_records() {
        let tasks = vec![task("a", "ready", Some(15))];
        let before = tasks.clone();
        let mut request = req(60);
        request.default_minutes = Some(-1);
        assert!(portfolio_capacity_plan(1, &tasks, &[], &request).is_err());
        assert_eq!(tasks, before);
    }
    #[test]
    fn status_and_deadline_changes_affect_rank_deterministically() {
        let mut tasks = vec![task("a", "ready", Some(10)), task("b", "ready", Some(10))];
        let first = portfolio_capacity_plan(1, &tasks, &[], &req(10)).unwrap();
        assert_eq!(first.chosen[0].task_id, "a");
        tasks[1].deadline_at_ms = Some(5);
        let second = portfolio_capacity_plan(1, &tasks, &[], &req(10)).unwrap();
        assert_eq!(second.chosen[0].task_id, "b");
        tasks[1].status = "done".into();
        let third = portfolio_capacity_plan(1, &tasks, &[], &req(10)).unwrap();
        assert_eq!(third.chosen[0].task_id, "a");
    }
}

#[cfg(test)]
mod portfolio_boundary_regressions {
    use super::*;
    fn one_task(project: &str, duration: Option<i64>) -> TaskRecord {
        TaskRecord {
            id: "one".into(),
            title: "one".into(),
            status: "ready".into(),
            project_id: Some(project.into()),
            milestone_id: Some("milestone".into()),
            estimated_minutes: duration,
            earliest_start_at_ms: None,
            deadline_at_ms: None,
            description: String::new(),
            priority: None,
            labels: Vec::new(),
        }
    }
    fn request() -> PortfolioCapacityRequest {
        PortfolioCapacityRequest {
            now_ms: 100,
            available_minutes: 100,
            max_tasks: 1,
            project_id: Some("team".into()),
            include_unassigned: false,
            include_unsized: false,
            default_minutes: None,
        }
    }
    #[test]
    fn unused_capacity_does_not_imply_scheduled_work() {
        let result =
            portfolio_capacity_plan(3, &[one_task("team", Some(30))], &[], &request()).unwrap();
        assert_eq!(result.capacity_minutes, 100);
        assert_eq!(result.assigned_minutes, 30);
        assert_eq!(result.unassigned_minutes, 70);
        assert_eq!(result.chosen[0].minutes, 30);
        assert_eq!(result.chosen[0].portfolio_rank, 1);
    }
    #[test]
    fn project_filter_excludes_foreign_tasks_even_when_ready() {
        let report =
            portfolio_capacity_plan(3, &[one_task("another", Some(20))], &[], &request()).unwrap();
        assert!(report.chosen.is_empty());
        assert!(report.excluded.is_empty());
        assert_eq!(report.project_filter.as_deref(), Some("team"));
    }
    #[test]
    fn downstream_dependency_cycle_is_rejected_in_capacity_planning() {
        let a = one_task("team", Some(20));
        let mut b = one_task("team", Some(20));
        b.id = "two".into();
        let edges = vec![
            TaskDependencyEdge {
                task_id: "one".into(),
                prerequisite_id: "two".into(),
            },
            TaskDependencyEdge {
                task_id: "two".into(),
                prerequisite_id: "one".into(),
            },
        ];
        assert!(portfolio_capacity_plan(3, &[a, b], &edges, &request()).is_err());
    }
    #[test]
    fn request_and_plan_are_machine_readable_for_cli_and_mcp() {
        let request = request();
        let decoded: PortfolioCapacityRequest =
            serde_json::from_value(serde_json::to_value(&request).unwrap()).unwrap();
        assert_eq!(decoded, request);
        let result =
            portfolio_capacity_plan(3, &[one_task("team", Some(10))], &[], &request).unwrap();
        let decoded: PortfolioCapacityPlan =
            serde_json::from_value(serde_json::to_value(&result).unwrap()).unwrap();
        assert_eq!(decoded, result);
    }
    #[test]
    fn historical_revision_is_explicit_and_never_inferred_from_time() {
        let report =
            portfolio_capacity_plan(42, &[one_task("team", Some(20))], &[], &request()).unwrap();
        assert_eq!(report.revision, 42);
        assert_eq!(report.evaluated_at_ms, 100);
    }
}
