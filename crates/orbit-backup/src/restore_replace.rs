//! Replacement-restore execution: preservation, journal, activation, rollback.
//!
//! Owned by the I2 restore implementation. Private support for `restore.rs`:
//! moves the existing workspace into a preserved directory by same-filesystem
//! rename, writes the recovery journal, activates the staged workspace, and
//! rolls back automatically when activation or post-activation verification
//! fails. The preserved copy is never deleted automatically.
//!
//! Order is deliberate: the previous workspace is preserved **before** the
//! staged database is activated, and the recovery journal is written **before**
//! activation, so a crash at any point leaves an operator-recoverable state.

use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::error::{BackupError, Result};
use crate::layout::WorkspaceLayout;
use crate::manifest::{BackupManifest, BackupSourceMode};
use crate::restore_extract::{
    STAGED_ATTACHMENTS, STAGED_DATABASE, STAGED_KNOWLEDGE, merge_attachment_objects,
};
use crate::snapshot;
use crate::util;

/// File name of the replacement recovery journal inside the preserved directory.
pub const JOURNAL_FILE: &str = "restore-journal.json";
/// Schema version of the recovery journal written by this build.
pub const JOURNAL_VERSION: u32 = 1;

/// The previous workspace state moved aside for rollback.
#[derive(Debug, Clone)]
pub struct PreservedWorkspace {
    /// Preserved directory under the workspace parent.
    pub directory: PathBuf,
    /// Preserved authoritative database, when one existed.
    pub database: Option<PathBuf>,
    /// Preserved `-wal` sidecar, when one existed.
    pub database_wal: Option<PathBuf>,
    /// Preserved `-shm` sidecar, when one existed.
    pub database_shm: Option<PathBuf>,
    /// Preserved knowledge vault directory, when one existed.
    pub knowledge_vault: Option<PathBuf>,
}

impl PreservedWorkspace {
    /// Every preserved path, for reporting.
    pub fn paths(&self) -> Vec<&Path> {
        [
            self.database.as_deref(),
            self.database_wal.as_deref(),
            self.database_shm.as_deref(),
            self.knowledge_vault.as_deref(),
        ]
        .into_iter()
        .flatten()
        .collect()
    }
}

/// Move the existing workspace components into a private preserved directory.
///
/// Same-filesystem renames are used, so this is cheap and atomic per component.
/// If any rename fails, every rename already performed is reversed and the
/// caller is told whether the previous workspace was restored, so a
/// preservation failure never leaves a half-moved active workspace silently.
pub fn preserve_existing_workspace(layout: &WorkspaceLayout) -> Result<PreservedWorkspace> {
    let directory = create_preserved_directory(layout.parent())?;
    let database_wal = snapshot::wal_path(layout.database());
    let database_shm = snapshot::shm_path(layout.database());
    let candidates: [(&Path, PreservedSlot); 4] = [
        (layout.database(), PreservedSlot::Database),
        (&database_wal, PreservedSlot::DatabaseWal),
        (&database_shm, PreservedSlot::DatabaseShm),
        (layout.knowledge_vault(), PreservedSlot::KnowledgeVault),
    ];
    let mut moved: Vec<(PathBuf, PathBuf)> = Vec::new();
    let mut preserved = PreservedWorkspace {
        directory: directory.clone(),
        database: None,
        database_wal: None,
        database_shm: None,
        knowledge_vault: None,
    };
    for (source, slot) in candidates {
        if !source.exists() {
            continue;
        }
        let target = directory.join(file_name(source)?);
        if let Err(error) = fs::rename(source, &target) {
            let original = BackupError::io(
                format!("preserve {} into {}", source.display(), directory.display()),
                error,
            );
            return Err(revert_preservation(&moved, &directory, original));
        }
        preserved.set(slot, target.clone());
        moved.push((source.to_path_buf(), target));
    }
    util::fsync_dir(&directory)?;
    util::fsync_dir(layout.parent())?;
    Ok(preserved)
}

#[derive(Clone, Copy)]
enum PreservedSlot {
    Database,
    DatabaseWal,
    DatabaseShm,
    KnowledgeVault,
}

impl PreservedWorkspace {
    fn set(&mut self, slot: PreservedSlot, target: PathBuf) {
        match slot {
            PreservedSlot::Database => self.database = Some(target),
            PreservedSlot::DatabaseWal => self.database_wal = Some(target),
            PreservedSlot::DatabaseShm => self.database_shm = Some(target),
            PreservedSlot::KnowledgeVault => self.knowledge_vault = Some(target),
        }
    }
}

/// Reverse a partially completed preservation and report the outcome honestly.
fn revert_preservation(
    moved: &[(PathBuf, PathBuf)],
    directory: &Path,
    original: BackupError,
) -> BackupError {
    let mut failures = Vec::new();
    for (source, target) in moved.iter().rev() {
        if let Err(error) = fs::rename(target, source) {
            failures.push(format!("{}: {error}", target.display()));
        }
    }
    if failures.is_empty() {
        let _ = fs::remove_dir(directory);
        BackupError::Integrity(format!(
            "{original}; the previous workspace was moved back, no changes remain"
        ))
    } else {
        BackupError::Integrity(format!(
            "{original}; PRESERVATION ROLLBACK FAILED for {}; recover manually from {}",
            failures.join(", "),
            directory.display()
        ))
    }
}

fn file_name(path: &Path) -> Result<PathBuf> {
    path.file_name()
        .map(PathBuf::from)
        .ok_or_else(|| BackupError::InvalidArgument(format!("{} has no file name", path.display())))
}

fn create_preserved_directory(parent: &Path) -> Result<PathBuf> {
    let stamp = util::format_compact_timestamp(util::now_ms());
    for attempt in 0..100_u32 {
        let name = if attempt == 0 {
            format!(".orbit-preserved-{stamp}")
        } else {
            format!(".orbit-preserved-{stamp}-{attempt}")
        };
        let path = parent.join(name);
        match fs::create_dir(&path) {
            Ok(()) => {
                set_private_permissions(&path)?;
                return Ok(path);
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(BackupError::io(
                    format!("create preserved directory {}", path.display()),
                    error,
                ));
            }
        }
    }
    Err(BackupError::Conflict(format!(
        "could not create a unique preserved directory under {}",
        parent.display()
    )))
}

fn set_private_permissions(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))
            .map_err(|error| BackupError::io(format!("chmod {}", path.display()), error))?;
    }
    Ok(())
}

/// Recorded preservation and activation paths in a recovery journal.
#[derive(Debug, Clone, Serialize)]
pub struct PreservedPaths {
    pub directory: String,
    pub database: Option<String>,
    pub database_wal: Option<String>,
    pub database_shm: Option<String>,
    pub knowledge_vault: Option<String>,
}

/// Recorded activation targets in a recovery journal.
#[derive(Debug, Clone, Serialize)]
pub struct ActivationTargets {
    pub database: String,
    pub knowledge_vault: String,
    pub attachment_store: String,
}

/// Portable recovery journal written inside the preserved directory.
///
/// It never contains credentials or file content: only paths, the archive
/// manifest hash and the exact rollback steps an operator can perform.
#[derive(Debug, Clone, Serialize)]
pub struct RestoreJournal {
    pub version: u32,
    pub created_at_ms: i64,
    pub created_at: String,
    pub archive: String,
    pub archive_manifest_sha256: String,
    pub source_mode: BackupSourceMode,
    pub workspace_id: Option<String>,
    pub source_revision: Option<i64>,
    pub restored_schema_version: i64,
    pub preserved: PreservedPaths,
    pub activation_targets: ActivationTargets,
    pub rollback_steps: Vec<String>,
    pub note: String,
}

/// Build the recovery journal for a replacement restore.
pub fn new_restore_journal(
    archive: &Path,
    manifest: &BackupManifest,
    manifest_sha256: &str,
    preserved: &PreservedWorkspace,
    layout: &WorkspaceLayout,
) -> RestoreJournal {
    let directory = preserved.directory.display().to_string();
    let database = layout.database().display().to_string();
    let database_wal = snapshot::wal_path(layout.database());
    let database_shm = snapshot::shm_path(layout.database());
    let knowledge_vault = layout.knowledge_vault().display().to_string();
    let mut rollback_steps = vec![
        "Stop every Orbit process using this workspace (CLI, server, agents).".to_owned(),
        format!(
            "Move the restored files aside: mv {database} {database}.restored-failed \
             (and the restored vault, if present)"
        ),
        format!(
            "Move the preserved database back: mv {directory}/{} {database}",
            display_file_name(layout.database(), "orbit.db")
        ),
    ];
    if preserved.database_wal.is_some() {
        rollback_steps.push(format!(
            "Restore the preserved WAL sidecar: mv {directory}/{} {}",
            display_file_name(&database_wal, "orbit.db-wal"),
            database_wal.display()
        ));
    }
    if preserved.database_shm.is_some() {
        rollback_steps.push(format!(
            "Restore the preserved shared-memory sidecar: mv {directory}/{} {}",
            display_file_name(&database_shm, "orbit.db-shm"),
            database_shm.display()
        ));
    }
    if preserved.knowledge_vault.is_some() {
        rollback_steps.push(format!(
            "Restore the preserved knowledge vault: mv {directory}/{} {knowledge_vault}",
            display_file_name(layout.knowledge_vault(), "orbit.knowledge")
        ));
    }
    rollback_steps.push(
        "Start Orbit again and verify the workspace before deleting the preserved copy.".to_owned(),
    );
    RestoreJournal {
        version: JOURNAL_VERSION,
        created_at_ms: util::now_ms(),
        created_at: util::format_rfc3339_utc(util::now_ms()),
        archive: archive.display().to_string(),
        archive_manifest_sha256: manifest_sha256.to_owned(),
        source_mode: manifest.source.mode,
        workspace_id: manifest.source.workspace_id.clone(),
        source_revision: manifest.source.revision,
        restored_schema_version: manifest.schema_version,
        preserved: PreservedPaths {
            directory,
            database: display_option(&preserved.database),
            database_wal: display_option(&preserved.database_wal),
            database_shm: display_option(&preserved.database_shm),
            knowledge_vault: display_option(&preserved.knowledge_vault),
        },
        activation_targets: ActivationTargets {
            database,
            knowledge_vault,
            attachment_store: layout.attachment_store().display().to_string(),
        },
        rollback_steps,
        note:
            "This preserved copy is the previous authoritative workspace. Orbit never deletes it \
               automatically. Keep it until the restored workspace has been verified in use."
                .to_owned(),
    }
}

fn display_file_name(path: &Path, fallback: &str) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| fallback.to_owned())
}

fn display_option(path: &Option<PathBuf>) -> Option<String> {
    path.as_ref().map(|path| path.display().to_string())
}

/// Write the recovery journal into the preserved directory and fsync it.
pub fn write_restore_journal(
    preserved: &PreservedWorkspace,
    journal: &RestoreJournal,
) -> Result<PathBuf> {
    let path = preserved.directory.join(JOURNAL_FILE);
    let mut bytes = serde_json::to_vec_pretty(journal)
        .map_err(|error| BackupError::Manifest(format!("serialize restore journal: {error}")))?;
    bytes.push(b'\n');
    fs::write(&path, &bytes)
        .map_err(|error| BackupError::io(format!("write {}", path.display()), error))?;
    util::fsync_file(&path)?;
    util::fsync_dir(&preserved.directory)?;
    Ok(path)
}

/// Activate a verified staging tree in place of a previously preserved workspace.
///
/// The database and vault are moved by rename; attachment objects are merged
/// (never deleting existing objects). The normalized configuration snapshot is
/// deliberately not applied to a live deployment: operator configuration is
/// global and stays under operator control. Returns the number of attachment
/// objects newly merged into the store.
pub fn activate_staged_workspace(staging: &Path, layout: &WorkspaceLayout) -> Result<u64> {
    let staged_database = staging.join(STAGED_DATABASE);
    if !staged_database.is_file() {
        return Err(BackupError::Integrity(format!(
            "staging tree has no database at {}",
            staged_database.display()
        )));
    }
    if layout.database().exists() || layout.knowledge_vault().exists() {
        return Err(BackupError::Conflict(format!(
            "cannot activate {}: the target database or vault still exists; \
             preserve the previous workspace first",
            layout.parent().display()
        )));
    }
    util::atomic_rename(&staged_database, layout.database())?;
    util::atomic_rename(&staging.join(STAGED_KNOWLEDGE), layout.knowledge_vault())?;
    let merged =
        merge_attachment_objects(&staging.join(STAGED_ATTACHMENTS), layout.attachment_store())?;
    util::fsync_dir(layout.parent())?;
    Ok(merged)
}

/// Automatic rollback after a failed activation or failed post-activation check.
///
/// Newly activated files are moved into `<preserved>/restored-failed/` (never
/// deleted) and the preserved previous workspace is moved back. Returns an
/// error naming the preserved directory and every path that could not be
/// restored, so the caller can tell the operator exactly what to do.
pub fn rollback_activation(layout: &WorkspaceLayout, preserved: &PreservedWorkspace) -> Result<()> {
    let failed = preserved.directory.join("restored-failed");
    fs::create_dir_all(&failed)
        .map_err(|error| BackupError::io(format!("create {}", failed.display()), error))?;
    let database_wal = snapshot::wal_path(layout.database());
    let database_shm = snapshot::shm_path(layout.database());
    let mut failures = Vec::new();
    for path in [
        layout.database().to_path_buf(),
        database_wal.clone(),
        database_shm.clone(),
        layout.knowledge_vault().to_path_buf(),
    ] {
        if !path.exists() {
            continue;
        }
        let target = match file_name(&path) {
            Ok(name) => failed.join(name),
            Err(error) => {
                failures.push(error.to_string());
                continue;
            }
        };
        if let Err(error) = fs::rename(&path, &target) {
            failures.push(format!("move restored {} aside: {error}", path.display()));
        }
    }
    let restores = [
        (preserved.database.clone(), layout.database().to_path_buf()),
        (preserved.database_wal.clone(), database_wal),
        (preserved.database_shm.clone(), database_shm),
        (
            preserved.knowledge_vault.clone(),
            layout.knowledge_vault().to_path_buf(),
        ),
    ];
    for (source, destination) in restores {
        let Some(source) = source else { continue };
        if !source.exists() {
            continue;
        }
        if let Err(error) = fs::rename(&source, &destination) {
            failures.push(format!(
                "{} -> {}: {error}",
                source.display(),
                destination.display()
            ));
        }
    }
    util::fsync_dir(layout.parent())?;
    if failures.is_empty() {
        Ok(())
    } else {
        Err(BackupError::Integrity(format!(
            "automatic rollback did not fully restore the previous workspace ({}); it is preserved \
             at {}",
            failures.join(", "),
            preserved.directory.display()
        )))
    }
}
