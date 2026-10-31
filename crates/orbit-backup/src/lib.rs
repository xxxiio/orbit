//! Shared workspace backup, verification and recovery services.
//!
//! Orbit stores one authoritative workspace across three resources: a SQLite
//! database, a Markdown knowledge vault and a content-addressed attachment
//! object store. This crate owns the portable `.orbitbak` format and the
//! protocol that turns those resources into one consistent, verifiable,
//! restorable artifact — and back again.
//!
//! Both the CLI and the self-hosted server call the same application-level
//! services here. Transports do not implement backup logic of their own.
//!
//! # Guarantees
//!
//! - Snapshot creation fences writers with SQLite's own locking for a bounded
//!   window; the database is copied with the online backup API, never with a
//!   filesystem copy of a live main file.
//! - Vault files and attachment objects are materialized during the same fence,
//!   so the archive describes one snapshot instant.
//! - Archives are written to a temporary path, verified, then atomically
//!   published. A failed backup never replaces a previous one and never
//!   mutates the workspace.
//! - Restores stage and verify before activation, preserve the previous
//!   workspace, and refuse unsafe targets.
//!
//! # What is not included
//!
//! External agent sessions, running processes, Git worktrees/branches, other
//! repositories, external credentials and unaccepted changes on disconnected
//! clients. The manifest lists these exclusions explicitly.

pub mod create;
pub mod error;
// Private support modules split out to keep each source file focused (I1):
// archive package assembly, staging materialization and verification support.
pub mod inspect;
pub mod layout;
pub mod list;
pub mod manifest;
mod package;
pub mod policy;
pub mod restore;
pub mod restore_extract;
pub mod restore_preflight;
pub mod restore_replace;
pub mod retention;
pub mod snapshot;
mod staging;
pub mod tar_fmt;
pub mod util;
pub mod verify;
mod verify_support;
mod verify_types;

pub use create::{
    BackupOptions, BackupOutcome, BackupProgress, BackupProgressSink, BackupService, NoProgress,
    SnapshotStage, stage_consistent_snapshot,
};
pub use error::{BackupError, Result};
pub use inspect::{BackupInspection, inspect_archive, read_manifest_only};
pub use layout::{WorkspaceDescription, WorkspaceLayout};
pub use list::{BackupListEntry, BackupListing, VerificationStatus, list_backups};
pub use manifest::{
    ATTACHMENT_PREFIX, BACKUP_EXTENSION, BACKUP_FORMAT_VERSION, BACKUP_MIN_READER_VERSION,
    BackupConfigSnapshot, BackupCounts, BackupExclusion, BackupManifest, BackupSource,
    BackupSourceMode, BackupTotals, CONFIG_ENTRY, DATABASE_ENTRY, InventoryEntry, InventoryKind,
    KNOWLEDGE_PREFIX, MANIFEST_ENTRY, VERIFICATION_SIDECAR_SUFFIX,
};
pub use policy::{BackupPolicy, POLICY_FILE_NAME, POLICY_VERSION};
pub use restore::{
    REPLACE_CONFIRMATION, RestoreMode, RestoreOptions, RestoreOutcome, RestorePlan,
    RestorePreflight, RestoreProblem, RestoreService, RestoreTarget, RestoreTargetDescription,
};
pub use retention::{BackupSchedule, RetentionOutcome, RetentionPolicy, apply_retention};
pub use verify::{
    CheckStatus, VerificationCheck, VerificationProblem, VerificationReport, VerificationSidecar,
    VerifyOptions, verify_archive,
};

/// SQLite schema version this build writes and understands.
pub fn supported_schema_version() -> i64 {
    orbit_storage::SUPPORTED_SCHEMA_VERSION
}

/// Human-readable Orbit version that produced or reads a backup.
pub fn orbit_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// Optional build identifier for provenance, supplied at compile time.
pub fn orbit_build() -> Option<String> {
    option_env!("ORBIT_BUILD_ID").map(str::to_owned)
}

/// Current wall-clock time in Unix epoch milliseconds.
pub fn now_ms() -> i64 {
    util::now_ms()
}
