use crate::app::{FocusPane, FormMode, Modal, Screen};
use crate::*;
use ratatui::backend::TestBackend;

fn app() -> App {
    let filename = std::env::temp_dir().join(format!(
        "orbit-tui-m28-quit-{}-{}.sqlite",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let _ = std::fs::remove_file(&filename);
    App::open(filename.to_string_lossy().into_owned()).unwrap()
}

fn selectable(app: &mut App) -> bool {
    app.current_component()
        .map(|component| !component.selection.is_empty())
        .unwrap_or(false)
        || matches!(app.screen, Screen::Projects | Screen::Home)
}

#[test]
fn q_quits_from_every_global_screen() {
    for screen in [
        Screen::Home,
        Screen::Inbox,
        Screen::Projects,
        Screen::Agents,
    ] {
        let mut app = app();
        app.go_screen(screen);
        assert!(
            handle_key(&mut app, KeyCode::Char('q'), KeyModifiers::NONE).unwrap(),
            "{screen:?}"
        );
    }
    let mut app = app();
    app.client
        .create_project(orbit_client::CreateProjectRequest {
            id: None,
            name: "Quit".into(),
        })
        .unwrap();
    app.refresh_workspace();
    app.open_project(&app.projects[0].id.clone());
    assert_eq!(app.screen, Screen::Project);
    assert!(handle_key(&mut app, KeyCode::Char('q'), KeyModifiers::NONE).unwrap());
}

#[test]
fn q_quits_non_text_modals_and_enters_text_in_text_fields() {
    let mut app = app();
    app.open_palette();
    for _ in 0..3 {
        handle_key(&mut app, KeyCode::Char('q'), KeyModifiers::NONE).unwrap();
    }
    let Some(Modal::Palette { query, .. }) = &app.modal else {
        panic!("palette");
    };
    assert_eq!(query, "qqq");

    app.modal = Some(Modal::Help);
    // `q` closes help instead of quitting so the user cannot lose context.
    assert!(!handle_key(&mut app, KeyCode::Char('q'), KeyModifiers::NONE).unwrap());
    assert!(app.modal.is_none());

    app.modal = Some(Modal::Message {
        title: "Notice".into(),
        lines: vec!["text".into()],
    });
    assert!(handle_key(&mut app, KeyCode::Char('q'), KeyModifiers::NONE).unwrap());

    app.modal = Some(Modal::Confirm {
        title: "Change".into(),
        body: "body".into(),
        pending: app::PendingAction::ApplyPlan,
    });
    assert!(handle_key(&mut app, KeyCode::Char('q'), KeyModifiers::NONE).unwrap());
}

#[test]
fn quit_confirmation_appears_for_unsaved_text_and_exposes_both_choices() {
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
    let rendered: String = (0..terminal.backend().buffer().area.height)
        .map(|y| {
            (0..terminal.backend().buffer().area.width)
                .map(|x| terminal.backend().buffer()[(x, y)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert!(rendered.contains("Quit Orbit?"), "{rendered}");
    assert!(rendered.contains("Ctrl+Q"), "{rendered}");
    assert!(rendered.contains("Esc or n"), "{rendered}");
    handle_key(&mut app, KeyCode::Esc, KeyModifiers::NONE).unwrap();
    assert!(matches!(app.modal, Some(Modal::Form(_))));
    assert!(!app.request_quit());
    assert!(matches!(app.modal, Some(Modal::QuitConfirm { .. })));
    assert!(handle_key(&mut app, KeyCode::Char('y'), KeyModifiers::NONE).unwrap());
}

#[test]
fn quit_is_independent_of_selection_and_focus() {
    let mut app = app();
    app.focus = FocusPane::Detail;
    app.selected = 99;
    assert!(app.request_quit());
    app.focus = FocusPane::Sidebar;
    app.selected = 0;
    assert!(app.request_quit());
    let _ = selectable(&mut app);
}
