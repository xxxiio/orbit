//! Read-only, explainable Today/Next work queue built from the *same* accepted
//! task, prerequisite and schedule snapshot. A proposal is never a reservation.
use crate::{
    authority_schedule::ScheduleRecord,
    domain_commands::TaskRecord,
    portfolio_actions::{PortfolioActionRequest, recommend_portfolio_actions},
    portfolio_schedule::plan_portfolio_schedule,
    schedule_planning::SchedulePlanRequest,
    task_dependencies::{TaskDependencyEdge, evaluate_readiness},
    task_next::next_actions,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkBriefRequest {
    pub now_ms: i64,
    pub ends_at_ms: i64,
    #[serde(default = "default_limit")]
    pub limit: usize,
    #[serde(default)]
    pub project_id: Option<String>,
    #[serde(default)]
    pub default_minutes: Option<i64>,
}
fn default_limit() -> usize {
    20
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkItemState {
    Scheduled,
    Ready,
    Blocked,
    Deferred,
    Completed,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkItem {
    pub id: String,
    pub title: String,
    pub project_id: Option<String>,
    pub milestone_id: Option<String>,
    pub state: WorkItemState,
    pub recommended_rank: Option<usize>,
    pub proposed_start_ms: Option<i64>,
    pub proposed_end_ms: Option<i64>,
    pub deadline_at_ms: Option<i64>,
    pub blocked_by: Vec<String>,
    pub unblocks: Vec<String>,
    pub explanation: Vec<String>,
    /// Global portfolio rank among actionable accepted tasks; separate from daily rank.
    pub portfolio_rank: Option<usize>,
    /// Distinct unfinished downstream tasks in this task’s own project.
    pub same_project_downstream: usize,
    /// Foreign task IDs are never copied into a project-scoped brief.
    pub other_project_downstream: usize,
    pub immediately_unblocked: usize,
    pub portfolio_reasons: Vec<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectWorkSummary {
    pub project_id: Option<String>,
    pub scheduled: usize,
    pub ready: usize,
    pub blocked: usize,
    pub deferred: usize,
    pub completed: usize,
}
/// Milestone-level work queue: never merges milestones from distinct projects.
/// A task with no milestone remains a separately visible unassigned bucket.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MilestoneWorkSummary {
    pub project_id: Option<String>,
    pub milestone_id: Option<String>,
    pub scheduled: usize,
    pub ready: usize,
    pub blocked: usize,
    pub deferred: usize,
    pub completed: usize,
    pub proposed_minutes: i64,
    pub overdue_task_ids: Vec<String>,
    pub prerequisite_bottlenecks: Vec<WorkBottleneck>,
    /// Ready portfolio actions in this milestone, ordered by leverage.
    pub leverage_actions: Vec<MilestoneLeverageAction>,
}
/// Actionable work within a milestone, with no foreign task identities.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MilestoneLeverageAction {
    pub task_id: String,
    pub portfolio_rank: usize,
    pub immediately_unblocked: usize,
    pub cross_project_downstream: usize,
    pub downstream_total: usize,
}

/// A prerequisite with the highest number of unfinished tasks waiting on it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkBottleneck {
    pub prerequisite_id: String,
    pub waiting_task_ids: Vec<String>,
    pub waiting_count: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkBrief {
    pub revision: i64,
    pub evaluated_at_ms: i64,
    pub ends_at_ms: i64,
    pub available_minutes: i64,
    pub proposed_minutes: i64,
    pub scheduled_count: usize,
    pub ready_count: usize,
    pub blocked_count: usize,
    pub deferred_count: usize,
    pub completed_count: usize,
    pub items: Vec<WorkItem>,
    pub projects: Vec<ProjectWorkSummary>,
    pub milestones: Vec<MilestoneWorkSummary>,
    pub warnings: Vec<String>,
    /// Portfolio-ranked actionable work included in this view.
    pub portfolio_actionable_total: usize,
    /// Number of visible ready tasks with cross-project downstream impact.
    pub cross_project_leverage_count: usize,
}
/// Scheduling must use an identical accepted revision for tasks, edges, and
/// calendar. The caller supplies one local or connected accepted snapshot.
pub fn build_work_brief(
    revision: i64,
    tasks: &[TaskRecord],
    edges: &[TaskDependencyEdge],
    blocks: &[ScheduleRecord],
    request: &WorkBriefRequest,
) -> Result<WorkBrief, &'static str> {
    if revision < 0 || request.now_ms < 0 || request.ends_at_ms <= request.now_ms {
        return Err("invalid work brief interval");
    }
    if request.limit > 10_000 {
        return Err("work brief limit exceeds safety bound");
    }
    if request
        .project_id
        .as_ref()
        .is_some_and(|id| id.trim().is_empty())
    {
        return Err("empty project filter");
    }
    let selection = plan_portfolio_schedule(
        revision,
        tasks,
        edges,
        blocks,
        &SchedulePlanRequest {
            starts_at_ms: request.now_ms,
            ends_at_ms: request.ends_at_ms,
            max_tasks: request.limit,
            project_id: request.project_id.clone(),
            default_minutes: request.default_minutes,
            include_unsized: false,
        },
    )?;
    let portfolio = recommend_portfolio_actions(
        revision,
        tasks,
        edges,
        &PortfolioActionRequest {
            now_ms: request.now_ms,
            project_id: request.project_id.clone(),
            limit: tasks.len(),
            include_unassigned: request.project_id.is_none(),
        },
    )?;
    let portfolio_by_id: BTreeMap<_, _> = portfolio
        .recommended
        .iter()
        .map(|action| (action.task_id.as_str(), action))
        .collect();
    let readiness = evaluate_readiness(revision, tasks, edges, request.now_ms)?;
    let ranking = next_actions(revision, tasks, edges, request.now_ms, tasks.len())?;
    let readiness_by_id: BTreeMap<_, _> = readiness
        .tasks
        .iter()
        .map(|row| (row.task_id.as_str(), row))
        .collect();
    let rank_by_id: BTreeMap<_, _> = ranking
        .actions
        .iter()
        .map(|row| (row.task_id.as_str(), row))
        .collect();
    let slot_by_id: BTreeMap<_, _> = selection
        .slots
        .iter()
        .map(|slot| (slot.task_id.as_str(), slot))
        .collect();
    let not_placed: BTreeMap<_, _> = selection
        .unplaced
        .iter()
        .map(|row| (row.task_id.as_str(), row.reason.as_str()))
        .collect();
    // Identities outside the requested project scope are not visible in this
    // brief. Aggregates such as `other_project_downstream` may still report a
    // count, but no foreign task identifier may be serialized.
    let visible_ids: BTreeSet<&str> = tasks
        .iter()
        .filter(|task| {
            request
                .project_id
                .as_ref()
                .is_none_or(|id| task.project_id.as_ref() == Some(id))
        })
        .map(|task| task.id.as_str())
        .collect();
    let mut items = Vec::new();
    let mut seen = BTreeSet::new();
    for task in tasks {
        if !seen.insert(task.id.as_str()) {
            return Err("duplicate accepted task identity");
        }
        if request
            .project_id
            .as_ref()
            .is_some_and(|id| task.project_id.as_ref() != Some(id))
        {
            continue;
        }
        let ready = readiness_by_id
            .get(task.id.as_str())
            .ok_or("readiness task missing")?;
        let slot = slot_by_id.get(task.id.as_str());
        let ranking = rank_by_id.get(task.id.as_str());
        let portfolio_action = portfolio_by_id.get(task.id.as_str());
        let complete = task.status == "done" || task.status == "cancelled";
        let state = if complete {
            WorkItemState::Completed
        } else if slot.is_some() {
            WorkItemState::Scheduled
        } else if ready.actionable {
            WorkItemState::Ready
        } else if !ready.blocking_task_ids.is_empty() {
            WorkItemState::Blocked
        } else {
            WorkItemState::Deferred
        };
        let mut explanation = Vec::new();
        if let Some(slot) = slot {
            explanation.extend(slot.explanation.clone());
        }
        if let Some(reason) = not_placed.get(task.id.as_str()) {
            explanation.push((*reason).to_owned());
        }
        if !ready.blocking_task_ids.is_empty() {
            explanation.push(format!(
                "Waiting for {} prerequisite task(s)",
                ready.blocking_task_ids.len()
            ));
        }
        if complete {
            explanation.push("Task is in a terminal status".into());
        }
        if explanation.is_empty() {
            explanation.push(format!("Accepted task status: {}", task.status));
        }
        items.push(WorkItem {
            id: task.id.clone(),
            title: task.title.clone(),
            project_id: task.project_id.clone(),
            milestone_id: task.milestone_id.clone(),
            state,
            recommended_rank: ranking.map(|r| r.rank),
            proposed_start_ms: slot.map(|s| s.starts_at_ms),
            proposed_end_ms: slot.map(|s| s.ends_at_ms),
            deadline_at_ms: task.deadline_at_ms,
            blocked_by: ready
                .blocking_task_ids
                .iter()
                .filter(|id| visible_ids.contains(id.as_str()))
                .cloned()
                .collect(),
            unblocks: ranking
                .map(|r| {
                    r.unblocks_task_ids
                        .iter()
                        .filter(|id| visible_ids.contains(id.as_str()))
                        .cloned()
                        .collect()
                })
                .unwrap_or_default(),
            explanation,
            portfolio_rank: portfolio_action.map(|action| action.rank),
            same_project_downstream: portfolio_action
                .map_or(0, |action| action.same_project_downstream),
            other_project_downstream: portfolio_action
                .map_or(0, |action| action.other_project_downstream),
            immediately_unblocked: portfolio_action
                .map_or(0, |action| action.immediately_unblocked),
            portfolio_reasons: portfolio_action
                .map(|action| action.reasons.clone())
                .unwrap_or_default(),
        });
    }
    // The presentation order is stable regardless of SQLite return order.
    // Proposed slots lead, then ready work by action rank, then diagnostic rows.
    items.sort_by(|a, b| {
        let state_rank = |state: &WorkItemState| match state {
            WorkItemState::Scheduled => 0,
            WorkItemState::Ready => 1,
            WorkItemState::Blocked => 2,
            WorkItemState::Deferred => 3,
            WorkItemState::Completed => 4,
        };
        state_rank(&a.state)
            .cmp(&state_rank(&b.state))
            .then_with(|| a.proposed_start_ms.cmp(&b.proposed_start_ms))
            .then_with(|| a.recommended_rank.cmp(&b.recommended_rank))
            .then_with(|| a.id.cmp(&b.id))
    });
    let mut project_map = BTreeMap::<Option<String>, ProjectWorkSummary>::new();
    let mut scheduled_count = 0;
    let mut ready_count = 0;
    let mut blocked_count = 0;
    let mut deferred_count = 0;
    let mut completed_count = 0;
    for item in &items {
        let row = project_map
            .entry(item.project_id.clone())
            .or_insert_with(|| ProjectWorkSummary {
                project_id: item.project_id.clone(),
                scheduled: 0,
                ready: 0,
                blocked: 0,
                deferred: 0,
                completed: 0,
            });
        match item.state {
            WorkItemState::Scheduled => {
                scheduled_count += 1;
                row.scheduled += 1;
            }
            WorkItemState::Ready => {
                ready_count += 1;
                row.ready += 1;
            }
            WorkItemState::Blocked => {
                blocked_count += 1;
                row.blocked += 1;
            }
            WorkItemState::Deferred => {
                deferred_count += 1;
                row.deferred += 1;
            }
            WorkItemState::Completed => {
                completed_count += 1;
                row.completed += 1;
            }
        }
    }
    let mut milestone_map =
        BTreeMap::<(Option<String>, Option<String>), MilestoneWorkSummary>::new();
    let mut bottlenecks =
        BTreeMap::<(Option<String>, Option<String>), BTreeMap<String, BTreeSet<String>>>::new();
    for item in &items {
        let key = (item.project_id.clone(), item.milestone_id.clone());
        let row = milestone_map
            .entry(key.clone())
            .or_insert_with(|| MilestoneWorkSummary {
                project_id: item.project_id.clone(),
                milestone_id: item.milestone_id.clone(),
                scheduled: 0,
                ready: 0,
                blocked: 0,
                deferred: 0,
                completed: 0,
                proposed_minutes: 0,
                overdue_task_ids: Vec::new(),
                prerequisite_bottlenecks: Vec::new(),
                leverage_actions: Vec::new(),
            });
        match item.state {
            WorkItemState::Scheduled => {
                row.scheduled += 1;
                if let (Some(start), Some(end)) = (item.proposed_start_ms, item.proposed_end_ms) {
                    row.proposed_minutes += (end - start) / 60_000;
                }
            }
            WorkItemState::Ready => row.ready += 1,
            WorkItemState::Blocked => row.blocked += 1,
            WorkItemState::Deferred => row.deferred += 1,
            WorkItemState::Completed => row.completed += 1,
        }
        if !matches!(item.state, WorkItemState::Completed)
            && item
                .deadline_at_ms
                .is_some_and(|deadline| deadline < request.now_ms)
        {
            row.overdue_task_ids.push(item.id.clone());
        }
        if let Some(rank) = item.portfolio_rank {
            row.leverage_actions.push(MilestoneLeverageAction {
                task_id: item.id.clone(),
                portfolio_rank: rank,
                immediately_unblocked: item.immediately_unblocked,
                cross_project_downstream: item.other_project_downstream,
                downstream_total: item
                    .same_project_downstream
                    .saturating_add(item.other_project_downstream),
            });
        }
        for blocker in &item.blocked_by {
            bottlenecks
                .entry(key.clone())
                .or_default()
                .entry(blocker.clone())
                .or_default()
                .insert(item.id.clone());
        }
    }
    let mut milestones: Vec<_> = milestone_map
        .into_iter()
        .map(|(key, mut row)| {
            if let Some(deps) = bottlenecks.remove(&key) {
                row.prerequisite_bottlenecks = deps
                    .into_iter()
                    .map(|(prerequisite_id, waiting)| {
                        let waiting_task_ids: Vec<_> = waiting.into_iter().collect();
                        WorkBottleneck {
                            prerequisite_id,
                            waiting_count: waiting_task_ids.len(),
                            waiting_task_ids,
                        }
                    })
                    .collect();
                row.prerequisite_bottlenecks.sort_by(|a, b| {
                    b.waiting_count
                        .cmp(&a.waiting_count)
                        .then_with(|| a.prerequisite_id.cmp(&b.prerequisite_id))
                });
            }
            row.leverage_actions.sort_by(|a, b| {
                a.portfolio_rank
                    .cmp(&b.portfolio_rank)
                    .then_with(|| a.task_id.cmp(&b.task_id))
            });
            row.overdue_task_ids.sort();
            row
        })
        .collect();
    milestones.sort_by(|a, b| {
        a.project_id
            .cmp(&b.project_id)
            .then_with(|| a.milestone_id.cmp(&b.milestone_id))
    });
    let cross_project_leverage_count = items
        .iter()
        .filter(|item| item.portfolio_rank.is_some() && item.other_project_downstream > 0)
        .count();
    Ok(WorkBrief {
        revision,
        evaluated_at_ms: request.now_ms,
        ends_at_ms: request.ends_at_ms,
        available_minutes: selection.available_minutes,
        proposed_minutes: selection.allocated_minutes,
        scheduled_count,
        ready_count,
        blocked_count,
        deferred_count,
        completed_count,
        items,
        projects: project_map.into_values().collect(),
        milestones,
        warnings: selection.warnings,
        portfolio_actionable_total: portfolio.actionable_total,
        cross_project_leverage_count,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn task(id: &str, status: &str, project: Option<&str>, minutes: Option<i64>) -> TaskRecord {
        TaskRecord {
            id: id.into(),
            title: format!("Task {id}"),
            status: status.into(),
            project_id: project.map(str::to_owned),
            milestone_id: Some("mile-1".into()),
            estimated_minutes: minutes,
            earliest_start_at_ms: None,
            deadline_at_ms: None,
        }
    }
    fn edge(id: &str, prerequisite: &str) -> TaskDependencyEdge {
        TaskDependencyEdge {
            task_id: id.into(),
            prerequisite_id: prerequisite.into(),
        }
    }
    fn block(start: i64, end: i64) -> ScheduleRecord {
        ScheduleRecord {
            id: format!("busy-{start}"),
            title: "Meeting".into(),
            starts_at_ms: start,
            ends_at_ms: end,
            duration_minutes: ((end - start) / 60_000) as u32,
            flexibility: "fixed".into(),
            target_kind: "none".into(),
            target_id: None,
        }
    }
    fn request() -> WorkBriefRequest {
        WorkBriefRequest {
            now_ms: 0,
            ends_at_ms: 180 * 60_000,
            limit: 10,
            project_id: None,
            default_minutes: None,
        }
    }
    #[test]
    fn distinguishes_scheduled_ready_blocked_deferred_and_done() {
        let mut deferred = task("future", "ready", None, Some(10));
        deferred.earliest_start_at_ms = Some(200 * 60_000);
        let tasks = [
            task("first", "ready", Some("p"), Some(60)),
            task("second", "ready", Some("p"), Some(80)),
            task("blocked", "ready", Some("p"), Some(10)),
            deferred,
            task("finished", "done", None, Some(10)),
            task("unsized", "ready", None, None),
        ];
        let result = build_work_brief(
            4,
            &tasks,
            &[edge("blocked", "first")],
            &[block(60 * 60_000, 120 * 60_000)],
            &request(),
        )
        .unwrap();
        assert_eq!(result.revision, 4);
        assert_eq!(result.scheduled_count, 1);
        assert_eq!(result.blocked_count, 1);
        assert_eq!(result.deferred_count, 1);
        assert_eq!(result.ready_count, 2);
        assert_eq!(result.completed_count, 1);
        assert_eq!(
            result
                .items
                .iter()
                .find(|i| i.id == "blocked")
                .unwrap()
                .blocked_by,
            ["first"]
        );
        let slot = result.items.iter().find(|i| i.id == "first").unwrap();
        assert_eq!(slot.state, WorkItemState::Scheduled);
        assert_eq!(
            (slot.proposed_start_ms, slot.proposed_end_ms),
            (Some(0), Some(60 * 60_000))
        );
        assert_eq!(
            result
                .projects
                .iter()
                .find(|p| p.project_id.as_deref() == Some("p"))
                .unwrap()
                .blocked,
            1
        );
    }
    #[test]
    fn filters_projects_without_leaking_other_work() {
        let tasks = [
            task("a", "ready", Some("red"), Some(30)),
            task("b", "ready", Some("blue"), Some(30)),
        ];
        let mut req = request();
        req.project_id = Some("red".into());
        let report = build_work_brief(0, &tasks, &[], &[], &req).unwrap();
        assert_eq!(report.items.len(), 1);
        assert_eq!(report.items[0].id, "a");
        assert_eq!(report.projects.len(), 1);
    }
    #[test]
    fn stable_report_independent_of_input_row_order() {
        let mut tasks = vec![
            task("b", "ready", None, Some(20)),
            task("a", "ready", None, Some(20)),
            task("c", "waiting", None, Some(15)),
        ];
        let first = build_work_brief(3, &tasks, &[], &[], &request()).unwrap();
        tasks.reverse();
        let second = build_work_brief(3, &tasks, &[], &[], &request()).unwrap();
        assert_eq!(first, second);
    }
    #[test]
    fn rejects_invalid_requests_and_duplicated_accepted_tasks() {
        let tasks = [
            task("same", "ready", None, Some(10)),
            task("same", "ready", None, Some(10)),
        ];
        assert!(build_work_brief(0, &tasks, &[], &[], &request()).is_err());
        let mut req = request();
        req.limit = 10001;
        assert!(build_work_brief(0, &[], &[], &[], &req).is_err());
        req = request();
        req.ends_at_ms = 0;
        assert!(build_work_brief(0, &[], &[], &[], &req).is_err());
        req = request();
        req.project_id = Some("  ".into());
        assert!(build_work_brief(0, &[], &[], &[], &req).is_err());
    }
    #[test]
    fn limits_proposals_without_hiding_unscheduled_actions() {
        let tasks = [
            task("a", "ready", None, Some(30)),
            task("b", "ready", None, Some(30)),
        ];
        let mut req = request();
        req.limit = 1;
        let result = build_work_brief(0, &tasks, &[], &[], &req).unwrap();
        assert_eq!(result.scheduled_count, 1);
        assert_eq!(result.ready_count, 1);
        assert_eq!(result.items.len(), 2);
    }
}

#[cfg(test)]
mod milestone_tests {
    use super::*;
    fn task(id: &str, milestone: &str, status: &str, deadline: Option<i64>) -> TaskRecord {
        TaskRecord {
            id: id.into(),
            title: id.into(),
            status: status.into(),
            project_id: Some("project".into()),
            milestone_id: Some(milestone.into()),
            estimated_minutes: Some(15),
            earliest_start_at_ms: None,
            deadline_at_ms: deadline,
        }
    }
    fn req() -> WorkBriefRequest {
        WorkBriefRequest {
            now_ms: 1000,
            ends_at_ms: 60 * 60_000,
            limit: 0,
            project_id: None,
            default_minutes: None,
        }
    }
    #[test]
    fn milestone_groups_are_separate_even_when_some_tasks_are_done() {
        let tasks = [
            task("done", "m1", "done", None),
            task("ready", "m1", "ready", None),
            task("other", "m2", "ready", None),
        ];
        let report = build_work_brief(10, &tasks, &[], &[], &req()).unwrap();
        assert_eq!(report.milestones.len(), 2);
        let first = &report.milestones[0];
        assert_eq!(first.milestone_id.as_deref(), Some("m1"));
        assert_eq!(first.completed, 1);
        assert_eq!(first.ready, 1);
        assert_eq!(first.proposed_minutes, 0);
        assert_eq!(report.milestones[1].ready, 1);
    }
    #[test]
    fn reports_overdue_and_shared_blocker_with_stable_dependents() {
        let tasks = [
            task("parent", "m1", "ready", None),
            task("child2", "m1", "ready", Some(500)),
            task("child1", "m1", "ready", Some(100)),
        ];
        let edges = [
            TaskDependencyEdge {
                task_id: "child1".into(),
                prerequisite_id: "parent".into(),
            },
            TaskDependencyEdge {
                task_id: "child2".into(),
                prerequisite_id: "parent".into(),
            },
        ];
        let report = build_work_brief(10, &tasks, &edges, &[], &req()).unwrap();
        let group = &report.milestones[0];
        assert_eq!(group.blocked, 2);
        assert_eq!(group.overdue_task_ids, ["child1", "child2"]);
        assert_eq!(group.prerequisite_bottlenecks[0].prerequisite_id, "parent");
        assert_eq!(
            group.prerequisite_bottlenecks[0].waiting_task_ids,
            ["child1", "child2"]
        );
        assert_eq!(group.prerequisite_bottlenecks[0].waiting_count, 2);
    }
    #[test]
    fn milestone_group_reports_only_its_own_waiting_tasks() {
        let tasks = [
            task("parent", "core", "ready", None),
            task("a", "one", "ready", None),
            task("b", "two", "ready", None),
        ];
        let edges = [
            TaskDependencyEdge {
                task_id: "a".into(),
                prerequisite_id: "parent".into(),
            },
            TaskDependencyEdge {
                task_id: "b".into(),
                prerequisite_id: "parent".into(),
            },
        ];
        let report = build_work_brief(10, &tasks, &edges, &[], &req()).unwrap();
        for group in report.milestones.iter().filter(|m| m.blocked > 0) {
            assert_eq!(group.prerequisite_bottlenecks.len(), 1);
            assert_eq!(group.prerequisite_bottlenecks[0].waiting_count, 1);
        }
    }
}

#[cfg(test)]
mod portfolio_today_tests {
    use super::*;
    fn task(id: &str, project: &str, status: &str) -> TaskRecord {
        TaskRecord {
            id: id.into(),
            title: format!("Task {id}"),
            status: status.into(),
            project_id: Some(project.into()),
            milestone_id: Some("launch".into()),
            estimated_minutes: Some(20),
            earliest_start_at_ms: None,
            deadline_at_ms: None,
        }
    }
    fn dep(task_id: &str, prerequisite_id: &str) -> TaskDependencyEdge {
        TaskDependencyEdge {
            task_id: task_id.into(),
            prerequisite_id: prerequisite_id.into(),
        }
    }
    fn req() -> WorkBriefRequest {
        WorkBriefRequest {
            now_ms: 100,
            ends_at_ms: 3_600_100,
            limit: 3,
            project_id: None,
            default_minutes: Some(30),
        }
    }
    #[test]
    fn portfolio_rank_is_available_in_the_same_today_snapshot() {
        let tasks = [
            task("root", "platform", "ready"),
            task("web", "web", "ready"),
            task("mobile", "mobile", "ready"),
        ];
        let report = build_work_brief(
            5,
            &tasks,
            &[dep("web", "root"), dep("mobile", "root")],
            &[],
            &req(),
        )
        .unwrap();
        let root = report.items.iter().find(|item| item.id == "root").unwrap();
        assert_eq!(root.portfolio_rank, Some(1));
        assert_eq!(root.immediately_unblocked, 2);
        assert_eq!(root.other_project_downstream, 2);
        assert_eq!(report.cross_project_leverage_count, 1);
        assert_eq!(report.portfolio_actionable_total, 1);
        let milestone = report
            .milestones
            .iter()
            .find(|item| item.project_id.as_deref() == Some("platform"))
            .unwrap();
        assert_eq!(milestone.leverage_actions[0].task_id, "root");
        assert_eq!(milestone.leverage_actions[0].downstream_total, 2);
        assert_eq!(report.revision, 5);
    }
    #[test]
    fn project_filter_omits_foreign_task_ids_even_in_portfolio_reasons() {
        let tasks = [
            task("visible", "public", "ready"),
            task("private-secret", "private", "ready"),
        ];
        let mut request = req();
        request.project_id = Some("public".into());
        let report = build_work_brief(
            3,
            &tasks,
            &[dep("private-secret", "visible")],
            &[],
            &request,
        )
        .unwrap();
        assert_eq!(report.items.len(), 1);
        assert_eq!(report.items[0].other_project_downstream, 1);
        assert_eq!(report.milestones[0].leverage_actions.len(), 1);
        let encoded = serde_json::to_string(&report).unwrap();
        assert!(!encoded.contains("private-secret"));
        assert!(!encoded.contains("Task private-secret"));
    }
    #[test]
    fn scheduled_task_still_has_portfolio_leverage() {
        let tasks = [
            task("foundation", "platform", "ready"),
            task("followup", "app", "ready"),
        ];
        let report =
            build_work_brief(2, &tasks, &[dep("followup", "foundation")], &[], &req()).unwrap();
        let foundation = report
            .items
            .iter()
            .find(|item| item.id == "foundation")
            .unwrap();
        assert_eq!(foundation.state, WorkItemState::Scheduled);
        assert_eq!(foundation.portfolio_rank, Some(1));
        assert_eq!(foundation.immediately_unblocked, 1);
    }
    #[test]
    fn blocked_and_finished_tasks_are_not_false_portfolio_actions() {
        let tasks = [
            task("root", "platform", "ready"),
            task("blocked", "app", "ready"),
            task("done", "app", "done"),
        ];
        let report = build_work_brief(2, &tasks, &[dep("blocked", "root")], &[], &req()).unwrap();
        for id in ["blocked", "done"] {
            let item = report.items.iter().find(|item| item.id == id).unwrap();
            assert_eq!(item.portfolio_rank, None);
            assert!(item.portfolio_reasons.is_empty());
        }
        assert_eq!(report.portfolio_actionable_total, 1);
    }
    #[test]
    fn shuffled_accepted_input_does_not_change_portfolio_planning() {
        let mut tasks = vec![
            task("root", "p", "ready"),
            task("child", "q", "ready"),
            task("other", "p", "ready"),
        ];
        let mut edges = vec![dep("child", "root")];
        let first = build_work_brief(2, &tasks, &edges, &[], &req()).unwrap();
        tasks.reverse();
        edges.reverse();
        let second = build_work_brief(2, &tasks, &edges, &[], &req()).unwrap();
        assert_eq!(first, second);
    }
    #[test]
    fn zero_daily_slot_limit_preserves_actionable_portfolio_visibility() {
        let tasks = [task("root", "p", "ready"), task("child", "q", "ready")];
        let mut request = req();
        request.limit = 0;
        let report = build_work_brief(1, &tasks, &[dep("child", "root")], &[], &request).unwrap();
        assert_eq!(report.scheduled_count, 0);
        assert_eq!(report.portfolio_actionable_total, 1);
        assert_eq!(
            report
                .milestones
                .iter()
                .map(|m| m.leverage_actions.len())
                .sum::<usize>(),
            1
        );
    }
}
