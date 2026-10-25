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
        // Capture the cursor before the accepted data is replaced, otherwise
        // restore_cursor has nothing stable to resolve the selected id against.
        self.sync_cursor();
        let connected_refresh_error = self.client.refresh_accepted().err();
        match self.client.workspace_view_data() {
            Ok(workspace) => {
                self.pending_count = workspace.pending_commands;
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
        // Rebuild an open inspector for its recorded subject. This never opens
        // one, so a refresh cannot make the overlay appear on its own.
        self.refresh_capability_actions();
        self.refresh_inspector();
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
    /// This is a *project-screen* fast path: the open project is re-resolved
    /// with one scoped read and the one task row in the already-loaded
    /// workspace snapshot is patched in place. Re-reading the whole workspace
    /// here would repeat per-project evidence, habit history, knowledge and
    /// agent-job reads that a title edit cannot change.
    ///
    /// On Home/Inbox/Agents/Projects there is no scoped read to fall back on:
    /// `refresh_current` is the full workspace read, so the fast path would be
    /// a lie. Those screens use the ordinary refresh instead.
    pub fn refresh_after_task_title(&mut self, task_id: &str, title: &str) {
        if self.screen != Screen::Project {
            self.refresh_current();
            return;
        }
        self.refresh_project();
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
        // Opening a project is deliberate navigation: the overlay closes and a
        // stashed inspector never reopens over it.
        self.close_inspector();
        self.pending_inspector = None;
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
        self.refresh_capability_actions();
        self.refresh_inspector();
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

    /// Identity of the item the cursor is on, or `None` when there is nothing
    /// to inspect. On the Projects screen the selected project row is the
    /// subject; everywhere else it is the selected item of the active
    /// component.
    pub(crate) fn current_inspector_subject(&self) -> Option<InspectorSubject> {
        if self.screen == Screen::Projects {
            return self
                .selected_project()
                .map(|project| InspectorSubject::Project {
                    project_id: project.id.clone(),
                });
        }
        let view = self.current_view()?;
        let component = view.components.get(self.active_component)?;
        let item = component.selected(self.selected)?;
        Some(InspectorSubject::Item {
            screen: self.screen,
            project_id: self.project.as_ref().map(|project| project.id.clone()),
            view_id: Some(view.id.clone()),
            component_id: component.id.clone(),
            kind: item.kind,
            id: item.id.clone(),
        })
    }

    /// Build the inspector model for `subject`.
    ///
    /// This is the only place the inspector issues detail reads
    /// (`task_detail`, `acceptance_criteria`, `list_attachments`, knowledge
    /// bodies). It therefore runs on input paths only - opening, refreshing and
    /// resuming after a mutation - and never from the renderer.
    ///
    /// `None` means the subject is no longer in accepted state: a deleted task,
    /// a closed project, a filtered-out row or a different view. Callers must
    /// paint the explicit fallback instead of inspecting the current selection.
    pub(crate) fn build_detail_for_subject(
        &self,
        subject: &InspectorSubject,
    ) -> Option<DetailModel> {
        #[cfg(test)]
        self.inspector_builds.set(self.inspector_builds.get() + 1);
        match subject {
            InspectorSubject::Project { project_id } => {
                let project = self
                    .projects
                    .iter()
                    .find(|project| &project.id == project_id)?;
                Some(ui_actions::detail_for(&project_selection(project), false))
            }
            InspectorSubject::Knowledge { id } => {
                let note = self
                    .client
                    .list_knowledge()
                    .ok()?
                    .into_iter()
                    .find(|note| &note.id == id)?;
                Some(DetailModel {
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
                })
            }
            InspectorSubject::Item {
                screen,
                project_id,
                view_id,
                component_id,
                kind,
                id,
            } => {
                // The subject is only reachable while the screen, project and
                // view it was opened from are still the active ones. A view
                // switch must show the fallback, not silently inspect a row
                // that happens to share the id or position.
                if self.screen != *screen {
                    return None;
                }
                let active_project = self.project.as_ref().map(|project| project.id.clone());
                if active_project != *project_id {
                    return None;
                }
                let view = self.current_view()?;
                if view_id.as_deref() != Some(view.id.as_str()) {
                    return None;
                }
                let component = view
                    .components
                    .iter()
                    .find(|component| &component.id == component_id)?;
                let item = component
                    .selection
                    .iter()
                    .find(|item| &item.id == id && item.kind == *kind)?;
                let mut detail = ui_actions::detail_for(item, self.screen == Screen::Project);
                if item.kind == ItemKind::Task
                    && let Some(task) = self.task_dto(&item.id)
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
                if item.kind == ItemKind::Knowledge
                    && let Ok(notes) = self.client.list_knowledge()
                    && let Some(note) = notes
                        .into_iter()
                        .find(|note| note.id == item.id || note.title == item.title)
                {
                    detail.note = note
                        .body_markdown
                        .lines()
                        .take(200)
                        .collect::<Vec<_>>()
                        .join("\n");
                }
                if item.kind == ItemKind::WorkItem {
                    for action in self.capability_actions_for(&item.id) {
                        super::actions::push_unique_action(&mut detail.actions, action);
                    }
                }
                Some(detail)
            }
        }
    }

    /// Open the inspector for the selected item.
    ///
    /// An empty view has nothing meaningful to inspect, so it reports
    /// `No selection` instead of painting an empty panel.
    pub fn open_inspector(&mut self) -> bool {
        let Some(subject) = self.current_inspector_subject() else {
            self.notify("No selection", StatusKind::Warning);
            return false;
        };
        self.open_inspector_for(subject)
    }

    pub(crate) fn open_inspector_for(&mut self, subject: InspectorSubject) -> bool {
        if matches!(
            &subject,
            InspectorSubject::Item {
                kind: ItemKind::WorkItem,
                ..
            }
        ) {
            // An explicit open may read: this is how the inspector's capability
            // block reflects durable state at the moment the user asks for it.
            self.refresh_capability_actions();
        }
        let Some(detail) = self.build_detail_for_subject(&subject) else {
            self.notify("No selection", StatusKind::Warning);
            return false;
        };
        let scroll = if self.inspector_subject.as_ref() == Some(&subject) {
            self.inspector_scroll
        } else {
            0
        };
        // Only painted panes can own the keyboard, so this is always a pane the
        // user can actually return to.
        let return_focus = self.focus;
        self.inspector_subject = Some(subject.clone());
        self.inspector_scroll = scroll;
        self.modal = Some(Modal::Inspector(InspectorState {
            subject,
            detail,
            scroll,
            return_focus,
            reopen_after_mutation: false,
            unavailable: false,
        }));
        true
    }

    /// Close the inspector and hand the keyboard back to the pane that had it.
    ///
    /// The workspace selection, active component and list scroll are untouched:
    /// the inspector is an overlay, not a pane that replaced the workspace.
    pub fn close_inspector(&mut self) {
        if !matches!(self.modal, Some(Modal::Inspector(_))) {
            return;
        }
        if let Some(Modal::Inspector(state)) = self.modal.take() {
            self.retire_inspector(state);
        }
    }

    /// Record a closing inspector's scroll/focus. Shared with key dispatch,
    /// which moves the modal out of `App` while it is handling a key.
    pub(crate) fn retire_inspector(&mut self, state: InspectorState) {
        self.inspector_subject = Some(state.subject);
        self.inspector_scroll = state.scroll;
        self.focus = state.return_focus;
        self.pending_inspector = None;
    }

    /// Rebuild the open inspector for its recorded subject.
    ///
    /// Called from the refresh and mutation paths instead of the old detail
    /// cache seeding. It never opens an inspector that is not already open, so
    /// navigating or refreshing cannot make the overlay appear on its own.
    pub fn refresh_inspector(&mut self) {
        let Some(Modal::Inspector(state)) = &self.modal else {
            return;
        };
        let subject = state.subject.clone();
        let built = self.build_detail_for_subject(&subject);
        let Some(Modal::Inspector(state)) = &mut self.modal else {
            return;
        };
        match built {
            Some(detail) => {
                state.detail = detail;
                state.unavailable = false;
            }
            // Keep the last known model: the fallback names what the user was
            // inspecting instead of showing a blank panel.
            None => state.unavailable = true,
        }
    }

    /// Stash the open inspector while a follow-up modal runs. The recorded
    /// intent is what makes the inspector reopen afterwards.
    pub(crate) fn begin_inspector_follow_up(&mut self, mut state: InspectorState) {
        state.reopen_after_mutation = true;
        self.pending_inspector = Some(state);
    }

    /// Reopen a stashed inspector once its follow-up modal has closed.
    ///
    /// A vanished subject is not inspected again: there is nothing to show, and
    /// silently inspecting a different row would be worse than staying closed.
    pub fn resume_pending_inspector(&mut self) {
        if self.modal.is_some() {
            return;
        }
        let Some(state) = self.pending_inspector.take() else {
            return;
        };
        if !state.reopen_after_mutation {
            return;
        }
        let Some(detail) = self.build_detail_for_subject(&state.subject) else {
            return;
        };
        self.inspector_subject = Some(state.subject.clone());
        self.inspector_scroll = state.scroll;
        self.modal = Some(Modal::Inspector(InspectorState {
            subject: state.subject,
            detail,
            scroll: state.scroll,
            return_focus: state.return_focus,
            reopen_after_mutation: false,
            unavailable: false,
        }));
    }

    /// Move the cursor back onto a recorded subject so the existing
    /// selection-driven actions (`a`, `e`, `E`) act on the inspected entity.
    ///
    /// Returns `false` when the subject is no longer in the current view; the
    /// caller then refuses instead of acting on an unrelated row.
    pub(crate) fn focus_inspector_subject(&mut self, subject: &InspectorSubject) -> bool {
        match subject {
            InspectorSubject::Project { project_id } => {
                let Some(index) = self
                    .projects
                    .iter()
                    .position(|project| &project.id == project_id)
                else {
                    return false;
                };
                self.projects_selection = index;
                self.focus = FocusPane::Workspace;
                true
            }
            InspectorSubject::Item {
                screen,
                project_id,
                view_id,
                component_id,
                kind,
                id,
            } => {
                if self.screen != *screen {
                    return false;
                }
                let active_project = self.project.as_ref().map(|project| project.id.clone());
                if active_project != *project_id {
                    return false;
                }
                let Some(view) = self.current_view() else {
                    return false;
                };
                if view_id.as_deref() != Some(view.id.as_str()) {
                    return false;
                }
                let Some(component) = view
                    .components
                    .iter()
                    .position(|component| &component.id == component_id)
                else {
                    return false;
                };
                let Some(row) = view.components[component]
                    .selection
                    .iter()
                    .position(|item| &item.id == id && item.kind == *kind)
                else {
                    return false;
                };
                self.active_component = component;
                self.selected = row;
                self.focus = FocusPane::Workspace;
                true
            }
            InspectorSubject::Knowledge { id } => {
                let Some(view) = self.current_view() else {
                    return false;
                };
                let located =
                    view.components
                        .iter()
                        .enumerate()
                        .find_map(|(component, resolved)| {
                            resolved
                                .selection
                                .iter()
                                .position(|item| &item.id == id)
                                .map(|row| (component, row))
                        });
                let Some((component, row)) = located else {
                    return false;
                };
                self.active_component = component;
                self.selected = row;
                self.focus = FocusPane::Workspace;
                true
            }
        }
    }

    pub fn switch_view(&mut self, delta: isize) {
        // A view switch is deliberate navigation: the overlay closes and a
        // stashed inspector never reopens over the new view.
        self.close_inspector();
        self.pending_inspector = None;
        let Some(state) = &self.project else {
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
        self.refresh_capability_actions();
    }

    pub fn switch_view_by_index(&mut self, index: usize) {
        // The inspector identity is view-scoped: switching views closes it and
        // drops the pending intent rather than reopening over another view.
        self.close_inspector();
        self.pending_inspector = None;
        let Some(state) = &self.project else {
            return;
        };
        if index < state.views.len() {
            self.sync_cursor();
            if let Some(state) = &mut self.project {
                state.active_view = index;
            }
            self.restore_cursor();
        }
        self.refresh_capability_actions();
    }

    pub fn activate_sidebar(&mut self) {
        match self.sidebar_selection {
            0 => {
                self.go_screen(Screen::Home);
                // Browsing the rail is not a move into the workspace: keep the
                // rail focused so j/k keeps switching screens. Opening a
                // project still hands the keyboard to the workspace.
                self.focus = FocusPane::Sidebar;
            }
            1 => {
                self.go_screen(Screen::Inbox);
                self.focus = FocusPane::Sidebar;
            }
            2 => {
                self.go_screen(Screen::Projects);
                self.focus = FocusPane::Sidebar;
            }
            3 => {
                self.go_screen(Screen::Agents);
                self.focus = FocusPane::Sidebar;
            }
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
        // Changing screen is deliberate navigation: the overlay closes and a
        // stashed inspector never reopens over the destination.
        self.close_inspector();
        self.pending_inspector = None;
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
        } else {
            self.refresh_capability_actions();
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
