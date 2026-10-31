//! Preflight behaviour: read-only inspection, compatibility and safety blockers.

#[path = "restore_support.rs"]
mod support;

use std::fs;
use std::path::Path;

use orbit_backup::manifest::BackupSourceMode;
use orbit_backup::{BackupError, RestoreOptions, RestoreService, RestoreTarget};
use support::*;

fn replace_target(fixture: &Fixture) -> RestoreTarget {
    RestoreTarget::Replace {
        layout: fixture.layout.clone(),
    }
}

fn isolated_target(destination: &Path) -> RestoreTarget {
    RestoreTarget::Isolated {
        destination_dir: destination.to_path_buf(),
    }
}

fn blocker_codes(preflight: &orbit_backup::RestorePreflight) -> Vec<&str> {
    preflight
        .blockers
        .iter()
        .map(|blocker| blocker.code.as_str())
        .collect()
}

fn source_archive(directory: &Path, spec: &WorkspaceSpec<'_>) -> (Fixture, std::path::PathBuf) {
    let source = build_workspace(&directory.join("source"), spec);
    let outcome = create_archive(
        &source,
        &directory.join("backups"),
        BackupSourceMode::Local,
        "preflight-source",
    );
    (source, outcome.archive)
}

#[test]
fn replace_preflight_mutates_nothing_on_a_delete_journal_workspace() {
    let directory = tempfile::tempdir().unwrap();
    let (_source, archive) = source_archive(
        directory.path(),
        &WorkspaceSpec {
            tasks: 2,
            attachment: Some(b"preflight payload"),
            ..Default::default()
        },
    );
    let target = build_workspace(
        &directory.path().join("target"),
        &WorkspaceSpec {
            tasks: 3,
            note: "# Target\n",
            ..Default::default()
        },
    );
    let before = hash_tree(&target.root);
    let preflight = RestoreService::preflight(
        &archive,
        &replace_target(&target),
        &RestoreOptions::default(),
    )
    .expect("preflight");
    assert!(
        preflight.verification.ok,
        "archive must verify: {:?}",
        preflight.verification.problems
    );
    assert!(preflight.admissible, "blockers: {:?}", preflight.blockers);
    let after = hash_tree(&target.root);
    assert_eq!(
        before, after,
        "preflight must not change a single file in a DELETE-journal target workspace"
    );
}

#[test]
fn replace_preflight_is_side_effect_free_on_a_wal_workspace_including_no_new_sidecars() {
    let directory = tempfile::tempdir().unwrap();
    let (_source, archive) = source_archive(directory.path(), &WorkspaceSpec::default());
    let target = build_workspace(
        &directory.path().join("target"),
        &WorkspaceSpec {
            tasks: 2,
            journal_mode: "wal",
            ..Default::default()
        },
    );
    // A dry-run preflight must not add `-wal`/`-shm` files to a WAL-mode
    // workspace: the recursive file map includes the file list, not just hashes.
    let before = hash_tree(&target.root);
    assert!(!target.root.join("orbit.db-wal").exists());
    assert!(!target.root.join("orbit.db-shm").exists());
    let preflight = RestoreService::preflight(
        &archive,
        &replace_target(&target),
        &RestoreOptions::default(),
    )
    .expect("preflight");
    assert!(preflight.admissible, "blockers: {:?}", preflight.blockers);
    let after = hash_tree(&target.root);
    assert_eq!(
        before, after,
        "WAL-mode preflight/dry-run must not add, remove or change any file"
    );
}

#[test]
fn isolated_preflight_accepts_a_new_empty_or_missing_directory() {
    let directory = tempfile::tempdir().unwrap();
    let (_source, archive) = source_archive(directory.path(), &WorkspaceSpec::default());
    let destination = directory.path().join("restore");
    let preflight = RestoreService::preflight(
        &archive,
        &isolated_target(&destination),
        &RestoreOptions::default(),
    )
    .expect("preflight");
    assert!(preflight.admissible, "blockers: {:?}", preflight.blockers);
    assert!(!preflight.plan.target.exists);
    assert!(!preflight.plan.downtime_required);
    assert!(!preflight.plan.schema_upgrade_required);
    assert!(preflight.plan.replaces.is_empty());
    assert!(preflight.plan.preserves.is_empty());
    assert_eq!(
        preflight.plan.target.database,
        destination.join("orbit.db").display().to_string()
    );

    fs::create_dir_all(&destination).unwrap();
    let empty = RestoreService::preflight(
        &archive,
        &isolated_target(&destination),
        &RestoreOptions::default(),
    )
    .expect("preflight on empty directory");
    assert!(empty.admissible);

    fs::write(destination.join("keep.txt"), b"do not overwrite").unwrap();
    let refused = RestoreService::preflight(
        &archive,
        &isolated_target(&destination),
        &RestoreOptions::default(),
    )
    .expect("preflight on non-empty directory");
    assert!(!refused.admissible);
    assert!(blocker_codes(&refused).contains(&"target_not_empty"));
}

#[test]
fn isolated_preflight_refuses_a_destination_that_contains_the_active_workspace() {
    let directory = tempfile::tempdir().unwrap();
    let (source, archive) = source_archive(directory.path(), &WorkspaceSpec::default());
    let active_workspace =
        build_workspace(&directory.path().join("active"), &WorkspaceSpec::default());
    let options = RestoreOptions {
        active_database: Some(active_workspace.layout.database().to_path_buf()),
        ..RestoreOptions::default()
    };
    let refused = RestoreService::preflight(
        &archive,
        &isolated_target(active_workspace.layout.parent()),
        &options,
    )
    .expect("preflight");
    assert!(!refused.admissible);
    assert!(blocker_codes(&refused).contains(&"active_workspace"));

    // A new subdirectory next to the active workspace is fine: it does not
    // contain any active workspace file.
    let drill = active_workspace.layout.parent().join("recovery-drill");
    let allowed =
        RestoreService::preflight(&archive, &isolated_target(&drill), &options).expect("preflight");
    assert!(allowed.admissible, "blockers: {:?}", allowed.blockers);
    drop(source);
}

#[test]
fn replace_preflight_defers_the_active_workspace_check_to_the_apply_path() {
    let directory = tempfile::tempdir().unwrap();
    let (_source, archive) = source_archive(directory.path(), &WorkspaceSpec::default());
    let target = build_workspace(&directory.path().join("target"), &WorkspaceSpec::default());
    let fence = hold_write_fence(target.layout.database());
    let preflight = RestoreService::preflight(
        &archive,
        &replace_target(&target),
        &RestoreOptions::default(),
    )
    .expect("preflight");
    // A dry run cannot prove the workspace is idle without creating SQLite lock
    // sidecars, so it stays admissible and warns that the fence check happens
    // when the restore is applied. The apply path refuses the active workspace.
    assert!(preflight.admissible, "blockers: {:?}", preflight.blockers);
    assert!(!preflight.plan.target.active);
    assert!(
        preflight
            .warnings
            .iter()
            .any(|warning| warning.code == "active_check_deferred"),
        "expected an active-check-deferred warning: {:?}",
        preflight.warnings
    );
    fence.release().unwrap();
}

#[test]
fn replace_preflight_blocks_pending_client_commands() {
    let directory = tempfile::tempdir().unwrap();
    let (_source, archive) = source_archive(directory.path(), &WorkspaceSpec::default());
    let target = build_workspace(
        &directory.path().join("target"),
        &WorkspaceSpec {
            outbox_rows: 2,
            ..Default::default()
        },
    );
    let preflight = RestoreService::preflight(
        &archive,
        &replace_target(&target),
        &RestoreOptions::default(),
    )
    .expect("preflight");
    assert!(!preflight.admissible);
    assert!(blocker_codes(&preflight).contains(&"pending_client_commands"));
    assert_eq!(preflight.plan.target.pending_outbox_rows, 2);
}

#[test]
fn replica_source_archive_is_refused_as_an_authority_restore_source() {
    let directory = tempfile::tempdir().unwrap();
    let source = build_workspace(&directory.path().join("replica"), &WorkspaceSpec::default());
    // Creation deliberately refuses replica snapshots in this build, so model a
    // format-valid archive that declares replica source mode to prove the
    // restore side still refuses it fail-closed.
    let archive = create_archive(
        &source,
        &directory.path().join("backups"),
        BackupSourceMode::Local,
        "replica-declared",
    )
    .archive;
    rewrite_manifest(&archive, |manifest| {
        manifest.source.mode = BackupSourceMode::Replica;
        manifest.source.excluded_client_pending_data = true;
    });
    let destination = directory.path().join("restore");
    let preflight = RestoreService::preflight(
        &archive,
        &isolated_target(&destination),
        &RestoreOptions::default(),
    )
    .expect("preflight");
    assert!(!preflight.admissible);
    assert!(blocker_codes(&preflight).contains(&"replica_snapshot_not_authoritative"));
}

#[test]
fn server_snapshot_with_enrolled_clients_produces_connected_warnings() {
    let directory = tempfile::tempdir().unwrap();
    let source = build_workspace(
        &directory.path().join("server"),
        &WorkspaceSpec {
            devices: 2,
            web_sessions: 1,
            ..Default::default()
        },
    );
    let archive = create_archive(
        &source,
        &directory.path().join("backups"),
        BackupSourceMode::Server,
        "server",
    )
    .archive;
    let destination = directory.path().join("restore");
    let preflight = RestoreService::preflight(
        &archive,
        &isolated_target(&destination),
        &RestoreOptions::default(),
    )
    .expect("preflight");
    assert!(preflight.admissible, "blockers: {:?}", preflight.blockers);
    assert_eq!(preflight.plan.backup_source_mode, BackupSourceMode::Server);
    assert_eq!(preflight.plan.connected_client_warnings.len(), 1);
    assert!(preflight.plan.connected_client_warnings[0].contains("2 enrolled device"));
    assert!(preflight.plan.connected_client_warnings[0].contains("1 web session"));
}

#[test]
fn newer_schema_archive_is_blocked_before_any_restore() {
    let directory = tempfile::tempdir().unwrap();
    let (_source, archive) = source_archive(directory.path(), &WorkspaceSpec::default());
    let mut manifest = read_manifest(&archive);
    manifest.schema_version = orbit_backup::supported_schema_version() + 1;
    let incompatible = directory.path().join("newer-schema.orbitbak");
    write_manifest_only_archive(&incompatible, &manifest);
    let destination = directory.path().join("restore");
    let preflight = RestoreService::preflight(
        &incompatible,
        &isolated_target(&destination),
        &RestoreOptions {
            deep_verification: false,
            ..RestoreOptions::default()
        },
    )
    .expect("preflight");
    assert!(!preflight.admissible);
    assert!(blocker_codes(&preflight).contains(&"incompatible_schema"));
    assert!(!preflight.plan.schema_compatible);
}

#[test]
fn server_archive_is_refused_over_a_connected_replica_target() {
    let directory = tempfile::tempdir().unwrap();
    let server = build_workspace(
        &directory.path().join("server"),
        &WorkspaceSpec {
            devices: 1,
            ..Default::default()
        },
    );
    let server_archive = create_archive(
        &server,
        &directory.path().join("backups"),
        BackupSourceMode::Server,
        "server",
    )
    .archive;
    let replica = build_workspace(
        &directory.path().join("replica-target"),
        &WorkspaceSpec {
            replica_markers: true,
            ..Default::default()
        },
    );
    let preflight = RestoreService::preflight(
        &server_archive,
        &replace_target(&replica),
        &RestoreOptions::default(),
    )
    .expect("preflight");
    assert!(!preflight.admissible);
    assert!(blocker_codes(&preflight).contains(&"target_connected_replica"));

    // A local authority archive over the same target only warns: the operator
    // may deliberately re-enroll the replica afterwards.
    let local = build_workspace(&directory.path().join("local"), &WorkspaceSpec::default());
    let local_archive = create_archive(
        &local,
        &directory.path().join("backups"),
        BackupSourceMode::Local,
        "local",
    )
    .archive;
    let preflight = RestoreService::preflight(
        &local_archive,
        &replace_target(&replica),
        &RestoreOptions::default(),
    )
    .expect("preflight");
    assert!(preflight.admissible, "blockers: {:?}", preflight.blockers);
    assert!(
        preflight
            .warnings
            .iter()
            .any(|warning| warning.code == "target_looks_like_replica")
    );
}

#[test]
fn corrupted_archive_is_refused_without_touching_the_target() {
    let directory = tempfile::tempdir().unwrap();
    let (_source, archive) = source_archive(directory.path(), &WorkspaceSpec::default());
    let target = build_workspace(&directory.path().join("target"), &WorkspaceSpec::default());
    let manifest = read_manifest(&archive);
    assert!(corrupt_entry_content(
        &archive,
        &manifest,
        "database/orbit.db"
    ));
    let before = hash_tree(&target.root);
    let result = RestoreService::preflight(
        &archive,
        &replace_target(&target),
        &RestoreOptions::default(),
    );
    match result {
        Ok(preflight) => {
            assert!(!preflight.admissible);
            assert!(blocker_codes(&preflight).contains(&"verification_failed"));
        }
        Err(_) => {
            // An archive whose manifest itself cannot be read is reported as an
            // archive-open error; both paths are safe refusals.
        }
    }
    assert_eq!(hash_tree(&target.root), before);
}

#[test]
fn traversal_archive_is_rejected_and_never_escapes_the_destination() {
    let directory = tempfile::tempdir().unwrap();
    let (_source, archive) = source_archive(directory.path(), &WorkspaceSpec::default());
    let manifest = read_manifest(&archive);
    assert!(rewrite_entry_path(
        &archive,
        &manifest,
        "knowledge/Work/note.md",
        "../escape.md"
    ));
    let destination = directory.path().join("restore");
    let mut progress = orbit_backup::NoProgress;
    let error = RestoreService::restore(
        &archive,
        isolated_target(&destination),
        &RestoreOptions::default(),
        &mut progress,
    )
    .expect_err("a traversal archive must be refused");
    assert!(matches!(
        error,
        BackupError::RestoreRefused(_) | BackupError::Integrity(_) | BackupError::Archive(_)
    ));
    assert!(!destination.exists());
    assert!(!directory.path().join("escape.md").exists());
}

#[test]
fn missing_archive_is_an_open_error() {
    let directory = tempfile::tempdir().unwrap();
    let destination = directory.path().join("restore");
    let error = RestoreService::preflight(
        &directory.path().join("absent.orbitbak"),
        &isolated_target(&destination),
        &RestoreOptions::default(),
    )
    .expect_err("missing archive must fail to open");
    assert!(
        matches!(error, BackupError::Io { .. } | BackupError::NotFound(_)),
        "unexpected error: {error}"
    );
}
