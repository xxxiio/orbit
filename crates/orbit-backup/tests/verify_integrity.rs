//! Verification failure-mode tests over deliberately damaged archives.

mod common;

use std::fs;

use common::*;
use orbit_backup::{
    BackupOptions, CheckStatus, DATABASE_ENTRY, VerifyOptions, read_manifest_only, verify_archive,
};
use serde_json::json;

fn ready_backup() -> (TestWorkspace, tempfile::TempDir, std::path::PathBuf) {
    let mut workspace = TestWorkspace::new();
    workspace.write_note("Notes/one.md", "# Unique note body\n");
    workspace.create_vault_dir("Empty");
    workspace.open_database();
    workspace.record_attachment(b"attachment body");
    let destination = tempfile::tempdir().unwrap();
    let outcome = workspace.create_backup(destination.path(), BackupOptions::default());
    assert!(
        outcome.verification.ok,
        "baseline backup must verify: {:?}",
        outcome.verification.problems
    );
    (workspace, destination, outcome.archive)
}

fn tampered_copy(destination: &std::path::Path, archive: &std::path::Path) -> std::path::PathBuf {
    let path = destination.join("tampered.orbitbak");
    fs::copy(archive, &path).unwrap();
    path
}

fn assert_problem(report: &orbit_backup::VerificationReport, check: &str, needle: &str) {
    assert!(!report.ok, "verification must fail for this archive");
    let matching = report
        .problems
        .iter()
        .find(|problem| problem.check == check && problem.detail.contains(needle));
    assert!(
        matching.is_some(),
        "expected a {check} problem containing {needle:?}, got {:?}",
        report.problems
    );
}

#[test]
fn tampered_note_content_fails_hash_verification() {
    let (_workspace, destination, archive) = ready_backup();
    let tampered = tampered_copy(destination.path(), &archive);
    let mut bytes = fs::read(&tampered).unwrap();
    let needle = b"# Unique note body\n";
    let offset = bytes
        .windows(needle.len())
        .position(|window| window == needle)
        .expect("note content must appear in the archive");
    bytes[offset + 2] ^= 0x20;
    fs::write(&tampered, &bytes).unwrap();

    let report = verify_archive(&tampered, &VerifyOptions::default()).unwrap();
    assert_problem(&report, "file_hashes", "knowledge/Notes/one.md");
    assert!(report.manifest.is_some(), "the manifest still parses");
}

#[test]
fn tampered_file_size_fails_hash_verification() {
    let (_workspace, destination, archive) = ready_backup();
    let tampered = tampered_copy(destination.path(), &archive);
    let entries = read_archive_entries(&tampered);
    let rewritten: Vec<(String, Vec<u8>)> = entries
        .into_iter()
        .map(|(path, bytes)| {
            if path == "knowledge/Notes/one.md" {
                (path, b"# Unique note body that is longer now\n".to_vec())
            } else {
                (path, bytes)
            }
        })
        .collect();
    write_archive_entries(&tampered, &rewritten);

    let report = verify_archive(&tampered, &VerifyOptions::default()).unwrap();
    assert_problem(&report, "file_hashes", "declared");
    assert_problem(&report, "file_hashes", "knowledge/Notes/one.md");
}

#[test]
fn undeclared_archive_entry_fails_path_verification() {
    let (_workspace, destination, archive) = ready_backup();
    let tampered = tampered_copy(destination.path(), &archive);
    let mut entries = read_archive_entries(&tampered);
    entries.push((
        "knowledge/Notes/rogue.md".to_owned(),
        b"# Not in the manifest".to_vec(),
    ));
    write_archive_entries(&tampered, &entries);

    let report = verify_archive(&tampered, &VerifyOptions::default()).unwrap();
    assert_problem(&report, "paths", "knowledge/Notes/rogue.md");
    assert_problem(&report, "paths", "not declared");
}

#[test]
fn a_missing_declared_file_fails_verification() {
    let (_workspace, destination, archive) = ready_backup();
    let tampered = tampered_copy(destination.path(), &archive);
    let entries: Vec<(String, Vec<u8>)> = read_archive_entries(&tampered)
        .into_iter()
        .filter(|(path, _)| path != DATABASE_ENTRY)
        .collect();
    write_archive_entries(&tampered, &entries);

    let report = verify_archive(&tampered, &VerifyOptions::default()).unwrap();
    assert_problem(&report, "file_hashes", "missing from the archive");
    assert_problem(&report, "sqlite_integrity", "missing");
    assert_problem(&report, "attachment_presence", "missing");
}

#[test]
fn a_newer_archive_format_is_rejected_with_an_actionable_message() {
    let (_workspace, destination, archive) = ready_backup();
    let tampered = tampered_copy(destination.path(), &archive);
    rewrite_manifest(&tampered, |manifest| {
        manifest["format_version"] = json!(2);
        manifest["min_reader_version"] = json!(2);
    });

    let report = verify_archive(&tampered, &VerifyOptions::default()).unwrap();
    assert_problem(&report, "manifest", "upgrade Orbit");
    let error = read_manifest_only(&tampered).unwrap_err();
    assert!(error.to_string().contains("upgrade Orbit"), "{error}");
}

#[test]
fn a_newer_schema_is_rejected_with_an_actionable_message() {
    let (_workspace, destination, archive) = ready_backup();
    let tampered = tampered_copy(destination.path(), &archive);
    let newer = orbit_backup::supported_schema_version() + 1;
    rewrite_manifest(&tampered, |manifest| {
        manifest["schema_version"] = json!(newer);
    });

    let report = verify_archive(&tampered, &VerifyOptions::default()).unwrap();
    assert_problem(&report, "schema_compatibility", "newer");
    let error = read_manifest_only(&tampered).unwrap_err();
    assert!(error.to_string().contains("newer"), "{error}");
}

#[test]
fn truncated_and_garbage_archives_are_reported_not_errors() {
    let (_workspace, destination, archive) = ready_backup();
    let bytes = fs::read(&archive).unwrap();

    let truncated = destination.path().join("truncated.orbitbak");
    fs::write(&truncated, &bytes[..bytes.len() / 2]).unwrap();
    let report = verify_archive(&truncated, &VerifyOptions::default()).unwrap();
    assert!(!report.ok);
    assert!(
        report.problems.iter().any(|problem| {
            problem.check == "structure" && problem.detail.to_lowercase().contains("truncat")
        }),
        "{:?}",
        report.problems
    );
    let hashes = report
        .checks
        .iter()
        .find(|check| check.name == "file_hashes")
        .unwrap();
    assert_ne!(
        hashes.status,
        orbit_backup::CheckStatus::Pass,
        "a failed structure must never report a hash pass: {hashes:?}"
    );

    let garbage = destination.path().join("garbage.orbitbak");
    fs::write(&garbage, vec![0x7f_u8; 2048]).unwrap();
    let report = verify_archive(&garbage, &VerifyOptions::default()).unwrap();
    assert!(!report.ok);
    assert_problem(&report, "manifest", "manifest.json");

    let empty = destination.path().join("empty.orbitbak");
    fs::write(&empty, b"").unwrap();
    let report = verify_archive(&empty, &VerifyOptions::default()).unwrap();
    assert!(!report.ok);
    assert!(
        report
            .problems
            .iter()
            .any(|problem| problem.check == "structure" && problem.detail.contains("truncated")),
        "{:?}",
        report.problems
    );
}

#[test]
fn a_missing_archive_cannot_be_opened() {
    let directory = tempfile::tempdir().unwrap();
    let missing = directory.path().join("absent.orbitbak");
    let error = verify_archive(&missing, &VerifyOptions::default()).unwrap_err();
    assert!(
        matches!(error, orbit_backup::BackupError::Io { .. }),
        "{error}"
    );
}

#[test]
fn structural_only_verification_skips_the_deep_checks_but_still_verifies_hashes() {
    let (_workspace, _destination, archive) = ready_backup();
    let report = verify_archive(&archive, &VerifyOptions { deep: false }).unwrap();
    assert!(report.ok, "{:?}", report.problems);
    let status = |name: &str| {
        report
            .checks
            .iter()
            .find(|check| check.name == name)
            .unwrap()
            .status
    };
    assert_eq!(status("sqlite_integrity"), CheckStatus::Skipped);
    assert_eq!(status("attachment_presence"), CheckStatus::Skipped);
    assert_eq!(status("file_hashes"), CheckStatus::Pass);
    assert!(report.checked_files() > 0);
}
