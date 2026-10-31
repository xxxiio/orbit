//! Inspection and directory-listing behaviour, including external copies and
//! corrupt files.

mod common;

use std::fs;
use std::path::{Path, PathBuf};

use common::*;
use orbit_backup::{
    BackupOptions, VERIFICATION_SIDECAR_SUFFIX, VerificationStatus, inspect_archive, list_backups,
    read_manifest_only,
};
use serde_json::json;

fn sidecar_path(archive: &Path) -> PathBuf {
    let mut text = archive.as_os_str().to_owned();
    text.push(".");
    text.push(VERIFICATION_SIDECAR_SUFFIX);
    PathBuf::from(text)
}

fn backup_to(destination: &Path) -> (TestWorkspace, PathBuf) {
    let mut workspace = TestWorkspace::new();
    workspace.write_note("Notes/one.md", "# One\n");
    workspace.open_database();
    let outcome = workspace.create_backup(destination, BackupOptions::default());
    assert!(
        outcome.verification.ok,
        "{:?}",
        outcome.verification.problems
    );
    (workspace, outcome.archive)
}

#[test]
fn inspection_reads_only_the_manifest_and_reports_components() {
    let directory = tempfile::tempdir().unwrap();
    let (_workspace, archive) = backup_to(directory.path());
    let inspection = inspect_archive(&archive).unwrap();
    assert_eq!(inspection.archive_path, archive.display().to_string());
    assert_eq!(
        inspection.inventory_files,
        inspection.manifest.inventory.len()
    );
    assert_eq!(
        inspection.content_bytes,
        inspection.manifest.totals.content_bytes
    );
    assert!(inspection.creation_verification.as_ref().unwrap().ok);
    assert_eq!(read_manifest_only(&archive).unwrap(), inspection.manifest);
}

#[test]
fn a_corrupt_or_foreign_file_cannot_be_inspected() {
    let directory = tempfile::tempdir().unwrap();
    let corrupt = directory.path().join("corrupt.orbitbak");
    fs::write(&corrupt, b"this is not a tar container").unwrap();
    let error = inspect_archive(&corrupt).unwrap_err();
    assert!(
        error.to_string().contains("invalid backup archive"),
        "{error}"
    );

    let empty = directory.path().join("empty.orbitbak");
    fs::write(&empty, b"").unwrap();
    assert!(inspect_archive(&empty).is_err());
}

#[test]
fn inspection_tolerates_a_missing_or_invalid_sidecar() {
    let directory = tempfile::tempdir().unwrap();
    let (_workspace, archive) = backup_to(directory.path());
    fs::remove_file(sidecar_path(&archive)).unwrap();
    assert!(
        inspect_archive(&archive)
            .unwrap()
            .creation_verification
            .is_none()
    );

    fs::write(sidecar_path(&archive), b"{ not json").unwrap();
    assert!(
        inspect_archive(&archive)
            .unwrap()
            .creation_verification
            .is_none()
    );
}

#[test]
fn listing_creates_a_missing_directory_and_ignores_unrelated_files() {
    let directory = tempfile::tempdir().unwrap();
    let missing = directory.path().join("nested").join("backups");
    let listing = list_backups(&missing, None).unwrap();
    assert!(missing.is_dir());
    assert!(listing.backups.is_empty());
    assert!(listing.skipped.is_empty());

    fs::write(missing.join("notes.txt"), b"ignored").unwrap();
    fs::write(missing.join("archive.orbitbak.bak"), b"ignored").unwrap();
    let listing = list_backups(&missing, None).unwrap();
    assert!(listing.backups.is_empty());
    assert!(
        listing.skipped.is_empty(),
        "only *.orbitbak files are candidates"
    );
}

#[test]
fn listing_reports_corrupt_archives_as_skipped_and_keeps_the_rest() {
    let directory = tempfile::tempdir().unwrap();
    let (_workspace, _archive) = backup_to(directory.path());
    fs::write(directory.path().join("corrupt.orbitbak"), b"garbage").unwrap();
    fs::write(directory.path().join("empty.orbitbak"), b"").unwrap();

    let listing = list_backups(directory.path(), None).unwrap();
    assert_eq!(listing.backups.len(), 1);
    assert_eq!(listing.skipped.len(), 2);
    for skipped in &listing.skipped {
        assert!(skipped.path.ends_with(".orbitbak"));
        assert!(!skipped.reason.is_empty());
    }
}

#[test]
fn external_copies_without_a_sidecar_are_listed_as_unknown() {
    let source = tempfile::tempdir().unwrap();
    let (_workspace, archive) = backup_to(source.path());

    let external = tempfile::tempdir().unwrap();
    let copied = external.path().join("copied-from-another-machine.orbitbak");
    fs::copy(&archive, &copied).unwrap();
    let listing = list_backups(external.path(), None).unwrap();
    assert_eq!(listing.backups.len(), 1);
    assert_eq!(listing.backups[0].verification, VerificationStatus::Unknown);
    assert!(listing.backups[0].verification_checked_at_ms.is_none());
    assert_eq!(listing.backups[0].label, None);
}

#[test]
fn a_failing_sidecar_marks_the_archive_as_failed() {
    let directory = tempfile::tempdir().unwrap();
    let (_workspace, archive) = backup_to(directory.path());
    let sidecar = json!({
        "verified_at_ms": 1_700_000_000_123_i64,
        "ok": false,
        "archive_bytes": 1,
        "format_version": 1,
        "verified_files": 0,
        "problems": [{"check": "file_hashes", "detail": "deliberate"}],
    });
    fs::write(
        sidecar_path(&archive),
        serde_json::to_vec(&sidecar).unwrap(),
    )
    .unwrap();

    let listing = list_backups(directory.path(), None).unwrap();
    assert_eq!(listing.backups.len(), 1);
    assert_eq!(listing.backups[0].verification, VerificationStatus::Failed);
    assert_eq!(
        listing.backups[0].verification_checked_at_ms,
        Some(1_700_000_000_123)
    );
}

#[test]
fn listing_is_newest_first_and_respects_the_limit() {
    let source = tempfile::tempdir().unwrap();
    let (_workspace, archive) = backup_to(source.path());
    let directory = tempfile::tempdir().unwrap();
    for (name, created_at_ms) in [
        ("a.orbitbak", 1_000_i64),
        ("b.orbitbak", 3_000),
        ("c.orbitbak", 2_000),
    ] {
        let copy = directory.path().join(name);
        fs::copy(&archive, &copy).unwrap();
        rewrite_manifest(&copy, |manifest| {
            manifest["created_at_ms"] = json!(created_at_ms);
        });
    }
    let listing = list_backups(directory.path(), None).unwrap();
    let ordered: Vec<&str> = listing
        .backups
        .iter()
        .map(|entry| entry.file_name.as_str())
        .collect();
    assert_eq!(ordered, vec!["b.orbitbak", "c.orbitbak", "a.orbitbak"]);

    let limited = list_backups(directory.path(), Some(2)).unwrap();
    assert_eq!(limited.backups.len(), 2);
    assert_eq!(limited.backups[0].file_name, "b.orbitbak");
}

#[test]
fn listed_entries_expose_manifest_facts() {
    let mut workspace = TestWorkspace::new();
    workspace.write_note("Notes/one.md", "# One\n");
    workspace.open_database();
    workspace.record_attachment(b"content");
    let directory = tempfile::tempdir().unwrap();
    let outcome = workspace.create_backup(
        directory.path(),
        BackupOptions {
            label: Some("Release rehearsal".to_owned()),
            ..BackupOptions::default()
        },
    );
    let listing = list_backups(directory.path(), None).unwrap();
    let entry = &listing.backups[0];
    assert_eq!(entry.label.as_deref(), Some("Release rehearsal"));
    assert_eq!(entry.created_at, outcome.manifest.created_at);
    assert_eq!(entry.source_mode, outcome.manifest.source.mode);
    assert_eq!(entry.format_version, outcome.manifest.format_version);
    assert_eq!(entry.schema_version, outcome.manifest.schema_version);
    assert_eq!(entry.complete, outcome.manifest.complete);
    assert_eq!(entry.verification, VerificationStatus::Verified);
    assert_eq!(
        entry.size_bytes,
        fs::metadata(&outcome.archive).unwrap().len()
    );
}
