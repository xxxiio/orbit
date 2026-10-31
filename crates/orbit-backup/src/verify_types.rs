//! Public verification result types shared by the CLI, server, Web GUI and the
//! verification sidecar.
//!
//! A [`VerificationReport`] never implies a restore: it only states that the
//! archive is internally sound and compatible with this build.

use serde::{Deserialize, Serialize};

use crate::manifest::{BackupManifest, InventoryEntry};

/// Options controlling how much verification work to perform.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VerifyOptions {
    /// Run SQLite integrity and schema checks by extracting the snapshot to a
    /// temporary file. Disabling this produces a structural-only result.
    pub deep: bool,
}

impl Default for VerifyOptions {
    fn default() -> Self {
        Self { deep: true }
    }
}

/// Status of one verification check.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckStatus {
    Pass,
    Fail,
    Skipped,
}

impl CheckStatus {
    /// Whether the check passed or was not applicable.
    pub fn is_ok(self) -> bool {
        matches!(self, Self::Pass | Self::Skipped)
    }
}

/// One named verification check.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerificationCheck {
    /// Stable check identifier, e.g. `file_hashes`.
    pub name: String,
    pub status: CheckStatus,
    /// Short operator-readable detail.
    pub detail: String,
    /// Number of items checked, where meaningful.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checked: Option<u64>,
}

impl VerificationCheck {
    /// Build a passing check.
    pub fn pass(name: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            status: CheckStatus::Pass,
            detail: detail.into(),
            checked: None,
        }
    }

    /// Build a failing check.
    pub fn fail(name: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            status: CheckStatus::Fail,
            detail: detail.into(),
            checked: None,
        }
    }

    /// Build a skipped check.
    pub fn skipped(name: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            status: CheckStatus::Skipped,
            detail: detail.into(),
            checked: None,
        }
    }

    /// Attach an item count.
    pub fn with_checked(mut self, checked: u64) -> Self {
        self.checked = Some(checked);
        self
    }
}

/// One verification problem, grouped by check name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerificationProblem {
    /// Failed check identifier.
    pub check: String,
    /// What exactly is wrong.
    pub detail: String,
}

/// Full verification result. Serialized into the CLI JSON output and the
/// verification sidecar written beside a created archive.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerificationReport {
    /// True only when every required check passed.
    pub ok: bool,
    /// Archive format version declared by the manifest.
    pub format_version: u32,
    /// Snapshot schema version declared by the manifest.
    pub schema_version: Option<i64>,
    /// Archive file size in bytes.
    pub archive_bytes: u64,
    /// Per-check outcomes.
    pub checks: Vec<VerificationCheck>,
    /// Required checks that failed.
    pub problems: Vec<VerificationProblem>,
    /// Non-fatal observations an operator should know about.
    pub warnings: Vec<String>,
    /// Parsed manifest, when it could be read and validated.
    pub manifest: Option<BackupManifest>,
    /// Inventory entries verified against content, for reporting.
    pub verified_files: Vec<InventoryEntry>,
}

impl VerificationReport {
    /// An empty report marking a hard failure before checks could run.
    pub fn empty() -> Self {
        Self {
            ok: false,
            format_version: 0,
            schema_version: None,
            archive_bytes: 0,
            checks: Vec::new(),
            problems: Vec::new(),
            warnings: Vec::new(),
            manifest: None,
            verified_files: Vec::new(),
        }
    }

    /// Human one-line summary.
    pub fn summary(&self) -> String {
        if self.ok {
            format!(
                "verified: {} file(s), schema {}",
                self.checked_files(),
                self.schema_version
                    .map(|version| version.to_string())
                    .unwrap_or_else(|| "unknown".to_owned())
            )
        } else {
            format!("verification failed: {} problem(s)", self.problems.len())
        }
    }

    /// Number of content files checked.
    pub fn checked_files(&self) -> u64 {
        self.checks
            .iter()
            .find(|check| check.name == "file_hashes")
            .and_then(|check| check.checked)
            .unwrap_or(0)
    }
}

/// Sidecar recorded beside an archive after successful creation.
///
/// It records the verification performed at creation time. It is not part of
/// the portable archive and never substitutes for verifying a copy later.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerificationSidecar {
    /// Timestamp of the verification, Unix epoch milliseconds.
    pub verified_at_ms: i64,
    /// Whether the archive was verified successfully.
    pub ok: bool,
    /// Archive file size at verification time.
    pub archive_bytes: u64,
    /// Format version verified.
    pub format_version: u32,
    /// Number of files verified.
    pub verified_files: u64,
    /// Problems recorded at creation, normally empty.
    #[serde(default)]
    pub problems: Vec<VerificationProblem>,
}
