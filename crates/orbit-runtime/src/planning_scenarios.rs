//! Read-only, revision-fenced portfolio scenario comparisons.
//!
//! A scenario is a proposal, never an instruction to write or reserve time.
//! All alternatives share one accepted task/dependency snapshot; callers must
//! explicitly submit separate authoritative commands for real changes.
use crate::{
    domain_commands::TaskRecord,
    portfolio_actions::{PortfolioActionRequest, recommend_portfolio_actions},
    portfolio_capacity::{
        PortfolioCapacityExclusion, PortfolioCapacityItem, PortfolioCapacityRequest,
    },
    task_dependencies::TaskDependencyEdge,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScenarioChoice {
    pub name: String,
    #[serde(default)]
    pub pinned_task_ids: Vec<String>,
    #[serde(default)]
    pub excluded_task_ids: Vec<String>,
    #[serde(default)]
    pub available_minutes: Option<i64>,
    #[serde(default)]
    pub max_tasks: Option<usize>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScenarioCompareRequest {
    pub baseline: PortfolioCapacityRequest,
    pub expected_revision: i64,
    pub alternatives: Vec<ScenarioChoice>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScenarioResult {
    pub name: String,
    pub capacity_minutes: i64,
    pub allocated_minutes: i64,
    pub remaining_minutes: i64,
    pub chosen: Vec<PortfolioCapacityItem>,
    pub excluded: Vec<PortfolioCapacityExclusion>,
    pub newly_selected: Vec<String>,
    pub displaced: Vec<String>,
    pub explanation: Vec<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScenarioComparison {
    pub accepted_revision: i64,
    pub evaluated_at_ms: i64,
    pub baseline: ScenarioResult,
    pub alternatives: Vec<ScenarioResult>,
    pub warnings: Vec<String>,
    pub read_only: bool,
}

fn normalized_ids(items: &[String]) -> Result<BTreeSet<String>, &'static str> {
    let mut result = BTreeSet::new();
    for id in items {
        if id.trim().is_empty() || id.len() > 256 {
            return Err("invalid scenario task identity");
        }
        if !result.insert(id.clone()) {
            return Err("duplicate scenario task identity");
        }
    }
    Ok(result)
}
fn allocate(
    ranking: &[crate::portfolio_actions::PortfolioAction],
    choice: &ScenarioChoice,
    request: &PortfolioCapacityRequest,
) -> Result<ScenarioResult, &'static str> {
    let pins = normalized_ids(&choice.pinned_task_ids)?;
    let excluded = normalized_ids(&choice.excluded_task_ids)?;
    if !pins.is_disjoint(&excluded) {
        return Err("pinned and excluded tasks overlap");
    }
    let budget = choice
        .available_minutes
        .unwrap_or(request.available_minutes);
    let task_limit = choice.max_tasks.unwrap_or(request.max_tasks);
    if !(0..=525_600).contains(&budget) {
        return Err("invalid scenario time budget");
    }
    if task_limit > 10_000 {
        return Err("scenario task count exceeds safety bound");
    }
    let lookup: BTreeMap<&str, &crate::portfolio_actions::PortfolioAction> =
        ranking.iter().map(|a| (a.task_id.as_str(), a)).collect();
    // Reject attempted pins of absent, blocked, scoped-out or deferred tasks.
    // Do not reveal which of these caused an inaccessible ID to be rejected.
    if pins.iter().any(|id| !lookup.contains_key(id.as_str())) {
        return Err("pinned task is not an actionable visible task");
    }
    if excluded.iter().any(|id| !lookup.contains_key(id.as_str())) {
        return Err("excluded task is not an actionable visible task");
    }
    let mut candidates: Vec<_> = ranking.iter().collect();
    candidates.sort_by_key(|a| (!pins.contains(&a.task_id), a.rank));
    let mut remaining = budget;
    let mut chosen = Vec::new();
    let mut rejected = Vec::new();
    for action in candidates {
        let id = &action.task_id;
        if excluded.contains(id) {
            rejected.push(PortfolioCapacityExclusion {
                task_id: id.clone(),
                reason: "Excluded by scenario".into(),
            });
            continue;
        }
        let minutes = match action.estimated_minutes {
            Some(n) if n > 0 => n,
            Some(_) => {
                rejected.push(PortfolioCapacityExclusion {
                    task_id: id.clone(),
                    reason: "Invalid estimate".into(),
                });
                continue;
            }
            None if request.include_unsized => request
                .default_minutes
                .ok_or("unsized task requires default")?,
            None => {
                rejected.push(PortfolioCapacityExclusion {
                    task_id: id.clone(),
                    reason: "Missing duration estimate".into(),
                });
                continue;
            }
        };
        if minutes <= 0 {
            return Err("invalid default duration");
        }
        if chosen.len() >= task_limit {
            rejected.push(PortfolioCapacityExclusion {
                task_id: id.clone(),
                reason: "Task count limit".into(),
            });
            continue;
        }
        if minutes > remaining {
            rejected.push(PortfolioCapacityExclusion {
                task_id: id.clone(),
                reason: format!("Requires {minutes} minutes, only {remaining} available"),
            });
            continue;
        }
        remaining -= minutes;
        chosen.push(PortfolioCapacityItem {
            task_id: id.clone(),
            title: action.title.clone(),
            project_id: action.project_id.clone(),
            minutes,
            portfolio_rank: action.rank,
            immediate_unlocks: action.immediately_unblocked,
            cross_project_downstream: action.other_project_downstream,
            rationale: vec![if pins.contains(id) {
                "Explicitly pinned for this scenario".into()
            } else {
                "Selected using deterministic portfolio order".into()
            }],
        });
    }
    // Pins are strict requests: never quietly return a proposal without them.
    if pins
        .iter()
        .any(|id| !chosen.iter().any(|task| &task.task_id == id))
    {
        return Err("pinned work cannot fit scenario capacity and count limits");
    }
    Ok(ScenarioResult {
        name: choice.name.clone(),
        capacity_minutes: budget,
        allocated_minutes: budget - remaining,
        remaining_minutes: remaining,
        chosen,
        excluded: rejected,
        newly_selected: Vec::new(),
        displaced: Vec::new(),
        explanation: vec![
            "Read-only scenario; does not change accepted tasks or calendar blocks".into(),
        ],
    })
}

pub fn compare_scenarios(
    revision: i64,
    tasks: &[TaskRecord],
    edges: &[TaskDependencyEdge],
    request: &ScenarioCompareRequest,
) -> Result<ScenarioComparison, &'static str> {
    if revision != request.expected_revision {
        return Err("accepted revision changed since scenario was prepared");
    }
    if request.alternatives.len() > 8 {
        return Err("too many scenario alternatives");
    }
    if request.baseline.available_minutes < 0 || request.baseline.max_tasks > 10_000 {
        return Err("invalid baseline constraints");
    }
    if request.baseline.default_minutes.is_some_and(|n| n <= 0) {
        return Err("invalid default duration");
    }
    let report = recommend_portfolio_actions(
        revision,
        tasks,
        edges,
        &PortfolioActionRequest {
            now_ms: request.baseline.now_ms,
            project_id: request.baseline.project_id.clone(),
            limit: tasks.len().min(10_000),
            include_unassigned: request.baseline.include_unassigned,
        },
    )?;
    let empty = ScenarioChoice {
        name: "baseline".into(),
        pinned_task_ids: vec![],
        excluded_task_ids: vec![],
        available_minutes: None,
        max_tasks: None,
    };
    let baseline = allocate(&report.recommended, &empty, &request.baseline)?;
    let baseline_ids: BTreeSet<_> = baseline
        .chosen
        .iter()
        .map(|item| item.task_id.clone())
        .collect();
    let mut names = BTreeSet::new();
    let mut alternatives = Vec::new();
    for choice in &request.alternatives {
        if choice.name.trim().is_empty()
            || choice.name.len() > 80
            || !names.insert(choice.name.clone())
            || choice.name == "baseline"
        {
            return Err("invalid or repeated scenario name");
        }
        let mut result = allocate(&report.recommended, choice, &request.baseline)?;
        let selected: BTreeSet<_> = result
            .chosen
            .iter()
            .map(|task| task.task_id.clone())
            .collect();
        result.newly_selected = selected.difference(&baseline_ids).cloned().collect();
        result.displaced = baseline_ids.difference(&selected).cloned().collect();
        result.explanation.push(format!(
            "{} added and {} displaced relative to baseline",
            result.newly_selected.len(),
            result.displaced.len()
        ));
        alternatives.push(result);
    }
    Ok(ScenarioComparison {
        accepted_revision: revision,
        evaluated_at_ms: request.baseline.now_ms,
        baseline,
        alternatives,
        warnings: report.warnings,
        read_only: true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn task(id: &str, minutes: i64) -> TaskRecord {
        TaskRecord {
            id: id.into(),
            title: id.into(),
            status: "ready".into(),
            project_id: Some("p".into()),
            milestone_id: None,
            estimated_minutes: Some(minutes),
            earliest_start_at_ms: None,
            deadline_at_ms: None,
        }
    }
    fn request() -> ScenarioCompareRequest {
        ScenarioCompareRequest {
            expected_revision: 3,
            baseline: PortfolioCapacityRequest {
                now_ms: 0,
                available_minutes: 60,
                max_tasks: 2,
                project_id: Some("p".into()),
                include_unassigned: false,
                include_unsized: false,
                default_minutes: None,
            },
            alternatives: vec![],
        }
    }
    #[test]
    fn baseline_is_read_only_and_deterministic() {
        let tasks = [task("a", 30), task("b", 30), task("c", 30)];
        let req = request();
        let a = compare_scenarios(3, &tasks, &[], &req).unwrap();
        let b = compare_scenarios(3, &tasks, &[], &req).unwrap();
        assert_eq!(a, b);
        assert!(a.read_only);
        assert_eq!(a.baseline.chosen.len(), 2);
        assert_eq!(tasks[0].status, "ready");
    }
    #[test]
    fn pin_displaces_lower_priority_work() {
        let mut req = request();
        req.alternatives.push(ScenarioChoice {
            name: "pin".into(),
            pinned_task_ids: vec!["c".into()],
            excluded_task_ids: vec![],
            available_minutes: None,
            max_tasks: None,
        });
        let result =
            compare_scenarios(3, &[task("a", 30), task("b", 30), task("c", 30)], &[], &req)
                .unwrap();
        assert!(
            result.alternatives[0]
                .chosen
                .iter()
                .any(|i| i.task_id == "c")
        );
        assert_eq!(result.alternatives[0].newly_selected, vec!["c"]);
        assert_eq!(result.alternatives[0].displaced.len(), 1);
    }
    #[test]
    fn stale_revision_rejected() {
        assert!(compare_scenarios(4, &[], &[], &request()).is_err());
    }
    #[test]
    fn blocked_or_invisible_pin_rejected() {
        let mut req = request();
        req.alternatives.push(ScenarioChoice {
            name: "bad".into(),
            pinned_task_ids: vec!["secret".into()],
            excluded_task_ids: vec![],
            available_minutes: None,
            max_tasks: None,
        });
        assert!(compare_scenarios(3, &[task("a", 30)], &[], &req).is_err());
    }
    #[test]
    fn overlapping_and_duplicate_choices_rejected() {
        let mut req = request();
        req.alternatives.push(ScenarioChoice {
            name: "duplicate".into(),
            pinned_task_ids: vec!["a".into()],
            excluded_task_ids: vec!["a".into()],
            available_minutes: None,
            max_tasks: None,
        });
        assert!(compare_scenarios(3, &[task("a", 30)], &[], &req).is_err());
        req.alternatives[0].excluded_task_ids.clear();
        req.alternatives[0].pinned_task_ids.push("a".into());
        assert!(compare_scenarios(3, &[task("a", 30)], &[], &req).is_err());
    }
    #[test]
    fn pins_cannot_overbook() {
        let mut req = request();
        req.alternatives.push(ScenarioChoice {
            name: "too-large".into(),
            pinned_task_ids: vec!["a".into()],
            excluded_task_ids: vec![],
            available_minutes: Some(5),
            max_tasks: None,
        });
        assert!(compare_scenarios(3, &[task("a", 30)], &[], &req).is_err());
    }
    #[test]
    fn zero_capacity_is_allowed() {
        let mut req = request();
        req.alternatives.push(ScenarioChoice {
            name: "rest".into(),
            pinned_task_ids: vec![],
            excluded_task_ids: vec![],
            available_minutes: Some(0),
            max_tasks: None,
        });
        let result = compare_scenarios(3, &[task("a", 30)], &[], &req).unwrap();
        assert_eq!(result.alternatives[0].chosen.len(), 0);
    }
    #[test]
    fn rejects_more_than_eight_alternatives() {
        let mut req = request();
        req.alternatives = (0..9)
            .map(|i| ScenarioChoice {
                name: format!("choice-{i}"),
                pinned_task_ids: vec![],
                excluded_task_ids: vec![],
                available_minutes: None,
                max_tasks: None,
            })
            .collect();
        assert!(compare_scenarios(3, &[], &[], &req).is_err());
    }
}
