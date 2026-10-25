use crate::app::{FocusPane, Modal, Screen};
use crate::*;
use ratatui::{Terminal, backend::TestBackend};

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

fn app() -> App {
    let path = std::env::temp_dir().join(format!(
        "orbit-tui-m28-scroll2-{}-{}.sqlite",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let _ = std::fs::remove_file(&path);
    App::open(path.to_string_lossy().into_owned()).unwrap()
}

#[test]
fn palette_selection_stays_visible_at_60x18() {
    let mut app = app();
    for index in 0..8 {
        app.client
            .create_project(orbit_client::CreateProjectRequest {
                id: None,
                name: format!("Palette project {index}"),
            })
            .unwrap();
    }
    app.refresh_workspace();
    app.open_palette();
    for _ in 0..100 {
        handle_key(&mut app, KeyCode::Down, KeyModifiers::NONE).unwrap();
    }
    let rendered = text(&mut app, 60, 18);
    assert!(rendered.contains("Quit"), "{rendered}");
    assert!(rendered.contains("▸ Quit"), "{rendered}");
}

#[test]
fn sidebar_selection_scrolls_into_view() {
    let mut app = app();
    for index in 0..30 {
        app.client
            .create_project(orbit_client::CreateProjectRequest {
                id: None,
                name: format!("Project {index}"),
            })
            .unwrap();
    }
    app.refresh_workspace();
    app.screen = Screen::Home;
    app.focus = FocusPane::Sidebar;
    app.sidebar_selection = app.sidebar_len() - 1;
    let rendered = text(&mut app, 60, 18);
    assert!(rendered.contains("Project 29"), "{rendered}");
    assert!(
        rendered
            .lines()
            .any(|line| line.contains("▸") && line.contains("Project 29")),
        "{rendered}"
    );
}

#[test]
fn help_modal_scrolls_to_later_lines() {
    let mut app = app();
    app.modal = Some(Modal::Help);
    let before = text(&mut app, 60, 18);
    assert!(!before.contains("Ctrl+Q / Ctrl+C"), "{before}");
    for _ in 0..4 {
        handle_key(&mut app, KeyCode::PageDown, KeyModifiers::NONE).unwrap();
    }
    let after = text(&mut app, 60, 18);
    assert!(after.contains("Ctrl+Q / Ctrl+C"), "{after}");
}

/// The inspector owns its own scroll; cycling focus or closing it never moves
/// the workspace list offset.
#[test]
fn inspector_scroll_is_independent_of_focus_and_the_list() {
    let mut app = app();
    app.focus = FocusPane::Workspace;
    app.scroll = 7;
    app.focus_next(false);
    assert_eq!(app.focus, FocusPane::Sidebar);
    app.focus_next(true);
    assert_eq!(app.focus, FocusPane::Workspace);
    assert_eq!(app.scroll, 7, "focus must not reset the list scroll");
}
