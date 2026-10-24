use crate::app::{FormMode, Modal};
use crate::*;

fn app() -> App {
    let path = std::env::temp_dir().join(format!(
        "orbit-tui-m28-scope-{}-{}.sqlite",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let _ = std::fs::remove_file(&path);
    App::open(path.to_string_lossy().into_owned()).unwrap()
}

fn fill(form: &mut app::FormState, key: &str, value: &str) {
    form.fields
        .iter_mut()
        .find(|field| field.key == key)
        .unwrap()
        .value = value.into();
    form.dirty = true;
}

#[test]
fn task_edit_moves_project_without_renaming_on_invalid_milestone() {
    let mut app = app();
    let first = app
        .client
        .create_project(orbit_client::CreateProjectRequest {
            id: None,
            name: "First".into(),
        })
        .unwrap();
    let second = app
        .client
        .create_project(orbit_client::CreateProjectRequest {
            id: None,
            name: "Second".into(),
        })
        .unwrap();
    app.refresh_workspace();
    let task = app
        .client
        .create_task(orbit_client::CreateTaskRequest {
            id: None,
            title: "Move me".into(),
            project_id: Some(first.id.clone()),
            milestone_id: None,
        })
        .unwrap();
    app.open_project(&first.id);
    app.start_form(FormMode::EditTask {
        task_id: task.id.clone(),
    });
    let Some(Modal::Form(mut form)) = app.modal.take() else {
        panic!("form");
    };
    fill(&mut form, "project", &second.id);
    app.submit_form(form);
    let moved = app
        .client
        .list_tasks()
        .unwrap()
        .into_iter()
        .find(|row| row.id == task.id)
        .unwrap();
    assert_eq!(moved.project_id.as_deref(), Some(second.id.as_str()));

    app.start_form(FormMode::EditTask {
        task_id: task.id.clone(),
    });
    let Some(Modal::Form(mut form)) = app.modal.take() else {
        panic!("form");
    };
    fill(&mut form, "title", "Should not apply");
    fill(&mut form, "milestone", "missing");
    app.submit_form(form);
    assert!(matches!(app.modal, Some(Modal::Form(_))));
    let unchanged = app
        .client
        .list_tasks()
        .unwrap()
        .into_iter()
        .find(|row| row.id == task.id)
        .unwrap();
    assert_eq!(unchanged.title, "Move me");
}
