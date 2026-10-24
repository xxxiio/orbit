use crate::app::{Modal, Screen};
use crate::*;

fn app() -> App {
    let filename = std::env::temp_dir().join(format!(
        "orbit-tui-m28-planning-{}-{}.sqlite",
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
fn today_tasks_are_selectable_and_offer_revision_checked_destinations() {
    let mut app = app();
    let project = app
        .client
        .create_project(orbit_client::CreateProjectRequest {
            id: None,
            name: "Planning".into(),
        })
        .unwrap();
    let task = app
        .client
        .create_task(orbit_client::CreateTaskRequest {
            id: None,
            title: "Accepted task".into(),
            project_id: Some(project.id),
            milestone_id: None,
        })
        .unwrap();
    app.client
        .set_task_status(orbit_client::SetTaskStatusRequest {
            id: task.id.clone(),
            status: "ready".into(),
        })
        .unwrap();
    app.refresh_planning();
    app.refresh_workspace();
    assert_eq!(app.screen, Screen::Home);
    let component = app
        .home
        .components
        .iter()
        .position(|component| component.id == "today_tasks")
        .expect("today task component");
    app.active_component = component;
    app.selected = app
        .current_component()
        .unwrap()
        .selection
        .iter()
        .position(|row| row.id == task.id)
        .expect("planning task selection");
    app.today_actions();
    assert!(
        matches!(app.modal, Some(Modal::TodayActions { .. })),
        "expected accepted planning action menu"
    );
    app.today_select_action(0);
    assert!(
        matches!(app.modal, Some(Modal::Message { .. })),
        "expected read-only destination"
    );
    let _ = task;
}
