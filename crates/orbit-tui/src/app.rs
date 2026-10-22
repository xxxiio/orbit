//! Project-centric application state. All mutations go through the local/connected backend;
//! this module owns navigation, focus, prompts and presentation state only.
use crate::backend::Backend;
use crate::planning_ui;
use crate::theme::Theme;
use orbit_blueprint::{Blueprint, FieldKind};
use orbit_client::{
    AcceptanceCriterionDto, ActivityDto, AttachmentDto, ConfigureProjectStrategyRequest,
    CreateKnowledgeRequest, CreateProjectRequest, CreateTaskRequest, MetricTargetDto,
    ProjectCriterionInputDto, RecordHabitOccurrenceRequest, SetProjectStatusRequest,
    SetTaskPlanningRequest, SetTaskStatusRequest,
};
use orbit_runtime::planning_actions::PlanningActionMenu;
use orbit_runtime::planning_brief::WorkBrief;
use orbit_ui::actions;
use orbit_ui::model::*;
use orbit_ui::{ComponentBody, ComponentRegistry, LoadStatus, ResolvedComponent, ResolvedView};
use std::collections::BTreeMap;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Screen {
    Home,
    Inbox,
    Projects,
    Agents,
    Project,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FocusPane {
    Sidebar,
    Workspace,
    Detail,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StatusKind {
    Info,
    Success,
    Warning,
    Error,
    Pending,
}

#[derive(Clone, Debug)]
pub struct StatusMessage {
    pub text: String,
    pub kind: StatusKind,
}

#[derive(Clone, Debug)]
pub struct ProjectState {
    pub id: String,
    pub name: String,
    pub data: orbit_ui::ProjectData,
    pub views: Vec<ResolvedView>,
    pub active_view: usize,
    pub generated: bool,
    pub blueprint_label: String,
    pub stale_error: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FormChoice {
    pub value: String,
    pub label: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FormFieldKind {
    Text,
    /// Markdown-compatible multi-line text. Enter inserts a newline; the form is
    /// saved with Ctrl+S.
    Multiline,
    Integer,
    Boolean,
    Choice(Vec<FormChoice>),
}

#[derive(Clone, Debug)]
pub struct FormField {
    pub key: String,
    pub label: String,
    pub value: String,
    pub initial: String,
    /// Caret position as a character index. Only used by text-like fields.
    pub cursor: usize,
    pub kind: FormFieldKind,
    pub required: bool,
    pub hint: String,
}

#[derive(Clone, Debug)]
pub enum FormMode {
    NewProject,
    NewTask {
        project_id: Option<String>,
    },
    EditTask {
        task_id: String,
    },
    EditTaskDetails {
        task_id: String,
    },
    AddCriterion {
        task_id: String,
    },
    EditCriterion {
        task_id: String,
        criterion_id: String,
    },
    AttachFile {
        owner_kind: String,
        owner_id: String,
    },
    ExportAttachment {
        attachment_id: String,
    },
    ChangeWorkflow {
        project_id: String,
    },
    AddDependency {
        task_id: String,
    },
    RemoveDependency {
        task_id: String,
    },
    EditProject {
        project_id: String,
    },
    NewWorkItemKind {
        project_id: String,
    },
    NewWorkItem {
        project_id: String,
        blueprint: Blueprint,
        kind: String,
    },
    EditWorkItem {
        item_id: String,
        revision: u64,
        blueprint: Blueprint,
        kind: String,
    },
    NewKnowledge,
    InstallBlueprint,
    BindBlueprint {
        project_id: String,
    },
    MetricTarget {
        metric_id: Option<String>,
    },
    MetricObservation {
        metric_id: Option<String>,
    },
    BindEvidence {
        project_id: String,
        unbind: bool,
    },
}

#[derive(Clone, Debug)]
pub struct FormState {
    pub title: String,
    pub hint: String,
    pub mode: FormMode,
    pub fields: Vec<FormField>,
    pub focused: usize,
    pub error: Option<String>,
    pub dirty: bool,
}

#[derive(Clone, Debug)]
pub enum PendingAction {
    SetTaskStatus {
        task_id: String,
        status: String,
    },
    BlueprintTransition {
        item_id: String,
        to: String,
        revision: u64,
    },
    RecordHabit {
        habit_id: String,
    },
    ApplyPlan,
    RemoveCriterion {
        task_id: String,
        criterion_id: String,
        task_title: String,
    },
    RemoveAttachment {
        attachment_id: String,
        owner_kind: String,
        owner_id: String,
        title: String,
    },
    OpenAttachment {
        attachment_id: String,
        filename: String,
    },
    /// Restore a dirty form after the user declines to discard it.
    DiscardFormChanges {
        form: Box<FormState>,
    },
}

/// Ordered acceptance criteria for one task, with a selectable row.
#[derive(Clone, Debug)]
pub struct CriteriaModal {
    pub task_id: String,
    pub task_title: String,
    pub items: Vec<AcceptanceCriterionDto>,
    pub selected: usize,
}

/// Attachments for one owner entity, with a selectable row.
#[derive(Clone, Debug)]
pub struct AttachmentsModal {
    pub owner_kind: String,
    pub owner_id: String,
    pub title: String,
    pub items: Vec<AttachmentDto>,
    pub selected: usize,
}

/// Read-only activity history for one entity.
#[derive(Clone, Debug)]
pub struct ActivityModal {
    pub title: String,
    pub items: Vec<ActivityDto>,
    pub scroll: u16,
}

#[derive(Clone, Debug)]
pub enum Modal {
    Form(FormState),
    Confirm {
        title: String,
        body: String,
        pending: PendingAction,
    },
    ActionMenu {
        title: String,
        actions: Vec<AvailableAction>,
        selected: usize,
    },
    Palette {
        query: String,
        entries: Vec<PaletteEntry>,
        selected: usize,
    },
    Help,
    Detail(DetailModel),
    Criteria(CriteriaModal),
    Attachments(AttachmentsModal),
    Activity(ActivityModal),
    QuitConfirm {
        resume: Option<Box<Modal>>,
    },
    TodayActions {
        menu: PlanningActionMenu,
        selected: usize,
    },
    Search {
        query: String,
        hits: Vec<SearchHit>,
        selected: usize,
    },
    Message {
        title: String,
        lines: Vec<String>,
    },
}

#[derive(Clone, Debug)]
pub struct PaletteEntry {
    pub label: String,
    pub description: String,
    pub action: PaletteAction,
    pub enabled: bool,
}

#[derive(Clone, Debug)]
pub enum PaletteAction {
    Go(Screen),
    OpenProject(String),
    OpenView(usize),
    Execute(AvailableAction),
    Form(FormMode),
    SelectItem { component: usize, index: usize },
    OpenKnowledge(String),
    Quit,
}

#[derive(Clone, Debug)]
pub struct SearchHit {
    pub label: String,
    pub description: String,
    pub action: PaletteAction,
}

#[derive(Clone, Debug)]
pub struct NavigationState {
    pub screen: Screen,
    pub project_id: Option<String>,
    pub active_view: usize,
    pub active_view_id: Option<String>,
    pub focus: FocusPane,
    pub sidebar_selection: usize,
    pub projects_selection: usize,
}

#[derive(Clone, Debug, Default)]
struct CursorState {
    active_component: usize,
    selected: Vec<usize>,
    selected_id: Vec<Option<String>>,
    scroll: Vec<usize>,
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
    pub viewport_width: u16,
    history: Vec<NavigationState>,
    cursors: BTreeMap<String, CursorState>,
    last_submission: Option<(String, Instant)>,
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
            viewport_width: 120,
            history: Vec::new(),
            cursors: BTreeMap::new(),
            last_submission: None,
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
        )
    }

    fn cursor_key(&self) -> Option<String> {
        match self.screen {
            Screen::Home => Some("home".into()),
            Screen::Inbox => Some("inbox".into()),
            Screen::Agents => Some("agents".into()),
            Screen::Projects => None,
            Screen::Project => {
                let project = self.project.as_ref()?;
                let view = project.views.get(project.active_view)?;
                Some(format!("project:{}:{}", project.id, view.id))
            }
        }
    }

    fn sync_cursor(&mut self) {
        let Some(key) = self.cursor_key() else {
            return;
        };
        let count = self.component_count();
        let selected_id = self.selected_item().map(|item| item.id.clone());
        let state = self.cursors.entry(key).or_default();
        state.active_component = self.active_component;
        state.selected.resize(count, 0);
        state.selected_id.resize(count, None);
        state.scroll.resize(count, 0);
        if let Some(index) = state.selected.get_mut(self.active_component) {
            *index = self.selected;
        }
        if let Some(index) = state.selected_id.get_mut(self.active_component) {
            *index = selected_id.clone();
        }
        if let Some(index) = state.scroll.get_mut(self.active_component) {
            *index = self.scroll;
        }
    }

    fn restore_cursor(&mut self) {
        let Some(key) = self.cursor_key() else {
            return;
        };
        let count = self.component_count();
        let state = self.cursors.get(&key).cloned().unwrap_or_default();
        self.active_component = state.active_component.min(count.saturating_sub(1));
        let selected = state
            .selected
            .get(self.active_component)
            .copied()
            .unwrap_or(0);
        let selected_id = state
            .selected_id
            .get(self.active_component)
            .cloned()
            .flatten();
        let scroll = state
            .scroll
            .get(self.active_component)
            .copied()
            .unwrap_or(0);
        let len = self
            .current_component()
            .map(|component| component.selection.len())
            .unwrap_or(0);
        let selected = selected_id
            .and_then(|id| {
                self.current_component()
                    .and_then(|component| component.selection.iter().position(|item| item.id == id))
            })
            .unwrap_or(selected);
        self.selected = selected.min(len.saturating_sub(1));
        self.scroll = if len == 0 {
            scroll
        } else {
            scroll.min(self.selected)
        };
    }

    fn push_history(&mut self) {
        self.sync_cursor();
        let Some(project_id) = self.project.as_ref().map(|project| project.id.clone()) else {
            if self.screen != Screen::Project {
                self.history.push(self.navigation_state(None));
                self.trim_history();
            }
            return;
        };
        if self.screen == Screen::Project {
            self.history.push(self.navigation_state(Some(project_id)));
        } else {
            self.history.push(self.navigation_state(None));
        }
        self.trim_history();
    }

    fn trim_history(&mut self) {
        if self.history.len() > 30 {
            self.history.remove(0);
        }
    }

    fn navigation_state(&self, project_id: Option<String>) -> NavigationState {
        NavigationState {
            screen: self.screen,
            project_id,
            active_view: self
                .project
                .as_ref()
                .map(|project| project.active_view)
                .unwrap_or(0),
            active_view_id: self.project.as_ref().and_then(|project| {
                project
                    .views
                    .get(project.active_view)
                    .map(|view| view.id.clone())
            }),
            focus: self.focus,
            sidebar_selection: self.sidebar_selection,
            projects_selection: self.projects_selection,
        }
    }

    pub fn go_back(&mut self) {
        self.sync_cursor();
        while let Some(previous) = self.history.pop() {
            if previous.screen == Screen::Project {
                let Some(project_id) = previous.project_id.as_deref() else {
                    continue;
                };
                if !self.projects.iter().any(|project| project.id == project_id) {
                    continue;
                }
                if self.project.as_ref().map(|project| project.id.as_str()) != Some(project_id) {
                    self.open_project_internal(project_id, false);
                }
                if let Some(project) = &mut self.project {
                    project.active_view = previous
                        .active_view_id
                        .as_deref()
                        .and_then(|id| project.views.iter().position(|view| view.id == id))
                        .unwrap_or(previous.active_view)
                        .min(project.views.len().saturating_sub(1));
                }
            }
            self.screen = previous.screen;
            self.focus = previous.focus;
            self.sidebar_selection = previous.sidebar_selection;
            self.projects_selection = previous.projects_selection;
            self.prefix = None;
            self.restore_cursor();
            self.notify("Back", StatusKind::Info);
            return;
        }
        if self.screen != Screen::Home {
            self.go_screen_internal(Screen::Home, false);
        } else {
            self.notify("Already Home • Ctrl+Q quits", StatusKind::Info);
        }
    }

    fn mutation_status(&mut self, text: impl Into<String>) -> (String, StatusKind) {
        if self.client.take_pending() {
            (
                format!("Pending (not accepted): {}", text.into()),
                StatusKind::Pending,
            )
        } else {
            (text.into(), StatusKind::Success)
        }
    }

    pub fn notify(&mut self, text: impl Into<String>, kind: StatusKind) {
        self.status = StatusMessage {
            text: text.into(),
            kind,
        };
    }

    pub fn refresh_workspace(&mut self) {
        let connected_refresh_error = self.client.refresh_accepted().err();
        match self.client.workspace_view_data() {
            Ok(workspace) => {
                self.projects = workspace.projects.clone();
                self.project_details = self.client.list_projects().unwrap_or_default();
                self.workspace = Some(workspace);
                self.workspace_error = connected_refresh_error
                    .as_ref()
                    .map(|error| format!("connected refresh failed: {}", error.message));
                if let Some(error) = &connected_refresh_error {
                    self.notify(
                        format!(
                            "Connected refresh failed; cached accepted view retained: {}",
                            error.message
                        ),
                        StatusKind::Warning,
                    );
                }
                self.refresh_planning();
                self.rebuild_global_views();
                let max = self.sidebar_len().saturating_sub(1);
                self.sidebar_selection = self.sidebar_selection.min(max);
                self.projects_selection = self
                    .projects_selection
                    .min(self.projects.len().saturating_sub(1));
                self.restore_cursor();
            }
            Err(error) => {
                self.workspace_error = Some(error.to_string());
                self.home
                    .components
                    .retain(|component| component.id != "workspace_error");
                self.home.components.insert(
                    0,
                    ResolvedComponent {
                        id: "workspace_error".into(),
                        title: "Workspace".into(),
                        component_type: "text".into(),
                        status: LoadStatus::Stale(error.to_string()),
                        body: ComponentBody::Text {
                            heading: "Workspace refresh failed".into(),
                            lines: vec![
                                error.to_string(),
                                "Previous accepted data is still visible; press r to retry.".into(),
                            ],
                        },
                        selection: Vec::new(),
                    },
                );
                self.notify(
                    format!("Workspace refresh failed (previous view retained): {error}"),
                    StatusKind::Error,
                );
            }
        }
    }

    fn rebuild_global_views(&mut self) {
        let Some(workspace) = self.workspace.clone() else {
            return;
        };
        self.home = orbit_ui::home_view(&workspace);
        self.insert_planning_component();
        self.inbox = orbit_ui::inbox_view(&workspace);
        self.agents = orbit_ui::agents_view(&workspace);
        if self.client.is_connected() {
            for component in &mut self.agents.components {
                if component.id == "agent_config" || component.component_type == "agent_jobs" {
                    component.body = ComponentBody::Text {
                        heading: "Connected agent monitoring".into(),
                        lines: vec![
                            "Accepted agent jobs are not projected into the connected snapshot yet.".into(),
                            "Inspection and review use the legacy connected command surface (`ORBIT_TUI_LEGACY_CONNECTED=1`).".into(),
                        ],
                    };
                    component.status =
                        LoadStatus::Stale("connected snapshot does not include agent jobs".into());
                }
            }
        }
        if let Some(error) = &self.workspace_error {
            self.home.components.insert(
                0,
                ResolvedComponent {
                    id: "workspace_error".into(),
                    title: "Connection".into(),
                    component_type: "text".into(),
                    status: LoadStatus::Stale(error.clone()),
                    body: ComponentBody::Text {
                        heading: "Accepted view may be stale".into(),
                        lines: vec![
                            error.clone(),
                            "Press r to retry; cached accepted data remains visible.".into(),
                        ],
                    },
                    selection: Vec::new(),
                },
            );
        }
        self.clamp_component();
    }

    fn insert_planning_component(&mut self) {
        let lines = if let Some(planning) = &self.planning {
            planning.lines.clone()
        } else if let Some(error) = &self.planning_error {
            vec![format!("Planning unavailable: {error}")]
        } else {
            vec!["Planning has not been refreshed yet.".into()]
        };
        let status = if self.planning_error.is_some() {
            LoadStatus::Stale(self.planning_error.clone().unwrap_or_default())
        } else if self.planning.is_some() {
            LoadStatus::Ready
        } else {
            LoadStatus::Error("planning unavailable".into())
        };
        self.home.components.insert(
            0,
            ResolvedComponent {
                id: "today_next".into(),
                title: format!("Today / Next • {}h • [/] horizon", self.planning_hours),
                component_type: "text".into(),
                status,
                body: ComponentBody::Text {
                    heading: "Today / Next".into(),
                    lines,
                },
                selection: Vec::new(),
            },
        );
        if let Some(brief) = &self.planning_brief {
            let selection: Vec<SelectionItem> = brief
                .items
                .iter()
                .map(|item| {
                    let status = match item.state {
                        orbit_runtime::planning_brief::WorkItemState::Scheduled => "planned",
                        orbit_runtime::planning_brief::WorkItemState::Ready => "ready",
                        orbit_runtime::planning_brief::WorkItemState::Blocked => "blocked",
                        orbit_runtime::planning_brief::WorkItemState::Deferred => "deferred",
                        orbit_runtime::planning_brief::WorkItemState::Completed => "done",
                    };
                    let mut fields = vec![
                        ("status".into(), status.to_owned()),
                        (
                            "readiness".into(),
                            format!("{:?}", item.state).to_lowercase(),
                        ),
                    ];
                    if let Some(project) = &item.project_id {
                        fields.push(("project".into(), project.clone()));
                    }
                    if !item.blocked_by.is_empty() {
                        fields.push(("blocked by".into(), item.blocked_by.join(", ")));
                    }
                    if let Some(deadline) = item.deadline_at_ms {
                        fields.push(("deadline".into(), format_epoch_ms(deadline)));
                    }
                    SelectionItem {
                        id: item.id.clone(),
                        kind: ItemKind::Task,
                        title: item.title.clone(),
                        subtitle: status.to_owned(),
                        status: status.to_owned(),
                        fields,
                        allowed_statuses: self.client.task_allowed_statuses(status),
                        allowed_transitions: Vec::new(),
                        revision: None,
                    }
                })
                .collect();
            let task_status = if selection.is_empty() {
                LoadStatus::Empty
            } else {
                LoadStatus::Ready
            };
            self.home.components.insert(
                1,
                ResolvedComponent {
                    id: "today_tasks".into(),
                    title: "Today / Next tasks".into(),
                    component_type: "task_list".into(),
                    status: task_status,
                    body: ComponentBody::List {
                        items: brief
                            .items
                            .iter()
                            .zip(selection.iter())
                            .map(|(item, selected)| ListItemModel {
                                id: item.id.clone(),
                                kind: ItemKind::Task,
                                title: item.title.clone(),
                                subtitle: format!(
                                    "{} · {}",
                                    selected.status,
                                    item.explanation.join("; ")
                                ),
                                status: selected.status.clone(),
                                group: selected.status.clone(),
                                meta: Vec::new(),
                            })
                            .collect(),
                    },
                    selection,
                },
            );
        }
    }

    pub fn refresh_planning(&mut self) {
        let request = planning_ui::request(
            now_ms(),
            self.planning_hours,
            100,
            self.planning_project.clone(),
        );
        match request.and_then(|request| {
            self.client
                .work_brief(&request)
                .map_err(|error| error.message)
        }) {
            Ok(brief) => {
                self.planning_brief = Some(brief.clone());
                self.planning = Some(planning_ui::PlanningDocument::from_brief(&brief));
                self.planning_error = None;
            }
            Err(error) => {
                self.planning_error = Some(error);
            }
        }
    }

    pub fn refresh_current(&mut self) {
        match self.screen {
            Screen::Home | Screen::Inbox | Screen::Agents | Screen::Projects => {
                self.refresh_workspace();
            }
            Screen::Project => self.refresh_project(),
        }
    }

    pub fn refresh_after_mutation(&mut self) {
        self.refresh_workspace();
        if self.screen == Screen::Project {
            self.refresh_project();
        }
    }

    pub fn open_project(&mut self, project_id: &str) {
        self.push_history();
        self.open_project_internal(project_id, false);
    }

    /// True when the project is present in the last accepted workspace.
    pub fn project_exists(&self, project_id: &str) -> bool {
        self.projects.iter().any(|project| project.id == project_id)
    }

    /// Reopen a project from durable session state, selecting the saved view by
    /// identifier so view reordering never lands on the wrong view.
    pub fn open_project_restored(&mut self, project_id: &str, view_id: Option<&str>) {
        self.open_project_internal(project_id, false);
        if let (Some(view_id), Some(project)) = (view_id, self.project.as_mut())
            && let Some(index) = project
                .views
                .iter()
                .position(|view| view.id.as_str() == view_id)
        {
            project.active_view = index;
        }
    }

    fn open_project_internal(&mut self, project_id: &str, record_history: bool) {
        if record_history {
            self.push_history();
        }
        let Some(project) = self
            .projects
            .iter()
            .find(|project| project.id == project_id)
            .cloned()
        else {
            self.notify(format!("Unknown project: {project_id}"), StatusKind::Error);
            return;
        };
        self.sync_cursor();
        let index = self
            .projects
            .iter()
            .position(|project| project.id == project_id)
            .unwrap_or(0);
        self.project = Some(ProjectState {
            id: project.id.clone(),
            name: project.name.clone(),
            data: orbit_ui::ProjectData {
                project: project.clone(),
                ..Default::default()
            },
            views: Vec::new(),
            active_view: 0,
            generated: true,
            blueprint_label: "generated default".into(),
            stale_error: None,
        });
        self.screen = Screen::Project;
        self.focus = FocusPane::Workspace;
        self.active_component = 0;
        self.selected = 0;
        self.scroll = 0;
        self.sidebar_selection = 4 + index;
        self.pending_plan = None;
        self.refresh_project();
    }

    pub fn refresh_project(&mut self) {
        let Some(state) = &self.project else {
            return;
        };
        let project_id = state.id.clone();
        let previous_view = state
            .views
            .get(state.active_view)
            .map(|view| view.id.clone());
        match self
            .client
            .resolve_project_views(&project_id, &self.registry, now_ms())
        {
            Ok((data, views, generated)) => {
                let blueprint_label = self
                    .client
                    .project_workflow(&project_id)
                    .ok()
                    .flatten()
                    .map(|workflow| self.workflow_label(&workflow.workflow_id, workflow.version))
                    .unwrap_or_else(|| "generated default views".into());
                let (name, labels) = {
                    let Some(state) = &mut self.project else {
                        return;
                    };
                    state.data = data;
                    state.views = views;
                    state.generated = generated;
                    state.blueprint_label = blueprint_label.clone();
                    state.stale_error = None;
                    state.active_view = previous_view
                        .and_then(|id| state.views.iter().position(|view| view.id == id))
                        .unwrap_or(0)
                        .min(state.views.len().saturating_sub(1));
                    (
                        state.name.clone(),
                        (state.blueprint_label.clone(), state.views.len()),
                    )
                };
                self.restore_cursor();
                self.clamp_component();
                if !matches!(
                    self.status.kind,
                    StatusKind::Success | StatusKind::Warning | StatusKind::Pending
                ) {
                    self.notify(
                        if generated {
                            format!(
                                "{name} • no workflow pinned: generated default views • W change workflow • r refresh"
                            )
                        } else {
                            format!("{name} • {} • {} views • r refresh", labels.0, labels.1)
                        },
                        StatusKind::Info,
                    );
                }
            }
            Err(error) => {
                if let Some(state) = &mut self.project {
                    state.stale_error = Some(error.to_string());
                    self.notify(
                        format!("Project refresh failed (stale view retained): {error}"),
                        StatusKind::Error,
                    );
                }
            }
        }
    }

    pub fn current_view(&self) -> Option<&ResolvedView> {
        match self.screen {
            Screen::Home => Some(&self.home),
            Screen::Inbox => Some(&self.inbox),
            Screen::Agents => Some(&self.agents),
            Screen::Projects => None,
            Screen::Project => self
                .project
                .as_ref()
                .and_then(|state| state.views.get(state.active_view)),
        }
    }

    pub fn current_component(&self) -> Option<&ResolvedComponent> {
        self.current_view()?.components.get(self.active_component)
    }

    pub fn component_count(&self) -> usize {
        self.current_view()
            .map(|view| view.components.len())
            .unwrap_or(0)
    }

    pub fn selected_item(&self) -> Option<&SelectionItem> {
        self.current_component()?.selected(self.selected)
    }

    pub fn detail_model(&self) -> Option<DetailModel> {
        if self.screen == Screen::Projects {
            return self
                .selected_project()
                .map(|project| actions::detail_for(&project_selection(project), false));
        }
        let project_scope = self.screen == Screen::Project;
        let item = self.selected_item()?;
        let mut detail = actions::detail_for(item, project_scope);
        if item.kind == ItemKind::Task
            && let Some(task) = self.task_dto(&item.id)
        {
            {
                if !task.description.trim().is_empty() {
                    detail
                        .fields
                        .push(("Description".into(), task.description.clone()));
                }
                if let Some(priority) = &task.priority {
                    detail.fields.push(("Priority".into(), priority.clone()));
                }
                if !task.labels.is_empty() {
                    detail
                        .fields
                        .push(("Labels".into(), task.labels.join(", ")));
                }
                if let Ok(criteria) = self.client.acceptance_criteria(&item.id) {
                    let done = criteria.iter().filter(|item| item.completed).count();
                    if !criteria.is_empty() {
                        detail.fields.push((
                            "Acceptance".into(),
                            format!("{done}/{} checked", criteria.len()),
                        ));
                    }
                }
                if let Ok(attachments) = self.client.list_attachments("task", &item.id) {
                    detail.fields.push((
                        "Attachments".into(),
                        if attachments.is_empty() {
                            "none".into()
                        } else {
                            format!("{} file(s)", attachments.len())
                        },
                    ));
                }
            }
        }
        Some(detail)
    }

    pub fn available_actions(&self) -> Vec<AvailableAction> {
        let project_scope = self.screen == Screen::Project;
        let mut actions = match self.selected_item() {
            Some(item) => actions::contextual(Some(item), project_scope),
            None => Vec::new(),
        };
        if self.client.is_connected() {
            push_unique_action(
                &mut actions,
                AvailableAction::enabled("flush_pending", "Flush pending commands", None),
            );
        }
        if self.screen == Screen::Projects
            && let Some(project) = self.selected_project()
        {
            push_unique_action(
                &mut actions,
                AvailableAction::enabled(
                    "open",
                    format!("Open {}", project.name),
                    Some(project.id.clone()),
                ),
            );
            push_unique_action(
                &mut actions,
                AvailableAction::enabled(
                    "edit_project",
                    "Project settings",
                    Some(project.id.clone()),
                ),
            );
            push_unique_action(
                &mut actions,
                AvailableAction::enabled(
                    "change_workflow",
                    "Change workflow",
                    Some(project.id.clone()),
                ),
            );
            push_unique_action(
                &mut actions,
                AvailableAction::enabled(
                    "bind_blueprint",
                    "Apply workflow version",
                    Some(project.id.clone()),
                ),
            );
        }
        if self.screen == Screen::Project {
            for action in [
                AvailableAction::enabled("create_task", "New task", None),
                AvailableAction::enabled("create_work_item", "New work item", None),
                AvailableAction::enabled("edit_project", "Project settings", None),
                AvailableAction::enabled("change_workflow", "Change workflow", None),
                AvailableAction::enabled("bind_blueprint", "Apply workflow version", None),
                AvailableAction::enabled("install_blueprint", "Add bundled workflow", None),
                AvailableAction::enabled("preview_plan", "Preview plan", None),
                AvailableAction::enabled("bind_evidence", "Bind evidence", None),
                AvailableAction::enabled("inspect_evidence", "Project evidence", None),
                AvailableAction::enabled("unbind_evidence", "Unbind evidence", None),
                AvailableAction::enabled("refresh", "Refresh project", None),
                AvailableAction::enabled("command_palette", "Command palette", None),
            ] {
                push_unique_action(&mut actions, action);
            }
            if self
                .project
                .as_ref()
                .is_some_and(|project| project.generated)
            {
                push_unique_action(
                    &mut actions,
                    AvailableAction::enabled("apply_basic_workflow", "Apply Basic workflow", None),
                );
            }
            if self.pending_plan.is_some() {
                push_unique_action(
                    &mut actions,
                    AvailableAction::enabled("apply_plan", "Apply previewed plan", None),
                );
            }
            if !self.client.is_connected() {
                push_unique_action(
                    &mut actions,
                    AvailableAction::enabled(
                        "purge_attachments",
                        "Clean up unreferenced attachment content",
                        None,
                    ),
                );
            }
        } else {
            for action in [
                AvailableAction::enabled("create_task", "New task", None),
                AvailableAction::enabled("search", "Search", None),
                AvailableAction::enabled("refresh", "Refresh", None),
                AvailableAction::enabled("command_palette", "Command palette", None),
            ] {
                push_unique_action(&mut actions, action);
            }
        }
        if self.client.is_connected() {
            actions.retain(|action| {
                matches!(
                    action.id.as_str(),
                    "open"
                        | "create_task"
                        | "edit_task"
                        | "add_dependency"
                        | "remove_dependency"
                        | "set_task_status"
                        | "complete_task"
                        | "inspect_dependencies"
                        | "refresh"
                        | "command_palette"
                        | "search"
                        | "toggle_detail"
                        | "help"
                        | "go_home"
                        | "go_inbox"
                        | "go_agents"
                        | "switch_project"
                        | "flush_pending"
                        | "quit"
                )
            });
        }
        actions
    }

    pub fn move_selection(&mut self, delta: isize) {
        if self.screen == Screen::Projects && self.focus == FocusPane::Workspace {
            let max = self.projects.len().saturating_sub(1);
            self.projects_selection = self
                .projects_selection
                .saturating_add_signed(delta)
                .min(max);
            return;
        }
        match self.focus {
            FocusPane::Sidebar => {
                let max = self.sidebar_len().saturating_sub(1);
                self.sidebar_selection =
                    self.sidebar_selection.saturating_add_signed(delta).min(max);
            }
            FocusPane::Workspace => {
                let len = self
                    .current_component()
                    .map(|component| component.selection.len())
                    .unwrap_or(0);
                if len == 0 {
                    self.scroll = self.scroll.saturating_add_signed(delta);
                    self.selected = 0;
                } else {
                    self.selected = self.selected.saturating_add_signed(delta).min(len - 1);
                    self.detail_scroll = 0;
                }
            }
            FocusPane::Detail => {
                self.detail_scroll = self.detail_scroll.saturating_add_signed(delta);
            }
        }
    }

    pub fn move_component(&mut self, delta: isize) {
        let count = self.component_count();
        if count == 0 {
            return;
        }
        self.sync_cursor();
        let next = self
            .active_component
            .saturating_add_signed(delta)
            .min(count - 1);
        self.active_component = next;
        if let Some(key) = self.cursor_key()
            && let Some(state) = self.cursors.get_mut(&key)
        {
            state.active_component = next;
        }
        self.restore_cursor();
    }

    pub fn switch_view(&mut self, delta: isize) {
        let Some(state) = &mut self.project else {
            return;
        };
        if state.views.is_empty() {
            return;
        }
        let next = state
            .active_view
            .saturating_add_signed(delta)
            .min(state.views.len() - 1);
        if next != state.active_view {
            self.sync_cursor();
            if let Some(state) = &mut self.project {
                state.active_view = next;
            }
            self.restore_cursor();
        }
    }

    pub fn switch_view_by_index(&mut self, index: usize) {
        let Some(state) = &mut self.project else {
            return;
        };
        if index < state.views.len() {
            self.sync_cursor();
            if let Some(state) = &mut self.project {
                state.active_view = index;
            }
            self.restore_cursor();
        }
    }

    pub fn focus_next(&mut self, reverse: bool) {
        let mut panes = vec![FocusPane::Sidebar, FocusPane::Workspace];
        if self.show_detail && self.wide_detail() {
            panes.push(FocusPane::Detail);
        }
        let index = panes
            .iter()
            .position(|pane| *pane == self.focus)
            .unwrap_or(1);
        let next = if reverse {
            (index + panes.len() - 1) % panes.len()
        } else {
            (index + 1) % panes.len()
        };
        self.focus = panes[next];
        if self.focus == FocusPane::Detail {
            self.detail_scroll = 0;
        }
    }

    fn wide_detail(&self) -> bool {
        // The renderer hides the detail pane below the wide breakpoint; the
        // actual width is updated by `set_viewport`.
        self.viewport_width >= crate::theme::WIDE_BREAKPOINT
    }

    pub fn activate_sidebar(&mut self) {
        match self.sidebar_selection {
            0 => self.go_screen(Screen::Home),
            1 => self.go_screen(Screen::Inbox),
            2 => self.go_screen(Screen::Projects),
            3 => self.go_screen(Screen::Agents),
            index => {
                if let Some(project) = self.projects.get(index - 4) {
                    let id = project.id.clone();
                    self.open_project(&id);
                }
            }
        }
    }

    pub fn go_screen(&mut self, screen: Screen) {
        self.go_screen_internal(screen, true);
    }

    fn go_screen_internal(&mut self, screen: Screen, record_history: bool) {
        if record_history && self.screen != screen {
            self.push_history();
        }
        self.sync_cursor();
        self.screen = screen;
        self.focus = FocusPane::Workspace;
        self.prefix = None;
        self.sidebar_selection = match screen {
            Screen::Home => 0,
            Screen::Inbox => 1,
            Screen::Projects => 2,
            Screen::Agents => 3,
            Screen::Project => self.sidebar_selection,
        };
        if screen == Screen::Projects {
            self.projects_selection = self
                .projects_selection
                .min(self.projects.len().saturating_sub(1));
        }
        self.restore_cursor();
        if screen == Screen::Project {
            self.refresh_project();
        }
    }

    pub fn sidebar_len(&self) -> usize {
        4 + self.projects.len()
    }

    pub fn sidebar_label(&self, index: usize) -> String {
        match index {
            0 => "Home / Today".into(),
            1 => "Inbox".into(),
            2 => "Projects".into(),
            3 => "Agents".into(),
            index => self
                .projects
                .get(index - 4)
                .map(|project| format!("{}  {}", project.name, project.status))
                .unwrap_or_else(|| "Project".into()),
        }
    }

    fn clamp_component(&mut self) {
        let count = self.component_count();
        self.active_component = self.active_component.min(count.saturating_sub(1));
        let selected = self
            .current_component()
            .map(|component| component.selection.len())
            .unwrap_or(0);
        self.selected = self.selected.min(selected.saturating_sub(1));
        self.scroll = if selected == 0 {
            self.scroll
        } else {
            self.scroll.min(self.selected)
        };
    }

    pub fn edit_selected(&mut self) {
        if self.client.is_connected()
            && (self.screen == Screen::Projects
                || self.selected_item().map(|item| item.kind) != Some(ItemKind::Task))
        {
            self.notify(
                "This edit surface is local-only; connected mode supports task edits and uses the command surface for project settings, blueprints, metrics and notes",
                StatusKind::Warning,
            );
            return;
        }
        if self.screen == Screen::Projects {
            if let Some(project) = self.selected_project() {
                let project_id = project.id.clone();
                self.start_form(FormMode::EditProject { project_id });
            } else {
                self.notify("Select a project first", StatusKind::Warning);
            }
            return;
        }
        let Some(item) = self.selected_item() else {
            self.notify("Select an item first", StatusKind::Warning);
            return;
        };
        match item.kind {
            ItemKind::Task => {
                let task_id = item.id.clone();
                self.start_form(FormMode::EditTask { task_id });
            }
            ItemKind::Project => {
                if let Some(project) = &self.project {
                    let project_id = project.id.clone();
                    self.start_form(FormMode::EditProject { project_id });
                }
            }
            ItemKind::WorkItem => {
                let item_id = item.id.clone();
                self.start_edit_work_item(&item_id);
            }
            ItemKind::Metric => {
                let metric_id = Some(item.id.clone());
                self.start_form(FormMode::MetricTarget { metric_id });
            }
            _ => self.notify(
                "This item has no supported edit form; use the action menu",
                StatusKind::Info,
            ),
        }
    }

    fn start_edit_work_item(&mut self, item_id: &str) {
        let Some(project) = &self.project else {
            self.notify("Open a project first", StatusKind::Warning);
            return;
        };
        let Some(work_item) = project
            .data
            .work_items
            .iter()
            .find(|work_item| work_item.id == item_id)
        else {
            self.notify(
                "Work item is not available after refresh",
                StatusKind::Warning,
            );
            return;
        };
        let Some(blueprint) = self
            .client
            .project_blueprint_definition(&project.id)
            .ok()
            .flatten()
        else {
            self.notify("Pinned blueprint is unavailable", StatusKind::Warning);
            return;
        };
        self.start_form(FormMode::EditWorkItem {
            item_id: work_item.id.clone(),
            revision: work_item.revision,
            blueprint,
            kind: work_item.kind.clone(),
        });
    }

    pub fn open_action_menu(&mut self) {
        let actions = self.available_actions();
        if actions.is_empty() {
            self.notify(
                "No contextual actions for this selection",
                StatusKind::Warning,
            );
            return;
        }
        self.modal = Some(Modal::ActionMenu {
            title: self
                .selected_item()
                .map(|item| item.title.clone())
                .unwrap_or_else(|| "Project actions".into()),
            actions,
            selected: 0,
        });
    }

    pub fn open_detail(&mut self) {
        match self.detail_model() {
            Some(mut detail) => {
                if detail.kind == ItemKind::Knowledge
                    && let Ok(notes) = self.client.list_knowledge()
                    && let Some(note) = notes
                        .into_iter()
                        .find(|note| note.id == detail.title || note.title == detail.title)
                {
                    detail.note = note
                        .body_markdown
                        .lines()
                        .take(200)
                        .collect::<Vec<_>>()
                        .join("\n");
                }
                self.modal = Some(Modal::Detail(detail));
            }
            None => self.notify("Select an item first", StatusKind::Warning),
        }
    }

    pub fn open_palette(&mut self) {
        let entries = self.palette_entries("");
        self.modal = Some(Modal::Palette {
            query: String::new(),
            entries,
            selected: 0,
        });
    }

    pub fn palette_entries(&self, query: &str) -> Vec<PaletteEntry> {
        let mut entries = vec![
            palette_nav("Home / Today", "Global planning aggregation", Screen::Home),
            palette_nav("Inbox", "Untriaged tasks", Screen::Inbox),
            palette_nav("Projects", "All projects", Screen::Projects),
            palette_nav("Agents", "Agent execution monitoring", Screen::Agents),
        ];
        for project in self.projects.iter().take(30) {
            entries.push(PaletteEntry {
                label: format!("Open project: {}", project.name),
                description: format!("{} · {}", project.mode, project.status),
                action: PaletteAction::OpenProject(project.id.clone()),
                enabled: true,
            });
        }
        if let Some(state) = &self.project {
            for (index, view) in state.views.iter().enumerate() {
                entries.push(PaletteEntry {
                    label: format!("View: {}", view.title),
                    description: if view.generated {
                        "Generated default view".into()
                    } else {
                        format!("{} components", view.components.len())
                    },
                    action: PaletteAction::OpenView(index),
                    enabled: true,
                });
            }
        }
        let mut seen = std::collections::BTreeSet::new();
        for action in self.available_actions() {
            if !seen.insert((action.id.clone(), action.payload.clone())) {
                continue;
            }
            entries.push(PaletteEntry {
                label: action.label.clone(),
                description: action
                    .disabled_reason
                    .clone()
                    .unwrap_or_else(|| action.description.clone()),
                action: PaletteAction::Execute(action.clone()),
                enabled: action.enabled,
            });
        }
        entries.push(PaletteEntry {
            label: "New project".into(),
            description: "Create a project with a structured form".into(),
            action: PaletteAction::Form(FormMode::NewProject),
            enabled: true,
        });
        entries.push(PaletteEntry {
            label: "New task".into(),
            description: "Create a task with project, estimate and deadline".into(),
            action: PaletteAction::Form(FormMode::NewTask {
                project_id: self
                    .project
                    .as_ref()
                    .filter(|_| self.screen == Screen::Project)
                    .map(|project| project.id.clone()),
            }),
            enabled: true,
        });
        if !self.client.is_connected() {
            entries.push(PaletteEntry {
                label: "New knowledge note".into(),
                description: "Capture a Markdown note".into(),
                action: PaletteAction::Form(FormMode::NewKnowledge),
                enabled: true,
            });
            entries.push(PaletteEntry {
                label: "Install bundled blueprint".into(),
                description: "development, research, learning or habits".into(),
                action: PaletteAction::Form(FormMode::InstallBlueprint),
                enabled: true,
            });
            if let Some(state) = &self.project {
                entries.push(PaletteEntry {
                    label: "Bind blueprint version".into(),
                    description: "Explicitly pin a published blueprint version".into(),
                    action: PaletteAction::Form(FormMode::BindBlueprint {
                        project_id: state.id.clone(),
                    }),
                    enabled: true,
                });
            }
        }
        if !self.client.is_connected()
            && let Some(state) = &self.project
        {
            entries.push(PaletteEntry {
                label: "Unbind project evidence".into(),
                description: "Remove a habit or metric binding".into(),
                action: PaletteAction::Form(FormMode::BindEvidence {
                    project_id: state.id.clone(),
                    unbind: true,
                }),
                enabled: true,
            });
        }
        entries.push(PaletteEntry {
            label: "Keyboard help".into(),
            description: "Show all navigation and action keys".into(),
            action: PaletteAction::Execute(AvailableAction::enabled("help", "Help", None)),
            enabled: true,
        });
        entries.push(PaletteEntry {
            label: "Quit".into(),
            description: "Leave Orbit".into(),
            action: PaletteAction::Quit,
            enabled: true,
        });
        let needle = query.trim().to_lowercase();
        entries
            .into_iter()
            .filter(|entry| {
                needle.is_empty()
                    || entry.label.to_lowercase().contains(&needle)
                    || entry.description.to_lowercase().contains(&needle)
            })
            .collect()
    }

    pub fn execute_palette_action(&mut self, action: PaletteAction) -> bool {
        match action {
            PaletteAction::Go(screen) => self.go_screen(screen),
            PaletteAction::OpenProject(id) => self.open_project(&id),
            PaletteAction::OpenView(index) => {
                if self.screen != Screen::Project {
                    return false;
                }
                self.switch_view_by_index(index);
            }
            PaletteAction::Execute(action) => return self.execute_action(action),
            PaletteAction::Form(mode) => self.start_form(mode),
            PaletteAction::SelectItem { component, index } => {
                self.active_component = component;
                self.selected = index;
                self.focus = FocusPane::Workspace;
                self.open_detail();
            }
            PaletteAction::OpenKnowledge(id) => {
                if let Ok(notes) = self.client.list_knowledge()
                    && let Some(note) = notes.into_iter().find(|note| note.id == id)
                {
                    self.modal = Some(Modal::Detail(DetailModel {
                        title: note.title.clone(),
                        subtitle: format!("{} · {}", note.kind, note.status),
                        status: note.status,
                        kind: ItemKind::Knowledge,
                        fields: vec![
                            ("id".into(), note.id),
                            ("kind".into(), note.kind),
                            ("provenance".into(), note.provenance),
                        ],
                        note: note.body_markdown,
                        actions: Vec::new(),
                    }));
                } else {
                    self.notify("Knowledge note not found", StatusKind::Warning);
                }
            }
            PaletteAction::Quit => return true,
        }
        false
    }

    pub fn execute_action(&mut self, action: AvailableAction) -> bool {
        if !action.enabled {
            self.notify(
                action
                    .disabled_reason
                    .clone()
                    .unwrap_or_else(|| format!("{} is unavailable", action.label)),
                StatusKind::Warning,
            );
            return false;
        }
        match action.id.as_str() {
            "open" if self.screen == Screen::Projects => {
                if let Some(id) = action.payload {
                    self.open_project(&id);
                } else {
                    self.activate_project_selection();
                }
            }
            "open" | "open_knowledge" => self.open_detail(),
            "create_task" => self.start_form(FormMode::NewTask {
                project_id: self
                    .project
                    .as_ref()
                    .filter(|_| self.screen == Screen::Project)
                    .map(|state| state.id.clone()),
            }),
            "create_work_item" => {
                if let Some(state) = &self.project {
                    self.start_form(FormMode::NewWorkItemKind {
                        project_id: state.id.clone(),
                    });
                } else {
                    self.notify(
                        "Open a project before creating a work item",
                        StatusKind::Warning,
                    );
                }
            }
            "edit_task" => {
                if let Some(item) = self.selected_item()
                    && item.kind == ItemKind::Task
                {
                    let task_id = item.id.clone();
                    self.start_form(FormMode::EditTask { task_id });
                }
            }
            "add_dependency" => {
                if let Some(item) = self.selected_item()
                    && item.kind == ItemKind::Task
                {
                    let task_id = item.id.clone();
                    self.start_form(FormMode::AddDependency { task_id });
                }
            }
            "remove_dependency" => {
                if let Some(item) = self.selected_item()
                    && item.kind == ItemKind::Task
                {
                    let task_id = item.id.clone();
                    self.start_form(FormMode::RemoveDependency { task_id });
                }
            }
            "edit_work_item" => {
                if let Some(item) = self.selected_item() {
                    let item_id = item.id.clone();
                    self.start_edit_work_item(&item_id);
                } else {
                    self.notify("Select a work item first", StatusKind::Warning);
                }
            }
            "edit_project" => {
                let project_id = if self.screen == Screen::Projects {
                    self.selected_project().map(|project| project.id.clone())
                } else {
                    self.project.as_ref().map(|project| project.id.clone())
                };
                if let Some(project_id) = project_id {
                    self.start_form(FormMode::EditProject { project_id });
                } else {
                    self.notify("Select a project first", StatusKind::Warning);
                }
            }
            "install_blueprint" => self.start_form(FormMode::InstallBlueprint),
            "bind_blueprint" => {
                let project_id = if self.screen == Screen::Projects {
                    self.selected_project().map(|project| project.id.clone())
                } else {
                    self.project.as_ref().map(|project| project.id.clone())
                };
                if let Some(project_id) = project_id {
                    self.start_form(FormMode::BindBlueprint { project_id });
                } else {
                    self.notify("Select a project first", StatusKind::Warning);
                }
            }
            "set_task_status" | "complete_task" => {
                let Some(item) = self.selected_item() else {
                    self.notify("Select a task first", StatusKind::Warning);
                    return false;
                };
                let status = action.payload.clone().unwrap_or_else(|| "done".into());
                let pending = PendingAction::SetTaskStatus {
                    task_id: item.id.clone(),
                    status: status.clone(),
                };
                if matches!(status.as_str(), "done" | "cancelled") {
                    self.confirm(
                        "Change task status",
                        format!("Move '{}' to {status}?", item.title),
                        pending,
                    );
                } else {
                    self.execute_pending(pending);
                }
            }
            "blueprint_transition" => {
                let Some(item) = self.selected_item() else {
                    self.notify("Select a work item first", StatusKind::Warning);
                    return false;
                };
                let Some(to) = action.payload.clone() else {
                    return false;
                };
                let Some(revision) = item.revision else {
                    self.notify(
                        "Work item revision unavailable; refresh first",
                        StatusKind::Error,
                    );
                    return false;
                };
                self.execute_pending(PendingAction::BlueprintTransition {
                    item_id: item.id.clone(),
                    to,
                    revision,
                });
            }
            "edit_task_details" => {
                let Some(item) = self.selected_item() else {
                    self.notify("Select a task first", StatusKind::Warning);
                    return false;
                };
                if item.kind != ItemKind::Task {
                    self.notify("Details editing applies to tasks", StatusKind::Warning);
                    return false;
                }
                let task_id = item.id.clone();
                self.start_form(FormMode::EditTaskDetails { task_id });
            }
            "manage_acceptance_criteria" => {
                let Some(item) = self.selected_item() else {
                    self.notify("Select a task first", StatusKind::Warning);
                    return false;
                };
                if item.kind != ItemKind::Task {
                    self.notify("Acceptance criteria apply to tasks", StatusKind::Warning);
                    return false;
                }
                let task_id = item.id.clone();
                let title = item.title.clone();
                self.open_criteria(task_id, title);
            }
            "task_activity" => {
                let Some(item) = self.selected_item() else {
                    self.notify("Select a task first", StatusKind::Warning);
                    return false;
                };
                if item.kind != ItemKind::Task {
                    self.notify("Activity history applies to tasks", StatusKind::Warning);
                    return false;
                }
                let task_id = item.id.clone();
                let title = format!("Activity · {}", item.title);
                self.open_activity(task_id, title);
            }
            "attach_file" => {
                let Some(item) = self.selected_item() else {
                    self.notify("Select an item first", StatusKind::Warning);
                    return false;
                };
                let Some((owner_kind, owner_id)) = Self::attachment_owner(item) else {
                    self.notify(
                        "Attachments are supported for tasks, projects and work items",
                        StatusKind::Warning,
                    );
                    return false;
                };
                self.start_form(FormMode::AttachFile {
                    owner_kind: owner_kind.to_owned(),
                    owner_id,
                });
            }
            "open_attachments" => {
                let Some(item) = self.selected_item() else {
                    self.notify("Select an item first", StatusKind::Warning);
                    return false;
                };
                let Some((owner_kind, owner_id)) = Self::attachment_owner(item) else {
                    self.notify(
                        "Attachments are supported for tasks, projects and work items",
                        StatusKind::Warning,
                    );
                    return false;
                };
                let title = format!("Attachments · {}", item.title);
                self.open_attachments(owner_kind.to_owned(), owner_id, title);
            }
            "change_workflow" => {
                let project_id = if self.screen == Screen::Projects {
                    self.selected_project().map(|project| project.id.clone())
                } else {
                    self.project.as_ref().map(|project| project.id.clone())
                };
                if let Some(project_id) = project_id {
                    self.start_form(FormMode::ChangeWorkflow { project_id });
                } else {
                    self.notify("Select a project first", StatusKind::Warning);
                }
            }
            "apply_basic_workflow" => {
                let Some(project_id) = self.project.as_ref().map(|project| project.id.clone())
                else {
                    self.notify("Open a project first", StatusKind::Warning);
                    return false;
                };
                match self
                    .client
                    .apply_workflow(project_id.clone(), "basic".to_owned(), None)
                {
                    Ok(_) => {
                        self.notify("Basic workflow applied", StatusKind::Success);
                        self.refresh_after_mutation();
                    }
                    Err(error) => self.notify(
                        format!("Applying the Basic workflow failed: {}", error.message),
                        StatusKind::Error,
                    ),
                }
            }
            "purge_attachments" => match self.client.purge_attachment_content() {
                Ok(removed) => self.notify(
                    format!("Removed {removed} unreferenced attachment object(s)"),
                    StatusKind::Success,
                ),
                Err(error) => self.notify(
                    format!("Attachment cleanup failed: {}", error.message),
                    StatusKind::Error,
                ),
            },
            "record_habit" => {
                let Some(item) = self.selected_item() else {
                    self.notify("Select a habit first", StatusKind::Warning);
                    return false;
                };
                self.execute_pending(PendingAction::RecordHabit {
                    habit_id: item.id.clone(),
                });
            }
            "record_metric" => {
                let metric_id = self.selected_item().map(|item| item.id.clone());
                self.start_form(FormMode::MetricObservation { metric_id });
            }
            "set_metric_target" => {
                let metric_id = self.selected_item().map(|item| item.id.clone());
                self.start_form(FormMode::MetricTarget { metric_id });
            }
            "bind_evidence" => {
                if let Some(state) = &self.project {
                    self.start_form(FormMode::BindEvidence {
                        project_id: state.id.clone(),
                        unbind: false,
                    });
                } else {
                    self.notify("Open a project first", StatusKind::Warning);
                }
            }
            "inspect_evidence" => {
                let project_id = self.project.as_ref().map(|project| project.id.clone());
                if let Some(project_id) = project_id {
                    match self.client.list_project_evidence(project_id) {
                        Ok(bindings) if bindings.is_empty() => {
                            self.modal = Some(Modal::Message {
                                title: "Project evidence".into(),
                                lines: vec![
                                    "No habits or metrics are bound to this project.".into(),
                                ],
                            });
                        }
                        Ok(bindings) => {
                            self.modal = Some(Modal::Message {
                                title: "Project evidence".into(),
                                lines: bindings
                                    .into_iter()
                                    .map(|binding| format!("{}:{}", binding.kind, binding.id))
                                    .collect(),
                            });
                        }
                        Err(error) => self.notify(
                            format!("Evidence unavailable: {}", error.message),
                            StatusKind::Error,
                        ),
                    }
                }
            }
            "unbind_evidence" => {
                if let Some(state) = &self.project {
                    self.start_form(FormMode::BindEvidence {
                        project_id: state.id.clone(),
                        unbind: true,
                    });
                }
            }
            "inspect_dependencies" => {
                let lines = self
                    .selected_item()
                    .map(|item| {
                        item.fields
                            .iter()
                            .filter(|(key, _)| {
                                key.contains("prereq")
                                    || key.contains("blocked")
                                    || key == "dependencies"
                            })
                            .map(|(key, value)| format!("{key}: {value}"))
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                self.modal = Some(Modal::Message {
                    title: "Dependencies".into(),
                    lines: if lines.is_empty() {
                        vec!["No dependencies recorded for this item.".into()]
                    } else {
                        lines
                    },
                });
            }
            "inspect_agent" | "review_agent" => {
                let Some(item) = self.selected_item() else {
                    self.notify("Select an agent job first", StatusKind::Warning);
                    return false;
                };
                match self.client.agent_job_detail(&item.id) {
                    Ok(Some(detail)) => {
                        let mut lines = vec![
                            format!("state: {}", detail.job.state),
                            format!("agent: {}", detail.job.agent_kind),
                            format!("task: {}", detail.job.task_id),
                            format!("runtime: {}", detail.job.runtime_kind),
                            format!("handoffs: {}", detail.handoffs.len()),
                        ];
                        if let Some(decision) = &detail.job.review_decision {
                            lines.push(format!("review: {decision}"));
                        }
                        if let Some(review) = &detail.review {
                            lines.push(format!("review record: {review}"));
                        }
                        for handoff in detail.handoffs.iter().take(5) {
                            lines.push(format!("handoff: {handoff}"));
                        }
                        lines.push(String::new());
                        lines.push(
                            "Job completion is execution state, not task acceptance. Review/resume remains an explicit authorized command."
                                .into(),
                        );
                        self.modal = Some(Modal::Message {
                            title: "Agent job detail".into(),
                            lines,
                        });
                    }
                    Ok(None) => {
                        self.notify("Agent job not found in accepted state", StatusKind::Warning)
                    }
                    Err(error) => self.notify(
                        format!("Agent job unavailable: {}", error.message),
                        StatusKind::Error,
                    ),
                }
            }
            "preview_plan" => self.preview_plan(),
            "apply_plan" => {
                if self.pending_plan.is_some() {
                    self.confirm(
                        "Apply plan",
                        "Apply the previewed schedule after authoritative revalidation?".into(),
                        PendingAction::ApplyPlan,
                    );
                } else {
                    self.notify("No pending plan; preview with p first", StatusKind::Warning);
                }
            }
            "flush_pending" => match self.client.flush_pending() {
                Ok((message, blocked)) => {
                    self.refresh_after_mutation();
                    self.notify(
                        message,
                        if blocked {
                            StatusKind::Warning
                        } else {
                            StatusKind::Success
                        },
                    );
                }
                Err(error) => self.notify(
                    format!("Flush failed: {}", error.message),
                    StatusKind::Error,
                ),
            },
            "refresh" => self.refresh_current(),
            "toggle_detail" => self.show_detail = !self.show_detail,
            "command_palette" => self.open_palette(),
            "search" => self.open_search(),
            "help" => self.modal = Some(Modal::Help),
            "switch_project" => self.go_screen(Screen::Projects),
            "go_home" => self.go_screen(Screen::Home),
            "go_inbox" => self.go_screen(Screen::Inbox),
            "go_agents" => self.go_screen(Screen::Agents),
            "quit" => return true,
            other => self.notify(
                format!("Action '{other}' is registered but has no local handler"),
                StatusKind::Warning,
            ),
        }
        false
    }

    fn confirm(&mut self, title: &str, body: String, pending: PendingAction) {
        self.modal = Some(Modal::Confirm {
            title: title.into(),
            body,
            pending,
        });
    }

    pub fn execute_pending(&mut self, pending: PendingAction) {
        let key = format!("{pending:?}");
        if let Some((last, at)) = &self.last_submission
            && last == &key
            && at.elapsed() < Duration::from_millis(750)
        {
            self.notify(
                "Duplicate submission ignored; waiting for refresh",
                StatusKind::Warning,
            );
            return;
        }
        self.last_submission = Some((key, Instant::now()));
        match pending {
            PendingAction::SetTaskStatus { task_id, status } => {
                match self.client.set_task_status(SetTaskStatusRequest {
                    id: task_id.clone(),
                    status: status.clone(),
                }) {
                    Ok(task) => {
                        let (message, kind) =
                            self.mutation_status(format!("{} → {}", task.title, task.status));
                        self.notify(message, kind);
                        self.refresh_after_mutation();
                    }
                    Err(error) => self.notify(
                        format!("Task transition rejected: {}", error.message),
                        StatusKind::Error,
                    ),
                }
            }
            PendingAction::BlueprintTransition {
                item_id,
                to,
                revision,
            } => {
                match self
                    .client
                    .transition_blueprint_work_item(&item_id, &to, revision)
                {
                    Ok(item) => {
                        self.notify(
                            format!("Accepted: {} → {}", item.id, item.state),
                            StatusKind::Success,
                        );
                        self.refresh_after_mutation();
                    }
                    Err(error) => self.notify(
                        format!("Workflow transition rejected: {}", error.message),
                        StatusKind::Error,
                    ),
                }
            }
            PendingAction::RecordHabit { habit_id } => {
                match self
                    .client
                    .record_habit_occurrence(RecordHabitOccurrenceRequest {
                        id: None,
                        habit_id,
                        occurred_at_ms: Some(now_ms()),
                        duration_minutes: None,
                    }) {
                    Ok(occurrence) => {
                        self.notify(
                            format!("Accepted habit occurrence {}", occurrence.id),
                            StatusKind::Success,
                        );
                        self.refresh_after_mutation();
                    }
                    Err(error) => self.notify(
                        format!("Habit occurrence rejected: {}", error.message),
                        StatusKind::Error,
                    ),
                }
            }
            PendingAction::RemoveCriterion {
                task_id,
                criterion_id,
                task_title,
            } => {
                match self
                    .client
                    .remove_acceptance_criterion(task_id.clone(), criterion_id)
                {
                    Ok(_) => {
                        self.notify("Acceptance criterion removed", StatusKind::Success);
                        self.open_criteria(task_id, task_title);
                    }
                    Err(error) => self.notify(
                        format!("Removing the criterion failed: {}", error.message),
                        StatusKind::Error,
                    ),
                }
            }
            PendingAction::RemoveAttachment {
                attachment_id,
                owner_kind,
                owner_id,
                title,
            } => match self.client.remove_attachment(attachment_id) {
                Ok(content_removed) => {
                    self.notify(
                        if content_removed {
                            "Attachment removed; unreferenced content deleted"
                        } else {
                            "Attachment removed; content still referenced elsewhere"
                        },
                        StatusKind::Success,
                    );
                    self.open_attachments(owner_kind, owner_id, title);
                }
                Err(error) => self.notify(
                    format!("Removing the attachment failed: {}", error.message),
                    StatusKind::Error,
                ),
            },
            PendingAction::OpenAttachment {
                attachment_id,
                filename,
            } => {
                self.open_attachment_externally(&attachment_id, &filename);
            }
            PendingAction::DiscardFormChanges { form } => {
                // The user confirmed discarding; nothing was mutated.
                self.notify("Unsaved changes discarded", StatusKind::Info);
                let _ = form;
            }
            PendingAction::ApplyPlan => {
                let Some(plan) = self.pending_plan.clone() else {
                    self.notify("No pending plan", StatusKind::Warning);
                    return;
                };
                match self.client.apply_task_plan(plan) {
                    Ok(blocks) => {
                        self.pending_plan = None;
                        self.notify(
                            format!(
                                "Applied {} schedule blocks after revalidation",
                                blocks.len()
                            ),
                            StatusKind::Success,
                        );
                        self.refresh_after_mutation();
                    }
                    Err(error) => self.notify(
                        format!("Plan apply rejected: {}", error.message),
                        StatusKind::Error,
                    ),
                }
            }
        }
    }

    pub fn preview_plan(&mut self) {
        let start = now_ms();
        let end = start + 8 * 60 * 60 * 1000;
        let policy = orbit_client::PlanningPolicyDto {
            protected_minutes: 60,
            max_new_work_minutes: Some(240),
            fixed_buffer_before_minutes: 5,
            fixed_buffer_after_minutes: 5,
        };
        match self.client.preview_task_plan(start, end, policy) {
            Ok(plan) => {
                let scheduled = plan
                    .scheduled
                    .iter()
                    .map(|block| {
                        format!(
                            "{}  {} → {}",
                            block.task_id,
                            format_epoch_ms(block.start_ms),
                            format_epoch_ms(block.end_ms)
                        )
                    })
                    .collect::<Vec<_>>();
                let unscheduled = plan
                    .unscheduled
                    .iter()
                    .map(|item| format!("{}  {:?}", item.task_id, item.reason))
                    .collect::<Vec<_>>();
                let mut lines = vec![
                    format!("Scheduled: {}", plan.scheduled.len()),
                    format!("Unscheduled: {}", plan.unscheduled.len()),
                    String::new(),
                ];
                lines.extend(scheduled);
                if !unscheduled.is_empty() {
                    lines.push(String::new());
                    lines.push("Unscheduled".into());
                    lines.extend(unscheduled);
                }
                lines.push(String::new());
                lines.push("Shift+P applies only after authoritative revalidation.".into());
                self.pending_plan = Some(plan);
                self.modal = Some(Modal::Message {
                    title: "Plan preview".into(),
                    lines,
                });
            }
            Err(error) => self.notify(
                format!("Plan preview failed: {}", error.message),
                StatusKind::Error,
            ),
        }
    }

    pub fn completion_impact(&mut self) {
        let Some(task_id) = self.selected_item().map(|item| item.id.clone()) else {
            self.notify("Select a task first", StatusKind::Warning);
            return;
        };
        let request = orbit_runtime::task_impact::CompletionImpactRequest {
            task_id,
            now_ms: now_ms(),
            next_limit: 20,
        };
        match self.client.completion_impact(&request) {
            Ok(report) => {
                self.modal = Some(Modal::Message {
                    title: "Completion impact (read-only)".into(),
                    lines: planning_ui::render_impact(&report),
                });
            }
            Err(error) => self.notify(
                format!("Impact preview unavailable: {}", error.message),
                StatusKind::Error,
            ),
        }
    }

    pub fn today_focus(&mut self) {
        let Some(task_id) = self.selected_item().map(|item| item.id.clone()) else {
            self.notify("Select a task first", StatusKind::Warning);
            return;
        };
        let result = (|| -> Result<Vec<String>, String> {
            let request = planning_ui::request(now_ms(), self.planning_hours, 100, None)?;
            let brief = self.client.work_brief(&request).map_err(|e| e.message)?;
            let focus = self
                .client
                .planning_focus(
                    &brief,
                    &orbit_runtime::planning_focus::FocusRequest {
                        task_id,
                        max_neighbors: 12,
                    },
                )
                .map_err(|error| error.message)?;
            Ok(planning_ui::render_focus(&focus))
        })();
        match result {
            Ok(lines) => {
                self.modal = Some(Modal::Message {
                    title: "Task focus (read-only)".into(),
                    lines,
                })
            }
            Err(error) => self.notify(
                format!("Task focus unavailable: {error}"),
                StatusKind::Error,
            ),
        }
    }

    /// Preserve the accepted Today/Next action-hint workflow: recompute the
    /// menu, bind selections to the reviewed revision, and render only
    /// read-only destinations.
    pub fn today_actions(&mut self) {
        let Some(task_id) = self
            .selected_item()
            .filter(|item| item.kind == ItemKind::Task)
            .map(|item| item.id.clone())
        else {
            self.notify(
                "Select an accepted task in Today/Inbox first",
                StatusKind::Warning,
            );
            return;
        };
        let result = (|| -> Result<(WorkBrief, PlanningActionMenu), String> {
            let request = planning_ui::request(
                now_ms(),
                self.planning_hours,
                100,
                self.planning_project.clone(),
            )?;
            let brief = self.client.work_brief(&request).map_err(|e| e.message)?;
            let menu = self
                .client
                .planning_task_actions(
                    &brief,
                    &orbit_runtime::planning_actions::TaskActionRequest {
                        task_id,
                        authority_available: true,
                    },
                )
                .map_err(|e| e.message)?;
            Ok((brief, menu))
        })();
        match result {
            Ok((brief, menu)) => {
                self.today_action_menu = Some((brief, menu.clone()));
                self.modal = Some(Modal::TodayActions { menu, selected: 0 });
                self.notify(
                    "Read-only planning destinations • Enter previews • Esc returns",
                    StatusKind::Info,
                );
            }
            Err(error) => self.notify(
                format!("Planning actions unavailable: {error}"),
                StatusKind::Error,
            ),
        }
    }

    pub fn today_select_action(&mut self, index: usize) {
        let Some((brief, menu)) = self.today_action_menu.clone() else {
            self.notify(
                "Open the planning action menu with A first",
                StatusKind::Warning,
            );
            return;
        };
        let result = (|| -> Result<Vec<String>, String> {
            let selection = orbit_runtime::planning_handoff::select_from_menu(&menu, index)
                .map_err(str::to_owned)?;
            // The accepted revision may have changed since the menu was drawn.
            let latest = self
                .client
                .work_brief(&planning_ui::request(
                    brief.evaluated_at_ms,
                    (brief.ends_at_ms - brief.evaluated_at_ms) / 3_600_000,
                    100,
                    self.planning_project.clone(),
                )?)
                .map_err(|e| e.message)?;
            if latest.revision != brief.revision {
                return Err("Accepted state changed: reopen the planning action menu".into());
            }
            let request = orbit_runtime::planning_actions::TaskActionRequest {
                task_id: menu
                    .selected_task_id
                    .clone()
                    .ok_or("Action menu missing a task target")?,
                authority_available: true,
            };
            let handoff = self
                .client
                .planning_task_handoff(&latest, &request, &selection)
                .map_err(|e| e.message)?;
            if let orbit_runtime::planning_handoff::HandoffView::Preview(preview) = &handoff.view
                && preview.operation
                    == orbit_runtime::planning_handoff::PreviewOperation::CompletionImpact
            {
                let report = self
                    .client
                    .completion_impact(&orbit_runtime::task_impact::CompletionImpactRequest {
                        task_id: preview.task_id.clone(),
                        now_ms: preview.evaluated_at_ms,
                        next_limit: 20,
                    })
                    .map_err(|e| e.message)?;
                if report.revision != handoff.revision {
                    return Err("Impact preview revision changed".into());
                }
                let mut lines = planning_ui::render_handoff(&handoff);
                lines.push(String::new());
                lines.extend(planning_ui::render_impact(&report));
                return Ok(lines);
            }
            if let Some(request) = planning_ui::dependency_request_from_handoff(&handoff)? {
                let report = self
                    .client
                    .preview_task_dependency(&request)
                    .map_err(|e| e.message)?;
                if report.accepted_revision != handoff.revision {
                    return Err("Dependency preview revision changed".into());
                }
                let mut lines = planning_ui::render_handoff(&handoff);
                lines.push(String::new());
                lines.extend(planning_ui::render_dependency_preview(&report));
                return Ok(lines);
            }
            if let Some(request) = planning_ui::calendar_request_from_handoff(&handoff)? {
                let plan = self.client.schedule_plan(&request).map_err(|e| e.message)?;
                let mut lines = planning_ui::render_handoff(&handoff);
                lines.push(String::new());
                lines.extend(planning_ui::render_task_calendar_preview(&handoff, &plan)?);
                return Ok(lines);
            }
            Ok(planning_ui::render_handoff(&handoff))
        })();
        match result {
            Ok(lines) => {
                self.today_action_menu = None;
                self.modal = Some(Modal::Message {
                    title: "Today/Next action destination (read-only)".into(),
                    lines,
                });
            }
            Err(error) => {
                self.today_action_menu = None;
                self.notify(
                    format!("Planning action rejected: {error}"),
                    StatusKind::Error,
                );
            }
        }
    }

    pub fn today_project_focus(&mut self) {
        let selected = self.selected_item().map(|item| item.id.clone());
        let result = (|| -> Result<Vec<String>, String> {
            let full_request = planning_ui::request(now_ms(), self.planning_hours, 100, None)?;
            let full_brief = self
                .client
                .work_brief(&full_request)
                .map_err(|e| e.message)?;
            let project_id = selected
                .as_ref()
                .and_then(|id| full_brief.items.iter().find(|row| &row.id == id))
                .and_then(|row| row.project_id.clone())
                .or_else(|| self.planning_project.clone())
                .ok_or("Select a task assigned to a project, or filter Today by project")?;
            let request =
                planning_ui::request(now_ms(), self.planning_hours, 100, Some(project_id.clone()))?;
            let brief = self.client.work_brief(&request).map_err(|e| e.message)?;
            let group = self
                .client
                .planning_group(
                    &brief,
                    &orbit_runtime::planning_focus::GroupFocusRequest {
                        project_id,
                        milestone_id: None,
                        max_tasks: 12,
                    },
                )
                .map_err(|e| e.message)?;
            Ok(planning_ui::render_group_focus(&group))
        })();
        match result {
            Ok(lines) => {
                self.modal = Some(Modal::Message {
                    title: "Accepted project context (read-only)".into(),
                    lines,
                })
            }
            Err(error) => self.notify(
                format!("Project context unavailable: {error}"),
                StatusKind::Error,
            ),
        }
    }

    pub fn adjust_planning_horizon(&mut self, longer: bool) {
        self.planning_hours = if longer {
            self.planning_hours.saturating_mul(2).min(168)
        } else {
            (self.planning_hours / 2).max(1)
        };
        self.refresh_planning();
        if let Some(workspace) = &self.workspace {
            self.home = orbit_ui::home_view(workspace);
            self.insert_planning_component();
        }
        self.notify(
            format!("Today horizon set to {}h", self.planning_hours),
            StatusKind::Info,
        );
    }

    fn text_field(
        key: &str,
        label: &str,
        value: impl Into<String>,
        required: bool,
        hint: &str,
    ) -> FormField {
        let value = value.into();
        let cursor = value.chars().count();
        FormField {
            key: key.into(),
            label: label.into(),
            initial: value.clone(),
            value,
            cursor,
            kind: FormFieldKind::Text,
            required,
            hint: hint.into(),
        }
    }

    fn integer_field(
        key: &str,
        label: &str,
        value: impl Into<String>,
        required: bool,
        hint: &str,
    ) -> FormField {
        let value = value.into();
        let cursor = value.chars().count();
        FormField {
            key: key.into(),
            label: label.into(),
            initial: value.clone(),
            value,
            cursor,
            kind: FormFieldKind::Integer,
            required,
            hint: hint.into(),
        }
    }

    fn boolean_field(key: &str, label: &str, value: bool) -> FormField {
        let value = if value { "true" } else { "false" }.to_owned();
        FormField {
            key: key.into(),
            label: label.into(),
            initial: value.clone(),
            value,
            cursor: 0,
            kind: FormFieldKind::Boolean,
            required: false,
            hint: String::new(),
        }
    }

    fn choice_field(
        key: &str,
        label: &str,
        value: impl Into<String>,
        choices: Vec<FormChoice>,
        required: bool,
    ) -> FormField {
        let value = value.into();
        FormField {
            key: key.into(),
            label: label.into(),
            initial: value.clone(),
            value,
            cursor: 0,
            kind: FormFieldKind::Choice(choices),
            required,
            hint: String::new(),
        }
    }

    fn form_choice(value: impl Into<String>, label: impl Into<String>) -> FormChoice {
        FormChoice {
            value: value.into(),
            label: label.into(),
        }
    }

    fn multiline_field(
        key: &str,
        label: &str,
        value: impl Into<String>,
        required: bool,
        hint: &str,
    ) -> FormField {
        let value = value.into();
        let cursor = value.chars().count();
        FormField {
            key: key.into(),
            label: label.into(),
            initial: value.clone(),
            value,
            cursor,
            kind: FormFieldKind::Multiline,
            required,
            hint: hint.into(),
        }
    }

    /// Workflows available to apply, with Basic first and clearly marked as the
    /// default. Works offline because the descriptors are compiled in.
    fn workflow_choices(&self) -> Vec<FormChoice> {
        let mut descriptors = self.client.bundled_workflows();
        descriptors.sort_by_key(|descriptor| (!descriptor.is_default, descriptor.name.clone()));
        descriptors
            .into_iter()
            .map(|descriptor| {
                let label = if descriptor.is_default {
                    format!("{} (default)", descriptor.name)
                } else {
                    descriptor.name.clone()
                };
                Self::form_choice(descriptor.id, label)
            })
            .collect()
    }

    fn workflow_label(&self, workflow_id: &str, version: u32) -> String {
        self.client
            .bundled_workflows()
            .into_iter()
            .find(|descriptor| descriptor.id == workflow_id)
            .map(|descriptor| format!("{} v{version}", descriptor.name))
            .unwrap_or_else(|| format!("{workflow_id} v{version}"))
    }

    /// Attachment owner kind for an item, when the entity supports attachments.
    fn attachment_owner(item: &SelectionItem) -> Option<(&'static str, String)> {
        match item.kind {
            ItemKind::Task => Some(("task", item.id.clone())),
            ItemKind::WorkItem => Some(("work_item", item.id.clone())),
            ItemKind::Project => Some(("project", item.id.clone())),
            _ => None,
        }
    }

    fn task_dto(&self, task_id: &str) -> Option<orbit_client::TaskDto> {
        self.client
            .list_tasks()
            .ok()?
            .into_iter()
            .find(|task| task.id == task_id)
    }

    /// Load ordered acceptance criteria for a task into a modal.
    pub fn open_criteria(&mut self, task_id: String, task_title: String) {
        match self.client.acceptance_criteria(&task_id) {
            Ok(items) => {
                let selected = 0;
                self.modal = Some(Modal::Criteria(CriteriaModal {
                    task_id,
                    task_title,
                    items,
                    selected,
                }));
            }
            Err(error) => self.notify(
                format!("Acceptance criteria unavailable: {}", error.message),
                StatusKind::Warning,
            ),
        }
    }

    /// Reload the criteria modal in place after a mutation.
    /// Metadata and a bounded text preview for one attachment. Binary content
    /// is never executed or decoded as an image here; the user is directed to
    /// the external viewer instead.
    pub fn preview_attachment(&mut self, attachment_id: String) {
        match self.client.attachment_content(&attachment_id) {
            Ok((attachment, content)) => {
                let mut lines = vec![
                    format!("File: {}", attachment.filename),
                    format!("Type: {}", attachment.content_type),
                    format!("Size: {} bytes", attachment.size_bytes),
                    format!("Checksum: {}", attachment.checksum),
                    format!("Created: {}", attachment.created_at_ms),
                ];
                if let Some(description) = &attachment.description {
                    lines.push(format!("Caption: {description}"));
                }
                if attachment.is_image {
                    lines.push(String::new());
                    lines.push(
                        "Image preview is not inlined in this terminal. Press o to open it with your configured viewer."
                            .into(),
                    );
                } else if let Ok(text) = std::str::from_utf8(&content) {
                    lines.push(String::new());
                    lines.push("Preview (first 200 lines):".into());
                    lines.extend(text.lines().take(200).map(str::to_owned));
                } else {
                    lines.push(String::new());
                    lines.push("Binary content: no inline preview. Press o to open it.".into());
                }
                self.modal = Some(Modal::Message {
                    title: format!("Attachment · {}", attachment.filename),
                    lines,
                });
            }
            Err(error) => self.notify(
                format!("Attachment preview failed: {}", error.message),
                StatusKind::Error,
            ),
        }
    }

    /// Toggle one criterion's completion. Task status is never changed here.
    /// Move one criterion to a new index within the ordered list.
    pub fn open_attachments(&mut self, owner_kind: String, owner_id: String, title: String) {
        match self.client.list_attachments(&owner_kind, &owner_id) {
            Ok(items) => {
                self.modal = Some(Modal::Attachments(AttachmentsModal {
                    owner_kind,
                    owner_id,
                    title,
                    items,
                    selected: 0,
                }));
            }
            Err(error) => self.notify(
                format!("Attachments unavailable: {}", error.message),
                StatusKind::Warning,
            ),
        }
    }

    pub fn open_activity(&mut self, task_id: String, title: String) {
        match self.client.task_activity(&task_id) {
            Ok(items) => {
                self.modal = Some(Modal::Activity(ActivityModal {
                    title,
                    items,
                    scroll: 0,
                }));
            }
            Err(error) => self.notify(
                format!("Activity history unavailable: {}", error.message),
                StatusKind::Warning,
            ),
        }
    }

    /// Export an attachment to a managed export directory, then hand the path
    /// to the user-approved external viewer. Attachment content is never
    /// executed and never passed through a shell.
    fn open_attachment_externally(&mut self, attachment_id: &str, filename: &str) {
        let export_dir = match self.client.attachment_export_dir() {
            Ok(dir) => dir,
            Err(error) => {
                self.notify(
                    format!("Attachment viewing is unavailable: {}", error.message),
                    StatusKind::Warning,
                );
                return;
            }
        };
        if let Err(error) = std::fs::create_dir_all(&export_dir) {
            self.notify(
                format!("Could not prepare the export directory: {error}"),
                StatusKind::Error,
            );
            return;
        }
        let destination = export_dir.join(format!("{attachment_id}-{filename}"));
        match self
            .client
            .export_attachment(attachment_id, destination.to_string_lossy().into_owned())
        {
            Ok(attachment) => match crate::viewer::open_with_viewer(&destination) {
                Ok(viewer) => self.notify(
                    format!("Opened {} with {viewer}", attachment.filename),
                    StatusKind::Success,
                ),
                Err(reason) => self.notify(
                    format!(
                        "Exported {} to {}; open it manually ({reason})",
                        attachment.filename,
                        destination.display()
                    ),
                    StatusKind::Info,
                ),
            },
            Err(error) => self.notify(
                format!("Opening the attachment failed: {}", error.message),
                StatusKind::Error,
            ),
        }
    }

    fn project_choices(&self, include_none: bool) -> Vec<FormChoice> {
        let mut choices = Vec::new();
        if include_none {
            choices.push(Self::form_choice("", "(none)"));
        }
        choices.extend(
            self.projects
                .iter()
                .map(|project| Self::form_choice(project.id.clone(), project.name.clone())),
        );
        choices
    }

    fn metric_choices(&self, include_none: bool) -> Vec<FormChoice> {
        let mut choices = Vec::new();
        if include_none {
            choices.push(Self::form_choice("", "(select)"));
        }
        if let Some(project) = &self.project {
            choices.extend(
                project
                    .data
                    .metrics
                    .iter()
                    .map(|metric| Self::form_choice(metric.id.clone(), metric.id.clone())),
            );
        }
        choices
    }

    fn blueprint_version_choices(&self) -> Vec<FormChoice> {
        let mut choices = Vec::new();
        if let Ok(installed) = self.client.list_blueprint_versions() {
            choices.extend(
                installed
                    .into_iter()
                    .map(|(id, version)| {
                        Self::form_choice(format!("{id}@{version}"), format!("{id}@{version}"))
                    })
                    .collect::<Vec<_>>(),
            );
        }
        choices
    }

    fn work_item_kind_choices(blueprint: &Blueprint) -> Vec<FormChoice> {
        blueprint
            .work_items
            .iter()
            .map(|kind| Self::form_choice(kind.id.clone(), kind.name.clone()))
            .collect()
    }

    fn work_item_fields(blueprint: &Blueprint, kind: &str) -> Option<Vec<FormField>> {
        let definition = blueprint.item_kind(kind).ok()?;
        Some(
            definition
                .fields
                .iter()
                .map(|field| {
                    let value = field
                        .default
                        .as_ref()
                        .map(|value| match value {
                            serde_json::Value::String(text) => text.clone(),
                            other => other.to_string(),
                        })
                        .unwrap_or_default();
                    match &field.kind {
                        FieldKind::Integer => Self::integer_field(
                            &field.id,
                            &field.id,
                            value,
                            field.required,
                            "integer",
                        ),
                        FieldKind::Boolean => Self::boolean_field(
                            &field.id,
                            &field.id,
                            field
                                .default
                                .as_ref()
                                .and_then(|value| value.as_bool())
                                .unwrap_or(false),
                        ),
                        FieldKind::Choice { options } => Self::choice_field(
                            &field.id,
                            &field.id,
                            value,
                            options
                                .iter()
                                .map(|option| Self::form_choice(option.clone(), option.clone()))
                                .collect(),
                            field.required,
                        ),
                        FieldKind::Text => {
                            Self::text_field(&field.id, &field.id, value, field.required, "text")
                        }
                    }
                })
                .collect(),
        )
    }

    pub fn start_form(&mut self, mode: FormMode) {
        let Some(form) = self.build_form(mode) else {
            return;
        };
        self.modal = Some(Modal::Form(form));
    }

    fn project_status_choices(current: &str) -> Vec<FormChoice> {
        use orbit_domain::ProjectStatus;
        let current_status = match current {
            "proposed" => ProjectStatus::Proposed,
            "active" => ProjectStatus::Active,
            "on_hold" => ProjectStatus::OnHold,
            "completed" => ProjectStatus::Completed,
            "cancelled" => ProjectStatus::Cancelled,
            "archived" => ProjectStatus::Archived,
            _ => return Vec::new(),
        };
        [
            ProjectStatus::Proposed,
            ProjectStatus::Active,
            ProjectStatus::OnHold,
            ProjectStatus::Completed,
            ProjectStatus::Cancelled,
            ProjectStatus::Archived,
        ]
        .into_iter()
        .filter(|status| current_status.can_transition_to(*status))
        .map(|status| Self::form_choice(status.as_str(), status.as_str()))
        .collect()
    }

    fn build_form(&mut self, mode: FormMode) -> Option<FormState> {
        match mode {
            FormMode::NewProject => Some(FormState {
                title: "New project".into(),
                hint: "Create a project without leaving the TUI".into(),
                mode: FormMode::NewProject,
                fields: vec![
                    Self::text_field("name", "Name", "", true, "required"),
                    Self::text_field("objective", "Objective", "", false, "optional"),
                    Self::choice_field(
                        "workflow",
                        "Workflow",
                        "basic",
                        self.workflow_choices(),
                        true,
                    ),
                ],
                focused: 0,
                error: None,
                dirty: false,
            }),
            FormMode::NewTask { project_id } => Some(FormState {
                title: "New task".into(),
                hint: "Create a task in the selected project or inbox".into(),
                mode: FormMode::NewTask {
                    project_id: project_id.clone(),
                },
                fields: vec![
                    Self::text_field("title", "Title", "", true, "required"),
                    Self::choice_field(
                        "project",
                        "Project",
                        project_id.unwrap_or_default(),
                        self.project_choices(true),
                        false,
                    ),
                    Self::text_field("milestone", "Milestone", "", false, "optional ID"),
                    Self::integer_field("earliest", "Earliest start", "", false, "epoch ms"),
                    Self::integer_field("estimate", "Estimate", "", false, "minutes"),
                    Self::integer_field("deadline", "Deadline", "", false, "epoch ms"),
                ],
                focused: 0,
                error: None,
                dirty: false,
            }),
            FormMode::EditTask { task_id } => {
                let tasks = self.client.list_tasks().ok()?;
                let task = tasks.iter().find(|task| task.id == task_id)?;
                Some(FormState {
                    title: format!("Edit task · {}", task.title),
                    hint: "Rename and set supported planning fields".into(),
                    mode: FormMode::EditTask {
                        task_id: task_id.clone(),
                    },
                    fields: vec![
                        Self::text_field("title", "Title", task.title.clone(), true, "required"),
                        Self::choice_field(
                            "project",
                            "Project",
                            task.project_id.clone().unwrap_or_default(),
                            self.project_choices(true),
                            false,
                        ),
                        Self::text_field(
                            "milestone",
                            "Milestone",
                            task.milestone_id.clone().unwrap_or_default(),
                            false,
                            "optional ID",
                        ),
                        Self::integer_field(
                            "estimate",
                            "Estimate",
                            task.estimated_minutes
                                .map(|value| value.to_string())
                                .unwrap_or_default(),
                            false,
                            "minutes",
                        ),
                        Self::integer_field(
                            "earliest",
                            "Earliest start",
                            task.earliest_start_at_ms
                                .map(|value| value.to_string())
                                .unwrap_or_default(),
                            false,
                            "epoch ms",
                        ),
                        Self::integer_field(
                            "deadline",
                            "Deadline",
                            task.deadline_at_ms
                                .map(|value| value.to_string())
                                .unwrap_or_default(),
                            false,
                            "epoch ms",
                        ),
                        Self::multiline_field(
                            "description",
                            "Description",
                            task.description.clone(),
                            false,
                            "Markdown text",
                        ),
                        Self::choice_field(
                            "priority",
                            "Priority",
                            task.priority.clone().unwrap_or_default(),
                            vec![
                                Self::form_choice("", "(unset)"),
                                Self::form_choice("low", "low"),
                                Self::form_choice("medium", "medium"),
                                Self::form_choice("high", "high"),
                                Self::form_choice("urgent", "urgent"),
                            ],
                            false,
                        ),
                        Self::text_field(
                            "labels",
                            "Labels",
                            task.labels.join(", "),
                            false,
                            "comma separated",
                        ),
                    ],
                    focused: 0,
                    error: None,
                    dirty: false,
                })
            }
            FormMode::EditTaskDetails { task_id } => {
                let task = self.task_dto(&task_id)?;
                let priority = task.priority.clone().unwrap_or_default();
                Some(FormState {
                    title: format!("Edit details · {}", task.title),
                    hint: "Markdown description, priority and labels. Ctrl+S saves; Esc cancels."
                        .into(),
                    mode: FormMode::EditTaskDetails {
                        task_id: task_id.clone(),
                    },
                    fields: vec![
                        Self::multiline_field(
                            "description",
                            "Description",
                            task.description.clone(),
                            false,
                            "Markdown text",
                        ),
                        Self::choice_field(
                            "priority",
                            "Priority",
                            priority,
                            vec![
                                Self::form_choice("", "(unset)"),
                                Self::form_choice("low", "low"),
                                Self::form_choice("medium", "medium"),
                                Self::form_choice("high", "high"),
                                Self::form_choice("urgent", "urgent"),
                            ],
                            false,
                        ),
                        Self::text_field(
                            "labels",
                            "Labels",
                            task.labels.join(", "),
                            false,
                            "comma separated",
                        ),
                    ],
                    focused: 0,
                    error: None,
                    dirty: false,
                })
            }
            FormMode::AddCriterion { task_id } => {
                let task = self.task_dto(&task_id)?;
                Some(FormState {
                    title: format!("Add acceptance criterion · {}", task.title),
                    hint: "Ordered and independently checkable".into(),
                    mode: FormMode::AddCriterion {
                        task_id: task_id.clone(),
                    },
                    fields: vec![Self::text_field("text", "Criterion", "", true, "required")],
                    focused: 0,
                    error: None,
                    dirty: false,
                })
            }
            FormMode::EditCriterion {
                task_id,
                criterion_id,
            } => {
                let items = self.client.acceptance_criteria(&task_id).ok()?;
                let item = items.iter().find(|item| item.id == criterion_id)?;
                Some(FormState {
                    title: "Edit acceptance criterion".into(),
                    hint: "Update the criterion text".into(),
                    mode: FormMode::EditCriterion {
                        task_id: task_id.clone(),
                        criterion_id: criterion_id.clone(),
                    },
                    fields: vec![Self::text_field(
                        "text",
                        "Criterion",
                        item.text.clone(),
                        true,
                        "required",
                    )],
                    focused: 0,
                    error: None,
                    dirty: false,
                })
            }
            FormMode::AttachFile {
                owner_kind,
                owner_id,
            } => Some(FormState {
                title: "Attach file".into(),
                hint: "Enter an existing file path; content is copied into managed storage".into(),
                mode: FormMode::AttachFile {
                    owner_kind: owner_kind.clone(),
                    owner_id: owner_id.clone(),
                },
                fields: vec![
                    Self::text_field("path", "File path", "", true, "required"),
                    Self::text_field("description", "Caption", "", false, "optional"),
                ],
                focused: 0,
                error: None,
                dirty: false,
            }),
            FormMode::ExportAttachment { attachment_id } => Some(FormState {
                title: "Export attachment".into(),
                hint: "The destination must not already exist".into(),
                mode: FormMode::ExportAttachment {
                    attachment_id: attachment_id.clone(),
                },
                fields: vec![Self::text_field(
                    "destination",
                    "Destination path",
                    "",
                    true,
                    "required",
                )],
                focused: 0,
                error: None,
                dirty: false,
            }),
            FormMode::ChangeWorkflow { project_id } => Some(FormState {
                title: "Change workflow".into(),
                hint: "Basic is the default; the pinned version is preserved for existing bindings"
                    .into(),
                mode: FormMode::ChangeWorkflow {
                    project_id: project_id.clone(),
                },
                fields: vec![
                    Self::choice_field(
                        "workflow",
                        "Workflow",
                        "basic",
                        self.workflow_choices(),
                        true,
                    ),
                    Self::integer_field("version", "Version", "", false, "optional"),
                ],
                focused: 0,
                error: None,
                dirty: false,
            }),
            FormMode::AddDependency { task_id } => {
                let tasks = self
                    .workspace
                    .as_ref()
                    .map(|workspace| workspace.tasks.clone())
                    .filter(|tasks| !tasks.is_empty())
                    .or_else(|| {
                        self.project
                            .as_ref()
                            .map(|project| project.data.tasks.clone())
                    })
                    .unwrap_or_default();
                let choices: Vec<FormChoice> = tasks
                    .iter()
                    .filter(|task| task.id != task_id)
                    .map(|task| Self::form_choice(task.id.clone(), task.title.clone()))
                    .collect();
                if choices.is_empty() {
                    self.notify(
                        "No other task is available as a prerequisite",
                        StatusKind::Warning,
                    );
                    return None;
                }
                Some(FormState {
                    title: "Add task prerequisite".into(),
                    hint: "The authority rejects self, missing and cyclic edges".into(),
                    mode: FormMode::AddDependency {
                        task_id: task_id.clone(),
                    },
                    fields: vec![Self::choice_field(
                        "prerequisite",
                        "Prerequisite",
                        choices[0].value.clone(),
                        choices,
                        true,
                    )],
                    focused: 0,
                    error: None,
                    dirty: false,
                })
            }
            FormMode::RemoveDependency { task_id } => {
                let task = self
                    .workspace
                    .as_ref()
                    .and_then(|workspace| workspace.tasks.iter().find(|task| task.id == task_id))
                    .cloned()
                    .or_else(|| {
                        self.project.as_ref().and_then(|project| {
                            project
                                .data
                                .tasks
                                .iter()
                                .find(|task| task.id == task_id)
                                .cloned()
                        })
                    })?;
                if task.prerequisite_ids.is_empty() {
                    self.notify("This task has no prerequisites", StatusKind::Warning);
                    return None;
                }
                let choices = task
                    .prerequisite_ids
                    .iter()
                    .map(|id| Self::form_choice(id.clone(), id.clone()))
                    .collect();
                Some(FormState {
                    title: "Remove task prerequisite".into(),
                    hint: "Removal is revision-checked by the authority".into(),
                    mode: FormMode::RemoveDependency { task_id },
                    fields: vec![Self::choice_field(
                        "prerequisite",
                        "Prerequisite",
                        "",
                        choices,
                        true,
                    )],
                    focused: 0,
                    error: None,
                    dirty: false,
                })
            }
            FormMode::EditProject { project_id } => {
                let project = self
                    .project_details
                    .iter()
                    .find(|project| project.id == project_id)?;
                Some(FormState {
                    title: format!("Project settings · {}", project.name),
                    hint: "Change supported project settings; criteria and signals are preserved"
                        .into(),
                    mode: FormMode::EditProject {
                        project_id: project_id.clone(),
                    },
                    fields: vec![
                        Self::choice_field(
                            "status",
                            "Status",
                            project.status.clone(),
                            Self::project_status_choices(&project.status),
                            true,
                        ),
                        Self::choice_field(
                            "mode",
                            "Mode",
                            project.mode.clone(),
                            [
                                "standard",
                                "learning",
                                "habit_driven",
                                "metric_target",
                                "maintenance",
                                "custom",
                            ]
                            .into_iter()
                            .map(|mode| Self::form_choice(mode, mode))
                            .collect(),
                            true,
                        ),
                        Self::text_field(
                            "objective",
                            "Objective",
                            project.objective.clone().unwrap_or_default(),
                            false,
                            "optional",
                        ),
                    ],
                    focused: 0,
                    error: None,
                    dirty: false,
                })
            }
            FormMode::NewWorkItemKind { project_id } => {
                if self.client.is_connected() {
                    self.notify(
                        "Work-item creation requires the connected command surface",
                        StatusKind::Warning,
                    );
                    return None;
                }
                let blueprint = match self.client.project_blueprint_definition(&project_id) {
                    Ok(Some(blueprint)) => blueprint,
                    Ok(None) => {
                        self.notify("This project has no pinned blueprint", StatusKind::Warning);
                        return None;
                    }
                    Err(error) => {
                        self.notify(
                            format!("Blueprint unavailable: {}", error.message),
                            StatusKind::Warning,
                        );
                        return None;
                    }
                };
                let choices = Self::work_item_kind_choices(&blueprint);
                if choices.is_empty() {
                    self.notify(
                        "This blueprint declares no work-item kinds",
                        StatusKind::Warning,
                    );
                    return None;
                }
                Some(FormState {
                    title: "New work item · choose kind".into(),
                    hint: "The next form shows the type's declared fields".into(),
                    mode: FormMode::NewWorkItemKind {
                        project_id: project_id.clone(),
                    },
                    fields: vec![Self::choice_field(
                        "kind",
                        "Kind",
                        choices[0].value.clone(),
                        choices,
                        true,
                    )],
                    focused: 0,
                    error: None,
                    dirty: false,
                })
            }
            FormMode::NewWorkItem {
                project_id,
                blueprint,
                kind,
            } => Some(FormState {
                title: format!("New {kind}"),
                hint: "Fields are generated from the pinned blueprint".into(),
                mode: FormMode::NewWorkItem {
                    project_id: project_id.clone(),
                    blueprint: blueprint.clone(),
                    kind: kind.clone(),
                },
                fields: Self::work_item_fields(&blueprint, &kind)?,
                focused: 0,
                error: None,
                dirty: false,
            }),
            FormMode::EditWorkItem {
                item_id,
                revision,
                blueprint,
                kind,
            } => {
                let item = self
                    .client
                    .list_blueprint_work_items(
                        self.project
                            .as_ref()
                            .map(|project| project.id.as_str())
                            .unwrap_or(""),
                    )
                    .ok()?
                    .into_iter()
                    .find(|item| item.id == item_id)?;
                let mut fields = Self::work_item_fields(&blueprint, &kind)?;
                for field in &mut fields {
                    if let Some(value) = item.properties.get(&field.key) {
                        field.value = match value {
                            serde_json::Value::String(text) => text.clone(),
                            other => other.to_string(),
                        };
                    }
                    field.initial = field.value.clone();
                }
                Some(FormState {
                    title: format!("Edit work item · {}", item_id),
                    hint: "Only declared properties are editable".into(),
                    mode: FormMode::EditWorkItem {
                        item_id,
                        revision,
                        blueprint,
                        kind,
                    },
                    fields,
                    focused: 0,
                    error: None,
                    dirty: false,
                })
            }
            FormMode::NewKnowledge => Some(FormState {
                title: "New knowledge note".into(),
                hint: "Capture a Markdown note".into(),
                mode: FormMode::NewKnowledge,
                fields: vec![
                    Self::text_field("title", "Title", "", true, "required"),
                    Self::text_field("body", "Body", "", false, "Markdown"),
                ],
                focused: 0,
                error: None,
                dirty: false,
            }),
            FormMode::InstallBlueprint => Some(FormState {
                title: "Install bundled blueprint".into(),
                hint: "Installation publishes a version; it does not bind existing projects".into(),
                mode: FormMode::InstallBlueprint,
                fields: vec![Self::choice_field(
                    "blueprint",
                    "Blueprint",
                    orbit_blueprint::bundled::BUNDLED_BLUEPRINTS[0].id,
                    orbit_blueprint::bundled::BUNDLED_BLUEPRINTS
                        .iter()
                        .map(|descriptor| Self::form_choice(descriptor.id, descriptor.name))
                        .collect(),
                    true,
                )],
                focused: 0,
                error: None,
                dirty: false,
            }),
            FormMode::BindBlueprint { project_id } => {
                let choices = self.blueprint_version_choices();
                if choices.is_empty() {
                    self.notify(
                        "No published blueprint versions are available",
                        StatusKind::Warning,
                    );
                    return None;
                }
                Some(FormState {
                    title: "Bind blueprint version".into(),
                    hint: "Binding is explicit and pins the selected version".into(),
                    mode: FormMode::BindBlueprint {
                        project_id: project_id.clone(),
                    },
                    fields: vec![Self::choice_field(
                        "version",
                        "Blueprint",
                        choices[0].value.clone(),
                        choices,
                        true,
                    )],
                    focused: 0,
                    error: None,
                    dirty: false,
                })
            }
            FormMode::MetricTarget { metric_id } => {
                let choices = self.metric_choices(true);
                let selected = metric_id.unwrap_or_else(|| {
                    choices
                        .get(1)
                        .map(|choice| choice.value.clone())
                        .unwrap_or_default()
                });
                let metric_field = if choices.len() <= 1 {
                    Self::text_field("metric", "Metric ID", selected.clone(), true, "metric ID")
                } else {
                    Self::choice_field("metric", "Metric", selected.clone(), choices, true)
                };
                Some(FormState {
                    title: "Metric target".into(),
                    hint: "Baseline and target are integers".into(),
                    mode: FormMode::MetricTarget {
                        metric_id: Some(selected),
                    },
                    fields: vec![
                        metric_field,
                        Self::integer_field("baseline", "Baseline", "", true, "integer"),
                        Self::integer_field("target", "Target", "", true, "integer"),
                        Self::choice_field(
                            "direction",
                            "Direction",
                            "at_least",
                            ["at_least", "at_most"]
                                .into_iter()
                                .map(|direction| Self::form_choice(direction, direction))
                                .collect(),
                            true,
                        ),
                    ],
                    focused: 0,
                    error: None,
                    dirty: false,
                })
            }
            FormMode::MetricObservation { metric_id } => {
                let choices = self.metric_choices(true);
                let selected = metric_id.unwrap_or_else(|| {
                    choices
                        .get(1)
                        .map(|choice| choice.value.clone())
                        .unwrap_or_default()
                });
                let metric_field = if choices.len() <= 1 {
                    Self::text_field("metric", "Metric ID", selected.clone(), true, "metric ID")
                } else {
                    Self::choice_field("metric", "Metric", selected.clone(), choices, true)
                };
                Some(FormState {
                    title: "Record metric observation".into(),
                    hint: "The observation is appended through the authority".into(),
                    mode: FormMode::MetricObservation {
                        metric_id: Some(selected),
                    },
                    fields: vec![
                        metric_field,
                        Self::integer_field("value", "Value", "", true, "integer"),
                    ],
                    focused: 0,
                    error: None,
                    dirty: false,
                })
            }
            FormMode::BindEvidence { project_id, unbind } => Some(FormState {
                title: if unbind {
                    "Unbind project evidence"
                } else {
                    "Bind project evidence"
                }
                .into(),
                hint: "Use an existing habit or metric ID".into(),
                mode: FormMode::BindEvidence { project_id, unbind },
                fields: vec![
                    Self::choice_field(
                        "kind",
                        "Kind",
                        "habit",
                        ["habit", "metric"]
                            .into_iter()
                            .map(|kind| Self::form_choice(kind, kind))
                            .collect(),
                        true,
                    ),
                    Self::text_field("id", "Evidence ID", "", true, "habit or metric ID"),
                ],
                focused: 0,
                error: None,
                dirty: false,
            }),
        }
    }

    pub fn form_value<'a>(form: &'a FormState, key: &str) -> &'a str {
        form.fields
            .iter()
            .find(|field| field.key == key)
            .map(|field| field.value.as_str())
            .unwrap_or("")
    }

    fn form_int(form: &FormState, key: &str, required: bool) -> Result<Option<i64>, String> {
        let value = Self::form_value(form, key).trim();
        if value.is_empty() {
            return if required {
                Err(format!("{key} is required"))
            } else {
                Ok(None)
            };
        }
        value
            .parse::<i64>()
            .map(Some)
            .map_err(|_| format!("{key} must be an integer"))
    }

    fn form_u32(form: &FormState, key: &str) -> Result<Option<u32>, String> {
        match Self::form_int(form, key, false)? {
            Some(value) if (1..=525_600).contains(&value) => Ok(Some(value as u32)),
            Some(_) => Err(format!("{key} must be between 1 and 525600")),
            None => Ok(None),
        }
    }

    fn form_error(&mut self, mut form: FormState, error: impl Into<String>) {
        form.error = Some(error.into());
        self.modal = Some(Modal::Form(form));
    }

    pub fn submit_form(&mut self, form: FormState) {
        let key = format!("form:{:?}", form.mode);
        if let Some((last, at)) = &self.last_submission
            && last == &key
            && at.elapsed() < Duration::from_millis(250)
        {
            self.notify("Duplicate form submission ignored", StatusKind::Warning);
            self.modal = Some(Modal::Form(form));
            return;
        }
        self.last_submission = Some((key, Instant::now()));
        let mut validation_error = None;
        for field in &form.fields {
            if field.required && field.value.trim().is_empty() {
                validation_error = Some(format!("{} is required", field.label));
                break;
            }
            if matches!(field.kind, FormFieldKind::Integer)
                && !field.value.trim().is_empty()
                && field.value.trim().parse::<i64>().is_err()
            {
                validation_error = Some(format!("{} must be an integer", field.label));
                break;
            }
            if let FormFieldKind::Choice(choices) = &field.kind
                && (!field.value.trim().is_empty() || field.required)
                && !choices.iter().any(|choice| choice.value == field.value)
            {
                validation_error = Some(format!("{} is not a valid choice", field.label));
                break;
            }
        }
        if let Some(error) = validation_error {
            self.form_error(form, error);
            return;
        }
        let result = match form.mode.clone() {
            FormMode::NewProject => self.submit_new_project(&form),
            FormMode::NewTask { .. } => self.submit_new_task(&form),
            FormMode::EditTask { task_id } => self.submit_edit_task(&form, &task_id),
            FormMode::EditTaskDetails { task_id } => self.submit_edit_task_details(&form, &task_id),
            FormMode::AddCriterion { task_id } => self.submit_add_criterion(&form, &task_id),
            FormMode::EditCriterion {
                task_id,
                criterion_id,
            } => self.submit_edit_criterion(&form, &task_id, &criterion_id),
            FormMode::AttachFile {
                owner_kind,
                owner_id,
            } => self.submit_attach_file(&form, &owner_kind, &owner_id),
            FormMode::ExportAttachment { attachment_id } => {
                self.submit_export_attachment(&form, &attachment_id)
            }
            FormMode::ChangeWorkflow { project_id } => {
                self.submit_change_workflow(&form, &project_id)
            }
            FormMode::AddDependency { task_id } => self.submit_add_dependency(&form, &task_id),
            FormMode::RemoveDependency { task_id } => {
                self.submit_remove_dependency(&form, &task_id)
            }
            FormMode::EditProject { project_id } => self.submit_edit_project(&form, &project_id),
            FormMode::NewWorkItemKind { project_id } => {
                let Some(blueprint) = self
                    .client
                    .project_blueprint_definition(&project_id)
                    .ok()
                    .flatten()
                else {
                    self.form_error(form, "The project has no pinned blueprint");
                    return;
                };
                let kind = Self::form_value(&form, "kind").to_owned();
                if let Some(next) = self.build_form(FormMode::NewWorkItem {
                    project_id,
                    blueprint,
                    kind,
                }) {
                    self.modal = Some(Modal::Form(next));
                }
                return;
            }
            FormMode::NewWorkItem {
                project_id,
                blueprint,
                kind,
            } => self.submit_new_work_item(&form, &project_id, &blueprint, &kind),
            FormMode::EditWorkItem {
                item_id,
                revision,
                blueprint,
                kind,
            } => self.submit_edit_work_item(&form, &item_id, revision, &blueprint, &kind),
            FormMode::NewKnowledge => self.submit_new_knowledge(&form),
            FormMode::InstallBlueprint => self.submit_install_blueprint(&form),
            FormMode::BindBlueprint { project_id } => {
                self.submit_bind_blueprint(&form, &project_id)
            }
            FormMode::MetricTarget { .. } => self.submit_metric_target(&form),
            FormMode::MetricObservation { .. } => self.submit_metric_observation(&form),
            FormMode::BindEvidence { project_id, unbind } => {
                self.submit_bind_evidence(&form, &project_id, unbind)
            }
        };
        match result {
            Ok(()) => {
                if self.status.kind != StatusKind::Pending {
                    self.refresh_after_mutation();
                }
                self.reopen_after_mutation(&form.mode);
            }
            Err(error) => self.form_error(form, error),
        }
    }

    /// Keep list-style modals in sync after a successful form submission.
    fn reopen_after_mutation(&mut self, mode: &FormMode) {
        match mode {
            FormMode::AddCriterion { task_id } | FormMode::EditCriterion { task_id, .. } => {
                let title = self
                    .task_dto(task_id)
                    .map(|task| task.title)
                    .unwrap_or_else(|| task_id.clone());
                self.open_criteria(task_id.clone(), title);
            }
            FormMode::AttachFile {
                owner_kind,
                owner_id,
            } => {
                let title = format!("Attachments · {owner_id}");
                self.open_attachments(owner_kind.clone(), owner_id.clone(), title);
            }
            _ => {}
        }
    }

    fn submit_edit_task_details(&mut self, form: &FormState, task_id: &str) -> Result<(), String> {
        let description = Self::form_value(form, "description").to_owned();
        let priority_raw = Self::form_value(form, "priority").trim().to_owned();
        let priority = (!priority_raw.is_empty()).then_some(priority_raw);
        let labels: Vec<String> = Self::form_value(form, "labels")
            .split(',')
            .map(str::trim)
            .filter(|label| !label.is_empty())
            .map(str::to_owned)
            .collect();
        self.client
            .update_task_details(task_id.to_owned(), description, priority, labels)
            .map_err(|error| error.message)?;
        self.notify("Task details saved", StatusKind::Success);
        Ok(())
    }

    fn submit_add_criterion(&mut self, form: &FormState, task_id: &str) -> Result<(), String> {
        let text = Self::form_value(form, "text").trim().to_owned();
        self.client
            .add_acceptance_criterion(task_id.to_owned(), text)
            .map_err(|error| error.message)?;
        self.notify("Acceptance criterion added", StatusKind::Success);
        Ok(())
    }

    fn submit_edit_criterion(
        &mut self,
        form: &FormState,
        task_id: &str,
        criterion_id: &str,
    ) -> Result<(), String> {
        let text = Self::form_value(form, "text").trim().to_owned();
        self.client
            .set_acceptance_criterion_text(task_id.to_owned(), criterion_id.to_owned(), text)
            .map_err(|error| error.message)?;
        self.notify("Acceptance criterion updated", StatusKind::Success);
        Ok(())
    }

    fn submit_attach_file(
        &mut self,
        form: &FormState,
        owner_kind: &str,
        owner_id: &str,
    ) -> Result<(), String> {
        let path = Self::form_value(form, "path").trim().to_owned();
        if path.is_empty() {
            return Err("File path is required".into());
        }
        let description = Self::form_value(form, "description").trim();
        let description = (!description.is_empty()).then(|| description.to_owned());
        let attachment = self
            .client
            .import_attachment(
                owner_kind.to_owned(),
                owner_id.to_owned(),
                path,
                description,
            )
            .map_err(|error| error.message)?;
        self.notify(
            format!("Attached {}", attachment.filename),
            StatusKind::Success,
        );
        Ok(())
    }

    fn submit_export_attachment(
        &mut self,
        form: &FormState,
        attachment_id: &str,
    ) -> Result<(), String> {
        let destination = Self::form_value(form, "destination").trim().to_owned();
        if destination.is_empty() {
            return Err("Destination path is required".into());
        }
        let attachment = self
            .client
            .export_attachment(attachment_id, destination.clone())
            .map_err(|error| error.message)?;
        self.notify(
            format!("Exported {} to {destination}", attachment.filename),
            StatusKind::Success,
        );
        Ok(())
    }

    fn submit_change_workflow(&mut self, form: &FormState, project_id: &str) -> Result<(), String> {
        let workflow = Self::form_value(form, "workflow").trim().to_owned();
        if workflow.is_empty() {
            return Err("Select a workflow".into());
        }
        let version = Self::form_int(form, "version", false)?
            .map(|value| u32::try_from(value).map_err(|_| "Version must be positive".to_owned()))
            .transpose()?;
        self.client
            .apply_workflow(project_id.to_owned(), workflow.clone(), version)
            .map_err(|error| error.message)?;
        let label = self.workflow_label(&workflow, version.unwrap_or(1));
        self.notify(format!("Workflow applied: {label}"), StatusKind::Success);
        Ok(())
    }

    fn default_project_signals(mode: &str) -> Vec<String> {
        orbit_domain::ProjectMode::parse(mode)
            .map(|mode| {
                mode.default_signals()
                    .into_iter()
                    .map(|signal| signal.as_str().to_owned())
                    .collect()
            })
            .unwrap_or_default()
    }

    fn submit_new_project(&mut self, form: &FormState) -> Result<(), String> {
        let name = Self::form_value(form, "name").trim().to_owned();
        let workflow = Self::form_value(form, "workflow").trim().to_owned();
        if self.client.is_connected() {
            if !Self::form_value(form, "objective").trim().is_empty() {
                return Err(
                    "connected mode cannot queue project objectives yet; create the project, then use the connected command surface"
                        .into(),
                );
            }
            if !workflow.is_empty() && workflow != "basic" {
                return Err(
                    "connected mode uses the server default workflow (Basic); change it after the project is accepted"
                        .into(),
                );
            }
        }
        let project = self
            .client
            .create_project(CreateProjectRequest {
                id: None,
                name: name.clone(),
            })
            .map_err(|error| error.message)?;
        if self.client.take_pending() {
            self.refresh_workspace();
            self.notify(
                format!("Project {name} is pending; it is not accepted yet"),
                StatusKind::Pending,
            );
            return Ok(());
        }
        let objective = Self::form_value(form, "objective").trim();
        let mut warning = None;
        if !objective.is_empty()
            && let Err(error) =
                self.client
                    .configure_project_strategy(ConfigureProjectStrategyRequest {
                        id: project.id.clone(),
                        mode: "standard".into(),
                        objective: Some(objective.to_owned()),
                        progress_signals: Self::default_project_signals("standard"),
                        success_criteria: Vec::new(),
                    })
        {
            warning = Some(format!("objective update failed: {}", error.message));
        }
        // New projects already carry the Basic binding from the authority; only
        // an explicit different workflow needs an extra pinned change.
        if !workflow.is_empty()
            && workflow != "basic"
            && let Err(error) =
                self.client
                    .apply_workflow(project.id.clone(), workflow.clone(), None)
        {
            warning = Some(format!("applying the workflow failed: {}", error.message));
        }
        let id = project.id.clone();
        self.refresh_workspace();
        self.open_project(&id);
        if let Some(warning) = warning {
            self.notify(
                format!("Created project {name}; {warning}"),
                StatusKind::Warning,
            );
        } else {
            self.notify(format!("Created project {name}"), StatusKind::Success);
        }
        Ok(())
    }

    fn submit_new_task(&mut self, form: &FormState) -> Result<(), String> {
        let title = Self::form_value(form, "title").trim().to_owned();
        let project = Self::form_value(form, "project").to_owned();
        let project_id = (!project.is_empty()).then_some(project);
        let milestone = Self::form_value(form, "milestone").trim();
        let estimate = Self::form_u32(form, "estimate")?;
        let earliest = Self::form_int(form, "earliest", false)?;
        let deadline = Self::form_int(form, "deadline", false)?;
        if earliest.is_some_and(|value| value < 0) || deadline.is_some_and(|value| value < 0) {
            return Err("planning timestamps must be non-negative".into());
        }
        if let (Some(earliest), Some(deadline)) = (earliest, deadline)
            && earliest > deadline
        {
            return Err("earliest start must not be after the deadline".into());
        }
        let task = self
            .client
            .create_task(CreateTaskRequest {
                id: None,
                title,
                project_id,
                milestone_id: (!milestone.is_empty()).then(|| milestone.to_owned()),
            })
            .map_err(|error| error.message)?;
        if self.client.take_pending() {
            if estimate.is_some() || earliest.is_some() || deadline.is_some() {
                if let Err(error) = self.client.set_task_planning(SetTaskPlanningRequest {
                    id: task.id.clone(),
                    estimated_minutes: estimate,
                    earliest_start_at_ms: earliest,
                    deadline_at_ms: deadline,
                }) {
                    self.refresh_after_mutation();
                    self.notify(
                        format!(
                            "Task {} is pending but planning was rejected: {}",
                            task.id, error.message
                        ),
                        StatusKind::Warning,
                    );
                    return Ok(());
                }
                let _ = self.client.take_pending();
            }
            self.refresh_after_mutation();
            self.notify(
                format!("Task {} is pending; it is not accepted yet", task.id),
                StatusKind::Pending,
            );
            return Ok(());
        }
        if (estimate.is_some() || earliest.is_some() || deadline.is_some())
            && let Err(error) = self.client.set_task_planning(SetTaskPlanningRequest {
                id: task.id.clone(),
                estimated_minutes: estimate,
                earliest_start_at_ms: earliest,
                deadline_at_ms: deadline,
            })
        {
            self.notify(
                format!(
                    "Created task {} but planning was rejected: {}",
                    task.id, error.message
                ),
                StatusKind::Warning,
            );
            return Ok(());
        }
        self.notify(format!("Created task {}", task.id), StatusKind::Success);
        Ok(())
    }

    fn submit_edit_task(&mut self, form: &FormState, task_id: &str) -> Result<(), String> {
        let title = Self::form_value(form, "title").trim().to_owned();
        let current = self
            .client
            .list_tasks()
            .map_err(|error| error.message)?
            .into_iter()
            .find(|task| task.id == task_id)
            .ok_or_else(|| "task no longer exists".to_owned())?;
        let estimate = Self::form_u32(form, "estimate")?;
        let earliest = Self::form_int(form, "earliest", false)?;
        let deadline = Self::form_int(form, "deadline", false)?;
        if earliest.is_some_and(|value| value < 0) || deadline.is_some_and(|value| value < 0) {
            return Err("planning timestamps must be non-negative".into());
        }
        if let (Some(earliest), Some(deadline)) = (earliest, deadline)
            && earliest > deadline
        {
            return Err("earliest start must not be after the deadline".into());
        }
        let project = Self::form_value(form, "project");
        let project_id = (!project.is_empty()).then(|| project.to_owned());
        let milestone = Self::form_value(form, "milestone").trim();
        let milestone_id = (!milestone.is_empty()).then(|| milestone.to_owned());
        if project_id == current.project_id
            && let Some(milestone_id) = &milestone_id
            && let Some(project) = &self.project
            && project.id == current.project_id.as_deref().unwrap_or("")
            && !project
                .data
                .milestones
                .iter()
                .any(|milestone| &milestone.id == milestone_id)
        {
            return Err(format!("milestone {milestone_id} is not in this project"));
        }
        if project_id != current.project_id {
            if let Some(project_id) = &project_id
                && !self
                    .projects
                    .iter()
                    .any(|project| &project.id == project_id)
            {
                return Err(format!("project {project_id} is not available"));
            }
            if current.milestone_id.is_some() {
                self.client
                    .assign_task_milestone(task_id.to_owned(), None)
                    .map_err(|error| error.message)?;
            }
            self.client
                .assign_task_project(task_id.to_owned(), project_id)
                .map_err(|error| error.message)?;
        }
        if milestone_id != current.milestone_id {
            self.client
                .assign_task_milestone(task_id.to_owned(), milestone_id)
                .map_err(|error| error.message)?;
        }
        if current.title != title {
            self.client
                .rename_task(task_id.to_owned(), title)
                .map_err(|error| error.message)?;
        }
        if estimate != current.estimated_minutes
            || earliest != current.earliest_start_at_ms
            || deadline != current.deadline_at_ms
        {
            self.client
                .set_task_planning(SetTaskPlanningRequest {
                    id: task_id.to_owned(),
                    estimated_minutes: estimate,
                    earliest_start_at_ms: earliest,
                    deadline_at_ms: deadline,
                })
                .map_err(|error| error.message)?;
        }
        let description = Self::form_value(form, "description").to_owned();
        let priority_raw = Self::form_value(form, "priority").trim().to_owned();
        let priority = (!priority_raw.is_empty()).then_some(priority_raw);
        let labels: Vec<String> = Self::form_value(form, "labels")
            .split(',')
            .map(str::trim)
            .filter(|label| !label.is_empty())
            .map(str::to_owned)
            .collect();
        self.client
            .update_task_details(task_id.to_owned(), description, priority, labels)
            .map_err(|error| error.message)?;
        let (message, kind) = self.mutation_status("Task updated");
        self.notify(message, kind);
        Ok(())
    }

    fn submit_add_dependency(&mut self, form: &FormState, task_id: &str) -> Result<(), String> {
        let prerequisite = Self::form_value(form, "prerequisite").to_owned();
        self.client
            .add_task_prerequisite(task_id.to_owned(), prerequisite)
            .map_err(|error| error.message)?;
        let (message, kind) = self.mutation_status("Task prerequisite added");
        self.notify(message, kind);
        Ok(())
    }

    fn submit_remove_dependency(&mut self, form: &FormState, task_id: &str) -> Result<(), String> {
        let prerequisite = Self::form_value(form, "prerequisite").to_owned();
        self.client
            .remove_task_prerequisite(task_id.to_owned(), prerequisite)
            .map_err(|error| error.message)?;
        let (message, kind) = self.mutation_status("Task prerequisite removed");
        self.notify(message, kind);
        Ok(())
    }

    fn submit_edit_project(&mut self, form: &FormState, project_id: &str) -> Result<(), String> {
        let project = self
            .project_details
            .iter()
            .find(|project| project.id == project_id)
            .cloned()
            .ok_or_else(|| "project not found".to_owned())?;
        let status = Self::form_value(form, "status").to_owned();
        let mode = Self::form_value(form, "mode").to_owned();
        let objective = {
            let objective = Self::form_value(form, "objective").trim();
            (!objective.is_empty()).then(|| objective.to_owned())
        };
        let mode_changed = mode != project.mode;
        let objective_changed = objective != project.objective;
        if mode_changed || objective_changed {
            let signals = if mode_changed {
                Self::default_project_signals(&mode)
            } else {
                project.progress_signals.clone()
            };
            self.client
                .configure_project_strategy(ConfigureProjectStrategyRequest {
                    id: project_id.to_owned(),
                    mode,
                    objective,
                    progress_signals: signals,
                    success_criteria: project
                        .success_criteria
                        .iter()
                        .map(|criterion| ProjectCriterionInputDto {
                            key: criterion.key.clone(),
                            description: criterion.description.clone(),
                            required: criterion.required,
                            status: criterion.status.clone(),
                        })
                        .collect(),
                })
                .map_err(|error| error.message)?;
        }
        let mut warning = None;
        if status != project.status
            && let Err(error) = self.client.set_project_status(SetProjectStatusRequest {
                id: project_id.to_owned(),
                status,
            })
        {
            warning = Some(error.message);
        }
        if let Some(warning) = warning {
            self.notify(
                format!("Project settings partially applied; status update failed: {warning}"),
                StatusKind::Warning,
            );
        } else {
            self.notify("Project updated", StatusKind::Success);
        }
        Ok(())
    }

    fn submit_new_work_item(
        &mut self,
        form: &FormState,
        project_id: &str,
        blueprint: &Blueprint,
        kind: &str,
    ) -> Result<(), String> {
        let definition = blueprint
            .item_kind(kind)
            .map_err(|error| error.to_string())?;
        let mut properties = BTreeMap::new();
        for field in &form.fields {
            let value = field.value.trim();
            if value.is_empty() {
                continue;
            }
            let definition = definition
                .fields
                .iter()
                .find(|candidate| candidate.id == field.key)
                .ok_or_else(|| format!("undeclared field {}", field.key))?;
            let value = match definition.kind {
                FieldKind::Integer => serde_json::Value::from(
                    value
                        .parse::<i64>()
                        .map_err(|_| format!("{} must be an integer", field.label))?,
                ),
                FieldKind::Boolean => serde_json::Value::Bool(value == "true"),
                FieldKind::Text | FieldKind::Choice { .. } => {
                    serde_json::Value::String(value.to_owned())
                }
            };
            properties.insert(field.key.clone(), value);
        }
        self.client
            .create_blueprint_work_item(project_id, kind, properties)
            .map_err(|error| error.message)?;
        self.notify(format!("Created {kind}"), StatusKind::Success);
        Ok(())
    }

    fn submit_edit_work_item(
        &mut self,
        form: &FormState,
        item_id: &str,
        revision: u64,
        blueprint: &Blueprint,
        kind: &str,
    ) -> Result<(), String> {
        let definition = blueprint
            .item_kind(kind)
            .map_err(|error| error.to_string())?;
        let mut changes = BTreeMap::new();
        for field in &form.fields {
            if field.value == field.initial {
                continue;
            }
            let value = field.value.trim();
            let definition = definition
                .fields
                .iter()
                .find(|candidate| candidate.id == field.key)
                .ok_or_else(|| format!("undeclared field {}", field.key))?;
            let value = match definition.kind {
                FieldKind::Integer => serde_json::Value::from(
                    value
                        .parse::<i64>()
                        .map_err(|_| format!("{} must be an integer", field.label))?,
                ),
                FieldKind::Boolean => serde_json::Value::Bool(value == "true"),
                FieldKind::Text | FieldKind::Choice { .. } => {
                    serde_json::Value::String(value.to_owned())
                }
            };
            changes.insert(field.key.clone(), value);
        }
        if changes.is_empty() {
            self.notify("No work-item changes to save", StatusKind::Info);
            return Ok(());
        }
        self.client
            .update_blueprint_work_item(item_id, revision, changes)
            .map_err(|error| error.message)?;
        self.notify("Work item updated", StatusKind::Success);
        Ok(())
    }

    fn submit_new_knowledge(&mut self, form: &FormState) -> Result<(), String> {
        let title = Self::form_value(form, "title").trim().to_owned();
        self.client
            .create_knowledge(CreateKnowledgeRequest {
                id: None,
                kind: "note".into(),
                title,
                body_markdown: Self::form_value(form, "body").to_owned(),
            })
            .map_err(|error| error.message)?;
        self.notify("Knowledge note created", StatusKind::Success);
        Ok(())
    }

    fn submit_install_blueprint(&mut self, form: &FormState) -> Result<(), String> {
        let id = Self::form_value(form, "blueprint");
        self.client
            .install_bundled_blueprint(id)
            .map_err(|error| error.message)?;
        self.notify(
            format!("Installed bundled blueprint {id}"),
            StatusKind::Success,
        );
        Ok(())
    }

    fn submit_bind_blueprint(&mut self, form: &FormState, project_id: &str) -> Result<(), String> {
        let value = Self::form_value(form, "version");
        let (id, version) = value
            .split_once('@')
            .ok_or_else(|| "invalid blueprint version".to_owned())?;
        let version = version
            .parse::<u32>()
            .map_err(|_| "blueprint version must be an integer".to_owned())?;
        self.client
            .bind_project_blueprint(project_id, id, version)
            .map_err(|error| error.message)?;
        self.notify(format!("Bound {id}@{version}"), StatusKind::Success);
        Ok(())
    }

    fn submit_metric_target(&mut self, form: &FormState) -> Result<(), String> {
        let metric_id = Self::form_value(form, "metric").to_owned();
        let baseline = Self::form_int(form, "baseline", true)?.unwrap();
        let target = Self::form_int(form, "target", true)?.unwrap();
        self.client
            .set_metric_target(MetricTargetDto {
                metric_id,
                baseline,
                target,
                direction: Self::form_value(form, "direction").to_owned(),
            })
            .map_err(|error| error.message)?;
        self.notify("Metric target accepted", StatusKind::Success);
        Ok(())
    }

    fn submit_metric_observation(&mut self, form: &FormState) -> Result<(), String> {
        let metric_id = Self::form_value(form, "metric").to_owned();
        let value = Self::form_int(form, "value", true)?.unwrap();
        self.client
            .record_metric_observation(metric_id, now_ms(), value)
            .map_err(|error| error.message)?;
        self.notify("Metric observation accepted", StatusKind::Success);
        Ok(())
    }

    fn submit_bind_evidence(
        &mut self,
        form: &FormState,
        project_id: &str,
        unbind: bool,
    ) -> Result<(), String> {
        self.client
            .change_project_evidence(
                project_id.to_owned(),
                Self::form_value(form, "kind").to_owned(),
                Self::form_value(form, "id").to_owned(),
                !unbind,
            )
            .map_err(|error| error.message)?;
        self.notify(
            if unbind {
                "Evidence unbound"
            } else {
                "Evidence bound"
            },
            StatusKind::Success,
        );
        Ok(())
    }

    pub fn open_search(&mut self) {
        let hits = self.search_hits("");
        self.modal = Some(Modal::Search {
            query: String::new(),
            hits,
            selected: 0,
        });
    }

    pub fn search_hits(&self, query: &str) -> Vec<SearchHit> {
        let needle = query.trim().to_lowercase();
        let mut hits = Vec::new();
        for project in &self.projects {
            if needle.is_empty()
                || project.name.to_lowercase().contains(&needle)
                || project.id.to_lowercase().contains(&needle)
            {
                hits.push(SearchHit {
                    label: project.name.clone(),
                    description: format!("project · {}", project.status),
                    action: PaletteAction::OpenProject(project.id.clone()),
                });
            }
        }
        if let Some(view) = self.current_view() {
            for (component_index, component) in view.components.iter().enumerate() {
                for (item_index, item) in component.selection.iter().enumerate() {
                    if needle.is_empty()
                        || item.title.to_lowercase().contains(&needle)
                        || item.id.to_lowercase().contains(&needle)
                        || item.subtitle.to_lowercase().contains(&needle)
                    {
                        hits.push(SearchHit {
                            label: item.title.clone(),
                            description: format!(
                                "{} · {} · {}",
                                item.kind.as_str(),
                                item.status,
                                item.id
                            ),
                            action: PaletteAction::SelectItem {
                                component: component_index,
                                index: item_index,
                            },
                        });
                    }
                }
            }
        }
        if let Some(workspace) = &self.workspace {
            for note in &workspace.recent_knowledge {
                if needle.is_empty()
                    || note.title.to_lowercase().contains(&needle)
                    || note.id.to_lowercase().contains(&needle)
                {
                    hits.push(SearchHit {
                        label: note.title.clone(),
                        description: format!("knowledge · {}", note.kind),
                        action: PaletteAction::OpenKnowledge(note.id.clone()),
                    });
                }
            }
        }
        hits.truncate(50);
        hits
    }

    pub fn selected_project(&self) -> Option<&orbit_ui::data::ProjectRow> {
        self.projects.get(self.projects_selection)
    }

    pub fn activate_project_selection(&mut self) {
        if let Some(project) = self.selected_project() {
            let id = project.id.clone();
            self.open_project(&id);
        }
    }

    pub fn set_viewport(&mut self, width: u16) {
        self.viewport_width = width;
        if width < crate::theme::WIDE_BREAKPOINT && self.focus == FocusPane::Detail {
            self.focus = FocusPane::Workspace;
        }
    }
}

fn push_unique_action(actions: &mut Vec<AvailableAction>, action: AvailableAction) {
    if !actions
        .iter()
        .any(|existing| existing.id == action.id && existing.payload == action.payload)
    {
        actions.push(action);
    }
}

fn empty_view(id: &str, title: &str) -> ResolvedView {
    ResolvedView {
        id: id.into(),
        title: title.into(),
        description: String::new(),
        layout: orbit_blueprint::presentation::ViewLayout::default(),
        components: vec![ResolvedComponent {
            id: "empty".into(),
            title: title.into(),
            component_type: "text".into(),
            status: LoadStatus::Error("not loaded".into()),
            body: ComponentBody::Text {
                heading: title.into(),
                lines: vec!["No data has been loaded yet. Press r to refresh.".into()],
            },
            selection: Vec::new(),
        }],
        generated: true,
    }
}

pub fn palette_nav(label: &str, description: &str, screen: Screen) -> PaletteEntry {
    PaletteEntry {
        label: label.into(),
        description: description.into(),
        action: PaletteAction::Go(screen),
        enabled: true,
    }
}

pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or_default()
}
