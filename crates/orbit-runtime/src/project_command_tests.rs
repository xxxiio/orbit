//! Project update commands and task timestamp reads over one authority store.
use crate::domain_commands::{
    ProjectCommand, ProjectCommandAction, ProjectCommandRequest, ProjectRecord, TaskAction,
    TaskCommand,
};
use crate::store::{AuthorityError, AuthorityStore, TaskTimestamps};
use std::path::Path;

fn store() -> AuthorityStore {
    let mut store = AuthorityStore::open(Path::new(":memory:")).unwrap();
    store.enroll("device-a", &"a".repeat(40)).unwrap();
    store.enroll("device-b", &"b".repeat(40)).unwrap();
    store
}

fn create(id: &str, revision: i64, project_id: &str, name: &str) -> ProjectCommandRequest {
    ProjectCommandRequest {
        command_id: id.into(),
        expected_revision: revision,
        action: ProjectCommandAction::Create,
        project_id: project_id.into(),
        name: name.into(),
        status: None,
        workflow_id: None,
    }
}

fn update(
    id: &str,
    revision: i64,
    project_id: &str,
    name: &str,
    status: Option<&str>,
) -> ProjectCommandRequest {
    ProjectCommandRequest {
        command_id: id.into(),
        expected_revision: revision,
        action: ProjectCommandAction::Update,
        project_id: project_id.into(),
        name: name.into(),
        status: status.map(str::to_owned),
        workflow_id: None,
    }
}

fn seeded() -> AuthorityStore {
    let mut store = store();
    store
        .authority_project_command("device-a", create("p-create", 0, "orbit", "Orbit"))
        .unwrap();
    store
}

fn project(store: &AuthorityStore, id: &str) -> ProjectRecord {
    store
        .projects()
        .unwrap()
        .1
        .into_iter()
        .find(|project| project.id == id)
        .expect("project exists")
}

#[test]
fn create_is_unchanged_and_the_payload_stays_backwards_compatible() {
    let mut store = store();
    // The historical five-field payload still parses and is accepted as create.
    let legacy: ProjectCommandRequest = serde_json::from_str(
        r#"{"command_id":"p-legacy","expected_revision":0,"project_id":"orbit","name":"Orbit"}"#,
    )
    .unwrap();
    assert_eq!(legacy.action, ProjectCommandAction::Create);
    assert_eq!(legacy.status, None);
    let result = store.authority_project_command("device-a", legacy).unwrap();
    assert_eq!(result.revision, 1);
    assert!(!result.duplicate);
    assert_eq!(result.project.status, "proposed");

    // An initial status is applied through the domain transition rules.
    let mut active = create("p-active", 1, "web", "Web");
    active.status = Some("active".into());
    let result = store.authority_project_command("device-a", active).unwrap();
    assert_eq!(result.project.status, "active");

    // A create with an illegal initial state is refused by the domain.
    let mut completed = create("p-completed", 2, "docs", "Docs");
    completed.status = Some("completed".into());
    assert!(matches!(
        store.authority_project_command("device-a", completed),
        Err(AuthorityError::Application(_))
    ));
    // Failed commands leave no trace.
    assert_eq!(store.tip().unwrap(), 2);
    assert_eq!(store.projects().unwrap().1.len(), 2);
}

#[test]
fn rename_and_status_move_commit_one_change_with_only_the_changed_fields() {
    let mut store = seeded();
    let renamed = store
        .authority_project_command(
            "device-a",
            update("p-rename", 1, "orbit", "Orbit Web", None),
        )
        .unwrap();
    assert_eq!(renamed.revision, 2);
    assert_eq!(renamed.project.name, "Orbit Web");
    assert_eq!(renamed.project.status, "proposed");

    let moved = store
        .authority_project_command(
            "device-a",
            update("p-status", 2, "orbit", "", Some("active")),
        )
        .unwrap();
    assert_eq!(moved.revision, 3);
    assert_eq!(moved.project.status, "active");

    let stored = project(&store, "orbit");
    assert_eq!(stored.name, "Orbit Web");
    assert_eq!(stored.status, "active");

    // One activity per accepted update, with the contract's kinds.
    let activities: Vec<(String, String)> = {
        let mut stmt = store
            .conn
            .prepare("SELECT activity_kind,summary FROM activities WHERE subject_kind='project' ORDER BY sequence")
            .unwrap();
        let rows = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .unwrap();
        rows.collect::<Result<Vec<_>, _>>().unwrap()
    };
    assert_eq!(activities.len(), 2);
    assert_eq!(activities[0].0, "updated");
    assert_eq!(activities[1].0, "status_changed");

    // The ledger carries only the fields each command changed.
    let mutations: Vec<String> = {
        let mut stmt = store
            .conn
            .prepare("SELECT wire FROM authority_history WHERE kind='change' ORDER BY cursor")
            .unwrap();
        let wires = stmt
            .query_map([], |row| row.get::<_, String>(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        wires
            .into_iter()
            .skip(1)
            .map(|wire| {
                let envelope = orbit_sync::ChangeEnvelope::from_wire_json(&wire).unwrap();
                let mut fields: Vec<String> = envelope
                    .change
                    .mutations
                    .iter()
                    .map(|mutation| mutation.field.clone())
                    .collect();
                fields.sort();
                fields.join(",")
            })
            .collect()
    };
    assert_eq!(mutations, vec!["name".to_owned(), "status".to_owned()]);
}

#[test]
fn illegal_status_move_is_refused_by_the_domain_without_writing() {
    let mut store = seeded();
    // `proposed -> completed` is not a lifecycle edge.
    let refused = store.authority_project_command(
        "device-a",
        update("p-illegal", 1, "orbit", "", Some("completed")),
    );
    match refused {
        Err(AuthorityError::Application(orbit_application::ApplicationError::Domain(
            orbit_domain::DomainError::InvalidProjectStatusTransition { from, to },
        ))) => {
            assert_eq!((from, to), ("proposed", "completed"));
        }
        other => panic!("expected a typed domain refusal, got {other:?}"),
    }
    assert_eq!(store.tip().unwrap(), 1);
    assert_eq!(project(&store, "orbit").status, "proposed");
    // Unknown status tokens are refused before the domain is reached.
    assert!(matches!(
        store.authority_project_command(
            "device-a",
            update("p-unknown", 1, "orbit", "", Some("backlog"))
        ),
        Err(AuthorityError::Invalid("invalid project status"))
    ));
    // An update without a name or status is a request error.
    assert!(matches!(
        store.authority_project_command("device-a", update("p-empty", 1, "orbit", "  ", None)),
        Err(AuthorityError::Invalid(
            "project update requires a name or status"
        ))
    ));
    // A missing project is reported as such.
    assert!(matches!(
        store.authority_project_command("device-a", update("p-missing", 1, "nope", "Nope", None)),
        Err(AuthorityError::Invalid("project does not exist"))
    ));
}

#[test]
fn update_fences_on_the_revision_and_replays_its_receipt() {
    let mut store = seeded();
    let rename = update("p-rename", 1, "orbit", "Orbit Web", None);
    store
        .authority_project_command("device-a", rename.clone())
        .unwrap();
    // A retry of the accepted command id returns the receipt, not a new write.
    let replay = store
        .authority_project_command("device-a", rename.clone())
        .unwrap();
    assert!(replay.duplicate);
    assert_eq!(replay.revision, 2);
    assert_eq!(store.tip().unwrap(), 2);

    // The same id with a different payload is an immutable conflict.
    assert!(matches!(
        store.authority_project_command("device-a", update("p-rename", 2, "orbit", "Other", None)),
        Err(AuthorityError::ImmutableConflict)
    ));
    // A stale fence is refused even though the row is untouched.
    assert!(matches!(
        store.authority_project_command("device-a", update("p-stale", 1, "orbit", "Stale", None)),
        Err(AuthorityError::StaleRevision)
    ));
    assert_eq!(store.tip().unwrap(), 2);
    assert_eq!(project(&store, "orbit").name, "Orbit Web");
}

#[test]
fn an_update_that_changes_nothing_writes_nothing_and_fences_the_revision() {
    let mut store = seeded();
    let before = store.tip().unwrap();
    let no_op = store
        .authority_project_command(
            "device-a",
            update("p-noop", 1, "orbit", "Orbit", Some("proposed")),
        )
        .unwrap();
    assert!(!no_op.duplicate);
    assert_eq!(no_op.revision, before);
    assert_eq!(no_op.project.name, "Orbit");
    assert_eq!(no_op.project.status, "proposed");
    // No ledger row, no receipt, no activity: the transaction rolled back.
    assert_eq!(store.tip().unwrap(), before);
    assert_eq!(store.authority_command_count().unwrap(), 1);
    let activities: i64 = store
        .conn
        .query_row("SELECT COUNT(*) FROM activities", [], |row| row.get(0))
        .unwrap();
    assert_eq!(activities, 0);
    // And the fence is real: a no-op at a stale revision is still stale.
    assert!(matches!(
        store.authority_project_command(
            "device-a",
            update("p-noop-stale", 0, "orbit", "Orbit", None)
        ),
        Err(AuthorityError::StaleRevision)
    ));
    // Retrying the no-op at the same revision stays a no-op.
    let again = store
        .authority_project_command(
            "device-a",
            update("p-noop", 1, "orbit", "Orbit", Some("proposed")),
        )
        .unwrap();
    assert!(!again.duplicate);
    assert_eq!(store.tip().unwrap(), before);
}

#[test]
fn task_timestamps_come_from_history_or_report_unknown() {
    let mut store = seeded();
    let task = TaskCommand {
        command_id: "t-create".into(),
        expected_revision: 1,
        action: TaskAction::Create {
            task_id: "task-1".into(),
            title: "Fix the login form".into(),
            project_id: Some("orbit".into()),
        },
    };
    store.authority_task_command("device-a", task).unwrap();
    let created = store.task_timestamps("task-1").unwrap();
    let created_at_ms = created.created_at_ms.expect("a created row exists");
    assert_eq!(created.updated_at_ms, Some(created_at_ms));

    // A real clock gap makes the ordering observable.
    std::thread::sleep(std::time::Duration::from_millis(5));
    store
        .authority_task_command(
            "device-a",
            TaskCommand {
                command_id: "t-rename".into(),
                expected_revision: 2,
                action: TaskAction::Rename {
                    task_id: "task-1".into(),
                    title: "Fix the sign-in form".into(),
                },
            },
        )
        .unwrap();
    let updated = store.task_timestamps("task-1").unwrap();
    assert_eq!(updated.created_at_ms, Some(created_at_ms));
    assert!(
        updated
            .updated_at_ms
            .is_some_and(|value| value > created_at_ms),
        "a rename must advance only the updated moment"
    );

    // No history row means honestly unknown, never a read-time clock value.
    assert_eq!(
        store.task_timestamps("task-missing").unwrap(),
        TaskTimestamps {
            created_at_ms: None,
            updated_at_ms: None
        }
    );
}

#[test]
fn update_cannot_rebind_a_workflow() {
    let mut store = seeded();
    let mut request = update("p-bind", 1, "orbit", "Orbit", None);
    request.workflow_id = Some("basic".into());
    assert!(matches!(
        store.authority_project_command("device-a", request),
        Err(AuthorityError::Invalid(_))
    ));
    assert_eq!(store.tip().unwrap(), 1);
}

#[test]
fn a_create_payload_stays_byte_identical_to_the_historical_command() {
    // The receipt payload is the command's JSON, so a create written by an old
    // client and a create written through the extended payload must compare
    // equal; otherwise an in-flight retry after an upgrade would look like a
    // different command.
    let legacy = ProjectCommand {
        command_id: "c-1".into(),
        expected_revision: 3,
        project_id: "orbit".into(),
        name: "Orbit".into(),
        workflow_id: Some("basic".into()),
    };
    let upgraded: ProjectCommandRequest = legacy.clone().into();
    assert_eq!(
        serde_json::to_string(&legacy).unwrap(),
        serde_json::to_string(&upgraded).unwrap()
    );
    // The additive fields only appear when they carry information.
    let update = ProjectCommandRequest {
        action: ProjectCommandAction::Update,
        name: String::new(),
        status: Some("active".into()),
        ..upgraded
    };
    let wire = serde_json::to_string(&update).unwrap();
    assert!(wire.contains(r#""action":"update""#));
    assert!(wire.contains(r#""status":"active""#));
}
