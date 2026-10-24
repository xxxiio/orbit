//! Navigation, refresh, history and detail-model plumbing.
use super::*;

impl App {
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

    pub fn refresh_workspace(&mut self) {
        #[cfg(test)]
        self.workspace_refreshes
            .set(self.workspace_refreshes.get().wrapping_add(1));
        self.invalidate_detail_cache();
        // Capture the cursor before the accepted data is replaced, otherwise
        // restore_cursor has nothing stable to resolve the selected id against.
        self.sync_cursor();
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
                            "Inspection and review use the shared shell: press `:` and choose Connected commands, or `!` for queued intentions.".into(),
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

    pub(crate) fn insert_planning_component(&mut self) {
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

    /// Refresh after a quick edit changed only one task's title.
    ///
    /// The open project is re-resolved (one scoped read) and the one task row in
    /// the already-loaded workspace snapshot is patched in place. Re-reading the
    /// whole workspace here would repeat per-project evidence, habit history,
    /// knowledge and agent-job reads that a title edit cannot change. Priority
    /// changes can reorder Today/Next and deliberately keep the full refresh.
    pub fn refresh_after_task_title(&mut self, task_id: &str, title: &str) {
        self.invalidate_detail_cache();
        if self.screen == Screen::Project {
            self.refresh_project();
        } else {
            self.refresh_current();
        }
        let Some(workspace) = &mut self.workspace else {
            return;
        };
        let mut found = false;
        for task in workspace.tasks.iter_mut().chain(workspace.inbox.iter_mut()) {
            if task.id == task_id {
                task.title = title.to_owned();
                found = true;
            }
        }
        if let Some(brief) = &mut self.planning_brief {
            for item in &mut brief.items {
                if item.id == task_id {
                    item.title = title.to_owned();
                }
            }
            self.planning = Some(planning_ui::PlanningDocument::from_brief(brief));
        }
        if found {
            // Pure rebuild from the patched snapshot; no backend reads.
            self.rebuild_global_views();
        } else {
            // The task was not in this snapshot (for example it is outside the
            // bounded project list): fall back to the full refresh.
            self.refresh_workspace();
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
        self.clamp_component();
    }

    fn open_project_internal(&mut self, project_id: &str, record_history: bool) {
        self.invalidate_detail_cache();
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
        self.restore_skipped = false;
        self.active_component = 0;
        self.selected = 0;
        self.scroll = 0;
        self.sidebar_selection = 4 + index;
        self.pending_plan = None;
        self.refresh_project();
    }

    pub fn refresh_project(&mut self) {
        self.invalidate_detail_cache();
        let Some(state) = &self.project else {
            return;
        };
        let project_id = state.id.clone();
        let previous_view = state
            .views
            .get(state.active_view)
            .map(|view| view.id.clone());
        // The old view is still in place here: capture the selected row before
        // the resolved components replace it.
        self.sync_cursor();
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

    /// Invalidate the cached detail pane model. Any refresh, mutation or
    /// navigation that can change the pane's subject calls this; the cache key
    /// also covers identity changes, but an in-place refresh keeps the same
    /// identity, so the revision is what forces a rebuild there.
    pub(crate) fn invalidate_detail_cache(&self) {
        self.detail_revision
            .set(self.detail_revision.get().wrapping_add(1));
    }

    fn detail_cache_key(&self) -> DetailCacheKey {
        let project = self.project.as_ref();
        DetailCacheKey {
            screen: self.screen,
            project_id: project.map(|project| project.id.clone()),
            view_id: project
                .and_then(|project| project.views.get(project.active_view))
                .map(|view| view.id.clone()),
            projects_selection: self.projects_selection,
            active_component: self.active_component,
            selected: self.selected,
            selected_id: self.selected_item().map(|item| item.id.clone()),
            revision: self.detail_revision.get(),
        }
    }

    pub fn detail_model(&self) -> Option<DetailModel> {
        let key = self.detail_cache_key();
        if let Some((cached_key, cached)) = self.detail_cache.borrow().as_ref()
            && *cached_key == key
        {
            return cached.clone();
        }
        let detail = self.build_detail_model();
        *self.detail_cache.borrow_mut() = Some((key, detail.clone()));
        detail
    }

    fn build_detail_model(&self) -> Option<DetailModel> {
        #[cfg(test)]
        self.detail_cache_builds
            .set(self.detail_cache_builds.get() + 1);
        if self.screen == Screen::Projects {
            return self
                .selected_project()
                .map(|project| ui_actions::detail_for(&project_selection(project), false));
        }
        let project_scope = self.screen == Screen::Project;
        let item = self.selected_item()?;
        let mut detail = ui_actions::detail_for(item, project_scope);
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

    pub fn switch_view(&mut self, delta: isize) {
        self.invalidate_detail_cache();
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
        self.invalidate_detail_cache();
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
        self.invalidate_detail_cache();
        if record_history && self.screen != screen {
            self.push_history();
        }
        self.sync_cursor();
        self.screen = screen;
        self.focus = FocusPane::Workspace;
        // Navigating is deliberate intent, so a skipped restore no longer owns
        // the session file (a normal Home visit must persist Home).
        self.restore_skipped = false;
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

    pub(crate) fn workflow_label(&self, workflow_id: &str, version: u32) -> String {
        self.client
            .bundled_workflows()
            .into_iter()
            .find(|descriptor| descriptor.id == workflow_id)
            .map(|descriptor| format!("{} v{version}", descriptor.name))
            .unwrap_or_else(|| format!("{workflow_id} v{version}"))
    }

    pub(crate) fn task_dto(&self, task_id: &str) -> Option<orbit_client::TaskDto> {
        // One uniform read: local uses the authoritative projection, connected
        // uses the task-detail route. Scanning `list_tasks` would only see the
        // thin accepted snapshot and hide description/priority/labels.
        self.client.task_detail(task_id).ok()
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

pub(crate) fn empty_view(id: &str, title: &str) -> ResolvedView {
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
