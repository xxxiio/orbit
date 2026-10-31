//! Failure injection: interrupted/failed restores must never damage the active
//! workspace, and a failed activation must roll back from the preserved copy.

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

#[test]
fn activation_failure_rolls_back_to_the_preserved_workspace() {
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
        "rollback",
    )
    .archive;
    let target = build_workspace(
        &directory.path().join("target"),
        &WorkspaceSpec {
            tasks: 1,
            note: "# Old workspace\n",
            attachment: Some(b"old attachment"),
            ..Default::default()
        },
    );
    let old_attachment = target.attachment_hash.clone().unwrap();
    let new_attachment = source.attachment_hash.clone().unwrap();
    let new_prefix = &new_attachment[..2];
    // Inject an activation failure: a regular file occupies the directory the
    // merge needs for the incoming object, after the database and vault have
    // already been renamed into place.
    let blocker = target
        .root
        .join("attachments")
        .join("objects")
        .join(new_prefix);
    fs::create_dir_all(blocker.parent().unwrap()).unwrap();
    fs::write(&blocker, b"not a directory").unwrap();
    let workspace_before = hash_tree(&target.root);

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
    .expect_err("activation must fail when the attachment store cannot be merged");
    let message = error.to_string();
    assert!(
        matches!(error, BackupError::Integrity(_)),
        "unexpected error: {error}"
    );
    assert!(
        message.contains(".orbit-preserved-"),
        "the error must name the preserved copy: {message}"
    );

    // The previous workspace is authoritative again.
    assert_eq!(task_titles(target.layout.database()), vec!["Task 0"]);
    assert_eq!(
        read_note(target.layout.knowledge_vault(), "Work/note.md"),
        "# Old workspace\n"
    );
    assert!(
        target
            .root
            .join("attachments/objects")
            .join(&old_attachment[..2])
            .join(&old_attachment)
            .is_file(),
        "the previous attachment object must survive the failed replacement"
    );
    // The failed restored copy is preserved for diagnosis, not deleted...
    let preserved = preserved_directories(target.layout.parent());
    assert_eq!(preserved.len(), 1, "one preserved directory expected");
    let preserved = &preserved[0];
    assert!(preserved.join("restore-journal.json").is_file());
    assert!(preserved.join("restored-failed/orbit.db").is_file());
    assert!(preserved.join("restored-failed/orbit.knowledge").is_dir());
    // ...and the injected blocker file is still there (nothing was deleted).
    assert_eq!(fs::read(&blocker).unwrap(), b"not a directory");
    let _ = workspace_before;
}

#[test]
fn replacement_with_a_corrupted_archive_changes_nothing() {
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
        "corrupt",
    )
    .archive;
    let target = build_workspace(&directory.path().join("target"), &WorkspaceSpec::default());
    let manifest = read_manifest(&archive);
    assert!(corrupt_entry_content(
        &archive,
        &manifest,
        "database/orbit.db"
    ));
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
    .expect_err("a corrupted archive must be refused");
    assert!(matches!(
        error,
        BackupError::RestoreRefused(_) | BackupError::Integrity(_)
    ));
    assert_eq!(
        hash_tree(&target.root),
        before,
        "a refused replacement must leave the workspace byte-identical"
    );
    assert!(preserved_directories(target.layout.parent()).is_empty());
}

#[cfg(unix)]
#[test]
fn replacement_that_cannot_create_staging_leaves_the_workspace_untouched() {
    use std::os::unix::fs::PermissionsExt;

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
        "read-only-parent",
    )
    .archive;
    let target = build_workspace(&directory.path().join("target"), &WorkspaceSpec::default());
    let before = hash_tree(&target.root);
    let parent = target.layout.parent().to_path_buf();
    let original_mode = fs::metadata(&parent).unwrap().permissions().mode();
    fs::set_permissions(&parent, fs::Permissions::from_mode(0o500)).unwrap();

    let mut progress = no_progress();
    let result = RestoreService::restore(
        &archive,
        replace_target(&target),
        &RestoreOptions {
            confirmation: Some("REPLACE".to_owned()),
            ..RestoreOptions::default()
        },
        &mut progress,
    );
    fs::set_permissions(&parent, fs::Permissions::from_mode(original_mode)).unwrap();
    assert!(
        result.is_err(),
        "staging cannot be created in a read-only workspace parent"
    );
    assert_eq!(
        hash_tree(&target.root),
        before,
        "a restore that cannot stage must not change the workspace"
    );
    assert!(preserved_directories(&parent).is_empty());
}

#[test]
fn isolated_restore_from_a_corrupted_archive_creates_no_destination() {
    let directory = tempfile::tempdir().unwrap();
    let source = build_workspace(&directory.path().join("source"), &WorkspaceSpec::default());
    let archive = create_archive(
        &source,
        &directory.path().join("backups"),
        BackupSourceMode::Local,
        "corrupt-isolated",
    )
    .archive;
    let manifest = read_manifest(&archive);
    assert!(corrupt_entry_content(
        &archive,
        &manifest,
        "knowledge/Work/note.md"
    ));
    let destination = directory.path().join("restore");
    let mut progress = no_progress();
    let error = RestoreService::restore(
        &archive,
        RestoreTarget::Isolated {
            destination_dir: destination.clone(),
        },
        &RestoreOptions::default(),
        &mut progress,
    )
    .expect_err("corrupted archive must not be restored");
    assert!(matches!(
        error,
        BackupError::RestoreRefused(_) | BackupError::Integrity(_)
    ));
    assert!(
        !destination.exists(),
        "a refused isolated restore must not leave a half-created destination"
    );
}
