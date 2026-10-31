//! Automatic-backup policy: schedule and retention preferences.
//!
//! The policy is stored as `<backup directory>/policy.json` so a local CLI and a
//! self-hosted server with the same backup directory always read one file. It
//! is deliberately not part of the unified Orbit configuration: adding fields
//! there would make older binaries reject a newer config file, and backup
//! preferences are operational, not part of the workspace model.
//!
//! Retention limits are applied only after a verified backup creation by
//! [`crate::retention::apply_retention`]. This module never deletes anything.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{BackupError, Result};
use crate::retention::{BackupSchedule, RetentionPolicy};
use crate::util;

/// File name of the policy inside the backup directory.
pub const POLICY_FILE_NAME: &str = "policy.json";
/// Only this policy schema version is accepted by this build.
pub const POLICY_VERSION: u32 = 1;
/// A day in milliseconds, used for schedule due-checks.
pub const DAY_MS: i64 = 86_400_000;

/// Durable automatic-backup policy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BackupPolicy {
    /// Policy schema version.
    pub version: u32,
    /// Which automatic backups are enabled.
    #[serde(default)]
    pub schedule: BackupSchedule,
    /// Maximum number of local backups to keep, including the newest.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_retained: Option<usize>,
    /// Minimum age in days before a backup may be removed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_retention_days: Option<u32>,
    /// Maximum total bytes of local backups.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_total_bytes: Option<u64>,
}

impl Default for BackupPolicy {
    fn default() -> Self {
        Self {
            version: POLICY_VERSION,
            schedule: BackupSchedule::Disabled,
            max_retained: None,
            min_retention_days: None,
            max_total_bytes: None,
        }
    }
}

impl BackupPolicy {
    /// Load the policy from a backup directory, creating the directory when
    /// missing. An absent file means defaults; an invalid file fails closed.
    pub fn load(backup_directory: &Path) -> Result<Self> {
        let path = backup_directory.join(POLICY_FILE_NAME);
        match std::fs::read(&path) {
            Ok(bytes) => {
                let policy: Self = serde_json::from_slice(&bytes).map_err(|error| {
                    BackupError::Manifest(format!(
                        "invalid backup policy {}: {error}",
                        path.display()
                    ))
                })?;
                policy.validate()?;
                Ok(policy)
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(error) => Err(BackupError::io(
                format!("read backup policy {}", path.display()),
                error,
            )),
        }
    }

    /// Persist the policy atomically beside the backups.
    pub fn save(&self, backup_directory: &Path) -> Result<PathBuf> {
        self.validate()?;
        util::create_private_dir(backup_directory)?;
        let path = backup_directory.join(POLICY_FILE_NAME);
        let temporary = backup_directory.join(format!(".policy-{}.json", util::unique_suffix()));
        let bytes = serde_json::to_vec_pretty(self)
            .map_err(|error| BackupError::Manifest(format!("serialize backup policy: {error}")))?;
        std::fs::write(&temporary, &bytes).map_err(|error| {
            BackupError::io(
                format!("write backup policy {}", temporary.display()),
                error,
            )
        })?;
        util::fsync_file(&temporary)?;
        util::atomic_rename(&temporary, &path)?;
        Ok(path)
    }

    /// Validate the policy schema and values.
    pub fn validate(&self) -> Result<()> {
        if self.version != POLICY_VERSION {
            return Err(BackupError::Manifest(format!(
                "backup policy version {} is not supported (expected {POLICY_VERSION})",
                self.version
            )));
        }
        if self.max_retained == Some(0) {
            return Err(BackupError::InvalidArgument(
                "max_retained must be at least 1; the newest verified backup is always kept"
                    .to_owned(),
            ));
        }
        if let Some(days) = self.min_retention_days
            && days > 3_650
        {
            return Err(BackupError::InvalidArgument(
                "min_retention_days must be at most 3650".to_owned(),
            ));
        }
        if let Some(bytes) = self.max_total_bytes
            && bytes < 1_048_576
        {
            return Err(BackupError::InvalidArgument(
                "max_total_bytes must be at least 1 MiB or omitted".to_owned(),
            ));
        }
        Ok(())
    }

    /// Convert the retention fields into the retention engine's policy.
    pub fn retention_policy(&self) -> RetentionPolicy {
        RetentionPolicy {
            max_retained: self.max_retained,
            min_retention_ms: self.min_retention_days.map(|days| i64::from(days) * DAY_MS),
            max_total_bytes: self.max_total_bytes,
        }
    }

    /// Whether an automatic backup is due.
    ///
    /// `last_backup_created_at_ms` is the creation time of the newest known
    /// backup, regardless of schedule. `BeforeUpgrade` is never due on a timer;
    /// the upgrade path runs it explicitly.
    pub fn is_due(&self, now_ms: i64, last_backup_created_at_ms: Option<i64>) -> bool {
        match self.schedule {
            BackupSchedule::Disabled | BackupSchedule::BeforeUpgrade => false,
            BackupSchedule::Daily => match last_backup_created_at_ms {
                None => true,
                Some(last) => utc_day(now_ms) > utc_day(last),
            },
            BackupSchedule::Weekly => match last_backup_created_at_ms {
                None => true,
                Some(last) => now_ms.saturating_sub(last) >= 7 * DAY_MS,
            },
        }
    }
}

/// UTC day index for schedule comparisons.
pub fn utc_day(epoch_ms: i64) -> i64 {
    epoch_ms.div_euclid(DAY_MS)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temporary_directory() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    #[test]
    fn missing_policy_file_means_defaults() {
        let directory = temporary_directory();
        let policy = BackupPolicy::load(directory.path()).unwrap();
        assert_eq!(policy, BackupPolicy::default());
        assert_eq!(policy.schedule, BackupSchedule::Disabled);
        assert!(!policy.is_due(1_700_000_000_000, None));
    }

    #[test]
    fn policy_round_trips_and_validates() {
        let directory = temporary_directory();
        let policy = BackupPolicy {
            version: POLICY_VERSION,
            schedule: BackupSchedule::Daily,
            max_retained: Some(10),
            min_retention_days: Some(7),
            max_total_bytes: Some(5 * 1024 * 1024 * 1024),
        };
        policy.save(directory.path()).unwrap();
        let loaded = BackupPolicy::load(directory.path()).unwrap();
        assert_eq!(loaded, policy);
        assert_eq!(
            loaded.retention_policy().max_retained,
            Some(10),
            "retention conversion must preserve the limit"
        );
    }

    #[test]
    fn newer_policy_version_fails_closed() {
        let directory = temporary_directory();
        let policy = BackupPolicy {
            version: POLICY_VERSION + 1,
            ..BackupPolicy::default()
        };
        let error = policy.save(directory.path()).unwrap_err();
        assert!(error.to_string().contains("not supported"));
    }

    #[test]
    fn invalid_retention_values_are_rejected() {
        let unlimited_retained = BackupPolicy {
            max_retained: Some(0),
            ..BackupPolicy::default()
        };
        assert!(unlimited_retained.validate().is_err());
        let excessive_age = BackupPolicy {
            min_retention_days: Some(99_999),
            ..BackupPolicy::default()
        };
        assert!(excessive_age.validate().is_err());
        let tiny_budget = BackupPolicy {
            max_total_bytes: Some(10),
            ..BackupPolicy::default()
        };
        assert!(tiny_budget.validate().is_err());
    }

    #[test]
    fn daily_schedule_is_due_on_a_new_utc_day_only() {
        let policy = BackupPolicy {
            schedule: BackupSchedule::Daily,
            ..BackupPolicy::default()
        };
        let noon = 20_000 * DAY_MS + 12 * 3_600_000;
        assert!(policy.is_due(noon, None), "no backup yet must be due");
        assert!(
            !policy.is_due(noon, Some(noon - 3_600_000)),
            "same day not due"
        );
        assert!(policy.is_due(noon, Some(noon - DAY_MS)), "previous day due");
    }

    #[test]
    fn weekly_schedule_uses_seven_days() {
        let policy = BackupPolicy {
            schedule: BackupSchedule::Weekly,
            ..BackupPolicy::default()
        };
        let now = 20_000 * DAY_MS;
        assert!(!policy.is_due(now, Some(now - 6 * DAY_MS)));
        assert!(policy.is_due(now, Some(now - 7 * DAY_MS)));
        assert!(policy.is_due(now, Some(now - 8 * DAY_MS)));
    }

    #[test]
    fn before_upgrade_schedule_is_never_timer_due() {
        let policy = BackupPolicy {
            schedule: BackupSchedule::BeforeUpgrade,
            ..BackupPolicy::default()
        };
        assert!(!policy.is_due(1_700_000_000_000, None));
    }
}
