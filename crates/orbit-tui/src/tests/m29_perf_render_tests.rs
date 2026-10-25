//! M29/M30 interaction regressions.
//!
//! The inspector model used to be a docked pane rebuilt during render. These
//! tests pin the current contract: render is read-free and rebuild-free, the
//! model is built exactly once per explicit open, and the quick-edit refresh
//! still avoids a whole-workspace re-read.
use crate::app::{FormMode, Modal, Screen};
use crate::*;
use ratatui::{Terminal, backend::TestBackend};

fn base_app(stem: &str) -> App {
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

fn project_app(stem: &str) -> App {
    let mut app = base_app(stem);
    let project = app
        .client
        .create_project(orbit_client::CreateProjectRequest {
            id: None,
            name: "Cache Project".into(),
        })
        .unwrap();
    app.client.install_bundled_blueprint("basic").unwrap();
    app.client
        .apply_workflow(project.id.clone(), "basic".to_owned(), None)
        .unwrap();
    for title in ["Alpha task", "Beta task", "Gamma task"] {
        let task = app
            .client
            .create_task(orbit_client::CreateTaskRequest {
                id: None,
                title: title.into(),
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
    }
    app.refresh_workspace();
    app.open_project(&project.id);
    app
}

fn switch_to(app: &mut App, view_id: &str) {
    let index = app
        .project
        .as_ref()
        .unwrap()
        .views
        .iter()
        .position(|view| view.id == view_id)
        .unwrap_or_else(|| panic!("view {view_id} missing"));
    app.switch_view_by_index(index);
    app.active_component = 0;
}

fn render(app: &mut App, width: u16, height: u16) -> String {
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
fn the_inspector_model_is_built_on_open_and_never_rebuilt_in_render() {
    let mut app = project_app("m29-inspector-build");
    switch_to(&mut app, "board");

    // Nothing is built for a closed inspector, even when the selection moves.
    app.move_selection(0);
    assert!(app.selected_item().is_some(), "board should select a row");
    app.move_selection(1);
    assert_eq!(
        app.inspector_builds.get(),
        0,
        "a closed inspector must not build a model"
    );
    crate::backend::diagnostics::reset_detail_reads();
    for _ in 0..3 {
        render(&mut app, 160, 45);
    }
    assert_eq!(
        crate::backend::diagnostics::detail_reads(),
        0,
        "render must not issue task_detail/acceptance_criteria/list_attachments reads"
    );
    assert_eq!(app.inspector_builds.get(), 0);

    // Opening is an explicit input-path request: exactly one build.
    app.open_inspector();
    assert_eq!(
        app.inspector_builds.get(),
        1,
        "opening the inspector builds exactly one model"
    );
    let Some(crate::app::Modal::Inspector(first)) = app.modal.as_ref() else {
        panic!("inspector modal: {}", app.status.text);
    };
    let first_title = first.detail.title.clone();

    // Repeated frames reuse the stored model.
    crate::backend::diagnostics::reset_detail_reads();
    let builds = app.inspector_builds.get();
    for _ in 0..3 {
        render(&mut app, 160, 45);
    }
    assert_eq!(
        app.inspector_builds.get(),
        builds,
        "render rebuilt the inspector model instead of painting the stored one"
    );
    assert_eq!(
        crate::backend::diagnostics::detail_reads(),
        0,
        "an open inspector is painted from app state, never from the backend"
    );

    // Closing, moving the selection and reopening builds exactly once more,
    // for the new subject.
    app.close_inspector();
    app.move_selection(1);
    assert_eq!(app.inspector_builds.get(), 1, "selection is not a subject");
    app.open_inspector();
    assert_eq!(app.inspector_builds.get(), 2);
    let Some(crate::app::Modal::Inspector(second)) = app.modal.as_ref() else {
        panic!("inspector modal");
    };
    assert_ne!(first_title, second.detail.title);

    // A refresh rebuilds the open inspector for its recorded subject.
    let before_refresh = app.inspector_builds.get();
    app.refresh_project();
    assert!(
        app.inspector_builds.get() > before_refresh,
        "refresh must rebuild the open inspector"
    );
    // ... but navigation closes the intent instead of reopening anything.
    app.close_inspector();
    let before_navigation = app.inspector_builds.get();
    app.go_screen(Screen::Home);
    assert_eq!(
        app.inspector_builds.get(),
        before_navigation,
        "screen navigation must not build or reopen the inspector"
    );
    assert!(app.modal.is_none());
}

#[test]
fn a_closed_inspector_skips_reads_and_opening_it_still_builds() {
    let mut app = project_app("m29-inspector-visibility");
    switch_to(&mut app, "board");
    app.move_selection(0);
    assert!(app.selected_item().is_some(), "board should select a row");

    // A narrow viewport never reserves anything for a detail pane.
    render(&mut app, 60, 18);
    assert_eq!(app.viewport_width, 60);
    crate::backend::diagnostics::reset_detail_reads();
    app.move_selection(1);
    assert!(app.selected_item().is_some());
    assert_eq!(
        crate::backend::diagnostics::detail_reads(),
        0,
        "moving the selection must not read detail for a closed inspector"
    );
    assert_eq!(
        app.inspector_builds.get(),
        0,
        "moving the selection must not build a model"
    );

    // The explicit open is allowed to read, and paints full screen at 60x18.
    app.open_inspector();
    assert!(
        matches!(app.modal, Some(Modal::Inspector(_))),
        "opening builds for the current subject: {}",
        app.status.text
    );
    assert!(
        crate::backend::diagnostics::detail_reads() > 0,
        "opening is an explicit request and may read task detail"
    );
    assert_eq!(app.inspector_builds.get(), 1);
    let narrow = render(&mut app, 60, 18);
    assert!(narrow.contains("Inspector ·"), "{narrow}");

    // Closing returns to a read-free two-pane frame.
    handle_key(&mut app, KeyCode::Esc, KeyModifiers::NONE).unwrap();
    assert!(app.modal.is_none());
    crate::backend::diagnostics::reset_detail_reads();
    let closed = render(&mut app, 60, 18);
    assert_eq!(
        crate::backend::diagnostics::detail_reads(),
        0,
        "a closed inspector must not read"
    );
    assert!(!closed.contains("Inspector ·"), "{closed}");
}

#[test]
fn render_never_reads_task_detail_for_the_inspector() {
    let mut app = project_app("m29-render-detail-reads");
    switch_to(&mut app, "board");
    app.move_selection(0);
    let task_id = app.selected_item().unwrap().id.clone();
    let title = app.selected_item().unwrap().title.clone();
    app.open_inspector();

    // The frame paints the stored model: no backend reads at all.
    crate::backend::diagnostics::reset_detail_reads();
    let text = render(&mut app, 160, 45);
    assert_eq!(
        crate::backend::diagnostics::detail_reads(),
        0,
        "render must use the stored inspector model only"
    );
    assert!(text.contains(&title), "inspector missing the task: {text}");
    assert!(text.contains("PROPERTIES"), "the inspector shows fields");

    // With the inspector closed the wide frame has no detail column at all.
    app.close_inspector();
    crate::backend::diagnostics::reset_detail_reads();
    let closed = render(&mut app, 160, 45);
    assert_eq!(
        crate::backend::diagnostics::detail_reads(),
        0,
        "a closed inspector performs no reads"
    );
    assert!(
        !closed.contains("PROPERTIES") && !closed.contains("Inspector ·"),
        "the default layout reserves no details pane: {closed}"
    );
    assert_eq!(app.selected_item().unwrap().id, task_id);

    // Reopening rebuilds from accepted state and paints again.
    app.open_inspector();
    let reopened = render(&mut app, 160, 45);
    assert!(reopened.contains("PROPERTIES"), "{reopened}");
}

#[test]
fn quick_edit_refreshes_the_project_without_rereading_the_workspace() {
    let mut app = project_app("m29-quick-edit-refresh");
    switch_to(&mut app, "board");
    app.selected = 0;
    let task_id = app.selected_item().unwrap().id.clone();

    app.start_form(FormMode::QuickEditTask {
        task_id: task_id.clone(),
    });
    let Some(Modal::Form(mut form)) = app.modal.take() else {
        panic!("quick edit form");
    };
    form.fields[0].value = "Renamed by quick edit".into();
    form.fields[0].cursor = form.fields[0].value.chars().count();
    form.dirty = true;
    let refreshes_before = app.workspace_refreshes.get();
    app.submit_form(form);

    assert!(
        app.status.text.contains("Quick edit saved"),
        "{}",
        app.status.text
    );
    assert_eq!(
        app.workspace_refreshes.get(),
        refreshes_before,
        "quick edit must not re-resolve the whole workspace"
    );
    let project_row = app
        .project
        .as_ref()
        .unwrap()
        .data
        .tasks
        .iter()
        .find(|task| task.id == task_id)
        .expect("task row in the open project");
    assert_eq!(project_row.title, "Renamed by quick edit");
    let workspace_row = app
        .workspace
        .as_ref()
        .unwrap()
        .tasks
        .iter()
        .find(|task| task.id == task_id)
        .expect("task row in the workspace snapshot");
    assert_eq!(
        workspace_row.title, "Renamed by quick edit",
        "the in-memory workspace row must stay in sync without a re-read"
    );

    // A priority change can reorder Today/Next, so it keeps the full refresh.
    app.last_submission = None;
    let refreshes_before = app.workspace_refreshes.get();
    app.start_form(FormMode::QuickEditTask {
        task_id: task_id.clone(),
    });
    let Some(Modal::Form(mut form)) = app.modal.take() else {
        panic!("quick edit form");
    };
    form.fields[1].value = "high".into();
    form.dirty = true;
    app.submit_form(form);
    assert_eq!(
        app.workspace_refreshes.get(),
        refreshes_before + 1,
        "a priority change must still refresh the whole workspace"
    );
    assert_eq!(
        app.client
            .task_detail(&task_id)
            .unwrap()
            .priority
            .as_deref(),
        Some("high")
    );
}

fn offline_connected_app(stem: &str) -> (App, std::path::PathBuf) {
    let path = std::env::temp_dir().join(format!(
        "orbit-tui-{stem}-{}-{}.db",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let _ = std::fs::remove_file(&path);
    let backend = crate::backend::Backend::open_connected_with_identity(
        &path,
        "https://example.invalid",
        None,
    )
    .expect("connected backend");
    (
        App::open_with_backend(backend).expect("connected app"),
        path,
    )
}

#[test]
fn render_reads_the_cached_pending_count_and_never_the_backend() {
    let (mut app, path) = offline_connected_app("m29-pending-cache");
    assert!(app.client.is_connected());

    // The frame header shows `N pending`; it must not ask the client per frame.
    crate::backend::diagnostics::reset_pending_count_reads();
    for _ in 0..3 {
        render(&mut app, 120, 35);
    }
    assert_eq!(
        crate::backend::diagnostics::pending_count_reads(),
        0,
        "rendering must read the cached pending count, not the backend"
    );

    let intent =
        crate::connected_actions::parse_intent("project.create cached-probe Queued", "generated")
            .expect("intent");
    app.execute_pending(app::PendingAction::QueueIntent { intent });
    assert_eq!(
        app.pending_count(),
        1,
        "queueing refreshes the cached count"
    );

    crate::backend::diagnostics::reset_pending_count_reads();
    let text = {
        let backend = TestBackend::new(120, 35);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| render::draw(frame, &mut app))
            .unwrap();
        let buffer = terminal.backend().buffer();
        (0..buffer.area.height)
            .map(|y| {
                (0..buffer.area.width)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    assert_eq!(
        crate::backend::diagnostics::pending_count_reads(),
        0,
        "the refreshed cache is enough for the header"
    );
    assert!(text.contains("1 pending"), "{text}");

    // Flushing refreshes the count too, so the shell never shows a stale depth.
    let _ = app.pending_flush_report(&[]);
    assert_eq!(app.pending_count(), app.client.pending_count());

    drop(app);
    let _ = std::fs::remove_file(&path);
}
