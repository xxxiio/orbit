use crate::app::{FocusPane, FormMode, Modal, PaletteAction, PendingAction, Screen};
use crate::*;
use ratatui::{Terminal, backend::TestBackend};

fn app() -> App {
    let filename = std::env::temp_dir().join(format!(
        "orbit-tui-m28-{}-{}.sqlite",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let _ = std::fs::remove_file(&filename);
    App::open(filename.to_string_lossy().into_owned()).unwrap()
}

fn submit_project(app: &mut App, name: &str) {
    app.start_form(FormMode::NewProject);
    let Some(Modal::Form(mut form)) = app.modal.take() else {
        panic!("expected project form");
    };
    form.fields[0].value = name.into();
    form.dirty = true;
    app.submit_form(form);
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
fn fresh_workspace_renders_home_without_querying_storage_in_renderer() {
    let mut app = app();
    assert_eq!(app.screen, Screen::Home);
    assert!(!app.home.components.is_empty());
    let text = buffer_text(&mut app, 120, 30);
    assert!(text.contains("ORBIT"), "{text}");
    assert!(text.contains("Today / Next"), "{text}");
    // The permanent Details column is gone: the inspector is opened on demand.
    assert!(!text.contains("Details"), "{text}");
}

#[test]
fn project_creation_pins_the_basic_workflow_by_default() {
    let mut app = app();
    submit_project(&mut app, "Apollo");
    assert_eq!(app.screen, Screen::Project);
    let project = app.project.as_ref().unwrap();
    assert_eq!(project.name, "Apollo");
    assert!(
        !project.generated,
        "a new project should carry the pinned Basic workflow, not generated defaults"
    );
    assert!(
        project.views.iter().any(|view| view.id == "overview"),
        "Basic declares an overview view"
    );
    assert!(
        project.blueprint_label.to_lowercase().contains("basic"),
        "label was {}",
        project.blueprint_label
    );
    let text = buffer_text(&mut app, 120, 30);
    assert!(text.contains("Basic"), "{text}");
}

#[test]
fn unbound_project_falls_back_to_generated_views() {
    let mut app = app();
    submit_project(&mut app, "Legacy");
    let project_id = app.project.as_ref().unwrap().id.clone();
    // Simulate a legacy workspace: the binding is removed while the project
    // stays fully usable through generated default views.
    app.client
        .remove_project_workflow(project_id.clone())
        .unwrap();
    app.refresh_project();
    let project = app.project.as_ref().unwrap();
    assert!(project.generated);
    assert!(project.views.iter().any(|view| view.id == "overview"));
    let text = buffer_text(&mut app, 120, 30);
    assert!(text.contains("generated default"), "{text}");
}

#[test]
fn research_blueprint_work_item_offers_authorized_transition() {
    let mut app = app();
    submit_project(&mut app, "Research");
    let project_id = app.project.as_ref().unwrap().id.clone();
    app.client.install_bundled_blueprint("research").unwrap();
    app.client
        .apply_workflow(project_id.clone(), "research".to_owned(), None)
        .unwrap();
    app.refresh_project();
    let item = app
        .client
        .create_blueprint_work_item(
            &project_id,
            "hypothesis",
            std::collections::BTreeMap::from([(
                "question".into(),
                serde_json::json!("Does M28 resolve?"),
            )]),
        )
        .unwrap();
    app.refresh_project();
    let board = app
        .project
        .as_ref()
        .unwrap()
        .views
        .iter()
        .position(|view| view.id == "workflow")
        .unwrap();
    app.switch_view_by_index(board);
    let component = app.project.as_ref().unwrap().views[board]
        .components
        .iter()
        .position(|component| component.id == "board")
        .unwrap();
    app.active_component = component;
    app.selected = app
        .current_component()
        .unwrap()
        .selection
        .iter()
        .position(|row| row.id == item.id)
        .unwrap();
    let action = app
        .available_actions()
        .into_iter()
        .find(|action| {
            action.id == "blueprint_transition"
                && action.payload.as_deref() == Some("investigating")
        })
        .expect("transition offer");
    app.execute_action(action);
    let updated = app.client.list_blueprint_work_items(&project_id).unwrap();
    assert_eq!(updated[0].state, "investigating");
}

#[test]
fn narrow_terminals_render_a_single_readable_pane() {
    let mut app = app();
    app.focus = FocusPane::Workspace;
    let text = buffer_text(&mut app, 60, 18);
    assert!(
        text.contains("Today / Next") || text.contains("Home"),
        "{text}"
    );
    assert!(
        !text.contains("Details"),
        "the shell must never reserve a detail pane: {text}"
    );
}

#[test]
fn missing_component_keeps_the_rest_of_the_view() {
    let mut app = app();
    app.home.components.insert(
        0,
        orbit_ui::ResolvedComponent {
            id: "unavailable".into(),
            title: "Unavailable".into(),
            component_type: "unknown_extension".into(),
            status: orbit_ui::LoadStatus::Missing("not installed".into()),
            body: orbit_ui::ComponentBody::Missing {
                component_type: "unknown_extension".into(),
                reason: "Install the extension that provides it.".into(),
            },
            selection: Vec::new(),
        },
    );
    let text = buffer_text(&mut app, 100, 26);
    assert!(text.contains("Missing component"), "{text}");
    assert!(text.contains("Today / Next"), "{text}");
}

#[test]
fn palette_and_search_filter_without_losing_modal_cancel() {
    let mut app = app();
    app.open_palette();
    assert!(matches!(app.modal, Some(Modal::Palette { .. })));
    handle_key(&mut app, KeyCode::Esc, KeyModifiers::NONE).unwrap();
    assert!(app.modal.is_none());
    app.open_search();
    handle_key(&mut app, KeyCode::Char('i'), KeyModifiers::NONE).unwrap();
    assert!(matches!(app.modal, Some(Modal::Search { .. })));
}

#[test]
fn project_selection_enter_opens_project() {
    let mut app = app();
    submit_project(&mut app, "Apollo");
    handle_key(&mut app, KeyCode::Esc, KeyModifiers::NONE).unwrap();
    handle_key(&mut app, KeyCode::Char('g'), KeyModifiers::NONE).unwrap();
    handle_key(&mut app, KeyCode::Char('p'), KeyModifiers::NONE).unwrap();
    assert_eq!(app.screen, Screen::Projects);
    app.focus = FocusPane::Workspace;
    app.projects_selection = 0;
    handle_key(&mut app, KeyCode::Enter, KeyModifiers::NONE).unwrap();
    assert_eq!(app.screen, Screen::Project);
    assert_eq!(app.project.as_ref().unwrap().name, "Apollo");
}

#[test]
fn duplicate_record_submission_is_ignored_until_refresh() {
    let mut app = app();
    submit_project(&mut app, "Habits");
    let project_id = app.project.as_ref().unwrap().id.clone();
    let habit = app
        .client
        .create_habit(orbit_client::CreateHabitRequest {
            id: None,
            name: "Stretch".into(),
            cadence: "daily".into(),
        })
        .unwrap();
    app.refresh_project();
    let pending = PendingAction::RecordHabit {
        habit_id: habit.id.clone(),
    };
    app.execute_pending(pending.clone());
    app.execute_pending(pending);
    let occurrences = app.client.habit_history(habit.id.clone()).unwrap();
    assert_eq!(occurrences.len(), 1);
    assert!(app.status.text.contains("Duplicate"), "{}", app.status.text);
    let _ = project_id;
}

#[test]
fn focus_cycles_between_the_two_painted_panes() {
    let mut app = app();
    app.set_viewport(120);
    app.focus = FocusPane::Workspace;
    app.focus_next(false);
    assert_eq!(app.focus, FocusPane::Sidebar);
    app.focus_next(false);
    assert_eq!(app.focus, FocusPane::Workspace);
    app.focus_next(true);
    assert_eq!(app.focus, FocusPane::Sidebar);
}

#[test]
fn q_quits_from_any_non_text_screen() {
    let mut app = app();
    submit_project(&mut app, "Apollo");
    assert!(handle_key(&mut app, KeyCode::Char('q'), KeyModifiers::NONE).unwrap());
    assert_eq!(app.screen, Screen::Project);
    let mut home = self::app();
    assert!(handle_key(&mut home, KeyCode::Char('q'), KeyModifiers::NONE).unwrap());
}

#[test]
fn ctrl_quit_keeps_unsaved_prompt_until_confirmed() {
    let mut app = app();
    app.start_form(FormMode::NewTask { project_id: None });
    let Some(Modal::Form(mut form)) = app.modal.take() else {
        panic!("form");
    };
    form.fields[0].value = "unsaved capture".into();
    form.dirty = true;
    app.modal = Some(Modal::Form(form));
    assert!(!app.request_quit());
    assert!(matches!(app.modal, Some(Modal::QuitConfirm { .. })));
    handle_key(&mut app, KeyCode::Esc, KeyModifiers::NONE).unwrap();
    let Some(Modal::Form(form)) = &app.modal else {
        panic!("form restored");
    };
    assert_eq!(form.fields[0].value, "unsaved capture");
}

#[test]
fn palette_actions_share_the_contextual_catalogue() {
    let mut app = app();
    submit_project(&mut app, "Apollo");
    app.open_palette();
    let Some(Modal::Palette { entries, .. }) = &app.modal else {
        panic!("palette");
    };
    assert!(entries.iter().any(|entry| entry.label.contains("New task")));
    assert!(
        entries
            .iter()
            .any(|entry| matches!(entry.action, PaletteAction::Go(Screen::Home)))
    );
}
