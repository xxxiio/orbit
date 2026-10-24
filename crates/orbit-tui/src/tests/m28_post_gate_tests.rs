use crate::app::{FormMode, Modal};
use crate::*;

fn app() -> App {
    let filename = std::env::temp_dir().join(format!(
        "orbit-tui-m28-gate-{}-{}.sqlite",
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
fn q_is_text_inside_forms_and_never_quits_or_discards() {
    let mut app = app();
    app.start_form(FormMode::NewTask { project_id: None });
    assert!(!handle_key(&mut app, KeyCode::Char('a'), KeyModifiers::NONE).unwrap());
    assert!(!handle_key(&mut app, KeyCode::Char('b'), KeyModifiers::NONE).unwrap());
    assert!(!handle_key(&mut app, KeyCode::Char('q'), KeyModifiers::NONE).unwrap());
    let Some(Modal::Form(form)) = &app.modal else {
        panic!("form should remain open");
    };
    assert_eq!(form.fields[0].value, "abq");
    assert!(form.dirty);
    assert!(!app.request_quit());
    assert!(matches!(app.modal, Some(Modal::QuitConfirm { .. })));
    handle_key(&mut app, KeyCode::Char('n'), KeyModifiers::NONE).unwrap();
    let Some(Modal::Form(form)) = &app.modal else {
        panic!("form should be restored");
    };
    assert_eq!(form.fields[0].value, "abq");
}
