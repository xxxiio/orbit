use crate::app::{FocusPane, Modal, Screen};
use crate::*;

fn app() -> App {
    let filename = std::env::temp_dir().join(format!(
        "orbit-tui-m28-scroll-{}-{}.sqlite",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let _ = std::fs::remove_file(&filename);
    App::open(filename.to_string_lossy().into_owned()).unwrap()
}

#[test]
fn text_component_scroll_survives_navigation_and_refresh() {
    let mut app = app();
    let component = app
        .home
        .components
        .iter()
        .position(|component| component.selection.is_empty())
        .expect("text component");
    app.screen = Screen::Home;
    app.active_component = component;
    app.selected = 0;
    app.scroll = 4;
    app.go_screen(Screen::Inbox);
    app.go_back();
    assert_eq!(app.active_component, component);
    assert_eq!(app.scroll, 4);
    app.refresh_workspace();
    assert_eq!(app.scroll, 4);
}

#[test]
fn palette_quit_with_query_uses_the_single_confirmation_path() {
    let mut app = app();
    app.open_palette();
    for character in "quit".chars() {
        handle_key(&mut app, KeyCode::Char(character), KeyModifiers::NONE).unwrap();
    }
    let Some(Modal::Palette {
        entries, selected, ..
    }) = &mut app.modal
    else {
        panic!("palette");
    };
    *selected = entries
        .iter()
        .position(|entry| matches!(entry.action, app::PaletteAction::Quit))
        .expect("quit entry");
    let result = handle_key(&mut app, KeyCode::Enter, KeyModifiers::NONE).unwrap();
    assert!(!result);
    assert!(matches!(app.modal, Some(Modal::QuitConfirm { .. })));
    let _ = FocusPane::Workspace;
}
