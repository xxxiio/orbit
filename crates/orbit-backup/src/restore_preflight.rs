//! Restore preflight: archive verification, target inspection and plan assembly.
//!
//! Owned by the I2 restore implementation. Private support for `restore.rs`.
//! Verifies the archive, checks format/schema compatibility, resolves and
//! compares isolated targets without touching the active workspace, inspects a
//! replacement target for connected-replica/outbox state, and assembles the
//! `RestorePlan`, blockers and warnings.
//!
//! Preflight is strictly non-mutating with respect to the target: it does **not**
//! open the target database for reading or writing with SQLite's normal open
//! path, because even a read-only WAL-mode connection may create empty
//! `-wal`/`-shm` lock files and a dry run must not add a single file. Instead it
//! opens the database through SQLite's `immutable=1` read-only URI, which takes
//! no locks and creates no sidecars, and it never takes the write fence.
//!
//! The definitive fail-closed checks — acquiring the workspace write fence,
//! refusing an active writer, and refusing pending connected-client commands —
//! happen in the apply path (`restore.rs`) after explicit replacement
//! confirmation. A dry run therefore reports `target.active = false` and states
//! that the fence check happens on apply; pre-existing `-wal`/`-shm` sidecars
//! are surfaced as a warning because they can indicate an in-use workspace.

use std::path::{Path, PathBuf};

use rusqlite::{Connection, OpenFlags};

use crate::error::{BackupError, Result};
use crate::layout::{
    ATTACHMENT_OBJECTS_DIR, ATTACHMENT_STORE_DIR, KNOWLEDGE_VAULT_SUFFIX, WorkspaceLayout,
};
use crate::manifest::{BackupManifest, BackupSourceMode};
use crate::restore::{
    RestoreMode, RestoreOptions, RestorePlan, RestorePreflight, RestoreProblem, RestoreTarget,
    RestoreTargetDescription,
};
use crate::snapshot;
use crate::verify::{VerificationReport, VerifyOptions};

/// Fixed external references that a restored workspace cannot contain.
const EXTERNAL_DEPENDENCIES: [&str; 6] = [
    "external Herdr agent sessions and their working directories are not restored",
    "running processes and operating-system state are not restored",
    "Git worktrees, branches and external repositories are not restored",
    "credentials held by external agents or providers are not restored",
    "actions that already executed outside Orbit are not replayed",
    "unaccepted changes and staged uploads on disconnected connected-mode clients are not in the archive",
];

/// Read-only facts gathered about the restore target.
struct TargetInspection {
    description: RestoreTargetDescription,
    replaces: Vec<String>,
    preserves: Vec<String>,
    downtime_required: bool,
}

/// Read-only state of an existing target workspace database.
struct TargetState {
    pending_outbox_rows: u64,
    looks_like_replica: bool,
}

/// Verify the archive and inspect the target without changing anything.
pub(crate) fn preflight(
    archive: &Path,
    target: &RestoreTarget,
    options: &RestoreOptions,
) -> Result<RestorePreflight> {
    let verification = crate::verify_archive(
        archive,
        &VerifyOptions {
            deep: options.deep_verification,
        },
    )?;
    let manifest = verification.manifest.clone().ok_or_else(|| {
        BackupError::Manifest(format!(
            "cannot build a restore plan for {}: the archive manifest could not be read.\n{}",
            archive.display(),
            verification_failure_detail(&verification)
        ))
    })?;
    // Independent re-validation, so format/schema safety never depends on the
    // verifier having run its deep checks.
    manifest.validate()?;
    let supported = crate::supported_schema_version();
    let schema_compatible = manifest.schema_version <= supported;
    let schema_upgrade_required = manifest.schema_version < supported;
    let mut blockers = Vec::new();
    let mut warnings = Vec::new();
    for warning in &verification.warnings {
        warnings.push(problem("verification_warning", warning.clone()));
    }
    if !verification.ok {
        blockers.push(problem(
            "verification_failed",
            verification_failure_detail(&verification),
        ));
    }
    if !schema_compatible {
        blockers.push(problem(
            "incompatible_schema",
            format!(
                "the archive database schema {} is newer than this Orbit build supports \
                 (schema {supported}); install a newer Orbit before restoring. Nothing was changed.",
                manifest.schema_version
            ),
        ));
    }
    if manifest.source.mode == BackupSourceMode::Replica {
        blockers.push(problem(
            "replica_snapshot_not_authoritative",
            "this archive is a connected-client replica snapshot, which is never an authoritative \
             restore source. Restore the server's own backup on the server host, or re-enroll the \
             client instead. Nothing was changed.",
        ));
    }
    if !manifest.complete {
        warnings.push(problem(
            "incomplete_archive",
            "the archive declares deliberate exclusions; the restored workspace will not contain \
             the excluded components listed in the plan",
        ));
    }
    if target.mode() == RestoreMode::Isolated {
        warnings.push(problem(
            "free_space_unknown",
            "free space on the destination filesystem could not be measured portably; extraction \
             fails closed on a full disk and the active workspace is never touched",
        ));
    }
    let connected_client_warnings = connected_client_warnings(&manifest);
    let inspection = inspect_target(target, &manifest, options, &mut blockers, &mut warnings)?;
    let plan = RestorePlan {
        mode: target.mode(),
        archive: archive.display().to_string(),
        archive_bytes: verification.archive_bytes,
        label: manifest.label.clone(),
        created_at: manifest.created_at.clone(),
        backup_source_mode: manifest.source.mode,
        backup_schema_version: manifest.schema_version,
        supported_schema_version: supported,
        schema_compatible,
        schema_upgrade_required,
        target: inspection.description,
        replaces: inspection.replaces,
        preserves: inspection.preserves,
        downtime_required: inspection.downtime_required,
        connected_client_warnings,
        external_dependencies: EXTERNAL_DEPENDENCIES
            .iter()
            .map(|dependency| (*dependency).to_owned())
            .collect(),
        excluded_components: manifest.exclusions.clone(),
        totals: manifest.totals.clone(),
        counts: manifest.counts.clone(),
        free_space_bytes: None,
    };
    Ok(RestorePreflight {
        admissible: blockers.is_empty(),
        plan,
        blockers,
        warnings,
        verification,
    })
}

fn problem(code: &str, detail: impl Into<String>) -> RestoreProblem {
    RestoreProblem {
        code: code.to_owned(),
        detail: detail.into(),
    }
}

fn verification_failure_detail(report: &VerificationReport) -> String {
    let mut detail = format!(
        "archive verification failed with {} problem(s); the archive cannot be restored safely. \
         Nothing was changed.",
        report.problems.len()
    );
    for failure in report.problems.iter().take(5) {
        detail.push_str(&format!("\n  [{}] {}", failure.check, failure.detail));
    }
    detail
}

/// Counts of enrolled authority devices and web sessions in a server snapshot.
///
/// The manifest counts are computed from the same fenced snapshot that is
/// stored in the archive, so they are the snapshot's row counts without
/// extracting the database a second time during preflight.
fn connected_client_warnings(manifest: &BackupManifest) -> Vec<String> {
    if manifest.source.mode != BackupSourceMode::Server {
        return Vec::new();
    }
    let devices = manifest.counts.tables.get("devices").copied().unwrap_or(0);
    let sessions = manifest
        .counts
        .tables
        .get("web_sessions")
        .copied()
        .unwrap_or(0);
    if devices == 0 && sessions == 0 {
        return vec![
            "this server-authority snapshot records no enrolled devices or web sessions; every \
             connected client must re-enroll after the restore"
                .to_owned(),
        ];
    }
    vec![format!(
        "this server-authority snapshot restores {devices} enrolled device(s) and {sessions} web \
         session(s). Connected clients may hold newer accepted revisions or pending commands; the \
         server must fail closed and require explicit reconciliation. Pending commands are never \
         replayed silently."
    )]
}

fn inspect_target(
    target: &RestoreTarget,
    manifest: &BackupManifest,
    options: &RestoreOptions,
    blockers: &mut Vec<RestoreProblem>,
    warnings: &mut Vec<RestoreProblem>,
) -> Result<TargetInspection> {
    match target {
        RestoreTarget::Isolated { destination_dir } => {
            inspect_isolated_target(destination_dir, options, blockers, warnings)
        }
        RestoreTarget::Replace { layout } => {
            inspect_replace_target(layout, manifest, options, blockers, warnings)
        }
    }
}

fn inspect_isolated_target(
    destination: &Path,
    options: &RestoreOptions,
    blockers: &mut Vec<RestoreProblem>,
    warnings: &mut Vec<RestoreProblem>,
) -> Result<TargetInspection> {
    let layout = WorkspaceLayout::for_database(destination.join("orbit.db"))?;
    if destination.exists() {
        if !destination.is_dir() {
            blockers.push(problem(
                "target_not_a_directory",
                format!(
                    "destination {} exists and is not a directory; isolated restore only creates \
                     a new directory. Nothing was changed.",
                    destination.display()
                ),
            ));
        } else if directory_has_entries(destination)? {
            blockers.push(problem(
                "target_not_empty",
                format!(
                    "destination {} already exists and is not empty; isolated restore only creates \
                     a new or empty directory and never overwrites existing files. Nothing was changed.",
                    destination.display()
                ),
            ));
        }
    } else if let Some(parent) = destination
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        && !parent.exists()
    {
        warnings.push(problem(
            "destination_parent_missing",
            format!(
                "the parent directory {} does not exist yet; restore will create it",
                parent.display()
            ),
        ));
    }
    if let Some(active_database) = &options.active_database
        && destination_contains_workspace(destination, active_database)?
    {
        {
            blockers.push(problem(
                "active_workspace",
                format!(
                    "destination {} overlaps the active workspace at {}; an isolated restore never \
                     writes into the running workspace. Choose a new directory. Nothing was changed.",
                    destination.display(),
                    active_database.display()
                ),
            ));
        }
    }
    Ok(TargetInspection {
        description: describe_layout(&layout, destination.exists(), false, 0),
        replaces: Vec::new(),
        preserves: Vec::new(),
        downtime_required: false,
    })
}

fn inspect_replace_target(
    layout: &WorkspaceLayout,
    manifest: &BackupManifest,
    _options: &RestoreOptions,
    blockers: &mut Vec<RestoreProblem>,
    warnings: &mut Vec<RestoreProblem>,
) -> Result<TargetInspection> {
    let inspection = TargetInspection {
        description: describe_layout(layout, layout.database_exists(), false, 0),
        replaces: replace_components(layout, manifest),
        preserves: preserve_components(layout),
        downtime_required: true,
    };
    if !layout.database().is_file() {
        blockers.push(problem(
            "target_workspace_missing",
            format!(
                "replacement requires an existing workspace database at {}; use --to <dir> for an \
                 isolated restore instead. Nothing was changed.",
                layout.database().display()
            ),
        ));
        return Ok(inspection);
    }
    if snapshot::sidecars_present(layout.database()) {
        warnings.push(sidecar_warning(layout));
    }
    // A dry run must not create SQLite lock sidecars, so preflight never takes
    // the write fence (and never opens the database with a normal read-only
    // connection either, which can create `-wal`/`-shm`). The apply path
    // re-checks the fence, pending commands and archive before moving data.
    warnings.push(problem(
        "active_check_deferred",
        format!(
            "preflight does not hold the workspace write lock: a dry run must not create SQLite \
             lock sidecars. The apply step takes the write fence on {} and refuses an active \
             workspace or pending connected-client commands before changing anything.",
            layout.database().display()
        ),
    ));
    let mut pending = 0_u64;
    match read_target_state(layout.database()) {
        Ok(state) => {
            pending = state.pending_outbox_rows;
            if state.looks_like_replica {
                match manifest.source.mode {
                    BackupSourceMode::Server => blockers.push(problem(
                        "target_connected_replica",
                        "the target database is a connected-client replica; an authority backup \
                         cannot be restored over a replica. Restore on the server host, or \
                         re-enroll the client instead. Nothing was changed.",
                    )),
                    BackupSourceMode::Local => warnings.push(problem(
                        "target_looks_like_replica",
                        "the target database looks like a connected-client replica; restoring a \
                         local authority backup over it requires explicit re-enrollment afterwards",
                    )),
                    BackupSourceMode::Replica => {}
                }
            }
        }
        Err(error) => warnings.push(problem(
            "target_inspection_failed",
            format!(
                "could not inspect the target database without opening it for writing: {error}; \
                 the apply step re-checks the workspace before changing anything"
            ),
        )),
    }
    if pending > 0 {
        blockers.push(problem(
            "pending_client_commands",
            format!(
                "the target workspace holds {pending} pending connected-client command(s); \
                 replacing the authority database would lose them and pending commands are never \
                 replayed silently. Flush or export them first, or take a deliberate backup and \
                 remove the replica state. Nothing was changed."
            ),
        ));
    }
    Ok(TargetInspection {
        description: describe_layout(layout, true, false, pending),
        ..inspection
    })
}

fn sidecar_warning(layout: &WorkspaceLayout) -> RestoreProblem {
    problem(
        "sidecars_present",
        format!(
            "SQLite sidecar files (-wal/-shm) are present beside {}, which can indicate an in-use \
             workspace or a previous unclean shutdown. They will be preserved with the previous \
             workspace for rollback; stop every Orbit process before applying the restore.",
            layout.database().display()
        ),
    )
}

fn describe_layout(
    layout: &WorkspaceLayout,
    exists: bool,
    active: bool,
    pending_outbox_rows: u64,
) -> RestoreTargetDescription {
    RestoreTargetDescription {
        database: layout.database().display().to_string(),
        knowledge_vault: layout.knowledge_vault().display().to_string(),
        attachment_store: layout.attachment_store().display().to_string(),
        exists,
        active,
        pending_outbox_rows,
    }
}

fn replace_components(layout: &WorkspaceLayout, manifest: &BackupManifest) -> Vec<String> {
    vec![
        format!("database {}", layout.database().display()),
        format!("knowledge vault {}", layout.knowledge_vault().display()),
        format!(
            "{} attachment object(s) merged into {} (existing objects are kept)",
            manifest.attachment_paths().count(),
            layout.attachment_store().display()
        ),
    ]
}

fn preserve_components(layout: &WorkspaceLayout) -> Vec<String> {
    let mut preserves = vec![format!("previous database {}", layout.database().display())];
    for (suffix, sidecar) in [
        ("-wal", snapshot::wal_path(layout.database())),
        ("-shm", snapshot::shm_path(layout.database())),
    ] {
        if sidecar.exists() {
            preserves.push(format!("previous {suffix} sidecar {}", sidecar.display()));
        }
    }
    if layout.knowledge_vault_exists() {
        preserves.push(format!(
            "previous knowledge vault {}",
            layout.knowledge_vault().display()
        ));
    }
    preserves.push(format!(
        "a recovery journal and rollback instructions under {}/.orbit-preserved-<timestamp>/",
        layout.parent().display()
    ));
    preserves
}

fn directory_has_entries(directory: &Path) -> Result<bool> {
    let mut entries = std::fs::read_dir(directory)
        .map_err(|error| BackupError::io(format!("list {}", directory.display()), error))?;
    Ok(entries.next().is_some())
}

/// Whether restoring into `destination` would write inside the active workspace.
fn destination_contains_workspace(destination: &Path, active_database: &Path) -> Result<bool> {
    let destination = canonicalish(destination)?;
    let active_database = canonicalish(active_database)?;
    let active_parent = match active_database.parent() {
        Some(parent) => canonicalish(parent)?,
        None => PathBuf::new(),
    };
    let active_vault = active_database.with_extension(KNOWLEDGE_VAULT_SUFFIX);
    let active_attachments = active_parent
        .join(ATTACHMENT_STORE_DIR)
        .join(ATTACHMENT_OBJECTS_DIR);
    Ok(active_database.starts_with(&destination)
        || active_vault.starts_with(&destination)
        || active_attachments.starts_with(&destination)
        || destination == active_parent)
}

/// Canonicalize when possible; fall back to a lexically normalized absolute path.
fn canonicalish(path: &Path) -> Result<PathBuf> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|error| BackupError::io("resolve current directory", error))?
            .join(path)
    };
    match absolute.canonicalize() {
        Ok(canonical) => Ok(canonical),
        Err(_) => Ok(crate::util::normalize_lexically(&absolute)),
    }
}

/// Read target state without writing: pending outbox commands and replica shape.
///
/// The database is opened through SQLite's `immutable=1` URI: no locks are
/// taken and no `-wal`/`-shm` sidecars are created, which is what makes a dry
/// run truly free of side effects. If `-wal`/`-shm` sidecars exist, uncheckpointed
/// WAL content is not visible to this advisory read; the apply path re-reads the
/// target while holding the write fence, so a stale dry-run reading cannot cause
/// a mutation or a lost pending command.
fn read_target_state(database: &Path) -> Result<TargetState> {
    let connection = open_immutable(database)?;
    let looks_like_replica: bool = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name IN \
             ('authority_outbox','authority_snapshot','authority_settings','accepted_markdown_cache'))",
            [],
            |row| row.get(0),
        )
        .map_err(target_read_error)?;
    let pending_outbox_rows = if table_exists(&connection, "sync_outbox")? {
        pending_outbox_count(&connection)?
    } else {
        0
    };
    Ok(TargetState {
        pending_outbox_rows,
        looks_like_replica,
    })
}

fn table_exists(connection: &Connection, name: &str) -> Result<bool> {
    let exists: bool = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1)",
            [name],
            |row| row.get(0),
        )
        .map_err(target_read_error)?;
    Ok(exists)
}

fn pending_outbox_count(connection: &Connection) -> Result<u64> {
    // Older replicas may lack `exported_at_ms`; count every row in that case.
    let count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM sync_outbox WHERE exported_at_ms IS NULL",
            [],
            |row| row.get(0),
        )
        .or_else(|_| connection.query_row("SELECT COUNT(*) FROM sync_outbox", [], |row| row.get(0)))
        .map_err(target_read_error)?;
    Ok(count.max(0) as u64)
}

/// Open a database with SQLite's `immutable=1` read-only URI.
///
/// This takes no locks and creates no sidecar files, which is required so that
/// preflight and dry runs never add a file to the workspace. Callers must treat
/// the result as advisory: it never proves the workspace is idle.
fn open_immutable(database: &Path) -> Result<Connection> {
    let uri = immutable_uri(database)?;
    Connection::open_with_flags(
        &uri,
        OpenFlags::SQLITE_OPEN_READ_ONLY
            | OpenFlags::SQLITE_OPEN_URI
            | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|error| {
        BackupError::Integrity(format!(
            "open target database {} for read-only preflight inspection: {error}",
            database.display()
        ))
    })
}

/// Build a `file:` URI with `immutable=1` and percent-escaped path bytes.
fn immutable_uri(database: &Path) -> Result<String> {
    let absolute = if database.is_absolute() {
        database.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|error| BackupError::io("resolve current directory", error))?
            .join(database)
    };
    let mut uri = String::from("file:");
    for byte in absolute.to_string_lossy().bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b'/' => {
                uri.push(byte as char)
            }
            other => uri.push_str(&format!("%{other:02X}")),
        }
    }
    uri.push_str("?immutable=1");
    Ok(uri)
}

fn target_read_error(error: rusqlite::Error) -> BackupError {
    BackupError::Integrity(format!("read target workspace state: {error}"))
}
