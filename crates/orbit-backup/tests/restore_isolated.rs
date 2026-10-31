//! Isolated restore: extract into a new directory, verify, publish, and prove
//! the active workspace and the archive were never touched.

#[path = "restore_support.rs"]
mod support;

use std::fs;
use std::path::Path;

use orbit_backup::manifest::BackupSourceMode;
use orbit_backup::{BackupError, RestoreOptions, RestoreService, RestoreTarget};
use support::*;

fn isolated_target(destination: &Path) -> RestoreTarget {
    RestoreTarget::Isolated {
        destination_dir: destination.to_path_buf(),
    }
}

fn no_progress() -> orbit_backup::NoProgress {
    orbit_backup::NoProgress
}

fn build_source(directory: &Path) -> (Fixture, std::path::PathBuf) {
    let source = build_workspace(
        &directory.join("source"),
        &WorkspaceSpec {
            tasks: 3,
            note: "# Restored note\n",
            attachment: Some(b"attachment payload"),
            ..Default::default()
        },
    );
    let outcome = create_archive(
        &source,
        &directory.join("backups"),
        BackupSourceMode::Local,
        "isolated",
    );
    (source, outcome.archive)
}

#[test]
fn isolated_restore_round_trip_preserves_rows_vault_attachments_and_config() {
    let directory = tempfile::tempdir().unwrap();
    let (source, archive) = build_source(directory.path());
    let source_tree_before = hash_tree(&source.root);
    let archive_hash_before = hash_file(&archive);
    let destination = directory.path().join("restored");

    let preflight = RestoreService::preflight(
        &archive,
        &isolated_target(&destination),
        &RestoreOptions::default(),
    )
    .expect("preflight");
    assert!(preflight.admissible, "blockers: {:?}", preflight.blockers);
    let mut progress = no_progress();
    let outcome = RestoreService::restore(
        &archive,
        isolated_target(&destination),
        &RestoreOptions::default(),
        &mut progress,
    )
    .expect("isolated restore");

    assert!(outcome.activated);
    assert!(outcome.preserved.is_none());
    assert!(outcome.journal.is_none());
    assert!(outcome.verification.ok);
    assert_eq!(
        outcome.target.database(),
        destination.join("orbit.db").as_path()
    );
    assert_eq!(
        outcome.target.knowledge_vault(),
        destination.join("orbit.knowledge").as_path()
    );
    assert_eq!(
        outcome.target.attachment_store(),
        destination.join("attachments").join("objects").as_path()
    );

    // Database rows are openable and match the source snapshot.
    assert_eq!(
        task_titles(outcome.target.database()),
        vec!["Task 0", "Task 1", "Task 2"]
    );
    assert_eq!(
        count_rows(outcome.target.database(), "attachment_objects"),
        1
    );
    assert_eq!(outcome.counts.tables.get("tasks"), Some(&3));
    assert_eq!(outcome.counts.knowledge_notes, 1);
    assert_eq!(outcome.counts.attachment_objects, 1);

    // Vault, empty directory and attachment object are materialized.
    assert_eq!(
        read_note(outcome.target.knowledge_vault(), "Work/note.md"),
        "# Restored note\n"
    );
    assert!(outcome.target.knowledge_vault().join("Archive").is_dir());
    let hash = source
        .attachment_hash
        .as_deref()
        .expect("source attachment");
    let object = outcome
        .target
        .attachment_store()
        .join(&hash[..2])
        .join(hash);
    assert_eq!(fs::read(&object).unwrap(), b"attachment payload");

    // Normalized non-secret configuration was restored with the workspace.
    let config = fs::read_to_string(destination.join("config/config.json"))
        .expect("config snapshot restored");
    let config: orbit_backup::BackupConfigSnapshot =
        serde_json::from_str(&config).expect("config snapshot parses");
    assert_eq!(config.config_version, 1);
    assert_eq!(config.authority_mode, "local");

    // Neither the archive nor the source workspace changed.
    assert_eq!(hash_file(&archive), archive_hash_before);
    assert_eq!(hash_tree(&source.root), source_tree_before);
}

#[test]
fn isolated_restore_works_from_fast_preflight_only() {
    let directory = tempfile::tempdir().unwrap();
    let (source, archive) = build_source(directory.path());
    let destination = directory.path().join("restored");
    let options = RestoreOptions {
        deep_verification: false,
        ..RestoreOptions::default()
    };
    let preflight =
        RestoreService::preflight(&archive, &isolated_target(&destination), &options).unwrap();
    assert!(preflight.admissible, "blockers: {:?}", preflight.blockers);
    assert!(!preflight.verification.checks.iter().any(|check| {
        check.name == "sqlite_integrity" && check.status == orbit_backup::CheckStatus::Pass
    }));
    let mut progress = no_progress();
    let outcome = RestoreService::restore(
        &archive,
        isolated_target(&destination),
        &options,
        &mut progress,
    )
    .expect("restore with fast preflight still verifies extracted content");
    let source_tree_before = hash_tree(&source.root);
    assert_eq!(
        task_titles(outcome.target.database()),
        vec!["Task 0", "Task 1", "Task 2"]
    );
    assert_eq!(hash_tree(&source.root), source_tree_before);
}

#[test]
fn isolated_restore_accepts_an_existing_empty_directory() {
    let directory = tempfile::tempdir().unwrap();
    let (_source, archive) = build_source(directory.path());
    let destination = directory.path().join("restored");
    fs::create_dir_all(&destination).unwrap();
    let mut progress = no_progress();
    let outcome = RestoreService::restore(
        &archive,
        isolated_target(&destination),
        &RestoreOptions::default(),
        &mut progress,
    )
    .expect("restore into empty directory");
    assert!(outcome.target.database().is_file());
}

#[test]
fn isolated_restore_refuses_a_non_empty_destination_and_changes_nothing() {
    let directory = tempfile::tempdir().unwrap();
    let (_source, archive) = build_source(directory.path());
    let destination = directory.path().join("restored");
    fs::create_dir_all(&destination).unwrap();
    fs::write(destination.join("keep.txt"), b"keep me").unwrap();
    let before = hash_tree(&destination);
    let mut progress = no_progress();
    let error = RestoreService::restore(
        &archive,
        isolated_target(&destination),
        &RestoreOptions::default(),
        &mut progress,
    )
    .expect_err("non-empty destination must be refused");
    assert!(
        matches!(
            error,
            BackupError::RestoreRefused(_) | BackupError::Conflict(_)
        ),
        "unexpected error: {error}"
    );
    assert_eq!(hash_tree(&destination), before);
    assert!(!destination.join("orbit.db").exists());
}

#[test]
fn isolated_restore_never_writes_into_the_active_workspace() {
    let directory = tempfile::tempdir().unwrap();
    let (source, archive) = build_source(directory.path());
    let active = build_workspace(
        &directory.path().join("active"),
        &WorkspaceSpec {
            tasks: 5,
            ..Default::default()
        },
    );
    let active_before = hash_tree(&active.root);
    let destination = directory.path().join("recovery-drill");
    let options = RestoreOptions {
        active_database: Some(active.layout.database().to_path_buf()),
        ..RestoreOptions::default()
    };
    let mut progress = no_progress();
    let outcome = RestoreService::restore(
        &archive,
        isolated_target(&destination),
        &options,
        &mut progress,
    )
    .expect("isolated restore beside the active workspace");
    assert_eq!(
        task_titles(outcome.target.database()),
        vec!["Task 0", "Task 1", "Task 2"]
    );
    assert_eq!(
        hash_tree(&active.root),
        active_before,
        "the active workspace must be byte-identical after an isolated restore"
    );
    drop(source);
}

#[test]
fn older_schema_archive_plans_a_documented_upgrade() {
    let directory = tempfile::tempdir().unwrap();
    let older = orbit_backup::supported_schema_version() - 1;
    let source = build_workspace(
        &directory.path().join("source"),
        &WorkspaceSpec {
            tasks: 1,
            schema_version: Some(older),
            ..Default::default()
        },
    );
    let archive = create_archive(
        &source,
        &directory.path().join("backups"),
        BackupSourceMode::Local,
        "older-schema",
    )
    .archive;
    let destination = directory.path().join("restored");
    let preflight = RestoreService::preflight(
        &archive,
        &isolated_target(&destination),
        &RestoreOptions::default(),
    )
    .expect("preflight");
    assert!(
        preflight.admissible,
        "an older schema is restorable through migration: {:?}",
        preflight.blockers
    );
    assert!(preflight.plan.schema_compatible);
    assert!(preflight.plan.schema_upgrade_required);
    assert_eq!(preflight.plan.backup_schema_version, older);
}
