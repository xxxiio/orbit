//! Automatic-backup policy schema tests: persistence, validation and due
//! logic. Retention itself is covered in `policy_retention_retention.rs`.

use std::fs;

use orbit_backup::{BackupPolicy, BackupSchedule};

const BASE_MS: i64 = 1_700_000_000_000;
const DAY_MS: i64 = 86_400_000;

#[test]
fn policy_file_round_trips_and_drives_retention_limits() {
    let directory = tempfile::tempdir().unwrap();
    let policy = orbit_backup::BackupPolicy {
        schedule: orbit_backup::BackupSchedule::Daily,
        max_retained: Some(3),
        min_retention_days: Some(2),
        max_total_bytes: Some(1024 * 1024),
        ..orbit_backup::BackupPolicy::default()
    };
    let path = policy.save(directory.path()).expect("save policy");
    assert_eq!(path, directory.path().join("policy.json"));
    let loaded = orbit_backup::BackupPolicy::load(directory.path()).expect("load policy");
    assert_eq!(loaded, policy);
    assert_eq!(
        loaded.retention_policy().min_retention_ms,
        Some(2 * DAY_MS),
        "days must convert to milliseconds"
    );
    assert!(!loaded.is_due(BASE_MS, Some(BASE_MS - 1000)));
}

#[test]
fn invalid_policy_file_fails_closed() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(
        directory.path().join("policy.json"),
        br#"{"version":1,"schedule":"daily","max_retained":0}"#,
    )
    .unwrap();
    let error = orbit_backup::BackupPolicy::load(directory.path()).expect_err("must reject");
    assert!(error.to_string().contains("max_retained"));
}

#[test]
fn disabled_and_before_upgrade_schedules_are_never_timer_due() {
    for schedule in [BackupSchedule::Disabled, BackupSchedule::BeforeUpgrade] {
        let policy = BackupPolicy {
            schedule,
            ..BackupPolicy::default()
        };
        assert!(!policy.is_due(BASE_MS, None));
        assert!(!policy.is_due(BASE_MS, Some(BASE_MS - 400 * DAY_MS)));
    }
}

#[test]
fn weekly_schedule_is_due_after_seven_days() {
    let policy = BackupPolicy {
        schedule: BackupSchedule::Weekly,
        ..BackupPolicy::default()
    };
    assert!(!policy.is_due(BASE_MS, Some(BASE_MS - 6 * DAY_MS)));
    assert!(policy.is_due(BASE_MS, Some(BASE_MS - 7 * DAY_MS)));
}

#[test]
fn invalid_policy_values_fail_validation() {
    let unlimited_retained = BackupPolicy {
        max_retained: Some(0),
        ..BackupPolicy::default()
    };
    assert!(unlimited_retained.validate().is_err());
    let tiny_budget = BackupPolicy {
        max_total_bytes: Some(512),
        ..BackupPolicy::default()
    };
    assert!(tiny_budget.validate().is_err());
    let excessive_age = BackupPolicy {
        min_retention_days: Some(9_999),
        ..BackupPolicy::default()
    };
    assert!(excessive_age.validate().is_err());
}

#[test]
fn unsupported_policy_version_fails_closed() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(
        directory.path().join("policy.json"),
        br#"{"version":99,"schedule":"daily"}"#,
    )
    .unwrap();
    let error = BackupPolicy::load(directory.path()).expect_err("newer version must be refused");
    assert!(error.to_string().contains("not supported"));
}

mod common;

use std::path::PathBuf;

use common::TestWorkspace;
use orbit_backup::BackupOptions;
use orbit_backup::retention::{RetentionPolicy, apply_retention};

#[test]
fn retention_keeps_the_last_verified_recovery_point_of_real_archives() {
    let mut workspace = TestWorkspace::new();
    workspace.open_database();
    workspace.write_note("Work/one.md", "# One\n");
    workspace.record_attachment(b"real attachment content");
    let backups = workspace.root().join("backups");
    let mut archives = Vec::new();
    for label in ["first", "second", "third"] {
        let outcome = workspace.create_backup(
            &backups,
            BackupOptions {
                label: Some(label.to_owned()),
                ..BackupOptions::default()
            },
        );
        assert!(outcome.verification.ok, "{label} must verify");
        assert!(
            outcome.sidecar.is_some(),
            "a created archive always gets a verification sidecar"
        );
        archives.push(outcome.archive);
    }

    let policy = RetentionPolicy {
        max_retained: Some(2),
        ..RetentionPolicy::default()
    };
    let outcome = apply_retention(&backups, &policy, orbit_backup::now_ms(), None, false)
        .expect("retention on real archives");

    assert_eq!(outcome.removed.len(), 1);
    assert!(
        !archives[0].exists(),
        "the oldest verified archive is removed"
    );
    assert!(archives[1].exists());
    assert!(
        archives[2].exists(),
        "the newest verified recovery point must never be removed"
    );
    let protected = outcome.protected.expect("a protected recovery point");
    assert_eq!(
        PathBuf::from(protected).file_name(),
        archives[2].file_name(),
        "protection must select the newest verified archive"
    );
    let removed_sidecar = PathBuf::from(format!("{}.verify.json", archives[0].display()));
    assert!(
        !removed_sidecar.exists(),
        "the verification sidecar is removed with its archive"
    );
}
