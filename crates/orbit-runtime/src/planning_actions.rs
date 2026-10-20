//! Read-only, scope-safe action discovery for a focused accepted planning view.
//! These are *intentions*, never executable commands. Actual writes must go
//! through the authority command ledger with a freshly checked revision.
use crate::planning_brief::{WorkBrief, WorkItem, WorkItemState};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionKind {
    InspectTask,
    InspectProject,
    InspectMilestone,
    InspectBlocker,
    PreviewCompletion,
    PreviewSchedule,
    ReviewSchedule,
    AddDependency,
    RemoveDependency,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionAvailability {
    Available,
    RequiresAuthorityConfirmation,
    Unavailable,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanningAction {
    pub kind: ActionKind,
    pub availability: ActionAvailability,
    pub task_id: Option<String>,
    pub project_id: Option<String>,
    pub milestone_id: Option<String>,
    pub prerequisite_id: Option<String>,
    pub explanation: String,
    pub requires_confirmation: bool,
    pub mutates_authority: bool,
    pub reviewed_revision: i64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskActionRequest {
    pub task_id: String,
    #[serde(default)]
    pub authority_available: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GroupActionRequest {
    pub project_id: String,
    pub milestone_id: Option<String>,
    #[serde(default)]
    pub authority_available: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanningActionMenu {
    pub revision: i64,
    pub evaluated_at_ms: i64,
    pub selected_task_id: Option<String>,
    pub selected_project_id: Option<String>,
    pub selected_milestone_id: Option<String>,
    pub actions: Vec<PlanningAction>,
    pub notices: Vec<String>,
}
fn validate_brief(brief: &WorkBrief) -> Result<BTreeMap<&str, &WorkItem>, &'static str> {
    if brief.revision < 0 || brief.evaluated_at_ms < 0 || brief.ends_at_ms <= brief.evaluated_at_ms
    {
        return Err("invalid accepted planning snapshot");
    }
    let mut visible = BTreeMap::new();
    for task in &brief.items {
        if task.id.trim().is_empty() || visible.insert(task.id.as_str(), task).is_some() {
            return Err("invalid or duplicate visible task identity");
        }
    }
    Ok(visible)
}
fn action(
    brief: &WorkBrief,
    kind: ActionKind,
    availability: ActionAvailability,
    task: Option<&WorkItem>,
    prerequisite: Option<&str>,
    explanation: &str,
    mutates: bool,
) -> PlanningAction {
    PlanningAction {
        kind,
        availability,
        task_id: task.map(|row| row.id.clone()),
        project_id: task.and_then(|row| row.project_id.clone()),
        milestone_id: task.and_then(|row| row.milestone_id.clone()),
        prerequisite_id: prerequisite.map(str::to_owned),
        explanation: explanation.to_owned(),
        requires_confirmation: mutates,
        mutates_authority: mutates,
        reviewed_revision: brief.revision,
    }
}
/// The returned IDs are *only* IDs already present in the caller's scoped
/// accepted work brief. In particular we never follow hidden prerequisites.
pub fn task_actions(
    brief: &WorkBrief,
    request: &TaskActionRequest,
) -> Result<PlanningActionMenu, &'static str> {
    let visible = validate_brief(brief)?;
    let task = visible
        .get(request.task_id.as_str())
        .copied()
        .ok_or("task outside visible planning scope")?;
    let mut actions = Vec::new();
    actions.push(action(
        brief,
        ActionKind::InspectTask,
        ActionAvailability::Available,
        Some(task),
        None,
        "Inspect accepted task details and dependency context",
        false,
    ));
    if task.project_id.is_some() {
        actions.push(action(
            brief,
            ActionKind::InspectProject,
            ActionAvailability::Available,
            Some(task),
            None,
            "Inspect visible tasks and bottlenecks in this project",
            false,
        ));
    }
    if task.project_id.is_some() && task.milestone_id.is_some() {
        actions.push(action(
            brief,
            ActionKind::InspectMilestone,
            ActionAvailability::Available,
            Some(task),
            None,
            "Inspect milestone work visible to this planning scope",
            false,
        ));
    }
    let mut parents = BTreeSet::new();
    for id in &task.blocked_by {
        if id == &task.id {
            return Err("self dependency in accepted planning brief");
        }
        if visible.contains_key(id.as_str()) {
            parents.insert(id.as_str());
        }
    }
    for prerequisite in parents {
        let parent = visible[prerequisite];
        actions.push(action(
            brief,
            ActionKind::InspectBlocker,
            ActionAvailability::Available,
            Some(parent),
            None,
            "Inspect a visible prerequisite task",
            false,
        ));
        actions.push(action(
            brief,
            ActionKind::RemoveDependency,
            if request.authority_available {
                ActionAvailability::RequiresAuthorityConfirmation
            } else {
                ActionAvailability::Unavailable
            },
            Some(task),
            Some(prerequisite),
            "Review prerequisite removal, then submit an explicit revision-fenced task command",
            true,
        ));
    }
    if !matches!(task.state, WorkItemState::Completed) {
        actions.push(action(
            brief,
            ActionKind::PreviewCompletion,
            ActionAvailability::Available,
            Some(task),
            None,
            "Preview downstream tasks that could become ready; does not complete this task",
            false,
        ));
        if matches!(task.state, WorkItemState::Ready | WorkItemState::Scheduled) {
            actions.push(action(
                brief,
                ActionKind::PreviewSchedule,
                ActionAvailability::Available,
                Some(task),
                None,
                "Generate a non-mutating calendar proposal for this actionable task",
                false,
            ));
            actions.push(action(
                brief,
                ActionKind::ReviewSchedule,
                if request.authority_available {
                    ActionAvailability::RequiresAuthorityConfirmation
                } else {
                    ActionAvailability::Unavailable
                },
                Some(task),
                None,
                "Review exact calendar slots before separately approving an authoritative write",
                true,
            ));
        }
        actions.push(action(
            brief,
            ActionKind::AddDependency,
            if request.authority_available {
                ActionAvailability::RequiresAuthorityConfirmation
            } else {
                ActionAvailability::Unavailable
            },
            Some(task),
            None,
            "Choose a visible prerequisite, preview the edge and submit a revision-fenced mutation",
            true,
        ));
    }
    let hidden = task
        .blocked_by
        .iter()
        .filter(|id| !visible.contains_key(id.as_str()))
        .count();
    let mut notices = vec![
        "Actions are suggestions over accepted state, never automatic mutations".to_owned(),
        "Mutation intents require revalidation and explicit authorization at execution".to_owned(),
    ];
    if hidden > 0 {
        notices.push(format!(
            "{hidden} prerequisite(s) are hidden by the current planning scope"
        ));
    }
    if !request.authority_available {
        notices.push(
            "Authority mutations require an available authority and separate confirmation"
                .to_owned(),
        );
    }
    Ok(PlanningActionMenu {
        revision: brief.revision,
        evaluated_at_ms: brief.evaluated_at_ms,
        selected_task_id: Some(task.id.clone()),
        selected_project_id: task.project_id.clone(),
        selected_milestone_id: task.milestone_id.clone(),
        actions,
        notices,
    })
}
/// Project/milestone menus are restricted to explicitly visible project work.
pub fn group_actions(
    brief: &WorkBrief,
    request: &GroupActionRequest,
) -> Result<PlanningActionMenu, &'static str> {
    let visible = validate_brief(brief)?;
    if request.project_id.trim().is_empty() {
        return Err("project id cannot be empty");
    }
    if request
        .milestone_id
        .as_ref()
        .is_some_and(|m| m.trim().is_empty())
    {
        return Err("milestone id cannot be empty");
    }
    let mut rows: Vec<&WorkItem> = visible
        .values()
        .copied()
        .filter(|t| {
            t.project_id.as_deref() == Some(request.project_id.as_str())
                && request
                    .milestone_id
                    .as_ref()
                    .is_none_or(|m| t.milestone_id.as_ref() == Some(m))
        })
        .collect();
    if rows.is_empty() {
        return Err("project or milestone outside visible planning scope");
    }
    rows.sort_by(|a, b| {
        a.portfolio_rank
            .unwrap_or(usize::MAX)
            .cmp(&b.portfolio_rank.unwrap_or(usize::MAX))
            .then_with(|| a.id.cmp(&b.id))
    });
    let mut actions = Vec::new();
    for task in rows.into_iter().take(30) {
        let kind = if matches!(task.state, WorkItemState::Ready | WorkItemState::Scheduled) {
            ActionKind::PreviewSchedule
        } else {
            ActionKind::InspectTask
        };
        actions.push(action(
            brief,
            kind,
            ActionAvailability::Available,
            Some(task),
            None,
            "Inspect this task or preview its placement in a calendar plan",
            false,
        ));
    }
    let mut notices = vec!["Group suggestions only reference accepted, visible tasks".to_owned()];
    if !request.authority_available {
        notices.push("Approval and execution require an available authority".to_owned());
    }
    Ok(PlanningActionMenu {
        revision: brief.revision,
        evaluated_at_ms: brief.evaluated_at_ms,
        selected_task_id: None,
        selected_project_id: Some(request.project_id.clone()),
        selected_milestone_id: request.milestone_id.clone(),
        actions,
        notices,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::planning_brief::WorkItem;
    fn task(
        id: &str,
        project: Option<&str>,
        milestone: Option<&str>,
        state: WorkItemState,
        parents: &[&str],
        rank: Option<usize>,
    ) -> WorkItem {
        WorkItem {
            id: id.into(),
            title: format!("Title {id}"),
            project_id: project.map(str::to_owned),
            milestone_id: milestone.map(str::to_owned),
            state,
            recommended_rank: None,
            proposed_start_ms: None,
            proposed_end_ms: None,
            deadline_at_ms: None,
            blocked_by: parents.iter().map(|s| s.to_string()).collect(),
            unblocks: vec![],
            explanation: vec![],
            portfolio_rank: rank,
            same_project_downstream: 0,
            other_project_downstream: 0,
            immediately_unblocked: 0,
            portfolio_reasons: vec![],
        }
    }
    fn brief(items: Vec<WorkItem>) -> WorkBrief {
        WorkBrief {
            revision: 12,
            evaluated_at_ms: 100,
            ends_at_ms: 500,
            available_minutes: 5,
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
    fn sample() -> WorkBrief {
        brief(vec![
            task(
                "parent",
                Some("p"),
                Some("m"),
                WorkItemState::Ready,
                &[],
                Some(1),
            ),
            task(
                "child",
                Some("p"),
                Some("m"),
                WorkItemState::Blocked,
                &["parent", "secret"],
                Some(2),
            ),
            task(
                "unrelated",
                Some("q"),
                None,
                WorkItemState::Completed,
                &[],
                None,
            ),
        ])
    }
    fn kinds(menu: &PlanningActionMenu) -> Vec<ActionKind> {
        menu.actions.iter().map(|a| a.kind.clone()).collect()
    }
    #[test]
    fn ready_task_has_preview_and_review_but_no_implicit_write() {
        let input = sample();
        let result = task_actions(
            &input,
            &TaskActionRequest {
                task_id: "parent".into(),
                authority_available: true,
            },
        )
        .unwrap();
        assert!(kinds(&result).contains(&ActionKind::PreviewSchedule));
        assert!(kinds(&result).contains(&ActionKind::ReviewSchedule));
        for action in &result.actions {
            assert_eq!(action.reviewed_revision, 12);
            if action.mutates_authority {
                assert!(action.requires_confirmation);
                assert_eq!(
                    action.availability,
                    ActionAvailability::RequiresAuthorityConfirmation
                );
            } else {
                assert!(!action.requires_confirmation);
            }
        }
    }
    #[test]
    fn offline_mode_disables_write_intentions() {
        let menu = task_actions(
            &sample(),
            &TaskActionRequest {
                task_id: "parent".into(),
                authority_available: false,
            },
        )
        .unwrap();
        assert!(
            menu.actions
                .iter()
                .filter(|a| a.mutates_authority)
                .all(|a| a.availability == ActionAvailability::Unavailable)
        );
        assert!(
            menu.actions
                .iter()
                .any(|a| a.kind == ActionKind::PreviewSchedule
                    && a.availability == ActionAvailability::Available)
        );
    }
    #[test]
    fn hidden_prerequisites_are_counted_not_disclosed() {
        let menu = task_actions(
            &sample(),
            &TaskActionRequest {
                task_id: "child".into(),
                authority_available: true,
            },
        )
        .unwrap();
        let data = serde_json::to_string(&menu).unwrap();
        assert!(!data.contains("secret"));
        assert!(menu.notices.iter().any(|n| n.contains("1 prerequisite")));
        assert_eq!(
            menu.actions
                .iter()
                .filter(|a| a.kind == ActionKind::InspectBlocker)
                .count(),
            1
        );
    }
    #[test]
    fn unrelated_task_is_not_disclosed_in_task_menu() {
        let menu = task_actions(
            &sample(),
            &TaskActionRequest {
                task_id: "child".into(),
                authority_available: true,
            },
        )
        .unwrap();
        assert!(!serde_json::to_string(&menu).unwrap().contains("unrelated"));
    }
    #[test]
    fn completed_tasks_cannot_request_schedule() {
        let menu = task_actions(
            &sample(),
            &TaskActionRequest {
                task_id: "unrelated".into(),
                authority_available: true,
            },
        )
        .unwrap();
        assert!(!kinds(&menu).contains(&ActionKind::PreviewSchedule));
        assert!(!kinds(&menu).contains(&ActionKind::ReviewSchedule));
        assert!(!kinds(&menu).contains(&ActionKind::AddDependency));
    }
    #[test]
    fn reject_unknown_task_even_if_a_prerequisite_names_it() {
        assert!(
            task_actions(
                &sample(),
                &TaskActionRequest {
                    task_id: "secret".into(),
                    authority_available: true
                }
            )
            .is_err()
        );
    }
    #[test]
    fn group_menu_is_project_scoped() {
        let menu = group_actions(
            &sample(),
            &GroupActionRequest {
                project_id: "p".into(),
                milestone_id: None,
                authority_available: true,
            },
        )
        .unwrap();
        assert_eq!(menu.actions.len(), 2);
        assert!(
            menu.actions
                .iter()
                .all(|a| a.project_id.as_deref() == Some("p"))
        );
        assert!(!serde_json::to_string(&menu).unwrap().contains("unrelated"));
    }
    #[test]
    fn group_menu_rejects_unknown_project_and_milestone() {
        assert!(
            group_actions(
                &sample(),
                &GroupActionRequest {
                    project_id: "missing".into(),
                    milestone_id: None,
                    authority_available: true
                }
            )
            .is_err()
        );
        assert!(
            group_actions(
                &sample(),
                &GroupActionRequest {
                    project_id: "p".into(),
                    milestone_id: Some("missing".into()),
                    authority_available: true
                }
            )
            .is_err()
        );
    }
    #[test]
    fn group_menu_deterministic_portfolio_order() {
        let menu = group_actions(
            &sample(),
            &GroupActionRequest {
                project_id: "p".into(),
                milestone_id: Some("m".into()),
                authority_available: true,
            },
        )
        .unwrap();
        assert_eq!(
            menu.actions
                .iter()
                .map(|a| a.task_id.as_deref().unwrap())
                .collect::<Vec<_>>(),
            vec!["parent", "child"]
        );
    }
    #[test]
    fn duplicate_task_ids_fail_closed() {
        let mut source = sample();
        source.items.push(source.items[0].clone());
        assert!(
            task_actions(
                &source,
                &TaskActionRequest {
                    task_id: "parent".into(),
                    authority_available: true
                }
            )
            .is_err()
        );
    }
    #[test]
    fn negative_revision_fails_closed() {
        let mut source = sample();
        source.revision = -1;
        assert!(
            task_actions(
                &source,
                &TaskActionRequest {
                    task_id: "parent".into(),
                    authority_available: true
                }
            )
            .is_err()
        );
    }
    #[test]
    fn self_dependency_fails_closed() {
        let mut source = sample();
        source.items[0].blocked_by.push("parent".into());
        assert!(
            task_actions(
                &source,
                &TaskActionRequest {
                    task_id: "parent".into(),
                    authority_available: true
                }
            )
            .is_err()
        );
    }
    #[test]
    fn task_menu_does_not_mutate_input_snapshot() {
        let source = sample();
        let before = source.clone();
        let _ = task_actions(
            &source,
            &TaskActionRequest {
                task_id: "parent".into(),
                authority_available: true,
            },
        )
        .unwrap();
        assert_eq!(source, before);
    }
    #[test]
    fn serialization_contains_no_executable_approval_token() {
        let menu = task_actions(
            &sample(),
            &TaskActionRequest {
                task_id: "parent".into(),
                authority_available: true,
            },
        )
        .unwrap();
        let wire = serde_json::to_string(&menu).unwrap();
        assert!(!wire.contains("approved_plan"));
        assert!(!wire.contains("command_id"));
    }
}

/// A UI or agent selection is only a request to resolve an advertised action;
/// it is not an authority command and never contains credentials or approval.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionSelection {
    pub reviewed_revision: i64,
    pub action_index: usize,
    pub kind: ActionKind,
    pub task_id: Option<String>,
    pub prerequisite_id: Option<String>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionDestination {
    TaskContext,
    ProjectContext,
    MilestoneContext,
    CompletionPreview,
    CalendarPreview,
    CalendarReview,
    DependencyPreview,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedAction {
    pub revision: i64,
    pub destination: ActionDestination,
    pub task_id: Option<String>,
    pub project_id: Option<String>,
    pub milestone_id: Option<String>,
    pub prerequisite_id: Option<String>,
    /// The caller must choose a visible prerequisite before an add operation.
    pub prerequisite_selection_required: bool,
    /// The result NEVER authorizes a mutation. A separate command must be
    /// freshly reviewed and approved against the authoritative revision.
    pub confirmation_required: bool,
    pub executable: bool,
}

/// Safely resolve a selection from a *recomputed* menu at the current accepted
/// revision. No caller-supplied command payload is trusted as authority.
pub fn resolve_task_action(
    brief: &WorkBrief,
    request: &TaskActionRequest,
    selection: &ActionSelection,
) -> Result<ResolvedAction, &'static str> {
    if selection.reviewed_revision != brief.revision {
        return Err("action selection was reviewed at a different revision");
    }
    let menu = task_actions(brief, request)?;
    resolve_menu_action(&menu, selection)
}
pub fn resolve_group_action(
    brief: &WorkBrief,
    request: &GroupActionRequest,
    selection: &ActionSelection,
) -> Result<ResolvedAction, &'static str> {
    if selection.reviewed_revision != brief.revision {
        return Err("action selection was reviewed at a different revision");
    }
    let menu = group_actions(brief, request)?;
    resolve_menu_action(&menu, selection)
}
fn resolve_menu_action(
    menu: &PlanningActionMenu,
    selection: &ActionSelection,
) -> Result<ResolvedAction, &'static str> {
    if selection.reviewed_revision != menu.revision {
        return Err("action menu revision changed");
    }
    let selected = menu
        .actions
        .get(selection.action_index)
        .ok_or("action index outside reviewed menu")?;
    if selected.kind != selection.kind
        || selected.task_id != selection.task_id
        || selected.prerequisite_id != selection.prerequisite_id
    {
        return Err("action selection no longer matches accepted menu");
    }
    if selected.availability == ActionAvailability::Unavailable {
        return Err("action is not available in this authority mode");
    }
    let destination = match selected.kind {
        ActionKind::InspectTask | ActionKind::InspectBlocker => ActionDestination::TaskContext,
        ActionKind::InspectProject => ActionDestination::ProjectContext,
        ActionKind::InspectMilestone => ActionDestination::MilestoneContext,
        ActionKind::PreviewCompletion => ActionDestination::CompletionPreview,
        ActionKind::PreviewSchedule => ActionDestination::CalendarPreview,
        ActionKind::ReviewSchedule => ActionDestination::CalendarReview,
        ActionKind::AddDependency | ActionKind::RemoveDependency => {
            ActionDestination::DependencyPreview
        }
    };
    Ok(ResolvedAction {
        revision: menu.revision,
        destination,
        task_id: selected.task_id.clone(),
        project_id: selected.project_id.clone(),
        milestone_id: selected.milestone_id.clone(),
        prerequisite_id: selected.prerequisite_id.clone(),
        prerequisite_selection_required: selected.kind == ActionKind::AddDependency,
        confirmation_required: selected.requires_confirmation,
        executable: false,
    })
}

#[cfg(test)]
mod action_selection_tests {
    use super::*;
    fn item(id: &str, project: &str, state: WorkItemState, parents: &[&str]) -> WorkItem {
        WorkItem {
            id: id.into(),
            title: id.into(),
            project_id: Some(project.into()),
            milestone_id: Some("m".into()),
            state,
            recommended_rank: None,
            proposed_start_ms: None,
            proposed_end_ms: None,
            deadline_at_ms: None,
            blocked_by: parents.iter().map(|s| s.to_string()).collect(),
            unblocks: vec![],
            explanation: vec![],
            portfolio_rank: None,
            same_project_downstream: 0,
            other_project_downstream: 0,
            immediately_unblocked: 0,
            portfolio_reasons: vec![],
        }
    }
    fn brief() -> WorkBrief {
        WorkBrief {
            revision: 7,
            evaluated_at_ms: 30,
            ends_at_ms: 200,
            available_minutes: 10,
            proposed_minutes: 0,
            scheduled_count: 0,
            ready_count: 0,
            blocked_count: 0,
            deferred_count: 0,
            completed_count: 0,
            items: vec![
                item("a", "alpha", WorkItemState::Ready, &[]),
                item("b", "alpha", WorkItemState::Blocked, &["a", "hidden"]),
                item("c", "beta", WorkItemState::Completed, &[]),
            ],
            projects: vec![],
            milestones: vec![],
            warnings: vec![],
            portfolio_actionable_total: 0,
            cross_project_leverage_count: 0,
        }
    }
    fn selection(menu: &PlanningActionMenu, index: usize) -> ActionSelection {
        let action = &menu.actions[index];
        ActionSelection {
            reviewed_revision: menu.revision,
            action_index: index,
            kind: action.kind.clone(),
            task_id: action.task_id.clone(),
            prerequisite_id: action.prerequisite_id.clone(),
        }
    }
    #[test]
    fn task_navigation_can_be_resolved_without_mutation() {
        let source = brief();
        let request = TaskActionRequest {
            task_id: "b".into(),
            authority_available: false,
        };
        let menu = task_actions(&source, &request).unwrap();
        let index = menu
            .actions
            .iter()
            .position(|a| a.kind == ActionKind::InspectBlocker)
            .unwrap();
        let result = resolve_task_action(&source, &request, &selection(&menu, index)).unwrap();
        assert_eq!(result.destination, ActionDestination::TaskContext);
        assert_eq!(result.task_id.as_deref(), Some("a"));
        assert_eq!(result.revision, 7);
        assert!(!result.executable);
        assert!(!result.confirmation_required);
    }
    #[test]
    fn mutation_selection_resolves_only_to_a_preview() {
        let source = brief();
        let request = TaskActionRequest {
            task_id: "b".into(),
            authority_available: true,
        };
        let menu = task_actions(&source, &request).unwrap();
        let index = menu
            .actions
            .iter()
            .position(|a| a.kind == ActionKind::RemoveDependency)
            .unwrap();
        let resolved = resolve_task_action(&source, &request, &selection(&menu, index)).unwrap();
        assert_eq!(resolved.destination, ActionDestination::DependencyPreview);
        assert!(resolved.confirmation_required);
        assert!(!resolved.executable);
        assert_eq!(resolved.prerequisite_id.as_deref(), Some("a"));
    }
    #[test]
    fn cannot_resolve_mutation_while_offline() {
        let source = brief();
        let online = TaskActionRequest {
            task_id: "b".into(),
            authority_available: true,
        };
        let offline = TaskActionRequest {
            task_id: "b".into(),
            authority_available: false,
        };
        let menu = task_actions(&source, &online).unwrap();
        let index = menu
            .actions
            .iter()
            .position(|a| a.kind == ActionKind::RemoveDependency)
            .unwrap();
        assert!(resolve_task_action(&source, &offline, &selection(&menu, index)).is_err());
    }
    #[test]
    fn cannot_resolve_action_from_stale_revision() {
        let source = brief();
        let request = TaskActionRequest {
            task_id: "a".into(),
            authority_available: true,
        };
        let menu = task_actions(&source, &request).unwrap();
        let mut newer = source.clone();
        newer.revision = 8;
        assert!(resolve_task_action(&newer, &request, &selection(&menu, 0)).is_err());
    }
    #[test]
    fn forged_action_kind_is_rejected() {
        let source = brief();
        let request = TaskActionRequest {
            task_id: "a".into(),
            authority_available: true,
        };
        let menu = task_actions(&source, &request).unwrap();
        let mut requested = selection(&menu, 0);
        requested.kind = ActionKind::AddDependency;
        assert!(resolve_task_action(&source, &request, &requested).is_err());
    }
    #[test]
    fn forged_task_id_is_rejected() {
        let source = brief();
        let request = TaskActionRequest {
            task_id: "a".into(),
            authority_available: true,
        };
        let menu = task_actions(&source, &request).unwrap();
        let mut requested = selection(&menu, 0);
        requested.task_id = Some("c".into());
        assert!(resolve_task_action(&source, &request, &requested).is_err());
    }
    #[test]
    fn stale_index_is_rejected() {
        let source = brief();
        let request = TaskActionRequest {
            task_id: "a".into(),
            authority_available: true,
        };
        let menu = task_actions(&source, &request).unwrap();
        let mut requested = selection(&menu, 0);
        requested.action_index = usize::MAX;
        assert!(resolve_task_action(&source, &request, &requested).is_err());
    }
    #[test]
    fn hidden_prerequisite_cannot_be_injected_into_selection() {
        let source = brief();
        let request = TaskActionRequest {
            task_id: "b".into(),
            authority_available: true,
        };
        let menu = task_actions(&source, &request).unwrap();
        let mut requested = selection(&menu, 0);
        requested.prerequisite_id = Some("hidden".into());
        assert!(resolve_task_action(&source, &request, &requested).is_err());
    }
    #[test]
    fn group_actions_resolve_only_visible_project_task() {
        let source = brief();
        let request = GroupActionRequest {
            project_id: "alpha".into(),
            milestone_id: None,
            authority_available: false,
        };
        let menu = group_actions(&source, &request).unwrap();
        assert_eq!(menu.actions.len(), 2);
        let resolved = resolve_group_action(&source, &request, &selection(&menu, 0)).unwrap();
        assert_eq!(resolved.project_id.as_deref(), Some("alpha"));
        assert!(!resolved.executable);
    }
    #[test]
    fn group_resolution_does_not_follow_scope_change() {
        let source = brief();
        let original = GroupActionRequest {
            project_id: "alpha".into(),
            milestone_id: None,
            authority_available: true,
        };
        let changed = GroupActionRequest {
            project_id: "beta".into(),
            milestone_id: None,
            authority_available: true,
        };
        let menu = group_actions(&source, &original).unwrap();
        assert!(resolve_group_action(&source, &changed, &selection(&menu, 0)).is_err());
    }
    #[test]
    fn add_dependency_never_selects_a_hidden_prerequisite_implicitly() {
        let source = brief();
        let request = TaskActionRequest {
            task_id: "a".into(),
            authority_available: true,
        };
        let menu = task_actions(&source, &request).unwrap();
        let i = menu
            .actions
            .iter()
            .position(|a| a.kind == ActionKind::AddDependency)
            .unwrap();
        let resolved = resolve_task_action(&source, &request, &selection(&menu, i)).unwrap();
        assert!(resolved.prerequisite_selection_required);
        assert!(resolved.prerequisite_id.is_none());
    }
    #[test]
    fn serialized_resolution_cannot_be_a_write_receipt() {
        let source = brief();
        let request = TaskActionRequest {
            task_id: "a".into(),
            authority_available: true,
        };
        let menu = task_actions(&source, &request).unwrap();
        let result = resolve_task_action(&source, &request, &selection(&menu, 0)).unwrap();
        let wire = serde_json::to_string(&result).unwrap();
        assert!(wire.contains("\"executable\":false"));
        assert!(!wire.contains("command_id"));
        assert!(!wire.contains("approved"));
    }
}
