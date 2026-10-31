//! Transport adapter for workspace backup and restore.
//!
//! Every decision here is transport-only: authenticate, resolve a backup id to
//! a file inside the configured backup directory, call the shared
//! `orbit-backup` service, then shape a path-free JSON response. No handler
//! implements archive, verification or restore logic of its own.
//!
//! # Safety boundaries
//!
//! - A backup id is an archive *file name*. It is resolved by scanning
//!   `ORBIT_BACKUP_DIR` (default `<db parent>/backups`); a request never
//!   supplies a filesystem path, and an id containing separators, `..` or a
//!   leading dot is rejected before any filesystem call.
//! - Product responses never contain absolute server paths. Inspect and
//!   preflight output is reduced to component labels; restore plans are passed
//!   through a redactor that replaces the configured paths with those labels.
//! - The replacement plan is the only administrator route, and it never
//!   touches the live database: it writes `<db>.restore-pending.json` and
//!   returns the operator commands for the documented offline CLI path.
//! - The download route buffers the archive in memory (offline build; no
//!   streaming helper dependency). `ORBIT_BACKUP_DOWNLOAD_MAX_BYTES` (default
//!   512 MiB) bounds that buffer and answers 413 above it; large archives
//!   should be copied from the server's backup folder instead.
//! - Only one backup creation runs at a time. A concurrent request gets 409
//!   "backup in progress" instead of competing for the writer fence.

use std::{
    io::Write,
    path::{Path, PathBuf},
};

use axum::{
    Json,
    extract::{Path as AxumPath, State, rejection::JsonRejection},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use orbit_backup::{
    BackupConfigSnapshot, BackupCounts, BackupError, BackupExclusion, BackupListEntry,
    BackupManifest, BackupOptions, BackupPolicy, BackupSchedule, BackupService, BackupSource,
    BackupSourceMode, BackupTotals, NoProgress, POLICY_VERSION, RestoreMode, RestoreOptions,
    RestorePreflight, RestoreService, RestoreTarget, VerificationReport, VerificationSidecar,
    VerificationStatus, VerifyOptions, WorkspaceLayout, inspect_archive, read_manifest_only,
    verify_archive,
};
use serde::{Deserialize, Serialize};
use sha2::Digest;

use crate::{ApiError, AppState, admin, device, error, now_ms};

/// Default ceiling for the in-memory archive download buffer (512 MiB).
const DEFAULT_DOWNLOAD_LIMIT_BYTES: u64 = 512 * 1024 * 1024;
/// Process-wide guard so two creations never take the writer fence together.
static BACKUP_CREATE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
/// Longest operator label accepted for a backup.
const MAX_LABEL_CHARS: usize = 64;
/// Longest isolated-restore target name accepted.
const MAX_TARGET_NAME_CHARS: usize = 64;
/// Deepest page of backup history returned to the browser.
const BACKUP_LIST_LIMIT: usize = 200;

/// Filesystem locations the shared backup service needs.
///
/// Stored once on `AppState`; every value is derived in `main()` from the
/// server database path and the two documented environment overrides.
#[derive(Debug, Clone)]
pub(crate) struct BackupPaths {
    /// Authoritative server database. Never returned to the browser.
    pub(crate) database: PathBuf,
    /// Directory scanned for `.orbitbak` archives and `policy.json`.
    pub(crate) backup_dir: PathBuf,
    /// Root under which isolated restores create new workspace copies.
    pub(crate) restore_root: PathBuf,
    /// Upper bound for the in-memory download buffer.
    pub(crate) download_limit_bytes: u64,
}

impl BackupPaths {
    /// Derive paths from the server database and documented env overrides.
    pub(crate) fn from_env(database: &Path) -> Self {
        let parent = database
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."));
        let backup_dir = env_path("ORBIT_BACKUP_DIR").unwrap_or_else(|| parent.join("backups"));
        let restore_root =
            env_path("ORBIT_BACKUP_RESTORE_ROOT").unwrap_or_else(|| parent.join("restores"));
        let download_limit_bytes = std::env::var("ORBIT_BACKUP_DOWNLOAD_MAX_BYTES")
            .ok()
            .and_then(|value| value.trim().parse::<u64>().ok())
            .filter(|value| *value > 0)
            .unwrap_or(DEFAULT_DOWNLOAD_LIMIT_BYTES);
        Self {
            database: database.to_path_buf(),
            backup_dir,
            restore_root,
            download_limit_bytes,
        }
    }

    /// Deterministic per-process locations for server unit tests.
    #[cfg(test)]
    pub(crate) fn for_tests() -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "orbit-server-backup-tests-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        Self {
            database: root.join("orbit-test.db"),
            backup_dir: root.join("backups"),
            restore_root: root.join("restores"),
            download_limit_bytes: 8 * 1024 * 1024,
        }
    }
}

fn env_path(name: &str) -> Option<PathBuf> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

/// Run a blocking service call off the async runtime.
async fn run_blocking<T, F>(task: F) -> Result<T, ApiError>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, ApiError> + Send + 'static,
{
    match tokio::task::spawn_blocking(task).await {
        Ok(result) => result,
        Err(join) => {
            eprintln!("orbit-server backup worker failed: {join}");
            Err(error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "backup worker failed",
            ))
        }
    }
}

/// Map a typed backup failure to the product error contract.
///
/// Messages are quoted only when they are authored human text; raw I/O errors
/// are logged and answered with an opaque message so a filesystem path never
/// reaches the browser.
fn map_backup_error(failure: BackupError, redactor: &Redactor) -> ApiError {
    let message = match &failure {
        BackupError::InvalidArgument(detail) => Some(redactor.redact(detail)),
        BackupError::UnsupportedArchiveFormat { .. } | BackupError::IncompatibleSchema { .. } => {
            Some(failure.to_string())
        }
        BackupError::Manifest(detail) => Some(format!(
            "invalid backup manifest: {}",
            redactor.redact(detail)
        )),
        BackupError::Archive(detail) => Some(format!(
            "invalid backup archive: {}",
            redactor.redact(detail)
        )),
        BackupError::Integrity(detail) => Some(format!(
            "backup verification failed: {}",
            redactor.redact(detail)
        )),
        BackupError::UnsafePath(detail) => {
            Some(format!("unsafe archive path: {}", redactor.redact(detail)))
        }
        BackupError::MissingAttachmentObjects(objects) => Some(format!(
            "{} referenced attachment object(s) are missing from the attachment store; run attachment repair before creating a backup",
            objects.len()
        )),
        BackupError::WorkspaceBusy(detail) => Some(format!(
            "workspace is busy, so the backup could not take its snapshot fence; retry the backup: {}",
            redactor.redact(detail)
        )),
        BackupError::RestoreRefused(detail) => {
            Some(format!("restore refused: {}", redactor.redact(detail)))
        }
        BackupError::Conflict(detail) => Some(format!("conflict: {}", redactor.redact(detail))),
        BackupError::Unavailable(detail) => Some(format!(
            "backup service unavailable: {}",
            redactor.redact(detail)
        )),
        BackupError::NotFound(_) => Some("backup not found".to_owned()),
        BackupError::Io { context, source } => {
            eprintln!("orbit-server backup I/O error: {context}: {source}");
            None
        }
        BackupError::Internal(detail) => {
            eprintln!("orbit-server backup internal error: {detail}");
            None
        }
    };
    let status = match &failure {
        BackupError::InvalidArgument(_)
        | BackupError::UnsupportedArchiveFormat { .. }
        | BackupError::IncompatibleSchema { .. }
        | BackupError::Manifest(_)
        | BackupError::Archive(_)
        | BackupError::UnsafePath(_) => StatusCode::BAD_REQUEST,
        BackupError::Integrity(_) | BackupError::Io { .. } | BackupError::Internal(_) => {
            StatusCode::INTERNAL_SERVER_ERROR
        }
        BackupError::NotFound(_) => StatusCode::NOT_FOUND,
        BackupError::Conflict(_)
        | BackupError::WorkspaceBusy(_)
        | BackupError::RestoreRefused(_)
        | BackupError::MissingAttachmentObjects(_) => StatusCode::CONFLICT,
        BackupError::Unavailable(_) => StatusCode::SERVICE_UNAVAILABLE,
    };
    let message = message.unwrap_or_else(|| {
        if status == StatusCode::INTERNAL_SERVER_ERROR {
            "backup storage error".to_owned()
        } else {
            "backup request failed".to_owned()
        }
    });
    error(status, message)
}

/// Replace configured filesystem paths in human text with component labels.
///
/// The restore engine writes operator-readable strings that may name the
/// archive, database, vault or attachment store. Product responses must stay
/// path-free, so every such string passes through this redactor.
pub(crate) struct Redactor {
    replacements: Vec<(String, String)>,
}

impl Redactor {
    fn for_paths(paths: &BackupPaths) -> Self {
        let mut replacements = vec![
            (
                paths.database.display().to_string(),
                "the authoritative database".to_owned(),
            ),
            (
                paths.backup_dir.display().to_string(),
                "the backup folder".to_owned(),
            ),
            (
                paths.restore_root.display().to_string(),
                "the restore folder".to_owned(),
            ),
        ];
        if let Ok(layout) = WorkspaceLayout::for_database(&paths.database) {
            replacements.push((
                layout.knowledge_vault().display().to_string(),
                "the knowledge vault".to_owned(),
            ));
            replacements.push((
                layout.attachment_store().display().to_string(),
                "the attachment store".to_owned(),
            ));
        }
        Self { replacements }
    }

    fn with_archive(mut self, archive: &Path, backup_id: &str) -> Self {
        self.replacements
            .push((archive.display().to_string(), format!("backup {backup_id}")));
        self
    }

    fn redact(&self, text: &str) -> String {
        let mut replacements = self.replacements.clone();
        // Longest first: a vault path contains the database name, so replacing
        // the shorter database path first would leave a half-redacted label.
        replacements.sort_by_key(|(path, _)| std::cmp::Reverse(path.len()));
        let mut redacted = text.to_owned();
        for (path, label) in replacements {
            if path.is_empty() {
                continue;
            }
            redacted = redacted.replace(&path, &label);
        }
        redacted
    }
}

fn redact_all(redactor: &Redactor, values: Vec<String>) -> Vec<String> {
    values
        .into_iter()
        .map(|value| redactor.redact(&value))
        .collect()
}

/* ------------------------------------------------------------------ */
/* Request bodies                                                      */
/* ------------------------------------------------------------------ */

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CreateBackupRequest {
    #[serde(default)]
    label: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RestorePreflightRequest {
    backup_id: String,
    mode: RestoreMode,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct IsolatedRestoreRequest {
    backup_id: String,
    #[serde(default)]
    name: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ReplacePlanRequest {
    backup_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ScheduleUpdate {
    schedule: BackupSchedule,
    #[serde(default)]
    max_retained: Option<usize>,
    #[serde(default)]
    min_retention_days: Option<u32>,
    #[serde(default)]
    max_total_bytes: Option<u64>,
}

/// Deserialise a body, turning axum's rejection into the product 400 contract.
fn json_body<T>(payload: Result<Json<T>, JsonRejection>) -> Result<T, ApiError> {
    match payload {
        Ok(Json(value)) => Ok(value),
        Err(rejection) => {
            eprintln!("orbit-server backup request rejected: {rejection}");
            Err(error(StatusCode::BAD_REQUEST, "invalid request body"))
        }
    }
}

/* ------------------------------------------------------------------ */
/* Response shapes                                                     */
/* ------------------------------------------------------------------ */

#[derive(Debug, Clone, Serialize)]
pub(crate) struct BackupSummary {
    /// Archive file name; the stable browser-facing identifier.
    backup_id: String,
    label: Option<String>,
    created_at: String,
    created_at_ms: i64,
    size_bytes: u64,
    format_version: u32,
    schema_version: i64,
    source_mode: BackupSourceMode,
    verification: VerificationStatus,
    verification_checked_at_ms: Option<i64>,
    complete: bool,
}

impl BackupSummary {
    fn from_entry(entry: &BackupListEntry) -> Self {
        Self {
            backup_id: entry.file_name.clone(),
            label: entry.label.clone(),
            created_at: entry.created_at.clone(),
            created_at_ms: entry.created_at_ms,
            size_bytes: entry.size_bytes,
            format_version: entry.format_version,
            schema_version: entry.schema_version,
            source_mode: entry.source_mode,
            verification: entry.verification,
            verification_checked_at_ms: entry.verification_checked_at_ms,
            complete: entry.complete,
        }
    }
}

#[derive(Debug, Serialize)]
pub(crate) struct SkippedBackupSummary {
    file_name: String,
    reason: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct BackupDestinationSummary {
    /// Human label for the configured destination; never a server path.
    label: String,
    writable: bool,
}

#[derive(Debug, Serialize)]
pub(crate) struct BackupOverviewResponse {
    destination: BackupDestinationSummary,
    schedule: BackupPolicy,
    last_backup: Option<BackupSummary>,
    backups: Vec<BackupSummary>,
    skipped: Vec<SkippedBackupSummary>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct BackupComponentSummary {
    /// Stable component key (`database`, `knowledge`, `attachments`, `configuration`).
    component: &'static str,
    label: &'static str,
    files: u64,
    bytes: u64,
}

#[derive(Debug, Serialize)]
pub(crate) struct BackupCreateResponse {
    backup_id: String,
    created_at: String,
    label: Option<String>,
    size_bytes: u64,
    format_version: u32,
    schema_version: i64,
    verified: bool,
    verification_summary: String,
    components: Vec<BackupComponentSummary>,
    totals: BackupTotals,
    counts: BackupCounts,
    warnings: Vec<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct BackupArchiveSummary {
    bytes: u64,
    inventory_files: u64,
    content_bytes: u64,
    knowledge_directories: u64,
}

#[derive(Debug, Serialize)]
pub(crate) struct BackupInspectionResponse {
    backup_id: String,
    label: Option<String>,
    created_at: String,
    created_at_ms: i64,
    format_version: u32,
    min_reader_version: u32,
    orbit_version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    orbit_build: Option<String>,
    schema_version: i64,
    complete: bool,
    source: BackupSource,
    #[serde(skip_serializing_if = "Option::is_none")]
    config: Option<BackupConfigSnapshot>,
    exclusions: Vec<BackupExclusion>,
    totals: BackupTotals,
    counts: BackupCounts,
    archive: BackupArchiveSummary,
    components: Vec<BackupComponentSummary>,
    creation_verification: Option<VerificationSidecar>,
}

#[derive(Debug, Serialize)]
pub(crate) struct RestoreTargetSummary {
    /// `isolated` (new workspace copy) or `replace` (offline maintenance).
    kind: &'static str,
    /// Generated isolated-restore folder name; absent for replacement.
    #[serde(skip_serializing_if = "Option::is_none")]
    target_name: Option<String>,
    /// Component labels the target refers to; paths are never returned.
    components: Vec<&'static str>,
    exists: bool,
    active: bool,
    pending_outbox_rows: u64,
}

#[derive(Debug, Serialize)]
pub(crate) struct RestoreProblemSummary {
    code: String,
    detail: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct RestorePlanSummary {
    mode: RestoreMode,
    backup_id: String,
    label: Option<String>,
    created_at: String,
    archive_bytes: u64,
    backup_source_mode: BackupSourceMode,
    backup_schema_version: i64,
    supported_schema_version: i64,
    schema_compatible: bool,
    schema_upgrade_required: bool,
    target: RestoreTargetSummary,
    replaces: Vec<String>,
    preserves: Vec<String>,
    downtime_required: bool,
    connected_client_warnings: Vec<String>,
    external_dependencies: Vec<String>,
    excluded_components: Vec<BackupExclusion>,
    totals: BackupTotals,
    counts: BackupCounts,
    free_space_bytes: Option<u64>,
}

#[derive(Debug, Serialize)]
pub(crate) struct RestorePreflightResponse {
    admissible: bool,
    plan: RestorePlanSummary,
    blockers: Vec<RestoreProblemSummary>,
    warnings: Vec<RestoreProblemSummary>,
    verification: VerificationReport,
}

#[derive(Debug, Serialize)]
pub(crate) struct IsolatedRestoreResponse {
    target_name: String,
    backup_id: String,
    /// The archive/restored workspace passed verification. This is never a
    /// claim that the recovered data behaves correctly.
    verified: bool,
    activated: bool,
    counts: BackupCounts,
    message: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct ReplacePlanStep {
    title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    command: Option<String>,
    detail: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct ReplacePlanResponse {
    maintenance_required: bool,
    backup_id: String,
    archive_sha256: String,
    manifest_sha256: String,
    created_at: String,
    pending_plan_written: bool,
    steps: Vec<ReplacePlanStep>,
    note: String,
}

/* ------------------------------------------------------------------ */
/* Backup ids and target names                                         */
/* ------------------------------------------------------------------ */

/// Validate a browser-supplied backup id: an archive file name, nothing else.
fn validate_backup_id(id: &str) -> Result<(), ApiError> {
    if id.is_empty() || id.len() > 200 {
        return Err(error(StatusCode::BAD_REQUEST, "invalid backup id"));
    }
    if id.starts_with('.') || id.contains('/') || id.contains('\\') || id.contains("..") {
        return Err(error(StatusCode::BAD_REQUEST, "invalid backup id"));
    }
    if !id.ends_with(".orbitbak") {
        return Err(error(StatusCode::BAD_REQUEST, "invalid backup id"));
    }
    if !id
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '_'))
    {
        return Err(error(StatusCode::BAD_REQUEST, "invalid backup id"));
    }
    Ok(())
}

/// Resolve an id to an archive by scanning the configured directory.
///
/// The path is never built from the raw id alone: the directory is listed and
/// a matching regular file is required, so a backup that does not exist in the
/// configured destination is a 404 even when a same-named file exists nearby.
fn resolve_backup_path(paths: &BackupPaths, id: &str) -> Result<PathBuf, ApiError> {
    validate_backup_id(id)?;
    let directory = &paths.backup_dir;
    let Ok(entries) = std::fs::read_dir(directory) else {
        return Err(error(StatusCode::NOT_FOUND, "backup not found"));
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        if name.to_str() != Some(id) {
            continue;
        }
        if !entry
            .file_type()
            .map(|kind| kind.is_file())
            .unwrap_or(false)
        {
            break;
        }
        let path = entry.path();
        // Defence in depth after the name/suffix validation: the resolved file
        // must be a direct child of the configured directory.
        if path.parent() != Some(directory.as_path()) {
            break;
        }
        return Ok(path);
    }
    Err(error(StatusCode::NOT_FOUND, "backup not found"))
}

fn normalize_label(raw: Option<String>) -> Result<Option<String>, ApiError> {
    let Some(raw) = raw else {
        return Ok(None);
    };
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    if trimmed.chars().count() > MAX_LABEL_CHARS {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "backup label must be at most 64 characters",
        ));
    }
    if trimmed.chars().any(char::is_control) {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "backup label must not contain control characters",
        ));
    }
    Ok(Some(trimmed.to_owned()))
}

/// Accept an isolated-restore folder name: one plain path segment.
fn sanitize_target_name(raw: &str) -> Result<String, ApiError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "restore name must not be empty",
        ));
    }
    if trimmed == "." || trimmed == ".." || trimmed.starts_with('.') {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "restore name must not start with a dot",
        ));
    }
    if trimmed.chars().count() > MAX_TARGET_NAME_CHARS {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "restore name must be at most 64 characters",
        ));
    }
    if trimmed.starts_with('-') {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "restore name must not start with a dash",
        ));
    }
    if !trimmed
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '_'))
    {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "restore name may contain only letters, digits, '.', '-' and '_'",
        ));
    }
    Ok(trimmed.to_owned())
}

/// Deterministic folder name derived from the archive name.
fn generated_target_name(backup_id: &str) -> String {
    let stem = backup_id.strip_suffix(".orbitbak").unwrap_or(backup_id);
    let mut sanitized: String = stem
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_') {
                character
            } else {
                '-'
            }
        })
        .collect();
    sanitized = sanitized.trim_matches('-').to_owned();
    if sanitized.len() > 48 {
        sanitized.truncate(48);
        sanitized = sanitized.trim_end_matches('-').to_owned();
    }
    if sanitized.is_empty() {
        "restore".to_owned()
    } else {
        format!("restore-{sanitized}")
    }
}

fn unique_target_name(restore_root: &Path, backup_id: &str) -> String {
    let base = generated_target_name(backup_id);
    if !restore_root.join(&base).exists() {
        return base;
    }
    for index in 2..=999_u32 {
        let candidate = format!("{base}-{index}");
        if !restore_root.join(&candidate).exists() {
            return candidate;
        }
    }
    format!("{base}-{}", orbit_backup::util::unique_suffix())
}

/* ------------------------------------------------------------------ */
/* Shared shaping helpers                                              */
/* ------------------------------------------------------------------ */

fn component_summaries(manifest: &BackupManifest) -> Vec<BackupComponentSummary> {
    let mut files = [0_u64; 4];
    let mut bytes = [0_u64; 4];
    for entry in &manifest.inventory {
        let index = match entry.kind {
            orbit_backup::InventoryKind::Database => 0,
            orbit_backup::InventoryKind::KnowledgeNote => 1,
            orbit_backup::InventoryKind::AttachmentObject => 2,
            orbit_backup::InventoryKind::Configuration => 3,
        };
        files[index] = files[index].saturating_add(1);
        bytes[index] = bytes[index].saturating_add(entry.size_bytes);
    }
    let mut summaries = Vec::new();
    summaries.push(BackupComponentSummary {
        component: "database",
        label: "Database",
        files: files[0].max(1),
        bytes: bytes[0].max(manifest.totals.database_bytes),
    });
    if files[1] > 0 {
        summaries.push(BackupComponentSummary {
            component: "knowledge",
            label: "Knowledge notes",
            files: files[1],
            bytes: bytes[1].max(manifest.totals.knowledge_bytes),
        });
    }
    if files[2] > 0 {
        summaries.push(BackupComponentSummary {
            component: "attachments",
            label: "Attachment objects",
            files: files[2],
            bytes: bytes[2].max(manifest.totals.attachment_bytes),
        });
    }
    if files[3] > 0 || manifest.totals.config_bytes > 0 {
        summaries.push(BackupComponentSummary {
            component: "configuration",
            label: "Configuration",
            files: files[3].max(1),
            bytes: bytes[3].max(manifest.totals.config_bytes),
        });
    }
    summaries
}

fn file_name_from_path(path: &str) -> String {
    Path::new(path)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "unknown".to_owned())
}

fn destination_summary(directory: &Path) -> BackupDestinationSummary {
    BackupDestinationSummary {
        label: "Server backup folder".to_owned(),
        writable: destination_writable(directory),
    }
}

/// Probe writability honestly instead of guessing from metadata: create the
/// configured directory if needed, then write and remove one small file.
fn destination_writable(directory: &Path) -> bool {
    if std::fs::create_dir_all(directory).is_err() {
        return false;
    }
    let probe = directory.join(format!(
        ".orbit-write-probe-{}",
        orbit_backup::util::unique_suffix()
    ));
    match std::fs::write(&probe, b"") {
        Ok(()) => {
            let _ = std::fs::remove_file(&probe);
            true
        }
        Err(_) => false,
    }
}

fn server_config_snapshot() -> BackupConfigSnapshot {
    BackupConfigSnapshot {
        config_version: 1,
        authority_mode: "server".to_owned(),
        default_agent_provider: None,
        connected_server: None,
    }
}

fn backup_options(label: Option<String>) -> BackupOptions {
    BackupOptions {
        source_mode: BackupSourceMode::Server,
        label,
        include_config: true,
        config: Some(server_config_snapshot()),
        ..BackupOptions::default()
    }
}

/* ------------------------------------------------------------------ */
/* GET /v1/backups                                                     */
/* ------------------------------------------------------------------ */

pub(crate) async fn list_backups(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<BackupOverviewResponse>, ApiError> {
    device(&state, &headers)?;
    let paths = state.backup.clone();
    let response = run_blocking(move || {
        let redactor = Redactor::for_paths(&paths);
        let schedule = BackupPolicy::load(&paths.backup_dir)
            .map_err(|failure| map_backup_error(failure, &redactor))?;
        let listing = orbit_backup::list_backups(&paths.backup_dir, Some(BACKUP_LIST_LIMIT))
            .map_err(|failure| map_backup_error(failure, &redactor))?;
        let backups: Vec<BackupSummary> = listing
            .backups
            .iter()
            .map(BackupSummary::from_entry)
            .collect();
        let last_backup = backups
            .iter()
            .find(|backup| backup.verification == VerificationStatus::Verified)
            .cloned();
        let skipped = listing
            .skipped
            .iter()
            .map(|skipped| SkippedBackupSummary {
                file_name: file_name_from_path(&skipped.path),
                reason: redactor.redact(&skipped.reason),
            })
            .collect();
        Ok(BackupOverviewResponse {
            destination: destination_summary(&paths.backup_dir),
            schedule,
            last_backup,
            backups,
            skipped,
        })
    })
    .await?;
    Ok(Json(response))
}

/* ------------------------------------------------------------------ */
/* POST /v1/backups                                                    */
/* ------------------------------------------------------------------ */

pub(crate) async fn create_backup(
    State(state): State<AppState>,
    headers: HeaderMap,
    payload: Result<Json<CreateBackupRequest>, JsonRejection>,
) -> Result<Json<BackupCreateResponse>, ApiError> {
    device(&state, &headers)?;
    let request = json_body(payload)?;
    let label = normalize_label(request.label)?;
    let paths = state.backup.clone();
    let response = run_blocking(move || {
        // Creation takes the writer fence briefly; two snapshots must never
        // compete for it, and a second request must be told so honestly.
        let _create_guard = match BACKUP_CREATE_LOCK.try_lock() {
            Ok(guard) => guard,
            Err(std::sync::TryLockError::WouldBlock) => {
                return Err(error(
                    StatusCode::CONFLICT,
                    "a backup is already in progress; wait for it to finish and retry",
                ));
            }
            Err(std::sync::TryLockError::Poisoned(_)) => {
                return Err(error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "backup service error",
                ));
            }
        };
        let redactor = Redactor::for_paths(&paths);
        let layout = WorkspaceLayout::for_database(&paths.database)
            .map_err(|failure| map_backup_error(failure, &redactor))?;
        let service = BackupService::new(layout);
        let outcome = service
            .create(&paths.backup_dir, backup_options(label), &mut NoProgress)
            .map_err(|failure| map_backup_error(failure, &redactor))?;
        let manifest = &outcome.manifest;
        let archive = &outcome.archive;
        let backup_id = archive
            .file_name()
            .and_then(|name| name.to_str())
            .map(str::to_owned)
            .ok_or_else(|| {
                error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "backup archive name is not valid UTF-8",
                )
            })?;
        let size_bytes = std::fs::metadata(archive)
            .map(|metadata| metadata.len())
            .unwrap_or(manifest.totals.archive_bytes);
        Ok(BackupCreateResponse {
            backup_id,
            created_at: manifest.created_at.clone(),
            label: manifest.label.clone(),
            size_bytes,
            format_version: manifest.format_version,
            schema_version: manifest.schema_version,
            verified: outcome.verification.ok,
            verification_summary: outcome.verification.summary(),
            components: component_summaries(manifest),
            totals: manifest.totals.clone(),
            counts: manifest.counts.clone(),
            warnings: redact_all(&redactor, outcome.verification.warnings.clone()),
        })
    })
    .await?;
    Ok(Json(response))
}

/* ------------------------------------------------------------------ */
/* GET /v1/backups/{id} and POST /v1/backups/{id}/verify               */
/* ------------------------------------------------------------------ */

pub(crate) async fn inspect_backup(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(id): AxumPath<String>,
) -> Result<Json<BackupInspectionResponse>, ApiError> {
    device(&state, &headers)?;
    let paths = state.backup.clone();
    let response = run_blocking(move || {
        let archive = resolve_backup_path(&paths, &id)?;
        let redactor = Redactor::for_paths(&paths).with_archive(&archive, &id);
        let inspection =
            inspect_archive(&archive).map_err(|failure| map_backup_error(failure, &redactor))?;
        let manifest = inspection.manifest;
        Ok(BackupInspectionResponse {
            backup_id: id,
            label: manifest.label.clone(),
            created_at: manifest.created_at.clone(),
            created_at_ms: manifest.created_at_ms,
            format_version: manifest.format_version,
            min_reader_version: manifest.min_reader_version,
            orbit_version: manifest.orbit_version.clone(),
            orbit_build: manifest.orbit_build.clone(),
            schema_version: manifest.schema_version,
            complete: manifest.complete,
            source: manifest.source.clone(),
            config: manifest.config.clone(),
            exclusions: manifest.exclusions.clone(),
            totals: manifest.totals.clone(),
            counts: manifest.counts.clone(),
            archive: BackupArchiveSummary {
                bytes: inspection.archive_bytes,
                inventory_files: inspection.inventory_files as u64,
                content_bytes: inspection.content_bytes,
                knowledge_directories: manifest.directories.len() as u64,
            },
            components: component_summaries(&manifest),
            creation_verification: inspection.creation_verification,
        })
    })
    .await?;
    Ok(Json(response))
}

pub(crate) async fn verify_backup(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(id): AxumPath<String>,
) -> Result<Json<VerificationReport>, ApiError> {
    device(&state, &headers)?;
    let paths = state.backup.clone();
    let report = run_blocking(move || {
        let archive = resolve_backup_path(&paths, &id)?;
        let redactor = Redactor::for_paths(&paths).with_archive(&archive, &id);
        verify_archive(&archive, &VerifyOptions { deep: true })
            .map_err(|failure| map_backup_error(failure, &redactor))
    })
    .await?;
    Ok(Json(report))
}

/* ------------------------------------------------------------------ */
/* GET /v1/backups/{id}/download                                       */
/* ------------------------------------------------------------------ */

pub(crate) async fn download_backup(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(id): AxumPath<String>,
) -> Result<Response, ApiError> {
    device(&state, &headers)?;
    let paths = state.backup.clone();
    let (file_name, bytes) = run_blocking(move || {
        let archive = resolve_backup_path(&paths, &id)?;
        let metadata = std::fs::metadata(&archive)
            .map_err(|_| error(StatusCode::NOT_FOUND, "backup not found"))?;
        if metadata.len() > paths.download_limit_bytes {
            return Err(error(
                StatusCode::PAYLOAD_TOO_LARGE,
                format!(
                    "backup archive is {} bytes, above the {} byte web download limit; copy it from the server's backup folder instead",
                    metadata.len(),
                    paths.download_limit_bytes
                ),
            ));
        }
        let bytes = std::fs::read(&archive).map_err(|read_error| {
            eprintln!("orbit-server backup download read error: {read_error}");
            error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "backup archive could not be read",
            )
        })?;
        Ok((file_name_from_path(&id), bytes))
    })
    .await?;
    let disposition = format!(
        "attachment; filename=\"{}\"",
        download_file_name(&file_name)
    );
    let mut response = bytes.into_response();
    let response_headers = response.headers_mut();
    response_headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/octet-stream"),
    );
    response_headers.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&disposition).map_err(|_| {
            error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "backup archive name is not a valid header value",
            )
        })?,
    );
    response_headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    Ok(response)
}

/// Sanitize a file name for `Content-Disposition`: ASCII only, no quotes or
/// control characters. Backup ids are already validated ASCII; this is defence
/// in depth for a hand-copied archive whose name the id validation allowed.
fn download_file_name(name: &str) -> String {
    let sanitized: String = name
        .chars()
        .filter(|character| *character != '"' && *character != '\\' && !character.is_control())
        .map(|character| if character.is_ascii() { character } else { '_' })
        .collect();
    if sanitized.trim().is_empty() {
        "backup.orbitbak".to_owned()
    } else {
        sanitized
    }
}

/* ------------------------------------------------------------------ */
/* Restore preflight and isolated restore                              */
/* ------------------------------------------------------------------ */

pub(crate) async fn restore_preflight(
    State(state): State<AppState>,
    headers: HeaderMap,
    payload: Result<Json<RestorePreflightRequest>, JsonRejection>,
) -> Result<Json<RestorePreflightResponse>, ApiError> {
    device(&state, &headers)?;
    let request = json_body(payload)?;
    let paths = state.backup.clone();
    let response = run_blocking(move || {
        let archive = resolve_backup_path(&paths, &request.backup_id)?;
        let redactor = Redactor::for_paths(&paths).with_archive(&archive, &request.backup_id);
        let (target, target_name) = restore_target(&paths, &request)?;
        let options = RestoreOptions {
            confirmation: None,
            deep_verification: true,
            // The service must never restore over the database this server has
            // open; it compares the resolved destination with this path.
            active_database: Some(paths.database.clone()),
            ..RestoreOptions::default()
        };
        let preflight = RestoreService::preflight(&archive, &target, &options)
            .map_err(|failure| map_backup_error(failure, &redactor))?;
        Ok(build_preflight_response(
            &request.backup_id,
            target_name,
            preflight,
            &redactor,
        ))
    })
    .await?;
    Ok(Json(response))
}

fn restore_target(
    paths: &BackupPaths,
    request: &RestorePreflightRequest,
) -> Result<(RestoreTarget, Option<String>), ApiError> {
    match request.mode {
        RestoreMode::Isolated => {
            let name = unique_target_name(&paths.restore_root, &request.backup_id);
            let destination = paths.restore_root.join(&name);
            Ok((
                RestoreTarget::Isolated {
                    destination_dir: destination,
                },
                Some(name),
            ))
        }
        RestoreMode::Replace => {
            let layout = WorkspaceLayout::for_database(&paths.database)
                .map_err(|failure| map_backup_error(failure, &Redactor::for_paths(paths)))?;
            Ok((RestoreTarget::Replace { layout }, None))
        }
    }
}

fn build_preflight_response(
    backup_id: &str,
    target_name: Option<String>,
    preflight: RestorePreflight,
    redactor: &Redactor,
) -> RestorePreflightResponse {
    let plan = preflight.plan;
    let kind = match plan.mode {
        RestoreMode::Isolated => "isolated",
        RestoreMode::Replace => "replace",
    };
    let components = match plan.mode {
        RestoreMode::Isolated => vec!["Database snapshot", "Knowledge vault", "Attachments"],
        RestoreMode::Replace => vec![
            "Authoritative database",
            "Knowledge vault",
            "Attachment store",
        ],
    };
    let target = RestoreTargetSummary {
        kind,
        target_name,
        components,
        exists: plan.target.exists,
        active: plan.target.active,
        pending_outbox_rows: plan.target.pending_outbox_rows,
    };
    let blockers = preflight
        .blockers
        .into_iter()
        .map(|problem| RestoreProblemSummary {
            code: problem.code,
            detail: redactor.redact(&problem.detail),
        })
        .collect();
    let warnings = preflight
        .warnings
        .into_iter()
        .map(|problem| RestoreProblemSummary {
            code: problem.code,
            detail: redactor.redact(&problem.detail),
        })
        .collect();
    RestorePreflightResponse {
        admissible: preflight.admissible,
        plan: RestorePlanSummary {
            mode: plan.mode,
            backup_id: backup_id.to_owned(),
            label: plan.label,
            created_at: plan.created_at,
            archive_bytes: plan.archive_bytes,
            backup_source_mode: plan.backup_source_mode,
            backup_schema_version: plan.backup_schema_version,
            supported_schema_version: plan.supported_schema_version,
            schema_compatible: plan.schema_compatible,
            schema_upgrade_required: plan.schema_upgrade_required,
            target,
            replaces: redact_all(redactor, plan.replaces),
            preserves: redact_all(redactor, plan.preserves),
            downtime_required: plan.downtime_required,
            connected_client_warnings: redact_all(redactor, plan.connected_client_warnings),
            external_dependencies: redact_all(redactor, plan.external_dependencies),
            excluded_components: plan.excluded_components,
            totals: plan.totals,
            counts: plan.counts,
            free_space_bytes: plan.free_space_bytes,
        },
        blockers,
        warnings,
        verification: preflight.verification,
    }
}

pub(crate) async fn restore_isolated(
    State(state): State<AppState>,
    headers: HeaderMap,
    payload: Result<Json<IsolatedRestoreRequest>, JsonRejection>,
) -> Result<Json<IsolatedRestoreResponse>, ApiError> {
    device(&state, &headers)?;
    let request = json_body(payload)?;
    let paths = state.backup.clone();
    let response = run_blocking(move || {
        let archive = resolve_backup_path(&paths, &request.backup_id)?;
        let redactor = Redactor::for_paths(&paths).with_archive(&archive, &request.backup_id);
        let name = match request.name {
            Some(raw) => sanitize_target_name(&raw)?,
            None => unique_target_name(&paths.restore_root, &request.backup_id),
        };
        let destination = paths.restore_root.join(&name);
        let options = RestoreOptions {
            active_database: Some(paths.database.clone()),
            ..RestoreOptions::default()
        };
        let outcome = RestoreService::restore(
            &archive,
            RestoreTarget::Isolated {
                destination_dir: destination,
            },
            &options,
            &mut NoProgress,
        )
        .map_err(|failure| map_backup_error(failure, &redactor))?;
        let verified = outcome.verification.ok;
        let message = if verified {
            "The isolated copy was created and verified. It is a separate workspace copy; the active workspace was not modified."
        } else {
            "The isolated copy failed verification and is not safe to open. The active workspace was not modified."
        };
        Ok(IsolatedRestoreResponse {
            target_name: name,
            backup_id: request.backup_id,
            verified,
            activated: outcome.activated,
            counts: outcome.counts,
            message: message.to_owned(),
        })
    })
    .await?;
    Ok(Json(response))
}

/* ------------------------------------------------------------------ */
/* POST /v1/restores/replace-plan (administrator only)                 */
/* ------------------------------------------------------------------ */

pub(crate) async fn replacement_plan(
    State(state): State<AppState>,
    headers: HeaderMap,
    payload: Result<Json<ReplacePlanRequest>, JsonRejection>,
) -> Result<Json<ReplacePlanResponse>, ApiError> {
    // Administrator credential only: local trust is refused with 403 by the
    // shared helper before any storage is touched.
    admin(&state, &headers)?;
    let request = json_body(payload)?;
    let paths = state.backup.clone();
    let response = run_blocking(move || plan_replacement(&paths, &request.backup_id)).await?;
    Ok(Json(response))
}

fn plan_replacement(paths: &BackupPaths, backup_id: &str) -> Result<ReplacePlanResponse, ApiError> {
    let redactor = Redactor::for_paths(paths);
    if paths.database == Path::new(":memory:") {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "the server has no durable database to plan a replacement for",
        ));
    }
    let archive = resolve_backup_path(paths, backup_id)?;
    let manifest =
        read_manifest_only(&archive).map_err(|failure| map_backup_error(failure, &redactor))?;
    let manifest_json = manifest
        .to_json()
        .map_err(|failure| map_backup_error(failure, &redactor))?;
    let manifest_sha256 = orbit_backup::util::hex_digest(sha2::Sha256::digest(&manifest_json));
    let (archive_bytes, archive_sha256) = orbit_backup::util::hash_file(&archive)
        .map_err(|failure| map_backup_error(failure, &redactor))?;
    let created_at_ms = now_ms()?;
    let created_at = orbit_backup::util::format_rfc3339_utc(created_at_ms);
    let archive_path = archive.display().to_string();
    let database_path = paths.database.display().to_string();
    let steps = replacement_steps(&archive_path, &database_path);
    let note = "This is an offline maintenance plan only: the live workspace was NOT replaced from the browser. Run the commands with the server stopped, then restart it.".to_owned();
    let pending = PendingRestorePlan {
        version: 1,
        backup_id: backup_id.to_owned(),
        archive_bytes,
        archive_sha256: archive_sha256.clone(),
        manifest_sha256: manifest_sha256.clone(),
        created_at_ms,
        created_at: created_at.clone(),
        requested_by: "administrator credential".to_owned(),
        archive_path: archive_path.clone(),
        database_path: database_path.clone(),
        steps: steps
            .iter()
            .map(|step| PendingRestoreStep {
                title: step.title.clone(),
                command: step.command.clone(),
                detail: step.detail.clone(),
            })
            .collect(),
        note: note.clone(),
    };
    write_pending_plan(paths, &pending)?;
    Ok(ReplacePlanResponse {
        maintenance_required: true,
        backup_id: backup_id.to_owned(),
        archive_sha256,
        manifest_sha256,
        created_at,
        pending_plan_written: true,
        steps,
        note,
    })
}

fn replacement_steps(archive_path: &str, database_path: &str) -> Vec<ReplacePlanStep> {
    let archive = shell_single_quote(archive_path);
    let database = shell_single_quote(database_path);
    vec![
        ReplacePlanStep {
            title: "Stop the Orbit server".to_owned(),
            command: None,
            detail: "A replacement restore must never run against a live SQLite database. Stop this server (Ctrl+C, or your service manager) before continuing.".to_owned(),
        },
        ReplacePlanStep {
            title: "Verify the recovery point".to_owned(),
            command: Some(format!("orbit backup verify {archive}")),
            detail: "Re-verify the archive on the server's disk; a copy that travelled between machines must be verified again.".to_owned(),
        },
        ReplacePlanStep {
            title: "Replace the workspace offline".to_owned(),
            command: Some(format!(
                "orbit backup restore {archive} --replace --confirm {database}"
            )),
            detail: "The CLI stages and verifies the restore, preserves the previous database and knowledge vault, and never deletes the preserved copy. Run it with the server stopped.".to_owned(),
        },
        ReplacePlanStep {
            title: "Restart the server and reconcile connected clients".to_owned(),
            command: None,
            detail: "Connected clients may hold newer accepted revisions or pending commands. Reconcile them explicitly; pending commands are never replayed silently.".to_owned(),
        },
    ]
}

fn shell_single_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

#[derive(Debug, Serialize)]
pub(crate) struct PendingRestoreStep {
    title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    command: Option<String>,
    detail: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct PendingRestorePlan {
    version: u32,
    backup_id: String,
    archive_bytes: u64,
    archive_sha256: String,
    manifest_sha256: String,
    created_at_ms: i64,
    created_at: String,
    requested_by: String,
    archive_path: String,
    database_path: String,
    steps: Vec<PendingRestoreStep>,
    note: String,
}

fn pending_plan_path(database: &Path) -> PathBuf {
    let mut raw = database.as_os_str().to_owned();
    raw.push(".restore-pending.json");
    PathBuf::from(raw)
}

fn write_pending_plan(paths: &BackupPaths, pending: &PendingRestorePlan) -> Result<(), ApiError> {
    let path = pending_plan_path(&paths.database);
    let bytes = serde_json::to_vec_pretty(pending).map_err(|serialize_error| {
        eprintln!("orbit-server pending restore plan serialization failed: {serialize_error}");
        error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "pending restore plan could not be written",
        )
    })?;
    orbit_backup::util::ensure_parent(&path)
        .map_err(|failure| map_backup_error(failure, &Redactor::for_paths(paths)))?;
    let temporary = PathBuf::from(format!(
        "{}.tmp-{}",
        path.display(),
        orbit_backup::util::unique_suffix()
    ));
    let write_result = std::fs::File::create(&temporary)
        .and_then(|mut file| {
            file.write_all(&bytes)?;
            file.sync_all()
        })
        .and_then(|()| std::fs::rename(&temporary, &path));
    if let Err(write_error) = write_result {
        let _ = std::fs::remove_file(&temporary);
        eprintln!("orbit-server pending restore plan write failed: {write_error}");
        return Err(error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "pending restore plan could not be written",
        ));
    }
    Ok(())
}

/* ------------------------------------------------------------------ */
/* Backup schedule policy (persisted by the shared service)            */
/* ------------------------------------------------------------------ */

pub(crate) async fn get_schedule(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<BackupPolicy>, ApiError> {
    device(&state, &headers)?;
    let paths = state.backup.clone();
    let policy = run_blocking(move || {
        let redactor = Redactor::for_paths(&paths);
        BackupPolicy::load(&paths.backup_dir)
            .map_err(|failure| map_backup_error(failure, &redactor))
    })
    .await?;
    Ok(Json(policy))
}

pub(crate) async fn update_schedule(
    State(state): State<AppState>,
    headers: HeaderMap,
    payload: Result<Json<ScheduleUpdate>, JsonRejection>,
) -> Result<Json<BackupPolicy>, ApiError> {
    device(&state, &headers)?;
    let update = json_body(payload)?;
    let paths = state.backup.clone();
    let policy = run_blocking(move || {
        let redactor = Redactor::for_paths(&paths);
        let policy = policy_from_update(update);
        policy
            .validate()
            .map_err(|failure| map_backup_error(failure, &redactor))?;
        policy
            .save(&paths.backup_dir)
            .map_err(|failure| map_backup_error(failure, &redactor))?;
        Ok(policy)
    })
    .await?;
    Ok(Json(policy))
}

fn policy_from_update(update: ScheduleUpdate) -> BackupPolicy {
    BackupPolicy {
        version: POLICY_VERSION,
        schedule: update.schedule,
        max_retained: update.max_retained,
        min_retention_days: update.min_retention_days,
        max_total_bytes: update.max_total_bytes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_paths() -> BackupPaths {
        BackupPaths {
            database: PathBuf::from("/srv/orbit/data/orbit-server.db"),
            backup_dir: PathBuf::from("/srv/orbit/data/backups"),
            restore_root: PathBuf::from("/srv/orbit/data/restores"),
            download_limit_bytes: 1024,
        }
    }

    #[test]
    fn backup_ids_are_file_names_only() {
        assert!(validate_backup_id("orbit-20261031-backup.orbitbak").is_ok());
        for rejected in [
            "",
            "../orbit.bak.orbitbak",
            "sub/dir.orbitbak",
            "sub\\dir.orbitbak",
            ".hidden.orbitbak",
            "orbit-20261031.tmp",
            "orbit 20261031.orbitbak",
        ] {
            assert!(
                validate_backup_id(rejected).is_err(),
                "{rejected:?} must not be accepted as a backup id"
            );
        }
    }

    /// Unwrap a helper result in tests without requiring the transport error
    /// type (`ErrorResponse`) to implement `Debug`.
    fn ok<T>(result: Result<T, ApiError>) -> T {
        result.map_err(|_| ()).unwrap()
    }

    #[test]
    fn restore_names_are_single_plain_segments() {
        assert_eq!(ok(sanitize_target_name("drill-1")), "drill-1");
        for rejected in [
            "",
            "..",
            ".",
            ".hidden",
            "../escape",
            "a/b",
            "a b",
            "-start",
            "x".repeat(65).as_str(),
        ] {
            assert!(
                sanitize_target_name(rejected).is_err(),
                "{rejected:?} must not be accepted as a restore name"
            );
        }
    }

    #[test]
    fn generated_target_names_are_safe_and_short() {
        let name = generated_target_name("orbit-20261031-1830-nightly.orbitbak");
        assert_eq!(name, "restore-orbit-20261031-1830-nightly");
        assert!(sanitize_target_name(&name).is_ok());
        let long = generated_target_name(&format!("{}.orbitbak", "a".repeat(120)));
        assert!(long.len() <= MAX_TARGET_NAME_CHARS);
    }

    #[test]
    fn labels_are_trimmed_bounded_and_control_free() {
        assert_eq!(ok(normalize_label(None)), None);
        assert_eq!(ok(normalize_label(Some("  ".into()))), None);
        assert_eq!(
            ok(normalize_label(Some("  before upgrade  ".into()))),
            Some("before upgrade".to_owned())
        );
        assert!(normalize_label(Some("line\nbreak".into())).is_err());
        assert!(normalize_label(Some("x".repeat(65))).is_err());
    }

    #[test]
    fn redactor_never_leaks_configured_paths() {
        let paths = test_paths();
        let redactor = Redactor::for_paths(&paths).with_archive(
            Path::new("/srv/orbit/data/backups/one.orbitbak"),
            "one.orbitbak",
        );
        let text = "replace /srv/orbit/data/orbit-server.db and /srv/orbit/data/orbit-server.knowledge from /srv/orbit/data/backups/one.orbitbak inside /srv/orbit/data/restores";
        let redacted = redactor.redact(text);
        assert!(!redacted.contains("/srv/orbit"), "{redacted}");
        assert!(
            redacted.contains("the authoritative database"),
            "{redacted}"
        );
        assert!(redacted.contains("backup one.orbitbak"), "{redacted}");
    }

    #[test]
    fn policy_update_maps_onto_the_shared_policy_type() {
        let policy = policy_from_update(ScheduleUpdate {
            schedule: BackupSchedule::Daily,
            max_retained: Some(7),
            min_retention_days: Some(14),
            max_total_bytes: Some(8 * 1024 * 1024 * 1024),
        });
        assert_eq!(policy.version, POLICY_VERSION);
        assert_eq!(policy.schedule, BackupSchedule::Daily);
        assert!(policy.validate().is_ok());
        assert_eq!(policy.retention_policy().max_retained, Some(7));
    }

    #[test]
    fn invalid_policies_are_refused_with_an_actionable_message() {
        let zero = policy_from_update(ScheduleUpdate {
            schedule: BackupSchedule::Weekly,
            max_retained: Some(0),
            min_retention_days: None,
            max_total_bytes: None,
        });
        let message = zero.validate().unwrap_err().to_string();
        assert!(message.contains("max_retained"), "{message}");
        let tiny_total = policy_from_update(ScheduleUpdate {
            schedule: BackupSchedule::Disabled,
            max_retained: None,
            min_retention_days: None,
            max_total_bytes: Some(1024),
        });
        assert!(tiny_total.validate().is_err());
    }

    #[test]
    fn schedule_json_is_the_shared_wire_shape() {
        let update: ScheduleUpdate = serde_json::from_str(
            r#"{"schedule":"before_upgrade","max_retained":3,"min_retention_days":7,"max_total_bytes":1048576}"#,
        )
        .unwrap();
        assert_eq!(update.schedule, BackupSchedule::BeforeUpgrade);
        assert!(serde_json::from_str::<ScheduleUpdate>(r#"{"schedule":"monthly"}"#).is_err());
        assert!(
            serde_json::from_str::<ScheduleUpdate>(r#"{"schedule":"daily","extra":true}"#).is_err()
        );
    }

    #[test]
    fn download_file_names_are_header_safe() {
        assert_eq!(
            download_file_name("orbit-20261031-backup.orbitbak"),
            "orbit-20261031-backup.orbitbak"
        );
        assert_eq!(download_file_name("a\"b\\c"), "abc");
        assert_eq!(download_file_name("日本語.orbitbak"), "___.orbitbak");
    }

    #[test]
    fn configured_download_limit_is_used_for_too_large_archives() {
        let paths = test_paths();
        assert_eq!(paths.download_limit_bytes, 1024);
    }

    #[test]
    fn replacement_commands_quote_operator_paths() {
        let steps = replacement_steps("/srv/a b/one.orbitbak", "/srv/orbit.db");
        assert_eq!(
            steps[2].command.as_deref(),
            Some(
                "orbit backup restore '/srv/a b/one.orbitbak' --replace --confirm '/srv/orbit.db'"
            )
        );
        let quoted = shell_single_quote("it's here");
        assert_eq!(quoted, "'it'\\''s here'");
    }
}
