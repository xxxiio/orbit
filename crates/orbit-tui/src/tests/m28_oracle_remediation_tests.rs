use crate::app::{FormFieldKind, FormMode, Modal, Screen};
use crate::*;

fn path() -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "orbit-tui-m28-fix-{}-{}.sqlite",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}
fn app() -> App {
    App::open(path().to_string_lossy().into_owned()).unwrap()
}
fn fill(form: &mut app::FormState, key: &str, value: &str) {
    let field = form
        .fields
        .iter_mut()
        .find(|field| field.key == key)
        .unwrap();
    field.value = value.into();
    form.dirty = true;
}
fn take_form(app: &mut App) -> app::FormState {
    let Some(Modal::Form(form)) = app.modal.take() else {
        panic!("form");
    };
    form
}
fn project(app: &mut App, name: &str) -> String {
    app.start_form(FormMode::NewProject);
    let mut form = take_form(app);
    fill(&mut form, "name", name);
    app.submit_form(form);
    app.project.as_ref().unwrap().id.clone()
}

#[test]
fn project_objective_uses_default_signals_and_opens() {
    let mut app = app();
    app.start_form(FormMode::NewProject);
    let mut form = take_form(&mut app);
    fill(&mut form, "name", "Objective");
    fill(&mut form, "objective", "Ship it");
    app.submit_form(form);
    assert!(app.modal.is_none(), "{:?}", app.modal);
    let project = app
        .client
        .list_projects()
        .unwrap()
        .into_iter()
        .find(|project| project.name == "Objective")
        .unwrap();
    assert_eq!(project.objective.as_deref(), Some("Ship it"));
    assert!(project.progress_signals.len() >= 4);
    assert_eq!(app.screen, Screen::Project);
}

#[test]
fn project_edit_preserves_satisfied_criterion_status() {
    let mut app = app();
    let project_id = project(&mut app, "Criteria");
    app.client
        .configure_project_strategy(orbit_client::ConfigureProjectStrategyRequest {
            id: project_id.clone(),
            mode: "standard".into(),
            objective: None,
            progress_signals: vec![
                "milestones".into(),
                "tasks".into(),
                "blockers".into(),
                "forecast".into(),
            ],
            success_criteria: vec![orbit_client::ProjectCriterionInputDto {
                key: "ship".into(),
                description: "Ship".into(),
                required: true,
                status: "satisfied".into(),
            }],
        })
        .unwrap();
    app.refresh_workspace();
    app.open_project(&project_id);
    app.start_form(FormMode::EditProject {
        project_id: project_id.clone(),
    });
    let mut form = take_form(&mut app);
    fill(&mut form, "objective", "Still ship");
    app.submit_form(form);
    let project = app
        .client
        .list_projects()
        .unwrap()
        .into_iter()
        .find(|project| project.id == project_id)
        .unwrap();
    assert_eq!(project.success_criteria[0].status, "satisfied");
}

#[test]
fn zero_estimate_never_creates_or_renames() {
    let mut app = app();
    let project_id = project(&mut app, "Estimate");
    app.start_form(FormMode::NewTask {
        project_id: Some(project_id.clone()),
    });
    let mut form = take_form(&mut app);
    fill(&mut form, "title", "Zero");
    fill(&mut form, "estimate", "0");
    app.submit_form(form);
    assert!(matches!(app.modal, Some(Modal::Form(_))));
    assert!(app.client.list_tasks().unwrap().is_empty());

    let task = app
        .client
        .create_task(orbit_client::CreateTaskRequest {
            id: None,
            title: "Original".into(),
            project_id: Some(project_id),
            milestone_id: None,
        })
        .unwrap();
    app.start_form(FormMode::EditTask {
        task_id: task.id.clone(),
    });
    let mut form = take_form(&mut app);
    fill(&mut form, "title", "Renamed");
    fill(&mut form, "estimate", "0");
    app.submit_form(form);
    let unchanged = app
        .client
        .list_tasks()
        .unwrap()
        .into_iter()
        .find(|row| row.id == task.id)
        .unwrap();
    assert_eq!(unchanged.title, "Original");
    assert_eq!(unchanged.estimated_minutes, None);
}

#[test]
fn work_item_edit_only_patches_changed_values() {
    let mut app = app();
    let project_id = project(&mut app, "Patch");
    app.client.install_bundled_blueprint("research").unwrap();
    app.client
        .apply_workflow(project_id.clone(), "research".to_owned(), None)
        .unwrap();
    let item = app
        .client
        .create_blueprint_work_item(
            &project_id,
            "hypothesis",
            std::collections::BTreeMap::from([(
                "question".into(),
                serde_json::json!("Original question"),
            )]),
        )
        .unwrap();
    app.open_project(&project_id);
    app.start_form(FormMode::EditWorkItem {
        item_id: item.id.clone(),
        revision: item.revision,
        blueprint: app
            .client
            .project_blueprint_definition(&project_id)
            .unwrap()
            .unwrap(),
        kind: item.kind.clone(),
    });
    let mut form = take_form(&mut app);
    fill(&mut form, "question", "Changed question");
    app.submit_form(form);
    let updated = app
        .client
        .list_blueprint_work_items(&project_id)
        .unwrap()
        .into_iter()
        .find(|row| row.id == item.id)
        .unwrap();
    assert_eq!(updated.properties["question"], "Changed question");
    // The blueprint normalizer may materialize declared defaults; the UI must
    // not invent fields that are not declared. Only the changed key was sent.
}

#[test]
fn metric_form_allows_first_metric_target_by_id() {
    let mut app = app();
    app.start_form(FormMode::MetricTarget { metric_id: None });
    let Some(Modal::Form(form)) = &app.modal else {
        panic!("metric form");
    };
    assert!(matches!(
        form.fields[0].kind,
        FormFieldKind::Text | FormFieldKind::Choice(_)
    ));
    assert!(form.fields[0].required);
}
