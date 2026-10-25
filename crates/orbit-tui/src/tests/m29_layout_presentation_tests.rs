use crate::app::{FocusPane, FormMode, Modal, Screen};
use crate::*;
use orbit_ui::model::{KanbanColumnModel, ListItemModel, SelectionItem, TableRowModel};
use orbit_ui::{ComponentBody, LoadStatus, ResolvedComponent};
use ratatui::style::Modifier;
use ratatui::{Terminal, backend::TestBackend, buffer::Buffer};

fn base_app(stem: &str) -> App {
    let path = std::env::temp_dir().join(format!(
        "orbit-tui-{stem}-{}-{}.sqlite",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let _ = std::fs::remove_file(&path);
    App::open(path.to_string_lossy().into_owned()).unwrap()
}

/// A project on the Basic workflow with three tasks and the Board view
/// active - the same shape the M28/M29 acceptance scenarios drive.
fn project_app(stem: &str) -> App {
    let mut app = base_app(stem);
    let project = app
        .client
        .create_project(orbit_client::CreateProjectRequest {
            id: None,
            name: "Layout Project With A Long Name".into(),
        })
        .unwrap();
    app.client.install_bundled_blueprint("basic").unwrap();
    app.client
        .apply_workflow(project.id.clone(), "basic".to_owned(), None)
        .unwrap();
    for (title, statuses) in [
        ("Alpha task with a long title", vec!["ready"]),
        ("Beta task with a long title", vec!["ready"]),
        ("Gamma task with a long title", vec!["ready", "in_progress"]),
    ] {
        let task = app
            .client
            .create_task(orbit_client::CreateTaskRequest {
                id: None,
                title: title.into(),
                project_id: Some(project.id.clone()),
                milestone_id: None,
            })
            .unwrap();
        for status in statuses {
            app.client
                .set_task_status(orbit_client::SetTaskStatusRequest {
                    id: task.id.clone(),
                    status: status.into(),
                })
                .unwrap();
        }
    }
    app.refresh_workspace();
    app.open_project(&project.id);
    app
}

fn switch_to(app: &mut App, view_id: &str) {
    let index = app
        .project
        .as_ref()
        .unwrap()
        .views
        .iter()
        .position(|view| view.id == view_id)
        .unwrap_or_else(|| panic!("view {view_id} missing"));
    app.switch_view_by_index(index);
    app.active_component = 0;
}

fn buffer(app: &mut App, width: u16, height: u16) -> Buffer {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|frame| render::draw(frame, app)).unwrap();
    terminal.backend().buffer().clone()
}

fn text(buffer: &Buffer) -> String {
    (0..buffer.area.height)
        .map(|y| {
            (0..buffer.area.width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn render(app: &mut App, width: u16, height: u16) -> String {
    text(&buffer(app, width, height))
}

/// Title of the currently open inspector.
fn inspector_title(app: &App) -> String {
    match &app.modal {
        Some(Modal::Inspector(state)) => state.detail.title.clone(),
        other => panic!("inspector modal expected, found {other:?}"),
    }
}

fn selection_of(items: impl IntoIterator<Item = (String, String)>) -> Vec<SelectionItem> {
    items
        .into_iter()
        .map(|(id, title)| SelectionItem {
            id,
            kind: orbit_ui::ItemKind::Task,
            title,
            subtitle: String::new(),
            status: "ready".into(),
            fields: Vec::new(),
            allowed_statuses: Vec::new(),
            allowed_transitions: Vec::new(),
            revision: None,
        })
        .collect()
}

fn show_only(app: &mut App, component: ResolvedComponent) {
    app.screen = Screen::Home;
    app.focus = FocusPane::Workspace;
    app.active_component = 0;
    app.home.components = vec![component];
}

fn long_kanban() -> ResolvedComponent {
    let columns: Vec<KanbanColumnModel> = (0..7)
        .map(|column| KanbanColumnModel {
            key: format!("state-{column}"),
            title: format!("Column {column}"),
            items: (0..2)
                .map(|row| ListItemModel {
                    id: format!("card-{column}-{row}"),
                    kind: orbit_ui::ItemKind::Task,
                    title: format!(
                        "Card {row} in column {column} with a title far too long for a column"
                    ),
                    subtitle: "ready".into(),
                    status: "ready".into(),
                    group: format!("state-{column}"),
                    meta: Vec::new(),
                })
                .collect(),
        })
        .collect();
    let selection = selection_of(
        columns
            .iter()
            .flat_map(|column| column.items.iter())
            .map(|item| (item.id.clone(), item.title.clone())),
    );
    ResolvedComponent {
        id: "board".into(),
        title: "Task board".into(),
        component_type: "kanban".into(),
        status: LoadStatus::Ready,
        body: ComponentBody::Kanban { columns },
        selection,
    }
}

fn wide_table() -> ResolvedComponent {
    let columns = ["title", "status", "priority", "due", "estimate", "owner"]
        .into_iter()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let rows = (0..3)
        .map(|index| TableRowModel {
            id: format!("t{index}"),
            kind: orbit_ui::ItemKind::Task,
            status: "ready".into(),
            cells: vec![
                format!(
                    "{} long task title that should be truncated",
                    ["Alpha", "Bravo", "Charlie"][index]
                ),
                "ready".into(),
                "high".into(),
                "2026-11-01".into(),
                "30m".into(),
                "Dana".into(),
            ],
        })
        .collect::<Vec<_>>();
    let selection = selection_of(
        rows.iter()
            .map(|row| (row.id.clone(), row.cells[0].clone())),
    );
    ResolvedComponent {
        id: "tasks".into(),
        title: "Tasks".into(),
        component_type: "table".into(),
        status: LoadStatus::Ready,
        body: ComponentBody::Table { columns, rows },
        selection,
    }
}

#[test]
fn five_terminal_sizes_keep_a_closed_frame_focus_and_action_bar() {
    let mut app = project_app("m29-sizes");
    switch_to(&mut app, "board");
    for (width, height) in [(60, 18), (80, 24), (100, 30), (120, 35), (160, 45)] {
        let text = render(&mut app, width, height);
        let label = format!("{width}x{height}");
        assert_eq!(text.lines().count(), usize::from(height), "{label}: {text}");
        assert!(text.contains("Task board"), "{label}: {text}");
        assert!(text.contains("Actions:"), "{label}: {text}");
        assert!(
            text.contains("Ctrl+Q quit") && text.contains("q quit"),
            "{label}: {text}"
        );
        // The focused component draws the heavy border set, so focus is
        // visible without colour.
        assert!(text.contains("┏ Task board"), "{label}: {text}");
        assert!(text.contains("┗"), "{label}: {text}");
        // The last body row before the action bar closes on a border cell.
        let lines = text.lines().collect::<Vec<_>>();
        let actions = lines
            .iter()
            .position(|line| line.contains("Actions:"))
            .unwrap_or_else(|| panic!("{label}: no action bar\n{text}"));
        let last_body = lines[actions - 1].trim_end();
        assert!(
            last_body.ends_with('│')
                || last_body.ends_with('┃')
                || last_body.ends_with('╯')
                || last_body.ends_with('╮')
                || last_body.ends_with('┛'),
            "{label}: body frame is not closed: {last_body:?}\n{text}"
        );
        let ink = text.chars().filter(|c| !c.is_whitespace()).count();
        assert!(
            ink > usize::from(width) * usize::from(height) / 8,
            "{label}: frame looks empty ({ink} non-blank cells)\n{text}"
        );
    }
}

#[test]
fn very_small_terminals_still_render_a_closed_frame() {
    let mut app = project_app("m29-tiny");
    switch_to(&mut app, "board");
    for (width, height) in [(20, 6), (24, 8), (40, 10)] {
        let rendered = render(&mut app, width, height);
        let label = format!("{width}x{height}");
        assert_eq!(
            rendered.lines().count(),
            usize::from(height),
            "{label}: {rendered}"
        );
        for line in rendered.lines() {
            assert_eq!(
                line.chars().count(),
                usize::from(width),
                "{label}: row width changed: {line:?}"
            );
        }
        assert!(rendered.contains("Task board"), "{label}: {rendered}");
        assert!(rendered.contains("┏"), "{label}: {rendered}");
        assert!(rendered.contains("Ctrl+Q quit"), "{label}: {rendered}");
    }
}

#[test]
fn compact_layout_names_both_panes_and_keeps_a_way_back() {
    let mut app = project_app("m29-compact");
    switch_to(&mut app, "board");
    app.focus = FocusPane::Workspace;
    let workspace = render(&mut app, 60, 18);
    assert!(workspace.contains("▸ Workspace"), "{workspace}");
    assert!(workspace.contains("Sidebar"), "{workspace}");
    assert!(workspace.contains("Tab switch"), "{workspace}");
    assert!(workspace.contains("Enter inspect"), "{workspace}");
    assert!(!workspace.contains("Details"), "{workspace}");
    assert!(!workspace.contains("D details"), "{workspace}");

    app.focus = FocusPane::Sidebar;
    let sidebar = render(&mut app, 60, 18);
    assert!(sidebar.contains("▸ Sidebar"), "{sidebar}");
    assert!(sidebar.contains("┏ Orbit"), "{sidebar}");
    assert!(sidebar.contains("Layout Project"), "{sidebar}");
    // Focus state survives the pane switch.
    assert_eq!(app.focus, FocusPane::Sidebar);
}

#[test]
fn kanban_windows_columns_and_follows_the_selection() {
    let mut app = base_app("m29-kanban");
    show_only(&mut app, long_kanban());
    let first = render(&mut app, 80, 24);
    assert!(first.contains("columns 1-2 of 7"), "{first}");
    assert!(first.contains("▾ Column 0"), "{first}");
    assert!(first.contains("j/k card"), "{first}");
    assert!(first.contains("h/l panel"), "{first}");
    assert!(
        first.contains("…"),
        "long card titles must be ellipsized: {first}"
    );
    assert!(
        first
            .lines()
            .any(|line| line.contains("▸") && line.contains("Card 0")),
        "{first}"
    );

    app.selected = 13;
    let last = render(&mut app, 80, 24);
    assert!(last.contains("columns 6-7 of 7"), "{last}");
    assert!(last.contains("▾ Column 6"), "{last}");
    assert!(
        last.lines()
            .any(|line| line.contains("▸") && line.contains("column 6")),
        "selected card must stay in the window: {last}"
    );
    assert!(!last.contains("Column 0"), "{last}");

    // A narrow workspace still shows readable columns, never slivers.
    let narrow = render(&mut app, 60, 18);
    assert!(narrow.contains("columns 6-7 of 7"), "{narrow}");
    assert!(
        narrow
            .lines()
            .any(|line| line.contains("▸") && line.contains("column 6")),
        "{narrow}"
    );
}

#[test]
fn table_columns_prioritise_title_then_status() {
    let mut app = base_app("m29-table");
    show_only(&mut app, wide_table());
    app.selected = 1;

    let narrow = render(&mut app, 60, 18);
    assert!(narrow.contains("TITLE"), "{narrow}");
    assert!(narrow.contains("STATUS"), "{narrow}");
    assert!(narrow.contains("PRIORITY"), "{narrow}");
    assert!(!narrow.contains("OWNER"), "{narrow}");
    assert!(!narrow.contains("ESTIMATE"), "{narrow}");
    assert!(
        narrow.contains("Bravo long task title"),
        "the title column keeps a readable share: {narrow}"
    );
    assert!(
        narrow
            .lines()
            .any(|line| line.contains("▸") && line.contains("Bravo")),
        "the selected row keeps its marker at 60 columns: {narrow}"
    );

    let wide = render(&mut app, 160, 45);
    for header in ["TITLE", "STATUS", "PRIORITY", "DUE", "ESTIMATE", "OWNER"] {
        assert!(wide.contains(header), "missing {header}: {wide}");
    }
    assert!(
        wide.lines()
            .any(|line| line.contains("▸") && line.contains("Bravo")),
        "{wide}"
    );
}

#[test]
fn inspector_scroll_is_bounded_and_keeps_the_footer_visible() {
    let mut app = project_app("m29-detail");
    switch_to(&mut app, "tasks");
    app.selected = 0;
    let list_scroll = app.scroll;
    app.open_inspector();
    let rendered = render(&mut app, 120, 20);
    assert!(rendered.contains("Inspector · Alpha task"), "{rendered}");
    assert!(rendered.contains("PROPERTIES"), "{rendered}");
    assert!(
        rendered.contains("Esc close · j/k scroll · Enter close"),
        "the inspector footer must stay visible: {rendered}"
    );

    // Scrolling is clamped in the app path that owns the offset, and it never
    // reaches into the workspace list.
    for _ in 0..40 {
        handle_key(&mut app, KeyCode::Char('j'), KeyModifiers::NONE).unwrap();
    }
    assert!(matches!(app.modal, Some(Modal::Inspector(_))));
    assert_eq!(app.scroll, list_scroll, "the list scroll must not move");
    let bottom = render(&mut app, 120, 20);
    assert!(
        bottom.contains("Esc close · j/k scroll · Enter close"),
        "{bottom}"
    );
    for _ in 0..80 {
        handle_key(&mut app, KeyCode::Char('k'), KeyModifiers::NONE).unwrap();
    }
    let top = render(&mut app, 120, 20);
    assert!(top.contains("PROPERTIES"), "{top}");
    handle_key(&mut app, KeyCode::Esc, KeyModifiers::NONE).unwrap();
    assert!(app.modal.is_none());
}

#[test]
fn board_selection_keeps_its_stable_id_across_refreshes_and_a_status_change() {
    let mut app = project_app("m29-refresh");
    switch_to(&mut app, "board");
    let (selected, selected_id) = app
        .current_component()
        .unwrap()
        .selection
        .iter()
        .enumerate()
        .find(|(_, item)| item.title.starts_with("Beta"))
        .map(|(index, item)| (index, item.id.clone()))
        .expect("Beta card");
    assert!(
        selected > 0,
        "the selection index must be non-zero to prove stability, not identity"
    );
    app.selected = selected;
    assert_eq!(
        app.current_selected_id().as_deref(),
        Some(selected_id.as_str())
    );
    app.open_inspector();
    assert_eq!(inspector_title(&app), "Beta task with a long title");
    app.close_inspector();

    app.refresh_project();
    assert_eq!(
        app.current_selected_id().as_deref(),
        Some(selected_id.as_str()),
        "selection lost after refresh_project"
    );
    app.open_inspector();
    assert_eq!(inspector_title(&app), "Beta task with a long title");
    app.close_inspector();

    app.execute_pending(app::PendingAction::SetTaskStatus {
        task_id: selected_id.clone(),
        status: "in_progress".into(),
    });
    assert_eq!(
        app.current_selected_id().as_deref(),
        Some(selected_id.as_str()),
        "selection lost after a status transition"
    );
    app.open_inspector();
    assert_eq!(inspector_title(&app), "Beta task with a long title");
    app.close_inspector();

    app.refresh_workspace();
    assert_eq!(
        app.current_selected_id().as_deref(),
        Some(selected_id.as_str()),
        "selection lost after refresh_workspace"
    );
    app.open_inspector();
    assert_eq!(inspector_title(&app), "Beta task with a long title");
    app.close_inspector();
}

#[test]
fn compact_bar_ranks_task_actions_and_never_repeats_a_key() {
    let mut app = project_app("m29-bar");
    switch_to(&mut app, "board");
    app.selected = 0;
    let rendered = render(&mut app, 120, 35);
    let line = rendered
        .lines()
        .find(|line| line.contains("Actions:"))
        .unwrap_or_else(|| panic!("no action bar:\n{rendered}"));
    for expected in ["Open", "Edit", "Status", "a more actions", "q quit"] {
        assert!(line.contains(expected), "missing {expected}: {line}");
    }
    let payload = line
        .split_once("Actions:")
        .map(|(_, rest)| rest)
        .unwrap_or(line);
    let keys: Vec<&str> = payload
        .split(" · ")
        .filter_map(|part| {
            let part = part.trim();
            if part.starts_with("a more") || part == "q quit" {
                None
            } else {
                part.split_whitespace().next()
            }
        })
        .collect();
    assert!(!keys.is_empty(), "no ranked actions in {line}");
    let mut unique = keys.clone();
    unique.sort_unstable();
    unique.dedup();
    assert_eq!(unique.len(), keys.len(), "duplicate key in {line}");
}

fn agent_job_selection() -> SelectionItem {
    SelectionItem {
        id: "job-1".into(),
        kind: orbit_ui::ItemKind::AgentJob,
        title: "Nightly run".into(),
        subtitle: "running".into(),
        status: "running".into(),
        fields: Vec::new(),
        allowed_statuses: Vec::new(),
        allowed_transitions: Vec::new(),
        revision: None,
    }
}

#[test]
fn disabled_action_shows_its_reason_and_enter_does_not_run_it() {
    let mut app = base_app("m29-disabled");
    show_only(
        &mut app,
        ResolvedComponent {
            id: "jobs".into(),
            title: "Agent jobs".into(),
            component_type: "agent_jobs".into(),
            status: LoadStatus::Ready,
            body: ComponentBody::AgentJobs { jobs: Vec::new() },
            selection: vec![agent_job_selection()],
        },
    );
    app.selected = 0;
    app.open_action_menu();
    let Some(Modal::ActionMenu {
        actions,
        selected,
        primary,
        ..
    }) = &mut app.modal
    else {
        panic!("action menu");
    };
    *selected = actions
        .iter()
        .position(|action| action.id == "review_agent")
        .expect("review action");
    assert!(*primary < actions.len(), "disabled action belongs to More");
    let rendered = render(&mut app, 120, 40);
    assert!(rendered.contains("Suggested"), "{rendered}");
    assert!(rendered.contains("More actions"), "{rendered}");
    assert!(
        rendered.contains("— Review/resume requires an explicitly configured"),
        "{rendered}"
    );
    handle_key(&mut app, KeyCode::Enter, KeyModifiers::NONE).unwrap();
    assert!(
        matches!(app.modal, Some(Modal::ActionMenu { .. })),
        "disabled action must keep the menu open"
    );
    assert!(
        app.status.text.contains("explicitly configured"),
        "{}",
        app.status.text
    );
}

#[test]
fn palette_ranking_prefers_prefix_and_keeps_recents_for_an_empty_query() {
    let mut app = project_app("m29-palette-rank");
    let project_id = app.project.as_ref().unwrap().id.clone();
    let project_name = app.project.as_ref().unwrap().name.clone();

    let ranked = app.palette_entries("pro");
    assert_eq!(ranked[0].label, "Projects", "{:?}", ranked[0].label);
    assert!(ranked[1].label.to_lowercase().contains("project"));

    let quit = app.palette_entries("qui");
    assert_eq!(quit[0].label, "Quit", "{:?}", quit[0].label);

    app.execute_palette_action(app::PaletteAction::OpenProject(project_id));
    let recent = app.palette_entries("");
    assert_eq!(
        recent[0].recent_rank,
        Some(0),
        "the opened project should be the first recent entry: {:?}",
        recent[0].label
    );
    assert!(recent[0].label.contains(&project_name), "{:?}", recent[0]);
}

#[test]
fn palette_refuses_a_disabled_entry_and_keeps_the_query() {
    let mut app = base_app("m29-palette-disabled");
    show_only(
        &mut app,
        ResolvedComponent {
            id: "jobs".into(),
            title: "Agent jobs".into(),
            component_type: "agent_jobs".into(),
            status: LoadStatus::Ready,
            body: ComponentBody::AgentJobs { jobs: Vec::new() },
            selection: vec![agent_job_selection()],
        },
    );
    app.selected = 0;
    app.open_palette();
    let Some(Modal::Palette {
        entries, selected, ..
    }) = &mut app.modal
    else {
        panic!("palette");
    };
    *selected = entries
        .iter()
        .position(|entry| !entry.enabled)
        .expect("disabled palette entry");
    handle_key(&mut app, KeyCode::Enter, KeyModifiers::NONE).unwrap();
    let Some(Modal::Palette { query, .. }) = &app.modal else {
        panic!("palette should stay open");
    };
    assert!(query.is_empty());
    assert!(
        app.status.text.contains("unavailable") && app.status.text.contains("Review/resume"),
        "{}",
        app.status.text
    );
}

#[test]
fn quick_edit_edits_title_and_priority_through_existing_commands() {
    let mut app = project_app("m29-quick");
    switch_to(&mut app, "board");
    app.selected = 0;
    let item = app.selected_item().cloned().expect("task selection");
    let task_id = item.id.clone();
    let original_status = item.status.clone();
    let entries = app.palette_entries("");
    assert!(
        entries.iter().any(|entry| entry.label == "Quick edit"),
        "palette should offer quick edit"
    );
    handle_key(&mut app, KeyCode::Char('E'), KeyModifiers::NONE).unwrap();
    let Some(Modal::Form(mut form)) = app.modal.take() else {
        panic!("quick edit form");
    };
    assert!(
        matches!(form.mode, FormMode::QuickEditTask { .. }),
        "{:?}",
        form.mode
    );
    let keys: Vec<&str> = form.fields.iter().map(|field| field.key.as_str()).collect();
    assert_eq!(keys, ["title", "priority"], "{keys:?}");
    form.fields[0].value = "Quick edited title".into();
    form.fields[0].cursor = form.fields[0].value.chars().count();
    form.fields[1].value = "high".into();
    form.dirty = true;
    app.submit_form(form);
    let task = app.client.task_detail(&task_id).unwrap();
    assert_eq!(task.title, "Quick edited title");
    assert_eq!(task.priority.as_deref(), Some("high"));
    assert_eq!(
        task.status, original_status,
        "quick edit must not move status"
    );
    assert!(
        app.status.text.contains("Quick edit saved"),
        "{}",
        app.status.text
    );
}

#[test]
fn edit_task_puts_everyday_fields_first_and_planning_last() {
    let mut app = project_app("m29-order");
    switch_to(&mut app, "board");
    app.selected = 0;
    let task_id = app.selected_item().unwrap().id.clone();
    app.start_form(FormMode::EditTask { task_id });
    let Some(Modal::Form(form)) = &app.modal else {
        panic!("edit form");
    };
    let keys: Vec<&str> = form.fields.iter().map(|field| field.key.as_str()).collect();
    assert_eq!(&keys[..4], ["title", "priority", "description", "labels"]);
    assert_eq!(
        &keys[4..],
        ["project", "milestone", "estimate", "earliest", "deadline"]
    );
}

#[test]
fn invalid_submission_marks_and_focuses_the_field_without_losing_input() {
    let mut app = project_app("m29-invalid");
    switch_to(&mut app, "board");
    app.selected = 0;
    let task_id = app.selected_item().unwrap().id.clone();
    app.start_form(FormMode::EditTask { task_id });
    let Some(Modal::Form(mut form)) = app.modal.take() else {
        panic!("edit form");
    };
    let description_index = form
        .fields
        .iter()
        .position(|field| field.key == "description")
        .unwrap();
    form.fields[0].value.clear();
    form.fields[description_index].value = "keep this text".into();
    form.dirty = true;
    app.submit_form(form);
    let Some(Modal::Form(form)) = &app.modal else {
        panic!("form should stay open");
    };
    assert_eq!(form.invalid, Some(0));
    assert_eq!(form.focused, 0);
    assert!(form.error.as_deref().unwrap_or("").contains("Title"));
    assert_eq!(
        App::form_value(form, "description"),
        "keep this text",
        "typed values must survive a validation failure"
    );
    let rendered = render(&mut app, 120, 40);
    assert!(rendered.contains("! Title"), "{rendered}");
    // Editing the failing field clears its error; other fields do not.
    handle_key(&mut app, KeyCode::Char('X'), KeyModifiers::NONE).unwrap();
    let Some(Modal::Form(form)) = &app.modal else {
        panic!("form");
    };
    assert_eq!(form.invalid, None);
    assert!(form.error.is_none());
    assert_eq!(App::form_value(form, "description"), "keep this text");
}

#[test]
fn focused_multiline_field_shows_several_lines_around_the_caret() {
    let mut app = project_app("m29-multiline");
    switch_to(&mut app, "board");
    app.selected = 0;
    let task_id = app.selected_item().unwrap().id.clone();
    app.start_form(FormMode::EditTask { task_id });
    let Some(Modal::Form(mut form)) = app.modal.take() else {
        panic!("edit form");
    };
    let index = form
        .fields
        .iter()
        .position(|field| field.key == "description")
        .unwrap();
    form.focused = index;
    form.fields[index].value = "line one\nline two\nline three\nline four".into();
    form.fields[index].cursor = form.fields[index].value.chars().count();
    form.dirty = true;
    app.modal = Some(Modal::Form(form));
    let rendered = render(&mut app, 120, 40);
    assert!(rendered.contains("line two"), "{rendered}");
    assert!(rendered.contains("line four"), "{rendered}");
    assert!(
        !rendered.contains("more line(s)"),
        "focused multiline text should not collapse to a suffix: {rendered}"
    );
    assert!(
        rendered.contains('▏'),
        "the caret should be visible: {rendered}"
    );
}

fn select_kind(app: &mut App, kind: orbit_ui::ItemKind) {
    if app.project.is_none() {
        let project = app
            .client
            .create_project(orbit_client::CreateProjectRequest {
                id: None,
                name: "Key context".into(),
            })
            .unwrap();
        app.refresh_workspace();
        app.open_project(&project.id);
    }
    let state = app.project.as_mut().expect("project");
    let view = state.active_view;
    let component = state.views[view].components.first_mut().expect("component");
    component.selection = vec![SelectionItem {
        id: format!("fake-{kind:?}"),
        kind,
        title: format!("Fake {kind:?}"),
        subtitle: String::new(),
        status: "ready".into(),
        fields: Vec::new(),
        allowed_statuses: Vec::new(),
        allowed_transitions: Vec::new(),
        revision: None,
    }];
    app.active_component = 0;
    app.selected = 0;
}

fn action_named(app: &App, id: &str) -> orbit_ui::model::AvailableAction {
    app.available_actions()
        .into_iter()
        .find(|action| action.id == id)
        .unwrap_or_else(|| panic!("missing action {id}"))
}

#[test]
fn hotkeys_follow_the_current_context_in_the_bar_and_menu() {
    let mut app = project_app("m29-keys");
    // A freshly opened project lands on the Overview; make the metric
    // component the selected entity for this context.
    assert!(matches!(app.screen, Screen::Project));
    select_kind(&mut app, orbit_ui::ItemKind::Metric);
    assert_eq!(
        app.selected_item().map(|item| item.kind),
        Some(orbit_ui::ItemKind::Metric)
    );
    let settings = action_named(&app, "edit_project");
    assert_eq!(
        app.action_key(&settings),
        None,
        "e edits a task/metric, not project settings"
    );
    let open = action_named(&app, "open");
    assert_eq!(app.action_key(&open), Some("Enter"));

    let rendered = render(&mut app, 120, 35);
    let bar = rendered
        .lines()
        .find(|line| line.contains("Actions:"))
        .expect("action bar");
    assert!(!bar.contains("e Edit"), "{bar}");

    app.open_action_menu();
    let menu = render(&mut app, 120, 40);
    let settings_line = menu
        .lines()
        .find(|line| line.contains("Project settings"))
        .expect("project settings row");
    assert!(
        !settings_line.contains("e     Project settings"),
        "project settings must not claim e: {settings_line:?}"
    );
    app.modal = None;

    for kind in [
        orbit_ui::ItemKind::Metric,
        orbit_ui::ItemKind::Habit,
        orbit_ui::ItemKind::Knowledge,
    ] {
        select_kind(&mut app, kind);
        let settings = action_named(&app, "edit_project");
        assert_eq!(app.action_key(&settings), None, "{kind:?}");
        app.open_action_menu();
        let menu = render(&mut app, 120, 40);
        let line = menu
            .lines()
            .find(|line| line.contains("Project settings"))
            .expect("settings row");
        assert!(
            !line.contains("e     Project settings"),
            "project settings must not claim e: {line:?}"
        );
        app.modal = None;
    }

    select_kind(&mut app, orbit_ui::ItemKind::Task);
    assert_eq!(app.action_key(&action_named(&app, "edit_task")), Some("e"));
    assert_eq!(app.action_key(&action_named(&app, "edit_project")), None);

    // Sidebar focus: Enter activates the sidebar row, not the item.
    app.focus = FocusPane::Sidebar;
    assert_eq!(app.action_key(&action_named(&app, "open")), None);
    let wide = render(&mut app, 120, 35);
    let bar = wide
        .lines()
        .find(|line| line.contains("Actions:"))
        .expect("action bar");
    assert!(!bar.contains("Enter Open"), "{bar}");

    // Compact mode with the sidebar visible: workspace actions are hidden.
    let compact = render(&mut app, 60, 18);
    let bar = compact
        .lines()
        .find(|line| line.contains("Actions:"))
        .expect("action bar");
    assert!(bar.contains("a more") && bar.contains("q quit"), "{bar}");
    assert!(!bar.contains("Open") && !bar.contains("Edit"), "{bar}");
}

fn kanban_board(prefix: &str) -> ResolvedComponent {
    let columns = (0..6)
        .map(|column| KanbanColumnModel {
            key: format!("{prefix}-state-{column}"),
            title: format!("{prefix} column {column}"),
            items: (0..2)
                .map(|row| ListItemModel {
                    id: format!("{prefix}-card-{column}-{row}"),
                    kind: orbit_ui::ItemKind::Task,
                    title: format!("{prefix} card {column}-{row}"),
                    subtitle: "ready".into(),
                    status: "ready".into(),
                    group: format!("{prefix}-state-{column}"),
                    meta: Vec::new(),
                })
                .collect(),
        })
        .collect::<Vec<_>>();
    let selection = columns
        .iter()
        .flat_map(|column| column.items.iter())
        .map(|item| SelectionItem {
            id: item.id.clone(),
            kind: orbit_ui::ItemKind::Task,
            title: item.title.clone(),
            subtitle: item.subtitle.clone(),
            status: item.status.clone(),
            fields: Vec::new(),
            allowed_statuses: Vec::new(),
            allowed_transitions: Vec::new(),
            revision: None,
        })
        .collect();
    ResolvedComponent {
        id: prefix.into(),
        title: format!("{prefix} board"),
        component_type: "kanban".into(),
        status: LoadStatus::Ready,
        body: ComponentBody::Kanban { columns },
        selection,
    }
}

#[test]
fn only_the_active_board_marks_or_pans_a_selected_card() {
    let mut app = base_app("m29-two-boards");
    app.screen = Screen::Home;
    app.focus = FocusPane::Workspace;
    app.active_component = 0;
    app.selected = 11;
    app.home = orbit_ui::ResolvedView {
        id: "two-boards".into(),
        title: "Two boards".into(),
        description: String::new(),
        layout: orbit_blueprint::presentation::ViewLayout {
            kind: orbit_blueprint::presentation::ViewLayoutKind::Horizontal,
            columns: 1,
        },
        components: vec![kanban_board("Alpha"), kanban_board("Beta")],
        generated: false,
    };
    let rendered = render(&mut app, 160, 45);
    let marked = rendered
        .lines()
        .filter(|line| {
            line.contains("▸") && (line.contains("Alpha card") || line.contains("Beta card"))
        })
        .count();
    assert_eq!(marked, 1, "exactly one board marks a card:\n{rendered}");
    assert_eq!(
        rendered.matches("columns 4-6 of 6").count(),
        1,
        "only the active board pans to the selected column:\n{rendered}"
    );
}

#[test]
fn status_menu_lists_only_allowed_transitions_and_executes_one() {
    let mut app = project_app("m29-status-menu");
    switch_to(&mut app, "board");
    let (index, task_id, allowed) = app
        .current_component()
        .unwrap()
        .selection
        .iter()
        .enumerate()
        .find(|(_, item)| item.title.starts_with("Alpha"))
        .map(|(index, item)| (index, item.id.clone(), item.allowed_statuses.clone()))
        .expect("task");
    app.selected = index;
    assert!(!allowed.is_empty(), "fixture needs a transition");
    handle_key(&mut app, KeyCode::Char('S'), KeyModifiers::NONE).unwrap();
    let Some(Modal::ActionMenu {
        actions, selected, ..
    }) = &mut app.modal
    else {
        panic!("status menu");
    };
    assert!(
        actions.iter().all(|action| action.id == "set_task_status"),
        "{actions:?}"
    );
    let payloads: Vec<&str> = actions
        .iter()
        .filter_map(|action| action.payload.as_deref())
        .collect();
    assert_eq!(payloads.len(), allowed.len());
    assert!(
        payloads
            .iter()
            .all(|status| allowed.contains(&status.to_string()))
    );
    *selected = actions
        .iter()
        .position(|action| {
            action
                .payload
                .as_deref()
                .is_some_and(|status| status != "done" && status != "cancelled")
        })
        .unwrap_or(0);
    let chosen = actions[*selected].payload.clone().unwrap();
    handle_key(&mut app, KeyCode::Enter, KeyModifiers::NONE).unwrap();
    if matches!(app.modal, Some(Modal::Confirm { .. })) {
        handle_key(&mut app, KeyCode::Char('y'), KeyModifiers::NONE).unwrap();
    }
    let task = app.client.task_detail(&task_id).unwrap();
    assert_eq!(task.status, chosen);
}

#[test]
fn capital_s_and_e_are_text_inside_forms_palettes_and_search() {
    let mut app = base_app("m29-capital-text");
    app.start_form(FormMode::NewTask { project_id: None });
    handle_key(&mut app, KeyCode::Char('S'), KeyModifiers::NONE).unwrap();
    handle_key(&mut app, KeyCode::Char('E'), KeyModifiers::NONE).unwrap();
    let Some(Modal::Form(form)) = &app.modal else {
        panic!("form");
    };
    assert_eq!(App::form_value(form, "title"), "SE");

    app.modal = None;
    app.open_palette();
    handle_key(&mut app, KeyCode::Char('S'), KeyModifiers::NONE).unwrap();
    handle_key(&mut app, KeyCode::Char('E'), KeyModifiers::NONE).unwrap();
    let Some(Modal::Palette { query, .. }) = &app.modal else {
        panic!("palette");
    };
    assert_eq!(query, "SE");

    app.modal = None;
    app.open_search();
    handle_key(&mut app, KeyCode::Char('S'), KeyModifiers::NONE).unwrap();
    handle_key(&mut app, KeyCode::Char('E'), KeyModifiers::NONE).unwrap();
    let Some(Modal::Search { query, .. }) = &app.modal else {
        panic!("search");
    };
    assert_eq!(query, "SE");
}

#[test]
fn estimate_range_error_marks_the_estimate_field() {
    let mut app = base_app("m29-estimate-error");
    app.start_form(FormMode::NewTask { project_id: None });
    let Some(Modal::Form(mut form)) = app.modal.take() else {
        panic!("task form");
    };
    form.fields[0].value = "Range task".into();
    let estimate = form
        .fields
        .iter()
        .position(|field| field.key == "estimate")
        .unwrap();
    form.fields[estimate].value = "0".into();
    form.dirty = true;
    app.submit_form(form);
    let Some(Modal::Form(form)) = &app.modal else {
        panic!("form should stay open");
    };
    assert_eq!(form.invalid, Some(estimate));
    assert_eq!(form.focused, estimate);
    assert!(
        form.error
            .as_deref()
            .unwrap_or("")
            .contains("between 1 and 525600"),
        "{:?}",
        form.error
    );
}

#[test]
fn negative_timestamp_marks_the_earliest_field() {
    let mut app = project_app("m29-negative-time");
    switch_to(&mut app, "board");
    app.selected = 0;
    let task_id = app.selected_item().unwrap().id.clone();
    app.start_form(FormMode::EditTask { task_id });
    let Some(Modal::Form(mut form)) = app.modal.take() else {
        panic!("edit form");
    };
    let earliest = form
        .fields
        .iter()
        .position(|field| field.key == "earliest")
        .unwrap();
    form.fields[earliest].value = "-1".into();
    form.dirty = true;
    app.submit_form(form);
    let Some(Modal::Form(form)) = &app.modal else {
        panic!("form should stay open");
    };
    assert_eq!(form.invalid, Some(earliest));
    assert_eq!(form.focused, earliest);
    assert!(
        form.error.as_deref().unwrap_or("").contains("non-negative"),
        "{:?}",
        form.error
    );
}

#[test]
fn narrow_action_bar_still_reaches_status_and_quit() {
    let mut app = project_app("m29-bar60");
    switch_to(&mut app, "board");
    app.selected = 0;
    let compact = render(&mut app, 60, 18);
    let bar = compact
        .lines()
        .find(|line| line.contains("Actions:"))
        .expect("action bar");
    for expected in ["S Status", "a more", "q quit"] {
        assert!(bar.contains(expected), "missing {expected}: {bar}");
    }
    assert!(!bar.contains("a more actions"), "{bar}");

    let wide = render(&mut app, 120, 35);
    assert!(wide.contains("S status"), "board legend: {wide}");
}

#[test]
fn palette_recents_record_successful_destinations_only() {
    let mut fresh = base_app("m29-recents-fresh");
    fresh.execute_palette_action(app::PaletteAction::OpenProject("missing".into()));
    assert!(
        fresh.recent_destinations().is_empty(),
        "a rejected project must not enter the MRU: {:?}",
        fresh.recent_destinations()
    );

    let mut app = project_app("m29-recents");
    switch_to(&mut app, "board");
    let hit = app
        .search_hits("Alpha")
        .into_iter()
        .find(|hit| matches!(hit.action, app::PaletteAction::SelectItem { .. }))
        .expect("search hit for a card");
    app.execute_palette_action(hit.action);
    let selected = app.selected_item().map(|item| item.id.clone()).unwrap();
    let expected = format!("item:{selected}");
    assert_eq!(
        app.recent_destinations().first().map(String::as_str),
        Some(expected.as_str()),
        "search selection should be recorded"
    );

    app.go_screen(Screen::Home);
    let entries = app.palette_entries("");
    assert!(
        !entries.iter().any(|entry| entry.label.starts_with("View:")),
        "Home must not offer stale project view destinations"
    );
}

#[test]
fn sidebar_marks_only_the_open_project_and_sizes_for_its_label() {
    let mut app = base_app("m29-sidebar");
    let alpha = app
        .client
        .create_project(orbit_client::CreateProjectRequest {
            id: None,
            name: "Alpha".into(),
        })
        .unwrap();
    app.client
        .create_project(orbit_client::CreateProjectRequest {
            id: None,
            name: "Beta".into(),
        })
        .unwrap();
    app.refresh_workspace();
    app.open_project(&alpha.id);
    let rendered = render(&mut app, 120, 35);
    let alpha_line = rendered
        .lines()
        .find(|line| line.contains("» Alpha"))
        .expect("marked alpha row");
    assert!(
        alpha_line.contains('▸') || alpha_line.contains("  »"),
        "{alpha_line:?}"
    );
    assert!(
        !rendered.lines().any(|line| line.contains("» Beta")),
        "only the open project is marked:\n{rendered}"
    );
    assert!(
        rendered.lines().any(|line| line.contains("  Beta")),
        "the other project should still be listed:\n{rendered}"
    );

    // `●` is the `active` project-status glyph. It stays a status signal: the
    // current destination uses `»`, so a row can carry both meanings.
    let beta = app
        .client
        .list_projects()
        .unwrap()
        .into_iter()
        .find(|project| project.name == "Beta")
        .expect("beta project");
    app.client
        .set_project_status(orbit_client::SetProjectStatusRequest {
            id: beta.id,
            status: "active".into(),
        })
        .unwrap();
    app.refresh_workspace();
    let rendered = render(&mut app, 120, 35);
    let beta_line = rendered
        .lines()
        .find(|line| line.contains("Beta"))
        .expect("beta row");
    assert!(
        beta_line.contains('●'),
        "the active status glyph must still render: {beta_line:?}"
    );
    assert!(
        !beta_line.contains('»'),
        "a status glyph is not a current-destination marker: {beta_line:?}"
    );
    let alpha_line = rendered
        .lines()
        .find(|line| line.contains("» Alpha"))
        .expect("alpha row");
    assert!(
        alpha_line.contains('»') && !alpha_line.contains('●'),
        "the open project keeps the distinct current marker: {alpha_line:?}"
    );

    let long = "ProjectName12345";
    app.client
        .create_project(orbit_client::CreateProjectRequest {
            id: None,
            name: long.into(),
        })
        .unwrap();
    app.refresh_workspace();
    let wide = render(&mut app, 200, 40);
    assert!(
        wide.contains(long),
        "sidebar truncated a fitting label:\n{wide}"
    );
}

#[test]
fn selection_and_focus_survive_without_colour() {
    let mut app = project_app("m29-mono");
    switch_to(&mut app, "board");
    app.theme = theme::Theme { color: false };
    let buffer = buffer(&mut app, 80, 24);
    let text = text(&buffer);
    assert!(text.contains("┏ Task board"), "{text}");
    let mut found = false;
    for y in 0..buffer.area.height {
        for x in 0..buffer.area.width {
            let cell = &buffer[(x, y)];
            if cell.symbol() == "▸" {
                found = true;
                assert!(
                    cell.modifier.contains(Modifier::REVERSED)
                        || cell.modifier.contains(Modifier::BOLD),
                    "selection marker carries no non-colour signal at ({x},{y})"
                );
            }
        }
    }
    assert!(found, "no selection marker rendered:\n{text}");
}
