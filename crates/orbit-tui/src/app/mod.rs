//! Project-centric application state. All mutations go through the local/connected backend;
//! this module owns navigation, focus, prompts and presentation state only.
mod actions;
mod connected;
mod forms;
mod navigation;
mod selection;
mod state;

pub use connected::CONNECTED_PURGE_REFUSED;
pub use state::*;

use navigation::empty_view;

use crate::backend::{Backend, MutationOutcome, PendingCommand};
use crate::connected_actions;
use crate::connected_ops;
use crate::planning_ui;
use crate::theme::Theme;
use orbit_blueprint::{Blueprint, FieldKind};
use orbit_client::{
    ConfigureProjectStrategyRequest, CreateKnowledgeRequest, CreateProjectRequest,
    CreateTaskRequest, MetricTargetDto, ProjectCriterionInputDto, RecordHabitOccurrenceRequest,
    SetProjectStatusRequest, SetTaskPlanningRequest, SetTaskStatusRequest,
};
use orbit_runtime::planning_actions::PlanningActionMenu;
use orbit_runtime::planning_brief::WorkBrief;
use orbit_ui::actions as ui_actions;
use orbit_ui::model::*;
use orbit_ui::{ComponentBody, ComponentRegistry, LoadStatus, ResolvedComponent, ResolvedView};
use std::collections::BTreeMap;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Turn a per-command outcome into user-facing status text.
///
/// A missing/unknown state is deliberately reported as pending: the TUI must
/// never promote an unobserved command to accepted. Conflicts name their state
/// and reason so the user can act on the outbox instead of seeing success.
fn describe_mutation_outcome(
    outcome: Option<MutationOutcome>,
    text: String,
) -> (String, StatusKind) {
    match outcome {
        None | Some(MutationOutcome::Accepted) => (text, StatusKind::Success),
        Some(MutationOutcome::Pending { state, .. }) => {
            let state = if state == "unavailable" {
                "state unavailable; treated as pending".to_owned()
            } else {
                format!("queued state: {state}")
            };
            (
                format!("Pending (not accepted; {state}): {text}"),
                StatusKind::Pending,
            )
        }
        Some(MutationOutcome::Conflict { state, reason, .. }) => (
            format!("Queued change rejected ({state}): {reason}; inspect the outbox. {text}"),
            StatusKind::Warning,
        ),
    }
}

pub struct App {
    pub client: Backend,
    pub registry: ComponentRegistry,
    pub theme: Theme,
    pub screen: Screen,
    pub focus: FocusPane,
    pub show_detail: bool,
    pub workspace: Option<orbit_ui::WorkspaceData>,
    pub workspace_error: Option<String>,
    pub home: ResolvedView,
    pub inbox: ResolvedView,
    pub agents: ResolvedView,
    pub projects: Vec<orbit_ui::data::ProjectRow>,
    pub project_details: Vec<orbit_client::ProjectDto>,
    pub projects_selection: usize,
    pub project: Option<ProjectState>,
    pub active_component: usize,
    pub selected: usize,
    pub scroll: usize,
    pub detail_scroll: usize,
    /// Last painted maximum detail scroll. Kept in the app so upward scrolling
    /// from an over-scrolled value moves immediately instead of looking dead.
    pub detail_max_scroll: std::cell::Cell<usize>,
    pub modal_scroll: usize,
    pub sidebar_selection: usize,
    pub modal: Option<Modal>,
    pub status: StatusMessage,
    pub prefix: Option<char>,
    pub pending_plan: Option<orbit_client::TaskPlanProposalDto>,
    pub planning: Option<planning_ui::PlanningDocument>,
    pub planning_brief: Option<WorkBrief>,
    pub planning_error: Option<String>,
    pub planning_hours: i64,
    pub planning_project: Option<String>,
    pub today_action_menu: Option<(WorkBrief, PlanningActionMenu)>,
    /// Snapshot-bound `plan.review` result awaiting the exact
    /// `plan.approve --confirm`. A changed accepted revision refuses approval.
    pub pending_scenario: Option<orbit_runtime::scenario_execution::ScenarioExecutionReview>,
    pub viewport_width: u16,
    /// A saved session could not be restored because the authority view never
    /// loaded. Navigation intent from the previous file is preserved until the
    /// user opens a project or navigates deliberately.
    pub restore_skipped: bool,
    pub(crate) history: Vec<NavigationState>,
    pub(crate) cursors: BTreeMap<String, CursorState>,
    pub(crate) last_submission: Option<(String, Instant)>,
    /// Most-recently opened destinations (screens, projects, views, notes).
    /// Deliberately in memory only: the palette is a session aid, not state.
    pub(crate) recent_destinations: Vec<String>,
}

impl App {
    #[allow(dead_code)]
    pub fn open(path: impl Into<String>) -> Result<Self, String> {
        let client = Backend::open_local(path.into()).map_err(|error| format!("{error:?}"))?;
        Self::open_with_backend(client)
    }

    pub fn open_deployment(
        deployment: orbit_client::deployment::Deployment,
    ) -> Result<Self, String> {
        let backend = match deployment {
            orbit_client::deployment::Deployment::Local { database } => {
                Backend::open_local(&database)
            }
            orbit_client::deployment::Deployment::Connected { replica, server } => {
                Backend::open_connected(&replica, &server.to_string())
            }
        }
        .map_err(|error| format!("{error:?}"))?;
        Self::open_with_backend(backend)
    }

    pub(crate) fn open_with_backend(client: Backend) -> Result<Self, String> {
        let mut app = Self {
            client,
            registry: ComponentRegistry::builtin(),
            theme: Theme::detect(),
            screen: Screen::Home,
            focus: FocusPane::Workspace,
            show_detail: true,
            workspace: None,
            workspace_error: None,
            home: empty_view("home", "Home"),
            inbox: empty_view("inbox", "Inbox"),
            agents: empty_view("agents", "Agents"),
            projects: Vec::new(),
            project_details: Vec::new(),
            projects_selection: 0,
            project: None,
            active_component: 0,
            selected: 0,
            scroll: 0,
            detail_scroll: 0,
            detail_max_scroll: std::cell::Cell::new(0),
            modal_scroll: 0,
            sidebar_selection: 0,
            modal: None,
            status: StatusMessage {
                text: "Ready • ? help • : palette • g h/i/p/a navigate".into(),
                kind: StatusKind::Info,
            },
            prefix: None,
            pending_plan: None,
            planning: None,
            planning_brief: None,
            planning_error: None,
            planning_hours: 12,
            planning_project: None,
            today_action_menu: None,
            pending_scenario: None,
            viewport_width: 120,
            restore_skipped: false,
            history: Vec::new(),
            cursors: BTreeMap::new(),
            last_submission: None,
            recent_destinations: Vec::new(),
        };
        app.refresh_workspace();
        Ok(app)
    }

    /// Single quit path for every screen and modal. Text prompts with content
    /// require confirmation; Ctrl+Q/Ctrl+C never insert text into a focused field.
    pub fn request_quit(&mut self) -> bool {
        if matches!(self.modal, Some(Modal::QuitConfirm { .. })) {
            return true;
        }
        if self.has_unsaved_input() {
            let resume = self.modal.take().map(Box::new);
            self.modal = Some(Modal::QuitConfirm { resume });
            self.notify(
                "Unsaved input: confirm quit to discard, or Esc to keep editing",
                StatusKind::Warning,
            );
            return false;
        }
        true
    }

    fn has_unsaved_input(&self) -> bool {
        matches!(
            &self.modal,
            Some(Modal::Form(form)) if form.dirty
        ) || matches!(
            &self.modal,
            Some(Modal::Palette { query, .. }) | Some(Modal::Search { query, .. })
                if !query.trim().is_empty()
        ) || matches!(
            &self.modal,
            Some(Modal::Console { input, .. }) if !input.trim().is_empty()
        ) || matches!(
            &self.modal,
            Some(Modal::Conflict { replacement, .. }) if !replacement.trim().is_empty()
        )
    }

    /// Report the exact per-command disposition instead of inferring success
    /// from a quiet client. Key handlers in `main` use this too.
    pub(crate) fn mutation_status(&mut self, text: impl Into<String>) -> (String, StatusKind) {
        describe_mutation_outcome(self.client.take_outcome(), text.into())
    }

    pub fn notify(&mut self, text: impl Into<String>, kind: StatusKind) {
        self.status = StatusMessage {
            text: text.into(),
            kind,
        };
    }
}

pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests;
