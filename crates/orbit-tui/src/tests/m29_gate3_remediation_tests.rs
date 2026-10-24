//! Gate-3 remediation regressions that need no authority server.
//!
//! The docked detail pane caches its model, so every mutation that changes a
//! shown count must go through `App` and invalidate that cache. These tests are
//! the accepted-data contract for that cache.
use crate::app::{FormMode, Modal, PendingAction, Screen};
use crate::*;

fn project_app(stem: &str) -> (App, String) {
    let path = std::env::temp_dir().join(format!(
        "orbit-tui-gate3-{stem}-{}-{}.sqlite",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let _ = std::fs::remove_file(&path);
    let mut app = App::open(path.to_string_lossy().into_owned()).unwrap();
    let project = app
        .client
        .create_project(orbit_client::CreateProjectRequest {
            id: None,
            name: "Detail Project".into(),
        })
        .unwrap();
    app.client.install_bundled_blueprint("basic").unwrap();
    app.client
        .apply_workflow(project.id.clone(), "basic".to_owned(), None)
        .unwrap();
    let task = app
        .client
        .create_task(orbit_client::CreateTaskRequest {
            id: None,
            title: "Cache me".into(),
            project_id: Some(project.id.clone()),
            milestone_id: None,
        })
        .unwrap();
    app.client
        .set_task_status(orbit_client::SetTaskStatusRequest {
            id: task.id.clone(),
            status: "ready".into(),
        })
        .unwrap();
    app.refresh_workspace();
    app.open_project(&project.id);
    let board = app
        .project
        .as_ref()
        .unwrap()
        .views
        .iter()
        .position(|view| view.id == "board")
        .expect("board view");
    app.switch_view_by_index(board);
    app.active_component = 0;
    app.selected = 0;
    assert_eq!(
        app.selected_item().map(|item| item.id.clone()),
        Some(task.id.clone())
    );
    (app, task.id)
}

fn detail_field(app: &App, field: &str) -> Option<String> {
    app.detail_model().and_then(|model| {
        model
            .fields
            .into_iter()
            .find(|(key, _)| key == field)
            .map(|(_, value)| value)
    })
}

fn open_detail_field(app: &mut App, field: &str) -> Option<String> {
    app.modal = None;
    app.open_detail();
    let Some(Modal::Detail(model)) = app.modal.take() else {
        panic!("detail modal missing: {}", app.status.text);
    };
    model
        .fields
        .into_iter()
        .find(|(key, _)| key == field)
        .map(|(_, value)| value)
}

#[test]
fn toggling_a_criterion_updates_the_docked_and_opened_detail_counts() {
    let (mut app, task_id) = project_app("criterion-toggle");
    app.client
        .add_acceptance_criterion(task_id.clone(), "Works offline".into())
        .unwrap();
    app.client
        .add_acceptance_criterion(task_id.clone(), "Syncs".into())
        .unwrap();
    // The direct backend writes above bypass the App cache invalidation that
    // every user-facing mutation performs.
    app.invalidate_detail_cache();
    assert_eq!(
        detail_field(&app, "Acceptance").as_deref(),
        Some("0/2 checked")
    );

    // The criteria modal toggles through `App`, which invalidates the cache.
    app.open_criteria(task_id.clone(), "Cache me".into());
    assert!(matches!(app.modal, Some(Modal::Criteria(_))));
    handle_key(&mut app, KeyCode::Char(' '), KeyModifiers::NONE).unwrap();

    let Some(Modal::Criteria(state)) = app.modal.as_ref() else {
        panic!("criteria modal closed unexpectedly");
    };
    assert!(state.items[0].completed, "toggle must reach the backend");
    assert_eq!(
        detail_field(&app, "Acceptance").as_deref(),
        Some("1/2 checked"),
        "docked detail must show the accepted count after a toggle"
    );
    assert_eq!(
        open_detail_field(&mut app, "Acceptance").as_deref(),
        Some("1/2 checked"),
        "the opened detail must not read a stale cache"
    );

    // Removing a criterion changes the denominator too.
    let remaining = app.client.acceptance_criteria(&task_id).unwrap();
    let removed_id = remaining
        .last()
        .map(|item| item.id.clone())
        .expect("a criterion to remove");
    app.execute_pending(PendingAction::RemoveCriterion {
        task_id: task_id.clone(),
        criterion_id: removed_id,
        task_title: "Cache me".into(),
    });
    assert_eq!(
        detail_field(&app, "Acceptance").as_deref(),
        Some("1/1 checked"),
        "{}",
        app.status.text
    );
    assert_eq!(
        open_detail_field(&mut app, "Acceptance").as_deref(),
        Some("1/1 checked"),
        "the opened detail must show the remaining criterion count"
    );
}

#[test]
fn removing_an_attachment_updates_the_detail_count() {
    let (mut app, task_id) = project_app("attachment-remove");
    let source = std::env::temp_dir().join(format!(
        "orbit-tui-gate3-attach-{}-{}.txt",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::write(&source, b"evidence").unwrap();
    let attachment = app
        .client
        .import_attachment(
            "task".into(),
            task_id.clone(),
            source.to_string_lossy().into_owned(),
            None,
        )
        .unwrap();
    // Direct backend write: invalidate as a user-facing import would.
    app.invalidate_detail_cache();
    assert_eq!(
        detail_field(&app, "Attachments").as_deref(),
        Some("1 file(s)")
    );
    assert_eq!(
        open_detail_field(&mut app, "Attachments").as_deref(),
        Some("1 file(s)")
    );

    app.execute_pending(PendingAction::RemoveAttachment {
        attachment_id: attachment.id,
        owner_kind: "task".into(),
        owner_id: task_id.clone(),
        title: "Attachments".into(),
    });
    assert_eq!(
        detail_field(&app, "Attachments").as_deref(),
        Some("none"),
        "{}",
        app.status.text
    );
    assert_eq!(
        open_detail_field(&mut app, "Attachments").as_deref(),
        Some("none"),
        "the opened detail must not keep the removed attachment count"
    );

    let _ = std::fs::remove_file(&source);
}

#[test]
fn criterion_edits_are_reflected_in_the_cached_detail() {
    // Editing criterion text is a form path; the modal reopens from accepted
    // state and the detail count must still describe the accepted list.
    let (mut app, task_id) = project_app("criterion-edit");
    app.client
        .add_acceptance_criterion(task_id.clone(), "First".into())
        .unwrap();
    let criterion_id = app
        .client
        .acceptance_criteria(&task_id)
        .unwrap()
        .first()
        .unwrap()
        .id
        .clone();
    app.start_form(FormMode::EditCriterion {
        task_id: task_id.clone(),
        criterion_id,
    });
    let Some(Modal::Form(mut form)) = app.modal.take() else {
        panic!("criterion form missing");
    };
    form.fields[0].value = "Renamed criterion".into();
    form.dirty = true;
    app.submit_form(form);
    assert!(matches!(app.modal, Some(Modal::Criteria(_))));
    assert_eq!(
        app.client
            .acceptance_criteria(&task_id)
            .unwrap()
            .first()
            .map(|item| item.text.clone()),
        Some("Renamed criterion".to_owned())
    );
    assert_eq!(
        detail_field(&app, "Acceptance").as_deref(),
        Some("0/1 checked")
    );
    assert_eq!(app.screen, Screen::Project);
}

#[test]
fn alt_modified_characters_are_not_text_input() {
    // Esc followed immediately by a character can arrive as Alt+<char> in a
    // real terminal. That chord is not text: it must not mutate a search,
    // palette or form value, while an Alt-free character still types.
    let (mut app, _task_id) = project_app("alt-input");

    app.open_search();
    handle_key(&mut app, KeyCode::Char('j'), KeyModifiers::ALT).unwrap();
    let Some(Modal::Search { query, .. }) = &app.modal else {
        panic!("search modal closed unexpectedly");
    };
    assert!(
        query.is_empty(),
        "Alt+j must not enter search text: {query:?}"
    );
    handle_key(&mut app, KeyCode::Char('j'), KeyModifiers::NONE).unwrap();
    let Some(Modal::Search { query, .. }) = &app.modal else {
        panic!("search modal closed unexpectedly");
    };
    assert_eq!(query, "j", "Alt-free characters still type");
    handle_key(&mut app, KeyCode::Esc, KeyModifiers::NONE).unwrap();

    app.open_palette();
    handle_key(&mut app, KeyCode::Char('x'), KeyModifiers::ALT).unwrap();
    let Some(Modal::Palette { query, .. }) = &app.modal else {
        panic!("palette modal closed unexpectedly");
    };
    assert!(
        query.is_empty(),
        "Alt+x must not enter palette text: {query:?}"
    );
    handle_key(&mut app, KeyCode::Esc, KeyModifiers::NONE).unwrap();

    app.start_form(FormMode::NewTask { project_id: None });
    handle_key(&mut app, KeyCode::Char('j'), KeyModifiers::ALT).unwrap();
    let Some(Modal::Form(form)) = &app.modal else {
        panic!("form modal closed unexpectedly");
    };
    assert!(
        form.fields[0].value.is_empty(),
        "Alt+j must not enter form text: {:?}",
        form.fields[0].value
    );
    assert!(!form.dirty, "an ignored chord must not dirty the form");
    handle_key(&mut app, KeyCode::Char('j'), KeyModifiers::NONE).unwrap();
    let Some(Modal::Form(form)) = &app.modal else {
        panic!("form modal closed unexpectedly");
    };
    assert_eq!(form.fields[0].value, "j", "Alt-free characters still type");
    assert!(form.dirty);
}

#[test]
fn accepted_list_commands_are_honest_about_local_deployments() {
    let (mut app, _task_id) = project_app("local-list");
    for command in [
        "routine.list",
        "strategy.list",
        "milestone.list",
        "schedule.list",
    ] {
        match app.console_run(command) {
            app::ConsoleOutcome::Error(error) => {
                assert!(
                    error.contains("connected"),
                    "{command} must say why it cannot list locally: {error}"
                );
            }
            other => panic!("{command} must not invent accepted rows locally: {other:?}"),
        }
    }
    assert_eq!(app.pending_count(), 0);
}
