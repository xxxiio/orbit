//! Contextual actions, palettes, planning shortcuts and the pending dispatcher.
use super::*;

impl App {
    pub fn available_actions(&self) -> Vec<AvailableAction> {
        let project_scope = self.screen == Screen::Project;
        let mut actions = match self.selected_item() {
            Some(item) => ui_actions::contextual(Some(item), project_scope),
            None => Vec::new(),
        };
        if let Some(item) = self.selected_item()
            && item.kind == ItemKind::Task
        {
            let mut quick =
                AvailableAction::enabled("quick_edit_task", "Quick edit", Some(item.id.clone()));
            quick.description = "Edit the title and priority in one compact form".into();
            push_unique_action(&mut actions, quick);
        }
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
            // Closed allowlist for connected mode. Attachment list/import/open
            // and export are supported; `purge_attachments` stays out because
            // only the authoritative server may garbage-collect content.
            actions.retain(|action| {
                matches!(
                    action.id.as_str(),
                    "open"
                        | "create_task"
                        | "attach_file"
                        | "open_attachments"
                        | "edit_task"
                        | "quick_edit_task"
                        | "edit_task_details"
                        | "manage_acceptance_criteria"
                        | "task_activity"
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

    /// Context-aware key hints. A key is advertised only when pressing it in
    /// the current focus/screen actually performs this action (or opens its
    /// menu). Actions without a reachable direct key stay menu/palette-only.
    pub fn action_key(&self, action: &AvailableAction) -> Option<&'static str> {
        // In compact mode the sidebar may own the only visible pane. Do not
        // advertise keys for a workspace selection the user cannot see.
        if self.viewport_width < crate::theme::MEDIUM_BREAKPOINT && self.focus == FocusPane::Sidebar
        {
            return None;
        }
        let selected = self.selected_item();
        let selected_kind = selected.map(|item| item.kind);
        let payload_matches_selection = action
            .payload
            .as_deref()
            .is_none_or(|payload| selected.is_some_and(|item| item.id == payload));
        match action.id.as_str() {
            "open" => {
                if self.screen == Screen::Projects {
                    let project = self.selected_project()?;
                    let payload = action.payload.as_deref();
                    return (payload.is_none() || payload == Some(project.id.as_str()))
                        .then_some("Enter");
                }
                // With sidebar focus Enter activates the sidebar row, not the
                // selected workspace item.
                if self.focus == FocusPane::Sidebar {
                    return None;
                }
                selected
                    .filter(|_| payload_matches_selection)
                    .map(|_| "Enter")
            }
            "edit_task" => {
                (selected_kind == Some(ItemKind::Task) && payload_matches_selection).then_some("e")
            }
            "edit_work_item" => (selected_kind == Some(ItemKind::WorkItem)
                && payload_matches_selection)
                .then_some("e"),
            "quick_edit_task" => {
                (selected_kind == Some(ItemKind::Task) && payload_matches_selection).then_some("E")
            }
            // `e` opens the selected entity's edit form. Project settings only
            // owns that key when the project itself is the selected entity.
            "edit_project" => {
                if self.screen == Screen::Projects || selected_kind == Some(ItemKind::Project) {
                    Some("e")
                } else {
                    None
                }
            }
            // `S` opens the status menu, whose entries are exactly these
            // model-allowed transitions.
            "set_task_status" | "complete_task" => {
                (selected_kind == Some(ItemKind::Task)).then_some("S")
            }
            // `c` creates a project on the Projects screen, not a task.
            "create_task" => (self.screen != Screen::Projects).then_some("c"),
            "create_work_item" => self.project.is_some().then_some("C"),
            "search" => Some("/"),
            "refresh" => Some("r"),
            "command_palette" => Some("Ctrl+P"),
            "help" => Some("?"),
            "attach_file" => (selected.is_some() && payload_matches_selection).then_some("i"),
            "open_attachments" => (selected.is_some() && payload_matches_selection).then_some("F"),
            "manage_acceptance_criteria" => {
                (selected_kind == Some(ItemKind::Task) && payload_matches_selection).then_some("K")
            }
            "task_activity" => {
                (selected_kind == Some(ItemKind::Task) && payload_matches_selection).then_some("H")
            }
            "change_workflow" => (!self.client.is_connected()).then_some("W"),
            "record_habit" => (selected_kind == Some(ItemKind::Habit) && payload_matches_selection)
                .then_some("Space"),
            "record_metric" => (selected_kind == Some(ItemKind::Metric)
                && payload_matches_selection)
                .then_some("o"),
            "set_metric_target" => (selected_kind == Some(ItemKind::Metric)
                && payload_matches_selection)
                .then_some("t"),
            "flush_pending" => self.client.is_connected().then_some("f"),
            "toggle_detail" => Some("D"),
            "preview_plan" => (self.screen == Screen::Project).then_some("p"),
            "apply_plan" => {
                (self.screen == Screen::Project && self.pending_plan.is_some()).then_some("P")
            }
            "bind_evidence" => (self.screen == Screen::Project).then_some("b"),
            _ => None,
        }
    }

    /// Short labels for the compact bar. The action menu and palette always
    /// keep the full, unambiguous label.
    pub fn short_label(id: &str) -> &'static str {
        match id {
            "open" | "open_knowledge" => "Open",
            "edit_task" | "edit_work_item" | "edit_project" => "Edit",
            "edit_task_details" => "Details",
            "quick_edit_task" => "Quick edit",
            "set_task_status" | "complete_task" => "Status",
            "create_task" => "New",
            "create_work_item" => "New item",
            "search" => "Search",
            "refresh" => "Refresh",
            "change_workflow" | "bind_blueprint" | "apply_basic_workflow" => "Workflow",
            "manage_acceptance_criteria" => "Criteria",
            "task_activity" => "Activity",
            "attach_file" => "Attach",
            "open_attachments" => "Files",
            "record_habit" => "Record",
            "record_metric" => "Log",
            "set_metric_target" => "Target",
            "command_palette" => "Palette",
            "inspect_agent" => "Inspect",
            "inspect_dependencies" => "Deps",
            "help" => "Help",
            _ => "Action",
        }
    }

    /// Ordered ids by context. The compact bar shows at most the first three
    /// that are enabled and have a real key; the action menu uses this list to
    /// build its "Suggested" group.
    fn primary_action_ids(&self) -> Vec<&'static str> {
        if self.screen == Screen::Projects && self.selected_project().is_some() {
            return vec!["open", "edit_project", "change_workflow"];
        }
        if let Some(item) = self.selected_item() {
            return match item.kind {
                ItemKind::Task => vec![
                    "open",
                    "edit_task",
                    "quick_edit_task",
                    "set_task_status",
                    "complete_task",
                ],
                ItemKind::WorkItem => vec!["open", "edit_work_item", "attach_file"],
                ItemKind::Habit => vec!["open", "record_habit"],
                ItemKind::Knowledge => vec!["open", "open_knowledge"],
                ItemKind::Metric => vec!["open", "record_metric", "set_metric_target"],
                ItemKind::AgentJob => vec!["open", "inspect_agent"],
                ItemKind::Project => vec!["open", "edit_project", "change_workflow", "create_task"],
                _ => Vec::new(),
            };
        }
        if self.screen == Screen::Project {
            vec!["create_task", "create_work_item", "edit_project", "refresh"]
        } else {
            vec!["create_task", "search", "refresh"]
        }
    }

    /// Bar order: the three most useful operations for the current selection.
    /// `status` is a synthetic entry that opens the status actions, so its key
    /// does not collide with any concrete "Move to …" action.
    fn bar_priority_ids(&self) -> Vec<&'static str> {
        if self.screen == Screen::Projects && self.selected_project().is_some() {
            return vec!["open", "edit_project", "change_workflow"];
        }
        if let Some(item) = self.selected_item() {
            return match item.kind {
                ItemKind::Task => vec!["open", "edit_task", "status"],
                ItemKind::WorkItem => vec!["open", "edit_work_item", "attach_file"],
                ItemKind::Habit => vec!["open", "record_habit"],
                ItemKind::Knowledge => vec!["open", "open_knowledge"],
                ItemKind::Metric => vec!["open", "record_metric", "set_metric_target"],
                ItemKind::AgentJob => vec!["open", "inspect_agent"],
                ItemKind::Project => vec!["open", "edit_project", "change_workflow"],
                _ => Vec::new(),
            };
        }
        vec!["create_task", "search", "refresh"]
    }

    /// Session MRU keys, most recent first. Test/diagnostic accessor; the list
    /// is never persisted.
    #[cfg(test)]
    pub fn recent_destinations(&self) -> &[String] {
        &self.recent_destinations
    }

    pub fn compact_actions(&self) -> Vec<CompactAction> {
        // In compact mode the sidebar can own the only visible pane; actions on
        // the hidden workspace selection must not be advertised as reachable.
        if self.viewport_width < crate::theme::MEDIUM_BREAKPOINT && self.focus == FocusPane::Sidebar
        {
            return Vec::new();
        }
        let actions = self.available_actions();
        let mut out: Vec<CompactAction> = Vec::new();
        let mut keys: Vec<String> = Vec::new();
        for id in self.bar_priority_ids() {
            let action = if id == "status" {
                actions
                    .iter()
                    .find(|action| action.enabled && action.id == "set_task_status")
            } else {
                actions
                    .iter()
                    .find(|action| action.enabled && action.id == id)
            };
            let Some(action) = action.cloned() else {
                continue;
            };
            let Some(key) = self.action_key(&action) else {
                continue;
            };
            let key = key.to_owned();
            let label = if id == "status" {
                "Status".to_owned()
            } else {
                Self::short_label(&action.id).to_owned()
            };
            if keys.contains(&key) {
                continue;
            }
            keys.push(key.clone());
            out.push(CompactAction { key, label });
            if out.len() == 3 {
                break;
            }
        }
        if out.is_empty() {
            for action in actions.iter().filter(|action| action.enabled) {
                let Some(key) = self.action_key(action) else {
                    continue;
                };
                if keys.contains(&key.to_owned()) {
                    continue;
                }
                keys.push(key.to_owned());
                out.push(CompactAction {
                    key: key.to_owned(),
                    label: Self::short_label(&action.id).to_owned(),
                });
                if out.len() == 3 {
                    break;
                }
            }
        }
        out
    }

    fn menu_actions(&self) -> (Vec<AvailableAction>, usize) {
        let primary_ids = self.primary_action_ids();
        let mut suggested = Vec::new();
        let mut rest = Vec::new();
        for action in self.available_actions() {
            if action.enabled && primary_ids.contains(&action.id.as_str()) {
                suggested.push(action);
            } else {
                rest.push(action);
            }
        }
        let split = suggested.len();
        suggested.extend(rest);
        (suggested, split)
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
        let (actions, primary) = self.menu_actions();
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
            primary,
            selected: 0,
        });
    }

    /// `S` opens the status transitions for the selected task. The compact bar
    /// advertises `S Status`; this is what that key actually does, so the hint
    /// never promises a direct mutation.
    pub fn open_status_menu(&mut self) {
        let Some(item) = self.selected_item() else {
            self.notify("Select a task to change its status", StatusKind::Warning);
            return;
        };
        if item.kind != ItemKind::Task {
            self.notify("Status changes apply to tasks", StatusKind::Warning);
            return;
        }
        let title = item.title.clone();
        let actions: Vec<AvailableAction> = self
            .available_actions()
            .into_iter()
            .filter(|action| action.id == "set_task_status" || action.id == "complete_task")
            .collect();
        if actions.is_empty() {
            self.notify(
                "No status change is available for this task",
                StatusKind::Info,
            );
            return;
        }
        let primary = actions.len();
        self.modal = Some(Modal::ActionMenu {
            title: format!("Status · {title}"),
            actions,
            primary,
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
        // The palette names the pending depth, so take one fresh count for this
        // deliberate user action instead of reading the client while rendering.
        self.refresh_pending_count();
        let entries = self.palette_entries("");
        self.modal = Some(Modal::Palette {
            query: String::new(),
            entries,
            selected: 0,
        });
    }

    pub fn palette_entries(&self, query: &str) -> Vec<PaletteEntry> {
        let entry = |label: String, description: String, action: PaletteAction, enabled: bool| {
            PaletteEntry {
                label,
                description,
                action,
                enabled,
                recent_rank: None,
            }
        };
        let mut entries = vec![
            palette_nav("Home / Today", "Global planning aggregation", Screen::Home),
            palette_nav("Inbox", "Untriaged tasks", Screen::Inbox),
            palette_nav("Projects", "All projects", Screen::Projects),
            palette_nav("Agents", "Agent execution monitoring", Screen::Agents),
        ];
        for project in self.projects.iter().take(30) {
            entries.push(entry(
                format!("Open project: {}", project.name),
                format!("{} · {}", project.mode, project.status),
                PaletteAction::OpenProject(project.id.clone()),
                true,
            ));
        }
        if let Some(state) = &self.project
            && self.screen == Screen::Project
        {
            for (index, view) in state.views.iter().enumerate() {
                entries.push(entry(
                    format!("View: {}", view.title),
                    if view.generated {
                        "Generated default view".into()
                    } else {
                        format!("{} components", view.components.len())
                    },
                    PaletteAction::OpenView(index),
                    true,
                ));
            }
        }
        let mut seen = std::collections::BTreeSet::new();
        for action in self.available_actions() {
            if !seen.insert((action.id.clone(), action.payload.clone())) {
                continue;
            }
            entries.push(entry(
                action.label.clone(),
                action
                    .disabled_reason
                    .clone()
                    .unwrap_or_else(|| action.description.clone()),
                PaletteAction::Execute(action.clone()),
                action.enabled,
            ));
        }
        entries.push(entry(
            "New project".into(),
            "Create a project with a structured form".into(),
            PaletteAction::Form(FormMode::NewProject),
            true,
        ));
        entries.push(entry(
            "New task".into(),
            "Create a task with project, estimate and deadline".into(),
            PaletteAction::Form(FormMode::NewTask {
                project_id: self
                    .project
                    .as_ref()
                    .filter(|_| self.screen == Screen::Project)
                    .map(|project| project.id.clone()),
            }),
            true,
        ));
        if !self.client.is_connected() {
            entries.push(entry(
                "New knowledge note".into(),
                "Capture a Markdown note".into(),
                PaletteAction::Form(FormMode::NewKnowledge),
                true,
            ));
            entries.push(entry(
                "Install bundled blueprint".into(),
                "development, research, learning or habits".into(),
                PaletteAction::Form(FormMode::InstallBlueprint),
                true,
            ));
            if let Some(state) = &self.project {
                entries.push(entry(
                    "Bind blueprint version".into(),
                    "Explicitly pin a published blueprint version".into(),
                    PaletteAction::Form(FormMode::BindBlueprint {
                        project_id: state.id.clone(),
                    }),
                    true,
                ));
            }
        }
        if !self.client.is_connected()
            && let Some(state) = &self.project
        {
            entries.push(entry(
                "Unbind project evidence".into(),
                "Remove a habit or metric binding".into(),
                PaletteAction::Form(FormMode::BindEvidence {
                    project_id: state.id.clone(),
                    unbind: true,
                }),
                true,
            ));
        }
        if self.client.is_connected() {
            entries.push(entry(
                "Pending commands".into(),
                format!(
                    "{} queued intention(s); none accepted yet — press ! to inspect",
                    self.pending_count()
                ),
                PaletteAction::Pending,
                true,
            ));
            entries.push(entry(
                "Connected commands…".into(),
                "Queue schedule, routine, milestone, strategy, habit, metric or note commands (PENDING)"
                    .into(),
                PaletteAction::Console,
                true,
            ));
        }
        entries.push(entry(
            "Keyboard help".into(),
            "Show all navigation and action keys".into(),
            PaletteAction::Execute(AvailableAction::enabled("help", "Help", None)),
            true,
        ));
        entries.push(entry(
            "Quit".into(),
            "Leave Orbit".into(),
            PaletteAction::Quit,
            true,
        ));
        let mut entries: Vec<PaletteEntry> = entries
            .into_iter()
            .map(|mut entry| {
                entry.recent_rank = self.palette_destination_key(&entry.action).and_then(|key| {
                    self.recent_destinations
                        .iter()
                        .position(|recent| recent == &key)
                });
                entry
            })
            .collect();
        let needle = query.trim().to_lowercase();
        if needle.is_empty() {
            // MRU first, then the stable construction order.
            entries.sort_by_key(|entry| entry.recent_rank.unwrap_or(usize::MAX));
            return entries;
        }
        let mut scored: Vec<(usize, i64, PaletteEntry)> = entries
            .into_iter()
            .enumerate()
            .filter_map(|(index, entry)| {
                let score = palette_match_score(&entry.label, &entry.description, &needle)?;
                let boost = self.palette_context_boost(&entry.action);
                Some((index, score - boost, entry))
            })
            .collect();
        // Stable tie-break by construction order keeps the list deterministic.
        scored.sort_by_key(|(index, score, _)| (*score, *index));
        scored.into_iter().map(|(_, _, entry)| entry).collect()
    }

    /// MRU key for a destination action. Mutations and commands are never
    /// recorded; only places the user actually opened.
    fn palette_destination_key(&self, action: &PaletteAction) -> Option<String> {
        match action {
            PaletteAction::Go(screen) => Some(format!("screen:{screen:?}")),
            PaletteAction::OpenProject(id) => Some(format!("project:{id}")),
            PaletteAction::OpenView(index) => Some(match &self.project {
                Some(project) => format!(
                    "view:{}:{}",
                    project.id,
                    project
                        .views
                        .get(*index)
                        .map(|view| view.id.as_str())
                        .unwrap_or("")
                ),
                None => format!("view:*:{index}"),
            }),
            PaletteAction::OpenKnowledge(id) => Some(format!("note:{id}")),
            PaletteAction::SelectItem { component, index } => {
                if self.active_component == *component && self.selected == *index {
                    self.selected_item().map(|item| format!("item:{}", item.id))
                } else {
                    None
                }
            }
            PaletteAction::Execute(action)
                if action.enabled && matches!(action.id.as_str(), "open" | "open_knowledge") =>
            {
                Some(format!(
                    "open:{}",
                    action.payload.as_deref().unwrap_or_default()
                ))
            }
            _ => None,
        }
    }

    /// A small ranking nudge (never more than half a match tier) for actions
    /// that belong to the current project, view, screen or selection.
    fn palette_context_boost(&self, action: &PaletteAction) -> i64 {
        let selected = self.selected_item();
        let selected_id = selected.map(|item| item.id.as_str());
        match action {
            PaletteAction::Go(screen) if *screen == self.screen => 40,
            PaletteAction::OpenProject(id)
                if self
                    .project
                    .as_ref()
                    .is_some_and(|project| &project.id == id) =>
            {
                40
            }
            PaletteAction::OpenView(index)
                if self
                    .project
                    .as_ref()
                    .is_some_and(|project| project.active_view == *index) =>
            {
                40
            }
            PaletteAction::SelectItem { component, index }
                if self.active_component == *component && self.selected == *index =>
            {
                50
            }
            PaletteAction::OpenKnowledge(id)
                if selected_id.is_some_and(|selected| selected == id) =>
            {
                40
            }
            PaletteAction::Execute(action) => {
                if selected_id.is_some() && action.payload.as_deref() == selected_id {
                    50
                } else if let Some(item) = selected {
                    let relevant = match item.kind {
                        ItemKind::Task => matches!(
                            action.id.as_str(),
                            "open"
                                | "edit_task"
                                | "quick_edit_task"
                                | "set_task_status"
                                | "complete_task"
                        ),
                        ItemKind::WorkItem => {
                            matches!(action.id.as_str(), "open" | "edit_work_item")
                        }
                        ItemKind::Metric => matches!(
                            action.id.as_str(),
                            "open" | "record_metric" | "set_metric_target"
                        ),
                        ItemKind::Habit => matches!(action.id.as_str(), "open" | "record_habit"),
                        _ => false,
                    };
                    if relevant { 30 } else { 0 }
                } else if matches!(action.id.as_str(), "create_task" | "search" | "refresh") {
                    20
                } else {
                    0
                }
            }
            PaletteAction::Form(FormMode::NewTask {
                project_id: Some(id),
            }) if self
                .project
                .as_ref()
                .is_some_and(|project| &project.id == id) =>
            {
                20
            }
            _ => 0,
        }
    }

    pub fn execute_palette_action(&mut self, action: PaletteAction) -> bool {
        // A disabled entry is listed so the reason is discoverable, but Enter
        // must never run it.
        if let PaletteAction::Execute(action) = &action
            && !action.enabled
        {
            self.notify(
                action
                    .disabled_reason
                    .clone()
                    .unwrap_or_else(|| format!("{} is unavailable", action.label)),
                StatusKind::Warning,
            );
            return false;
        }
        let mut destination = self.palette_destination_key(&action);
        let result = match action {
            PaletteAction::Go(screen) => {
                self.go_screen(screen);
                false
            }
            PaletteAction::OpenProject(id) => {
                self.open_project(&id);
                if !self
                    .project
                    .as_ref()
                    .is_some_and(|project| project.id == id)
                {
                    // A rejected/unknown project must not enter the MRU.
                    destination = None;
                }
                false
            }
            PaletteAction::OpenView(index) => {
                if self.screen != Screen::Project {
                    self.notify("Open a project before choosing a view", StatusKind::Info);
                    return false;
                }
                self.switch_view_by_index(index);
                false
            }
            PaletteAction::Execute(action) => self.execute_action(action),
            PaletteAction::Form(mode) => {
                self.start_form(mode);
                false
            }
            PaletteAction::SelectItem { component, index } => {
                self.active_component = component;
                self.selected = index;
                self.focus = FocusPane::Workspace;
                self.open_detail();
                // Record the row that was actually selected, not just rows
                // that were already active when the palette opened.
                destination = self.selected_item().map(|item| format!("item:{}", item.id));
                false
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
                false
            }
            PaletteAction::Pending => {
                self.open_pending();
                false
            }
            PaletteAction::Console => {
                self.open_connected_console();
                false
            }
            PaletteAction::Quit => return true,
        };
        if let Some(key) = destination {
            self.recent_destinations.retain(|recent| recent != &key);
            self.recent_destinations.insert(0, key);
            self.recent_destinations.truncate(5);
        }
        result
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
            "quick_edit_task" => {
                if let Some(item) = self.selected_item()
                    && item.kind == ItemKind::Task
                {
                    let task_id = item.id.clone();
                    self.start_form(FormMode::QuickEditTask { task_id });
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
            "purge_attachments" => {
                if self.client.is_connected() {
                    self.notify(CONNECTED_PURGE_REFUSED, StatusKind::Warning);
                    return false;
                }
                match self.client.purge_attachment_content() {
                    Ok(removed) => self.notify(
                        format!("Removed {removed} unreferenced attachment object(s)"),
                        StatusKind::Success,
                    ),
                    Err(error) => self.notify(
                        format!("Attachment cleanup failed: {}", error.message),
                        StatusKind::Error,
                    ),
                }
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
                    let uploads = self.client.pending_attachment_count().unwrap_or_default();
                    self.refresh_after_mutation();
                    self.refresh_pending_count();
                    // A blocked flush is a rejection; a flush that leaves
                    // staged uploads queued is still pending work. Neither may
                    // read as success.
                    let kind = if blocked {
                        StatusKind::Warning
                    } else if uploads > 0 {
                        StatusKind::Pending
                    } else {
                        StatusKind::Success
                    };
                    self.notify(
                        if uploads > 0 {
                            format!(
                                "{message}; {uploads} staged attachment upload(s) still pending (not accepted)"
                            )
                        } else {
                            message
                        },
                        kind,
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
            } => match self.remove_criterion(task_id.clone(), criterion_id) {
                Ok(_) => {
                    let (message, kind) = self.mutation_status("Acceptance criterion removed");
                    self.notify(message, kind);
                    self.open_criteria(task_id, task_title);
                }
                Err(error) => self.notify(
                    format!("Removing the criterion failed: {}", error.message),
                    StatusKind::Error,
                ),
            },
            PendingAction::RemoveAttachment {
                attachment_id,
                owner_kind,
                owner_id,
                title,
            } => {
                // Pending rows are local intent: discarding them needs no
                // server and must not imply that accepted content changed.
                let was_pending = self
                    .client
                    .pending_attachments(&owner_kind, &owner_id)
                    .map(|items| items.iter().any(|item| item.attachment.id == attachment_id))
                    .unwrap_or(false);
                match self.remove_attachment(attachment_id) {
                    Ok(content_removed) => {
                        if was_pending {
                            // Refresh only the staging side from the replica;
                            // an offline accepted-list failure must not close
                            // the modal or replace the discard message.
                            let items = match &self.modal {
                                Some(Modal::Attachments(state))
                                    if state.owner_kind == owner_kind
                                        && state.owner_id == owner_id =>
                                {
                                    state.items.clone()
                                }
                                _ => Vec::new(),
                            };
                            let pending = self
                                .client
                                .pending_attachments(&owner_kind, &owner_id)
                                .unwrap_or_default();
                            self.modal = Some(Modal::Attachments(AttachmentsModal {
                                owner_kind,
                                owner_id,
                                title,
                                items,
                                pending,
                                selected: 0,
                            }));
                        } else {
                            self.open_attachments(owner_kind, owner_id, title);
                        }
                        self.notify(
                            if was_pending {
                                "Staged upload discarded locally; it was never accepted"
                            } else if content_removed {
                                "Attachment removed; unreferenced content deleted"
                            } else {
                                "Attachment removed; content still referenced elsewhere"
                            },
                            StatusKind::Success,
                        );
                    }
                    Err(error) => self.notify(
                        format!("Removing the attachment failed: {}", error.message),
                        StatusKind::Error,
                    ),
                }
            }
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
            PendingAction::QueueIntent { intent } => {
                match self.client.queue_connected_intent(
                    &intent.description,
                    intent.kind,
                    &intent.action,
                ) {
                    Ok(command_id) => {
                        let (message, kind) = self.mutation_status(format!(
                            "{} queued as {command_id} (PENDING; not accepted; press ! to inspect)",
                            intent.description
                        ));
                        self.refresh_pending_count();
                        self.notify(message, kind);
                    }
                    Err(error) => self.notify(
                        format!("Queueing {} failed: {error}", intent.description),
                        StatusKind::Error,
                    ),
                }
            }
            PendingAction::QueueBatch { intents } => {
                let total = intents.len();
                let commands: Vec<_> = intents
                    .iter()
                    .enumerate()
                    .map(|(index, intent)| {
                        (
                            format!("{}-{index}", crate::backend::next_command_id()),
                            intent.kind,
                            intent.action.clone(),
                        )
                    })
                    .collect();
                match self.client.queue_connected_batch(&commands) {
                    Ok(inserted) => {
                        let (message, kind) = self.mutation_status(format!(
                            "Staged {inserted}/{total} pending intention(s) in one transaction (PENDING; not accepted; press ! to inspect)"
                        ));
                        self.refresh_pending_count();
                        self.notify(message, kind);
                    }
                    Err(error) => self.notify(
                        format!("{error}. No new commands were queued."),
                        StatusKind::Error,
                    ),
                }
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

    /// Read-only accepted task focus shared by `v` and `plan.focus`.
    pub(crate) fn task_focus_lines(&mut self, task_id: String) -> Result<Vec<String>, String> {
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
            .map_err(|e| e.message)?;
        Ok(planning_ui::render_focus(&focus))
    }

    /// Read-only accepted project focus shared by `G` and `plan.group`.
    pub(crate) fn project_focus_lines(
        &mut self,
        project_id: String,
        milestone_id: Option<String>,
    ) -> Result<Vec<String>, String> {
        let request =
            planning_ui::request(now_ms(), self.planning_hours, 100, Some(project_id.clone()))?;
        let brief = self.client.work_brief(&request).map_err(|e| e.message)?;
        let group = self
            .client
            .planning_group(
                &brief,
                &orbit_runtime::planning_focus::GroupFocusRequest {
                    project_id,
                    milestone_id,
                    max_tasks: 12,
                },
            )
            .map_err(|e| e.message)?;
        Ok(planning_ui::render_group_focus(&group))
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
        let result = self.task_focus_lines(task_id);
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
            self.project_focus_lines(project_id, None)
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

    /// Attachment owner kind for an item, when the entity supports attachments.
    fn attachment_owner(item: &SelectionItem) -> Option<(&'static str, String)> {
        match item.kind {
            ItemKind::Task => Some(("task", item.id.clone())),
            ItemKind::WorkItem => Some(("work_item", item.id.clone())),
            ItemKind::Project => Some(("project", item.id.clone())),
            _ => None,
        }
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

    /// Toggle one criterion's completion through the backend.
    ///
    /// Task status is never changed here. The docked detail pane shows
    /// `Acceptance: done/total`, so a successful mutation invalidates its cache
    /// and the next paint re-reads the accepted count.
    pub(crate) fn set_criterion_completed(
        &mut self,
        task_id: String,
        criterion_id: String,
        completed: bool,
    ) -> Result<Vec<orbit_client::AcceptanceCriterionDto>, orbit_client::ClientError> {
        let result =
            self.client
                .set_acceptance_criterion_completed(task_id, criterion_id, completed);
        if result.is_ok() {
            self.invalidate_detail_cache();
        }
        result
    }

    /// Move one criterion to a new index within the ordered list.
    pub(crate) fn move_criterion(
        &mut self,
        task_id: String,
        criterion_id: String,
        position: usize,
    ) -> Result<Vec<orbit_client::AcceptanceCriterionDto>, orbit_client::ClientError> {
        let result = self
            .client
            .move_acceptance_criterion(task_id, criterion_id, position);
        if result.is_ok() {
            self.invalidate_detail_cache();
        }
        result
    }

    /// Remove one criterion. The detail pane's `Acceptance` count changes, so
    /// its cache is invalidated for both accepted and queued outcomes.
    pub(crate) fn remove_criterion(
        &mut self,
        task_id: String,
        criterion_id: String,
    ) -> Result<Vec<orbit_client::AcceptanceCriterionDto>, orbit_client::ClientError> {
        let result = self
            .client
            .remove_acceptance_criterion(task_id, criterion_id);
        if result.is_ok() {
            self.invalidate_detail_cache();
        }
        result
    }

    /// Remove one attachment. The detail pane's `Attachments` count changes, so
    /// its cache is invalidated before the caller reopens the modal.
    pub(crate) fn remove_attachment(
        &mut self,
        attachment_id: String,
    ) -> Result<bool, orbit_client::ClientError> {
        let result = self.client.remove_attachment(attachment_id);
        if result.is_ok() {
            self.invalidate_detail_cache();
            self.refresh_pending_count();
        }
        result
    }

    /// Stage or import one attachment through the backend. A connected stage
    /// adds to the pending sum, so the cached count is refreshed here. The
    /// detail pane's `Attachments` count also changes, so the cache is
    /// invalidated in this wrapper rather than relying on the form caller.
    pub(crate) fn import_attachment(
        &mut self,
        owner_kind: String,
        owner_id: String,
        path: String,
        description: Option<String>,
    ) -> Result<orbit_client::AttachmentDto, orbit_client::ClientError> {
        let result = self
            .client
            .import_attachment(owner_kind, owner_id, path, description);
        if result.is_ok() {
            self.invalidate_detail_cache();
            self.refresh_pending_count();
        }
        result
    }

    /// Metadata and a bounded text preview for one attachment. Binary content
    /// is never executed or decoded as an image here; the user is directed to
    /// the external viewer instead.
    pub fn preview_attachment(&mut self, attachment_id: String) {
        let pending = self
            .client
            .pending_attachment(&attachment_id)
            .ok()
            .flatten()
            .is_some();
        match self.client.attachment_content(&attachment_id) {
            Ok((attachment, content)) => {
                let mut lines = vec![
                    if pending {
                        "Status: [pending] staged locally; not accepted by the server".to_owned()
                    } else {
                        "Status: accepted (last accepted server state)".to_owned()
                    },
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
                    title: if pending {
                        format!("Attachment · {} [pending]", attachment.filename)
                    } else {
                        format!("Attachment · {} [accepted]", attachment.filename)
                    },
                    lines,
                });
            }
            Err(error) => self.notify(
                format!("Attachment preview failed: {}", error.message),
                StatusKind::Error,
            ),
        }
    }

    /// Open the accepted attachments modal for one owner, with staged uploads
    /// listed separately and marked `[pending]`.
    pub fn open_attachments(&mut self, owner_kind: String, owner_id: String, title: String) {
        let pending = self
            .client
            .pending_attachments(&owner_kind, &owner_id)
            .unwrap_or_default();
        match self.client.list_attachments(&owner_kind, &owner_id) {
            Ok(items) => {
                self.modal = Some(Modal::Attachments(AttachmentsModal {
                    owner_kind,
                    owner_id,
                    title,
                    items,
                    pending,
                    selected: 0,
                }));
            }
            // Staged uploads are readable offline. Show them instead of hiding
            // durable local intent just because accepted reads need the server.
            Err(error) if !pending.is_empty() => {
                self.modal = Some(Modal::Attachments(AttachmentsModal {
                    owner_kind,
                    owner_id,
                    title,
                    items: Vec::new(),
                    pending,
                    selected: 0,
                }));
                // Never replace a just-reported pending upload with a warning;
                // the pending marker is the important acceptance fact.
                if self.status.kind != StatusKind::Pending {
                    self.notify(
                        format!(
                            "Accepted attachments unavailable (showing staged uploads): {}",
                            error.message
                        ),
                        StatusKind::Warning,
                    );
                }
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
}

fn push_unique_action(actions: &mut Vec<AvailableAction>, action: AvailableAction) {
    if !actions
        .iter()
        .any(|existing| existing.id == action.id && existing.payload == action.payload)
    {
        actions.push(action);
    }
}

pub(crate) fn palette_nav(label: &str, description: &str, screen: Screen) -> PaletteEntry {
    PaletteEntry {
        label: label.into(),
        description: description.into(),
        action: PaletteAction::Go(screen),
        enabled: true,
        recent_rank: None,
    }
}

/// Deterministic palette ranking. Tiers are 100 apart so a context boost can
/// reorder near-equals without letting a weaker match beat a stronger one:
/// label prefix, word start, subsequence, substring, description.
fn palette_match_score(label: &str, description: &str, needle: &str) -> Option<i64> {
    let label = label.to_lowercase();
    let description = description.to_lowercase();
    if label.starts_with(needle) {
        return Some(0);
    }
    if has_word_start(&label, needle) {
        return Some(100);
    }
    if is_subsequence(&label, needle) {
        return Some(200);
    }
    if label.contains(needle) {
        return Some(300);
    }
    if description.contains(needle) {
        return Some(400);
    }
    // Multi-word queries may match across words in either text.
    let tokens: Vec<&str> = needle.split_whitespace().collect();
    if tokens.len() > 1
        && tokens
            .iter()
            .all(|token| label.contains(token) || description.contains(token))
    {
        return Some(500);
    }
    None
}

fn has_word_start(haystack: &str, needle: &str) -> bool {
    haystack.char_indices().any(|(index, _)| {
        let before_is_boundary = index == 0
            || haystack[..index]
                .chars()
                .next_back()
                .is_none_or(|character| !character.is_alphanumeric());
        before_is_boundary && haystack[index..].starts_with(needle)
    })
}

fn is_subsequence(haystack: &str, needle: &str) -> bool {
    let mut characters = haystack.chars();
    needle
        .chars()
        .all(|wanted| characters.any(|candidate| candidate == wanted))
}
