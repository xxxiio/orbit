//! End-to-end creation tests against real SQLite/vault/attachment workspaces.

mod common;

use std::fs;
use std::time::Duration;

use common::*;
use orbit_backup::{
    BackupConfigSnapshot, BackupError, BackupOptions, BackupProgress, BackupSourceMode,
    VerificationStatus, VerifyOptions, stage_consistent_snapshot, verify_archive,
};
use rusqlite::Connection;

#[test]
fn create_publishes_a_verified_archive_and_round_trips_through_the_readers() {
    let mut workspace = TestWorkspace::new();
    workspace.write_note("Work/one.md", "# One\n");
    workspace.write_note("Work/Sub/two.md", "# Two\n");
    workspace.create_vault_dir("Archive/Empty");
    workspace.open_database();
    workspace.record_attachment(b"attachment-content");

    let destination = tempfile::tempdir().unwrap();
    let outcome = workspace.create_backup(destination.path(), BackupOptions::default());
    assert!(
        outcome.verification.ok,
        "problems: {:?}",
        outcome.verification.problems
    );
    assert_eq!(outcome.verification.checks.len(), 8);
    assert_eq!(outcome.manifest.schema_version, workspace.schema_version());
    assert_eq!(outcome.manifest.counts.knowledge_notes, 2);
    assert_eq!(outcome.manifest.counts.knowledge_folders, 4);
    assert_eq!(outcome.manifest.counts.attachment_objects, 1);
    assert_eq!(outcome.manifest.counts.attachment_associations, 1);
    assert_eq!(outcome.manifest.counts.tables.get("tasks"), Some(&1));

    let archive_bytes = fs::metadata(&outcome.archive).unwrap().len();
    assert_eq!(
        outcome.manifest.totals.archive_bytes, archive_bytes,
        "the manifest must declare the exact archive size"
    );
    let sidecar = outcome.sidecar.as_ref().expect("sidecar written");
    let sidecar_json: serde_json::Value =
        serde_json::from_slice(&fs::read(sidecar).unwrap()).unwrap();
    assert_eq!(sidecar_json["ok"], true);
    assert_eq!(
        sidecar_json["verified_files"].as_u64(),
        Some(outcome.manifest.inventory.len() as u64)
    );

    // The archive re-verifies from disk, deeply and structurally.
    let report = verify_archive(&outcome.archive, &VerifyOptions::default()).unwrap();
    assert!(report.ok, "deep problems: {:?}", report.problems);
    assert_eq!(
        report.verified_files.len(),
        outcome.manifest.inventory.len()
    );
    assert_eq!(
        report
            .checks
            .iter()
            .map(|check| check.name.as_str())
            .collect::<Vec<_>>(),
        vec![
            "structure",
            "manifest",
            "paths",
            "file_hashes",
            "sqlite_integrity",
            "schema_compatibility",
            "attachment_presence",
            "vault_consistency",
        ]
    );
    let structural = verify_archive(&outcome.archive, &VerifyOptions { deep: false }).unwrap();
    assert!(
        structural.ok,
        "structural problems: {:?}",
        structural.problems
    );

    // Inspection reads only the manifest and attaches the sidecar.
    let inspection = orbit_backup::inspect_archive(&outcome.archive).unwrap();
    assert_eq!(inspection.manifest, outcome.manifest);
    assert_eq!(inspection.archive_bytes, archive_bytes);
    assert_eq!(inspection.inventory_files, outcome.manifest.inventory.len());
    assert!(inspection.creation_verification.unwrap().ok);

    // Empty vault directories are recorded and the archive never leaks an
    // absolute source path.
    assert!(
        outcome
            .manifest
            .directories
            .contains(&"knowledge/Archive/Empty".to_owned())
    );
    let manifest_json = String::from_utf8(outcome.manifest.to_json().unwrap()).unwrap();
    assert!(!manifest_json.contains(workspace.root().to_str().unwrap()));

    // The archive contains the manifest first and the actual note content.
    let entries = read_archive_entries(&outcome.archive);
    assert_eq!(entries[0].0, orbit_backup::MANIFEST_ENTRY);
    assert!(entries.iter().any(|(path, _)| path == "database/orbit.db"));
    let note = entries
        .iter()
        .find(|(path, _)| path == "knowledge/Work/one.md")
        .expect("note archived");
    assert_eq!(note.1, b"# One\n");
    assert!(
        entries
            .iter()
            .any(|(path, _)| path.starts_with("attachments/objects/"))
    );

    let listing = orbit_backup::list_backups(destination.path(), None).unwrap();
    assert_eq!(listing.backups.len(), 1);
    assert_eq!(
        listing.backups[0].verification,
        VerificationStatus::Verified
    );
    assert_eq!(
        listing.backups[0].file_name,
        outcome.archive.file_name().unwrap().to_string_lossy()
    );
    assert_eq!(listing.backups[0].size_bytes, archive_bytes);
}

#[test]
fn wal_mode_source_database_is_snapshotted_consistently() {
    let mut workspace = TestWorkspace::new();
    workspace.write_note("Notes/wal.md", "# WAL\n");
    let connection = workspace.open_database();
    let journal_mode: String = connection
        .query_row("PRAGMA journal_mode", [], |row| row.get(0))
        .unwrap();
    assert_eq!(journal_mode, "wal");
    // A committed row that is still only in the WAL file must be in the backup.
    connection
        .execute(
            "INSERT INTO tasks(id, title) VALUES ('wal-task', 'Only in WAL')",
            [],
        )
        .unwrap();

    let destination = tempfile::tempdir().unwrap();
    let outcome = workspace.create_backup(destination.path(), BackupOptions::default());
    assert!(
        outcome.verification.ok,
        "{:?}",
        outcome.verification.problems
    );
    assert_eq!(outcome.manifest.counts.tables.get("tasks"), Some(&2));
    let entries = read_archive_entries(&outcome.archive);
    assert!(
        entries
            .iter()
            .all(|(path, _)| !path.ends_with("-wal") && !path.ends_with("-shm"))
    );

    // The snapshot database is a standalone SQLite file: open it read-only.
    let report = verify_archive(&outcome.archive, &VerifyOptions::default()).unwrap();
    assert!(report.ok, "{:?}", report.problems);
}

#[test]
fn the_fence_blocks_a_concurrent_writer_during_staging() {
    let mut workspace = TestWorkspace::new();
    workspace.write_note("one.md", "# One");
    workspace.open_database();
    let database = workspace.database.clone();
    let staging_parent = tempfile::tempdir().unwrap();

    let mut blocked: Option<Result<usize, rusqlite::Error>> = None;
    let mut progress = |event: &BackupProgress| {
        if matches!(event, BackupProgress::DatabaseSnapshotted { .. }) {
            let contender = Connection::open(&database).unwrap();
            contender.busy_timeout(Duration::from_millis(10)).unwrap();
            blocked = Some(contender.execute(
                "INSERT INTO tasks(id, title) VALUES ('blocked', 'Blocked')",
                [],
            ));
        }
    };
    let stage = stage_consistent_snapshot(
        &workspace.layout(),
        Duration::from_millis(300),
        staging_parent.path(),
        &mut progress,
    )
    .unwrap();
    assert!(
        blocked.expect("staging reported progress").is_err(),
        "another writer must be fenced out while staging"
    );
    drop(stage);
    let contender = Connection::open(&database).unwrap();
    contender
        .execute(
            "INSERT INTO tasks(id, title) VALUES ('allowed', 'Allowed')",
            [],
        )
        .unwrap();
}

#[test]
fn a_held_write_lock_fails_create_with_workspace_busy() {
    let mut workspace = TestWorkspace::new();
    workspace.write_note("one.md", "# One");
    workspace.open_database();
    {
        let holder = workspace.connection();
        holder.execute_batch("BEGIN IMMEDIATE").unwrap();
    }
    let destination = tempfile::tempdir().unwrap();
    let options = BackupOptions {
        fence_timeout: Duration::from_millis(50),
        ..BackupOptions::default()
    };
    let error = workspace
        .try_create_backup(destination.path(), options)
        .unwrap_err();
    assert!(matches!(error, BackupError::WorkspaceBusy(_)), "{error}");
    workspace.connection().execute_batch("ROLLBACK").unwrap();
}

#[test]
fn a_missing_attachment_object_fails_create_and_leaves_no_archive() {
    let mut workspace = TestWorkspace::new();
    workspace.write_note("one.md", "# One");
    workspace.open_database();
    workspace.record_attachment(b"gone");
    let hash = sha256_hex(b"gone");
    fs::remove_file(workspace.object_path(&hash)).unwrap();

    let destination = tempfile::tempdir().unwrap();
    let error = workspace
        .try_create_backup(destination.path(), BackupOptions::default())
        .unwrap_err();
    assert!(
        matches!(error, BackupError::MissingAttachmentObjects(_)),
        "{error}"
    );
    assert!(error.to_string().contains(&hash), "{error}");
    assert!(files_with_extension(destination.path(), "orbitbak").is_empty());
    assert!(
        fs::read_dir(destination.path()).unwrap().next().is_none(),
        "temporary or staging files must be cleaned up after a failure"
    );
}

#[test]
fn a_non_directory_destination_is_rejected_without_writing_anything() {
    let mut workspace = TestWorkspace::new();
    workspace.write_note("one.md", "# One");
    workspace.open_database();
    let directory = tempfile::tempdir().unwrap();
    let destination = directory.path().join("not-a-directory");
    fs::write(&destination, b"keep me").unwrap();

    let error = workspace
        .try_create_backup(&destination, BackupOptions::default())
        .unwrap_err();
    assert!(matches!(error, BackupError::InvalidArgument(_)), "{error}");
    assert_eq!(fs::read(&destination).unwrap(), b"keep me");
}

#[test]
fn label_names_the_archive_and_configuration_is_recorded() {
    let mut workspace = TestWorkspace::new();
    workspace.write_note("one.md", "# One");
    workspace.open_database();
    let options = BackupOptions {
        label: Some("Before agent testing".to_owned()),
        config: Some(BackupConfigSnapshot {
            config_version: 1,
            authority_mode: "local".to_owned(),
            default_agent_provider: Some("deepseek".to_owned()),
            connected_server: None,
        }),
        ..BackupOptions::default()
    };
    let destination = tempfile::tempdir().unwrap();
    let outcome = workspace.create_backup(destination.path(), options);
    assert!(
        outcome.verification.ok,
        "{:?}",
        outcome.verification.problems
    );
    let file_name = outcome.archive.file_name().unwrap().to_string_lossy();
    assert!(
        file_name.ends_with("-before-agent-testing.orbitbak"),
        "{file_name}"
    );
    assert_eq!(
        outcome.manifest.label.as_deref(),
        Some("Before agent testing")
    );
    assert!(
        outcome
            .manifest
            .inventory
            .iter()
            .any(|entry| entry.path == orbit_backup::CONFIG_ENTRY)
    );
    assert_eq!(
        outcome.manifest.totals.config_bytes,
        outcome
            .manifest
            .inventory
            .iter()
            .find(|entry| entry.path == orbit_backup::CONFIG_ENTRY)
            .unwrap()
            .size_bytes
    );
}

#[test]
fn configuration_is_omitted_when_disabled() {
    let mut workspace = TestWorkspace::new();
    workspace.write_note("one.md", "# One");
    workspace.open_database();
    let options = BackupOptions {
        include_config: false,
        ..BackupOptions::default()
    };
    let destination = tempfile::tempdir().unwrap();
    let outcome = workspace.create_backup(destination.path(), options);
    assert!(
        outcome.verification.ok,
        "{:?}",
        outcome.verification.problems
    );
    assert!(outcome.manifest.config.is_none());
    assert!(
        !outcome
            .manifest
            .inventory
            .iter()
            .any(|entry| entry.path == orbit_backup::CONFIG_ENTRY)
    );
}

#[test]
fn replica_snapshots_are_refused_with_actionable_guidance() {
    let mut workspace = TestWorkspace::new();
    workspace.write_note("one.md", "# One");
    workspace.open_database();
    let database_before = orbit_backup::util::hash_file(workspace.layout().database()).unwrap();
    let options = BackupOptions {
        source_mode: BackupSourceMode::Replica,
        ..BackupOptions::default()
    };
    let destination = tempfile::tempdir().unwrap();
    let error = workspace
        .try_create_backup(destination.path(), options)
        .expect_err("client replica snapshots must fail closed");
    assert!(
        error
            .to_string()
            .contains("not supported as a recovery point"),
        "{error}"
    );
    assert!(
        error.to_string().contains("server workspace"),
        "the message must point at the supported recovery path: {error}"
    );
    assert!(
        std::fs::read_dir(destination.path())
            .unwrap()
            .next()
            .is_none(),
        "a refused backup must not publish or stage anything in the destination"
    );
    assert_eq!(
        orbit_backup::util::hash_file(workspace.layout().database()).unwrap(),
        database_before,
        "a refused backup must not mutate the workspace"
    );
}

#[test]
fn server_backups_declare_excluded_client_pending_data() {
    let mut workspace = TestWorkspace::new();
    workspace.write_note("one.md", "# One");
    workspace.open_database();
    let options = BackupOptions {
        source_mode: BackupSourceMode::Server,
        ..BackupOptions::default()
    };
    let destination = tempfile::tempdir().unwrap();
    let outcome = workspace.create_backup(destination.path(), options);
    assert!(
        outcome.verification.ok,
        "{:?}",
        outcome.verification.problems
    );
    assert!(outcome.manifest.source.excluded_client_pending_data);
    assert_eq!(outcome.manifest.source.mode, BackupSourceMode::Server);
    assert!(
        outcome
            .manifest
            .exclusions
            .iter()
            .any(|exclusion| exclusion.category == "client_pending_data"),
        "a server backup must say that disconnected-client pending data is outside it"
    );
    assert!(
        outcome
            .manifest
            .exclusions
            .iter()
            .any(|exclusion| exclusion.category == "external_agent_sessions"),
        "external boundaries must always be declared"
    );
    assert!(outcome.manifest.complete);
}

#[test]
fn publishing_never_overwrites_an_existing_archive() {
    let directory = tempfile::tempdir().unwrap();
    let existing = directory
        .path()
        .join("orbit-20260101T000000Z-backup.orbitbak");
    fs::write(&existing, b"existing archive").unwrap();
    let chosen = orbit_backup::create::unique_archive_path(
        directory.path(),
        "orbit-20260101T000000Z-backup.orbitbak",
    );
    assert_eq!(
        chosen.file_name().unwrap().to_string_lossy(),
        "orbit-20260101T000000Z-backup-2.orbitbak"
    );

    // Two real backups in the same directory always coexist and verify.
    let mut workspace = TestWorkspace::new();
    workspace.write_note("one.md", "# One");
    workspace.open_database();
    let first = workspace.create_backup(directory.path(), BackupOptions::default());
    let second = workspace.create_backup(directory.path(), BackupOptions::default());
    assert_ne!(first.archive, second.archive);
    for archive in [&first.archive, &second.archive] {
        assert!(archive.is_file());
        let report = verify_archive(archive, &VerifyOptions::default()).unwrap();
        assert!(report.ok, "{:?}", report.problems);
    }
}

#[cfg(unix)]
#[test]
fn attachment_objects_are_hardlinked_into_staging() {
    use std::os::unix::fs::MetadataExt;

    let mut workspace = TestWorkspace::new();
    workspace.write_note("one.md", "# One");
    workspace.open_database();
    workspace.record_attachment(b"hardlinked object");
    let hash = sha256_hex(b"hardlinked object");
    let source = workspace.object_path(&hash);
    let staging_parent = tempfile::tempdir().unwrap();
    let same_device =
        fs::metadata(&source).unwrap().dev() == fs::metadata(staging_parent.path()).unwrap().dev();
    let stage = stage_consistent_snapshot(
        &workspace.layout(),
        Duration::from_millis(500),
        staging_parent.path(),
        &mut orbit_backup::NoProgress,
    )
    .unwrap();
    let staged = stage
        .attachment_objects
        .iter()
        .find(|object| object.object_hash == hash)
        .unwrap();
    assert_eq!(fs::read(&staged.path).unwrap(), b"hardlinked object");
    if same_device {
        let staged_metadata = fs::metadata(&staged.path).unwrap();
        let source_metadata = fs::metadata(&source).unwrap();
        assert_eq!(
            staged_metadata.ino(),
            source_metadata.ino(),
            "same-device staging must be a hardlink, not a copy"
        );
    }
}

/// Copy the attachment into staging when the destination is on another device.
/// The test is skipped (not silently weakened) when no second filesystem exists.
#[cfg(unix)]
#[test]
fn attachment_objects_fall_back_to_copy_across_devices() {
    use std::os::unix::fs::MetadataExt;

    let mut workspace = TestWorkspace::new();
    workspace.write_note("one.md", "# One");
    workspace.open_database();
    workspace.record_attachment(b"cross-device object");
    let hash = sha256_hex(b"cross-device object");
    let source = workspace.object_path(&hash);
    let workspace_device = fs::metadata(&source).unwrap().dev();
    let Some(destination) = other_device_directory(workspace_device) else {
        eprintln!("skipping cross-device test: no second filesystem is available");
        return;
    };
    // Prove the hardlink shortcut cannot work here, so the copy path runs.
    assert!(fs::hard_link(&source, destination.path().join("probe")).is_err());

    let outcome = workspace.create_backup(destination.path(), BackupOptions::default());
    assert!(
        outcome.verification.ok,
        "{:?}",
        outcome.verification.problems
    );
    let entries = read_archive_entries(&outcome.archive);
    let (_, attachment) = entries
        .iter()
        .find(|(path, _)| path.starts_with(orbit_backup::ATTACHMENT_PREFIX))
        .expect("attachment archived");
    assert_eq!(attachment, b"cross-device object");
}

#[cfg(unix)]
fn other_device_directory(workspace_device: u64) -> Option<tempfile::TempDir> {
    use std::os::unix::fs::MetadataExt;
    for candidate in ["/dev/shm", "/run/shm", "/tmp"] {
        let base = std::path::Path::new(candidate);
        if !base.is_dir() {
            continue;
        }
        let Ok(directory) = tempfile::Builder::new()
            .prefix("orbit-xdev")
            .tempdir_in(base)
        else {
            continue;
        };
        if fs::metadata(directory.path()).unwrap().dev() != workspace_device {
            return Some(directory);
        }
    }
    None
}

#[cfg(unix)]
#[test]
fn a_failed_staging_step_leaves_no_partial_archive_behind() {
    use std::os::unix::fs::PermissionsExt;

    let mut workspace = TestWorkspace::new();
    workspace.write_note("one.md", "# One");
    workspace.open_database();
    let destination = tempfile::tempdir().unwrap();
    // A read-only destination makes staging creation fail before any archive
    // bytes exist; the cleanup path must leave the directory untouched.
    fs::set_permissions(destination.path(), fs::Permissions::from_mode(0o555)).unwrap();
    let error = workspace
        .try_create_backup(destination.path(), BackupOptions::default())
        .unwrap_err();
    fs::set_permissions(destination.path(), fs::Permissions::from_mode(0o755)).unwrap();
    assert!(matches!(error, BackupError::Io { .. }), "{error}");
    assert!(
        fs::read_dir(destination.path()).unwrap().next().is_none(),
        "no staging directory or partial archive may survive a failed create"
    );
}
