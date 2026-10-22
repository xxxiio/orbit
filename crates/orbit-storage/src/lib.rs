//! SQLite-backed local persistence for Orbit Core.
//!
//! The SQLite database is a local materialized state store. It is deliberately
//! not a sync unit: future sync operates on semantic change records instead of
//! copying the live database file.

use orbit_application::{
    AcceptanceCriteriaRepository, ActivityRepository, ApplicationError, AttachmentObjectRecord,
    AttachmentRepository, HabitOccurrenceRepository, HabitRepository, MetricEvidenceRepository,
    MetricObservation, MetricTargetRecord, MilestoneRepository, PendingConflictResolution,
    PendingSemanticChange, ProjectEvidenceBinding, ProjectEvidenceRepository, ProjectRepository,
    RepositoryError, ReservedConflictResolution, ReservedSemanticChange, RoutineRepository,
    ScheduleRepository, SemanticEntityKind, SemanticFieldMutation, SyncFieldVersion,
    SyncOutboxRepository, TaskRepository, UnitOfWork,
};
use orbit_domain::{
    AcceptanceCriteria, AcceptanceCriterionId, ActivityId, ActivityKind, ActivityOrigin,
    ActivityRecord, ActivitySubject, AttachmentId, Habit, HabitCadence, HabitId, HabitOccurrence,
    HabitOccurrenceId, KnowledgeItemId, Milestone, MilestoneId, MilestoneStatus, Project,
    ProjectId, ProjectMode, ProjectProgressSignal, ProjectStatus, ProjectSuccessCriterion, Routine,
    RoutineId, ScheduleBlock, ScheduleBlockId, ScheduleFlexibility, ScheduleTarget,
    SuccessCriterionStatus, Task, TaskId, TaskPlanning, TaskScope, TaskStatus,
};
use rusqlite::{Connection, OpenFlags, OptionalExtension, Row, params};
use std::{
    fs,
    path::{Path, PathBuf},
    process,
};

pub mod agent_jobs;
mod agent_launch_port;
mod agent_observation_port;
pub mod blueprint_actions;
pub mod blueprint_command_receipts;
#[cfg(test)]
mod blueprint_template_tests;
pub mod blueprint_work_items;
pub mod blueprints;
mod repositories;
pub use agent_jobs::{AgentRuntimeObservation, RuntimeActivity};

const SCHEMA_VERSION: i64 = 20;
const DEFAULT_BUSY_TIMEOUT_MS: u64 = 5_000;

/// Task richness added in schema 20. Descriptions and priorities are typed
/// columns; labels are a separate table so the payload never becomes one
/// untyped JSON object.
const TASK_DETAILS_SCHEMA: &str = r#"
ALTER TABLE tasks ADD COLUMN description TEXT NOT NULL DEFAULT '';
ALTER TABLE tasks ADD COLUMN priority TEXT;

CREATE TABLE IF NOT EXISTS task_labels (
    task_id TEXT NOT NULL,
    label TEXT NOT NULL,
    PRIMARY KEY(task_id, label),
    FOREIGN KEY(task_id) REFERENCES tasks(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_task_labels_label ON task_labels(label);

CREATE TABLE IF NOT EXISTS task_acceptance_criteria (
    task_id TEXT NOT NULL,
    criterion_id TEXT NOT NULL,
    position INTEGER NOT NULL CHECK(position >= 0),
    text TEXT NOT NULL,
    completed_at_ms INTEGER,
    PRIMARY KEY(task_id, criterion_id),
    UNIQUE(task_id, position),
    FOREIGN KEY(task_id) REFERENCES tasks(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_task_acceptance_task_position
    ON task_acceptance_criteria(task_id, position);
"#;

/// Attachment metadata. Content bytes live in a managed, content-addressed
/// object store on the filesystem; only metadata and associations are stored
/// here. Multiple associations may reference the same immutable object.
const ATTACHMENT_SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS attachment_objects (
    object_hash TEXT PRIMARY KEY NOT NULL,
    size_bytes INTEGER NOT NULL CHECK(size_bytes >= 0),
    created_at_ms INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS attachments (
    id TEXT PRIMARY KEY NOT NULL,
    object_hash TEXT NOT NULL,
    owner_kind TEXT NOT NULL,
    owner_id TEXT NOT NULL,
    filename TEXT NOT NULL,
    content_type TEXT NOT NULL,
    size_bytes INTEGER NOT NULL CHECK(size_bytes >= 0),
    checksum TEXT NOT NULL,
    description TEXT,
    created_at_ms INTEGER NOT NULL,
    FOREIGN KEY(object_hash) REFERENCES attachment_objects(object_hash)
);

CREATE INDEX IF NOT EXISTS idx_attachments_owner
    ON attachments(owner_kind, owner_id);
CREATE INDEX IF NOT EXISTS idx_attachments_object
    ON attachments(object_hash);
"#;

/// The schema version this build creates and understands.
pub const SUPPORTED_SCHEMA_VERSION: i64 = SCHEMA_VERSION;

/// Read-only facts about a database file. Produced without migrating,
/// repairing, or otherwise writing to the file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatabaseInspection {
    pub schema_version: Option<i64>,
    pub orbit_schema_present: bool,
    pub table_count: i64,
    pub integrity_ok: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SqliteConcurrencyPolicy {
    pub busy_timeout_ms: u64,
    pub wal_enabled: bool,
    pub foreign_keys_enabled: bool,
}

impl Default for SqliteConcurrencyPolicy {
    fn default() -> Self {
        Self {
            busy_timeout_ms: DEFAULT_BUSY_TIMEOUT_MS,
            wal_enabled: true,
            foreign_keys_enabled: true,
        }
    }
}

pub struct SqliteStore {
    connection: Connection,
}

// The command ledger uses the SAME connection and transaction boundary as
// application repositories. This temporary SQL integration boundary allows
// the authoritative command handler to operate on the rich application
// tables without constructing a second SQLite connection or schema.
impl std::ops::Deref for SqliteStore {
    type Target = Connection;
    fn deref(&self) -> &Self::Target {
        &self.connection
    }
}
impl std::ops::DerefMut for SqliteStore {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.connection
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageIntegrityIssue {
    pub area: &'static str,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct StorageIntegrityReport {
    pub issues: Vec<StorageIntegrityIssue>,
}

impl StorageIntegrityReport {
    pub fn is_healthy(&self) -> bool {
        self.issues.is_empty()
    }
}

// The storage facade intentionally contains only stable public types and module wiring.
// Implementation is grouped by responsibility while all operations retain their
// original SqliteStore receiver and transaction behavior.
#[cfg(test)]
mod attachment_storage_tests;
mod project_criteria;
mod row_mapping;
mod sqlite_helpers;
#[cfg(test)]
mod storage_tests;
mod store_core;
mod unit_of_work;

use row_mapping::*;
use sqlite_helpers::*;

/// Inspect a database without opening it for writing. This is deliberately
/// separate from [`SqliteStore::open`], which configures WAL and migrates.
pub fn inspect_database_readonly(
    path: impl AsRef<Path>,
) -> Result<DatabaseInspection, RepositoryError> {
    let path = path.as_ref();
    let connection =
        Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY).map_err(repo_error)?;
    let orbit_schema_present: bool = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'orbit_schema')",
            [],
            |row| row.get(0),
        )
        .map_err(repo_error)?;
    let table_count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%'",
            [],
            |row| row.get(0),
        )
        .map_err(repo_error)?;
    let schema_version = if orbit_schema_present {
        connection
            .query_row("SELECT version FROM orbit_schema WHERE id = 1", [], |row| {
                row.get(0)
            })
            .optional()
            .map_err(repo_error)?
    } else {
        None
    };
    let integrity_ok = connection
        .query_row("PRAGMA integrity_check", [], |row| row.get::<_, String>(0))
        .map(|result| result == "ok")
        .unwrap_or(false);
    Ok(DatabaseInspection {
        schema_version,
        orbit_schema_present,
        table_count,
        integrity_ok,
    })
}

/// Copy a SQLite database through the online backup API without opening the
/// source for writing. The source is opened read-only and the destination must
/// not exist, so an interrupted backup can never overwrite an earlier copy.
///
/// This is used by `orbit update` before binaries are replaced. It does not
/// migrate, repair, or otherwise mutate the source database.
pub fn backup_database_readonly(
    source: impl AsRef<Path>,
    destination: impl AsRef<Path>,
) -> Result<(), RepositoryError> {
    let source = source.as_ref();
    let destination = destination.as_ref();
    if destination.exists() {
        return Err(RepositoryError::Other(format!(
            "backup destination already exists: {}",
            destination.display()
        )));
    }
    let source_connection = Connection::open_with_flags(
        source,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(repo_error)?;
    let mut destination_connection = Connection::open(destination).map_err(repo_error)?;
    let result = {
        let backup = rusqlite::backup::Backup::new(&source_connection, &mut destination_connection)
            .map_err(repo_error)?;
        backup
            .run_to_completion(5, std::time::Duration::from_millis(10), None)
            .map_err(repo_error)
    };
    if let Err(error) = result {
        let _ = fs::remove_file(destination);
        return Err(error);
    }
    destination_connection
        .close()
        .map_err(|(_, error)| repo_error(error))?;
    Ok(())
}

#[cfg(test)]
mod database_backup_tests {
    use super::*;

    fn scratch_path(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "orbit-{label}-{}-{}.sqlite",
            process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    #[test]
    fn backup_copies_an_open_database_without_touching_the_source() {
        let source = scratch_path("backup-source");
        let destination = scratch_path("backup-destination");
        {
            let store = SqliteStore::open(&source).unwrap();
            store
                .execute_batch("CREATE TABLE backup_probe(value TEXT); INSERT INTO backup_probe VALUES ('kept');")
                .unwrap();
        }
        let before = fs::read(&source).unwrap();
        backup_database_readonly(&source, &destination).unwrap();
        assert!(destination.is_file());
        assert_eq!(fs::read(&source).unwrap(), before);
        let copied =
            Connection::open_with_flags(&destination, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
        let value: String = copied
            .query_row("SELECT value FROM backup_probe", [], |row| row.get(0))
            .unwrap();
        assert_eq!(value, "kept");
        drop(copied);
        let _ = fs::remove_file(source);
        let _ = fs::remove_file(destination);
    }

    #[test]
    fn backup_refuses_to_overwrite_an_existing_destination() {
        let source = scratch_path("backup-existing-source");
        let destination = scratch_path("backup-existing-destination");
        drop(SqliteStore::open(&source).unwrap());
        fs::write(&destination, b"keep me").unwrap();
        assert!(backup_database_readonly(&source, &destination).is_err());
        assert_eq!(fs::read(&destination).unwrap(), b"keep me");
        let _ = fs::remove_file(source);
        let _ = fs::remove_file(destination);
    }

    #[test]
    fn backup_fails_for_a_non_database_source() {
        let source = scratch_path("backup-garbage-source");
        let destination = scratch_path("backup-garbage-destination");
        fs::write(&source, b"not a sqlite database").unwrap();
        assert!(backup_database_readonly(&source, &destination).is_err());
        assert!(!destination.exists());
        let _ = fs::remove_file(source);
        let _ = fs::remove_file(destination);
    }
}

#[cfg(test)]
mod readonly_inspection_tests {
    use super::*;

    fn scratch_path(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "orbit-{label}-{}-{}.sqlite",
            process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    #[test]
    fn missing_database_is_an_error() {
        let path = scratch_path("missing-inspect");
        assert!(inspect_database_readonly(&path).is_err());
        assert!(!path.exists());
    }

    #[test]
    fn fresh_database_is_inspected_without_mutation() {
        let path = scratch_path("fresh-inspect");
        drop(SqliteStore::open(&path).unwrap());
        let before = fs::read(&path).unwrap();
        let inspection = inspect_database_readonly(&path).unwrap();
        assert_eq!(inspection.schema_version, Some(SUPPORTED_SCHEMA_VERSION));
        assert!(inspection.orbit_schema_present);
        assert!(inspection.table_count > 0);
        assert!(inspection.integrity_ok);
        assert_eq!(fs::read(&path).unwrap(), before);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn non_orbit_sqlite_database_is_readable_but_not_an_orbit_workspace() {
        let path = scratch_path("foreign-inspect");
        let connection = Connection::open(&path).unwrap();
        connection
            .execute_batch("CREATE TABLE unrelated(id INTEGER PRIMARY KEY)")
            .unwrap();
        drop(connection);
        let inspection = inspect_database_readonly(&path).unwrap();
        assert_eq!(inspection.schema_version, None);
        assert!(!inspection.orbit_schema_present);
        assert_eq!(inspection.table_count, 1);
        assert!(inspection.integrity_ok);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn unreadable_database_is_an_error() {
        let path = scratch_path("garbage-inspect");
        fs::write(&path, b"not a sqlite database").unwrap();
        assert!(inspect_database_readonly(&path).is_err());
        let _ = fs::remove_file(path);
    }
}
