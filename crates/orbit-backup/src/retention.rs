//! Retention: bounded cleanup of locally created backups.
//!
//! Retention is only ever run after a new backup has been created and
//! verified. It never removes the newest verified recovery point, never
//! touches files that are not `.orbitbak` archives with a valid manifest, and
//! only operates on the configured local backup directory. A user's external
//! copies are out of scope by construction.
//!
//! Discovery delegates to [`crate::list::list_backups`], so "is this a backup?"
//! is decided by the archive's own manifest, never by a file name alone.

use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::error::{BackupError, Result};
use crate::list::{BackupListEntry, VerificationStatus, list_backups};
use crate::manifest::VERIFICATION_SIDECAR_SUFFIX;
use crate::util;

/// Which automatic backups are enabled.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, serde::Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum BackupSchedule {
    /// Automatic backups disabled.
    #[default]
    Disabled,
    /// At most one automatic backup per calendar day.
    Daily,
    /// At most one automatic backup per ISO week.
    Weekly,
    /// A recovery point before an application upgrade.
    BeforeUpgrade,
}

impl BackupSchedule {
    /// Stable lowercase name.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Disabled => "disabled",
            Self::Daily => "daily",
            Self::Weekly => "weekly",
            Self::BeforeUpgrade => "before_upgrade",
        }
    }
}

/// Retention rules applied after a verified backup creation.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct RetentionPolicy {
    /// Maximum number of local backups to keep, including the newest.
    pub max_retained: Option<usize>,
    /// Minimum age, in milliseconds, before a backup may be considered for
    /// removal.
    pub min_retention_ms: Option<i64>,
    /// Maximum total bytes of local backups. Oldest backups are removed first.
    pub max_total_bytes: Option<u64>,
}

impl RetentionPolicy {
    /// Whether any limit is configured.
    pub fn has_limits(&self) -> bool {
        self.max_retained.is_some()
            || self.min_retention_ms.is_some()
            || self.max_total_bytes.is_some()
    }
}

/// Result of applying retention.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RetentionOutcome {
    /// Removed archive paths (or removal candidates when `dry_run`).
    pub removed: Vec<String>,
    /// Archives retained.
    pub retained: usize,
    /// The protected newest verified recovery point, if any.
    pub protected: Option<String>,
    /// Total archive bytes before cleanup.
    pub bytes_before: u64,
    /// Total archive bytes after cleanup (projected when `dry_run`).
    pub bytes_after: u64,
    /// Why an archive was not removed, one line per retained archive.
    pub retained_reasons: Vec<String>,
}

/// Apply a retention policy to a local backup directory.
///
/// `protected` must name the archive created by the just-completed verified
/// backup. It is never removed. The newest verified archive is additionally
/// protected when no explicit archive is supplied.
pub fn apply_retention(
    directory: &Path,
    policy: &RetentionPolicy,
    now_ms: i64,
    protected: Option<&Path>,
    dry_run: bool,
) -> Result<RetentionOutcome> {
    let Some(canonical_directory) = canonical_directory(directory)? else {
        return Ok(empty_outcome());
    };
    let listing = list_backups(directory, None)?;
    // Oldest first is both the removal order and the order in which limits are
    // satisfied; never trust an adapter's ordering for a destructive decision.
    let mut entries = listing.backups;
    entries.sort_by(|left, right| {
        left.created_at_ms
            .cmp(&right.created_at_ms)
            .then_with(|| left.file_name.cmp(&right.file_name))
    });
    let protected_index = select_protected(&entries, protected, directory)?;
    let protected = protected_index.map(|index| entries[index].path.clone());
    if entries.is_empty() {
        return Ok(RetentionOutcome {
            protected: None,
            ..empty_outcome()
        });
    }

    let mut outcome = RetentionOutcome {
        removed: Vec::new(),
        retained: entries.len(),
        protected,
        bytes_before: entries.iter().map(|entry| entry.size_bytes).sum(),
        bytes_after: 0,
        retained_reasons: Vec::new(),
    };
    outcome.bytes_after = outcome.bytes_before;
    for (index, entry) in entries.iter().enumerate() {
        if protected_index == Some(index) {
            outcome.retained_reasons.push(format!(
                "{}: protected recovery point (never removed)",
                entry.file_name
            ));
            continue;
        }
        if is_young(entry, policy, now_ms) {
            outcome.retained_reasons.push(format!(
                "{}: younger than the minimum retention age",
                entry.file_name
            ));
            continue;
        }
        if !limit_exceeded(&outcome, policy) {
            if policy.has_limits() {
                outcome.retained_reasons.push(format!(
                    "{}: retained within the configured limits",
                    entry.file_name
                ));
            }
            continue;
        }
        remove_archive(directory, &canonical_directory, entry, dry_run)?;
        outcome.retained -= 1;
        outcome.bytes_after = outcome.bytes_after.saturating_sub(entry.size_bytes);
        outcome.removed.push(entry.path.clone());
    }
    Ok(outcome)
}

fn empty_outcome() -> RetentionOutcome {
    RetentionOutcome {
        removed: Vec::new(),
        retained: 0,
        protected: None,
        bytes_before: 0,
        bytes_after: 0,
        retained_reasons: Vec::new(),
    }
}

fn canonical_directory(directory: &Path) -> Result<Option<PathBuf>> {
    match fs::canonicalize(directory) {
        Ok(path) if path.is_dir() => Ok(Some(path)),
        Ok(_) => Err(BackupError::InvalidArgument(format!(
            "backup directory {} is not a directory",
            directory.display()
        ))),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(BackupError::io(
            format!("resolve backup directory {}", directory.display()),
            error,
        )),
    }
}

/// Choose the one archive retention must never remove.
///
/// An explicit path wins (it is the recovery point the caller just verified).
/// Otherwise the newest archive with a passing verification sidecar is
/// protected; when no sidecar exists the newest archive with a valid manifest
/// is protected instead, so the last known recovery point always survives.
fn select_protected(
    entries: &[BackupListEntry],
    explicit: Option<&Path>,
    directory: &Path,
) -> Result<Option<usize>> {
    if let Some(explicit) = explicit {
        return entries
            .iter()
            .position(|entry| same_archive(&entry.path, explicit))
            .map(Some)
            .ok_or_else(|| {
                BackupError::NotFound(format!(
                    "protected backup {} is not a valid backup in {}; refusing to run retention",
                    explicit.display(),
                    directory.display()
                ))
            });
    }
    // Entries are oldest first, so the last match is the newest verified one.
    let sidecar_verified = entries
        .iter()
        .rposition(|entry| entry.verification == VerificationStatus::Verified);
    Ok(sidecar_verified.or_else(|| entries.len().checked_sub(1)))
}

fn same_archive(listed: &str, explicit: &Path) -> bool {
    let listed_path = Path::new(listed);
    if listed_path == explicit {
        return true;
    }
    if listed_path.file_name() == explicit.file_name() {
        return true;
    }
    match (fs::canonicalize(listed_path), fs::canonicalize(explicit)) {
        (Ok(left), Ok(right)) => left == right,
        _ => false,
    }
}

fn is_young(entry: &BackupListEntry, policy: &RetentionPolicy, now_ms: i64) -> bool {
    let Some(minimum) = policy.min_retention_ms else {
        return false;
    };
    now_ms.saturating_sub(entry.created_at_ms) < minimum
}

fn limit_exceeded(outcome: &RetentionOutcome, policy: &RetentionPolicy) -> bool {
    let count_exceeded = policy
        .max_retained
        .map(|limit| outcome.retained > limit.max(1))
        .unwrap_or(false);
    let bytes_exceeded = policy
        .max_total_bytes
        .map(|limit| outcome.bytes_after > limit)
        .unwrap_or(false);
    count_exceeded || bytes_exceeded
}

/// Remove one validated archive and its verification sidecar(s).
///
/// Every filesystem check is repeated here: a destructive operation never
/// trusts a path just because it appeared in a listing.
fn remove_archive(
    directory: &Path,
    canonical_directory: &Path,
    entry: &BackupListEntry,
    dry_run: bool,
) -> Result<()> {
    let archive = Path::new(&entry.path);
    if !is_inside(directory, canonical_directory, archive) {
        return Err(BackupError::UnsafePath(format!(
            "refusing to remove {}: it is outside the configured backup directory",
            entry.path
        )));
    }
    if archive.extension().and_then(|extension| extension.to_str()) != Some("orbitbak") {
        return Err(BackupError::InvalidArgument(format!(
            "refusing to remove {}: not a .orbitbak archive",
            entry.path
        )));
    }
    let metadata = match fs::symlink_metadata(archive) {
        Ok(metadata) => metadata,
        // It disappeared after listing; there is nothing left to remove.
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(BackupError::io(format!("inspect {}", entry.path), error));
        }
    };
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(BackupError::UnsafePath(format!(
            "refusing to remove {}: not a regular archive file",
            entry.path
        )));
    }
    if dry_run {
        return Ok(());
    }
    match fs::remove_file(archive) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(BackupError::io(format!("remove {}", entry.path), error)),
    }
    for sidecar in sidecar_candidates(archive) {
        if is_inside(directory, canonical_directory, &sidecar)
            && fs::symlink_metadata(&sidecar)
                .is_ok_and(|metadata| metadata.is_file() && !metadata.file_type().is_symlink())
        {
            fs::remove_file(&sidecar).map_err(|error| {
                BackupError::io(format!("remove sidecar {}", sidecar.display()), error)
            })?;
        }
    }
    util::fsync_dir(directory)
}

fn is_inside(directory: &Path, canonical_directory: &Path, path: &Path) -> bool {
    let joined = if path.is_absolute() {
        path.to_path_buf()
    } else {
        directory.join(path)
    };
    match fs::canonicalize(&joined) {
        Ok(canonical) => canonical.parent() == Some(canonical_directory),
        // A file that disappeared between listing and removal cannot escape.
        Err(error) => error.kind() == std::io::ErrorKind::NotFound,
    }
}

/// Verification sidecar file names accepted for one archive.
///
/// The documented location is `<archive>.verify.json`; the second form (the
/// archive extension replaced) is accepted so a sidecar written by an older
/// adapter is still removed with its archive instead of being orphaned.
fn sidecar_candidates(archive: &Path) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(name) = archive.file_name().and_then(|name| name.to_str()) {
        candidates.push(archive.with_file_name(format!("{name}.{VERIFICATION_SIDECAR_SUFFIX}")));
    }
    candidates.push(archive.with_extension(VERIFICATION_SIDECAR_SUFFIX));
    candidates.sort();
    candidates.dedup();
    candidates
}

#[cfg(test)]
mod tests {
    use super::*;

    fn automatic_retention() -> RetentionPolicy {
        RetentionPolicy {
            max_retained: Some(2),
            min_retention_ms: None,
            max_total_bytes: None,
        }
    }

    #[test]
    fn policy_without_limits_never_removes_anything() {
        assert!(!RetentionPolicy::default().has_limits());
        assert!(automatic_retention().has_limits());
        assert_eq!(
            BackupSchedule::default(),
            BackupSchedule::Disabled,
            "automatic backups are opt-in"
        );
    }

    #[test]
    fn sidecar_candidates_cover_both_documented_forms() {
        let archive = Path::new("/backups/orbit-20231114T221320Z-manual.orbitbak");
        let candidates = sidecar_candidates(archive);
        assert!(candidates.contains(&PathBuf::from(
            "/backups/orbit-20231114T221320Z-manual.orbitbak.verify.json"
        )));
        assert!(candidates.contains(&PathBuf::from(
            "/backups/orbit-20231114T221320Z-manual.verify.json"
        )));
    }

    #[test]
    fn missing_directory_is_an_empty_outcome() {
        let directory = tempfile::tempdir().unwrap();
        let missing = directory.path().join("does-not-exist");
        let outcome = apply_retention(
            &missing,
            &automatic_retention(),
            1_700_000_000_000,
            None,
            false,
        )
        .unwrap();
        assert_eq!(outcome, empty_outcome());
        assert!(!missing.exists(), "retention must not create directories");
    }

    #[test]
    fn explicit_protected_archive_not_in_the_directory_fails_closed() {
        let directory = tempfile::tempdir().unwrap();
        let outcome = apply_retention(
            directory.path(),
            &automatic_retention(),
            1_700_000_000_000,
            Some(Path::new("/nonexistent/orbit.orbitbak")),
            false,
        );
        assert!(matches!(outcome, Err(BackupError::NotFound(_))));
    }

    #[test]
    fn limit_exceeded_uses_projected_values() {
        let outcome = RetentionOutcome {
            removed: Vec::new(),
            retained: 3,
            protected: None,
            bytes_before: 300,
            bytes_after: 300,
            retained_reasons: Vec::new(),
        };
        let mut policy = RetentionPolicy {
            max_retained: Some(2),
            ..RetentionPolicy::default()
        };
        assert!(limit_exceeded(&outcome, &policy));
        policy.max_retained = Some(4);
        assert!(!limit_exceeded(&outcome, &policy));
        let bytes_policy = RetentionPolicy {
            max_retained: None,
            min_retention_ms: None,
            max_total_bytes: Some(200),
        };
        assert!(limit_exceeded(&outcome, &bytes_policy));
        assert!(bytes_policy.has_limits());
    }
}
