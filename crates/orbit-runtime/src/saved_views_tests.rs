//! Saved views: version fencing, receipts, actor isolation and the guarantee
//! that a saved view never moves the authority revision.
use crate::saved_views::{SavedViewCommand, SavedViewResult};
use crate::store::{AuthorityError, AuthorityStore};
use std::path::Path;

fn store() -> AuthorityStore {
    let mut store = AuthorityStore::open(Path::new(":memory:")).unwrap();
    store.enroll("device-a", &"a".repeat(40)).unwrap();
    store.enroll("device-b", &"b".repeat(40)).unwrap();
    store
}

fn save(command_id: &str, view_id: &str, expected_version: i64, name: &str) -> SavedViewCommand {
    SavedViewCommand::Save {
        command_id: command_id.into(),
        view_id: view_id.into(),
        expected_version,
        name: name.into(),
        scope: "project".into(),
        project_id: Some("orbit".into()),
        query: "status=ready&sort=due".into(),
        display: "list".into(),
    }
}

fn rename(command_id: &str, view_id: &str, expected_version: i64, name: &str) -> SavedViewCommand {
    SavedViewCommand::Rename {
        command_id: command_id.into(),
        view_id: view_id.into(),
        expected_version,
        name: name.into(),
    }
}

fn delete(command_id: &str, view_id: &str, expected_version: i64) -> SavedViewCommand {
    SavedViewCommand::Delete {
        command_id: command_id.into(),
        view_id: view_id.into(),
        expected_version,
    }
}

/// Authority state that saved views must never touch.
fn authority_fingerprint(store: &AuthorityStore) -> (i64, i64, i64, i64) {
    let commands: i64 = store
        .conn
        .query_row("SELECT COUNT(*) FROM authority_commands", [], |row| {
            row.get(0)
        })
        .unwrap();
    let history: i64 = store
        .conn
        .query_row("SELECT COUNT(*) FROM authority_history", [], |row| {
            row.get(0)
        })
        .unwrap();
    let projection: i64 = store
        .conn
        .query_row("SELECT COUNT(*) FROM authority_projection", [], |row| {
            row.get(0)
        })
        .unwrap();
    (store.tip().unwrap(), commands, history, projection)
}

#[test]
fn save_rename_delete_round_trip_with_per_row_versions() {
    let mut store = store();
    let before = authority_fingerprint(&store);

    let saved = store
        .saved_view_command("device-a", save("c-1", "sv-1", 0, "Ready work"))
        .unwrap();
    assert!(!saved.duplicate);
    let view = saved.view.expect("a save returns the row");
    assert_eq!(view.version, 1);
    assert_eq!(view.name, "Ready work");
    assert_eq!(view.scope, "project");
    assert_eq!(view.project_id.as_deref(), Some("orbit"));
    assert_eq!(view.query, "status=ready&sort=due");
    assert_eq!(view.display, "list");
    assert_eq!(view.created_at_ms, view.updated_at_ms);

    let listed = store.saved_views("device-a").unwrap();
    assert_eq!(listed, vec![view.clone()]);

    let renamed = store
        .saved_view_command("device-a", rename("c-2", "sv-1", 1, "Ready queue"))
        .unwrap();
    let renamed = renamed.view.expect("a rename returns the row");
    assert_eq!(renamed.version, 2);
    assert_eq!(renamed.name, "Ready queue");
    assert_eq!(renamed.created_at_ms, view.created_at_ms);
    assert!(renamed.updated_at_ms >= view.updated_at_ms);
    assert_eq!(
        renamed.query, "status=ready&sort=due",
        "a rename keeps the stored configuration"
    );

    let deleted = store
        .saved_view_command("device-a", delete("c-3", "sv-1", 2))
        .unwrap();
    assert!(!deleted.duplicate);
    assert_eq!(deleted.view, None, "a delete returns no view");
    assert!(store.saved_views("device-a").unwrap().is_empty());

    // Not one authority tuple moved: no revision, ledger, receipt or projection.
    assert_eq!(authority_fingerprint(&store), before);
}

#[test]
fn a_stale_version_is_refused_and_leave_the_row_untouched() {
    let mut store = store();
    store
        .saved_view_command("device-a", save("c-1", "sv-1", 0, "Ready work"))
        .unwrap();
    store
        .saved_view_command("device-a", rename("c-2", "sv-1", 1, "Ready queue"))
        .unwrap();

    for command in [
        rename("c-3", "sv-1", 1, "Stale rename"),
        delete("c-4", "sv-1", 1),
        save("c-5", "sv-1", 1, "Stale save"),
    ] {
        assert!(
            matches!(
                store.saved_view_command("device-a", command),
                Err(AuthorityError::Invalid("saved view changed on the server"))
            ),
            "a stale per-row version must be refused"
        );
    }
    let listed = store.saved_views("device-a").unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].name, "Ready queue");
    assert_eq!(listed[0].version, 2);
}

#[test]
fn another_actors_view_is_reported_as_missing() {
    let mut store = store();
    store
        .saved_view_command("device-a", save("c-1", "sv-1", 0, "Ready work"))
        .unwrap();

    assert!(store.saved_views("device-b").unwrap().is_empty());
    assert!(matches!(
        store.saved_view_command("device-b", rename("c-2", "sv-1", 1, "Hijacked")),
        Err(AuthorityError::Invalid("saved view does not exist"))
    ));
    assert!(matches!(
        store.saved_view_command("device-b", delete("c-3", "sv-1", 1)),
        Err(AuthorityError::Invalid("saved view does not exist"))
    ));
    // A different actor cannot overwrite it through a save either.
    assert!(matches!(
        store.saved_view_command("device-b", save("c-4", "sv-1", 1, "Hijacked")),
        Err(AuthorityError::Invalid("saved view does not exist"))
    ));
    let listed = store.saved_views("device-a").unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].name, "Ready work");
}

#[test]
fn a_command_id_replays_its_receipt_and_never_mutates_twice() {
    let mut store = store();
    let command = save("c-1", "sv-1", 0, "Ready work");
    let first = store
        .saved_view_command("device-a", command.clone())
        .unwrap();
    assert!(!first.duplicate);

    // The retry answers from the stored receipt, even after a later edit.
    store
        .saved_view_command("device-a", rename("c-2", "sv-1", 1, "Ready queue"))
        .unwrap();
    let replay = store
        .saved_view_command("device-a", command.clone())
        .unwrap();
    assert!(replay.duplicate);
    assert_eq!(replay.view.expect("receipt carries the view").version, 1);
    let listed = store.saved_views("device-a").unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].name, "Ready queue", "the replay wrote nothing");

    // The same id with a different payload is immutable.
    assert!(matches!(
        store.saved_view_command("device-a", save("c-1", "sv-2", 0, "Other")),
        Err(AuthorityError::ImmutableConflict)
    ));
    // A delete receipt replays as a null view.
    store
        .saved_view_command("device-a", delete("c-3", "sv-1", 2))
        .unwrap();
    let replayed = store
        .saved_view_command("device-a", delete("c-3", "sv-1", 2))
        .unwrap();
    assert!(replayed.duplicate);
    assert_eq!(replayed.view, None);
}

#[test]
fn validation_refusals_use_the_contract_text() {
    let mut store = store();
    let mut invalid_name = save("c-1", "sv-1", 0, "   ");
    if let SavedViewCommand::Save { name, .. } = &mut invalid_name {
        *name = "   ".into();
    }
    assert!(matches!(
        store.saved_view_command("device-a", invalid_name),
        Err(AuthorityError::Invalid("invalid saved view name"))
    ));
    let too_long = "n".repeat(crate::saved_views::MAX_NAME_CHARS + 1);
    assert!(matches!(
        store.saved_view_command("device-a", save("c-2", "sv-1", 0, &too_long)),
        Err(AuthorityError::Invalid("invalid saved view name"))
    ));

    let mut bad_scope = save("c-3", "sv-1", 0, "Ready work");
    if let SavedViewCommand::Save { scope, .. } = &mut bad_scope {
        *scope = "timeline".into();
    }
    assert!(matches!(
        store.saved_view_command("device-a", bad_scope),
        Err(AuthorityError::Invalid("invalid saved view scope"))
    ));

    let mut bad_display = save("c-4", "sv-1", 0, "Ready work");
    if let SavedViewCommand::Save { display, .. } = &mut bad_display {
        *display = "calendar".into();
    }
    assert!(matches!(
        store.saved_view_command("device-a", bad_display),
        Err(AuthorityError::Invalid("invalid saved view display"))
    ));

    let mut long_query = save("c-5", "sv-1", 0, "Ready work");
    if let SavedViewCommand::Save { query, .. } = &mut long_query {
        *query = "q".repeat(crate::saved_views::MAX_QUERY_CHARS + 1);
    }
    assert!(matches!(
        store.saved_view_command("device-a", long_query),
        Err(AuthorityError::Invalid("saved view query too long"))
    ));

    store
        .saved_view_command("device-a", save("c-6", "sv-1", 0, "Ready work"))
        .unwrap();
    // An existing id cannot be created twice.
    assert!(matches!(
        store.saved_view_command("device-a", save("c-7", "sv-1", 0, "Again")),
        Err(AuthorityError::Invalid("saved view already exists"))
    ));
    // Renaming something that does not exist is a missing-row refusal.
    assert!(matches!(
        store.saved_view_command("device-a", rename("c-8", "sv-missing", 1, "Nowhere")),
        Err(AuthorityError::Invalid("saved view does not exist"))
    ));
    assert_eq!(store.saved_views("device-a").unwrap().len(), 1);
}

#[test]
fn the_list_is_ordered_by_last_change_then_id() {
    let mut store = store();
    store
        .saved_view_command("device-a", save("c-1", "sv-a", 0, "Alpha"))
        .unwrap();
    // A real clock gap makes "most recent" observable under a coarse clock.
    std::thread::sleep(std::time::Duration::from_millis(5));
    store
        .saved_view_command("device-a", save("c-2", "sv-b", 0, "Beta"))
        .unwrap();
    assert_eq!(
        store
            .saved_views("device-a")
            .unwrap()
            .iter()
            .map(|view| view.id.as_str())
            .collect::<Vec<_>>(),
        vec!["sv-b", "sv-a"],
        "the most recently written view comes first"
    );
    std::thread::sleep(std::time::Duration::from_millis(5));
    store
        .saved_view_command("device-a", rename("c-3", "sv-a", 1, "Alpha again"))
        .unwrap();
    assert_eq!(
        store
            .saved_views("device-a")
            .unwrap()
            .iter()
            .map(|view| view.id.as_str())
            .collect::<Vec<_>>(),
        vec!["sv-a", "sv-b"],
        "the touched row moves first"
    );
    // Equal timestamps fall back to the id, which keeps the order total.
    store
        .conn
        .execute("UPDATE web_saved_views SET updated_at_ms=1", [])
        .unwrap();
    assert_eq!(
        store
            .saved_views("device-a")
            .unwrap()
            .iter()
            .map(|view| view.id.as_str())
            .collect::<Vec<_>>(),
        vec!["sv-a", "sv-b"]
    );
}

#[test]
fn a_deleted_view_does_not_reappear_in_the_list_or_accept_renames() {
    let mut store = store();
    store
        .saved_view_command("device-a", save("c-1", "sv-1", 0, "Ready work"))
        .unwrap();
    store
        .saved_view_command("device-a", delete("c-2", "sv-1", 1))
        .unwrap();
    assert!(store.saved_views("device-a").unwrap().is_empty());
    assert!(matches!(
        store.saved_view_command("device-a", rename("c-3", "sv-1", 2, "Back")),
        Err(AuthorityError::Invalid("saved view does not exist"))
    ));
    // A soft-deleted row still occupies its id.
    assert!(matches!(
        store.saved_view_command("device-a", save("c-4", "sv-1", 0, "Reborn")),
        Err(AuthorityError::Invalid("saved view already exists"))
    ));
}

#[test]
fn saved_view_commands_and_the_authority_ledger_share_no_state() {
    let mut store = store();
    // Accepted authority work to compare against.
    store
        .authority_project_command(
            "device-a",
            crate::domain_commands::ProjectCommandRequest {
                command_id: "p-create".into(),
                expected_revision: 0,
                action: crate::domain_commands::ProjectCommandAction::Create,
                project_id: "orbit".into(),
                name: "Orbit".into(),
                status: None,
                workflow_id: None,
            },
        )
        .unwrap();
    let before = authority_fingerprint(&store);
    assert_eq!(before.0, 1);
    let result: SavedViewResult = store
        .saved_view_command("device-a", save("c-1", "sv-1", 0, "Ready work"))
        .unwrap();
    assert!(result.view.is_some());
    store
        .saved_view_command("device-a", rename("c-2", "sv-1", 1, "Ready queue"))
        .unwrap();
    store
        .saved_view_command("device-a", delete("c-3", "sv-1", 2))
        .unwrap();
    assert_eq!(
        authority_fingerprint(&store),
        before,
        "saved views must not advance the revision or touch the ledger"
    );
    // The receipt table is separate from the authority command log.
    let saved_receipts: i64 = store
        .conn
        .query_row("SELECT COUNT(*) FROM web_saved_view_commands", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(saved_receipts, 3);
}

#[test]
fn the_command_union_is_tagged_and_rejects_unknown_fields() {
    let delete: SavedViewCommand = serde_json::from_str(
        r#"{"type":"delete","command_id":"c-1","view_id":"sv-1","expected_version":1}"#,
    )
    .expect("tagged delete parses");
    assert!(matches!(
        delete,
        SavedViewCommand::Delete {
            expected_version: 1,
            ..
        }
    ));
    let save: SavedViewCommand = serde_json::from_str(
        r#"{"type":"save","command_id":"c-2","view_id":"sv-1","expected_version":0,"name":"Ready","scope":"home","query":"","display":"board"}"#,
    )
    .expect("project_id is optional");
    assert!(matches!(save, SavedViewCommand::Save { .. }));
    // Unknown tags and undeclared fields are refused at the boundary.
    assert!(
        serde_json::from_str::<SavedViewCommand>(
            r#"{"type":"archive","command_id":"c-3","view_id":"sv-1","expected_version":1}"#
        )
        .is_err()
    );
    assert!(serde_json::from_str::<SavedViewCommand>(
        r#"{"type":"delete","command_id":"c-4","view_id":"sv-1","expected_version":1,"extra":true}"#
    )
    .is_err());
}
