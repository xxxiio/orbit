//! Deterministic archive assembly: inventory, manifest, ustar stream and the
//! atomic publish step.
//!
//! Everything here runs after the writer fence has been released. The archive
//! is written to a temporary sibling of the destination, verified in full, and
//! only then renamed into place, so a failed backup can never surface a partial
//! `.orbitbak` file.

use std::fs;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;

use sha2::{Digest, Sha256};

use crate::create::{
    BackupOptions, BackupOutcome, BackupProgress, BackupProgressSink, SnapshotStage,
    attachment_relative_path, default_archive_name, describe_source, file_len, unique_archive_path,
};
use crate::error::{BackupError, Result};
use crate::manifest::{
    ATTACHMENT_PREFIX, BACKUP_FORMAT_VERSION, BACKUP_MIN_READER_VERSION, BackupCounts,
    BackupExclusion, BackupManifest, BackupSourceMode, BackupTotals, CONFIG_ENTRY, DATABASE_ENTRY,
    InventoryEntry, InventoryKind, KNOWLEDGE_PREFIX, MANIFEST_ENTRY,
};
use crate::snapshot::SnapshotFacts;
use crate::tar_fmt::{BLOCK_SIZE, TarWriter};
use crate::util;
use crate::verify::{
    VerificationProblem, VerificationReport, VerificationSidecar, VerifyOptions, verify_archive,
};

pub(crate) fn assemble_and_publish(
    stage: &SnapshotStage,
    destination_dir: &Path,
    options: &BackupOptions,
    created_at_ms: i64,
    started: Instant,
    progress: &mut dyn BackupProgressSink,
) -> Result<BackupOutcome> {
    let config_bytes = config_snapshot_bytes(options)?;
    let inventory = build_inventory(stage, config_bytes.as_deref())?;
    let manifest = build_manifest(
        stage,
        options,
        created_at_ms,
        inventory,
        config_bytes.is_some(),
    );
    let base_name = default_archive_name(created_at_ms, options.label.as_deref());
    let temp_path = destination_dir.join(format!(".{base_name}.tmp-{}", util::unique_suffix()));

    progress.report(&BackupProgress::WritingArchive {
        content_bytes: manifest.totals.content_bytes,
    });
    let manifest = match write_archive(&temp_path, stage, manifest, config_bytes.as_deref()) {
        Ok(manifest) => manifest,
        Err(error) => {
            let _ = fs::remove_file(&temp_path);
            return Err(error);
        }
    };

    // Verify before publish: a failed archive never becomes visible.
    progress.report(&BackupProgress::VerifyingArchive);
    let verification = match verify_archive(&temp_path, &VerifyOptions::default()) {
        Ok(report) if report.ok => report,
        Ok(report) => {
            let _ = fs::remove_file(&temp_path);
            return Err(BackupError::Integrity(format!(
                "the newly written archive failed its own verification: {}",
                summarize_problems(&report.problems)
            )));
        }
        Err(error) => {
            let _ = fs::remove_file(&temp_path);
            return Err(error);
        }
    };

    let final_path = unique_archive_path(destination_dir, &base_name);
    if let Err(error) = util::atomic_rename(&temp_path, &final_path) {
        let _ = fs::remove_file(&temp_path);
        return Err(error);
    }
    let sidecar = write_sidecar(&final_path, &verification);
    progress.report(&BackupProgress::Published {
        path: final_path.clone(),
        archive_bytes: verification.archive_bytes,
    });
    Ok(BackupOutcome {
        archive: final_path,
        manifest,
        verification,
        elapsed_ms: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
        sidecar,
    })
}

fn config_snapshot_bytes(options: &BackupOptions) -> Result<Option<Vec<u8>>> {
    if !options.include_config {
        return Ok(None);
    }
    let Some(config) = &options.config else {
        return Ok(None);
    };
    let mut bytes = serde_json::to_vec_pretty(config).map_err(|error| {
        BackupError::Manifest(format!("serialize configuration snapshot: {error}"))
    })?;
    bytes.push(b'\n');
    Ok(Some(bytes))
}

fn build_inventory(
    stage: &SnapshotStage,
    config_bytes: Option<&[u8]>,
) -> Result<Vec<InventoryEntry>> {
    let mut inventory = Vec::new();
    let (size_bytes, sha256) = util::hash_file(&stage.database)?;
    inventory.push(InventoryEntry {
        path: DATABASE_ENTRY.to_owned(),
        kind: InventoryKind::Database,
        size_bytes,
        sha256,
    });
    for attachment in &stage.attachment_objects {
        let (size_bytes, sha256) = util::hash_file(&attachment.path)?;
        if size_bytes != attachment.size_bytes {
            return Err(BackupError::Integrity(format!(
                "staged attachment object {} changed size during the backup",
                attachment.object_hash
            )));
        }
        inventory.push(InventoryEntry {
            path: format!(
                "{ATTACHMENT_PREFIX}{}",
                attachment_relative_path(&attachment.object_hash)?
            ),
            kind: InventoryKind::AttachmentObject,
            size_bytes,
            sha256,
        });
    }
    for relative in &stage.knowledge_files {
        let staged = stage.knowledge_hashes.get(relative).ok_or_else(|| {
            BackupError::Internal(format!("staged note {relative} has no recorded digest"))
        })?;
        let actual = file_len(&stage.vault.join(relative))?;
        if actual != staged.size_bytes {
            return Err(BackupError::Integrity(format!(
                "staged note {relative} changed size during the backup"
            )));
        }
        inventory.push(InventoryEntry {
            path: format!("{KNOWLEDGE_PREFIX}{relative}"),
            kind: InventoryKind::KnowledgeNote,
            size_bytes: staged.size_bytes,
            sha256: staged.sha256.clone(),
        });
    }
    if let Some(bytes) = config_bytes {
        inventory.push(InventoryEntry {
            path: CONFIG_ENTRY.to_owned(),
            kind: InventoryKind::Configuration,
            size_bytes: bytes.len() as u64,
            sha256: util::hex_digest(Sha256::digest(bytes)),
        });
    }
    if !inventory
        .iter()
        .any(|entry| entry.kind == InventoryKind::Database)
    {
        return Err(BackupError::Internal(
            "the inventory has no database snapshot".to_owned(),
        ));
    }
    inventory.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(inventory)
}

fn build_manifest(
    stage: &SnapshotStage,
    options: &BackupOptions,
    created_at_ms: i64,
    inventory: Vec<InventoryEntry>,
    config_included: bool,
) -> BackupManifest {
    let totals = totals_from_inventory(&inventory);
    BackupManifest {
        format_version: BACKUP_FORMAT_VERSION,
        min_reader_version: BACKUP_MIN_READER_VERSION,
        orbit_version: crate::orbit_version().to_owned(),
        orbit_build: crate::orbit_build(),
        schema_version: stage.facts.schema_version,
        created_at_ms,
        created_at: util::format_rfc3339_utc(created_at_ms),
        label: options.label.clone(),
        complete: true,
        source: describe_source(
            options.source_mode,
            &stage.facts,
            options.source_mode != BackupSourceMode::Local,
        ),
        config: if config_included {
            options.config.clone()
        } else {
            None
        },
        exclusions: default_exclusions(options.source_mode, &stage.facts),
        inventory,
        directories: stage
            .knowledge_directories
            .iter()
            .map(|directory| format!("{KNOWLEDGE_PREFIX}{directory}"))
            .collect(),
        totals,
        counts: BackupCounts {
            knowledge_notes: stage.knowledge_files.len() as u64,
            knowledge_folders: stage.knowledge_directories.len() as u64,
            attachment_objects: stage.facts.attachment_objects.len() as u64,
            attachment_associations: stage
                .facts
                .table_counts
                .get("attachments")
                .copied()
                .unwrap_or(0),
            tables: stage.facts.table_counts.clone(),
        },
    }
}

fn totals_from_inventory(inventory: &[InventoryEntry]) -> BackupTotals {
    let mut totals = BackupTotals::default();
    for entry in inventory {
        match entry.kind {
            InventoryKind::Database => totals.database_bytes += entry.size_bytes,
            InventoryKind::KnowledgeNote => totals.knowledge_bytes += entry.size_bytes,
            InventoryKind::AttachmentObject => totals.attachment_bytes += entry.size_bytes,
            InventoryKind::Configuration => totals.config_bytes += entry.size_bytes,
        }
        totals.content_bytes += entry.size_bytes;
    }
    totals
}

/// Components that are deliberately outside every backup boundary.
fn default_exclusions(mode: BackupSourceMode, facts: &SnapshotFacts) -> Vec<BackupExclusion> {
    let mut exclusions = Vec::new();
    if mode == BackupSourceMode::Replica {
        exclusions.push(BackupExclusion {
            category: "client_pending_data".to_owned(),
            detail: format!(
                "{} unaccepted outbox row(s) on this client replica are outside an authority backup",
                facts.pending_outbox_rows
            ),
        });
    }
    if mode == BackupSourceMode::Server {
        // A server snapshot is the authority, but it cannot contain work that
        // a disconnected client has not submitted yet. Say so explicitly rather
        // than letting a restore look like a lossless recovery of every device.
        exclusions.push(BackupExclusion {
            category: "client_pending_data".to_owned(),
            detail: "unaccepted changes, pending outbox commands and staged attachment uploads \
                     held by disconnected connected-mode clients are outside a server backup; \
                     reconcile each client explicitly after a restore"
                .to_owned(),
        });
    }
    for (category, detail) in [
        (
            "external_agent_sessions",
            "external Herdr agent sessions and their working directories",
        ),
        (
            "operating_system_state",
            "running processes and operating-system state",
        ),
        (
            "external_repositories",
            "Git worktrees, branches and external repositories",
        ),
        (
            "external_credentials",
            "credentials held by external agents or providers",
        ),
        (
            "actions_executed_outside_orbit",
            "actions that already executed outside Orbit",
        ),
    ] {
        exclusions.push(BackupExclusion {
            category: category.to_owned(),
            detail: detail.to_owned(),
        });
    }
    exclusions
}

fn write_archive(
    path: &Path,
    stage: &SnapshotStage,
    mut manifest: BackupManifest,
    config_bytes: Option<&[u8]>,
) -> Result<BackupManifest> {
    manifest.validate()?;
    let manifest_bytes = stabilize_archive_bytes(&mut manifest)?;
    let file = fs::File::create(path)
        .map_err(|error| BackupError::io(format!("create {}", path.display()), error))?;
    let mut writer = TarWriter::new(BufWriter::new(file));
    writer.append_bytes(MANIFEST_ENTRY, &manifest_bytes)?;
    for entry in &manifest.inventory {
        append_inventory_entry(&mut writer, stage, entry, config_bytes)?;
    }
    writer.finish()?;
    let archived_bytes = writer.archived_bytes();
    drop(writer);
    if archived_bytes != manifest.totals.archive_bytes {
        return Err(BackupError::Internal(format!(
            "archive size {} does not match the declared {}",
            archived_bytes, manifest.totals.archive_bytes
        )));
    }
    util::fsync_file(path)?;
    Ok(manifest)
}

fn append_inventory_entry<W: Write>(
    writer: &mut TarWriter<W>,
    stage: &SnapshotStage,
    entry: &InventoryEntry,
    config_bytes: Option<&[u8]>,
) -> Result<()> {
    let staged_path = match entry.kind {
        InventoryKind::Database => stage.database.clone(),
        InventoryKind::KnowledgeNote => {
            let relative = entry.path.strip_prefix(KNOWLEDGE_PREFIX).ok_or_else(|| {
                BackupError::Internal(format!("knowledge entry outside prefix: {}", entry.path))
            })?;
            stage.vault.join(relative)
        }
        InventoryKind::AttachmentObject => {
            let relative = entry.path.strip_prefix(ATTACHMENT_PREFIX).ok_or_else(|| {
                BackupError::Internal(format!("attachment entry outside prefix: {}", entry.path))
            })?;
            stage.attachments.join(relative)
        }
        InventoryKind::Configuration => {
            let bytes = config_bytes.ok_or_else(|| {
                BackupError::Internal("configuration entry has no content".to_owned())
            })?;
            return writer.append_bytes(&entry.path, bytes);
        }
    };
    let actual = file_len(&staged_path)?;
    if actual != entry.size_bytes {
        return Err(BackupError::Integrity(format!(
            "staged content for {} changed size during assembly",
            entry.path
        )));
    }
    let mut file = fs::File::open(&staged_path)
        .map_err(|error| BackupError::io(format!("open {}", staged_path.display()), error))?;
    writer.append_file(&entry.path, entry.size_bytes, &mut file)
}

/// Resolve the exact archive size, which the manifest itself records.
///
/// The manifest is the first archive entry, so its own JSON length feeds back
/// into the total archive size. Iterating from zero converges in one or two
/// steps because only the decimal width of `archive_bytes` can change.
fn stabilize_archive_bytes(manifest: &mut BackupManifest) -> Result<Vec<u8>> {
    let content_entry_bytes: u64 = manifest
        .inventory
        .iter()
        .map(|entry| (BLOCK_SIZE as u64) + entry.size_bytes + padding_for(entry.size_bytes))
        .sum();
    let trailer_bytes = (BLOCK_SIZE * 2) as u64;
    let mut bytes = manifest.to_json()?;
    for _ in 0..12 {
        let total = content_entry_bytes
            + (BLOCK_SIZE as u64)
            + bytes.len() as u64
            + padding_for(bytes.len() as u64)
            + trailer_bytes;
        if manifest.totals.archive_bytes == total {
            return Ok(bytes);
        }
        manifest.totals.archive_bytes = total;
        bytes = manifest.to_json()?;
    }
    Err(BackupError::Internal(
        "the declared archive size did not stabilize".to_owned(),
    ))
}

fn padding_for(size: u64) -> u64 {
    let remainder = size % BLOCK_SIZE as u64;
    if remainder == 0 {
        0
    } else {
        BLOCK_SIZE as u64 - remainder
    }
}

fn write_sidecar(archive: &Path, report: &VerificationReport) -> Option<PathBuf> {
    let sidecar = VerificationSidecar {
        verified_at_ms: util::now_ms(),
        ok: report.ok,
        archive_bytes: report.archive_bytes,
        format_version: report.format_version,
        verified_files: report.checked_files(),
        problems: report.problems.clone(),
    };
    let bytes = serde_json::to_vec_pretty(&sidecar).ok()?;
    let path = crate::verify::sidecar_path(archive);
    fs::write(&path, bytes).ok()?;
    let _ = util::fsync_file(&path);
    Some(path)
}

fn summarize_problems(problems: &[VerificationProblem]) -> String {
    problems
        .iter()
        .take(3)
        .map(|problem| format!("{}: {}", problem.check, problem.detail))
        .collect::<Vec<_>>()
        .join("; ")
}
