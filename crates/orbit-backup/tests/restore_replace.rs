//! Replacement restore: confirmation, preservation, recovery journal, merge
//! semantics and repeated maintenance restores.

#[path = "restore_support.rs"]
mod support;

use std::fs;
use std::path::{Path, PathBuf};

use orbit_backup::manifest::BackupSourceMode;
use orbit_backup::{BackupError, RestoreOptions, RestoreService, RestoreTarget};
use support::*;

fn replace_target(fixture: &Fixture) -> RestoreTarget {
    RestoreTarget::Replace {
        layout: fixture.layout.clone(),
    }
}

fn no_progress() -> orbit_backup::NoProgress {
    orbit_backup::NoProgress
}

fn preserved_directories(parent: &Path) -> Vec<PathBuf> {
    let mut directories: Vec<PathBuf> = fs::read_dir(parent)
        .expect("read workspace parent")
        .map(|entry| entry.expect("directory entry").path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with(".orbit-preserved-"))
        })
        .collect();
    directories.sort();
    directories
}

fn build_target(directory: &Path) -> Fixture {
    build_workspace(
        &directory.join("target"),
        &WorkspaceSpec {
            tasks: 1,
            note: "# Old workspace\n",
            attachment: Some(b"old attachment"),
            ..Default::default()
        },
    )
}

#[test]
fn replacement_requires_confirmation_and_changes_nothing() {
    let directory = tempfile::tempdir().unwrap();
    let source = build_workspace(
        &directory.path().join("source"),
        &WorkspaceSpec {
            tasks: 2,
            note: "# New workspace\n",
            ..Default::default()
        },
    );
    let archive = create_archive(
        &source,
        &directory.path().join("backups"),
        BackupSourceMode::Local,
        "replace",
    )
    .archive;
    let target = build_target(directory.path());
    let before = hash_tree(&target.root);
    for confirmation in [None, Some("yes please".to_owned())] {
        let mut progress = no_progress();
        let error = RestoreService::restore(
            &archive,
            replace_target(&target),
            &RestoreOptions {
                confirmation,
                ..RestoreOptions::default()
            },
            &mut progress,
        )
        .expect_err("replacement without the exact confirmation must be refused");
        assert!(matches!(error, BackupError::RestoreRefused(_)));
        assert_eq!(
            hash_tree(&target.root),
            before,
            "a refused replacement must not change the workspace"
        );
    }
}

#[test]
fn replacement_preserves_previous_workspace_merges_attachments_and_writes_journal() {
    let directory = tempfile::tempdir().unwrap();
    let source = build_workspace(
        &directory.path().join("source"),
        &WorkspaceSpec {
            tasks: 2,
            note: "# New workspace\n",
            attachment: Some(b"new attachment"),
            ..Default::default()
        },
    );
    let archive = create_archive(
        &source,
        &directory.path().join("backups"),
        BackupSourceMode::Local,
        "replace",
    )
    .archive;
    let target = build_target(directory.path());
    let old_attachment = target
        .attachment_hash
        .clone()
        .expect("target attachment hash");
    let new_attachment = source
        .attachment_hash
        .clone()
        .expect("source attachment hash");
    assert_ne!(old_attachment, new_attachment);
    let confirmation = target.layout.database().display().to_string();

    let preflight = RestoreService::preflight(
        &archive,
        &replace_target(&target),
        &RestoreOptions {
            confirmation: Some(confirmation.clone()),
            ..RestoreOptions::default()
        },
    )
    .expect("preflight");
    assert!(preflight.admissible, "blockers: {:?}", preflight.blockers);
    assert!(preflight.plan.downtime_required);
    assert!(!preflight.plan.replaces.is_empty());
    assert!(!preflight.plan.preserves.is_empty());

    let mut progress = no_progress();
    let outcome = RestoreService::restore(
        &archive,
        replace_target(&target),
        &RestoreOptions {
            confirmation: Some(confirmation),
            ..RestoreOptions::default()
        },
        &mut progress,
    )
    .expect("replacement restore");

    // The restored workspace is the archived one and passed verification.
    assert!(outcome.activated);
    assert!(outcome.verification.ok);
    assert_eq!(
        task_titles(outcome.target.database()),
        vec!["Task 0", "Task 1"]
    );
    assert_eq!(
        read_note(outcome.target.knowledge_vault(), "Work/note.md"),
        "# New workspace\n"
    );
    assert_eq!(outcome.counts.tables.get("tasks"), Some(&2));

    // Attachment objects are content-addressed and merged, never deleted.
    let store = outcome.target.attachment_store();
    for hash in [&old_attachment, &new_attachment] {
        assert!(
            store.join(&hash[..2]).join(hash).is_file(),
            "attachment object {hash} must exist after the merge"
        );
    }

    // The previous database and vault were preserved by rename, with a journal.
    let preserved = outcome.preserved.expect("preserved directory");
    assert!(preserved.is_dir());
    let preserved_database = preserved.join("orbit.db");
    assert!(preserved_database.is_file());
    assert_eq!(count_rows(&preserved_database, "tasks"), 1);
    assert_eq!(
        read_note(&preserved.join("orbit.knowledge"), "Work/note.md"),
        "# Old workspace\n"
    );
    let journal_path = outcome.journal.expect("journal path");
    assert_eq!(journal_path, preserved.join("restore-journal.json"));
    let journal: serde_json::Value =
        serde_json::from_slice(&fs::read(&journal_path).unwrap()).expect("journal JSON");
    assert_eq!(journal["version"], 1);
    assert_eq!(journal["source_mode"], "local");
    let manifest_hash = journal["archive_manifest_sha256"]
        .as_str()
        .expect("manifest hash");
    assert_eq!(manifest_hash.len(), 64);
    assert!(
        journal["rollback_steps"]
            .as_array()
            .is_some_and(|steps| !steps.is_empty())
    );
    assert_eq!(
        journal["preserved"]["database"].as_str(),
        Some(preserved_database.display().to_string().as_str())
    );

    // Old workspace files are no longer active: the vault was moved, not copied.
    assert!(
        !target.root.join("orbit.knowledge").is_dir()
            || read_note(&target.root.join("orbit.knowledge"), "Work/note.md")
                == "# New workspace\n"
    );
}

#[test]
fn repeated_replacement_never_deletes_earlier_preserved_copies() {
    let directory = tempfile::tempdir().unwrap();
    let first = build_workspace(
        &directory.path().join("first"),
        &WorkspaceSpec {
            tasks: 2,
            note: "# First archive\n",
            ..Default::default()
        },
    );
    let second = build_workspace(
        &directory.path().join("second"),
        &WorkspaceSpec {
            tasks: 3,
            note: "# Second archive\n",
            ..Default::default()
        },
    );
    let first_archive = create_archive(
        &first,
        &directory.path().join("backups"),
        BackupSourceMode::Local,
        "first",
    )
    .archive;
    let second_archive = create_archive(
        &second,
        &directory.path().join("backups"),
        BackupSourceMode::Local,
        "second",
    )
    .archive;
    let target = build_target(directory.path());
    let parent = target.layout.parent().to_path_buf();

    for archive in [&first_archive, &second_archive] {
        let mut progress = no_progress();
        RestoreService::restore(
            archive,
            replace_target(&target),
            &RestoreOptions {
                confirmation: Some("REPLACE".to_owned()),
                ..RestoreOptions::default()
            },
            &mut progress,
        )
        .expect("replacement restore");
    }
    assert_eq!(
        preserved_directories(&parent).len(),
        2,
        "both previous workspaces must remain preserved"
    );
    assert_eq!(
        task_titles(target.layout.database()),
        vec!["Task 0", "Task 1", "Task 2"]
    );
    assert_eq!(
        read_note(target.layout.knowledge_vault(), "Work/note.md"),
        "# Second archive\n"
    );
}

#[test]
fn replacement_is_refused_while_the_workspace_is_actively_used() {
    let directory = tempfile::tempdir().unwrap();
    let source = build_workspace(
        &directory.path().join("source"),
        &WorkspaceSpec {
            tasks: 2,
            ..Default::default()
        },
    );
    let archive = create_archive(
        &source,
        &directory.path().join("backups"),
        BackupSourceMode::Local,
        "active",
    )
    .archive;
    let target = build_target(directory.path());
    let fence = hold_write_fence(target.layout.database());
    let before = hash_tree(&target.root);
    let mut progress = no_progress();
    let error = RestoreService::restore(
        &archive,
        replace_target(&target),
        &RestoreOptions {
            confirmation: Some("REPLACE".to_owned()),
            ..RestoreOptions::default()
        },
        &mut progress,
    )
    .expect_err("an active workspace must be refused");
    assert!(matches!(error, BackupError::RestoreRefused(_)));
    assert_eq!(hash_tree(&target.root), before);
    fence.release().unwrap();
}

#[test]
fn replacement_is_refused_when_pending_client_commands_would_be_lost() {
    let directory = tempfile::tempdir().unwrap();
    let source = build_workspace(&directory.path().join("source"), &WorkspaceSpec::default());
    let archive = create_archive(
        &source,
        &directory.path().join("backups"),
        BackupSourceMode::Local,
        "pending",
    )
    .archive;
    let target = build_workspace(
        &directory.path().join("target"),
        &WorkspaceSpec {
            tasks: 2,
            outbox_rows: 3,
            ..Default::default()
        },
    );
    let before = hash_tree(&target.root);
    let mut progress = no_progress();
    let error = RestoreService::restore(
        &archive,
        replace_target(&target),
        &RestoreOptions {
            confirmation: Some("REPLACE".to_owned()),
            ..RestoreOptions::default()
        },
        &mut progress,
    )
    .expect_err("pending client commands must block replacement");
    assert!(matches!(error, BackupError::RestoreRefused(_)));
    assert_eq!(hash_tree(&target.root), before);
}

#[test]
fn exported_outbox_history_does_not_block_replacement() {
    let directory = tempfile::tempdir().unwrap();
    let source = build_workspace(
        &directory.path().join("source"),
        &WorkspaceSpec {
            tasks: 2,
            ..Default::default()
        },
    );
    let archive = create_archive(
        &source,
        &directory.path().join("backups"),
        BackupSourceMode::Local,
        "exported-outbox",
    )
    .archive;
    let target = build_workspace(
        &directory.path().join("target"),
        &WorkspaceSpec {
            tasks: 1,
            exported_outbox_rows: 3,
            ..Default::default()
        },
    );
    let preflight = RestoreService::preflight(
        &archive,
        &replace_target(&target),
        &RestoreOptions::default(),
    )
    .expect("preflight");
    assert!(
        preflight.admissible,
        "exported outbox history is not pending work: {:?}",
        preflight.blockers
    );
    assert_eq!(preflight.plan.target.pending_outbox_rows, 0);
    let mut progress = no_progress();
    let outcome = RestoreService::restore(
        &archive,
        replace_target(&target),
        &RestoreOptions {
            confirmation: Some("REPLACE".to_owned()),
            ..RestoreOptions::default()
        },
        &mut progress,
    )
    .expect("replacement with only exported outbox history");
    assert_eq!(
        task_titles(outcome.target.database()),
        vec!["Task 0", "Task 1"]
    );
}

#[test]
fn replica_archive_cannot_replace_an_authoritative_workspace() {
    let directory = tempfile::tempdir().unwrap();
    let replica = build_workspace(&directory.path().join("replica"), &WorkspaceSpec::default());
    // Creation refuses replica snapshots in this build; declare replica source
    // mode on a format-valid archive to prove restore still refuses it.
    let archive = create_archive(
        &replica,
        &directory.path().join("backups"),
        BackupSourceMode::Local,
        "replica-declared",
    )
    .archive;
    rewrite_manifest(&archive, |manifest| {
        manifest.source.mode = BackupSourceMode::Replica;
        manifest.source.excluded_client_pending_data = true;
    });
    let target = build_target(directory.path());
    let before = hash_tree(&target.root);
    let mut progress = no_progress();
    let error = RestoreService::restore(
        &archive,
        replace_target(&target),
        &RestoreOptions {
            confirmation: Some("REPLACE".to_owned()),
            ..RestoreOptions::default()
        },
        &mut progress,
    )
    .expect_err("a replica snapshot must not replace an authority workspace");
    assert!(matches!(error, BackupError::RestoreRefused(_)));
    assert_eq!(hash_tree(&target.root), before);
}
