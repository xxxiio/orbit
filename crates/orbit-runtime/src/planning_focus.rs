//! Accepted-state, read-only task navigation for the Today/Next experience.
//! All task references are drawn from the already-scoped WorkBrief; the view
//! never manufactures links to invisible projects or pending local intentions.
use crate::planning_brief::{WorkBrief, WorkItem, WorkItemState};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FocusRequest {
    pub task_id: String,
    #[serde(default = "default_neighbor_limit")]
    pub max_neighbors: usize,
}
fn default_neighbor_limit() -> usize {
    12
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FocusNeighbor {
    pub task_id: String,
    pub title: String,
    pub state: WorkItemState,
    pub project_id: Option<String>,
    pub milestone_id: Option<String>,
    pub portfolio_rank: Option<usize>,
    pub relationship: String,
}
/// Navigation path through visible prerequisite edges, root to selected task.
/// A path never includes an ID outside the visible planning scope.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FocusTrail {
    pub task_ids: Vec<String>,
    pub unresolved_leaf: bool,
    pub depth: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskFocus {
    pub revision: i64,
    pub evaluated_at_ms: i64,
    pub task: WorkItem,
    pub visible_prerequisites: Vec<FocusNeighbor>,
    pub visible_dependents: Vec<FocusNeighbor>,
    pub project_peers: Vec<FocusNeighbor>,
    pub milestone_peers: Vec<FocusNeighbor>,
    pub hidden_prerequisite_count: usize,
    pub visible_prerequisite_count: usize,
    pub immediate_next: Option<FocusNeighbor>,
    pub previous_in_today: Option<FocusNeighbor>,
    pub project_task_count: usize,
    pub milestone_task_count: usize,
    pub navigation_notes: Vec<String>,
    pub prerequisite_trails: Vec<FocusTrail>,
    pub dependent_trails: Vec<FocusTrail>,
}
fn neighbor(task: &WorkItem, relationship: &str) -> FocusNeighbor {
    FocusNeighbor {
        task_id: task.id.clone(),
        title: task.title.clone(),
        state: task.state.clone(),
        project_id: task.project_id.clone(),
        milestone_id: task.milestone_id.clone(),
        portfolio_rank: task.portfolio_rank,
        relationship: relationship.to_owned(),
    }
}
/// Trace visible dependency relationships without graph mutation. The caller
/// supplies the already-scoped accepted planning rows; missing IDs end a path
/// rather than causing a global lookup. Return deterministic bounded paths.
fn walk_trails(
    brief: &WorkBrief,
    start: &str,
    upstream: bool,
    max_paths: usize,
) -> Vec<FocusTrail> {
    let by_id: BTreeMap<&str, &WorkItem> = brief
        .items
        .iter()
        .map(|row| (row.id.as_str(), row))
        .collect();
    if !by_id.contains_key(start) {
        return Vec::new();
    }
    let mut frontier = vec![vec![start.to_owned()]];
    let mut results = Vec::new();
    let mut traversed = 0usize;
    while let Some(path) = frontier.pop() {
        // The traversal limit protects a malformed or extremely dense brief.
        traversed += 1;
        if traversed > 20_000 || results.len() >= max_paths {
            break;
        }
        let Some(current) = path.last() else {
            continue;
        };
        let mut next: Vec<String> = if upstream {
            by_id
                .get(current.as_str())
                .map(|row| {
                    row.blocked_by
                        .iter()
                        .filter(|id| by_id.contains_key(id.as_str()))
                        .cloned()
                        .collect()
                })
                .unwrap_or_default()
        } else {
            brief
                .items
                .iter()
                .filter(|row| row.blocked_by.iter().any(|id| id == current))
                .map(|row| row.id.clone())
                .collect()
        };
        next.sort();
        next.dedup();
        next.retain(|id| !path.contains(id));
        let truncated = path.len() >= 32;
        if next.is_empty() || truncated {
            if path.len() > 1 {
                let mut trail = path;
                if upstream {
                    trail.reverse();
                }
                results.push(FocusTrail {
                    depth: trail.len() - 1,
                    task_ids: trail,
                    unresolved_leaf: truncated,
                });
            }
        } else {
            for child in next.into_iter().rev() {
                let mut successor = path.clone();
                successor.push(child);
                frontier.push(successor);
            }
        }
    }
    results.sort_by(|a, b| a.task_ids.cmp(&b.task_ids));
    results.truncate(max_paths);
    results
}

/// Task-scoped view with a strict visibility boundary. A scoped WorkBrief is
/// authoritative for which task identities are disclosed. Missing references
/// never cause a fallback lookup in a global task map.
pub fn focus_task(brief: &WorkBrief, request: &FocusRequest) -> Result<TaskFocus, &'static str> {
    if brief.revision < 0 || brief.evaluated_at_ms < 0 {
        return Err("invalid accepted planning revision");
    }
    if request.task_id.trim().is_empty() {
        return Err("task id cannot be empty");
    }
    if request.max_neighbors > 100 {
        return Err("focus neighbor limit exceeds safety bound");
    }
    let mut visible = BTreeMap::new();
    for (index, row) in brief.items.iter().enumerate() {
        if visible.insert(row.id.as_str(), index).is_some() {
            return Err("duplicate task in work brief");
        }
    }
    let index = *visible
        .get(request.task_id.as_str())
        .ok_or("task not visible in accepted planning scope")?;
    let mut task = brief.items[index].clone();
    let lookup = |id: &str| visible.get(id).map(|&i| &brief.items[i]);
    let mut parents = BTreeSet::new();
    for id in &task.blocked_by {
        if id == &task.id {
            return Err("self-referential prerequisite");
        }
        parents.insert(id.clone());
    }
    // Preserve counts separately, but never serialize a foreign identifier in
    // the focused task record. The parent brief may be globally constructed;
    // task focus is a scoped navigation output by design.
    task.blocked_by
        .retain(|id| visible.contains_key(id.as_str()));
    task.unblocks.retain(|id| visible.contains_key(id.as_str()));
    let mut prerequisites = Vec::new();
    let mut hidden = 0;
    for id in &parents {
        if let Some(row) = lookup(id) {
            prerequisites.push(neighbor(row, "prerequisite"));
        } else {
            hidden += 1;
        }
    }
    let mut dependents = Vec::new();
    for row in &brief.items {
        if row.id != task.id && row.blocked_by.iter().any(|id| id == &task.id) {
            dependents.push(neighbor(row, "dependent"));
        }
    }
    // Portfolio recommendations may include additional unblocks, but only
    // explicitly visible IDs can be used to produce a navigable target.
    let dependent_ids: BTreeSet<String> = dependents.iter().map(|n| n.task_id.clone()).collect();
    for id in task.unblocks.iter() {
        if !dependent_ids.contains(id.as_str())
            && let Some(row) = lookup(id)
        {
            dependents.push(neighbor(row, "potential_unlock"));
        }
    }
    dependents.sort_by(|a, b| a.task_id.cmp(&b.task_id));
    dependents.dedup_by(|a, b| a.task_id == b.task_id);
    let mut project_peers: Vec<_> = brief
        .items
        .iter()
        .filter(|row| row.id != task.id && row.project_id == task.project_id)
        .map(|row| neighbor(row, "project_peer"))
        .collect();
    let mut milestone_peers: Vec<_> = brief
        .items
        .iter()
        .filter(|row| {
            row.id != task.id
                && row.project_id == task.project_id
                && row.milestone_id == task.milestone_id
        })
        .map(|row| neighbor(row, "milestone_peer"))
        .collect();
    let sort_neighbors = |rows: &mut Vec<FocusNeighbor>| {
        rows.sort_by(|a, b| {
            a.portfolio_rank
                .unwrap_or(usize::MAX)
                .cmp(&b.portfolio_rank.unwrap_or(usize::MAX))
                .then_with(|| a.task_id.cmp(&b.task_id))
        })
    };
    sort_neighbors(&mut project_peers);
    sort_neighbors(&mut milestone_peers);
    let project_task_count = project_peers.len() + 1;
    let milestone_task_count = milestone_peers.len() + 1;
    let visible_prerequisite_count = prerequisites.len();
    prerequisites.truncate(request.max_neighbors);
    dependents.truncate(request.max_neighbors);
    project_peers.truncate(request.max_neighbors);
    milestone_peers.truncate(request.max_neighbors);
    let mut notes=vec!["This navigation is based on the accepted planning snapshot; pending edits are not included".to_owned()];
    if hidden > 0 {
        notes.push(format!(
            "{hidden} prerequisite identity/identities are outside the visible planning scope"
        ));
    }
    if task.project_id.is_none() {
        notes.push("Task is not assigned to a project".to_owned());
    }
    if task.milestone_id.is_none() {
        notes.push("Task has no milestone".to_owned());
    }
    let next = brief
        .items
        .get(index + 1)
        .map(|row| neighbor(row, "next_today"));
    let previous = index
        .checked_sub(1)
        .and_then(|i| brief.items.get(i))
        .map(|row| neighbor(row, "previous_today"));
    let prerequisite_trails = walk_trails(brief, &task.id, true, request.max_neighbors);
    let dependent_trails = walk_trails(brief, &task.id, false, request.max_neighbors);
    Ok(TaskFocus {
        revision: brief.revision,
        evaluated_at_ms: brief.evaluated_at_ms,
        task,
        visible_prerequisites: prerequisites,
        visible_dependents: dependents,
        project_peers,
        milestone_peers,
        hidden_prerequisite_count: hidden,
        visible_prerequisite_count,
        immediate_next: next,
        previous_in_today: previous,
        project_task_count,
        milestone_task_count,
        navigation_notes: notes,
        prerequisite_trails,
        dependent_trails,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn item(
        id: &str,
        project: Option<&str>,
        milestone: Option<&str>,
        blocked: &[&str],
    ) -> WorkItem {
        WorkItem {
            id: id.into(),
            title: id.into(),
            project_id: project.map(str::to_owned),
            milestone_id: milestone.map(str::to_owned),
            state: if blocked.is_empty() {
                WorkItemState::Ready
            } else {
                WorkItemState::Blocked
            },
            recommended_rank: None,
            proposed_start_ms: None,
            proposed_end_ms: None,
            deadline_at_ms: None,
            blocked_by: blocked.iter().map(|s| s.to_string()).collect(),
            unblocks: vec![],
            explanation: vec![],
            portfolio_rank: None,
            same_project_downstream: 0,
            other_project_downstream: 0,
            immediately_unblocked: 0,
            portfolio_reasons: vec![],
        }
    }
    fn brief(items: Vec<WorkItem>) -> WorkBrief {
        WorkBrief {
            revision: 5,
            evaluated_at_ms: 10,
            ends_at_ms: 100,
            available_minutes: 1,
            proposed_minutes: 0,
            scheduled_count: 0,
            ready_count: 0,
            blocked_count: 0,
            deferred_count: 0,
            completed_count: 0,
            items,
            projects: vec![],
            milestones: vec![],
            warnings: vec![],
            portfolio_actionable_total: 0,
            cross_project_leverage_count: 0,
        }
    }
    #[test]
    fn scoped_prerequisite_never_leaks_identity() {
        let view = brief(vec![item("visible", Some("p"), Some("m"), &["foreign"])]);
        let output = focus_task(
            &view,
            &FocusRequest {
                task_id: "visible".into(),
                max_neighbors: 10,
            },
        )
        .unwrap();
        assert_eq!(output.hidden_prerequisite_count, 1);
        assert!(output.visible_prerequisites.is_empty());
        assert!(!serde_json::to_string(&output).unwrap().contains("foreign"));
    }
    #[test]
    fn peers_are_scoped_to_project_and_milestone() {
        let view = brief(vec![
            item("a", Some("p"), Some("m"), &[]),
            item("b", Some("p"), Some("m"), &["a"]),
            item("c", Some("p"), Some("other"), &[]),
            item("d", Some("q"), Some("m"), &[]),
        ]);
        let output = focus_task(
            &view,
            &FocusRequest {
                task_id: "a".into(),
                max_neighbors: 10,
            },
        )
        .unwrap();
        assert_eq!(output.project_task_count, 3);
        assert_eq!(output.milestone_task_count, 2);
        assert_eq!(output.visible_dependents.len(), 1);
        assert_eq!(output.visible_dependents[0].task_id, "b");
        assert_eq!(output.milestone_peers[0].task_id, "b");
    }
    #[test]
    fn no_implicit_global_lookup_and_limits() {
        let view = brief(vec![item("a", Some("p"), None, &[])]);
        assert!(
            focus_task(
                &view,
                &FocusRequest {
                    task_id: "unknown".into(),
                    max_neighbors: 10
                }
            )
            .is_err()
        );
        assert!(
            focus_task(
                &view,
                &FocusRequest {
                    task_id: "a".into(),
                    max_neighbors: 101
                }
            )
            .is_err()
        );
        let mut duplicate = view.clone();
        duplicate.items.push(duplicate.items[0].clone());
        assert!(
            focus_task(
                &duplicate,
                &FocusRequest {
                    task_id: "a".into(),
                    max_neighbors: 10
                }
            )
            .is_err()
        );
    }
    #[test]
    fn navigation_preserves_input_brief_and_order() {
        let view = brief(vec![
            item("first", None, None, &[]),
            item("second", None, None, &[]),
        ]);
        let saved = view.clone();
        let focus = focus_task(
            &view,
            &FocusRequest {
                task_id: "second".into(),
                max_neighbors: 0,
            },
        )
        .unwrap();
        assert_eq!(focus.previous_in_today.unwrap().task_id, "first");
        assert_eq!(view, saved);
        assert!(focus.project_peers.is_empty());
    }
}

/// One accepted task group for drilling down from a project or milestone.
/// This model is constructed only from the already-visible WorkBrief rows.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FocusGroup {
    pub project_id: Option<String>,
    pub milestone_id: Option<String>,
    pub task_count: usize,
    pub scheduled: usize,
    pub actionable: usize,
    pub blocked: usize,
    pub deferred: usize,
    pub completed: usize,
    pub proposed_minutes: i64,
    pub overdue: usize,
    pub top_actions: Vec<FocusNeighbor>,
    pub bottlenecks: Vec<FocusBottleneck>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FocusBottleneck {
    pub prerequisite_id: String,
    /// Only tasks within the visible group are counted here.
    pub visible_waiters: usize,
    pub waiting_task_ids: Vec<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GroupFocusRequest {
    pub project_id: String,
    #[serde(default)]
    pub milestone_id: Option<String>,
    #[serde(default = "default_neighbor_limit")]
    pub max_tasks: usize,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GroupFocus {
    pub revision: i64,
    pub evaluated_at_ms: i64,
    pub project: FocusGroup,
    pub milestones: Vec<FocusGroup>,
    pub sibling_projects: Vec<String>,
    pub notes: Vec<String>,
}
fn summarize_group(
    rows: &[&WorkItem],
    project_id: Option<String>,
    milestone_id: Option<String>,
    limit: usize,
    now_ms: i64,
) -> FocusGroup {
    let mut scheduled = 0;
    let mut actionable = 0;
    let mut blocked = 0;
    let mut deferred = 0;
    let mut completed = 0;
    let mut proposed_minutes = 0;
    let mut overdue = 0;
    let mut waiters: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut top_actions = Vec::new();
    let visible: BTreeSet<&str> = rows.iter().map(|r| r.id.as_str()).collect();
    for row in rows {
        match row.state {
            WorkItemState::Scheduled => scheduled += 1,
            WorkItemState::Ready => actionable += 1,
            WorkItemState::Blocked => blocked += 1,
            WorkItemState::Deferred => deferred += 1,
            WorkItemState::Completed => completed += 1,
        }
        if let (Some(start), Some(end)) = (row.proposed_start_ms, row.proposed_end_ms)
            && end >= start
        {
            proposed_minutes += (end - start) / 60_000;
        }
        if row.deadline_at_ms.is_some_and(|deadline| deadline < now_ms)
            && !matches!(row.state, WorkItemState::Completed)
        {
            overdue += 1;
        }
        if matches!(row.state, WorkItemState::Scheduled | WorkItemState::Ready) {
            top_actions.push(neighbor(row, "actionable"));
        }
        for prereq in &row.blocked_by {
            if visible.contains(prereq.as_str()) && prereq != &row.id {
                waiters
                    .entry(prereq.clone())
                    .or_default()
                    .insert(row.id.clone());
            }
        }
    }
    top_actions.sort_by(|a, b| {
        a.portfolio_rank
            .unwrap_or(usize::MAX)
            .cmp(&b.portfolio_rank.unwrap_or(usize::MAX))
            .then_with(|| a.task_id.cmp(&b.task_id))
    });
    top_actions.truncate(limit);
    let mut bottlenecks: Vec<_> = waiters
        .into_iter()
        .map(|(prerequisite_id, waiting_task_ids)| FocusBottleneck {
            prerequisite_id,
            visible_waiters: waiting_task_ids.len(),
            waiting_task_ids: waiting_task_ids.into_iter().collect(),
        })
        .collect();
    bottlenecks.sort_by(|a, b| {
        b.visible_waiters
            .cmp(&a.visible_waiters)
            .then_with(|| a.prerequisite_id.cmp(&b.prerequisite_id))
    });
    bottlenecks.truncate(limit);
    FocusGroup {
        project_id,
        milestone_id,
        task_count: rows.len(),
        scheduled,
        actionable,
        blocked,
        deferred,
        completed,
        proposed_minutes,
        overdue,
        top_actions,
        bottlenecks,
    }
}

/// Project/milestone context is intentionally limited to identifiers visible
/// in the supplied planning brief. Scope checks must happen before the brief is
/// created; this function will not query or infer global project membership.
pub fn focus_group(
    brief: &WorkBrief,
    request: &GroupFocusRequest,
) -> Result<GroupFocus, &'static str> {
    if brief.revision < 0 || brief.evaluated_at_ms < 0 {
        return Err("invalid planning snapshot");
    }
    if request.project_id.trim().is_empty() {
        return Err("project identifier required");
    }
    if request
        .milestone_id
        .as_ref()
        .is_some_and(|m| m.trim().is_empty())
    {
        return Err("milestone identifier cannot be empty");
    }
    if request.max_tasks > 100 {
        return Err("group focus limit exceeds safety bound");
    }
    let mut ids = BTreeSet::new();
    for task in &brief.items {
        if !ids.insert(task.id.as_str()) {
            return Err("duplicate task in accepted planning brief");
        }
    }
    let project_rows: Vec<_> = brief
        .items
        .iter()
        .filter(|row| row.project_id.as_deref() == Some(request.project_id.as_str()))
        .collect();
    if project_rows.is_empty() {
        return Err("project not present in visible planning scope");
    }
    let mut milestone_ids = BTreeSet::new();
    for task in &project_rows {
        milestone_ids.insert(task.milestone_id.clone());
    }
    if let Some(milestone) = &request.milestone_id
        && !milestone_ids.contains(&Some(milestone.clone()))
    {
        return Err("milestone not visible within this project");
    }
    let selected: Vec<_> = project_rows
        .iter()
        .copied()
        .filter(|row| {
            request
                .milestone_id
                .as_ref()
                .is_none_or(|m| row.milestone_id.as_ref() == Some(m))
        })
        .collect();
    let project = summarize_group(
        &selected,
        Some(request.project_id.clone()),
        request.milestone_id.clone(),
        request.max_tasks,
        brief.evaluated_at_ms,
    );
    let mut milestones = Vec::new();
    for id in milestone_ids {
        let rows: Vec<_> = project_rows
            .iter()
            .copied()
            .filter(|row| row.milestone_id == id)
            .collect();
        if request
            .milestone_id
            .as_ref()
            .is_none_or(|m| id.as_ref() == Some(m))
        {
            milestones.push(summarize_group(
                &rows,
                Some(request.project_id.clone()),
                id,
                request.max_tasks,
                brief.evaluated_at_ms,
            ));
        }
    }
    let sibling_projects = brief
        .items
        .iter()
        .filter_map(|task| task.project_id.clone())
        .filter(|id| id != &request.project_id)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    Ok(GroupFocus {
        revision: brief.revision,
        evaluated_at_ms: brief.evaluated_at_ms,
        project,
        milestones,
        sibling_projects,
        notes: vec![
            "Only accepted tasks and visible project identities are included".into(),
            "Bottlenecks count visible waiters; hidden cross-project tasks are excluded".into(),
        ],
    })
}

#[cfg(test)]
mod group_tests {
    use super::*;
    fn task(id: &str, project: &str, milestone: Option<&str>, blocked: &[&str]) -> WorkItem {
        WorkItem {
            id: id.into(),
            title: id.into(),
            project_id: Some(project.into()),
            milestone_id: milestone.map(str::to_owned),
            state: if blocked.is_empty() {
                WorkItemState::Ready
            } else {
                WorkItemState::Blocked
            },
            recommended_rank: None,
            proposed_start_ms: None,
            proposed_end_ms: None,
            deadline_at_ms: None,
            blocked_by: blocked.iter().map(|x| x.to_string()).collect(),
            unblocks: vec![],
            explanation: vec![],
            portfolio_rank: None,
            same_project_downstream: 0,
            other_project_downstream: 0,
            immediately_unblocked: 0,
            portfolio_reasons: vec![],
        }
    }
    fn brief(items: Vec<WorkItem>) -> WorkBrief {
        WorkBrief {
            revision: 9,
            evaluated_at_ms: 10,
            ends_at_ms: 100,
            available_minutes: 0,
            proposed_minutes: 0,
            scheduled_count: 0,
            ready_count: 0,
            blocked_count: 0,
            deferred_count: 0,
            completed_count: 0,
            items,
            projects: vec![],
            milestones: vec![],
            warnings: vec![],
            portfolio_actionable_total: 0,
            cross_project_leverage_count: 0,
        }
    }
    #[test]
    fn project_and_milestone_scope_never_mix_projects() {
        let view = brief(vec![
            task("a", "alpha", Some("first"), &[]),
            task("b", "alpha", Some("first"), &["a"]),
            task("c", "alpha", Some("second"), &[]),
            task("d", "beta", Some("first"), &[]),
        ]);
        let focus = focus_group(
            &view,
            &GroupFocusRequest {
                project_id: "alpha".into(),
                milestone_id: Some("first".into()),
                max_tasks: 5,
            },
        )
        .unwrap();
        assert_eq!(focus.project.task_count, 2);
        assert_eq!(focus.project.bottlenecks.len(), 1);
        assert_eq!(focus.project.bottlenecks[0].prerequisite_id, "a");
        assert_eq!(focus.milestones.len(), 1);
        assert_eq!(focus.sibling_projects, vec!["beta"]);
        assert!(
            focus
                .project
                .top_actions
                .iter()
                .all(|x| x.project_id.as_deref() == Some("alpha"))
        );
    }
    #[test]
    fn scoped_project_not_visible_is_rejected() {
        let view = brief(vec![task("a", "alpha", None, &[])]);
        assert!(
            focus_group(
                &view,
                &GroupFocusRequest {
                    project_id: "beta".into(),
                    milestone_id: None,
                    max_tasks: 5
                }
            )
            .is_err()
        );
        assert!(
            focus_group(
                &view,
                &GroupFocusRequest {
                    project_id: "alpha".into(),
                    milestone_id: Some("missing".into()),
                    max_tasks: 5
                }
            )
            .is_err()
        );
    }
    #[test]
    fn bottlenecks_are_deduplicated_and_bounded() {
        let view = brief(vec![
            task("root", "p", None, &[]),
            task("b", "p", None, &["root", "root"]),
            task("c", "p", None, &["root"]),
        ]);
        let focus = focus_group(
            &view,
            &GroupFocusRequest {
                project_id: "p".into(),
                milestone_id: None,
                max_tasks: 1,
            },
        )
        .unwrap();
        assert_eq!(focus.project.bottlenecks[0].visible_waiters, 2);
        assert_eq!(focus.project.top_actions.len(), 1);
    }
}

#[cfg(test)]
mod path_and_context_tests {
    use super::*;
    fn row(
        id: &str,
        project: &str,
        milestone: &str,
        blocked: &[&str],
        status: WorkItemState,
        rank: Option<usize>,
    ) -> WorkItem {
        WorkItem {
            id: id.into(),
            title: format!("Task {id}"),
            project_id: Some(project.into()),
            milestone_id: Some(milestone.into()),
            state: status,
            recommended_rank: None,
            proposed_start_ms: None,
            proposed_end_ms: None,
            deadline_at_ms: None,
            blocked_by: blocked.iter().map(|s| s.to_string()).collect(),
            unblocks: vec![],
            explanation: vec![],
            portfolio_rank: rank,
            same_project_downstream: 0,
            other_project_downstream: 0,
            immediately_unblocked: 0,
            portfolio_reasons: vec![],
        }
    }
    fn snapshot(items: Vec<WorkItem>) -> WorkBrief {
        WorkBrief {
            revision: 7,
            evaluated_at_ms: 1_000_000,
            ends_at_ms: 2_000_000,
            available_minutes: 60,
            proposed_minutes: 0,
            scheduled_count: 0,
            ready_count: 0,
            blocked_count: 0,
            deferred_count: 0,
            completed_count: 0,
            items,
            projects: vec![],
            milestones: vec![],
            warnings: vec![],
            portfolio_actionable_total: 0,
            cross_project_leverage_count: 0,
        }
    }
    fn focus(brief: &WorkBrief, id: &str, max: usize) -> TaskFocus {
        focus_task(
            brief,
            &FocusRequest {
                task_id: id.into(),
                max_neighbors: max,
            },
        )
        .unwrap()
    }
    #[test]
    fn upstream_chain_is_ordered_root_to_focused_task() {
        let brief = snapshot(vec![
            row(
                "finish",
                "p",
                "m",
                &["middle"],
                WorkItemState::Blocked,
                None,
            ),
            row("middle", "p", "m", &["root"], WorkItemState::Blocked, None),
            row("root", "p", "m", &[], WorkItemState::Ready, Some(1)),
        ]);
        let result = focus(&brief, "finish", 10);
        assert_eq!(result.prerequisite_trails.len(), 1);
        assert_eq!(
            result.prerequisite_trails[0].task_ids,
            vec!["root", "middle", "finish"]
        );
        assert_eq!(result.prerequisite_trails[0].depth, 2);
        assert!(!result.prerequisite_trails[0].unresolved_leaf);
    }
    #[test]
    fn downstream_chain_is_ordered_focused_task_to_leaf() {
        let brief = snapshot(vec![
            row("c", "p", "m", &["b"], WorkItemState::Blocked, None),
            row("a", "p", "m", &[], WorkItemState::Ready, Some(1)),
            row("b", "p", "m", &["a"], WorkItemState::Blocked, None),
        ]);
        let result = focus(&brief, "a", 10);
        assert_eq!(result.dependent_trails[0].task_ids, vec!["a", "b", "c"]);
        assert_eq!(result.dependent_trails[0].depth, 2);
    }
    #[test]
    fn diamonds_produce_two_deterministic_paths_without_foreign_lookups() {
        let brief = snapshot(vec![
            row("root", "p", "m", &[], WorkItemState::Ready, None),
            row("left", "p", "m", &["root"], WorkItemState::Blocked, None),
            row("right", "p", "m", &["root"], WorkItemState::Blocked, None),
            row(
                "tail",
                "p",
                "m",
                &["left", "right"],
                WorkItemState::Blocked,
                None,
            ),
        ]);
        let paths = focus(&brief, "root", 10).dependent_trails;
        assert_eq!(paths.len(), 2);
        assert_eq!(paths[0].task_ids, vec!["root", "left", "tail"]);
        assert_eq!(paths[1].task_ids, vec!["root", "right", "tail"]);
        let reversed = snapshot(brief.items.iter().cloned().rev().collect());
        assert_eq!(focus(&reversed, "root", 10).dependent_trails, paths);
    }
    #[test]
    fn missing_upstream_task_is_not_disclosed_in_a_path() {
        let brief = snapshot(vec![row(
            "visible",
            "p",
            "m",
            &["secret-in-other-project"],
            WorkItemState::Blocked,
            None,
        )]);
        let focused = focus(&brief, "visible", 10);
        assert!(focused.prerequisite_trails.is_empty());
        assert_eq!(focused.hidden_prerequisite_count, 1);
        let serialized = serde_json::to_string(&focused).unwrap();
        assert!(!serialized.contains("secret-in-other-project"));
    }
    #[test]
    fn malformed_dependency_cycle_is_bounded_and_does_not_loop() {
        let brief = snapshot(vec![
            row("a", "p", "m", &["b"], WorkItemState::Blocked, None),
            row("b", "p", "m", &["a"], WorkItemState::Blocked, None),
        ]);
        let focused = focus(&brief, "a", 10);
        assert!(
            focused
                .prerequisite_trails
                .iter()
                .all(|trail| trail.task_ids.len() <= 2)
        );
        assert!(
            focused
                .dependent_trails
                .iter()
                .all(|trail| trail.task_ids.len() <= 2)
        );
        assert!(
            focused.prerequisite_trails.iter().all(|trail| trail
                .task_ids
                .iter()
                .collect::<BTreeSet<_>>()
                .len()
                == trail.task_ids.len())
        );
    }
    #[test]
    fn bounded_path_count_is_independent_of_unbounded_task_list() {
        let mut items = vec![row("root", "p", "m", &[], WorkItemState::Ready, None)];
        for i in 0..100 {
            items.push(row(
                &format!("leaf-{i:03}"),
                "p",
                "m",
                &["root"],
                WorkItemState::Blocked,
                None,
            ));
        }
        let brief = snapshot(items);
        let focused = focus(&brief, "root", 3);
        assert_eq!(focused.dependent_trails.len(), 3);
        assert_eq!(focused.dependent_trails[0].task_ids[1], "leaf-000");
        assert_eq!(focused.visible_dependents.len(), 3);
    }
    #[test]
    fn portfolio_rank_breaks_project_peer_order_before_identity() {
        let brief = snapshot(vec![
            row("current", "p", "m", &[], WorkItemState::Ready, None),
            row("a", "p", "m", &[], WorkItemState::Ready, Some(9)),
            row("z", "p", "m", &[], WorkItemState::Ready, Some(1)),
            row("b", "p", "m", &[], WorkItemState::Ready, None),
        ]);
        let focused = focus(&brief, "current", 10);
        let ids: Vec<_> = focused
            .project_peers
            .iter()
            .map(|p| p.task_id.as_str())
            .collect();
        assert_eq!(ids, vec!["z", "a", "b"]);
    }
    #[test]
    fn same_milestone_name_does_not_merge_distinct_projects() {
        let brief = snapshot(vec![
            row("a", "alpha", "shared", &[], WorkItemState::Ready, None),
            row("b", "beta", "shared", &[], WorkItemState::Ready, None),
            row("c", "alpha", "other", &[], WorkItemState::Ready, None),
        ]);
        let focused = focus(&brief, "a", 10);
        assert_eq!(focused.project_task_count, 2);
        assert_eq!(focused.milestone_task_count, 1);
        assert!(focused.milestone_peers.is_empty());
        assert_eq!(focused.project_peers.len(), 1);
        assert_eq!(focused.project_peers[0].task_id, "c");
    }
    #[test]
    fn terminal_work_does_not_count_as_actionable_group_work() {
        let brief = snapshot(vec![
            row("done", "p", "m", &[], WorkItemState::Completed, None),
            row("ready", "p", "m", &[], WorkItemState::Ready, Some(3)),
            row("deferred", "p", "m", &[], WorkItemState::Deferred, None),
        ]);
        let group = focus_group(
            &brief,
            &GroupFocusRequest {
                project_id: "p".into(),
                milestone_id: Some("m".into()),
                max_tasks: 10,
            },
        )
        .unwrap();
        assert_eq!(group.project.completed, 1);
        assert_eq!(group.project.actionable, 1);
        assert_eq!(group.project.deferred, 1);
        assert_eq!(group.project.top_actions.len(), 1);
    }
    #[test]
    fn malformed_project_filters_are_rejected() {
        let brief = snapshot(vec![row("a", "p", "m", &[], WorkItemState::Ready, None)]);
        for (project, milestone, max_tasks) in
            [("", None, 10), ("p", Some(""), 10), ("p", None, 101)]
        {
            assert!(
                focus_group(
                    &brief,
                    &GroupFocusRequest {
                        project_id: project.into(),
                        milestone_id: milestone.map(str::to_owned),
                        max_tasks,
                    }
                )
                .is_err()
            );
        }
    }
    #[test]
    fn group_focus_is_deterministic_and_nonmutating() {
        let brief = snapshot(vec![
            row("b", "p", "m", &["a"], WorkItemState::Blocked, None),
            row("a", "p", "m", &[], WorkItemState::Ready, Some(1)),
            row("c", "q", "m", &[], WorkItemState::Ready, None),
        ]);
        let original = brief.clone();
        let request = GroupFocusRequest {
            project_id: "p".into(),
            milestone_id: None,
            max_tasks: 10,
        };
        let result = focus_group(&brief, &request).unwrap();
        let mut reversed = brief.clone();
        reversed.items.reverse();
        assert_eq!(focus_group(&reversed, &request).unwrap(), result);
        assert_eq!(brief, original);
    }
    #[test]
    fn group_bottlenecks_ignore_nonvisible_prerequisite_ids() {
        let brief = snapshot(vec![
            row(
                "visible",
                "p",
                "m",
                &["hidden"],
                WorkItemState::Blocked,
                None,
            ),
            row("ready", "p", "m", &[], WorkItemState::Ready, None),
        ]);
        let group = focus_group(
            &brief,
            &GroupFocusRequest {
                project_id: "p".into(),
                milestone_id: None,
                max_tasks: 10,
            },
        )
        .unwrap();
        assert!(group.project.bottlenecks.is_empty());
        assert!(
            !serde_json::to_string(&group)
                .unwrap()
                .contains("\"hidden\"")
        );
    }
}
