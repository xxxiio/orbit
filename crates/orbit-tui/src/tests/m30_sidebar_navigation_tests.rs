//! M30 follow-up: the navigation rail must drive the workspace.
//!
//! Regression tests for the reported defect: on the Projects screen `Enter`
//! ignored the focused sidebar row (it always opened the workspace's selected
//! project row), and opening a project from the rail dropped focus into the
//! workspace, so further rail movement silently did nothing and the right pane
//! looked stuck on the project list.

use crate::app::{App, FocusPane, Screen};
use crate::input::handle_key;
use crossterm::event::{KeyCode, KeyModifiers};

fn app(stem: &str) -> App {
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

fn two_projects(stem: &str) -> (App, String, String) {
    let mut app = app(stem);
    let first = app
        .client
        .create_project(orbit_client::CreateProjectRequest {
            id: None,
            name: "Alpha Project".into(),
        })
        .unwrap();
    let second = app
        .client
        .create_project(orbit_client::CreateProjectRequest {
            id: None,
            name: "Beta Project".into(),
        })
        .unwrap();
    app.refresh_workspace();
    (app, first.id, second.id)
}

fn press(app: &mut App, code: KeyCode) {
    handle_key(app, code, KeyModifiers::NONE).unwrap();
}

#[test]
fn enter_on_projects_screen_activates_the_focused_sidebar_row() {
    let (mut app, _, _) = two_projects("m30-rail-home");
    app.go_screen(Screen::Projects);
    app.focus = FocusPane::Sidebar;
    app.sidebar_selection = 0; // Home / Today

    press(&mut app, KeyCode::Enter);

    assert_eq!(app.screen, Screen::Home);
    assert_eq!(app.focus, FocusPane::Sidebar);
    assert_eq!(app.sidebar_selection, 0);

    // Inbox and Agents work the same way from the Projects screen.
    app.go_screen(Screen::Projects);
    app.focus = FocusPane::Sidebar;
    app.sidebar_selection = 1;
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.screen, Screen::Inbox);

    app.go_screen(Screen::Projects);
    app.focus = FocusPane::Sidebar;
    app.sidebar_selection = 3;
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.screen, Screen::Agents);
}

#[test]
fn opening_a_project_from_the_rail_hands_the_keyboard_to_the_workspace() {
    let (mut app, first, second) = two_projects("m30-rail-projects");
    app.go_screen(Screen::Projects);
    app.focus = FocusPane::Sidebar;
    app.sidebar_selection = 4; // first project row

    press(&mut app, KeyCode::Enter);
    assert_eq!(app.screen, Screen::Project);
    assert_eq!(
        app.project.as_ref().map(|project| project.id.as_str()),
        Some(first.as_str())
    );
    // Opening the project is a move into the content, as documented.
    assert_eq!(app.focus, FocusPane::Workspace);

    // Returning to the rail and moving it must switch projects, not silently
    // move the workspace cursor.
    press(&mut app, KeyCode::BackTab);
    assert_eq!(app.focus, FocusPane::Sidebar);
    press(&mut app, KeyCode::Char('j'));
    assert_eq!(app.sidebar_selection, 5);
    press(&mut app, KeyCode::Enter);
    assert_eq!(
        app.project.as_ref().map(|project| project.id.as_str()),
        Some(second.as_str())
    );
}

#[test]
fn workspace_enter_on_projects_screen_still_opens_the_selected_row() {
    let (mut app, first, _) = two_projects("m30-rail-workspace");
    app.go_screen(Screen::Projects);
    app.focus = FocusPane::Workspace;
    app.projects_selection = 0;

    press(&mut app, KeyCode::Enter);

    assert_eq!(app.screen, Screen::Project);
    assert_eq!(
        app.project.as_ref().map(|project| project.id.as_str()),
        Some(first.as_str())
    );
    // Opening from the workspace keeps the workspace focused, as before.
    assert_eq!(app.focus, FocusPane::Workspace);
}

#[test]
fn enter_in_the_workspace_still_opens_the_inspector() {
    let (mut app, _, _) = two_projects("m30-rail-inspector");
    app.go_screen(Screen::Projects);
    app.focus = FocusPane::Workspace;
    app.projects_selection = 0;

    press(&mut app, KeyCode::Char('D'));
    assert!(matches!(app.modal, Some(crate::app::Modal::Inspector(_))));
}
