//! M29 interaction regressions.
//!
//! The docked detail pane is repainted every frame at the wide breakpoint, and
//! its model used to issue three backend reads per frame. These tests pin the
//! cache contract (render is read-free, selection/navigation/refresh invalidate)
//! and the targeted quick-edit refresh (no whole-workspace re-read).
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
fn detail_pane_model_is_seeded_by_input_paths_and_never_rebuilt_in_render() {
    let mut app = project_app("m29-detail-cache");
    switch_to(&mut app, "board");
    // `move_selection` is an input path: it seeds the pane model once.
    app.move_selection(0);
    assert!(app.selected_item().is_some(), "board should select a row");

    let seeded = app.detail_cache_builds.get();
    // The wide layout draws the docked pane; repeated frames must reuse the
    // seeded model and therefore issue no backend reads and no rebuilds.
    crate::backend::diagnostics::reset_detail_reads();
    for _ in 0..3 {
        render(&mut app, 160, 45);
    }
    assert_eq!(
        app.detail_cache_builds.get(),
        seeded,
        "render rebuilt the detail model instead of using the seeded one"
    );
    assert_eq!(
        crate::backend::diagnostics::detail_reads(),
        0,
        "render must not issue task_detail/acceptance_criteria/list_attachments reads"
    );
    let first = app.detail_model().expect("detail model");

    // A selection change is a different subject; the input path rebuilds it.
    let before = app.detail_cache_builds.get();
    app.move_selection(1);
    assert_eq!(
        app.detail_cache_builds.get(),
        before + 1,
        "selection change must reseed exactly once"
    );
    let second = app.detail_model().expect("detail model");
    assert_eq!(
        app.detail_cache_builds.get(),
        before + 1,
        "reading the seeded model must not rebuild it"
    );
    assert_ne!(first.title, second.title);

    // Navigation to another view is a different subject.
    let before = app.detail_cache_builds.get();
    let other = app
        .project
        .as_ref()
        .unwrap()
        .views
        .iter()
        .position(|view| view.id != "board")
        .unwrap();
    app.switch_view_by_index(other);
    let seeded_after_navigation = app.detail_cache_builds.get();
    assert!(
        seeded_after_navigation > before,
        "view change must reseed the cache"
    );
    let _ = app.detail_model();
    assert_eq!(
        app.detail_cache_builds.get(),
        seeded_after_navigation,
        "the view change already seeded the cache"
    );

    // A refresh keeps the same subject but replaces its accepted data.
    let before_refresh = app.detail_cache_builds.get();
    app.refresh_project();
    let seeded_after_refresh = app.detail_cache_builds.get();
    assert!(
        seeded_after_refresh > before_refresh,
        "refresh must reseed the cache"
    );
    let _ = app.detail_model();
    assert_eq!(
        app.detail_cache_builds.get(),
        seeded_after_refresh,
        "refresh already seeded the cache"
    );

    // Leaving the project screen changes the subject too.
    let before_navigation = app.detail_cache_builds.get();
    app.go_screen(Screen::Home);
    assert!(
        app.detail_cache_builds.get() > before_navigation,
        "screen navigation must reseed the cache"
    );
}

#[test]
fn hidden_detail_pane_skips_reads_and_open_detail_still_builds() {
    let mut app = project_app("m29-detail-visibility");
    switch_to(&mut app, "board");
    app.move_selection(0);
    assert!(app.selected_item().is_some(), "board should select a row");

    // Narrow viewport: the pane is not docked, so a selection change must not
    // issue task detail, criteria or attachment reads for it.
    render(&mut app, 100, 30);
    assert_eq!(app.viewport_width, 100);
    let builds = app.detail_cache_builds.get();
    crate::backend::diagnostics::reset_detail_reads();
    app.move_selection(1);
    assert!(app.selected_item().is_some());
    assert_eq!(
        crate::backend::diagnostics::detail_reads(),
        0,
        "a narrow viewport must not read detail for a pane that is not drawn"
    );
    assert_eq!(
        app.detail_cache_builds.get(),
        builds,
        "a narrow viewport must not build the detail model"
    );

    // Widening after a hidden selection paints one placeholder frame: render is
    // still cache-only, so it neither reads nor builds.
    crate::backend::diagnostics::reset_detail_reads();
    let wide = render(&mut app, 160, 45);
    assert_eq!(
        crate::backend::diagnostics::detail_reads(),
        0,
        "the first wide frame after a hidden selection stays cache-only"
    );
    assert!(
        !wide.contains("PROPERTIES"),
        "the first wide frame shows the neutral placeholder: {wide}"
    );
    assert!(
        wide.contains("Details"),
        "the pane frame is docked and visible: {wide}"
    );

    // Hiding the docked pane with `D` makes selection changes read-free too.
    handle_key(&mut app, KeyCode::Char('D'), KeyModifiers::NONE).unwrap();
    assert!(!app.show_detail);
    let builds = app.detail_cache_builds.get();
    crate::backend::diagnostics::reset_detail_reads();
    app.move_selection(1);
    assert_eq!(
        crate::backend::diagnostics::detail_reads(),
        0,
        "a hidden pane must not read detail"
    );
    assert_eq!(
        app.detail_cache_builds.get(),
        builds,
        "a hidden pane must not build the detail model"
    );

    // An explicit overlay request still builds the model on demand.
    let builds = app.detail_cache_builds.get();
    crate::backend::diagnostics::reset_detail_reads();
    app.open_detail();
    assert!(
        matches!(app.modal, Some(Modal::Detail(_))),
        "open_detail builds even while the docked pane is hidden"
    );
    assert!(
        crate::backend::diagnostics::detail_reads() > 0,
        "open_detail is an explicit request and may read task detail"
    );
    assert!(
        app.detail_cache_builds.get() > builds,
        "open_detail builds the model"
    );
}

#[test]
fn render_never_reads_task_detail_even_on_the_first_frame_after_a_change() {
    let mut app = project_app("m29-render-detail-reads");
    switch_to(&mut app, "board");
    app.move_selection(0);
    let task_id = app.selected_item().unwrap().id.clone();
    let title = app.selected_item().unwrap().title.clone();

    // First frame after a selection change: the input path seeded the model,
    // so the frame is a cache hit and the pane already shows task fields.
    crate::backend::diagnostics::reset_detail_reads();
    let text = {
        let backend = TestBackend::new(160, 45);
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
        crate::backend::diagnostics::detail_reads(),
        0,
        "render must use the cached detail model only"
    );
    assert!(
        text.contains(&title),
        "detail pane missing the task: {text}"
    );
    assert!(
        text.contains("PROPERTIES"),
        "a seeded detail pane shows task fields"
    );

    // A deliberately invalidated cache (a path that forgot to seed) renders a
    // neutral placeholder instead of performing backend reads.
    app.invalidate_detail_cache();
    crate::backend::diagnostics::reset_detail_reads();
    let fallback = {
        let backend = TestBackend::new(160, 45);
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
        crate::backend::diagnostics::detail_reads(),
        0,
        "an unseeded detail pane must not fall back to backend reads"
    );
    assert!(
        !fallback.contains("PROPERTIES"),
        "an unseeded pane shows the neutral placeholder: {fallback}"
    );
    assert!(
        fallback.contains("Details"),
        "the pane frame stays visible: {fallback}"
    );

    // The next input path reseeds it, so the miss is one frame at most.
    app.move_selection(0);
    let reseeded = {
        let backend = TestBackend::new(160, 45);
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
    assert!(
        reseeded.contains("PROPERTIES"),
        "the seeded pane comes back: {reseeded}"
    );
    assert_eq!(app.selected_item().unwrap().id, task_id);
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
