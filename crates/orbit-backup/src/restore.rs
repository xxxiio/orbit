//! Safe restore: preflight, isolated restore and maintenance replacement.
//!
//! Restore is more dangerous than backup. The protocol is:
//!
//! 1. **Preflight** verifies the archive, checks schema compatibility, free
//!    space, permissions, target activity and connected-client state, and
//!    produces the exact replacement/preservation plan. It mutates nothing.
//! 2. **Isolated restore** extracts and verifies into a new directory that is
//!    never the running workspace. This is the preferred recovery drill.
//! 3. **Replacement** requires an explicit confirmation token, stages and
//!    verifies before activation, moves the previous workspace into a preserved
//!    directory, and records a recovery journal. The preserved copy is never
//!    deleted automatically. The definitive active-workspace and pending-command
//!    checks take the workspace write fence in this apply path, never during
//!    preflight, so a dry run cannot even create SQLite lock sidecars.
//!
//! A connected client replica is never treated as an authority snapshot: an
//! authority backup cannot be restored over a replica, and a replica backup is
//! not accepted as an authoritative restore source without explicit handling.
//!
//! Implementation split: archive extraction and staging verification live in
//! [`crate::restore_extract`], target inspection and plan assembly in
//! [`crate::restore_preflight`], and preservation/journal/activation/rollback
//! in [`crate::restore_replace`]. This module owns the public types and the
//! orchestration between those steps.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::create::BackupProgressSink;
use crate::error::{BackupError, Result};
use crate::layout::WorkspaceLayout;
use crate::manifest::{BackupCounts, BackupExclusion, BackupSourceMode, BackupTotals};
use crate::restore_extract as extract;
use crate::restore_replace as replace_one;
use crate::snapshot;
use crate::util;
use crate::verify::VerificationReport;

/// Confirmation token required to replace an existing workspace.
pub const REPLACE_CONFIRMATION: &str = "REPLACE";

/// Which restore path is being taken.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RestoreMode {
    /// Restore into a new, non-active location.
    Isolated,
    /// Replace an existing workspace after a maintenance boundary.
    Replace,
}

/// The destination of a restore.
#[derive(Debug, Clone)]
pub enum RestoreTarget {
    /// Restore into `destination_dir`, creating a self-contained workspace.
    Isolated { destination_dir: PathBuf },
    /// Replace the workspace described by an existing layout.
    Replace { layout: WorkspaceLayout },
}

impl RestoreTarget {
    /// Mode implied by the target.
    pub fn mode(&self) -> RestoreMode {
        match self {
            Self::Isolated { .. } => RestoreMode::Isolated,
            Self::Replace { .. } => RestoreMode::Replace,
        }
    }
}

/// Options for preflight and restore.
#[derive(Debug, Clone)]
pub struct RestoreOptions {
    /// Explicit replacement confirmation. Required (and only accepted) for
    /// replacement; must be [`REPLACE_CONFIRMATION`] or the exact target
    /// database path.
    pub confirmation: Option<String>,
    /// Maximum time to wait when probing the target for active writers.
    ///
    /// The probe runs in the apply path only: preflight and dry runs never take
    /// the target's write fence and never create SQLite lock sidecars.
    pub target_probe_timeout: Duration,
    /// Run the archive verification as part of preflight (always true for a
    /// real restore; preflight may skip deep checks for speed when false).
    ///
    /// `false` still blocks on format, schema, source-mode, path and target
    /// safety problems; only the expensive integrity/hash extraction checks
    /// are skipped. Restore itself always extracts and re-verifies.
    pub deep_verification: bool,
    /// Resolved active workspace database of the current process, when known.
    ///
    /// Used to refuse an isolated destination that contains or equals the
    /// active workspace. Never used to probe or open that database.
    pub active_database: Option<PathBuf>,
}

impl Default for RestoreOptions {
    fn default() -> Self {
        Self {
            confirmation: None,
            target_probe_timeout: Duration::from_millis(250),
            deep_verification: true,
            active_database: None,
        }
    }
}

/// Human-readable description of a restore target.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RestoreTargetDescription {
    pub database: String,
    pub knowledge_vault: String,
    pub attachment_store: String,
    /// Whether the target already exists.
    pub exists: bool,
    /// Whether the target appears to be in active use.
    pub active: bool,
    /// Pending connected-client outbox rows found in the target, when known.
    pub pending_outbox_rows: u64,
}

/// One preflight blocker or warning.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RestoreProblem {
    /// Stable machine-readable code, e.g. `active_workspace`.
    pub code: String,
    /// Operator-readable explanation and next action.
    pub detail: String,
}

/// The exact effect a restore would have, rendered by `--dry-run` and the Web
/// GUI preflight panel.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RestorePlan {
    pub mode: RestoreMode,
    pub archive: String,
    pub archive_bytes: u64,
    pub label: Option<String>,
    pub created_at: String,
    pub backup_source_mode: BackupSourceMode,
    pub backup_schema_version: i64,
    pub supported_schema_version: i64,
    pub schema_compatible: bool,
    /// The current binary will migrate the restored schema on first open.
    pub schema_upgrade_required: bool,
    pub target: RestoreTargetDescription,
    /// Components that would be replaced (replacement mode).
    pub replaces: Vec<String>,
    /// Workspace state preserved for rollback (replacement mode).
    pub preserves: Vec<String>,
    pub downtime_required: bool,
    /// Connected-client reconciliation warnings.
    pub connected_client_warnings: Vec<String>,
    /// References that cannot be restored and remain external.
    pub external_dependencies: Vec<String>,
    /// Components the archive explicitly excludes.
    pub excluded_components: Vec<BackupExclusion>,
    pub totals: BackupTotals,
    pub counts: BackupCounts,
    /// Free space on the target filesystem, when it could be determined.
    ///
    /// This build reports `None`: no portable, dependency-free free-space
    /// measurement is available. Disk-full is caught during extraction and
    /// surfaces as a typed error without touching the active workspace; the
    /// preflight never claims a number it did not measure.
    pub free_space_bytes: Option<u64>,
}

/// Preflight outcome. `admissible` is false when any blocker is present.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RestorePreflight {
    pub admissible: bool,
    pub plan: RestorePlan,
    pub blockers: Vec<RestoreProblem>,
    pub warnings: Vec<RestoreProblem>,
    /// Verification report produced during preflight.
    pub verification: VerificationReport,
}

/// Successful restore.
#[derive(Debug)]
pub struct RestoreOutcome {
    /// Layout of the restored (and, for replacement, active) workspace.
    pub target: WorkspaceLayout,
    /// Preserved previous workspace directory, replacement mode only.
    pub preserved: Option<PathBuf>,
    /// Verification performed before activation.
    pub verification: VerificationReport,
    /// Whether the restored workspace was activated in place.
    pub activated: bool,
    /// Recovery journal path describing the replacement and rollback.
    pub journal: Option<PathBuf>,
    /// Recorded counts of the restored database.
    pub counts: BackupCounts,
}

/// Application-level restore service shared by the CLI and server adapters.
pub struct RestoreService;

impl RestoreService {
    /// Inspect the archive and target and produce the exact restore plan.
    ///
    /// Preflight never mutates the archive, the target or the workspace.
    pub fn preflight(
        archive: &Path,
        target: &RestoreTarget,
        options: &RestoreOptions,
    ) -> Result<RestorePreflight> {
        crate::restore_preflight::preflight(archive, target, options)
    }

    /// Execute a preflighted restore.
    ///
    /// Isolated restore creates the destination directory; replacement requires
    /// a passed preflight in the same call and an explicit confirmation token.
    /// On any failure before activation the active workspace is untouched; a
    /// failed replacement is rolled back from the preserved copy and reports
    /// where that copy is when automatic rollback cannot complete.
    pub fn restore(
        archive: &Path,
        target: RestoreTarget,
        options: &RestoreOptions,
        _progress: &mut dyn BackupProgressSink,
    ) -> Result<RestoreOutcome> {
        if let RestoreTarget::Replace { layout } = &target {
            check_replacement_confirmation(options.confirmation.as_deref(), layout)?;
        }
        let preflight = Self::preflight(archive, &target, options)?;
        if !preflight.admissible {
            return Err(refusal_error(&preflight));
        }
        let manifest = preflight.verification.manifest.clone().ok_or_else(|| {
            BackupError::Manifest(
                "the verified archive manifest disappeared before extraction".to_owned(),
            )
        })?;
        match target {
            RestoreTarget::Isolated { destination_dir } => {
                restore_isolated_workspace(archive, &destination_dir, &preflight, &manifest)
            }
            RestoreTarget::Replace { layout } => {
                replace_workspace(archive, &layout, &preflight, &manifest, options)
            }
        }
    }
}

fn check_replacement_confirmation(
    confirmation: Option<&str>,
    layout: &WorkspaceLayout,
) -> Result<()> {
    let database = layout.database().display().to_string();
    match confirmation {
        Some(token) if token == REPLACE_CONFIRMATION || token == database => Ok(()),
        Some(_) => Err(BackupError::RestoreRefused(format!(
            "confirmation did not match the target; replacement requires --confirm \
             {REPLACE_CONFIRMATION} or the exact target database path {database}. Nothing was changed."
        ))),
        None => Err(BackupError::RestoreRefused(format!(
            "replacement requires explicit confirmation: pass --confirm {REPLACE_CONFIRMATION} or \
             the exact target database path {database}. Nothing was changed."
        ))),
    }
}

fn refusal_error(preflight: &RestorePreflight) -> BackupError {
    let mut detail = "restore preflight refused the archive; nothing was changed.".to_owned();
    for blocker in &preflight.blockers {
        detail.push_str(&format!("\n  [{}] {}", blocker.code, blocker.detail));
    }
    BackupError::RestoreRefused(detail)
}

/// Extract into a private staging directory beside the destination, verify the
/// staged workspace, then atomically publish it as the new isolated workspace.
fn restore_isolated_workspace(
    archive: &Path,
    destination: &Path,
    preflight: &RestorePreflight,
    manifest: &crate::manifest::BackupManifest,
) -> Result<RestoreOutcome> {
    let parent = destination_parent(destination);
    std::fs::create_dir_all(&parent)
        .map_err(|error| BackupError::io(format!("create {}", parent.display()), error))?;
    ensure_isolated_destination_available(destination)?;
    let staging = util::TempDir::new(&parent, "orbit-restore")?;
    extract::extract_into(staging.path(), archive, manifest)?;
    let counts = extract::verify_staged_workspace(staging.path(), manifest)?;
    // Re-check immediately before publish: never replace anything that appeared
    // while the archive was being extracted.
    ensure_isolated_destination_available(destination)?;
    if destination.exists() {
        std::fs::remove_dir(destination).map_err(|error| {
            BackupError::io(
                format!("remove empty destination {}", destination.display()),
                error,
            )
        })?;
    }
    util::atomic_rename(staging.path(), destination)?;
    let layout = WorkspaceLayout::for_database(destination.join("orbit.db"))?;
    Ok(RestoreOutcome {
        target: layout,
        preserved: None,
        verification: preflight.verification.clone(),
        activated: true,
        journal: None,
        counts,
    })
}

/// Stage and verify, preserve the previous workspace, write the recovery
/// journal, activate, and verify after activation. Any failure rolls back.
fn replace_workspace(
    archive: &Path,
    layout: &WorkspaceLayout,
    preflight: &RestorePreflight,
    manifest: &crate::manifest::BackupManifest,
    options: &RestoreOptions,
) -> Result<RestoreOutcome> {
    // The definitive fail-closed check happens here, in the apply path: the
    // preflight/dry run never takes the workspace write lock. Refuse before
    // staging anything if the workspace is active or holds pending commands.
    recheck_target_is_quiet(layout, options)?;
    let staging = util::TempDir::new(layout.parent(), "orbit-restore")?;
    let extraction = extract::extract_into(staging.path(), archive, manifest)?;
    let counts = extract::verify_staged_workspace(staging.path(), manifest)?;
    // Re-check immediately before data is moved aside, closing the staging
    // window in which Orbit could have been started.
    recheck_target_is_quiet(layout, options)?;
    let preserved = replace_one::preserve_existing_workspace(layout)?;
    let journal = replace_one::new_restore_journal(
        archive,
        manifest,
        &extraction.manifest_sha256,
        &preserved,
        layout,
    );
    let journal_path = match replace_one::write_restore_journal(&preserved, &journal) {
        Ok(path) => path,
        Err(error) => return Err(journal_write_failure(layout, &preserved, error)),
    };
    if let Err(activation_error) = activate_and_verify(staging.path(), layout, manifest) {
        return Err(activation_failure(
            layout,
            &preserved,
            &journal_path,
            activation_error,
        ));
    }
    Ok(RestoreOutcome {
        target: layout.clone(),
        preserved: Some(preserved.directory),
        verification: preflight.verification.clone(),
        activated: true,
        journal: Some(journal_path),
        counts,
    })
}

fn activate_and_verify(
    staging: &Path,
    layout: &WorkspaceLayout,
    manifest: &crate::manifest::BackupManifest,
) -> Result<()> {
    replace_one::activate_staged_workspace(staging, layout)?;
    extract::verify_activated_workspace(layout, manifest)?;
    Ok(())
}

fn journal_write_failure(
    layout: &WorkspaceLayout,
    preserved: &replace_one::PreservedWorkspace,
    error: BackupError,
) -> BackupError {
    match replace_one::rollback_activation(layout, preserved) {
        Ok(()) => BackupError::Integrity(format!(
            "{error}; the previous workspace was moved back and remains authoritative"
        )),
        Err(rollback_error) => BackupError::Integrity(format!(
            "{error}; moving the previous workspace back ALSO failed ({rollback_error}); stop every \
             Orbit process and recover manually from {}",
            preserved.directory.display()
        )),
    }
}

fn activation_failure(
    layout: &WorkspaceLayout,
    preserved: &replace_one::PreservedWorkspace,
    journal_path: &Path,
    error: BackupError,
) -> BackupError {
    match replace_one::rollback_activation(layout, preserved) {
        Ok(()) => BackupError::Integrity(format!(
            "restore failed while activating the staged workspace ({error}); the previous workspace \
             was automatically restored from {}. Recovery journal: {}",
            preserved.directory.display(),
            journal_path.display()
        )),
        Err(rollback_error) => BackupError::Integrity(format!(
            "restore failed while activating the staged workspace ({error}) AND automatic rollback \
             failed ({rollback_error}); stop every Orbit process and recover manually from {} using {}",
            preserved.directory.display(),
            journal_path.display()
        )),
    }
}

/// Re-probe the live target immediately before data is moved aside.
fn recheck_target_is_quiet(layout: &WorkspaceLayout, options: &RestoreOptions) -> Result<()> {
    if !layout.database().is_file() {
        return Err(BackupError::RestoreRefused(format!(
            "the target workspace database {} disappeared while the archive was staged; \
             nothing was changed",
            layout.database().display()
        )));
    }
    let fence = snapshot::FencedConnection::acquire(
        layout.database(),
        options.target_probe_timeout,
    )
    .map_err(|error| match error {
        BackupError::WorkspaceBusy(detail) => BackupError::RestoreRefused(format!(
            "the target workspace became active while the archive was staged ({detail}); stop every \
             Orbit process and retry. Nothing was changed."
        )),
        other => other,
    })?;
    // Read pending commands while the fence is held, so a writer cannot add a
    // command between the probe and the preservation. Only unexported rows are
    // pending: exported outbox history is not a reason to refuse a replacement.
    let pending = pending_client_commands(layout.database());
    fence.release()?;
    let pending = pending?;
    if pending > 0 {
        return Err(BackupError::RestoreRefused(format!(
            "the target workspace holds {pending} pending connected-client command(s); replacing it \
             would lose them and pending commands are never replayed silently. Nothing was changed."
        )));
    }
    Ok(())
}

/// Count unexported connected-client outbox rows in a target database.
///
/// Called while a write fence is held in the apply path; failures are reported
/// rather than guessed so a replace cannot silently drop pending commands.
fn pending_client_commands(database: &Path) -> Result<u64> {
    let connection =
        rusqlite::Connection::open_with_flags(database, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .map_err(|error| {
                BackupError::Integrity(format!(
                    "open target workspace {} to count pending client commands: {error}",
                    database.display()
                ))
            })?;
    let exists: bool = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'sync_outbox')",
            [],
            |row| row.get(0),
        )
        .map_err(pending_read_error)?;
    if !exists {
        return Ok(0);
    }
    let count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM sync_outbox WHERE exported_at_ms IS NULL",
            [],
            |row| row.get(0),
        )
        .or_else(|_| connection.query_row("SELECT COUNT(*) FROM sync_outbox", [], |row| row.get(0)))
        .map_err(pending_read_error)?;
    Ok(count.max(0) as u64)
}

fn pending_read_error(error: rusqlite::Error) -> BackupError {
    BackupError::Integrity(format!("count pending connected-client commands: {error}"))
}

fn destination_parent(destination: &Path) -> PathBuf {
    match destination.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent.to_path_buf(),
        _ => PathBuf::from("."),
    }
}

fn ensure_isolated_destination_available(destination: &Path) -> Result<()> {
    if !destination.exists() {
        return Ok(());
    }
    if !destination.is_dir() {
        return Err(BackupError::Conflict(format!(
            "destination {} exists and is not a directory; refusing to overwrite it",
            destination.display()
        )));
    }
    let mut entries = std::fs::read_dir(destination)
        .map_err(|error| BackupError::io(format!("list {}", destination.display()), error))?;
    if entries.next().is_some() {
        return Err(BackupError::Conflict(format!(
            "destination {} is not empty; an isolated restore never overwrites existing files",
            destination.display()
        )));
    }
    Ok(())
}
