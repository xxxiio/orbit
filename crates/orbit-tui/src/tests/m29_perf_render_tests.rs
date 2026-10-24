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

fn render(app: &mut App, width: u16, height: u16) {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|frame| render::draw(frame, app)).unwrap();
}

#[test]
fn detail_pane_model_is_cached_across_frames_and_invalidated_on_change() {
    let mut app = project_app("m29-detail-cache");
    switch_to(&mut app, "board");
    app.selected = 0;
    assert!(app.selected_item().is_some(), "board should select a row");

    let before = app.detail_cache_builds.get();
    let first = app.detail_model().expect("detail model");
    assert_eq!(
        app.detail_cache_builds.get(),
        before + 1,
        "first read builds the model"
    );

    // The wide layout draws the docked pane; repeated frames must reuse it and
    // therefore issue no backend reads.
    for _ in 0..3 {
        render(&mut app, 160, 45);
    }
    assert_eq!(
        app.detail_cache_builds.get(),
        before + 1,
        "render rebuilt the detail model instead of using the cache"
    );
    let second = app.detail_model().expect("detail model");
    assert_eq!(first.title, second.title);

    // A selection change is a different subject.
    app.move_selection(1);
    let _ = app.detail_model();
    assert_eq!(
        app.detail_cache_builds.get(),
        before + 2,
        "selection change must invalidate the cache"
    );

    // Navigation to another view is a different subject.
    let other = app
        .project
        .as_ref()
        .unwrap()
        .views
        .iter()
        .position(|view| view.id != "board")
        .unwrap();
    app.switch_view_by_index(other);
    let _ = app.detail_model();
    assert_eq!(
        app.detail_cache_builds.get(),
        before + 3,
        "view change must invalidate the cache"
    );

    // A refresh keeps the same subject but replaces its accepted data.
    let before_refresh = app.detail_cache_builds.get();
    app.refresh_project();
    let _ = app.detail_model();
    assert_eq!(
        app.detail_cache_builds.get(),
        before_refresh + 1,
        "refresh must invalidate the cache"
    );

    // Leaving the project screen changes the subject too.
    let before_navigation = app.detail_cache_builds.get();
    app.go_screen(Screen::Home);
    let _ = app.detail_model();
    assert_eq!(
        app.detail_cache_builds.get(),
        before_navigation + 1,
        "screen navigation must invalidate the cache"
    );
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
