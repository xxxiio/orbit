//! Backup discovery: list known `.orbitbak` archives in a directory.
//!
//! Every entry is derived from the archive's own manifest, so backups copied in
//! from another machine or an external disk are listed correctly. Unreadable
//! files are reported as skipped instead of aborting the whole listing.

use std::ffi::OsStr;
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::{BackupError, Result};
use crate::inspect::{read_manifest_only, read_sidecar};
use crate::manifest::{BACKUP_EXTENSION, BackupSourceMode};

/// Whether an archive was verified at creation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerificationStatus {
    /// Created by this build and verified before publish (sidecar present).
    Verified,
    /// A verification sidecar exists and records a failure.
    Failed,
    /// No verification sidecar; the copy has not been verified by this tool.
    Unknown,
}

/// One listed backup.
#[derive(Debug, Clone, Serialize)]
pub struct BackupListEntry {
    /// Absolute or caller-relative archive path.
    pub path: String,
    /// File name used as a stable identifier in CLI/API responses.
    pub file_name: String,
    pub label: Option<String>,
    pub created_at_ms: i64,
    pub created_at: String,
    pub size_bytes: u64,
    pub format_version: u32,
    pub schema_version: i64,
    pub source_mode: BackupSourceMode,
    pub verification: VerificationStatus,
    /// Timestamp from the verification sidecar, when present.
    pub verification_checked_at_ms: Option<i64>,
    pub complete: bool,
}

/// One file skipped while listing.
#[derive(Debug, Clone, Serialize)]
pub struct SkippedBackup {
    pub path: String,
    pub reason: String,
}

/// Result of listing a backup directory.
#[derive(Debug, Clone, Serialize)]
pub struct BackupListing {
    /// Directory that was scanned.
    pub directory: String,
    /// Known backups, newest first.
    pub backups: Vec<BackupListEntry>,
    /// Files with the backup extension that could not be read as backups.
    pub skipped: Vec<SkippedBackup>,
}

/// List `.orbitbak` archives in `directory`, newest first.
pub fn list_backups(directory: &Path, limit: Option<usize>) -> Result<BackupListing> {
    fs::create_dir_all(directory)
        .map_err(|error| BackupError::io(format!("create {}", directory.display()), error))?;
    let entries = fs::read_dir(directory)
        .map_err(|error| BackupError::io(format!("read {}", directory.display()), error))?;
    let mut candidates = Vec::new();
    for entry in entries {
        let entry = entry
            .map_err(|error| BackupError::io(format!("read {}", directory.display()), error))?;
        let path = entry.path();
        if path.is_file() && path.extension() == Some(OsStr::new(BACKUP_EXTENSION)) {
            candidates.push(path);
        }
    }
    candidates.sort();

    let mut backups = Vec::new();
    let mut skipped = Vec::new();
    for path in candidates {
        match list_one(&path) {
            Ok(entry) => backups.push(entry),
            Err(error) => skipped.push(SkippedBackup {
                path: path.display().to_string(),
                reason: error.to_string(),
            }),
        }
    }
    // Newest first; the file name breaks ties so the order is deterministic.
    backups.sort_by(|left, right| {
        right
            .created_at_ms
            .cmp(&left.created_at_ms)
            .then_with(|| right.file_name.cmp(&left.file_name))
    });
    if let Some(limit) = limit {
        backups.truncate(limit);
    }
    Ok(BackupListing {
        directory: directory.display().to_string(),
        backups,
        skipped,
    })
}

fn list_one(path: &Path) -> Result<BackupListEntry> {
    let manifest = read_manifest_only(path)?;
    let size_bytes = fs::metadata(path)
        .map_err(|error| BackupError::io(format!("stat {}", path.display()), error))?
        .len();
    let sidecar = read_sidecar(path);
    let (verification, verification_checked_at_ms) = match &sidecar {
        Some(sidecar) if sidecar.ok => (VerificationStatus::Verified, Some(sidecar.verified_at_ms)),
        Some(sidecar) => (VerificationStatus::Failed, Some(sidecar.verified_at_ms)),
        None => (VerificationStatus::Unknown, None),
    };
    Ok(BackupListEntry {
        path: path.display().to_string(),
        file_name: path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.display().to_string()),
        label: manifest.label.clone(),
        created_at_ms: manifest.created_at_ms,
        created_at: manifest.created_at.clone(),
        size_bytes,
        format_version: manifest.format_version,
        schema_version: manifest.schema_version,
        source_mode: manifest.source.mode,
        verification,
        verification_checked_at_ms,
        complete: manifest.complete,
    })
}
