//! Feature-level cases for atomic template instantiation.
use super::*;
use orbit_blueprint::{Blueprint, ProjectTemplate, TemplateItem};
use std::collections::{BTreeMap, BTreeSet};

fn fixture() -> SqliteStore {
    let mut store = SqliteStore::in_memory().unwrap();
    let id = ProjectId::new("template-project").unwrap();
    ProjectRepository::save(&mut store, Project::new(id, "Template Project").unwrap()).unwrap();
    let mut blueprint: Blueprint =
        serde_json::from_str(include_str!("../../../blueprints/development.json")).unwrap();
    blueprint.templates.push(ProjectTemplate {
        id: "starter".into(),
        name: "Starter".into(),
        items: vec![
            TemplateItem {
                key: "design".into(),
                kind: "feature".into(),
                properties: BTreeMap::new(),
                prerequisites: vec![],
            },
            TemplateItem {
                key: "build".into(),
                kind: "feature".into(),
                properties: BTreeMap::new(),
                prerequisites: vec!["design".into()],
            },
        ],
    });
    store
        .publish_blueprint(&blueprint, &BTreeSet::new())
        .unwrap();
    store
        .bind_existing_project_blueprint("template-project", &blueprint.id, blueprint.version)
        .unwrap();
    store
}

#[test]
fn template_writes_items_and_edges_as_a_single_unit() {
    let mut store = fixture();
    let plan = store
        .apply_blueprint_template("template-project", "starter", "batch1")
        .unwrap();
    assert_eq!(plan.items.len(), 2);
    assert_eq!(
        store
            .blueprint_work_item_prerequisites("batch1.build")
            .unwrap(),
        vec!["batch1.design"]
    );
    assert!(
        store
            .blueprint_work_item("batch1.design")
            .unwrap()
            .is_some()
    );
}

#[test]
fn collision_rolls_back_other_items_and_dependencies() {
    let mut store = fixture();
    // The second ID collides after the first insert, proving the savepoint rolls back.
    store
        .create_blueprint_work_item(
            "batch2.build",
            "template-project",
            "feature",
            BTreeMap::new(),
        )
        .unwrap();
    assert!(
        store
            .apply_blueprint_template("template-project", "starter", "batch2")
            .is_err()
    );
    assert!(
        store
            .blueprint_work_item("batch2.design")
            .unwrap()
            .is_none()
    );
    assert!(
        store
            .blueprint_work_item_prerequisites("batch2.build")
            .unwrap()
            .is_empty()
    );
}

#[test]
fn distinct_namespaces_allow_repeated_instantiation() {
    let mut store = fixture();
    store
        .apply_blueprint_template("template-project", "starter", "first")
        .unwrap();
    store
        .apply_blueprint_template("template-project", "starter", "second")
        .unwrap();
    assert!(
        store
            .apply_blueprint_template("template-project", "starter", "first")
            .is_err()
    );
    assert!(store.blueprint_work_item("second.build").unwrap().is_some());
}
