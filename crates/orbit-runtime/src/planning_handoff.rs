//! Revision-bound dispatch of accepted planning action selections.
//!
//! A handoff is a read-only navigation result. In particular it is never an
//! authorization token for a task command or for schedule approval.
use crate::{
    planning_actions::{
        self, ActionDestination, ActionSelection, GroupActionRequest, ResolvedAction,
        TaskActionRequest,
    },
    planning_brief::{WorkBrief, WorkItem},
    planning_focus::{self, FocusRequest, GroupFocus, GroupFocusRequest, TaskFocus},
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "view", content = "data", rename_all = "snake_case")]
pub enum HandoffView {
    Task(Box<TaskFocus>),
    Project(GroupFocus),
    /// A preview is an instruction, not a mutation or an implicit approval.
    Preview(PreviewInstruction),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PreviewInstruction {
    pub revision: i64,
    pub operation: PreviewOperation,
    pub task_id: String,
    pub prerequisite_id: Option<String>,
    pub evaluated_at_ms: i64,
    pub window_end_ms: i64,
    pub explanation: String,
    pub requires_explicit_approval: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PreviewOperation {
    CompletionImpact,
    CalendarSlots,
    CalendarReview,
    DependencyAddition,
    DependencyRemoval,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanningHandoff {
    pub revision: i64,
    pub destination: ActionDestination,
    pub view: HandoffView,
    pub authoritative_write_performed: bool,
    pub notes: Vec<String>,
}

fn visible<'a>(brief: &'a WorkBrief, id: &str) -> Result<&'a WorkItem, &'static str> {
    if id.trim().is_empty() {
        return Err("empty task identity");
    }
    let mut matches = brief.items.iter().filter(|row| row.id == id);
    let one = matches
        .next()
        .ok_or("handoff target outside accepted visible planning scope")?;
    if matches.next().is_some() {
        return Err("duplicate accepted visible task identity");
    }
    Ok(one)
}

/// Construct a selection from an actually rendered menu, not arbitrary input.
/// Display adapters use this when converting a 1-based menu choice to a
/// revision-bound selection. JSON clients may still submit a selection; the
/// dispatch functions independently revalidate it.
pub fn select_from_menu(
    menu: &planning_actions::PlanningActionMenu,
    index: usize,
) -> Result<ActionSelection, &'static str> {
    let action = menu
        .actions
        .get(index)
        .ok_or("action index outside reviewed menu")?;
    Ok(ActionSelection {
        reviewed_revision: menu.revision,
        action_index: index,
        kind: action.kind.clone(),
        task_id: action.task_id.clone(),
        prerequisite_id: action.prerequisite_id.clone(),
    })
}

/// Recompute the menu and resolve the exact selected entry at the accepted
/// revision before building any destination. Callers MUST NOT construct a
/// ResolvedAction directly to bypass this check.
pub fn dispatch_task(
    brief: &WorkBrief,
    request: &TaskActionRequest,
    selection: &ActionSelection,
) -> Result<PlanningHandoff, &'static str> {
    let resolved = planning_actions::resolve_task_action(brief, request, selection)?;
    dispatch_resolved(brief, resolved)
}
pub fn dispatch_group(
    brief: &WorkBrief,
    request: &GroupActionRequest,
    selection: &ActionSelection,
) -> Result<PlanningHandoff, &'static str> {
    let resolved = planning_actions::resolve_group_action(brief, request, selection)?;
    dispatch_resolved(brief, resolved)
}
fn dispatch_resolved(
    brief: &WorkBrief,
    action: ResolvedAction,
) -> Result<PlanningHandoff, &'static str> {
    if brief.revision != action.revision || action.executable {
        return Err("invalid revision or executable action handoff");
    }
    let destination = action.destination;
    let view = match destination {
        ActionDestination::TaskContext => {
            let task_id = action
                .task_id
                .as_deref()
                .ok_or("task navigation missing target")?;
            visible(brief, task_id)?;
            HandoffView::Task(Box::new(planning_focus::focus_task(
                brief,
                &FocusRequest {
                    task_id: task_id.into(),
                    max_neighbors: 20,
                },
            )?))
        }
        ActionDestination::ProjectContext | ActionDestination::MilestoneContext => {
            let project_id = action
                .project_id
                .as_deref()
                .ok_or("project navigation missing target")?;
            if matches!(destination, ActionDestination::MilestoneContext)
                && action.milestone_id.is_none()
            {
                return Err("milestone navigation missing target");
            }
            if let Some(task_id) = action.task_id.as_deref() {
                let row = visible(brief, task_id)?;
                if row.project_id.as_deref() != Some(project_id)
                    || (matches!(destination, ActionDestination::MilestoneContext)
                        && row.milestone_id != action.milestone_id)
                {
                    return Err("navigation target differs from accepted task context");
                }
            }
            HandoffView::Project(planning_focus::focus_group(
                brief,
                &GroupFocusRequest {
                    project_id: project_id.into(),
                    milestone_id: action
                        .milestone_id
                        .clone()
                        .filter(|_| matches!(destination, ActionDestination::MilestoneContext)),
                    max_tasks: 30,
                },
            )?)
        }
        ActionDestination::CompletionPreview
        | ActionDestination::CalendarPreview
        | ActionDestination::CalendarReview
        | ActionDestination::DependencyPreview => {
            let task_id = action.task_id.as_deref().ok_or("preview missing target")?;
            let task = visible(brief, task_id)?;
            let (operation, explanation) = match destination {
                ActionDestination::CompletionPreview => (
                    PreviewOperation::CompletionImpact,
                    "Open the completion-impact simulator; do not complete the task",
                ),
                ActionDestination::CalendarPreview => (
                    PreviewOperation::CalendarSlots,
                    "Open a read-only calendar slot proposal",
                ),
                ActionDestination::CalendarReview => (
                    PreviewOperation::CalendarReview,
                    "Prepare an exact plan review; approval must be separate and explicit",
                ),
                _ => {
                    if let Some(parent) = action.prerequisite_id.as_deref() {
                        visible(brief, parent)?;
                        if !task.blocked_by.iter().any(|id| id == parent) {
                            return Err("dependency no longer present");
                        }
                        (
                            PreviewOperation::DependencyRemoval,
                            "Preview removing this existing prerequisite; no mutation",
                        )
                    } else {
                        (
                            PreviewOperation::DependencyAddition,
                            "Choose a visible prerequisite, then preview its addition; no mutation",
                        )
                    }
                }
            };
            HandoffView::Preview(PreviewInstruction {
                revision: brief.revision,
                operation,
                task_id: task.id.clone(),
                prerequisite_id: action.prerequisite_id.clone(),
                evaluated_at_ms: brief.evaluated_at_ms,
                window_end_ms: brief.ends_at_ms,
                explanation: explanation.into(),
                requires_explicit_approval: action.confirmation_required,
            })
        }
    };
    Ok(PlanningHandoff {
        revision: brief.revision,
        destination,
        view,
        authoritative_write_performed: false,
        notes: vec![
            "Read-only action handoff; no authoritative write has occurred".into(),
            "Revalidate accepted revision before any separate mutation or calendar approval".into(),
        ],
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::planning_actions::{ActionAvailability, ActionKind};
    use crate::planning_brief::WorkItemState;

    fn row(
        id: &str,
        project: &str,
        milestone: &str,
        state: WorkItemState,
        blockers: &[&str],
        rank: usize,
    ) -> WorkItem {
        WorkItem {
            id: id.into(),
            title: format!("Task {id}"),
            project_id: Some(project.into()),
            milestone_id: Some(milestone.into()),
            state,
            recommended_rank: Some(rank),
            proposed_start_ms: None,
            proposed_end_ms: None,
            deadline_at_ms: None,
            blocked_by: blockers.iter().map(|id| id.to_string()).collect(),
            unblocks: Vec::new(),
            explanation: Vec::new(),
            portfolio_rank: Some(rank),
            same_project_downstream: 0,
            other_project_downstream: 0,
            immediately_unblocked: 0,
            portfolio_reasons: Vec::new(),
        }
    }
    fn sample() -> WorkBrief {
        WorkBrief {
            revision: 25,
            evaluated_at_ms: 1_000,
            ends_at_ms: 3_601_000,
            available_minutes: 60,
            proposed_minutes: 0,
            scheduled_count: 0,
            ready_count: 2,
            blocked_count: 1,
            deferred_count: 0,
            completed_count: 1,
            items: vec![
                row("schema", "core", "foundation", WorkItemState::Ready, &[], 0),
                row(
                    "auth",
                    "app",
                    "launch",
                    WorkItemState::Blocked,
                    &["schema", "secret"],
                    1,
                ),
                row("tests", "app", "launch", WorkItemState::Ready, &[], 2),
                row(
                    "done",
                    "core",
                    "foundation",
                    WorkItemState::Completed,
                    &[],
                    3,
                ),
            ],
            projects: Vec::new(),
            milestones: Vec::new(),
            warnings: Vec::new(),
            portfolio_actionable_total: 2,
            cross_project_leverage_count: 1,
        }
    }
    fn request(id: &str, connected: bool) -> TaskActionRequest {
        TaskActionRequest {
            task_id: id.into(),
            authority_available: connected,
        }
    }
    fn select(menu: &planning_actions::PlanningActionMenu, kind: ActionKind) -> ActionSelection {
        let index = menu
            .actions
            .iter()
            .position(|action| action.kind == kind)
            .unwrap();
        let action = &menu.actions[index];
        ActionSelection {
            reviewed_revision: menu.revision,
            action_index: index,
            kind: action.kind.clone(),
            task_id: action.task_id.clone(),
            prerequisite_id: action.prerequisite_id.clone(),
        }
    }
    fn select_parent(menu: &planning_actions::PlanningActionMenu, id: &str) -> ActionSelection {
        let index = menu
            .actions
            .iter()
            .position(|action| {
                action.kind == ActionKind::InspectBlocker && action.task_id.as_deref() == Some(id)
            })
            .unwrap();
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
    fn selection_is_bound_to_the_exact_rendered_index() {
        let brief = sample();
        let req = request("auth", true);
        let menu = planning_actions::task_actions(&brief, &req).unwrap();
        for index in 0..menu.actions.len() {
            let selected = select_from_menu(&menu, index).unwrap();
            let action = &menu.actions[index];
            assert_eq!(selected.reviewed_revision, 25);
            assert_eq!(selected.action_index, index);
            assert_eq!(selected.kind, action.kind);
            assert_eq!(selected.task_id, action.task_id);
            assert_eq!(selected.prerequisite_id, action.prerequisite_id);
        }
        assert!(select_from_menu(&menu, menu.actions.len()).is_err());
        assert!(select_from_menu(&menu, usize::MAX).is_err());
    }
    #[test]
    fn selected_task_opens_actual_accepted_focus() {
        let brief = sample();
        let req = request("auth", true);
        let menu = planning_actions::task_actions(&brief, &req).unwrap();
        let handoff = dispatch_task(&brief, &req, &select(&menu, ActionKind::InspectTask)).unwrap();
        assert_eq!(handoff.revision, 25);
        assert_eq!(handoff.destination, ActionDestination::TaskContext);
        assert!(!handoff.authoritative_write_performed);
        if let HandoffView::Task(focus) = handoff.view {
            assert_eq!(focus.task.id, "auth");
            assert_eq!(focus.visible_prerequisites.len(), 1);
            assert_eq!(focus.visible_prerequisites[0].task_id, "schema");
            assert_eq!(focus.hidden_prerequisite_count, 1);
        } else {
            panic!("expected a task drilldown");
        }
    }
    #[test]
    fn blocker_handoff_follows_only_visible_accepted_prerequisite() {
        let brief = sample();
        let req = request("auth", true);
        let menu = planning_actions::task_actions(&brief, &req).unwrap();
        assert!(
            !menu
                .actions
                .iter()
                .any(|a| a.task_id.as_deref() == Some("secret"))
        );
        let handoff = dispatch_task(&brief, &req, &select_parent(&menu, "schema")).unwrap();
        if let HandoffView::Task(focus) = handoff.view {
            assert_eq!(focus.task.id, "schema");
            assert_eq!(focus.task.project_id.as_deref(), Some("core"));
        } else {
            panic!("visible blocker must open task focus");
        }
    }
    #[test]
    fn project_action_filters_to_accepted_project() {
        let brief = sample();
        let req = request("auth", true);
        let menu = planning_actions::task_actions(&brief, &req).unwrap();
        let handoff =
            dispatch_task(&brief, &req, &select(&menu, ActionKind::InspectProject)).unwrap();
        if let HandoffView::Project(group) = handoff.view {
            assert_eq!(group.project.project_id.as_deref(), Some("app"));
            assert_eq!(group.project.task_count, 2);
        } else {
            panic!("project view expected");
        }
    }
    #[test]
    fn milestone_action_filters_to_accepted_milestone() {
        let brief = sample();
        let req = request("auth", true);
        let menu = planning_actions::task_actions(&brief, &req).unwrap();
        let handoff =
            dispatch_task(&brief, &req, &select(&menu, ActionKind::InspectMilestone)).unwrap();
        assert_eq!(handoff.destination, ActionDestination::MilestoneContext);
        if let HandoffView::Project(group) = handoff.view {
            assert_eq!(group.project.project_id.as_deref(), Some("app"));
            assert_eq!(group.project.task_count, 2);
        } else {
            panic!("milestone drilldown expected");
        }
    }
    #[test]
    fn group_action_opens_visible_task() {
        let brief = sample();
        let req = GroupActionRequest {
            project_id: "app".into(),
            milestone_id: Some("launch".into()),
            authority_available: true,
        };
        let menu = planning_actions::group_actions(&brief, &req).unwrap();
        let index = menu
            .actions
            .iter()
            .position(|a| a.kind == ActionKind::InspectTask)
            .unwrap();
        let action = &menu.actions[index];
        let selection = ActionSelection {
            reviewed_revision: menu.revision,
            action_index: index,
            kind: action.kind.clone(),
            task_id: action.task_id.clone(),
            prerequisite_id: None,
        };
        let report = dispatch_group(&brief, &req, &selection).unwrap();
        assert!(matches!(report.view, HandoffView::Task(_)));
    }
    #[test]
    fn completion_action_is_never_a_completion_command() {
        let brief = sample();
        let req = request("auth", true);
        let menu = planning_actions::task_actions(&brief, &req).unwrap();
        let handoff =
            dispatch_task(&brief, &req, &select(&menu, ActionKind::PreviewCompletion)).unwrap();
        if let HandoffView::Preview(preview) = handoff.view {
            assert_eq!(preview.operation, PreviewOperation::CompletionImpact);
            assert_eq!(preview.task_id, "auth");
            assert!(!preview.requires_explicit_approval);
        } else {
            panic!("completion should not change state");
        }
        assert!(!handoff.authoritative_write_performed);
    }
    #[test]
    fn schedule_review_requires_separate_explicit_approval() {
        let brief = sample();
        let req = request("tests", true);
        let menu = planning_actions::task_actions(&brief, &req).unwrap();
        let handoff =
            dispatch_task(&brief, &req, &select(&menu, ActionKind::ReviewSchedule)).unwrap();
        if let HandoffView::Preview(preview) = handoff.view {
            assert_eq!(preview.operation, PreviewOperation::CalendarReview);
            assert!(preview.requires_explicit_approval);
            assert_eq!(preview.evaluated_at_ms, 1_000);
            assert_eq!(preview.window_end_ms, 3_601_000);
        } else {
            panic!("approval must be separate");
        }
        assert!(!handoff.authoritative_write_performed);
    }
    #[test]
    fn offline_schedule_review_is_unavailable() {
        let brief = sample();
        let req = request("tests", false);
        let menu = planning_actions::task_actions(&brief, &req).unwrap();
        let selection = select(&menu, ActionKind::ReviewSchedule);
        assert_eq!(
            menu.actions[selection.action_index].availability,
            ActionAvailability::Unavailable
        );
        assert!(dispatch_task(&brief, &req, &selection).is_err());
    }
    #[test]
    fn removing_an_existing_dependency_is_only_a_preview() {
        let brief = sample();
        let req = request("auth", true);
        let menu = planning_actions::task_actions(&brief, &req).unwrap();
        let report =
            dispatch_task(&brief, &req, &select(&menu, ActionKind::RemoveDependency)).unwrap();
        if let HandoffView::Preview(preview) = report.view {
            assert_eq!(preview.operation, PreviewOperation::DependencyRemoval);
            assert_eq!(preview.prerequisite_id.as_deref(), Some("schema"));
            assert!(preview.requires_explicit_approval);
        } else {
            panic!("dependency removal must remain nonmutating");
        }
    }
    #[test]
    fn adding_a_dependency_does_not_guess_prerequisite() {
        let brief = sample();
        let req = request("tests", true);
        let menu = planning_actions::task_actions(&brief, &req).unwrap();
        let report =
            dispatch_task(&brief, &req, &select(&menu, ActionKind::AddDependency)).unwrap();
        if let HandoffView::Preview(preview) = report.view {
            assert_eq!(preview.operation, PreviewOperation::DependencyAddition);
            assert_eq!(preview.prerequisite_id, None);
            assert!(preview.requires_explicit_approval);
        } else {
            panic!("addition should request a prerequisite");
        }
    }
    #[test]
    fn old_revision_cannot_open_a_newer_accepted_view() {
        let mut brief = sample();
        let req = request("tests", true);
        let menu = planning_actions::task_actions(&brief, &req).unwrap();
        let selection = select(&menu, ActionKind::InspectTask);
        brief.revision += 1;
        assert!(dispatch_task(&brief, &req, &selection).is_err());
    }
    #[test]
    fn forged_task_identity_cannot_redirect_handoff() {
        let brief = sample();
        let req = request("tests", true);
        let menu = planning_actions::task_actions(&brief, &req).unwrap();
        let mut selection = select(&menu, ActionKind::InspectTask);
        selection.task_id = Some("secret".into());
        assert!(dispatch_task(&brief, &req, &selection).is_err());
    }
    #[test]
    fn forged_kind_cannot_redirect_handoff() {
        let brief = sample();
        let req = request("tests", true);
        let menu = planning_actions::task_actions(&brief, &req).unwrap();
        let mut selection = select(&menu, ActionKind::InspectTask);
        selection.kind = ActionKind::ReviewSchedule;
        assert!(dispatch_task(&brief, &req, &selection).is_err());
    }
    #[test]
    fn changed_visible_scope_rejects_hidden_destination() {
        let mut brief = sample();
        let req = request("tests", true);
        let menu = planning_actions::task_actions(&brief, &req).unwrap();
        let selection = select(&menu, ActionKind::InspectProject);
        brief
            .items
            .retain(|item| item.project_id.as_deref() == Some("core"));
        assert!(dispatch_task(&brief, &req, &selection).is_err());
    }
    #[test]
    fn malformed_group_context_is_rejected() {
        let brief = sample();
        let req = GroupActionRequest {
            project_id: "missing".into(),
            milestone_id: None,
            authority_available: true,
        };
        let selection = ActionSelection {
            reviewed_revision: 25,
            action_index: 0,
            kind: ActionKind::InspectTask,
            task_id: Some("auth".into()),
            prerequisite_id: None,
        };
        assert!(dispatch_group(&brief, &req, &selection).is_err());
    }
    #[test]
    fn all_handoffs_serialize_as_explicit_nonexecuting_views() {
        let brief = sample();
        let req = request("tests", true);
        let menu = planning_actions::task_actions(&brief, &req).unwrap();
        for action in &menu.actions {
            let selection = select(&menu, action.kind.clone());
            let report = dispatch_task(&brief, &req, &selection).unwrap();
            let json = serde_json::to_string(&report).unwrap();
            assert!(json.contains("\"authoritative_write_performed\":false"));
            assert!(!json.contains("approved_plan"));
            assert!(!json.contains("command_receipt"));
        }
    }
}

#[cfg(test)]
mod integration_regression_tests {
    use super::*;
    use crate::planning_actions::{ActionAvailability, ActionKind};
    use crate::planning_brief::{WorkItem, WorkItemState};

    fn item(id: &str, project: Option<&str>, blockers: &[&str]) -> WorkItem {
        WorkItem {
            id: id.into(),
            title: format!("Visible {id}"),
            project_id: project.map(str::to_owned),
            milestone_id: Some("ship".into()),
            state: if blockers.is_empty() {
                WorkItemState::Ready
            } else {
                WorkItemState::Blocked
            },
            recommended_rank: None,
            proposed_start_ms: None,
            proposed_end_ms: None,
            deadline_at_ms: None,
            blocked_by: blockers.iter().map(|id| id.to_string()).collect(),
            unblocks: Vec::new(),
            explanation: Vec::new(),
            portfolio_rank: None,
            same_project_downstream: 0,
            other_project_downstream: 0,
            immediately_unblocked: 0,
            portfolio_reasons: Vec::new(),
        }
    }
    fn brief() -> WorkBrief {
        WorkBrief {
            revision: 100,
            evaluated_at_ms: 60,
            ends_at_ms: 3_600_060,
            available_minutes: 60,
            proposed_minutes: 0,
            scheduled_count: 0,
            ready_count: 3,
            blocked_count: 1,
            deferred_count: 0,
            completed_count: 0,
            items: vec![
                item("shared", Some("infra"), &[]),
                item("client", Some("app"), &["shared", "hidden"]),
                item("test", Some("app"), &[]),
                item("misc", None, &[]),
            ],
            projects: Vec::new(),
            milestones: Vec::new(),
            warnings: Vec::new(),
            portfolio_actionable_total: 3,
            cross_project_leverage_count: 1,
        }
    }
    #[test]
    fn selecting_every_available_action_never_returns_an_executable_result() {
        let brief = brief();
        for task in &brief.items {
            let request = TaskActionRequest {
                task_id: task.id.clone(),
                authority_available: true,
            };
            let menu = planning_actions::task_actions(&brief, &request).unwrap();
            for index in 0..menu.actions.len() {
                let selection = select_from_menu(&menu, index).unwrap();
                let result = dispatch_task(&brief, &request, &selection).unwrap();
                assert_eq!(result.revision, brief.revision);
                assert!(!result.authoritative_write_performed);
                assert!(!result.notes.is_empty());
                let encoded = serde_json::to_value(result).unwrap();
                assert_eq!(encoded["authoritative_write_performed"], false);
                assert!(encoded.get("command_id").is_none());
                assert!(encoded.get("receipt").is_none());
            }
        }
    }
    #[test]
    fn offline_availability_is_enforced_by_recomputed_menu() {
        let brief = brief();
        let connected = TaskActionRequest {
            task_id: "test".into(),
            authority_available: true,
        };
        let offline = TaskActionRequest {
            task_id: "test".into(),
            authority_available: false,
        };
        let menu = planning_actions::task_actions(&brief, &connected).unwrap();
        let index = menu
            .actions
            .iter()
            .position(|action| action.kind == ActionKind::ReviewSchedule)
            .unwrap();
        let selection = select_from_menu(&menu, index).unwrap();
        assert!(dispatch_task(&brief, &offline, &selection).is_err());
        let offline_menu = planning_actions::task_actions(&brief, &offline).unwrap();
        assert_eq!(
            offline_menu.actions[index].availability,
            ActionAvailability::Unavailable
        );
    }
    #[test]
    fn spoofed_cross_project_prerequisite_is_rejected() {
        let brief = brief();
        let request = TaskActionRequest {
            task_id: "client".into(),
            authority_available: true,
        };
        let menu = planning_actions::task_actions(&brief, &request).unwrap();
        let index = menu
            .actions
            .iter()
            .position(|action| action.kind == ActionKind::RemoveDependency)
            .unwrap();
        let mut selection = select_from_menu(&menu, index).unwrap();
        selection.prerequisite_id = Some("hidden".into());
        assert!(dispatch_task(&brief, &request, &selection).is_err());
        selection.prerequisite_id = Some("shared".into());
        let handoff = dispatch_task(&brief, &request, &selection).unwrap();
        if let HandoffView::Preview(preview) = handoff.view {
            assert_eq!(preview.prerequisite_id.as_deref(), Some("shared"));
        } else {
            panic!("expected a preview");
        }
    }
    #[test]
    fn hidden_tasks_never_become_navigation_targets() {
        let brief = brief();
        let request = TaskActionRequest {
            task_id: "client".into(),
            authority_available: true,
        };
        let menu = planning_actions::task_actions(&brief, &request).unwrap();
        assert!(
            menu.actions
                .iter()
                .all(|action| action.task_id.as_deref() != Some("hidden"))
        );
        let selection = ActionSelection {
            reviewed_revision: brief.revision,
            action_index: 0,
            kind: ActionKind::InspectTask,
            task_id: Some("hidden".into()),
            prerequisite_id: None,
        };
        assert!(dispatch_task(&brief, &request, &selection).is_err());
    }
    #[test]
    fn group_scoping_cannot_be_changed_between_menu_and_handoff() {
        let brief = brief();
        let request = GroupActionRequest {
            project_id: "app".into(),
            milestone_id: None,
            authority_available: true,
        };
        let menu = planning_actions::group_actions(&brief, &request).unwrap();
        let selection = select_from_menu(&menu, 0).unwrap();
        let wrong = GroupActionRequest {
            project_id: "infra".into(),
            milestone_id: None,
            authority_available: true,
        };
        assert!(dispatch_group(&brief, &wrong, &selection).is_err());
        let result = dispatch_group(&brief, &request, &selection).unwrap();
        if let HandoffView::Task(focus) = result.view {
            assert_eq!(focus.task.project_id.as_deref(), Some("app"));
        } else {
            panic!("group menu must remain scoped");
        }
    }
    #[test]
    fn malformed_duplicate_visible_identity_fails_closed() {
        let mut brief = brief();
        brief.items.push(item("test", Some("infra"), &[]));
        let request = TaskActionRequest {
            task_id: "test".into(),
            authority_available: true,
        };
        assert!(planning_actions::task_actions(&brief, &request).is_err());
        let selection = ActionSelection {
            reviewed_revision: brief.revision,
            action_index: 0,
            kind: ActionKind::InspectTask,
            task_id: Some("test".into()),
            prerequisite_id: None,
        };
        assert!(dispatch_task(&brief, &request, &selection).is_err());
    }
}

#[cfg(test)]
mod wire_contract_tests {
    use super::*;
    use crate::planning_brief::{WorkItem, WorkItemState};
    #[test]
    fn typed_preview_roundtrips_without_gaining_execution_authority() {
        for operation in [
            PreviewOperation::CompletionImpact,
            PreviewOperation::CalendarSlots,
            PreviewOperation::CalendarReview,
            PreviewOperation::DependencyAddition,
            PreviewOperation::DependencyRemoval,
        ] {
            let report = PlanningHandoff {
                revision: 101,
                destination: ActionDestination::DependencyPreview,
                view: HandoffView::Preview(PreviewInstruction {
                    revision: 101,
                    operation,
                    task_id: "t".into(),
                    prerequisite_id: None,
                    evaluated_at_ms: 12,
                    window_end_ms: 36_000,
                    explanation: "review only".into(),
                    requires_explicit_approval: true,
                }),
                authoritative_write_performed: false,
                notes: vec!["do not mutate".into()],
            };
            let encoded = serde_json::to_vec(&report).unwrap();
            let decoded: PlanningHandoff = serde_json::from_slice(&encoded).unwrap();
            assert_eq!(decoded, report);
            assert!(!decoded.authoritative_write_performed);
            assert_eq!(decoded.revision, 101);
        }
    }
    #[test]
    fn invalid_index_never_defaults_to_the_first_available_action() {
        let brief = WorkBrief {
            revision: 1,
            evaluated_at_ms: 1,
            ends_at_ms: 3_600_001,
            available_minutes: 60,
            proposed_minutes: 0,
            scheduled_count: 0,
            ready_count: 1,
            blocked_count: 0,
            deferred_count: 0,
            completed_count: 0,
            items: vec![WorkItem {
                id: "task".into(),
                title: "Review".into(),
                project_id: None,
                milestone_id: None,
                state: WorkItemState::Ready,
                recommended_rank: None,
                proposed_start_ms: None,
                proposed_end_ms: None,
                deadline_at_ms: None,
                blocked_by: Vec::new(),
                unblocks: Vec::new(),
                explanation: Vec::new(),
                portfolio_rank: None,
                same_project_downstream: 0,
                other_project_downstream: 0,
                immediately_unblocked: 0,
                portfolio_reasons: Vec::new(),
            }],
            projects: Vec::new(),
            milestones: Vec::new(),
            warnings: Vec::new(),
            portfolio_actionable_total: 1,
            cross_project_leverage_count: 0,
        };
        let request = TaskActionRequest {
            task_id: "task".into(),
            authority_available: false,
        };
        let menu = planning_actions::task_actions(&brief, &request).unwrap();
        assert!(menu.actions.len() > 1);
        assert!(select_from_menu(&menu, menu.actions.len()).is_err());
        assert!(select_from_menu(&menu, usize::MAX).is_err());
        let forged = ActionSelection {
            reviewed_revision: 1,
            action_index: menu.actions.len(),
            kind: planning_actions::ActionKind::InspectTask,
            task_id: Some("task".into()),
            prerequisite_id: None,
        };
        assert!(dispatch_task(&brief, &request, &forged).is_err());
    }
}
