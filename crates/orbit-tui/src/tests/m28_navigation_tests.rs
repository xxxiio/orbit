use crate::app::{FocusPane, FormMode, Modal, Screen};
use crate::*;
use ratatui::backend::TestBackend;

fn app() -> App {
    let filename = std::env::temp_dir().join(format!(
        "orbit-tui-m28-nav-{}-{}.sqlite",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let _ = std::fs::remove_file(&filename);
    App::open(filename.to_string_lossy().into_owned()).unwrap()
}

fn add_project(app: &mut App, name: &str) -> String {
    let project = app
        .client
        .create_project(orbit_client::CreateProjectRequest {
            id: None,
            name: name.into(),
        })
        .unwrap();
    app.refresh_workspace();
    project.id
}

fn add_task(app: &mut App, project_id: &str, title: &str) -> String {
    let task = app
        .client
        .create_task(orbit_client::CreateTaskRequest {
            id: None,
            title: title.into(),
            project_id: Some(project_id.into()),
            milestone_id: None,
        })
        .unwrap();
    app.refresh_workspace();
    task.id
}

#[test]
fn back_navigation_restores_the_previous_screen_and_selection() {
    let mut app = app();
    add_project(&mut app, "First");
    add_project(&mut app, "Second");
    app.go_screen(Screen::Projects);
    app.focus = FocusPane::Workspace;
    app.projects_selection = 1;
    handle_key(&mut app, KeyCode::Enter, KeyModifiers::NONE).unwrap();
    assert_eq!(app.screen, Screen::Project);
    handle_key(&mut app, KeyCode::Esc, KeyModifiers::NONE).unwrap();
    assert_eq!(app.screen, Screen::Projects);
    assert_eq!(app.projects_selection, 1);
    handle_key(&mut app, KeyCode::Esc, KeyModifiers::NONE).unwrap();
    assert_eq!(app.screen, Screen::Home);
}

#[test]
fn switching_views_restores_each_views_component_and_selection() {
    let mut app = app();
    let project = add_project(&mut app, "Work");
    app.client.install_bundled_blueprint("development").unwrap();
    app.client
        .apply_workflow(project.clone(), "development".to_owned(), None)
        .unwrap();
    for title in ["One", "Two", "Three"] {
        add_task(&mut app, &project, title);
    }
    app.open_project(&project);
    let overview = app
        .project
        .as_ref()
        .unwrap()
        .views
        .iter()
        .position(|view| view.id == "overview")
        .unwrap();
    let board = app
        .project
        .as_ref()
        .unwrap()
        .views
        .iter()
        .position(|view| view.id == "board")
        .unwrap();
    app.switch_view_by_index(overview);
    let component = app.project.as_ref().unwrap().views[overview]
        .components
        .iter()
        .position(|component| component.selection.len() >= 3)
        .unwrap();
    app.active_component = component;
    app.selected = 2;
    app.scroll = 1;
    app.switch_view_by_index(board);
    let board_component = app.project.as_ref().unwrap().views[board]
        .components
        .iter()
        .position(|component| component.selection.len() >= 3)
        .unwrap();
    app.active_component = board_component;
    app.selected = 1;
    app.switch_view_by_index(overview);
    assert_eq!(app.active_component, component);
    assert_eq!(app.selected, 2);
    assert_eq!(app.scroll, 1);
    app.switch_view_by_index(board);
    assert_eq!(app.active_component, board_component);
    assert_eq!(app.selected, 1);
}

#[test]
fn back_restores_the_same_project_view_by_stable_id() {
    let mut app = app();
    let project = add_project(&mut app, "View restore");
    app.client.install_bundled_blueprint("development").unwrap();
    app.client
        .apply_workflow(project.clone(), "development".to_owned(), None)
        .unwrap();
    app.open_project(&project);
    let board = app
        .project
        .as_ref()
        .unwrap()
        .views
        .iter()
        .position(|view| view.id == "board")
        .unwrap();
    app.switch_view_by_index(board);
    app.go_screen(Screen::Projects);
    app.go_back();
    assert_eq!(app.screen, Screen::Project);
    assert_eq!(
        app.project.as_ref().unwrap().views[app.project.as_ref().unwrap().active_view].id,
        "board"
    );
}

#[test]
fn switching_components_restores_each_components_selection() {
    let mut app = app();
    let project = add_project(&mut app, "Work");
    app.client.install_bundled_blueprint("research").unwrap();
    app.client
        .apply_workflow(project.clone(), "research".to_owned(), None)
        .unwrap();
    for question in ["One?", "Two?", "Three?"] {
        app.client
            .create_blueprint_work_item(
                &project,
                "hypothesis",
                std::collections::BTreeMap::from([(
                    "question".into(),
                    serde_json::json!(question),
                )]),
            )
            .unwrap();
    }
    app.open_project(&project);
    let workflow = app
        .project
        .as_ref()
        .unwrap()
        .views
        .iter()
        .position(|view| view.id == "workflow")
        .unwrap();
    app.switch_view_by_index(workflow);
    let view = app.project.as_ref().unwrap().active_view;
    let components: Vec<usize> = app.project.as_ref().unwrap().views[view]
        .components
        .iter()
        .enumerate()
        .filter(|(_, component)| component.selection.len() >= 3)
        .map(|(index, _)| index)
        .collect();
    assert!(components.len() >= 2, "need two selectable components");
    let (first, second) = (components[0], components[1]);
    app.active_component = first;
    app.selected = 2;
    app.scroll = 1;
    app.move_component((second as isize) - (first as isize));
    assert_eq!(app.active_component, second);
    assert_eq!(app.selected, 0);
    app.selected = 1;
    app.move_component((first as isize) - (second as isize));
    assert_eq!(app.active_component, first);
    assert_eq!(app.selected, 2);
    assert_eq!(app.scroll, 1);
}

#[test]
fn quit_confirmation_renders_and_can_be_cancelled() {
    let mut app = app();
    app.start_form(FormMode::NewTask { project_id: None });
    let Some(Modal::Form(mut form)) = app.modal.take() else {
        panic!("form");
    };
    form.fields[0].value = "draft".into();
    form.dirty = true;
    app.modal = Some(Modal::Form(form));
    assert!(!app.request_quit());
    let backend = TestBackend::new(100, 30);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| render::draw(frame, &mut app))
        .unwrap();
    let buffer = terminal.backend().buffer();
    let text = (0..buffer.area.height)
        .map(|y| {
            (0..buffer.area.width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert!(text.contains("Quit Orbit?"), "{text}");
    handle_key(&mut app, KeyCode::Esc, KeyModifiers::NONE).unwrap();
    assert!(matches!(app.modal, Some(Modal::Form(_))));
}
