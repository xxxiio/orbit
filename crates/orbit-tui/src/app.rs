//! Project-centric application state. All mutations go through `OrbitClient`;
//! this module owns navigation, focus, prompts and presentation state only.
use crate::planning_ui;
use crate::theme::Theme;
use orbit_client::{
    CreateKnowledgeRequest, CreateProjectRequest, CreateTaskRequest, MetricTargetDto, OrbitClient,
    RecordHabitOccurrenceRequest, SetTaskStatusRequest,
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
pub enum PromptKind {
    NewTask { project_id: Option<String> },
    NewKnowledge,
    NewProject,
    NewWorkItem { project_id: String },
    InstallBlueprint,
    BindBlueprint { project_id: String },
    MetricTarget { metric_id: Option<String> },
    MetricObservation { metric_id: Option<String> },
    BindEvidence { project_id: String, unbind: bool },
}

#[derive(Clone, Debug)]
pub struct PromptState {
    pub title: String,
    pub hint: String,
    pub kind: PromptKind,
    pub value: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
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
    RecordMetric {
        metric_id: String,
        value: i64,
    },
    SetMetricTarget {
        metric_id: String,
        baseline: i64,
        target: i64,
        direction: String,
    },
    BindEvidence {
        project_id: String,
        kind: String,
        evidence_id: String,
        bind: bool,
    },
    ApplyPlan,
}

#[derive(Clone, Debug)]
pub enum Modal {
    Prompt(PromptState),
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
    Prompt(PromptKind),
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

pub struct App {
    pub client: OrbitClient,
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
    pub projects_selection: usize,
    pub project: Option<ProjectState>,
    pub active_component: usize,
    pub selected: usize,
    pub scroll: usize,
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
    last_submission: Option<(String, Instant)>,
}

impl App {
    pub fn open(path: impl Into<String>) -> Result<Self, String> {
        let client = OrbitClient::open(path.into()).map_err(|error| format!("{error:?}"))?;
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
            projects_selection: 0,
            project: None,
            active_component: 0,
            selected: 0,
            scroll: 0,
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
            last_submission: None,
        };
        app.refresh_planning();
        app.refresh_workspace();
        Ok(app)
    }

    pub fn notify(&mut self, text: impl Into<String>, kind: StatusKind) {
        self.status = StatusMessage {
            text: text.into(),
            kind,
        };
    }

    pub fn refresh_workspace(&mut self) {
        match self.client.workspace_view_data() {
            Ok(workspace) => {
                self.projects = workspace.projects.clone();
                self.workspace = Some(workspace);
                self.workspace_error = None;
                self.rebuild_global_views();
                let max = self.sidebar_len().saturating_sub(1);
                self.sidebar_selection = self.sidebar_selection.min(max);
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
                self.refresh_planning();
                self.refresh_workspace();
            }
            Screen::Project => self.refresh_project(),
        }
    }

    pub fn refresh_after_mutation(&mut self) {
        self.refresh_planning();
        self.refresh_workspace();
        if self.screen == Screen::Project {
            self.refresh_project();
        }
    }

    pub fn open_project(&mut self, project_id: &str) {
        let Some(project) = self
            .projects
            .iter()
            .find(|project| project.id == project_id)
        else {
            self.notify(format!("Unknown project: {project_id}"), StatusKind::Error);
            return;
        };
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
                    .project_blueprint_binding(&project_id)
                    .ok()
                    .flatten()
                    .map(|binding| format!("{}@{}", binding.blueprint_id, binding.version))
                    .unwrap_or_else(|| "generated default".into());
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
                self.clamp_component();
                if !matches!(
                    self.status.kind,
                    StatusKind::Success | StatusKind::Warning | StatusKind::Pending
                ) {
                    self.notify(
                        if generated {
                            format!(
                                "{name} • legacy blueprint: generated default views • r refresh"
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
        Some(actions::detail_for(item, project_scope))
    }

    pub fn available_actions(&self) -> Vec<AvailableAction> {
        let project_scope = self.screen == Screen::Project;
        let mut actions = match self.selected_item() {
            Some(item) => actions::contextual(Some(item), project_scope),
            None => Vec::new(),
        };
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
        }
        if self.screen == Screen::Project {
            for action in [
                AvailableAction::enabled("create_task", "New task", None),
                AvailableAction::enabled("create_work_item", "New work item", None),
                AvailableAction::enabled("preview_plan", "Preview plan", None),
                AvailableAction::enabled("bind_evidence", "Bind evidence", None),
                AvailableAction::enabled("refresh", "Refresh project", None),
                AvailableAction::enabled("command_palette", "Command palette", None),
            ] {
                push_unique_action(&mut actions, action);
            }
            if self.pending_plan.is_some() {
                push_unique_action(
                    &mut actions,
                    AvailableAction::enabled("apply_plan", "Apply previewed plan", None),
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
                    if self.selected < self.scroll {
                        self.scroll = self.selected;
                    } else if self.selected > self.scroll + 7 {
                        self.scroll = self.selected.saturating_sub(7);
                    }
                }
            }
            FocusPane::Detail => {}
        }
    }

    pub fn move_component(&mut self, delta: isize) {
        let count = self.component_count();
        if count == 0 {
            return;
        }
        self.active_component = self
            .active_component
            .saturating_add_signed(delta)
            .min(count - 1);
        self.selected = 0;
        self.scroll = 0;
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
            state.active_view = next;
            self.active_component = 0;
            self.selected = 0;
            self.scroll = 0;
        }
    }

    pub fn switch_view_by_index(&mut self, index: usize) {
        let Some(state) = &mut self.project else {
            return;
        };
        if index < state.views.len() {
            state.active_view = index;
            self.active_component = 0;
            self.selected = 0;
            self.scroll = 0;
        }
    }

    pub fn focus_next(&mut self, reverse: bool) {
        let panes = if self.show_detail && self.wide_detail() {
            [FocusPane::Sidebar, FocusPane::Workspace, FocusPane::Detail]
        } else {
            [
                FocusPane::Sidebar,
                FocusPane::Workspace,
                FocusPane::Workspace,
            ]
        };
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
        self.screen = screen;
        self.focus = FocusPane::Workspace;
        self.active_component = 0;
        self.selected = 0;
        self.scroll = 0;
        self.prefix = None;
        self.sidebar_selection = match screen {
            Screen::Home => 0,
            Screen::Inbox => 1,
            Screen::Projects => 2,
            Screen::Agents => 3,
            Screen::Project => self.sidebar_selection,
        };
        if screen == Screen::Projects {
            self.projects_selection = 0;
        }
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
        self.scroll = self.scroll.min(self.selected);
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
            description: "Create an empty project".into(),
            action: PaletteAction::Prompt(PromptKind::NewProject),
            enabled: true,
        });
        entries.push(PaletteEntry {
            label: "New knowledge note".into(),
            description: "Capture a Markdown note".into(),
            action: PaletteAction::Prompt(PromptKind::NewKnowledge),
            enabled: true,
        });
        entries.push(PaletteEntry {
            label: "Install bundled blueprint".into(),
            description: "development, research, learning or habits".into(),
            action: PaletteAction::Prompt(PromptKind::InstallBlueprint),
            enabled: true,
        });
        if let Some(state) = &self.project {
            entries.push(PaletteEntry {
                label: "Bind blueprint version".into(),
                description: "Explicitly pin a published blueprint version".into(),
                action: PaletteAction::Prompt(PromptKind::BindBlueprint {
                    project_id: state.id.clone(),
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
            PaletteAction::Prompt(kind) => self.start_prompt(kind),
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
            "create_task" => self.start_prompt(PromptKind::NewTask {
                project_id: self
                    .project
                    .as_ref()
                    .filter(|_| self.screen == Screen::Project)
                    .map(|state| state.id.clone()),
            }),
            "create_work_item" => {
                if let Some(state) = &self.project {
                    self.start_prompt(PromptKind::NewWorkItem {
                        project_id: state.id.clone(),
                    });
                } else {
                    self.notify(
                        "Open a project before creating a work item",
                        StatusKind::Warning,
                    );
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
                self.start_prompt(PromptKind::MetricObservation { metric_id });
            }
            "set_metric_target" => {
                let metric_id = self.selected_item().map(|item| item.id.clone());
                self.start_prompt(PromptKind::MetricTarget { metric_id });
            }
            "bind_evidence" => {
                if let Some(state) = &self.project {
                    self.start_prompt(PromptKind::BindEvidence {
                        project_id: state.id.clone(),
                        unbind: false,
                    });
                } else {
                    self.notify("Open a project first", StatusKind::Warning);
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
            "review_agent" => {
                let lines = self
                    .selected_item()
                    .map(|item| {
                        item.fields
                            .iter()
                            .map(|(key, value)| format!("{key}: {value}"))
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                self.modal = Some(Modal::Message {
                    title: "Agent job".into(),
                    lines: if lines.is_empty() {
                        vec!["No agent job selected.".into()]
                    } else {
                        let mut lines = lines;
                        lines.push(String::new());
                        lines.push(
                            "Review is performed against accepted handoff evidence; job completion never completes the task."
                                .into(),
                        );
                        lines
                    },
                });
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
                        self.notify(
                            format!("Accepted: {} → {}", task.title, task.status),
                            StatusKind::Success,
                        );
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
            PendingAction::RecordMetric { metric_id, value } => {
                match self
                    .client
                    .record_metric_observation(metric_id.clone(), now_ms(), value)
                {
                    Ok(()) => {
                        self.notify(
                            format!("Accepted observation for {metric_id}"),
                            StatusKind::Success,
                        );
                        self.refresh_after_mutation();
                    }
                    Err(error) => self.notify(
                        format!("Metric observation rejected: {}", error.message),
                        StatusKind::Error,
                    ),
                }
            }
            PendingAction::SetMetricTarget {
                metric_id,
                baseline,
                target,
                direction,
            } => match self.client.set_metric_target(MetricTargetDto {
                metric_id: metric_id.clone(),
                baseline,
                target,
                direction,
            }) {
                Ok(()) => {
                    self.notify(
                        format!("Accepted target for {metric_id}"),
                        StatusKind::Success,
                    );
                    self.refresh_after_mutation();
                }
                Err(error) => self.notify(
                    format!("Metric target rejected: {}", error.message),
                    StatusKind::Error,
                ),
            },
            PendingAction::BindEvidence {
                project_id,
                kind,
                evidence_id,
                bind,
            } => match self
                .client
                .change_project_evidence(project_id, kind, evidence_id, bind)
            {
                Ok(()) => {
                    self.notify(
                        if bind {
                            "Evidence binding accepted"
                        } else {
                            "Evidence unbinding accepted"
                        },
                        StatusKind::Success,
                    );
                    self.refresh_after_mutation();
                }
                Err(error) => self.notify(
                    format!("Evidence change rejected: {}", error.message),
                    StatusKind::Error,
                ),
            },
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

    pub fn start_prompt(&mut self, kind: PromptKind) {
        let (title, hint) = match &kind {
            PromptKind::NewTask { .. } => ("New task", "title"),
            PromptKind::NewKnowledge => ("New knowledge note", "title"),
            PromptKind::NewProject => ("New project", "name"),
            PromptKind::NewWorkItem { .. } => (
                "New work item",
                "kind key=value key=value (e.g. hypothesis question=Does it work?)",
            ),
            PromptKind::InstallBlueprint => (
                "Install bundled blueprint",
                "development | research | learning | habits",
            ),
            PromptKind::BindBlueprint { .. } => ("Bind blueprint", "id@version (e.g. research@2)"),
            PromptKind::MetricTarget { .. } => (
                "Metric target",
                "metric-id baseline target at_least|at_most",
            ),
            PromptKind::MetricObservation { .. } => ("Record metric", "metric-id value"),
            PromptKind::BindEvidence { unbind, .. } => (
                if *unbind {
                    "Unbind evidence"
                } else {
                    "Bind evidence"
                },
                "habit:id or metric:id",
            ),
        };
        self.modal = Some(Modal::Prompt(PromptState {
            title: title.into(),
            hint: hint.into(),
            kind,
            value: String::new(),
        }));
    }

    pub fn submit_prompt(&mut self, prompt: PromptState) {
        let value = prompt.value.trim().to_owned();
        if value.is_empty() {
            self.notify("Empty input cancelled", StatusKind::Warning);
            return;
        }
        match prompt.kind {
            PromptKind::NewTask { project_id } => {
                match self.client.create_task(CreateTaskRequest {
                    id: None,
                    title: value,
                    project_id,
                    milestone_id: None,
                }) {
                    Ok(task) => {
                        self.notify(format!("Accepted task {}", task.id), StatusKind::Success);
                        self.refresh_after_mutation();
                    }
                    Err(error) => self.notify(
                        format!("Task capture rejected: {}", error.message),
                        StatusKind::Error,
                    ),
                }
            }
            PromptKind::NewKnowledge => {
                match self.client.create_knowledge(CreateKnowledgeRequest {
                    id: None,
                    kind: "note".into(),
                    title: value,
                    body_markdown: String::new(),
                }) {
                    Ok(note) => {
                        self.notify(format!("Accepted note {}", note.id), StatusKind::Success);
                        self.refresh_after_mutation();
                    }
                    Err(error) => self.notify(
                        format!("Note capture rejected: {}", error.message),
                        StatusKind::Error,
                    ),
                }
            }
            PromptKind::NewProject => {
                match self.client.create_project(CreateProjectRequest {
                    id: None,
                    name: value,
                }) {
                    Ok(project) => {
                        let id = project.id.clone();
                        self.notify(
                            format!("Accepted project {} ({})", project.name, id),
                            StatusKind::Success,
                        );
                        self.refresh_workspace();
                        self.open_project(&id);
                    }
                    Err(error) => self.notify(
                        format!("Project creation rejected: {}", error.message),
                        StatusKind::Error,
                    ),
                }
            }
            PromptKind::NewWorkItem { project_id } => {
                let mut parts = value.split_whitespace();
                let Some(kind) = parts.next() else {
                    self.notify("Expected work-item kind", StatusKind::Error);
                    return;
                };
                let mut properties = BTreeMap::new();
                for token in parts {
                    let Some((key, raw)) = token.split_once('=') else {
                        self.notify(
                            format!("Expected key=value, got '{token}'"),
                            StatusKind::Error,
                        );
                        return;
                    };
                    let value = if raw == "true" {
                        serde_json::Value::Bool(true)
                    } else if raw == "false" {
                        serde_json::Value::Bool(false)
                    } else if let Ok(number) = raw.parse::<i64>() {
                        serde_json::Value::from(number)
                    } else {
                        serde_json::Value::String(raw.into())
                    };
                    properties.insert(key.into(), value);
                }
                match self
                    .client
                    .create_blueprint_work_item(&project_id, kind, properties)
                {
                    Ok(item) => {
                        self.notify(
                            format!("Accepted work item {}", item.id),
                            StatusKind::Success,
                        );
                        self.refresh_after_mutation();
                    }
                    Err(error) => self.notify(
                        format!("Work item rejected: {}", error.message),
                        StatusKind::Error,
                    ),
                }
            }
            PromptKind::InstallBlueprint => match self.client.install_bundled_blueprint(&value) {
                Ok(blueprint) => {
                    self.notify(
                        format!(
                            "Installed {}@{} with {} views (not auto-bound)",
                            blueprint.id,
                            blueprint.version,
                            blueprint.views.len()
                        ),
                        StatusKind::Success,
                    );
                }
                Err(error) => self.notify(
                    format!("Blueprint install rejected: {}", error.message),
                    StatusKind::Error,
                ),
            },
            PromptKind::BindBlueprint { project_id } => {
                let Some((id, version)) = value.split_once('@') else {
                    self.notify("Expected id@version", StatusKind::Error);
                    return;
                };
                let Ok(version) = version.parse::<u32>() else {
                    self.notify("Version must be a positive integer", StatusKind::Error);
                    return;
                };
                match self.client.bind_project_blueprint(&project_id, id, version) {
                    Ok(binding) => {
                        self.notify(
                            format!(
                                "Bound {}@{} (pinned)",
                                binding.blueprint_id, binding.version
                            ),
                            StatusKind::Success,
                        );
                        self.refresh_after_mutation();
                    }
                    Err(error) => self.notify(
                        format!("Blueprint binding rejected: {}", error.message),
                        StatusKind::Error,
                    ),
                }
            }
            PromptKind::MetricTarget { metric_id } => {
                let parts: Vec<_> = value.split_whitespace().collect();
                let (metric_id, baseline, target, direction) = if let Some(id) = metric_id {
                    if parts.len() == 3 {
                        (id, parts[0], parts[1], parts[2])
                    } else {
                        self.notify(
                            "Expected baseline target at_least|at_most",
                            StatusKind::Error,
                        );
                        return;
                    }
                } else if parts.len() == 4 {
                    (parts[0].to_owned(), parts[1], parts[2], parts[3])
                } else {
                    self.notify(
                        "Expected: metric-id baseline target at_least|at_most",
                        StatusKind::Error,
                    );
                    return;
                };
                let (Ok(baseline), Ok(target)) = (baseline.parse::<i64>(), target.parse::<i64>())
                else {
                    self.notify("Baseline and target must be integers", StatusKind::Error);
                    return;
                };
                self.execute_pending(PendingAction::SetMetricTarget {
                    metric_id,
                    baseline,
                    target,
                    direction: direction.into(),
                });
            }
            PromptKind::MetricObservation { metric_id } => {
                let parts: Vec<_> = value.split_whitespace().collect();
                let (metric_id, raw_value) = if let Some(id) = metric_id {
                    if parts.len() != 1 {
                        self.notify("Expected value", StatusKind::Error);
                        return;
                    }
                    (id, parts[0])
                } else if parts.len() == 2 {
                    (parts[0].to_owned(), parts[1])
                } else {
                    self.notify("Expected: metric-id value", StatusKind::Error);
                    return;
                };
                let Ok(value) = raw_value.parse::<i64>() else {
                    self.notify("Observation value must be an integer", StatusKind::Error);
                    return;
                };
                self.execute_pending(PendingAction::RecordMetric { metric_id, value });
            }
            PromptKind::BindEvidence { project_id, unbind } => {
                let Some((kind, evidence_id)) = value.split_once(':') else {
                    self.notify("Expected habit:<id> or metric:<id>", StatusKind::Error);
                    return;
                };
                self.execute_pending(PendingAction::BindEvidence {
                    project_id,
                    kind: kind.into(),
                    evidence_id: evidence_id.into(),
                    bind: !unbind,
                });
            }
        }
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

    pub fn project_scroll(&self) -> usize {
        self.projects_selection.saturating_sub(8)
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
