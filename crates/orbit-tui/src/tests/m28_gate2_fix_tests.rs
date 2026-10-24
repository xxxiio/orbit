use crate::app::{FormMode, Modal};
use crate::*;
use ratatui::backend::TestBackend;

fn app() -> App {
    let path = std::env::temp_dir().join(format!(
        "orbit-tui-m28-g2-{}-{}.sqlite",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let _ = std::fs::remove_file(&path);
    App::open(path.to_string_lossy().into_owned()).unwrap()
}

#[test]
fn form_errors_are_rendered_above_the_fields() {
    let mut app = app();
    app.start_form(FormMode::NewTask { project_id: None });
    let Some(Modal::Form(mut form)) = app.modal.take() else {
        panic!("form");
    };
    form.error = Some("title is required".into());
    app.modal = Some(Modal::Form(form));
    let backend = TestBackend::new(100, 30);
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
    assert!(rendered.contains("error: title is required"), "{rendered}");
}

#[test]
fn duplicate_enter_keeps_the_invalid_form_open() {
    let mut app = app();
    app.start_form(FormMode::NewTask { project_id: None });
    handle_key(&mut app, KeyCode::Enter, KeyModifiers::NONE).unwrap();
    assert!(matches!(app.modal, Some(Modal::Form(_))));
    handle_key(&mut app, KeyCode::Enter, KeyModifiers::NONE).unwrap();
    assert!(matches!(app.modal, Some(Modal::Form(_))));
}

#[test]
fn estimate_upper_bound_is_rejected_before_creation() {
    let mut app = app();
    app.start_form(FormMode::NewTask { project_id: None });
    let Some(Modal::Form(mut form)) = app.modal.take() else {
        panic!("form");
    };
    form.fields[0].value = "Too long".into();
    form.fields
        .iter_mut()
        .find(|field| field.key == "estimate")
        .unwrap()
        .value = "525601".into();
    form.dirty = true;
    app.submit_form(form);
    assert!(matches!(app.modal, Some(Modal::Form(_))));
    assert!(app.client.list_tasks().unwrap().is_empty());
}

#[test]
fn unchanged_work_item_edit_does_not_bump_revision() {
    let mut app = app();
    app.client
        .create_project(orbit_client::CreateProjectRequest {
            id: None,
            name: "Noop".into(),
        })
        .unwrap();
    app.refresh_workspace();
    let project = app.projects[0].id.clone();
    app.client.install_bundled_blueprint("research").unwrap();
    app.client
        .apply_workflow(project.clone(), "research".to_owned(), None)
        .unwrap();
    let item = app
        .client
        .create_blueprint_work_item(
            &project,
            "hypothesis",
            std::collections::BTreeMap::from([("question".into(), serde_json::json!("No change"))]),
        )
        .unwrap();
    app.open_project(&project);
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
    let Some(Modal::Form(form)) = app.modal.take() else {
        panic!("form");
    };
    app.submit_form(form);
    assert!(app.modal.is_none());
    let after = app
        .client
        .list_blueprint_work_items(&project)
        .unwrap()
        .into_iter()
        .find(|row| row.id == item.id)
        .unwrap();
    assert_eq!(after.revision, item.revision);
}
