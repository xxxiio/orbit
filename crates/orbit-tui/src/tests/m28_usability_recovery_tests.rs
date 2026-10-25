use crate::app::{FocusPane, FormFieldKind, FormMode, Modal, Screen};
use crate::*;
use ratatui::{Terminal, backend::TestBackend};

fn app() -> App {
    let filename = std::env::temp_dir().join(format!(
        "orbit-tui-usability-{}-{}.sqlite",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let _ = std::fs::remove_file(&filename);
    App::open(filename.to_string_lossy().into_owned()).unwrap()
}

fn submit_project(app: &mut App, name: &str) -> String {
    app.start_form(FormMode::NewProject);
    let Some(Modal::Form(mut form)) = app.modal.take() else {
        panic!("expected project form");
    };
    form.fields[0].value = name.into();
    form.dirty = true;
    app.submit_form(form);
    app.project.as_ref().expect("project opened").id.clone()
}

fn submit_task(app: &mut App, project_id: &str, title: &str) -> String {
    app.start_form(FormMode::NewTask {
        project_id: Some(project_id.to_owned()),
    });
    let Some(Modal::Form(mut form)) = app.modal.take() else {
        panic!("expected task form");
    };
    form.fields[0].value = title.into();
    form.dirty = true;
    app.submit_form(form);
    app.client
        .list_tasks()
        .unwrap()
        .into_iter()
        .find(|task| task.title == title)
        .expect("task created")
        .id
}

fn buffer_text(app: &mut App, width: u16, height: u16) -> String {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|frame| render::draw(frame, app)).unwrap();
    let buffer = terminal.backend().buffer();
    (0..buffer.area.height)
        .map(|y| {
            (0..buffer.area.width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn new_project_form_defaults_to_the_basic_workflow() {
    let mut app = app();
    app.start_form(FormMode::NewProject);
    let Some(Modal::Form(form)) = &app.modal else {
        panic!("form");
    };
    let workflow = form
        .fields
        .iter()
        .find(|field| field.key == "workflow")
        .expect("workflow picker");
    assert_eq!(workflow.value, "basic");
    let FormFieldKind::Choice(choices) = &workflow.kind else {
        panic!("workflow must be a choice");
    };
    assert!(choices.iter().any(|choice| choice.value == "basic"));
}

#[test]
fn task_details_form_edits_multiline_text_and_saves_with_ctrl_s() {
    let mut app = app();
    let project_id = submit_project(&mut app, "Details");
    let task_id = submit_task(&mut app, &project_id, "Write docs");
    app.start_form(FormMode::EditTaskDetails {
        task_id: task_id.clone(),
    });
    for code in [
        KeyCode::Char('L'),
        KeyCode::Char('i'),
        KeyCode::Char('n'),
        KeyCode::Char('e'),
    ] {
        handle_key(&mut app, code, KeyModifiers::NONE).unwrap();
    }
    // Enter inserts a newline in a multi-line field instead of submitting.
    handle_key(&mut app, KeyCode::Enter, KeyModifiers::NONE).unwrap();
    assert!(matches!(app.modal, Some(Modal::Form(_))));
    handle_key(&mut app, KeyCode::Char('2'), KeyModifiers::NONE).unwrap();
    handle_key(&mut app, KeyCode::Char(' '), KeyModifiers::NONE).unwrap();
    handle_key(&mut app, KeyCode::Char('!'), KeyModifiers::NONE).unwrap();
    handle_key(&mut app, KeyCode::Char('s'), KeyModifiers::CONTROL).unwrap();

    let task = app
        .client
        .list_tasks()
        .unwrap()
        .into_iter()
        .find(|task| task.id == task_id)
        .expect("task");
    assert_eq!(task.description, "Line\n2 !");
    assert!(app.modal.is_none());
}

#[test]
fn editing_details_marks_priority_and_labels_without_touching_status() {
    let mut app = app();
    let project_id = submit_project(&mut app, "Rich");
    let task_id = submit_task(&mut app, &project_id, "Ship it");
    app.start_form(FormMode::EditTaskDetails {
        task_id: task_id.clone(),
    });
    let Some(Modal::Form(mut form)) = app.modal.take() else {
        panic!("form");
    };
    form.fields[1].value = "high".into();
    form.fields[2].value = "api, docs".into();
    app.submit_form(form);
    let task = app
        .client
        .list_tasks()
        .unwrap()
        .into_iter()
        .find(|task| task.id == task_id)
        .expect("task");
    assert_eq!(task.priority.as_deref(), Some("high"));
    assert_eq!(task.labels, vec!["api".to_owned(), "docs".to_owned()]);
    assert_eq!(task.status, "inbox");
}

#[test]
fn acceptance_criteria_are_checkable_without_completing_the_task() {
    let mut app = app();
    let project_id = submit_project(&mut app, "Criteria");
    let task_id = submit_task(&mut app, &project_id, "Acceptance");
    app.start_form(FormMode::AddCriterion {
        task_id: task_id.clone(),
    });
    let Some(Modal::Form(mut form)) = app.modal.take() else {
        panic!("form");
    };
    form.fields[0].value = "Works offline".into();
    form.dirty = true;
    app.submit_form(form);
    assert!(matches!(app.modal, Some(Modal::Criteria(_))));
    handle_key(&mut app, KeyCode::Char(' '), KeyModifiers::NONE).unwrap();
    let criteria = app.client.acceptance_criteria(&task_id).unwrap();
    assert_eq!(criteria.len(), 1);
    assert!(criteria[0].completed);
    let task = app
        .client
        .list_tasks()
        .unwrap()
        .into_iter()
        .find(|task| task.id == task_id)
        .expect("task");
    assert_eq!(
        task.status, "inbox",
        "checking every criterion must not complete the task"
    );
}

#[test]
fn attachments_can_be_added_listed_and_removed_from_the_tui() {
    let mut app = app();
    let project_id = submit_project(&mut app, "Files");
    let task_id = submit_task(&mut app, &project_id, "Attach");
    let source = std::env::temp_dir().join(format!(
        "orbit-attach-{}-{}.txt",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::write(&source, b"evidence").unwrap();

    app.start_form(FormMode::AttachFile {
        owner_kind: "task".into(),
        owner_id: task_id.clone(),
    });
    let Some(Modal::Form(mut form)) = app.modal.take() else {
        panic!("form");
    };
    form.fields[0].value = source.to_string_lossy().into_owned();
    form.dirty = true;
    app.submit_form(form);
    assert!(matches!(app.modal, Some(Modal::Attachments(_))));
    let attachments = app.client.list_attachments("task", &task_id).unwrap();
    assert_eq!(attachments.len(), 1);

    handle_key(&mut app, KeyCode::Char('x'), KeyModifiers::NONE).unwrap();
    assert!(matches!(app.modal, Some(Modal::Confirm { .. })));
    handle_key(&mut app, KeyCode::Char('y'), KeyModifiers::NONE).unwrap();
    app.open_attachments("task".into(), task_id.clone(), "Attach".into());
    let Some(Modal::Attachments(state)) = &app.modal else {
        panic!("attachments modal");
    };
    assert!(state.items.is_empty());
    let _ = std::fs::remove_file(&source);
}

#[test]
fn first_run_banner_and_action_bar_are_visible() {
    let mut app = app();
    let text = buffer_text(&mut app, 120, 30);
    assert!(text.contains("Getting started"), "{text}");
    assert!(text.contains("Basic is the default workflow"), "{text}");
    submit_project(&mut app, "Guided");
    let text = buffer_text(&mut app, 120, 30);
    assert!(text.contains("Actions:"), "{text}");
    assert!(text.contains("more actions"), "{text}");
}

/// Session-state tests mutate a process-global environment variable, so they
/// must not run concurrently with each other.
static STATE_ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn state_env_lock() -> std::sync::MutexGuard<'static, ()> {
    STATE_ENV_LOCK
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
}

fn temp_path(stem: &str, extension: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "orbit-tui-{stem}-{}-{}.{extension}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}

fn session_app(database: &std::path::Path) -> App {
    App::open(database.to_string_lossy().into_owned()).unwrap()
}

/// Temp database that removes itself (and its WAL sidecars) once every
/// `App` opened on it has been dropped. Declare it before the apps so the
/// connection closes first: SQLite can recreate the file on close.
struct TempDatabase {
    path: std::path::PathBuf,
}

impl TempDatabase {
    fn new(stem: &str) -> Self {
        let path = temp_path(stem, "sqlite");
        let _ = std::fs::remove_file(&path);
        Self { path }
    }

    fn path(&self) -> &std::path::Path {
        &self.path
    }
}

impl Drop for TempDatabase {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
        for suffix in ["-wal", "-shm"] {
            let mut path = self.path.as_os_str().to_owned();
            path.push(suffix);
            let _ = std::fs::remove_file(std::path::PathBuf::from(path));
        }
        // Attachment content lives in a sibling directory.
        let _ = std::fs::remove_dir_all(self.path.with_extension("knowledge"));
    }
}

/// Create a task directly (the form path deduplicates rapid submissions)
/// and refresh the active project view.
fn create_task(app: &mut App, project_id: &str, title: &str) -> String {
    let task = app
        .client
        .create_task(orbit_client::CreateTaskRequest {
            id: None,
            title: title.into(),
            project_id: Some(project_id.to_owned()),
            milestone_id: None,
        })
        .unwrap();
    app.refresh_current();
    task.id
}

/// Position of a stable id inside one view, as `(component, row)`.
fn locate_selection(view: &orbit_ui::ResolvedView, id: &str) -> Option<(usize, usize)> {
    view.components
        .iter()
        .enumerate()
        .find_map(|(component, item)| {
            item.selection
                .iter()
                .position(|row| row.id == id)
                .map(|row| (component, row))
        })
}

/// Position of a stable id across a project's views, as
/// `(view, component, row)`.
fn locate_project_selection(app: &App, id: &str) -> Option<(usize, usize, usize)> {
    let project = app.project.as_ref()?;
    project
        .views
        .iter()
        .enumerate()
        .find_map(|(view, resolved)| {
            locate_selection(resolved, id).map(|(component, row)| (view, component, row))
        })
}

#[test]
fn session_state_round_trips_active_project_and_view() {
    let _guard = state_env_lock();
    let state_file = temp_path("state", "json");
    let database = TempDatabase::new("session");
    // SAFETY: the module lock keeps this variable private to the test.
    unsafe { std::env::set_var("ORBIT_TUI_STATE", &state_file) };

    let mut app = session_app(database.path());
    let project_id = submit_project(&mut app, "Session");
    create_task(&mut app, &project_id, "First");
    let target = create_task(&mut app, &project_id, "Target");
    let (view_index, component_index, row) =
        locate_project_selection(&app, &target).expect("selectable task row");
    app.switch_view_by_index(view_index);
    app.active_component = component_index;
    app.selected = row;
    app.scroll = row;
    app.focus = FocusPane::Workspace;
    let expected_view = app.current_view().map(|view| view.id.clone());
    assert!(expected_view.is_some(), "project view resolved");

    ui_state::save(&app).unwrap();
    let loaded = ui_state::load().expect("state file");
    assert_eq!(loaded.project_id.as_deref(), Some(project_id.as_str()));
    assert_eq!(loaded.view_id, expected_view);
    assert_eq!(loaded.selected_id.as_deref(), Some(target.as_str()));
    assert_eq!(loaded.selected_index, Some(row));
    assert_eq!(loaded.active_component, Some(component_index));
    assert_eq!(loaded.scroll, Some(row));
    assert_eq!(loaded.focus.as_deref(), Some("workspace"));

    let mut restored = session_app(database.path());
    ui_state::restore(&mut restored);
    assert_eq!(restored.screen, Screen::Project);
    assert_eq!(
        restored.project.as_ref().map(|project| project.id.clone()),
        Some(project_id)
    );
    assert_eq!(
        restored.current_view().map(|view| view.id.clone()),
        expected_view
    );
    assert_eq!(
        restored.focus,
        FocusPane::Workspace,
        "focus must survive project restore"
    );
    assert_eq!(restored.active_component, component_index);
    assert_eq!(
        restored.current_selected_id().as_deref(),
        Some(target.as_str())
    );
    assert_eq!(restored.selected, row);
    assert_eq!(restored.scroll, row);

    unsafe { std::env::remove_var("ORBIT_TUI_STATE") };
    let _ = std::fs::remove_file(&state_file);
}

/// Regression: opening the project resets focus, so restore must apply the
/// saved focus afterwards.
#[test]
fn session_state_focus_survives_project_restore() {
    let _guard = state_env_lock();
    let state_file = temp_path("state-focus", "json");
    let database = TempDatabase::new("session-focus");
    // SAFETY: the module lock keeps this variable private to the test.
    unsafe { std::env::set_var("ORBIT_TUI_STATE", &state_file) };

    let mut app = session_app(database.path());
    submit_project(&mut app, "Focus");
    app.focus = FocusPane::Sidebar;
    ui_state::save(&app).unwrap();

    let mut restored = session_app(database.path());
    ui_state::restore(&mut restored);
    assert_eq!(restored.screen, Screen::Project);
    assert_eq!(restored.focus, FocusPane::Sidebar);

    unsafe { std::env::remove_var("ORBIT_TUI_STATE") };
    let _ = std::fs::remove_file(&state_file);
}

#[test]
fn session_state_restores_selection_by_id_over_a_stale_index() {
    let _guard = state_env_lock();
    let state_file = temp_path("state-reorder", "json");
    let database = TempDatabase::new("session-reorder");
    // SAFETY: the module lock keeps this variable private to the test.
    unsafe { std::env::set_var("ORBIT_TUI_STATE", &state_file) };

    let mut app = session_app(database.path());
    let project_id = submit_project(&mut app, "Reorder");
    create_task(&mut app, &project_id, "First");
    create_task(&mut app, &project_id, "Second");
    let target = create_task(&mut app, &project_id, "Third");
    let (view_index, component_index, row) =
        locate_project_selection(&app, &target).expect("selectable task row");
    assert!(row > 0, "target is not the first row: {row}");
    let view_id = app
        .project
        .as_ref()
        .and_then(|project| project.views.get(view_index))
        .map(|view| view.id.clone())
        .expect("view id");

    // A stale index (rows reordered since the save) must not win over the id.
    std::fs::write(
            &state_file,
            format!(
                r#"{{"project_id":"{project_id}","view_id":"{view_id}","active_component":{component_index},"selected_id":"{target}","selected_index":0}}"#
            ),
        )
        .unwrap();

    let mut restored = session_app(database.path());
    ui_state::restore(&mut restored);
    assert_eq!(
        restored.current_view().map(|view| view.id.clone()),
        Some(view_id)
    );
    assert_eq!(restored.active_component, component_index);
    assert_eq!(
        restored.current_selected_id().as_deref(),
        Some(target.as_str())
    );
    assert_eq!(restored.selected, row, "the id resolves to its new index");

    unsafe { std::env::remove_var("ORBIT_TUI_STATE") };
    let _ = std::fs::remove_file(&state_file);
}

#[test]
fn session_state_clamps_when_the_selected_item_is_gone() {
    let _guard = state_env_lock();
    let state_file = temp_path("state-gone", "json");
    let database = TempDatabase::new("session-gone");
    // SAFETY: the module lock keeps this variable private to the test.
    unsafe { std::env::set_var("ORBIT_TUI_STATE", &state_file) };

    let mut app = session_app(database.path());
    let project_id = submit_project(&mut app, "Gone");
    create_task(&mut app, &project_id, "First");
    let target = create_task(&mut app, &project_id, "Second");
    let (view_index, component_index, _) =
        locate_project_selection(&app, &target).expect("selectable task row");
    let view_id = app
        .project
        .as_ref()
        .and_then(|project| project.views.get(view_index))
        .map(|view| view.id.clone())
        .expect("view id");
    std::fs::write(
            &state_file,
            format!(
                r#"{{"project_id":"{project_id}","view_id":"{view_id}","active_component":{component_index},"selected_id":"deleted-item","selected_index":99}}"#
            ),
        )
        .unwrap();

    let mut restored = session_app(database.path());
    ui_state::restore(&mut restored);
    let len = restored
        .current_component()
        .map(|component| component.selection.len())
        .unwrap_or(0);
    assert!(len > 0, "the view still has rows");
    assert_eq!(
        restored.selected,
        len - 1,
        "a missing id falls back to the clamped index"
    );
    let selected = restored.selected_item().expect("a valid row is selected");
    assert_ne!(selected.id, "deleted-item");

    unsafe { std::env::remove_var("ORBIT_TUI_STATE") };
    let _ = std::fs::remove_file(&state_file);
}

#[test]
fn session_state_falls_back_to_view_zero_when_the_view_is_gone() {
    let _guard = state_env_lock();
    let state_file = temp_path("state-view", "json");
    let database = TempDatabase::new("session-view");
    // SAFETY: the module lock keeps this variable private to the test.
    unsafe { std::env::set_var("ORBIT_TUI_STATE", &state_file) };

    let mut app = session_app(database.path());
    let project_id = submit_project(&mut app, "Views");
    create_task(&mut app, &project_id, "First");
    let first_view = app
        .project
        .as_ref()
        .and_then(|project| project.views.first())
        .map(|view| view.id.clone())
        .expect("first view");
    let first_component_len = app
        .project
        .as_ref()
        .and_then(|project| project.views.first())
        .and_then(|view| view.components.first())
        .map(|component| component.selection.len())
        .unwrap_or(0);
    std::fs::write(
            &state_file,
            format!(
                r#"{{"project_id":"{project_id}","view_id":"retired-view","active_component":0,"selected_id":"retired-row","selected_index":99,"scroll":5}}"#
            ),
        )
        .unwrap();

    let mut restored = session_app(database.path());
    ui_state::restore(&mut restored);
    assert_eq!(
        restored.current_view().map(|view| view.id.clone()),
        Some(first_view),
        "a missing view id lands on view 0"
    );
    // The retired view's component/index must not carry over: view 0 clamps
    // its own saved index to a real row of its first component.
    assert_eq!(restored.active_component, 0);
    assert_eq!(
        restored.selected,
        first_component_len.saturating_sub(1),
        "a stale index from another view is clamped to view 0"
    );
    assert_eq!(
        restored.selected_item().is_some(),
        first_component_len > 0,
        "selection validity matches the component's row count"
    );
    assert_eq!(
        restored.scroll, 0,
        "a scroll captured in another view is not applied"
    );

    unsafe { std::env::remove_var("ORBIT_TUI_STATE") };
    let _ = std::fs::remove_file(&state_file);
}

#[test]
fn session_state_restores_non_project_screen_and_selection() {
    let _guard = state_env_lock();
    let state_file = temp_path("state-home", "json");
    let database = TempDatabase::new("session-home");
    // SAFETY: the module lock keeps this variable private to the test.
    unsafe { std::env::set_var("ORBIT_TUI_STATE", &state_file) };

    let mut app = session_app(database.path());
    let project_id = submit_project(&mut app, "Home");
    create_task(&mut app, &project_id, "One");
    let target = create_task(&mut app, &project_id, "Two");
    app.go_screen(Screen::Home);
    app.refresh_workspace();
    let (component_index, row) =
        locate_selection(app.current_view().expect("home view"), &target).expect("home row");
    app.active_component = component_index;
    app.selected = row;
    app.focus = FocusPane::Sidebar;
    ui_state::save(&app).unwrap();

    let mut restored = session_app(database.path());
    ui_state::restore(&mut restored);
    assert_eq!(restored.screen, Screen::Home);
    assert_eq!(restored.focus, FocusPane::Sidebar);
    assert_eq!(restored.active_component, component_index);
    assert_eq!(
        restored.current_selected_id().as_deref(),
        Some(target.as_str())
    );

    unsafe { std::env::remove_var("ORBIT_TUI_STATE") };
    let _ = std::fs::remove_file(&state_file);
}

#[test]
fn session_state_ignores_corrupt_and_partial_files() {
    let _guard = state_env_lock();
    let state_file = temp_path("state-corrupt", "json");
    let database = TempDatabase::new("session-corrupt");
    // SAFETY: the module lock keeps this variable private to the test.
    unsafe { std::env::set_var("ORBIT_TUI_STATE", &state_file) };

    std::fs::write(&state_file, "{not json").unwrap();
    assert!(ui_state::load().is_none(), "corrupt state is ignored");
    let mut app = session_app(database.path());
    ui_state::restore(&mut app);
    assert_eq!(app.screen, Screen::Home);
    assert_eq!(app.selected, 0);

    std::fs::write(&state_file, r#"{"project_id":"missing-project"}"#).unwrap();
    let partial = ui_state::load().expect("partial state loads");
    assert_eq!(partial.project_id.as_deref(), Some("missing-project"));
    assert_eq!(partial.screen, None);
    assert_eq!(partial.selected_id, None);
    ui_state::restore(&mut app);
    assert_eq!(app.screen, Screen::Home);
    assert_eq!(app.selected, 0);

    unsafe { std::env::remove_var("ORBIT_TUI_STATE") };
    let _ = std::fs::remove_file(&state_file);
}

#[test]
fn session_state_ignores_unknown_screen_and_focus_names() {
    let _guard = state_env_lock();
    let state_file = temp_path("state-unknown", "json");
    let database = TempDatabase::new("session-unknown");
    // SAFETY: the module lock keeps this variable private to the test.
    unsafe { std::env::set_var("ORBIT_TUI_STATE", &state_file) };

    let project_id = {
        let mut app = session_app(database.path());
        submit_project(&mut app, "Unknown")
    };
    std::fs::write(
        &state_file,
        format!(r#"{{"screen":"nowhere","project_id":"{project_id}","focus":"ceiling"}}"#),
    )
    .unwrap();
    let mut app = session_app(database.path());
    ui_state::restore(&mut app);
    // The unknown screen name is ignored; the saved project still restores
    // and the unknown focus name falls back to the default pane.
    assert_eq!(app.screen, Screen::Project);
    assert_eq!(
        app.project.as_ref().map(|project| project.id.clone()),
        Some(project_id)
    );
    assert_eq!(app.focus, FocusPane::Workspace);

    unsafe { std::env::remove_var("ORBIT_TUI_STATE") };
    let _ = std::fs::remove_file(&state_file);
}

#[test]
fn session_state_clamps_sidebar_and_projects_selections() {
    let _guard = state_env_lock();
    let state_file = temp_path("state-clamp", "json");
    let database = TempDatabase::new("session-clamp");
    // SAFETY: the module lock keeps this variable private to the test.
    unsafe { std::env::set_var("ORBIT_TUI_STATE", &state_file) };

    let mut app = session_app(database.path());
    submit_project(&mut app, "Clamp");
    std::fs::write(
        &state_file,
        r#"{"screen":"projects","focus":"sidebar","sidebar_selection":99,"projects_selection":99}"#,
    )
    .unwrap();

    let mut restored = session_app(database.path());
    ui_state::restore(&mut restored);
    assert_eq!(restored.screen, Screen::Projects);
    assert_eq!(restored.focus, FocusPane::Sidebar);
    assert_eq!(restored.sidebar_selection, restored.sidebar_len() - 1);
    assert_eq!(restored.projects_selection, restored.projects.len() - 1);

    unsafe { std::env::remove_var("ORBIT_TUI_STATE") };
    let _ = std::fs::remove_file(&state_file);
}

#[test]
fn selection_restore_tolerates_empty_views_and_missing_components() {
    let database = TempDatabase::new("selection-restore");
    let mut app = session_app(database.path());
    app.screen = Screen::Home;
    app.home.components.clear();
    let home_view = app.home.id.clone();
    app.restore_selection(
        Some("missing"),
        Some(5),
        Some(5),
        Some(home_view.as_str()),
        Some(3),
    );
    assert_eq!(app.active_component, 0);
    assert_eq!(app.selected, 0);
    assert_eq!(app.scroll, 3, "scroll applies for a component with no rows");

    app.home.components = vec![orbit_ui::ResolvedComponent {
        id: "empty".into(),
        title: "Empty".into(),
        component_type: "list".into(),
        status: orbit_ui::LoadStatus::Empty,
        body: orbit_ui::ComponentBody::List { items: Vec::new() },
        selection: Vec::new(),
    }];
    app.restore_selection(
        Some("missing"),
        Some(5),
        Some(2),
        Some("another-view"),
        Some(1),
    );
    assert_eq!(app.active_component, 0);
    assert_eq!(app.selected, 0);
    assert_eq!(
        app.scroll, 0,
        "a scroll from another view is not applied to this component"
    );

    app.restore_selection(
        Some("missing"),
        Some(5),
        Some(0),
        Some(home_view.as_str()),
        Some(1),
    );
    assert_eq!(app.scroll, 1, "matching view and component keep the scroll");
}

/// M1: background refreshes after a restore must not reset the restored row.
#[test]
fn session_state_survives_project_refresh_after_restore() {
    let _guard = state_env_lock();
    let state_file = temp_path("state-refresh", "json");
    let database = TempDatabase::new("session-refresh");
    // SAFETY: the module lock keeps this variable private to the test.
    unsafe { std::env::set_var("ORBIT_TUI_STATE", &state_file) };

    let mut app = session_app(database.path());
    let project_id = submit_project(&mut app, "Refresh");
    create_task(&mut app, &project_id, "First");
    let target = create_task(&mut app, &project_id, "Target");
    let (view_index, component_index, row) =
        locate_project_selection(&app, &target).expect("selectable task row");
    app.switch_view_by_index(view_index);
    app.active_component = component_index;
    app.selected = row;
    app.scroll = row;
    app.focus = FocusPane::Workspace;
    ui_state::save(&app).unwrap();

    let mut restored = session_app(database.path());
    ui_state::restore(&mut restored);
    assert_eq!(
        restored.current_selected_id().as_deref(),
        Some(target.as_str())
    );

    restored.refresh_project();
    assert!(
        restored
            .project
            .as_ref()
            .expect("project")
            .stale_error
            .is_none(),
        "refresh_project failed, so the restore assertions would be vacuous"
    );
    assert_eq!(
        restored.current_selected_id().as_deref(),
        Some(target.as_str()),
        "refresh_project reset the restored row"
    );
    assert_eq!(restored.active_component, component_index);
    assert_eq!(restored.scroll, row);
    assert_eq!(restored.focus, FocusPane::Workspace);

    restored.refresh_after_mutation();
    assert!(
        restored
            .project
            .as_ref()
            .expect("project")
            .stale_error
            .is_none(),
        "refresh_after_mutation failed, so the restore assertions would be vacuous"
    );
    assert_eq!(
        restored.current_selected_id().as_deref(),
        Some(target.as_str()),
        "refresh_after_mutation reset the restored row"
    );
    assert_eq!(restored.active_component, component_index);
    assert_eq!(restored.scroll, row);
    assert_eq!(restored.focus, FocusPane::Workspace);

    unsafe { std::env::remove_var("ORBIT_TUI_STATE") };
    let _ = std::fs::remove_file(&state_file);
}

/// M2: leaving and re-entering the restored view keeps the saved row.
#[test]
fn session_state_keeps_selection_after_switching_views() {
    let _guard = state_env_lock();
    let state_file = temp_path("state-switch", "json");
    let database = TempDatabase::new("session-switch");
    // SAFETY: the module lock keeps this variable private to the test.
    unsafe { std::env::set_var("ORBIT_TUI_STATE", &state_file) };

    let mut app = session_app(database.path());
    let project_id = submit_project(&mut app, "Switch");
    create_task(&mut app, &project_id, "First");
    let target = create_task(&mut app, &project_id, "Target");
    let (view_index, component_index, row) =
        locate_project_selection(&app, &target).expect("selectable task row");
    app.switch_view_by_index(view_index);
    app.active_component = component_index;
    app.selected = row;
    let view_count = app
        .project
        .as_ref()
        .map(|project| project.views.len())
        .unwrap_or(0);
    assert!(view_count > 1, "need a second view to switch to");
    ui_state::save(&app).unwrap();

    let mut restored = session_app(database.path());
    ui_state::restore(&mut restored);
    let other = (view_index + 1) % view_count;
    restored.switch_view_by_index(other);
    restored.switch_view_by_index(view_index);
    assert_eq!(
        restored.current_selected_id().as_deref(),
        Some(target.as_str()),
        "the restored row was lost after a view round-trip"
    );
    assert_eq!(restored.active_component, component_index);

    unsafe { std::env::remove_var("ORBIT_TUI_STATE") };
    let _ = std::fs::remove_file(&state_file);
}

/// M3/F1: a stale component index must not hide a row that still exists in
/// another component of the same view.
#[test]
fn session_state_finds_the_row_when_the_saved_component_is_stale() {
    let _guard = state_env_lock();
    let state_file = temp_path("state-stale-component", "json");
    let database = TempDatabase::new("session-stale-component");
    // SAFETY: the module lock keeps this variable private to the test.
    unsafe { std::env::set_var("ORBIT_TUI_STATE", &state_file) };

    let mut app = session_app(database.path());
    let project_id = submit_project(&mut app, "Stale component");
    create_task(&mut app, &project_id, "First");
    create_task(&mut app, &project_id, "Second");
    let target = create_task(&mut app, &project_id, "Third");
    let (view_index, component_index, row) =
        locate_project_selection(&app, &target).expect("selectable task row");
    let view = app
        .project
        .as_ref()
        .and_then(|project| project.views.get(view_index))
        .expect("view");
    let view_id = view.id.clone();
    let component_count = view.components.len();
    assert!(component_count > 1, "need another component to point at");
    let stale_component = (component_index + 1) % component_count;
    assert_ne!(stale_component, component_index);
    std::fs::write(
            &state_file,
            format!(
                r#"{{"project_id":"{project_id}","view_id":"{view_id}","active_component":{stale_component},"selected_id":"{target}","selected_index":0,"scroll":2}}"#
            ),
        )
        .unwrap();

    let mut restored = session_app(database.path());
    ui_state::restore(&mut restored);
    assert_eq!(
        restored.current_view().map(|view| view.id.clone()),
        Some(view_id)
    );
    assert_eq!(
        restored.active_component, component_index,
        "the owning component must win over the stale saved index"
    );
    assert_eq!(
        restored.current_selected_id().as_deref(),
        Some(target.as_str())
    );
    assert_eq!(restored.selected, row);
    assert_eq!(
        restored.scroll, 0,
        "a scroll offset from another component is not applied"
    );

    unsafe { std::env::remove_var("ORBIT_TUI_STATE") };
    let _ = std::fs::remove_file(&state_file);
}

/// F9: scroll for a component without rows (text/detail) is real state and
/// must round-trip, not just clamp to a selection.
#[test]
fn session_state_round_trips_scroll_for_a_non_selectable_component() {
    let _guard = state_env_lock();
    let state_file = temp_path("state-scroll", "json");
    let database = TempDatabase::new("session-scroll");
    // SAFETY: the module lock keeps this variable private to the test.
    unsafe { std::env::set_var("ORBIT_TUI_STATE", &state_file) };

    let mut app = session_app(database.path());
    let project_id = submit_project(&mut app, "Scroll");
    create_task(&mut app, &project_id, "First");
    // Only Text/Detail bodies render `app.scroll`; a metric would make the
    // round-trip assertion vacuous.
    let (view_index, component_index) = app
        .project
        .as_ref()
        .and_then(|project| {
            project
                .views
                .iter()
                .enumerate()
                .find_map(|(view, resolved)| {
                    resolved
                        .components
                        .iter()
                        .position(|component| {
                            component.selection.is_empty()
                                && matches!(
                                    component.body,
                                    orbit_ui::ComponentBody::Text { .. }
                                        | orbit_ui::ComponentBody::Detail { .. }
                                )
                        })
                        .map(|component| (view, component))
                })
        })
        .expect("a text or detail component");
    app.switch_view_by_index(view_index);
    app.active_component = component_index;
    app.selected = 0;
    app.scroll = 7;
    ui_state::save(&app).unwrap();

    let loaded = ui_state::load().expect("state file");
    assert_eq!(loaded.active_component, Some(component_index));
    assert_eq!(loaded.selected_id, None);
    assert_eq!(loaded.selected_index, Some(0));
    assert_eq!(loaded.scroll, Some(7));

    let mut restored = session_app(database.path());
    ui_state::restore(&mut restored);
    let component = restored.current_component().expect("component restored");
    assert!(
        component.selection.is_empty(),
        "the saved component has no rows"
    );
    assert!(
        matches!(
            component.body,
            orbit_ui::ComponentBody::Text { .. } | orbit_ui::ComponentBody::Detail { .. }
        ),
        "the restored component must be one that renders app.scroll"
    );
    assert_eq!(restored.active_component, component_index);
    assert_eq!(restored.selected, 0);
    assert_eq!(
        restored.scroll, 7,
        "scroll for a text pane was not restored"
    );

    unsafe { std::env::remove_var("ORBIT_TUI_STATE") };
    let _ = std::fs::remove_file(&state_file);
}

/// A pre-versioning file (no `version`) still restores project, view, focus.
#[test]
fn session_state_accepts_a_pre_versioning_file() {
    let _guard = state_env_lock();
    let state_file = temp_path("state-v0", "json");
    let database = TempDatabase::new("session-v0");
    // SAFETY: the module lock keeps this variable private to the test.
    unsafe { std::env::set_var("ORBIT_TUI_STATE", &state_file) };

    let mut app = session_app(database.path());
    let project_id = submit_project(&mut app, "Legacy");
    create_task(&mut app, &project_id, "First");
    let view_index = app
        .project
        .as_ref()
        .map(|project| project.views.len())
        .unwrap_or(0)
        .saturating_sub(1);
    let view_id = app
        .project
        .as_ref()
        .and_then(|project| project.views.get(view_index))
        .map(|view| view.id.clone())
        .expect("view id");
    std::fs::write(
            &state_file,
            format!(
                r#"{{"screen":"project","project_id":"{project_id}","view_id":"{view_id}","focus":"sidebar"}}"#
            ),
        )
        .unwrap();

    let loaded = ui_state::load().expect("v0 state loads");
    assert_eq!(loaded.version, 0);
    assert_eq!(loaded.active_component, None);

    let mut restored = session_app(database.path());
    ui_state::restore(&mut restored);
    assert_eq!(restored.screen, Screen::Project);
    assert_eq!(
        restored.project.as_ref().map(|project| project.id.clone()),
        Some(project_id)
    );
    assert_eq!(
        restored.current_view().map(|view| view.id.clone()),
        Some(view_id)
    );
    assert_eq!(restored.focus, FocusPane::Sidebar);

    unsafe { std::env::remove_var("ORBIT_TUI_STATE") };
    let _ = std::fs::remove_file(&state_file);
}

/// F5: a newer schema is forward-incompatible and must be ignored.
#[test]
fn session_state_ignores_a_newer_schema_version() {
    let _guard = state_env_lock();
    let state_file = temp_path("state-future", "json");
    let database = TempDatabase::new("session-future");
    // SAFETY: the module lock keeps this variable private to the test.
    unsafe { std::env::set_var("ORBIT_TUI_STATE", &state_file) };

    let project_id = {
        let mut app = session_app(database.path());
        submit_project(&mut app, "Future")
    };
    std::fs::write(
        &state_file,
        format!(
            r#"{{"version":{},"screen":"project","project_id":"{project_id}","selected_id":"x"}}"#,
            ui_state::UI_STATE_VERSION + 1
        ),
    )
    .unwrap();
    assert!(ui_state::load().is_none(), "a newer schema is ignored");

    let mut app = session_app(database.path());
    ui_state::restore(&mut app);
    assert_eq!(app.screen, Screen::Home);
    assert!(app.project.is_none());

    unsafe { std::env::remove_var("ORBIT_TUI_STATE") };
    let _ = std::fs::remove_file(&state_file);
}

/// F2: a failed workspace load must not erase the last good session.
#[test]
fn session_state_preserves_the_session_when_the_workspace_fails_to_load() {
    let _guard = state_env_lock();
    let state_file = temp_path("state-failed-load", "json");
    let database = TempDatabase::new("session-failed-load");
    // SAFETY: the module lock keeps this variable private to the test.
    unsafe { std::env::set_var("ORBIT_TUI_STATE", &state_file) };

    let mut app = session_app(database.path());
    let project_id = submit_project(&mut app, "Failed load");
    let target = create_task(&mut app, &project_id, "Target");
    let (view_index, component_index, row) =
        locate_project_selection(&app, &target).expect("selectable task row");
    app.switch_view_by_index(view_index);
    app.active_component = component_index;
    app.selected = row;
    app.focus = FocusPane::Sidebar;
    ui_state::save(&app).unwrap();
    let view_id = app.current_view().map(|view| view.id.clone());

    // The authority view never loaded: the shell is an empty fallback.
    let mut failed = session_app(database.path());
    failed.workspace = None;
    failed.screen = Screen::Home;
    ui_state::save(&failed).unwrap();

    let merged = ui_state::load().expect("merged state");
    assert_eq!(merged.version, ui_state::UI_STATE_VERSION);
    assert_eq!(merged.project_id.as_deref(), Some(project_id.as_str()));
    assert_eq!(merged.view_id, view_id);
    assert_eq!(merged.selected_id.as_deref(), Some(target.as_str()));
    assert_eq!(merged.selected_index, Some(row));
    assert_eq!(merged.active_component, Some(component_index));
    assert_eq!(merged.screen.as_deref(), Some("project"));
    assert_eq!(merged.focus.as_deref(), Some("sidebar"));

    unsafe { std::env::remove_var("ORBIT_TUI_STATE") };
    let _ = std::fs::remove_file(&state_file);
}

/// F2: a missing saved project lands on Home after a failed load and on the
/// Projects list when the workspace did load.
#[test]
fn session_state_missing_project_lands_home_after_a_failed_load() {
    let _guard = state_env_lock();
    let state_file = temp_path("state-missing-project", "json");
    let database = TempDatabase::new("session-missing-project");
    // SAFETY: the module lock keeps this variable private to the test.
    unsafe { std::env::set_var("ORBIT_TUI_STATE", &state_file) };

    std::fs::write(
        &state_file,
        r#"{"screen":"project","project_id":"missing-project"}"#,
    )
    .unwrap();

    let mut loaded = session_app(database.path());
    ui_state::restore(&mut loaded);
    assert_eq!(loaded.screen, Screen::Projects);

    let mut failed = session_app(database.path());
    failed.workspace = None;
    ui_state::restore(&mut failed);
    assert_eq!(failed.screen, Screen::Home);
    assert!(failed.project.is_none());

    unsafe { std::env::remove_var("ORBIT_TUI_STATE") };
    let _ = std::fs::remove_file(&state_file);
}

/// N1: a session whose restore was skipped must survive the authority
/// recovering while the user never reopens the project, and a deliberate
/// Home visit must still persist Home.
#[test]
fn session_state_survives_a_skipped_restore_after_recovery() {
    let _guard = state_env_lock();
    let state_file = temp_path("state-skipped-restore", "json");
    let database = TempDatabase::new("session-skipped-restore");
    // SAFETY: the module lock keeps this variable private to the test.
    unsafe { std::env::set_var("ORBIT_TUI_STATE", &state_file) };

    let mut app = session_app(database.path());
    let project_id = submit_project(&mut app, "Skipped");
    create_task(&mut app, &project_id, "First");
    let target = create_task(&mut app, &project_id, "Target");
    let (view_index, component_index, row) =
        locate_project_selection(&app, &target).expect("selectable task row");
    app.switch_view_by_index(view_index);
    app.active_component = component_index;
    app.selected = row;
    app.focus = FocusPane::Workspace;
    ui_state::save(&app).unwrap();
    let view_id = app.current_view().map(|view| view.id.clone());

    // Launch where the authority view never loads: the restore is skipped.
    let mut failed = session_app(database.path());
    failed.workspace = None;
    failed.projects.clear();
    failed.project_details.clear();
    ui_state::restore(&mut failed);
    assert!(failed.restore_skipped, "a skipped restore must be recorded");
    assert_eq!(failed.screen, Screen::Home);
    assert!(failed.project.is_none());

    // The authority recovers, but the user never reopens the project.
    failed.refresh_workspace();
    assert!(failed.workspace.is_some(), "authority view recovered");
    assert!(failed.project.is_none());
    ui_state::save(&failed).unwrap();

    let preserved = ui_state::load().expect("state file");
    assert_eq!(preserved.project_id.as_deref(), Some(project_id.as_str()));
    assert_eq!(preserved.view_id, view_id);
    assert_eq!(preserved.selected_id.as_deref(), Some(target.as_str()));
    assert_eq!(preserved.selected_index, Some(row));
    assert_eq!(preserved.active_component, Some(component_index));
    assert_eq!(
        preserved.screen.as_deref(),
        Some("project"),
        "the skipped session must survive a successful refresh"
    );

    // The next launch restores the project and the row.
    let mut restored = session_app(database.path());
    ui_state::restore(&mut restored);
    assert_eq!(restored.screen, Screen::Project);
    assert_eq!(
        restored.project.as_ref().map(|project| project.id.clone()),
        Some(project_id)
    );
    assert_eq!(
        restored.current_selected_id().as_deref(),
        Some(target.as_str())
    );

    // A deliberate Home visit is still remembered.
    restored.go_screen(Screen::Home);
    ui_state::save(&restored).unwrap();
    assert_eq!(
        ui_state::load().expect("state file").screen.as_deref(),
        Some("home"),
        "a deliberate Home visit must persist Home"
    );

    // Deliberate navigation also releases a skipped restore.
    failed.go_screen(Screen::Projects);
    assert!(!failed.restore_skipped);
    ui_state::save(&failed).unwrap();
    assert_eq!(
        ui_state::load().expect("state file").screen.as_deref(),
        Some("projects")
    );

    unsafe { std::env::remove_var("ORBIT_TUI_STATE") };
    let _ = std::fs::remove_file(&state_file);
}
