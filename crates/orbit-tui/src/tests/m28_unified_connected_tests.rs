use crate::app::{FormMode, Modal, Screen};
use crate::*;

#[test]
fn connected_project_objective_is_rejected_before_queueing_anything() {
    let path = std::env::temp_dir().join(format!(
        "orbit-tui-m28-connected-objective-{}-{}.db",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let backend =
        crate::backend::Backend::open_connected_with_identity(&path, "https://example.org", None)
            .unwrap();
    let mut app = App::open_with_backend(backend).unwrap();
    app.start_form(FormMode::NewProject);
    let Some(Modal::Form(mut form)) = app.modal.take() else {
        panic!("form");
    };
    form.fields[0].value = "No orphan".into();
    form.fields[1].value = "Unsupported objective".into();
    form.dirty = true;
    app.submit_form(form);
    assert!(matches!(app.modal, Some(Modal::Form(_))));
    assert_eq!(app.client.pending_count(), 0);
    let _ = std::fs::remove_file(&path);
}

#[test]
fn connected_mode_uses_the_shared_shell_and_never_claims_pending_acceptance() {
    let path = std::env::temp_dir().join(format!(
        "orbit-tui-m28-connected-{}-{}.db",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let backend =
        crate::backend::Backend::open_connected_with_identity(&path, "https://example.org", None)
            .unwrap();
    let mut app = App::open_with_backend(backend).unwrap();
    assert_eq!(app.screen, Screen::Home);
    assert!(app.client.is_connected());
    let backend = ratatui::backend::TestBackend::new(120, 30);
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
    assert!(rendered.contains("CONNECTED"), "{rendered}");

    app.screen = Screen::Agents;
    let mut terminal = Terminal::new(ratatui::backend::TestBackend::new(120, 35)).unwrap();
    terminal
        .draw(|frame| render::draw(frame, &mut app))
        .unwrap();
    let agents: String = (0..terminal.backend().buffer().area.height)
        .map(|y| {
            (0..terminal.backend().buffer().area.width)
                .map(|x| terminal.backend().buffer()[(x, y)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        agents.contains("not") && agents.contains("projected"),
        "{agents}"
    );
    assert!(!agents.contains("No agent jobs"), "{agents}");
    app.screen = Screen::Home;

    app.start_form(FormMode::NewProject);
    let Some(Modal::Form(mut form)) = app.modal.take() else {
        panic!("form");
    };
    form.fields[0].value = "Queued project".into();
    form.dirty = true;
    app.submit_form(form);
    assert!(
        app.projects.is_empty(),
        "pending project must not appear accepted"
    );
    assert!(
        app.status.text.to_lowercase().contains("pending"),
        "{}",
        app.status.text
    );
    assert!(app.request_quit());
    let _ = std::fs::remove_file(&path);
}
