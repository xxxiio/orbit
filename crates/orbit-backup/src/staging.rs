//! Fenced snapshot staging: the bounded pause during which the workspace is
//! materialized into a self-contained staging directory.
//!
//! The fence is a SQLite `BEGIN IMMEDIATE` lock. While it is held the database
//! is copied with the online backup API, referenced attachment objects are
//! hardlinked (or copied across devices) and the Markdown vault is copied and
//! hashed. Staging is immune to later workspace writes, so archive assembly and
//! verification happen after the fence is released.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::create::{
    BackupProgress, BackupProgressSink, SnapshotStage, StagedAttachment, StagedFile,
    attachment_relative_path, file_len,
};
use crate::error::{BackupError, Result};
use crate::layout::WorkspaceLayout;
use crate::manifest::DATABASE_ENTRY;
use crate::snapshot::{FencedConnection, SnapshotFacts};
use crate::util::{self, TempDir};

/// Materialize a consistent snapshot of the workspace into staging.
///
/// The returned stage is complete and self-contained; the live workspace may
/// resume writing immediately after this function returns.
pub fn stage_consistent_snapshot(
    layout: &WorkspaceLayout,
    fence_timeout: Duration,
    staging_parent: &Path,
    progress: &mut dyn BackupProgressSink,
) -> Result<SnapshotStage> {
    layout.require_database()?;
    util::create_private_dir(staging_parent)?;
    let root = TempDir::new(staging_parent, "orbit-backup-stage")?;
    let database = root.path().join(DATABASE_ENTRY);
    let vault = root.path().join("knowledge");
    let attachments = root.path().join("attachments").join("objects");

    progress.report(&BackupProgress::Fencing);
    let fence = FencedConnection::acquire(layout.database(), fence_timeout)?;
    let facts = fence.facts()?;
    fence.snapshot_database(&database)?;
    let database_bytes = file_len(&database)?;
    progress.report(&BackupProgress::DatabaseSnapshotted {
        bytes: database_bytes,
    });
    let attachment_objects = stage_attachments(layout, &facts, &attachments)?;
    progress.report(&BackupProgress::AttachmentsStaged {
        objects: attachment_objects.len() as u64,
    });
    let vault_stage = stage_vault(layout, &vault)?;
    progress.report(&BackupProgress::VaultStaged {
        files: vault_stage.files.len() as u64,
        directories: vault_stage.directories.len() as u64,
    });
    fence.release()?;
    progress.report(&BackupProgress::FenceReleased);
    Ok(SnapshotStage {
        root,
        database,
        vault,
        attachments,
        facts,
        knowledge_files: vault_stage.files,
        knowledge_directories: vault_stage.directories,
        knowledge_hashes: vault_stage.hashes,
        attachment_objects,
    })
}

struct VaultStage {
    files: Vec<String>,
    directories: Vec<String>,
    hashes: BTreeMap<String, StagedFile>,
}

fn stage_attachments(
    layout: &WorkspaceLayout,
    facts: &SnapshotFacts,
    destination: &Path,
) -> Result<Vec<StagedAttachment>> {
    let mut staged = Vec::with_capacity(facts.attachment_objects.len());
    let mut missing = Vec::new();
    for object in &facts.attachment_objects {
        let relative = attachment_relative_path(&object.object_hash)?;
        let source = layout.attachment_store().join(&relative);
        let target = destination.join(&relative);
        match materialize_object(&source, &target, object.size_bytes)? {
            true => staged.push(StagedAttachment {
                object_hash: object.object_hash.clone(),
                size_bytes: object.size_bytes,
                path: target,
            }),
            false => missing.push(object.object_hash.clone()),
        }
    }
    if !missing.is_empty() {
        return Err(BackupError::MissingAttachmentObjects(missing));
    }
    Ok(staged)
}

/// Hardlink one immutable object into staging, falling back to a copy when the
/// store and the staging directory are on different devices.
fn materialize_object(source: &Path, target: &Path, size_bytes: u64) -> Result<bool> {
    match fs::symlink_metadata(source) {
        Ok(metadata) if metadata.file_type().is_file() => {}
        Ok(_) => return Ok(false),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => {
            return Err(BackupError::io(
                format!("inspect attachment object {}", source.display()),
                error,
            ));
        }
    }
    util::ensure_parent(target)?;
    if fs::hard_link(source, target).is_err()
        && let Err(error) = fs::copy(source, target)
    {
        if error.kind() == std::io::ErrorKind::NotFound {
            return Ok(false);
        }
        return Err(BackupError::io(
            format!("copy attachment object {} into staging", source.display()),
            error,
        ));
    }
    let actual = file_len(target)?;
    if actual != size_bytes {
        return Err(BackupError::Integrity(format!(
            "attachment object {} is {actual} bytes in the store but the snapshot declares {size_bytes}",
            source.display()
        )));
    }
    Ok(true)
}

fn stage_vault(layout: &WorkspaceLayout, destination: &Path) -> Result<VaultStage> {
    let mut stage = VaultStage {
        files: Vec::new(),
        directories: Vec::new(),
        hashes: BTreeMap::new(),
    };
    let source_root = layout.knowledge_vault();
    if !source_root.is_dir() {
        return Ok(stage);
    }
    let mut pending = vec![PathBuf::new()];
    while let Some(relative_dir) = pending.pop() {
        let absolute_dir = source_root.join(&relative_dir);
        let mut names = match read_sorted_names(&absolute_dir)? {
            Some(names) => names,
            None => continue,
        };
        names.sort();
        for name in names {
            let relative = relative_dir.join(&name);
            let absolute_entry = source_root.join(&relative);
            let metadata = match fs::symlink_metadata(&absolute_entry) {
                Ok(metadata) => metadata,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(error) => {
                    return Err(BackupError::io(
                        format!("inspect vault entry {}", absolute_entry.display()),
                        error,
                    ));
                }
            };
            let file_type = metadata.file_type();
            // Symlinks are never followed or archived: a vault cannot make the
            // backup reader escape the workspace boundary.
            if file_type.is_symlink() {
                continue;
            }
            let archive_relative = util::relative_archive_path(source_root, &absolute_entry)?;
            if file_type.is_dir() {
                stage.directories.push(archive_relative);
                pending.push(relative);
            } else if file_type.is_file()
                && std::path::Path::new(&name)
                    .extension()
                    .is_some_and(|ext| ext == "md")
            {
                let target = destination.join(&relative);
                util::ensure_parent(&target)?;
                let staged = copy_note(&absolute_entry, &target)?;
                stage.files.push(archive_relative.clone());
                stage.hashes.insert(archive_relative, staged);
            }
        }
    }
    stage.files.sort();
    stage.directories.sort();
    Ok(stage)
}

/// Read the entries of a directory, or `None` when it vanished during scanning.
fn read_sorted_names(directory: &Path) -> Result<Option<Vec<std::ffi::OsString>>> {
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(BackupError::io(
                format!("read vault directory {}", directory.display()),
                error,
            ));
        }
    };
    let mut names = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|error| {
            BackupError::io(
                format!("read vault directory {}", directory.display()),
                error,
            )
        })?;
        names.push(entry.file_name());
    }
    Ok(Some(names))
}

fn copy_note(source: &Path, target: &Path) -> Result<StagedFile> {
    let mut reader = fs::File::open(source)
        .map_err(|error| BackupError::io(format!("open note {}", source.display()), error))?;
    let mut writer = fs::File::create(target).map_err(|error| {
        BackupError::io(format!("create staged note {}", target.display()), error)
    })?;
    let (size_bytes, sha256) = util::copy_and_hash(&mut reader, &mut writer)
        .map_err(|error| BackupError::io(format!("copy note {}", source.display()), error))?;
    Ok(StagedFile { size_bytes, sha256 })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_file(path: &Path, bytes: &[u8]) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    }

    #[test]
    fn materialize_object_falls_back_to_copy_when_hardlinking_fails() {
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("store").join("aa").join("object");
        write_file(&source, b"content-addressed bytes");
        // A pre-existing target makes `hard_link` fail deterministically, which
        // exercises the copy fallback used across devices and filesystems.
        let target = directory.path().join("staging").join("aa").join("object");
        write_file(&target, b"partial staging file");
        let staged = materialize_object(&source, &target, 23).unwrap();
        assert!(staged);
        assert_eq!(std::fs::read(&target).unwrap(), b"content-addressed bytes");
    }

    #[test]
    fn materialize_object_reports_missing_and_wrong_sized_objects() {
        let directory = tempfile::tempdir().unwrap();
        let missing = directory.path().join("missing");
        assert!(!materialize_object(&missing, &directory.path().join("out"), 3).unwrap());

        let source = directory.path().join("source");
        write_file(&source, b"1234");
        let error = materialize_object(&source, &directory.path().join("out"), 9).unwrap_err();
        assert!(matches!(error, BackupError::Integrity(_)), "{error}");
    }
}
