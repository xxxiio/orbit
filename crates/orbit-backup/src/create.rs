//! Backup creation: fenced snapshot staging, archive assembly and publish.
//!
//! Protocol (each step is mandatory; do not weaken it):
//!
//! 1. Acquire a SQLite `BEGIN IMMEDIATE` write fence on the authoritative
//!    database (bounded by [`BackupOptions::fence_timeout`]).
//! 2. Copy the database through SQLite's online backup API into staging.
//! 3. Enumerate referenced attachment objects and materialize them into staging
//!    with hardlinks where possible (immutable, content-addressed, O(1)).
//! 4. Copy and hash the Markdown vault (a bounded writer pause) and record every
//!    file and empty directory.
//! 5. Release the fence. From this point the staged copies are immune to
//!    concurrent workspace writes.
//! 6. Hash the staged database and attachment objects, build the manifest and
//!    stream the archive to a temporary sibling of the destination.
//! 7. Verify the assembled archive, fsync, then atomically rename it into
//!    place. Only then write the verification sidecar.
//! 8. Remove staging on success and on every failure path.
//!
//! The fenced staging machinery lives in [`crate::staging`]; deterministic
//! archive assembly lives in [`crate::package`]. This module owns the public
//! service/options/progress types and the publish naming rules.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::error::{BackupError, Result};
use crate::layout::WorkspaceLayout;
use crate::manifest::{
    BACKUP_EXTENSION, BackupConfigSnapshot, BackupManifest, BackupSource, BackupSourceMode,
};
use crate::package::assemble_and_publish;
use crate::snapshot::SnapshotFacts;
use crate::util::{self, TempDir};
use crate::verify::VerificationReport;

pub use crate::staging::stage_consistent_snapshot;

/// How [`BackupService::create`] should treat the workspace.
#[derive(Debug, Clone)]
pub struct BackupOptions {
    /// Authority mode of the workspace being backed up.
    pub source_mode: BackupSourceMode,
    /// Optional user label recorded in the manifest and file name.
    pub label: Option<String>,
    /// Include the normalized, non-secret configuration snapshot.
    pub include_config: bool,
    /// Caller-resolved non-secret configuration values.
    pub config: Option<BackupConfigSnapshot>,
    /// Maximum time to wait for the writer fence.
    pub fence_timeout: Duration,
    /// Keep the staging directory for diagnostics (tests only).
    pub keep_staging: bool,
}

impl Default for BackupOptions {
    fn default() -> Self {
        Self {
            source_mode: BackupSourceMode::Local,
            label: None,
            include_config: true,
            config: None,
            fence_timeout: Duration::from_secs(5),
            keep_staging: false,
        }
    }
}

/// Coarse progress reported during backup creation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BackupProgress {
    /// Waiting for or holding the writer fence.
    Fencing,
    /// Online database snapshot copied to staging.
    DatabaseSnapshotted { bytes: u64 },
    /// Markdown vault files and empty directories staged.
    VaultStaged { files: u64, directories: u64 },
    /// Referenced attachment objects staged.
    AttachmentsStaged { objects: u64 },
    /// Writer fence released; archive assembly may take as long as needed.
    FenceReleased,
    /// Writing the archive container.
    WritingArchive { content_bytes: u64 },
    /// Verifying the assembled archive before publish.
    VerifyingArchive,
    /// Archive atomically published at the final path.
    Published { path: PathBuf, archive_bytes: u64 },
}

/// Receiver for backup progress. Implemented by closures in CLI/UI adapters.
pub trait BackupProgressSink {
    /// Report one progress event. Implementations must not block for long.
    fn report(&mut self, progress: &BackupProgress);
}

/// A sink that ignores progress.
#[derive(Debug, Default)]
pub struct NoProgress;

impl BackupProgressSink for NoProgress {
    fn report(&mut self, _progress: &BackupProgress) {}
}

impl<F> BackupProgressSink for F
where
    F: FnMut(&BackupProgress),
{
    fn report(&mut self, progress: &BackupProgress) {
        self(progress)
    }
}

/// Size and SHA-256 of one staged file, recorded while it was copied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StagedFile {
    pub size_bytes: u64,
    pub sha256: String,
}

/// One attachment object materialized into staging.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StagedAttachment {
    /// Lowercase SHA-256 object identifier.
    pub object_hash: String,
    /// Declared size from the snapshot metadata.
    pub size_bytes: u64,
    /// Staged file path.
    pub path: PathBuf,
}

/// A consistent snapshot materialized in a staging directory.
///
/// Dropping this value removes the staging directory unless
/// [`SnapshotStage::keep_root`] has been called.
pub struct SnapshotStage {
    pub(crate) root: TempDir,
    /// Staged standalone SQLite snapshot.
    pub database: PathBuf,
    /// Staged vault directory (mirrors vault-relative paths).
    pub vault: PathBuf,
    /// Staged attachment object directory (mirrors `<aa>/<hash>`).
    pub attachments: PathBuf,
    /// Facts read while the fence was held.
    pub facts: SnapshotFacts,
    /// Vault file paths relative to the vault root, slash-separated.
    pub knowledge_files: Vec<String>,
    /// Vault directory paths relative to the vault root, including empty ones.
    pub knowledge_directories: Vec<String>,
    /// Sizes and digests recorded while each note was copied.
    pub knowledge_hashes: BTreeMap<String, StagedFile>,
    /// Materialized attachment objects.
    pub attachment_objects: Vec<StagedAttachment>,
}

impl SnapshotStage {
    /// Staging root directory.
    pub fn root(&self) -> &Path {
        self.root.path()
    }

    /// Keep the staging directory after this value is dropped.
    pub fn keep_root(self) -> PathBuf {
        self.root.keep()
    }
}

/// Successful backup creation.
#[derive(Debug)]
pub struct BackupOutcome {
    /// Published `.orbitbak` path.
    pub archive: PathBuf,
    /// Manifest stored inside the archive.
    pub manifest: BackupManifest,
    /// Verification of the assembled archive, run before publish.
    pub verification: VerificationReport,
    /// Total wall-clock duration in milliseconds.
    pub elapsed_ms: u64,
    /// Verification sidecar written next to the archive, when writable.
    pub sidecar: Option<PathBuf>,
}

/// Application-level backup service shared by the CLI, server and MCP adapters.
pub struct BackupService {
    layout: WorkspaceLayout,
}

impl BackupService {
    /// Bind the service to one authoritative workspace layout.
    pub fn new(layout: WorkspaceLayout) -> Self {
        Self { layout }
    }

    /// The workspace this service backs up.
    pub fn layout(&self) -> &WorkspaceLayout {
        &self.layout
    }

    /// Create one complete backup archive in `destination_dir`.
    ///
    /// The destination directory is created when missing. An existing file at
    /// the final backup name is never overwritten; a unique name is chosen.
    pub fn create(
        &self,
        destination_dir: &Path,
        options: BackupOptions,
        progress: &mut dyn BackupProgressSink,
    ) -> Result<BackupOutcome> {
        if options.source_mode == BackupSourceMode::Replica {
            // A connected client replica is not a restorable authority backup:
            // its schema, pending outbox and staged uploads differ, and an
            // older authority snapshot restored over newer client state is the
            // dangerous case M34 must fail closed on. Do not pretend.
            return Err(crate::error::BackupError::Unavailable(
                "connected client replica snapshots are not supported as a recovery point in \
                 this build; back up the server workspace on the server host instead \
                 (`orbit backup create --source-mode server`), and keep each replica's own \
                 pending outbox/staging directory with that replica"
                    .to_owned(),
            ));
        }
        let started = Instant::now();
        let options = normalized_options(options)?;
        prepare_destination(destination_dir)?;
        let created_at_ms = util::now_ms();
        let stage = stage_consistent_snapshot(
            &self.layout,
            options.fence_timeout,
            destination_dir,
            progress,
        )?;
        let result = assemble_and_publish(
            &stage,
            destination_dir,
            &options,
            created_at_ms,
            started,
            progress,
        );
        if options.keep_staging {
            let _ = stage.keep_root();
        }
        result
    }
}

fn normalized_options(mut options: BackupOptions) -> Result<BackupOptions> {
    if options.fence_timeout.is_zero() {
        return Err(BackupError::InvalidArgument(
            "the snapshot fence timeout must be greater than zero".to_owned(),
        ));
    }
    if !options.include_config && options.config.is_some() {
        return Err(BackupError::InvalidArgument(
            "configuration values were supplied but include_config is disabled".to_owned(),
        ));
    }
    options.label = options
        .label
        .map(|label| label.trim().to_owned())
        .filter(|label| !label.is_empty());
    if options
        .label
        .as_ref()
        .is_some_and(|label| label.chars().count() > 200)
    {
        return Err(BackupError::InvalidArgument(
            "a backup label must be at most 200 characters".to_owned(),
        ));
    }
    Ok(options)
}

fn prepare_destination(directory: &Path) -> Result<()> {
    if directory.exists() && !directory.is_dir() {
        return Err(BackupError::InvalidArgument(format!(
            "backup destination is not a directory: {}",
            directory.display()
        )));
    }
    fs::create_dir_all(directory)
        .map_err(|error| BackupError::io(format!("create {}", directory.display()), error))
}

pub(crate) fn file_len(path: &Path) -> Result<u64> {
    fs::metadata(path)
        .map(|metadata| metadata.len())
        .map_err(|error| BackupError::io(format!("stat {}", path.display()), error))
}

pub(crate) fn attachment_relative_path(object_hash: &str) -> Result<String> {
    let valid = object_hash.len() == 64 && object_hash.bytes().all(|byte| byte.is_ascii_hexdigit());
    if !valid {
        return Err(BackupError::Integrity(format!(
            "attachment object identifier is not a SHA-256 digest: {object_hash:?}"
        )));
    }
    Ok(format!("{}/{}", &object_hash[..2], object_hash))
}

/// Build the default archive file name for a manifest.
pub fn default_archive_name(created_at_ms: i64, label: Option<&str>) -> String {
    let stamp = crate::util::format_compact_timestamp(created_at_ms);
    let slug = label
        .map(crate::util::label_slug)
        .unwrap_or_else(|| "backup".to_owned());
    format!("orbit-{stamp}-{slug}.{BACKUP_EXTENSION}")
}

/// Choose a final archive path that never overwrites an existing file.
pub fn unique_archive_path(directory: &Path, base_name: &str) -> PathBuf {
    let candidate = directory.join(base_name);
    if !candidate.exists() {
        return candidate;
    }
    let suffix = format!(".{BACKUP_EXTENSION}");
    let stem = base_name.strip_suffix(&suffix).unwrap_or(base_name);
    for index in 2..=9_999_u32 {
        let candidate = directory.join(format!("{stem}-{index}{suffix}"));
        if !candidate.exists() {
            return candidate;
        }
    }
    directory.join(format!("{stem}-{}{suffix}", util::unique_suffix()))
}

/// Assemble the archive source description for a snapshot.
pub fn describe_source(
    mode: BackupSourceMode,
    facts: &SnapshotFacts,
    excluded_client_pending_data: bool,
) -> BackupSource {
    let described_as = match mode {
        BackupSourceMode::Local => "local authoritative workspace",
        BackupSourceMode::Server => "server-authoritative workspace",
        BackupSourceMode::Replica => "connected client replica",
    }
    .to_owned();
    BackupSource {
        mode,
        workspace_id: facts.workspace_id.clone(),
        revision: facts.revision,
        described_as,
        excluded_client_pending_data,
    }
}
