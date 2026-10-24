use crate::app::{FormField, FormMode, Modal};
use crate::*;

fn path() -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "orbit-tui-m28-form-{}-{}.sqlite",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}

fn app_at(path: &std::path::Path) -> App {
    App::open(path.to_string_lossy().into_owned()).unwrap()
}

fn fill(form: &mut app::FormState, key: &str, value: &str) {
    let field = form
        .fields
        .iter_mut()
        .find(|field: &&mut FormField| field.key == key)
        .unwrap_or_else(|| panic!("missing field {key}"));
    field.value = value.into();
    form.dirty = true;
}

fn current_form(app: &mut App) -> app::FormState {
    let Some(Modal::Form(form)) = app.modal.take() else {
        panic!("expected form");
    };
    form
}

fn create_project(app: &mut App, name: &str) -> String {
    app.start_form(FormMode::NewProject);
    let mut form = current_form(app);
    fill(&mut form, "name", name);
    app.submit_form(form);
    app.project.as_ref().unwrap().id.clone()
}

#[test]
fn project_and_task_forms_persist_across_restart() {
    let path = path();
    let mut app = app_at(&path);
    let project = create_project(&mut app, "Form Project");
    app.start_form(FormMode::NewTask {
        project_id: Some(project.clone()),
    });
    let mut form = current_form(&mut app);
    fill(&mut form, "title", "Form task");
    fill(&mut form, "estimate", "45");
    fill(&mut form, "deadline", "1893456000000");
    app.submit_form(form);
    assert!(
        app.modal.is_none(),
        "task form should close: {:?}",
        app.modal
    );
    assert!(
        app.client
            .list_tasks()
            .unwrap()
            .iter()
            .any(|task| task.title == "Form task" && task.estimated_minutes == Some(45))
    );
    drop(app);

    let app = app_at(&path);
    assert!(app.projects.iter().any(|row| row.name == "Form Project"));
    let task = app
        .client
        .list_tasks()
        .unwrap()
        .into_iter()
        .find(|task| task.title == "Form task")
        .expect("persisted task");
    assert_eq!(task.estimated_minutes, Some(45));
    assert_eq!(task.deadline_at_ms, Some(1_893_456_000_000));
}

#[test]
fn task_form_closes_through_the_key_handler() {
    let mut app = app_at(&path());
    create_project(&mut app, "Key handler");
    app.start_form(FormMode::NewTask {
        project_id: app.project.as_ref().map(|project| project.id.clone()),
    });
    for character in "First task".chars() {
        handle_key(&mut app, KeyCode::Char(character), KeyModifiers::NONE).unwrap();
    }
    assert!(!handle_key(&mut app, KeyCode::Enter, KeyModifiers::NONE).unwrap());
    assert!(app.modal.is_none(), "modal after Enter: {:?}", app.modal);
}

#[test]
fn edit_task_form_renames_and_updates_planning_fields() {
    let mut app = app_at(&path());
    let project = create_project(&mut app, "Edit Project");
    let task = app
        .client
        .create_task(orbit_client::CreateTaskRequest {
            id: None,
            title: "Old title".into(),
            project_id: Some(project),
            milestone_id: None,
        })
        .unwrap();
    app.start_form(FormMode::EditTask {
        task_id: task.id.clone(),
    });
    let mut form = current_form(&mut app);
    fill(&mut form, "title", "New title");
    fill(&mut form, "estimate", "30");
    app.submit_form(form);
    let updated = app
        .client
        .list_tasks()
        .unwrap()
        .into_iter()
        .find(|row| row.id == task.id)
        .unwrap();
    assert_eq!(updated.title, "New title");
    assert_eq!(updated.estimated_minutes, Some(30));
}

#[test]
fn edit_project_form_preserves_criteria_and_updates_supported_settings() {
    let mut app = app_at(&path());
    let project = create_project(&mut app, "Settings");
    app.start_form(FormMode::EditProject {
        project_id: project.clone(),
    });
    let mut form = current_form(&mut app);
    fill(&mut form, "status", "active");
    fill(&mut form, "mode", "learning");
    fill(&mut form, "objective", "Learn the workflow");
    app.submit_form(form);
    let updated = app
        .client
        .list_projects()
        .unwrap()
        .into_iter()
        .find(|row| row.id == project)
        .unwrap();
    assert_eq!(updated.status, "active");
    assert_eq!(updated.mode, "learning");
    assert_eq!(updated.objective.as_deref(), Some("Learn the workflow"));
}

#[test]
fn work_item_forms_use_declared_fields_and_persist_edits() {
    let mut app = app_at(&path());
    let project = create_project(&mut app, "Research");
    app.client.install_bundled_blueprint("research").unwrap();
    app.client
        .apply_workflow(project.clone(), "research".to_owned(), None)
        .unwrap();
    app.refresh_project();
    app.start_form(FormMode::NewWorkItemKind {
        project_id: project.clone(),
    });
    let mut kind_form = current_form(&mut app);
    fill(&mut kind_form, "kind", "hypothesis");
    app.submit_form(kind_form);
    let mut fields = current_form(&mut app);
    fill(&mut fields, "question", "Does the form work?");
    fill(&mut fields, "confidence", "high");
    app.submit_form(fields);
    app.refresh_project();
    let item = app
        .client
        .list_blueprint_work_items(&project)
        .unwrap()
        .into_iter()
        .find(|item| item.kind == "hypothesis")
        .expect("created item");
    assert_eq!(item.properties["question"], "Does the form work?");
    app.start_form(FormMode::EditWorkItem {
        item_id: item.id.clone(),
        revision: item.revision,
        blueprint: app
            .client
            .project_blueprint_definition(&project)
            .unwrap()
            .unwrap(),
        kind: item.kind.clone(),
    });
    let mut edit = current_form(&mut app);
    fill(&mut edit, "question", "Edited question");
    app.submit_form(edit);
    let edited = app
        .client
        .list_blueprint_work_items(&project)
        .unwrap()
        .into_iter()
        .find(|row| row.id == item.id)
        .unwrap();
    assert_eq!(edited.properties["question"], "Edited question");
}

#[test]
fn dependency_forms_add_and_remove_prerequisites() {
    let mut app = app_at(&path());
    let project = create_project(&mut app, "Dependencies");
    let first = app
        .client
        .create_task(orbit_client::CreateTaskRequest {
            id: None,
            title: "First".into(),
            project_id: Some(project.clone()),
            milestone_id: None,
        })
        .unwrap();
    let second = app
        .client
        .create_task(orbit_client::CreateTaskRequest {
            id: None,
            title: "Second".into(),
            project_id: Some(project),
            milestone_id: None,
        })
        .unwrap();
    app.refresh_project();
    app.start_form(FormMode::AddDependency {
        task_id: second.id.clone(),
    });
    let mut add = current_form(&mut app);
    fill(&mut add, "prerequisite", &first.id);
    app.submit_form(add);
    let data = app
        .client
        .project_view_data(&app.project.as_ref().unwrap().id, crate::app::now_ms())
        .unwrap();
    assert!(
        data.tasks
            .iter()
            .find(|task| task.id == second.id)
            .unwrap()
            .prerequisite_ids
            .contains(&first.id)
    );

    app.refresh_project();
    app.start_form(FormMode::RemoveDependency {
        task_id: second.id.clone(),
    });
    let mut remove = current_form(&mut app);
    fill(&mut remove, "prerequisite", &first.id);
    app.submit_form(remove);
    let data = app
        .client
        .project_view_data(&app.project.as_ref().unwrap().id, crate::app::now_ms())
        .unwrap();
    assert!(
        !data
            .tasks
            .iter()
            .find(|task| task.id == second.id)
            .unwrap()
            .prerequisite_ids
            .contains(&first.id)
    );
}

#[test]
fn invalid_form_preserves_values_and_reports_the_field() {
    let mut app = app_at(&path());
    app.start_form(FormMode::NewTask { project_id: None });
    let mut form = current_form(&mut app);
    fill(&mut form, "title", "Valid title");
    fill(&mut form, "estimate", "not-a-number");
    app.submit_form(form);
    let Some(Modal::Form(form)) = &app.modal else {
        panic!("form should stay open");
    };
    assert!(form.error.as_deref().unwrap_or("").contains("integer"));
    assert_eq!(
        form.fields
            .iter()
            .find(|field| field.key == "estimate")
            .unwrap()
            .value,
        "not-a-number"
    );
}
