use crate::app::{FocusPane, Screen};
use crate::*;
use ratatui::{Terminal, backend::TestBackend};

fn app() -> App {
    let filename = std::env::temp_dir().join(format!(
        "orbit-tui-m28-render-{}-{}.sqlite",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let _ = std::fs::remove_file(&filename);
    App::open(filename.to_string_lossy().into_owned()).unwrap()
}

fn text(app: &mut App, width: u16, height: u16) -> String {
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

fn project_with_development(app: &mut App) -> String {
    let project = app
        .client
        .create_project(orbit_client::CreateProjectRequest {
            id: None,
            name: "Apollo".into(),
        })
        .unwrap();
    app.client.install_bundled_blueprint("development").unwrap();
    app.client
        .apply_workflow(project.id.clone(), "development".to_owned(), None)
        .unwrap();
    app.refresh_workspace();
    app.open_project(&project.id);
    project.id
}

#[test]
fn overview_renders_live_metric_cards_and_task_rows() {
    let mut app = app();
    let project_id = project_with_development(&mut app);
    app.client
        .create_task(orbit_client::CreateTaskRequest {
            id: None,
            title: "Ship the shell".into(),
            project_id: Some(project_id.clone()),
            milestone_id: None,
        })
        .unwrap();
    let task = app
        .client
        .create_task(orbit_client::CreateTaskRequest {
            id: None,
            title: "Ready work".into(),
            project_id: Some(project_id),
            milestone_id: None,
        })
        .unwrap();
    app.client
        .set_task_status(orbit_client::SetTaskStatusRequest {
            id: task.id,
            status: "ready".into(),
        })
        .unwrap();
    app.refresh_project();
    let rendered = text(&mut app, 140, 40);
    assert!(rendered.contains("Ready tasks"), "{rendered}");
    assert!(rendered.contains("Ready work"), "{rendered}");
    assert!(rendered.contains("Ship the shell"), "{rendered}");
}

#[test]
fn board_renders_workflow_columns_and_selection() {
    let mut app = app();
    project_with_development(&mut app);
    let board = app
        .project
        .as_ref()
        .unwrap()
        .views
        .iter()
        .position(|view| view.id == "board")
        .unwrap();
    app.switch_view_by_index(board);
    let rendered = text(&mut app, 140, 40);
    for column in ["Inbox", "Planned", "Ready", "In progress"] {
        assert!(rendered.contains(column), "missing {column}: {rendered}");
    }
}

#[test]
fn stale_and_error_states_are_visible() {
    let mut app = app();
    let project_id = project_with_development(&mut app);
    app.project.as_mut().unwrap().stale_error = Some("server unavailable".into());
    let rendered = text(&mut app, 120, 30);
    assert!(rendered.contains("[stale]"), "{rendered}");
    app.home.components.insert(
        0,
        orbit_ui::ResolvedComponent {
            id: "broken".into(),
            title: "Broken".into(),
            component_type: "list".into(),
            status: orbit_ui::LoadStatus::Error("bad binding".into()),
            body: orbit_ui::ComponentBody::Text {
                heading: "Broken".into(),
                lines: vec!["Invalid configuration".into()],
            },
            selection: Vec::new(),
        },
    );
    app.go_screen(Screen::Home);
    let rendered = text(&mut app, 120, 30);
    assert!(rendered.contains("error: bad binding"), "{rendered}");
    let _ = project_id;
}

#[test]
fn loading_component_keeps_context_and_never_looks_authoritative() {
    let mut app = app();
    app.home.components.insert(
        0,
        orbit_ui::ResolvedComponent {
            id: "loading".into(),
            title: "Loading".into(),
            component_type: "list".into(),
            status: orbit_ui::LoadStatus::Loading,
            body: orbit_ui::ComponentBody::Text {
                heading: "Loading".into(),
                lines: Vec::new(),
            },
            selection: Vec::new(),
        },
    );
    let rendered = text(&mut app, 120, 30);
    assert!(rendered.contains("Loading accepted data"), "{rendered}");
    assert!(rendered.contains("previous view remains"), "{rendered}");
    assert!(rendered.contains("resolution"), "{rendered}");
}

#[test]
fn table_renderer_handles_narrow_and_wide_terminals() {
    let mut app = app();
    app.home.components = vec![orbit_ui::ResolvedComponent {
        id: "table".into(),
        title: "Table".into(),
        component_type: "table".into(),
        status: orbit_ui::LoadStatus::Ready,
        body: orbit_ui::ComponentBody::Table {
            columns: vec!["title".into(), "status".into(), "estimate".into()],
            rows: vec![orbit_ui::model::TableRowModel {
                id: "t1".into(),
                kind: orbit_ui::ItemKind::Task,
                status: "done".into(),
                cells: vec![
                    "A long task title that should be truncated".into(),
                    "done".into(),
                    "30m".into(),
                ],
            }],
        },
        selection: Vec::new(),
    }];
    app.focus = FocusPane::Workspace;
    let wide = text(&mut app, 120, 24);
    assert!(wide.contains("TITLE"), "{wide}");
    assert!(wide.contains("STATUS"), "{wide}");
    assert!(wide.contains("ESTIMATE"), "{wide}");
    assert!(wide.contains("A long task title"), "{wide}");
    assert!(wide.contains("✓"), "{wide}");
    let narrow = text(&mut app, 60, 16);
    assert!(narrow.contains("TITLE"), "{narrow}");
    assert!(narrow.contains("STATUS"), "{narrow}");
}

#[test]
fn tab_focus_and_key_prefix_do_not_collide() {
    let mut app = app();
    app.set_viewport(120);
    app.focus = FocusPane::Workspace;
    handle_key(&mut app, KeyCode::Char('g'), KeyModifiers::NONE).unwrap();
    assert_eq!(app.prefix, Some('g'));
    handle_key(&mut app, KeyCode::Char('a'), KeyModifiers::NONE).unwrap();
    assert_eq!(app.screen, Screen::Agents);
    assert!(app.prefix.is_none());
    handle_key(&mut app, KeyCode::Tab, KeyModifiers::NONE).unwrap();
    assert_eq!(app.focus, FocusPane::Detail);
    handle_key(&mut app, KeyCode::Tab, KeyModifiers::NONE).unwrap();
    assert_eq!(app.focus, FocusPane::Sidebar);
    handle_key(&mut app, KeyCode::BackTab, KeyModifiers::NONE).unwrap();
    assert_eq!(app.focus, FocusPane::Detail);
}

#[test]
fn project_switch_reloads_views_without_reusing_previous_project_data() {
    let mut app = app();
    let first = app
        .client
        .create_project(orbit_client::CreateProjectRequest {
            id: None,
            name: "First".into(),
        })
        .unwrap();
    let second = app
        .client
        .create_project(orbit_client::CreateProjectRequest {
            id: None,
            name: "Second".into(),
        })
        .unwrap();
    app.refresh_workspace();
    app.go_screen(Screen::Projects);
    app.projects_selection = app
        .projects
        .iter()
        .position(|project| project.id == first.id)
        .unwrap();
    handle_key(&mut app, KeyCode::Enter, KeyModifiers::NONE).unwrap();
    assert_eq!(app.project.as_ref().unwrap().id, first.id);
    handle_key(&mut app, KeyCode::Esc, KeyModifiers::NONE).unwrap();
    app.go_screen(Screen::Projects);
    app.projects_selection = app
        .projects
        .iter()
        .position(|project| project.id == second.id)
        .unwrap();
    handle_key(&mut app, KeyCode::Enter, KeyModifiers::NONE).unwrap();
    assert_eq!(app.project.as_ref().unwrap().id, second.id);
}

#[test]
fn tui_sources_do_not_open_storage_or_spawn_commands() {
    for source in [
        include_str!("../app/mod.rs"),
        include_str!("../app/state.rs"),
        include_str!("../app/navigation.rs"),
        include_str!("../app/selection.rs"),
        include_str!("../app/actions.rs"),
        include_str!("../app/forms.rs"),
        include_str!("../app/connected.rs"),
        include_str!("../render/mod.rs"),
        include_str!("../render/layout.rs"),
        include_str!("../render/shell.rs"),
        include_str!("../render/components.rs"),
        include_str!("../render/modals.rs"),
        include_str!("../theme.rs"),
    ] {
        assert!(!source.contains("rusqlite"));
        assert!(!source.contains("Command::new"));
        assert!(!source.contains("std::process::Command"));
        assert!(!source.contains("SELECT "));
        assert!(!source.contains("INSERT INTO"));
    }
}
