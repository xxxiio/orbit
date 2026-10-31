//! SQLite snapshot primitives for the backup protocol.
//!
//! A backup must never copy a live WAL-mode database with the filesystem. This
//! module acquires a short, bounded write fence with SQLite's own locking, uses
//! the online backup API to produce a standalone snapshot file, and reads the
//! authority metadata (revision, workspace identity, counts and referenced
//! attachment objects) from that same fenced snapshot.
//!
//! The fence blocks writers for the duration of `stage_snapshot`: the database
//! copy plus O(1) hardlinks for immutable attachment objects and a small vault
//! copy. Archive compression and hashing happen after the fence is released.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use rusqlite::{Connection, OpenFlags, OptionalExtension};
use serde::Serialize;

use crate::error::{BackupError, Result};
use crate::util;

/// Facts read from one fenced snapshot.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct SnapshotFacts {
    /// Schema version recorded in `orbit_schema`.
    pub schema_version: i64,
    /// Authority revision recorded in `authority_projection`, if present.
    pub revision: Option<i64>,
    /// Stable workspace identity (`authority_identity.generation`), if present.
    pub workspace_id: Option<String>,
    /// Referenced attachment object hashes and declared sizes.
    pub attachment_objects: Vec<AttachmentObjectFact>,
    /// Pending replica outbox rows, if this database is a client replica.
    pub pending_outbox_rows: u64,
    /// Recorded row counts for comparison before and after a restore.
    pub table_counts: BTreeMap<String, u64>,
}

/// One attachment object referenced by the snapshot database.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AttachmentObjectFact {
    pub object_hash: String,
    pub size_bytes: u64,
}

impl SnapshotFacts {
    /// Declared total bytes of referenced attachment objects.
    pub fn attachment_bytes(&self) -> u64 {
        self.attachment_objects
            .iter()
            .map(|object| object.size_bytes)
            .sum()
    }
}

/// A connection holding a SQLite `BEGIN IMMEDIATE` write fence.
///
/// While this value is alive, no other connection can commit a write to the
/// database. Readers continue to run. Dropping it (or calling
/// [`FencedConnection::release`]) rolls back and releases the fence.
pub struct FencedConnection {
    /// Holds the bounded `BEGIN IMMEDIATE` write fence.
    ///
    /// SQLite's backup API cannot source from the same connection that holds
    /// the write transaction, so the snapshot is taken through the established
    /// read-only online-backup helper while this fence prevents any commit.
    fence: Connection,
    /// Authoritative database path (used by the read-only backup helper).
    database: PathBuf,
    released: bool,
}

impl FencedConnection {
    /// Acquire the write fence on an existing SQLite database.
    ///
    /// A busy workspace is reported as [`BackupError::WorkspaceBusy`] rather
    /// than being silently retried.
    pub fn acquire(database: &Path, timeout: Duration) -> Result<Self> {
        let fence = Connection::open_with_flags(
            database,
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .map_err(|error| {
            BackupError::io(
                format!("open {}", database.display()),
                std::io::Error::other(error.to_string()),
            )
        })?;
        fence.busy_timeout(timeout).map_err(|error| {
            BackupError::io(
                "configure fence timeout",
                std::io::Error::other(error.to_string()),
            )
        })?;
        // IMMEDIATE takes the write lock without waiting for a reader to leave,
        // and waits for any in-flight writer to commit or abort within timeout.
        fence.execute_batch("BEGIN IMMEDIATE").map_err(|error| {
            if is_busy(&error) {
                BackupError::WorkspaceBusy(format!(
                    "another process held the write lock for longer than {} ms",
                    timeout.as_millis()
                ))
            } else {
                BackupError::io(
                    format!("begin snapshot fence on {}", database.display()),
                    std::io::Error::other(error.to_string()),
                )
            }
        })?;
        Ok(Self {
            fence,
            database: database.to_path_buf(),
            released: false,
        })
    }

    /// Copy the fenced database through SQLite's online backup API.
    ///
    /// The destination must not already exist; a partially written snapshot is
    /// removed on failure so an interrupted backup cannot publish a torn file.
    pub fn snapshot_database(&self, destination: &Path) -> Result<()> {
        if destination.exists() {
            return Err(BackupError::Conflict(format!(
                "snapshot destination already exists: {}",
                destination.display()
            )));
        }
        util::ensure_parent(destination)?;
        // Keep the fence held for the whole copy. Reuse the established
        // read-only SQLite online-backup helper so WAL handling lives in one
        // place. The helper never opens the source for writing.
        let result = orbit_storage::backup_database_readonly(&self.database, destination);
        if let Err(error) = result {
            let _ = fs::remove_file(destination);
            return Err(BackupError::io(
                format!("snapshot {} through SQLite backup", self.database.display()),
                std::io::Error::other(error.to_string()),
            ));
        }
        util::fsync_file(destination)?;
        Ok(())
    }

    /// Read the authority revision at the fenced instant.
    pub fn revision(&self) -> Result<Option<i64>> {
        optional_i64(
            &self.fence,
            "SELECT authority_revision FROM authority_projection WHERE id = 1",
            "read authority revision",
        )
    }

    /// Read the stable workspace generation identifier at the fenced instant.
    pub fn workspace_id(&self) -> Result<Option<String>> {
        optional_string(
            &self.fence,
            "SELECT generation FROM authority_identity WHERE id = 1",
            "read workspace identity",
        )
    }

    /// Read every referenced attachment object hash and declared size.
    pub fn attachment_objects(&self) -> Result<Vec<AttachmentObjectFact>> {
        if !self.table_exists("attachment_objects")? {
            return Ok(Vec::new());
        }
        let mut statement = self
            .fence
            .prepare("SELECT object_hash, size_bytes FROM attachment_objects ORDER BY object_hash")
            .map_err(|error| sql_error("prepare attachment object query", error))?;
        let rows = statement
            .query_map([], |row| {
                Ok(AttachmentObjectFact {
                    object_hash: row.get(0)?,
                    size_bytes: row.get::<_, i64>(1)?.max(0) as u64,
                })
            })
            .map_err(|error| sql_error("query attachment objects", error))?;
        let mut objects = Vec::new();
        for row in rows {
            objects.push(row.map_err(|error| sql_error("read attachment object", error))?);
        }
        Ok(objects)
    }

    /// Read row counts for every non-internal table in the snapshot.
    pub fn table_counts(&self) -> Result<BTreeMap<String, u64>> {
        let mut statement = self
            .fence
            .prepare(
                "SELECT name FROM sqlite_master WHERE type = 'table' \
                 AND name NOT LIKE 'sqlite_%' ORDER BY name",
            )
            .map_err(|error| sql_error("prepare table list", error))?;
        let names = statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|error| sql_error("query table list", error))?;
        let mut table_names = Vec::new();
        for name in names {
            table_names.push(name.map_err(|error| sql_error("read table name", error))?);
        }
        let mut counts = BTreeMap::new();
        for name in table_names {
            let quoted = name.replace('"', "\"\"");
            let count: i64 = self
                .fence
                .query_row(&format!("SELECT COUNT(*) FROM \"{quoted}\""), [], |row| {
                    row.get(0)
                })
                .map_err(|error| sql_error("count table rows", error))?;
            counts.insert(name, count.max(0) as u64);
        }
        Ok(counts)
    }

    /// Count pending replica outbox rows, when this database has an outbox.
    pub fn pending_outbox_rows(&self) -> Result<u64> {
        if !self.table_exists("sync_outbox")? {
            return Ok(0);
        }
        let count: i64 = self
            .fence
            .query_row("SELECT COUNT(*) FROM sync_outbox", [], |row| row.get(0))
            .map_err(|error| sql_error("count pending outbox rows", error))?;
        Ok(count.max(0) as u64)
    }

    /// Read schema version and all snapshot facts while the fence is held.
    pub fn facts(&self) -> Result<SnapshotFacts> {
        let schema_version = self
            .fence
            .query_row("SELECT version FROM orbit_schema WHERE id = 1", [], |row| {
                row.get::<_, i64>(0)
            })
            .optional()
            .map_err(|error| sql_error("read schema version", error))?
            .ok_or_else(|| {
                BackupError::Integrity(
                    "database has no orbit_schema row; it is not an Orbit workspace".to_owned(),
                )
            })?;
        Ok(SnapshotFacts {
            schema_version,
            revision: self.revision()?,
            workspace_id: self.workspace_id()?,
            attachment_objects: self.attachment_objects()?,
            pending_outbox_rows: self.pending_outbox_rows()?,
            table_counts: self.table_counts()?,
        })
    }

    /// Release the fence by rolling back the empty write transaction.
    pub fn release(mut self) -> Result<()> {
        self.release_inner()
    }

    fn release_inner(&mut self) -> Result<()> {
        if self.released {
            return Ok(());
        }
        self.released = true;
        self.fence
            .execute_batch("ROLLBACK")
            .map_err(|error| sql_error("release snapshot fence", error))
    }

    fn table_exists(&self, table: &str) -> Result<bool> {
        let exists: bool = self
            .fence
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1)",
                [table],
                |row| row.get(0),
            )
            .map_err(|error| sql_error("check table existence", error))?;
        Ok(exists)
    }
}

impl Drop for FencedConnection {
    fn drop(&mut self) {
        let _ = self.release_inner();
    }
}

/// Read-only snapshot inspection result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SnapshotInspection {
    pub schema_version: Option<i64>,
    pub orbit_schema_present: bool,
    pub integrity_ok: bool,
    pub table_count: i64,
}

/// Inspect a snapshot file without writing to it.
pub fn inspect_snapshot(path: &Path) -> Result<SnapshotInspection> {
    if !path.is_file() {
        return Err(BackupError::NotFound(format!(
            "snapshot file not found: {}",
            path.display()
        )));
    }
    let inspection = orbit_storage::inspect_database_readonly(path).map_err(|error| {
        BackupError::Integrity(format!(
            "database inspection failed for {}: {error}",
            path.display()
        ))
    })?;
    Ok(SnapshotInspection {
        schema_version: inspection.schema_version,
        orbit_schema_present: inspection.orbit_schema_present,
        integrity_ok: inspection.integrity_ok,
        table_count: inspection.table_count,
    })
}

/// Read the workspace revision from an existing (unfenced) database, if any.
pub fn read_revision(database: &Path) -> Result<Option<i64>> {
    let connection = Connection::open_with_flags(database, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|error| {
            BackupError::io(
                format!("open {}", database.display()),
                std::io::Error::other(error.to_string()),
            )
        })?;
    optional_i64(
        &connection,
        "SELECT authority_revision FROM authority_projection WHERE id = 1",
        "read authority revision",
    )
}

/// Full path of the SQLite `-wal` sidecar.
pub fn wal_path(database: &Path) -> PathBuf {
    let mut text = database.as_os_str().to_owned();
    text.push("-wal");
    PathBuf::from(text)
}

/// Full path of the SQLite `-shm` sidecar.
pub fn shm_path(database: &Path) -> PathBuf {
    let mut text = database.as_os_str().to_owned();
    text.push("-shm");
    PathBuf::from(text)
}

/// Whether SQLite sidecar files exist beside a database, which indicates a
/// writer may have been active. Used by restore preflight, never as proof.
pub fn sidecars_present(database: &Path) -> bool {
    wal_path(database).exists() || shm_path(database).exists()
}

fn is_busy(error: &rusqlite::Error) -> bool {
    matches!(
        error,
        rusqlite::Error::SqliteFailure(code, _)
            if code.code == rusqlite::ErrorCode::DatabaseBusy
                || code.code == rusqlite::ErrorCode::DatabaseLocked
    )
}

fn sql_error(context: &str, error: rusqlite::Error) -> BackupError {
    BackupError::Integrity(format!("{context}: {error}"))
}

fn optional_i64(connection: &Connection, query: &str, context: &str) -> Result<Option<i64>> {
    match connection.query_row(query, [], |row| row.get::<_, i64>(0)) {
        Ok(value) => Ok(Some(value)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(error) if error.to_string().contains("no such table") => Ok(None),
        Err(error) => Err(sql_error(context, error)),
    }
}

fn optional_string(connection: &Connection, query: &str, context: &str) -> Result<Option<String>> {
    match connection.query_row(query, [], |row| row.get::<_, String>(0)) {
        Ok(value) => Ok(Some(value)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(error) if error.to_string().contains("no such table") => Ok(None),
        Err(error) => Err(sql_error(context, error)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn create_workspace(path: &Path) -> Connection {
        let connection = Connection::open(path).unwrap();
        let object_hash = "a".repeat(64);
        connection
            .execute_batch(&format!(
                "CREATE TABLE orbit_schema (id INTEGER PRIMARY KEY, version INTEGER);
                 INSERT INTO orbit_schema(id, version) VALUES (1, 20);
                 CREATE TABLE projects (id TEXT PRIMARY KEY, name TEXT);
                 INSERT INTO projects(id, name) VALUES ('p1', 'Orbit');
                 CREATE TABLE attachment_objects (object_hash TEXT PRIMARY KEY, size_bytes INTEGER);
                 INSERT INTO attachment_objects VALUES ('{object_hash}', 5);"
            ))
            .unwrap();
        connection
    }

    #[test]
    fn fence_blocks_other_writers_until_released() {
        let directory = tempfile::tempdir().unwrap();
        let database = directory.path().join("orbit.db");
        create_workspace(&database);
        let fence = FencedConnection::acquire(&database, Duration::from_millis(100)).unwrap();

        let contender = Connection::open(&database).unwrap();
        contender.busy_timeout(Duration::from_millis(10)).unwrap();
        let write = contender.execute(
            "INSERT INTO projects(id, name) VALUES ('p2', 'Blocked')",
            [],
        );
        assert!(write.is_err(), "a second writer must be fenced out");

        fence.release().unwrap();
        contender
            .execute(
                "INSERT INTO projects(id, name) VALUES ('p2', 'Allowed')",
                [],
            )
            .unwrap();
    }

    #[test]
    fn snapshot_is_standalone_and_consistent() {
        let directory = tempfile::tempdir().unwrap();
        let database = directory.path().join("orbit.db");
        create_workspace(&database);
        let fence = FencedConnection::acquire(&database, Duration::from_millis(100)).unwrap();
        let facts = fence.facts().unwrap();
        assert_eq!(facts.schema_version, 20);
        assert_eq!(facts.table_counts.get("projects"), Some(&1));
        assert_eq!(facts.attachment_objects.len(), 1);
        let snapshot = directory.path().join("snapshot.db");
        fence.snapshot_database(&snapshot).unwrap();
        // Never overwrite an existing snapshot destination.
        fence.snapshot_database(&snapshot).unwrap_err();
        fence.release().unwrap();

        let inspection = inspect_snapshot(&snapshot).unwrap();
        assert!(inspection.integrity_ok);
        assert_eq!(inspection.schema_version, Some(20));
        assert!(!sidecars_present(&snapshot));
        assert!(fs::metadata(&snapshot).unwrap().len() > 0);
    }

    #[test]
    fn missing_workspace_is_reported() {
        let directory = tempfile::tempdir().unwrap();
        let database = directory.path().join("missing.db");
        let error = FencedConnection::acquire(&database, Duration::from_millis(10));
        assert!(error.is_err());
    }
}
