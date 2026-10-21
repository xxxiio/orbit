//! Shared action catalogue used by contextual menus and the command palette.
//!
//! A blueprint can mention an action in presentation metadata, but this module
//! derives availability from the selected item and the calling context. No
//! declaration grants authority; every mutation still travels through Orbit's
//! authorized application/client interfaces.
use crate::model::{AvailableAction, DetailModel, ItemKind, SelectionItem};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActionScope {
    Global,
    Navigation,
    Project,
    Item,
    View,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ActionDescriptor {
    pub id: &'static str,
    pub label: &'static str,
    pub description: &'static str,
    pub keys: &'static [&'static str],
    pub scope: ActionScope,
    pub destructive: bool,
}

pub const ACTIONS: &[ActionDescriptor] = &[
    ActionDescriptor {
        id: "open",
        label: "Open",
        description: "Open the selected item in the detail view",
        keys: &["enter"],
        scope: ActionScope::Item,
        destructive: false,
    },
    ActionDescriptor {
        id: "create_task",
        label: "New task",
        description: "Capture a task in the current project or inbox",
        keys: &["c"],
        scope: ActionScope::Project,
        destructive: false,
    },
    ActionDescriptor {
        id: "create_work_item",
        label: "New work item",
        description: "Create a typed blueprint work item",
        keys: &["C"],
        scope: ActionScope::Project,
        destructive: false,
    },
    ActionDescriptor {
        id: "edit_task",
        label: "Edit task",
        description: "Rename a task and set planning fields",
        keys: &["e"],
        scope: ActionScope::Item,
        destructive: false,
    },
    ActionDescriptor {
        id: "add_dependency",
        label: "Add dependency",
        description: "Add a prerequisite task",
        keys: &[],
        scope: ActionScope::Item,
        destructive: false,
    },
    ActionDescriptor {
        id: "remove_dependency",
        label: "Remove dependency",
        description: "Remove a prerequisite edge",
        keys: &[],
        scope: ActionScope::Item,
        destructive: false,
    },
    ActionDescriptor {
        id: "edit_project",
        label: "Project settings",
        description: "Change supported project status, mode and objective",
        keys: &[],
        scope: ActionScope::Item,
        destructive: false,
    },
    ActionDescriptor {
        id: "edit_work_item",
        label: "Edit work item",
        description: "Edit blueprint-declared work-item properties",
        keys: &[],
        scope: ActionScope::Item,
        destructive: false,
    },
    ActionDescriptor {
        id: "install_blueprint",
        label: "Install blueprint",
        description: "Install a bundled blueprint version",
        keys: &[],
        scope: ActionScope::Project,
        destructive: false,
    },
    ActionDescriptor {
        id: "bind_blueprint",
        label: "Bind blueprint",
        description: "Explicitly pin a published blueprint version",
        keys: &[],
        scope: ActionScope::Project,
        destructive: false,
    },
    ActionDescriptor {
        id: "set_task_status",
        label: "Change task status",
        description: "Offer an authorized task workflow transition",
        keys: &["s"],
        scope: ActionScope::Item,
        destructive: false,
    },
    ActionDescriptor {
        id: "complete_task",
        label: "Complete task",
        description: "Move a task to done through the authoritative workflow",
        keys: &["x"],
        scope: ActionScope::Item,
        destructive: false,
    },
    ActionDescriptor {
        id: "blueprint_transition",
        label: "Workflow transition",
        description: "Apply a transition declared by the pinned blueprint",
        keys: &["s"],
        scope: ActionScope::Item,
        destructive: false,
    },
    ActionDescriptor {
        id: "record_habit",
        label: "Record occurrence",
        description: "Record an authorized habit occurrence",
        keys: &["space"],
        scope: ActionScope::Item,
        destructive: false,
    },
    ActionDescriptor {
        id: "open_knowledge",
        label: "Open knowledge",
        description: "Inspect the selected note in the detail panel",
        keys: &["enter"],
        scope: ActionScope::Item,
        destructive: false,
    },
    ActionDescriptor {
        id: "inspect_dependencies",
        label: "Inspect dependencies",
        description: "Show task or work-item prerequisites",
        keys: &["d"],
        scope: ActionScope::Item,
        destructive: false,
    },
    ActionDescriptor {
        id: "review_agent",
        label: "Review agent job",
        description: "Inspect an agent handoff and review state",
        keys: &[],
        scope: ActionScope::Item,
        destructive: false,
    },
    ActionDescriptor {
        id: "preview_plan",
        label: "Preview plan",
        description: "Generate a deterministic schedule preview",
        keys: &["p"],
        scope: ActionScope::Project,
        destructive: false,
    },
    ActionDescriptor {
        id: "apply_plan",
        label: "Apply plan",
        description: "Apply a previously previewed schedule after revalidation",
        keys: &["P"],
        scope: ActionScope::Project,
        destructive: true,
    },
    ActionDescriptor {
        id: "record_metric",
        label: "Record metric",
        description: "Append an authorized metric observation",
        keys: &["o"],
        scope: ActionScope::Item,
        destructive: false,
    },
    ActionDescriptor {
        id: "set_metric_target",
        label: "Set metric target",
        description: "Configure a project metric target",
        keys: &["t"],
        scope: ActionScope::Project,
        destructive: false,
    },
    ActionDescriptor {
        id: "bind_evidence",
        label: "Bind evidence",
        description: "Bind a habit or metric as project evidence",
        keys: &["b"],
        scope: ActionScope::Project,
        destructive: false,
    },
    ActionDescriptor {
        id: "switch_project",
        label: "Switch project",
        description: "Open the project picker",
        keys: &["g", "p"],
        scope: ActionScope::Navigation,
        destructive: false,
    },
    ActionDescriptor {
        id: "go_home",
        label: "Home",
        description: "Open the Today aggregation",
        keys: &["g", "h"],
        scope: ActionScope::Navigation,
        destructive: false,
    },
    ActionDescriptor {
        id: "go_inbox",
        label: "Inbox",
        description: "Open the global inbox",
        keys: &["g", "i"],
        scope: ActionScope::Navigation,
        destructive: false,
    },
    ActionDescriptor {
        id: "go_agents",
        label: "Agents",
        description: "Open agent job monitoring",
        keys: &["g", "a"],
        scope: ActionScope::Navigation,
        destructive: false,
    },
    ActionDescriptor {
        id: "command_palette",
        label: "Command palette",
        description: "Search every available navigation and action",
        keys: &[":", "ctrl+p"],
        scope: ActionScope::Global,
        destructive: false,
    },
    ActionDescriptor {
        id: "search",
        label: "Search",
        description: "Search tasks, work items and knowledge",
        keys: &["/"],
        scope: ActionScope::Global,
        destructive: false,
    },
    ActionDescriptor {
        id: "refresh",
        label: "Refresh",
        description: "Reload accepted data for the current screen",
        keys: &["r"],
        scope: ActionScope::Global,
        destructive: false,
    },
    ActionDescriptor {
        id: "toggle_detail",
        label: "Toggle details",
        description: "Show or hide the contextual detail panel",
        keys: &["D"],
        scope: ActionScope::Global,
        destructive: false,
    },
    ActionDescriptor {
        id: "help",
        label: "Keyboard help",
        description: "Show navigation, focus and action keys",
        keys: &["?"],
        scope: ActionScope::Global,
        destructive: false,
    },
    ActionDescriptor {
        id: "quit",
        label: "Quit",
        description: "Leave Orbit",
        keys: &["q"],
        scope: ActionScope::Global,
        destructive: false,
    },
];

pub fn descriptor(id: &str) -> Option<&'static ActionDescriptor> {
    ACTIONS.iter().find(|action| action.id == id)
}

/// Contextual offers for the currently selected item. The returned list is
/// data, not permission: each command is validated again by the client.
pub fn contextual(item: Option<&SelectionItem>, project_scope: bool) -> Vec<AvailableAction> {
    let mut actions = Vec::new();
    if let Some(item) = item {
        actions.push(AvailableAction::enabled(
            "open",
            "Open details",
            Some(item.id.clone()),
        ));
        match item.kind {
            ItemKind::Task => {
                actions.push(AvailableAction::enabled(
                    "edit_task",
                    "Edit task",
                    Some(item.id.clone()),
                ));
                actions.push(AvailableAction::enabled(
                    "add_dependency",
                    "Add dependency",
                    Some(item.id.clone()),
                ));
                if item.fields.iter().any(|(key, value)| {
                    (key == "prerequisites" || key == "blocked by") && !value.is_empty()
                }) {
                    actions.push(AvailableAction::enabled(
                        "remove_dependency",
                        "Remove dependency",
                        Some(item.id.clone()),
                    ));
                }
                for status in &item.allowed_statuses {
                    let mut action = AvailableAction::enabled(
                        "set_task_status",
                        format!("Move to {status}"),
                        Some(status.clone()),
                    );
                    action.description = format!("{} → {status}", item.status);
                    action.destructive = status == "cancelled";
                    actions.push(action);
                }
                actions.push(AvailableAction::enabled(
                    "inspect_dependencies",
                    "Inspect dependencies",
                    Some(item.id.clone()),
                ));
            }
            ItemKind::WorkItem => {
                actions.push(AvailableAction::enabled(
                    "edit_work_item",
                    "Edit work item",
                    Some(item.id.clone()),
                ));
                for state in &item.allowed_transitions {
                    let mut action = AvailableAction::enabled(
                        "blueprint_transition",
                        format!("Move to {state}"),
                        Some(state.clone()),
                    );
                    action.description = format!("{} → {state}", item.status);
                    actions.push(action);
                }
                actions.push(AvailableAction::enabled(
                    "inspect_dependencies",
                    "Inspect dependencies",
                    Some(item.id.clone()),
                ));
            }
            ItemKind::Habit => actions.push(AvailableAction::enabled(
                "record_habit",
                "Record occurrence",
                Some(item.id.clone()),
            )),
            ItemKind::Knowledge => actions.push(AvailableAction::enabled(
                "open_knowledge",
                "Open note",
                Some(item.id.clone()),
            )),
            ItemKind::Metric => {
                actions.push(AvailableAction::enabled(
                    "record_metric",
                    "Record observation",
                    Some(item.id.clone()),
                ));
                actions.push(AvailableAction::enabled(
                    "set_metric_target",
                    "Set target",
                    Some(item.id.clone()),
                ));
            }
            ItemKind::AgentJob => actions.push(AvailableAction::disabled(
                "review_agent",
                "Review agent job",
                "Handoff review is available from the connected authority client",
            )),
            ItemKind::Project if project_scope => {
                actions.push(AvailableAction::enabled(
                    "create_task",
                    "New task",
                    Some(item.id.clone()),
                ));
                actions.push(AvailableAction::enabled(
                    "edit_project",
                    "Project settings",
                    Some(item.id.clone()),
                ));
                actions.push(AvailableAction::enabled(
                    "bind_blueprint",
                    "Bind blueprint",
                    Some(item.id.clone()),
                ));
            }
            ItemKind::Project => {}
            ItemKind::Milestone | ItemKind::Schedule | ItemKind::Text => {}
        }
    }
    if project_scope {
        actions.push(AvailableAction::enabled("create_task", "New task", None));
        actions.push(AvailableAction::enabled(
            "create_work_item",
            "New work item",
            None,
        ));
        actions.push(AvailableAction::enabled(
            "preview_plan",
            "Preview plan",
            None,
        ));
        actions.push(AvailableAction::enabled(
            "bind_evidence",
            "Bind evidence",
            None,
        ));
    }
    actions
}

pub fn detail_for(item: &SelectionItem, project_scope: bool) -> DetailModel {
    DetailModel {
        title: item.title.clone(),
        subtitle: item.subtitle.clone(),
        status: item.status.clone(),
        kind: item.kind,
        fields: item.fields.clone(),
        note: String::new(),
        actions: contextual(Some(item), project_scope),
    }
}

pub fn hotkey_hint(id: &str) -> String {
    descriptor(id)
        .map(|action| {
            if action.keys.is_empty() {
                String::new()
            } else {
                action.keys.join(" / ")
            }
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn task() -> SelectionItem {
        SelectionItem {
            id: "task-1".into(),
            kind: ItemKind::Task,
            title: "Task".into(),
            subtitle: "ready".into(),
            status: "ready".into(),
            fields: Vec::new(),
            allowed_statuses: vec!["in_progress".into()],
            allowed_transitions: Vec::new(),
            revision: None,
        }
    }

    #[test]
    fn blueprint_declarations_never_add_actions() {
        let offers = contextual(Some(&task()), false);
        assert!(offers.iter().any(|action| action.id == "set_task_status"));
        assert!(!offers.iter().any(|action| action.id == "create_work_item"));
        assert!(!offers.iter().any(|action| action.id == "apply_plan"));
    }

    #[test]
    fn every_catalogue_action_is_unique_and_has_a_scope() {
        let mut ids = std::collections::BTreeSet::new();
        for action in ACTIONS {
            assert!(ids.insert(action.id), "duplicate action {}", action.id);
            let _ = action.scope;
        }
    }

    #[test]
    fn disabled_actions_carry_a_reason() {
        let job = SelectionItem {
            id: "job".into(),
            kind: ItemKind::AgentJob,
            title: "Job".into(),
            subtitle: String::new(),
            status: "running".into(),
            fields: Vec::new(),
            allowed_statuses: Vec::new(),
            allowed_transitions: Vec::new(),
            revision: None,
        };
        let review = contextual(Some(&job), false)
            .into_iter()
            .find(|action| action.id == "review_agent")
            .unwrap();
        assert!(!review.enabled);
        assert!(review.disabled_reason.is_some());
    }
}
