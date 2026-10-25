//! Compiled acceptance for the explicit blueprint upgrade path.
//!
//! Every test uses the real authoritative host, real SQLite store and real
//! published manifests. Nothing is mocked: the fingerprint, the blockers and
//! the binding are read back from accepted state.
use super::blueprint_upgrade::{BlueprintUpgradeApplied, BlueprintUpgradePreview, codes};
use crate::{AuthoritativeHost, capability_composition::TrustedComposition};
use orbit_application::ProjectRepository;
use orbit_blueprint::{Blueprint, Field, FieldKind, Transition, WorkItemKind};
use orbit_domain::{Project, ProjectId};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

const DEMO_ID: &str = "upgrade-demo";

fn open_host() -> AuthoritativeHost {
    let mut host = AuthoritativeHost::open(":memory:").unwrap();
    for id in ["project-1", "project-2"] {
        let project =
            Project::new(ProjectId::new(id).unwrap(), format!("Project {id}")).expect("project");
        ProjectRepository::save(&mut host.authority.conn, project).expect("save project");
    }
    host
}

/// Version 1 of the demo manifest: one kind, two fields, two transitions.
fn base(version: u32) -> Blueprint {
    serde_json::from_value(json!({
        "id": DEMO_ID,
        "version": version,
        "name": "Upgrade demo",
        "work_items": [{
            "id": "feature",
            "name": "Feature",
            "initial_state": "backlog",
            "states": ["backlog", "ready", "done"],
            "fields": [
                {
                    "id": "priority",
                    "kind": {"type": "choice", "options": ["low", "high"]},
                    "required": true,
                    "default": "low"
                },
                {"id": "notes", "kind": {"type": "text"}}
            ],
            "transitions": [
                {"from": "backlog", "to": "ready"},
                {"from": "ready", "to": "done"}
            ]
        }]
    }))
    .expect("demo manifest")
}

fn feature(blueprint: &mut Blueprint) -> &mut WorkItemKind {
    blueprint
        .work_items
        .iter_mut()
        .find(|kind| kind.id == "feature")
        .expect("feature kind")
}

fn publish(host: &mut AuthoritativeHost, blueprint: &Blueprint) {
    host.publish_project_blueprint(blueprint, &BTreeSet::new())
        .expect("publish manifest");
}

fn publish_with_capabilities(
    host: &mut AuthoritativeHost,
    blueprint: &Blueprint,
    installed: &BTreeSet<String>,
) {
    host.publish_project_blueprint(blueprint, installed)
        .expect("publish manifest");
}

fn bind(host: &mut AuthoritativeHost, project: &str, version: u32) {
    host.activate_project_blueprint(project, DEMO_ID, version)
        .expect("bind project");
}

fn notes(value: &str) -> BTreeMap<String, Value> {
    BTreeMap::from([("notes".to_owned(), json!(value))])
}

fn codes_of(notices: &[super::blueprint_upgrade::UpgradeNotice]) -> Vec<String> {
    notices.iter().map(|notice| notice.code.clone()).collect()
}

fn binding_version(host: &AuthoritativeHost, project: &str) -> Option<u32> {
    host.active_project_blueprint(project)
        .unwrap()
        .map(|binding| binding.version)
}

fn preview(host: &AuthoritativeHost, version: u32) -> BlueprintUpgradePreview {
    host.preview_blueprint_upgrade("project-1", DEMO_ID, version)
        .expect("preview")
}

#[test]
fn preview_of_an_unpublished_target_is_an_error() {
    let mut host = open_host();
    publish(&mut host, &base(1));
    bind(&mut host, "project-1", 1);
    assert!(
        host.preview_blueprint_upgrade("project-1", DEMO_ID, 2)
            .is_err(),
        "a target that is not published has nothing to compare against"
    );
    assert!(
        host.preview_blueprint_upgrade("project-1", "absent", 1)
            .is_err()
    );
    assert!(
        host.preview_blueprint_upgrade("absent-project", DEMO_ID, 1)
            .is_err()
    );
}

#[test]
fn identical_versions_preview_with_no_changes_and_a_fingerprint() {
    let mut host = open_host();
    publish(&mut host, &base(1));
    bind(&mut host, "project-1", 1);

    let report = preview(&host, 1);
    assert_eq!(report.current.as_ref().unwrap().version, 1);
    assert_eq!(report.target.version, 1);
    // The same accepted state always produces the same fingerprint.
    assert_eq!(preview(&host, 1).fingerprint, report.fingerprint);
    assert!(
        report.kinds.is_empty(),
        "no kind changed: {:?}",
        report.kinds
    );
    assert!(report.views.added.is_empty());
    assert!(report.views.removed.is_empty());
    assert!(report.views.changed.is_empty());
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    assert!(report.blockers.is_empty(), "{:?}", report.blockers);
    assert_eq!(report.fingerprint.len(), 64);
    assert!(
        report
            .fingerprint
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    );
    // A read-only preview must never change the binding.
    assert_eq!(binding_version(&host, "project-1"), Some(1));
}

#[test]
fn report_lists_added_kinds_states_fields_actions_and_removed_transitions() {
    let mut host = open_host();
    let composition = TrustedComposition::with_builtin_reference();
    host.install_trusted_capability(
        &composition,
        crate::capability_composition::REFERENCE_CAPABILITY_ID,
        1,
    )
    .unwrap();
    let installed = host.installed_capability_ids(&composition).unwrap();
    publish_with_capabilities(&mut host, &base(1), &BTreeSet::new());
    bind(&mut host, "project-1", 1);

    let mut target = base(2);
    target.required_capabilities =
        vec![crate::capability_composition::REFERENCE_CAPABILITY_ID.into()];
    let kind = feature(&mut target);
    kind.states.push("blocked".into());
    kind.fields.push(Field {
        id: "owner".into(),
        kind: FieldKind::Text,
        required: false,
        default: None,
    });
    kind.transitions
        .retain(|edge| !(edge.from == "ready" && edge.to == "done"));
    kind.transitions[0]
        .actions
        .push(orbit_blueprint::WorkflowAction {
            id: "note".into(),
            capability: crate::capability_composition::REFERENCE_CAPABILITY_ID.into(),
            description: "Append a note".into(),
            parameters: BTreeMap::new(),
        });
    target.work_items.push(WorkItemKind {
        id: "bug".into(),
        name: "Bug".into(),
        initial_state: "open".into(),
        states: vec!["open".into(), "fixed".into()],
        fields: Vec::new(),
        transitions: vec![Transition {
            from: "open".into(),
            to: "fixed".into(),
            required_capability: None,
            guards: Vec::new(),
            actions: Vec::new(),
        }],
    });
    publish_with_capabilities(&mut host, &target, &installed);

    let report = preview(&host, 2);
    assert!(report.blockers.is_empty(), "{:?}", report.blockers);
    let feature_diff = report
        .kinds
        .iter()
        .find(|diff| diff.kind == "feature")
        .expect("feature diff");
    assert!(feature_diff.states_added.contains(&"blocked".to_owned()));
    assert!(
        feature_diff
            .fields_added
            .iter()
            .any(|field| field.id == "owner")
    );
    assert!(
        feature_diff
            .transitions_removed
            .contains(&"ready->done".to_owned())
    );
    assert!(feature_diff.actions_added.iter().any(|action| {
        action.transition == "backlog->ready"
            && action.action == "note"
            && action.capability == crate::capability_composition::REFERENCE_CAPABILITY_ID
    }));
    assert!(
        feature_diff
            .required_capabilities_added
            .contains(&crate::capability_composition::REFERENCE_CAPABILITY_ID.to_owned())
    );
    assert!(
        report
            .kinds
            .iter()
            .any(|diff| diff.kind == "bug" && diff.added)
    );
    let warning_codes = codes_of(&report.warnings);
    for code in [
        codes::STATE_ADDED,
        codes::FIELD_ADDED,
        codes::ACTION_ADDED,
        codes::TRANSITION_REMOVED,
        codes::KIND_ADDED,
    ] {
        assert!(
            warning_codes.contains(&code.to_owned()),
            "missing {code}: {warning_codes:?}"
        );
    }
    // Removed transitions are a warning, never a blocker.
    assert!(
        report
            .warnings
            .iter()
            .any(|notice| notice.code == codes::TRANSITION_REMOVED)
    );
}

#[test]
fn removed_kind_with_a_live_item_blocks_application() {
    let mut host = open_host();
    publish(&mut host, &base(1));
    bind(&mut host, "project-1", 1);
    host.create_project_work_item("item-1", "project-1", "feature", notes("kept"))
        .unwrap();

    let mut target = base(2);
    target.work_items.clear();
    target.work_items.push(WorkItemKind {
        id: "task".into(),
        name: "Task".into(),
        initial_state: "open".into(),
        states: vec!["open".into()],
        fields: Vec::new(),
        transitions: Vec::new(),
    });
    publish(&mut host, &target);

    let report = preview(&host, 2);
    let blocker = report
        .blockers
        .iter()
        .find(|notice| notice.code == codes::KIND_REMOVED_WITH_LIVE_ITEMS)
        .unwrap_or_else(|| panic!("missing blocker: {:?}", report.blockers));
    assert_eq!(blocker.subjects, vec!["item-1".to_owned()]);

    let error = host
        .apply_blueprint_upgrade("project-1", DEMO_ID, 2, &report.fingerprint, true)
        .expect_err("blockers must refuse application");
    assert!(
        format!("{error:?}").contains(codes::KIND_REMOVED_WITH_LIVE_ITEMS),
        "{error:?}"
    );
    assert_eq!(binding_version(&host, "project-1"), Some(1));
    assert_eq!(host.project_work_items("project-1").unwrap().len(), 1);
}

#[test]
fn removed_state_with_a_live_item_blocks_application() {
    let mut host = open_host();
    publish(&mut host, &base(1));
    bind(&mut host, "project-1", 1);
    host.create_project_work_item("item-1", "project-1", "feature", notes("kept"))
        .unwrap();
    let moved = host
        .transition_project_work_item_authorized("item-1", "ready", 0)
        .unwrap();
    assert_eq!(moved.state, "ready");

    let mut target = base(2);
    let kind = feature(&mut target);
    kind.states.retain(|state| state != "ready");
    kind.transitions
        .retain(|edge| edge.from != "ready" && edge.to != "ready");
    publish(&mut host, &target);

    let report = preview(&host, 2);
    assert!(
        report
            .blockers
            .iter()
            .any(|notice| notice.code == codes::STATE_REMOVED_WITH_LIVE_ITEMS
                && notice.subjects == vec!["item-1".to_owned()]),
        "{:?}",
        report.blockers
    );
    assert!(
        host.apply_blueprint_upgrade("project-1", DEMO_ID, 2, &report.fingerprint, true)
            .is_err()
    );
    assert_eq!(binding_version(&host, "project-1"), Some(1));
    assert_eq!(
        host.project_work_item("item-1").unwrap().unwrap().state,
        "ready"
    );
}

#[test]
fn incompatible_field_type_change_with_a_live_value_is_a_blocker() {
    let mut host = open_host();
    publish(&mut host, &base(1));
    bind(&mut host, "project-1", 1);
    host.create_project_work_item("item-1", "project-1", "feature", notes("hello"))
        .unwrap();

    let mut target = base(2);
    let kind = feature(&mut target);
    let field = kind.fields.iter_mut().find(|f| f.id == "notes").unwrap();
    field.kind = FieldKind::Boolean;
    publish(&mut host, &target);

    let report = preview(&host, 2);
    assert!(
        report
            .blockers
            .iter()
            .any(|notice| notice.code == codes::FIELD_TYPE_INCOMPATIBLE
                && notice.subjects == vec!["item-1".to_owned()]),
        "{:?}",
        report.blockers
    );
    assert!(
        host.apply_blueprint_upgrade("project-1", DEMO_ID, 2, &report.fingerprint, true)
            .is_err()
    );
    assert_eq!(binding_version(&host, "project-1"), Some(1));
}

#[test]
fn compatible_type_widening_is_a_warning_and_applies() {
    let mut host = open_host();
    publish(&mut host, &base(1));
    bind(&mut host, "project-1", 1);
    host.create_project_work_item("item-1", "project-1", "feature", notes("hello"))
        .unwrap();

    let mut target = base(2);
    let kind = feature(&mut target);
    let field = kind.fields.iter_mut().find(|f| f.id == "notes").unwrap();
    field.kind = FieldKind::Choice {
        options: vec!["hello".into(), "review".into()],
    };
    publish(&mut host, &target);

    let report = preview(&host, 2);
    assert!(report.blockers.is_empty(), "{:?}", report.blockers);
    assert!(
        report
            .warnings
            .iter()
            .any(|notice| notice.code == codes::FIELD_TYPE_WIDENED),
        "{:?}",
        report.warnings
    );

    let applied = host
        .apply_blueprint_upgrade("project-1", DEMO_ID, 2, &report.fingerprint, true)
        .expect("compatible upgrade applies");
    assert_eq!(applied.binding.version, 2);
    assert_eq!(applied.previous.as_ref().unwrap().version, 1);
    assert!(!applied.already_applied);
    assert_eq!(
        host.project_work_item("item-1")
            .unwrap()
            .unwrap()
            .properties
            .get("notes"),
        Some(&json!("hello"))
    );
}

#[test]
fn removing_an_unused_choice_option_is_a_warning() {
    let mut host = open_host();
    publish(&mut host, &base(1));
    bind(&mut host, "project-1", 1);
    host.create_project_work_item("item-1", "project-1", "feature", notes("kept"))
        .unwrap();

    let mut target = base(2);
    let kind = feature(&mut target);
    let field = kind.fields.iter_mut().find(|f| f.id == "priority").unwrap();
    field.kind = FieldKind::Choice {
        options: vec!["low".into(), "medium".into()],
    };
    publish(&mut host, &target);

    let report = preview(&host, 2);
    assert!(report.blockers.is_empty(), "{:?}", report.blockers);
    assert!(
        report
            .warnings
            .iter()
            .any(|notice| notice.code == codes::FIELD_CHOICE_OPTIONS_TRIMMED),
        "{:?}",
        report.warnings
    );
    host.apply_blueprint_upgrade("project-1", DEMO_ID, 2, &report.fingerprint, true)
        .expect("unused option removal applies");
    assert_eq!(binding_version(&host, "project-1"), Some(2));
}

#[test]
fn apply_refuses_without_confirm_wrong_fingerprint_or_a_stale_preview() {
    let mut host = open_host();
    publish(&mut host, &base(1));
    bind(&mut host, "project-1", 1);
    host.create_project_work_item("item-1", "project-1", "feature", notes("kept"))
        .unwrap();
    publish(&mut host, &base(2));

    let report = preview(&host, 2);
    assert!(report.blockers.is_empty(), "{:?}", report.blockers);

    assert!(
        host.apply_blueprint_upgrade("project-1", DEMO_ID, 2, &report.fingerprint, false)
            .is_err(),
        "confirmation is mandatory"
    );
    assert_eq!(binding_version(&host, "project-1"), Some(1));

    assert!(
        host.apply_blueprint_upgrade("project-1", DEMO_ID, 2, "not-the-fingerprint", true)
            .is_err()
    );
    assert_eq!(binding_version(&host, "project-1"), Some(1));

    // Any relevant change after the preview invalidates the fingerprint.
    host.create_project_work_item("item-2", "project-1", "feature", notes("later"))
        .unwrap();
    let error = host
        .apply_blueprint_upgrade("project-1", DEMO_ID, 2, &report.fingerprint, true)
        .expect_err("stale preview must be refused");
    assert!(
        format!("{error:?}").contains("fingerprint mismatch"),
        "{error:?}"
    );
    assert_eq!(binding_version(&host, "project-1"), Some(1));
    assert_eq!(host.project_work_items("project-1").unwrap().len(), 2);

    // The freshly computed fingerprint is accepted.
    let refreshed = preview(&host, 2);
    assert_ne!(refreshed.fingerprint, report.fingerprint);
    let applied = host
        .apply_blueprint_upgrade("project-1", DEMO_ID, 2, &refreshed.fingerprint, true)
        .expect("refreshed preview applies");
    assert_eq!(applied.binding.version, 2);
}

#[test]
fn successful_apply_preserves_items_is_idempotent_and_spares_other_projects() {
    let mut host = open_host();
    publish(&mut host, &base(1));
    bind(&mut host, "project-1", 1);
    bind(&mut host, "project-2", 1);
    host.create_project_work_item("item-1", "project-1", "feature", notes("kept"))
        .unwrap();
    let moved = host
        .transition_project_work_item_authorized("item-1", "ready", 0)
        .unwrap();
    assert_eq!(moved.revision, 1);
    host.create_project_work_item("item-2", "project-2", "feature", notes("other"))
        .unwrap();

    let mut target = base(2);
    let kind = feature(&mut target);
    kind.states.push("blocked".into());
    kind.fields.push(Field {
        id: "owner".into(),
        kind: FieldKind::Text,
        required: false,
        default: None,
    });
    publish(&mut host, &target);

    let report = preview(&host, 2);
    assert!(report.blockers.is_empty(), "{:?}", report.blockers);
    let applied: BlueprintUpgradeApplied = host
        .apply_blueprint_upgrade("project-1", DEMO_ID, 2, &report.fingerprint, true)
        .expect("apply");
    assert_eq!(applied.binding.project_id, "project-1");
    assert_eq!(applied.binding.blueprint_id, DEMO_ID);
    assert_eq!(applied.binding.version, 2);
    assert_eq!(applied.preview.fingerprint, report.fingerprint);

    // Work items are never rewritten by an upgrade.
    let preserved = host.project_work_item("item-1").unwrap().unwrap();
    assert_eq!(preserved.state, "ready");
    assert_eq!(preserved.kind, "feature");
    assert_eq!(preserved.revision, 1);
    assert_eq!(preserved.properties.get("notes"), Some(&json!("kept")));
    assert_eq!(preserved.properties.get("priority"), Some(&json!("low")));

    // The new schema is live: the field added by version 2 is accepted.
    let updated = host
        .update_project_work_item_properties(
            "item-1",
            1,
            BTreeMap::from([("owner".to_owned(), json!("alice"))]),
        )
        .expect("target schema is in force");
    assert_eq!(updated.revision, 2);
    assert_eq!(updated.properties.get("owner"), Some(&json!("alice")));

    // Repeating the identical operation is idempotent.
    let replay = host
        .apply_blueprint_upgrade("project-1", DEMO_ID, 2, &report.fingerprint, true)
        .expect("replay");
    assert!(replay.already_applied);
    assert_eq!(replay.binding.version, 2);
    assert_eq!(binding_version(&host, "project-1"), Some(2));

    // No other project was rebound or modified.
    assert_eq!(binding_version(&host, "project-2"), Some(1));
    let untouched = host.project_work_item("item-2").unwrap().unwrap();
    assert_eq!(untouched.revision, 0);
    assert_eq!(untouched.state, "backlog");
}

#[test]
fn publishing_or_installing_a_newer_version_never_rebinds() {
    let mut host = open_host();
    publish(&mut host, &base(1));
    bind(&mut host, "project-1", 1);
    bind(&mut host, "project-2", 1);

    publish(&mut host, &base(2));
    publish(&mut host, &base(3));
    assert_eq!(binding_version(&host, "project-1"), Some(1));
    assert_eq!(binding_version(&host, "project-2"), Some(1));

    // The explicit (non-upgrade) `bind` path still refuses a second binding.
    assert!(
        host.activate_project_blueprint("project-1", DEMO_ID, 2)
            .is_err()
    );
    assert_eq!(binding_version(&host, "project-1"), Some(1));

    // Installing an unrelated bundled workflow binds nobody.
    host.install_bundled_project_blueprint("development")
        .unwrap();
    assert_eq!(binding_version(&host, "project-1"), Some(1));
    assert_eq!(binding_version(&host, "project-2"), Some(1));
}

#[test]
fn a_missing_required_capability_is_a_blocker_not_a_silent_allow() {
    let mut host = open_host();
    let composition = TrustedComposition::with_builtin_reference();
    let capability = crate::capability_composition::REFERENCE_CAPABILITY_ID;
    host.install_trusted_capability(&composition, capability, 1)
        .unwrap();
    let installed = host.installed_capability_ids(&composition).unwrap();
    publish_with_capabilities(&mut host, &base(1), &BTreeSet::new());
    bind(&mut host, "project-1", 1);

    let mut target = base(2);
    target.required_capabilities = vec![capability.into()];
    feature(&mut target).transitions[0]
        .actions
        .push(orbit_blueprint::WorkflowAction {
            id: "note".into(),
            capability: capability.into(),
            description: "Append a note".into(),
            parameters: BTreeMap::new(),
        });
    publish_with_capabilities(&mut host, &target, &installed);

    // The target published while the capability was installed. Uninstalling
    // it must turn the preview into a blocker, never a silent allow.
    host.uninstall_trusted_capability(capability).unwrap();
    let report = preview(&host, 2);
    assert!(
        report.blockers.iter().any(
            |notice| notice.code == codes::REQUIRED_CAPABILITY_UNAVAILABLE
                && notice.subjects == vec![capability.to_owned()]
        ),
        "{:?}",
        report.blockers
    );
    assert!(
        host.apply_blueprint_upgrade("project-1", DEMO_ID, 2, &report.fingerprint, true)
            .is_err()
    );
    assert_eq!(binding_version(&host, "project-1"), Some(1));
}

#[test]
fn unbound_project_previews_and_applies_as_a_first_binding() {
    let mut host = open_host();
    publish(&mut host, &base(1));

    let report = preview(&host, 1);
    assert!(report.current.is_none());
    assert!(report.blockers.is_empty(), "{:?}", report.blockers);
    assert_eq!(report.target.version, 1);
    assert!(
        !report.kinds.is_empty(),
        "first binding adds the whole schema"
    );

    let applied = host
        .apply_blueprint_upgrade("project-1", DEMO_ID, 1, &report.fingerprint, true)
        .unwrap();
    assert_eq!(applied.binding.version, 1);
    assert!(applied.previous.is_none());
    assert_eq!(binding_version(&host, "project-1"), Some(1));
}
