//! Retention integration tests against real `.orbitbak` archives.
//!
//! Archives are assembled with the real container writer and manifest types so
//! discovery runs through the same manifest validation the CLI and server use.
//! They contain a database entry and one Markdown note; the point under test is
//! retention behaviour, not archive creation (see the create/verify tests).

use std::fs;
use std::path::{Path, PathBuf};

use orbit_backup::manifest::{
    BACKUP_EXTENSION, BACKUP_FORMAT_VERSION, BACKUP_MIN_READER_VERSION, BackupCounts,
    BackupManifest, BackupSource, BackupSourceMode, BackupTotals, DATABASE_ENTRY, InventoryEntry,
    InventoryKind, MANIFEST_ENTRY,
};
use orbit_backup::retention::{RetentionPolicy, apply_retention};
use orbit_backup::tar_fmt::TarWriter;
use orbit_backup::util::{format_compact_timestamp, format_rfc3339_utc, hex_digest, label_slug};
use orbit_backup::verify::VerificationSidecar;

const BASE_MS: i64 = 1_700_000_000_000;
const DAY_MS: i64 = 86_400_000;

fn sha256(bytes: &[u8]) -> String {
    use sha2::Digest;
    hex_digest(sha2::Sha256::digest(bytes))
}

fn inventory_entry(path: &str, kind: InventoryKind, bytes: &[u8]) -> InventoryEntry {
    InventoryEntry {
        path: path.to_owned(),
        kind,
        size_bytes: bytes.len() as u64,
        sha256: sha256(bytes),
    }
}

/// Write one valid archive containing a database and one Markdown note.
fn write_archive(directory: &Path, created_at_ms: i64, label: &str) -> PathBuf {
    let database = format!("sqlite database for {label}").into_bytes();
    let note = format!("# {label}\n").into_bytes();
    let note_path = "knowledge/Work/one.md";
    let inventory = vec![
        inventory_entry(DATABASE_ENTRY, InventoryKind::Database, &database),
        inventory_entry(note_path, InventoryKind::KnowledgeNote, &note),
    ];
    let content_bytes = inventory.iter().map(|entry| entry.size_bytes).sum();
    let manifest = BackupManifest {
        format_version: BACKUP_FORMAT_VERSION,
        min_reader_version: BACKUP_MIN_READER_VERSION,
        orbit_version: "0.1.0".to_owned(),
        orbit_build: None,
        schema_version: orbit_backup::supported_schema_version(),
        created_at_ms,
        created_at: format_rfc3339_utc(created_at_ms),
        label: Some(label.to_owned()),
        complete: true,
        source: BackupSource {
            mode: BackupSourceMode::Local,
            workspace_id: Some("generation-1".to_owned()),
            revision: Some(1),
            described_as: "local authoritative workspace".to_owned(),
            excluded_client_pending_data: false,
        },
        config: None,
        exclusions: Vec::new(),
        inventory,
        directories: vec!["knowledge/Work".to_owned()],
        totals: BackupTotals {
            database_bytes: database.len() as u64,
            knowledge_bytes: note.len() as u64,
            attachment_bytes: 0,
            config_bytes: 0,
            content_bytes,
            archive_bytes: 0,
        },
        counts: BackupCounts::default(),
    };
    manifest.validate().expect("test manifest must be valid");
    let name = format!(
        "orbit-{}-{}.{}",
        format_compact_timestamp(created_at_ms),
        label_slug(label),
        BACKUP_EXTENSION
    );
    let path = directory.join(name);
    let file = fs::File::create(&path).expect("create archive");
    let mut writer = TarWriter::new(std::io::BufWriter::new(file));
    writer
        .append_bytes(MANIFEST_ENTRY, &manifest.to_json().expect("manifest json"))
        .expect("append manifest");
    writer
        .append_bytes(DATABASE_ENTRY, &database)
        .expect("append database");
    writer.append_bytes(note_path, &note).expect("append note");
    writer.finish().expect("finish archive");
    path
}

/// Write the sidecar in both documented forms; listing reads the first.
fn write_sidecar(archive: &Path, ok: bool) {
    let archive_bytes = fs::metadata(archive).expect("archive metadata").len();
    let sidecar = VerificationSidecar {
        verified_at_ms: 0,
        ok,
        archive_bytes,
        format_version: BACKUP_FORMAT_VERSION,
        verified_files: 2,
        problems: Vec::new(),
    };
    let bytes = serde_json::to_vec(&sidecar).expect("sidecar json");
    let name = archive
        .file_name()
        .and_then(|name| name.to_str())
        .expect("archive name");
    fs::write(
        archive.with_file_name(format!("{name}.verify.json")),
        &bytes,
    )
    .expect("sidecar");
    fs::write(archive.with_extension("verify.json"), &bytes).expect("sidecar alt");
}

fn sidecar_paths(archive: &Path) -> Vec<PathBuf> {
    let name = archive
        .file_name()
        .and_then(|name| name.to_str())
        .expect("archive name");
    vec![
        archive.with_file_name(format!("{name}.verify.json")),
        archive.with_extension("verify.json"),
    ]
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .expect("file name")
        .to_owned()
}

fn names(paths: &[PathBuf]) -> Vec<String> {
    let mut names: Vec<String> = paths.iter().map(|path| file_name(path)).collect();
    names.sort();
    names
}

fn removed_names(outcome: &orbit_backup::RetentionOutcome) -> Vec<String> {
    let mut names: Vec<String> = outcome
        .removed
        .iter()
        .map(|path| {
            Path::new(path)
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| path.clone())
        })
        .collect();
    names.sort();
    names
}

fn archive_sizes(paths: &[&Path]) -> u64 {
    paths
        .iter()
        .map(|path| fs::metadata(path).expect("archive metadata").len())
        .sum()
}

#[test]
fn keeps_newest_n_and_protects_the_newest_verified_recovery_point() {
    let directory = tempfile::tempdir().unwrap();
    let archives: Vec<PathBuf> = (1..=4)
        .map(|index| {
            let path = write_archive(
                directory.path(),
                BASE_MS + i64::from(index) * DAY_MS,
                &format!("daily-{index}"),
            );
            write_sidecar(&path, true);
            path
        })
        .collect();
    let bytes_before = archive_sizes(&archives.iter().map(PathBuf::as_path).collect::<Vec<_>>());
    let policy = RetentionPolicy {
        max_retained: Some(2),
        ..RetentionPolicy::default()
    };
    let outcome = apply_retention(
        directory.path(),
        &policy,
        BASE_MS + 10 * DAY_MS,
        None,
        false,
    )
    .expect("retention");

    assert_eq!(removed_names(&outcome), names(&archives[..2]));
    assert_eq!(outcome.retained, 2);
    assert_eq!(
        outcome
            .protected
            .as_deref()
            .map(|path| file_name(Path::new(path))),
        Some(file_name(&archives[3]))
    );
    assert_eq!(outcome.bytes_before, bytes_before);
    assert_eq!(
        outcome.bytes_after,
        archive_sizes(&[&archives[2], &archives[3]])
    );
    assert!(!archives[0].exists() && !archives[1].exists());
    assert!(archives[2].exists() && archives[3].exists());
    for removed in &archives[..2] {
        for sidecar in sidecar_paths(removed) {
            assert!(
                !sidecar.exists(),
                "sidecar {} must be removed",
                sidecar.display()
            );
        }
    }
    for kept in &archives[2..] {
        assert!(
            sidecar_paths(kept)[0].exists(),
            "kept archive keeps its sidecar"
        );
    }
    assert!(
        outcome
            .retained_reasons
            .iter()
            .any(|reason| reason.contains("protected recovery point"))
    );
}

#[test]
fn explicit_protected_archive_is_never_removed() {
    let directory = tempfile::tempdir().unwrap();
    let archives: Vec<PathBuf> = (1..=3)
        .map(|index| {
            let path = write_archive(
                directory.path(),
                BASE_MS + i64::from(index) * DAY_MS,
                &format!("manual-{index}"),
            );
            write_sidecar(&path, true);
            path
        })
        .collect();
    let policy = RetentionPolicy {
        max_retained: Some(1),
        ..RetentionPolicy::default()
    };
    let outcome = apply_retention(
        directory.path(),
        &policy,
        BASE_MS + 10 * DAY_MS,
        Some(&archives[0]),
        false,
    )
    .expect("retention");

    assert_eq!(
        outcome
            .protected
            .as_deref()
            .map(|path| file_name(Path::new(path))),
        Some(file_name(&archives[0]))
    );
    assert!(
        archives[0].exists(),
        "the explicitly protected archive must survive"
    );
    assert_eq!(outcome.retained, 1);
    assert_eq!(removed_names(&outcome), names(&archives[1..]));
}

#[test]
fn minimum_retention_age_protects_young_archives() {
    let directory = tempfile::tempdir().unwrap();
    let now = BASE_MS + 100 * DAY_MS;
    let archives: Vec<PathBuf> = [30, 20, 5, 1]
        .iter()
        .enumerate()
        .map(|(index, age_days)| {
            write_archive(
                directory.path(),
                now - i64::from(*age_days) * DAY_MS,
                &format!("age-{}", index + 1),
            )
        })
        .collect();
    let policy = RetentionPolicy {
        max_retained: Some(1),
        min_retention_ms: Some(10 * DAY_MS),
        max_total_bytes: None,
    };
    let outcome = apply_retention(directory.path(), &policy, now, None, false).expect("retention");

    assert!(
        archives[3].exists(),
        "the youngest archive must never be removed"
    );
    assert!(
        archives[2].exists(),
        "an archive younger than the minimum retention age must be kept"
    );
    assert!(!archives[0].exists() && !archives[1].exists());
    assert!(
        outcome
            .retained_reasons
            .iter()
            .any(|reason| reason.contains("younger than the minimum retention age")),
        "reasons: {:?}",
        outcome.retained_reasons
    );
    assert_eq!(outcome.retained, 2);
}

#[test]
fn max_total_bytes_removes_oldest_until_under_the_limit() {
    let directory = tempfile::tempdir().unwrap();
    let archives: Vec<PathBuf> = (1..=4)
        .map(|index| {
            write_archive(
                directory.path(),
                BASE_MS + i64::from(index) * DAY_MS,
                &format!("bytes-{index}"),
            )
        })
        .collect();
    let limit = archive_sizes(&[&archives[2], &archives[3]]);
    let policy = RetentionPolicy {
        max_retained: None,
        min_retention_ms: None,
        max_total_bytes: Some(limit),
    };
    let outcome = apply_retention(
        directory.path(),
        &policy,
        BASE_MS + 10 * DAY_MS,
        None,
        false,
    )
    .expect("retention");

    assert_eq!(removed_names(&outcome), names(&archives[..2]));
    assert!(outcome.bytes_after <= limit);
    assert_eq!(outcome.bytes_after, limit);
}

#[test]
fn dry_run_reports_candidates_without_deleting_anything() {
    let directory = tempfile::tempdir().unwrap();
    let archives: Vec<PathBuf> = (1..=3)
        .map(|index| {
            let path = write_archive(
                directory.path(),
                BASE_MS + i64::from(index) * DAY_MS,
                &format!("dry-{index}"),
            );
            write_sidecar(&path, true);
            path
        })
        .collect();
    let policy = RetentionPolicy {
        max_retained: Some(1),
        ..RetentionPolicy::default()
    };
    let outcome = apply_retention(directory.path(), &policy, BASE_MS + 10 * DAY_MS, None, true)
        .expect("retention");

    assert_eq!(removed_names(&outcome), names(&archives[..2]));
    assert_eq!(outcome.retained, 1);
    assert_eq!(
        outcome.bytes_after,
        fs::metadata(&archives[2]).unwrap().len()
    );
    for archive in &archives {
        assert!(
            archive.exists(),
            "dry-run must not delete {}",
            archive.display()
        );
        for sidecar in sidecar_paths(archive) {
            assert!(sidecar.exists(), "dry-run must not delete sidecars");
        }
    }
}

#[test]
fn non_backup_files_nested_directories_and_external_copies_are_untouched() {
    let directory = tempfile::tempdir().unwrap();
    let external = tempfile::tempdir().unwrap();
    let oldest = write_archive(directory.path(), BASE_MS, "keep-old");
    let newest = write_archive(directory.path(), BASE_MS + DAY_MS, "keep-new");
    let user_file = directory.path().join("notes.txt");
    fs::write(&user_file, b"user data that retention must never touch").unwrap();
    let policy_file = directory.path().join("policy.json");
    fs::write(&policy_file, br#"{"version":1,"schedule":"daily"}"#).unwrap();
    let corrupt = directory.path().join("broken.orbitbak");
    fs::write(&corrupt, b"this is not a tar archive").unwrap();
    let nested = directory.path().join("nested");
    fs::create_dir(&nested).unwrap();
    let nested_archive = write_archive(&nested, BASE_MS, "nested-backup");
    let external_archive = write_archive(external.path(), BASE_MS, "external-backup");

    let policy = RetentionPolicy {
        max_retained: Some(1),
        ..RetentionPolicy::default()
    };
    let outcome = apply_retention(
        directory.path(),
        &policy,
        BASE_MS + 10 * DAY_MS,
        None,
        false,
    )
    .expect("retention");

    assert_eq!(removed_names(&outcome), vec![file_name(&oldest)]);
    assert!(newest.exists());
    assert!(user_file.exists());
    assert!(policy_file.exists());
    assert!(corrupt.exists());
    assert!(nested_archive.exists());
    assert!(external_archive.exists());
}

#[test]
fn newest_passing_sidecar_is_preferred_over_a_plain_newest_manifest() {
    let directory = tempfile::tempdir().unwrap();
    let oldest = write_archive(directory.path(), BASE_MS, "prefer-1");
    let verified = write_archive(directory.path(), BASE_MS + DAY_MS, "prefer-2");
    write_sidecar(&verified, true);
    let newest_but_failed = write_archive(directory.path(), BASE_MS + 2 * DAY_MS, "prefer-3");
    write_sidecar(&newest_but_failed, false);

    let policy = RetentionPolicy {
        max_retained: Some(1),
        ..RetentionPolicy::default()
    };
    let outcome = apply_retention(
        directory.path(),
        &policy,
        BASE_MS + 10 * DAY_MS,
        None,
        false,
    )
    .expect("retention");

    assert_eq!(
        outcome
            .protected
            .as_deref()
            .map(|path| file_name(Path::new(path))),
        Some(file_name(&verified)),
        "a passing sidecar marks the recovery point even when a newer unverified file exists"
    );
    assert!(verified.exists());
    assert!(!newest_but_failed.exists());
    assert!(!oldest.exists());
}

#[test]
fn without_sidecars_the_newest_valid_manifest_is_protected() {
    let directory = tempfile::tempdir().unwrap();
    let archives: Vec<PathBuf> = (1..=3)
        .map(|index| {
            write_archive(
                directory.path(),
                BASE_MS + i64::from(index) * DAY_MS,
                &format!("plain-{index}"),
            )
        })
        .collect();
    let policy = RetentionPolicy {
        max_retained: Some(1),
        ..RetentionPolicy::default()
    };
    let outcome = apply_retention(
        directory.path(),
        &policy,
        BASE_MS + 10 * DAY_MS,
        None,
        false,
    )
    .expect("retention");

    assert_eq!(
        outcome
            .protected
            .as_deref()
            .map(|path| file_name(Path::new(path))),
        Some(file_name(&archives[2]))
    );
    assert!(archives[2].exists());
}
