//! Canonical Orbit authority state and durable history live in one SQLite store.
//! The command ledger, receipts, and entity tables share a transaction boundary.
#[cfg(test)]
use orbit_sync::{ChangeEnvelope, ConflictResolution};
use orbit_sync::{ChangeId, DeviceId};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use sha2::{Digest, Sha256};
use std::path::Path;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AuthorityEvent {
    pub cursor: i64,
    pub device_id: String,
    pub sequence: u64,
    pub kind: String,
    pub wire: String,
}

#[cfg(test)]
#[derive(Debug, Clone, serde::Serialize)]
pub struct AppendResult {
    pub cursor: i64,
    pub duplicate: bool,
}

#[derive(Debug)]
#[allow(dead_code)]
pub enum AuthorityError {
    Sql(rusqlite::Error),
    Invalid(&'static str),
    ImmutableConflict,
    Unauthorized,
    StaleRevision,
    IncompleteProjection,
    MigrationConflict,
}
impl From<rusqlite::Error> for AuthorityError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Sql(error)
    }
}

pub fn digest(value: &str) -> Vec<u8> {
    Sha256::digest(value.as_bytes()).to_vec()
}

pub(crate) type FullSnapshot = (
    i64,
    String,
    Vec<crate::domain_commands::ProjectRecord>,
    Vec<crate::domain_commands::TaskRecord>,
    crate::authority_evidence::EvidenceSnapshot,
);

pub struct AuthorityStore {
    // One owning SQLite store: command ledger and application repositories
    // share its exact connection, schema and transaction scope.
    pub(crate) conn: orbit_storage::SqliteStore,
}
impl AuthorityStore {
    pub fn open(path: &Path) -> Result<Self, AuthorityError> {
        // Refuse a replica database *before* orbit-storage initializes the
        // authoritative schema. This prevents either deployment mode from
        // silently modifying the other's SQLite file.
        if path != Path::new(":memory:") && path.exists() {
            let inspection = rusqlite::Connection::open_with_flags(
                path,
                rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
            )?;
            let is_replica: bool = inspection.query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name IN (                 'authority_outbox','authority_snapshot','authority_settings','accepted_markdown_cache'))",
                [], |row| row.get(0),
            )?;
            if is_replica {
                return Err(AuthorityError::Invalid(
                    "connected replica cannot be opened as a local authoritative workspace",
                ));
            }
        }
        let conn = if path == Path::new(":memory:") {
            orbit_storage::SqliteStore::in_memory()
        } else {
            orbit_storage::SqliteStore::open(path)
        }
        .map_err(|_| AuthorityError::Invalid("application schema unavailable"))?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        // No in-place conversion from peer-relay history is supported in the
        // greenfield authority schema: it would risk silently losing commands.
        // Require explicit fresh initialization instead of creating an empty
        // authority history alongside a populated legacy event log.
        let legacy_history: i64 = conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name IN ('relay_events','relay_identity','accepted_projection')",
            [], |row| row.get(0),
        )?;
        if legacy_history != 0 {
            return Err(AuthorityError::MigrationConflict);
        }
        conn.execute_batch("\
            CREATE TABLE IF NOT EXISTS authority_identity (\
                id INTEGER PRIMARY KEY CHECK(id=1), generation TEXT NOT NULL\
            );\
            INSERT OR IGNORE INTO authority_identity(id,generation) VALUES(1,lower(hex(randomblob(16))));\
            CREATE TABLE IF NOT EXISTS devices (\
                id TEXT PRIMARY KEY NOT NULL, token_hash BLOB NOT NULL, revoked INTEGER NOT NULL DEFAULT 0\
            );\
            CREATE TABLE IF NOT EXISTS authority_history (\
                cursor INTEGER PRIMARY KEY AUTOINCREMENT,\
                device_id TEXT NOT NULL REFERENCES devices(id),\
                sequence INTEGER NOT NULL CHECK(sequence > 0),\
                kind TEXT NOT NULL CHECK(kind IN ('change', 'resolution')),\
                wire TEXT NOT NULL,\
                UNIQUE(device_id, sequence)\
            );\
            CREATE INDEX IF NOT EXISTS idx_authority_history_cursor ON authority_history(cursor);\
            CREATE TABLE IF NOT EXISTS activities (
                sequence INTEGER PRIMARY KEY AUTOINCREMENT,
                id TEXT NOT NULL UNIQUE,
                subject_kind TEXT NOT NULL,
                subject_id TEXT NOT NULL,
                activity_kind TEXT NOT NULL,
                activity_kind_value TEXT,
                origin_kind TEXT NOT NULL,
                origin_value TEXT,
                occurred_at_ms INTEGER NOT NULL,
                summary TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_activities_occurred ON activities(occurred_at_ms, sequence);
            CREATE TABLE IF NOT EXISTS markdown_note_index (id TEXT PRIMARY KEY NOT NULL, path TEXT NOT NULL UNIQUE, title TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS task_prerequisites (
                task_id TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
                prerequisite_id TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
                PRIMARY KEY(task_id,prerequisite_id),
                CHECK(task_id != prerequisite_id)
            );
            CREATE INDEX IF NOT EXISTS idx_task_prerequisites_reverse ON task_prerequisites(prerequisite_id,task_id);
            CREATE TABLE IF NOT EXISTS authority_commands (command_id TEXT PRIMARY KEY, actor TEXT NOT NULL, payload TEXT NOT NULL, result_json TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS authority_projection (\
                id INTEGER PRIMARY KEY CHECK(id=1),\
                authority_revision INTEGER NOT NULL,\
                view_json TEXT NOT NULL\
            );")?;
        // A single projects/tasks record is authoritative for both local and
        // HTTP transports. Revision metadata is attached to the same tasks row.
        let task_columns = {
            let mut stmt = conn.prepare("PRAGMA table_info(tasks)")?;
            let rows = stmt.query_map([], |r| r.get::<_, String>(1))?;
            rows.collect::<Result<Vec<_>, _>>()?
        };
        if !task_columns
            .iter()
            .any(|column| column == "status_sequence")
        {
            conn.execute(
                "ALTER TABLE tasks ADD COLUMN status_sequence INTEGER NOT NULL DEFAULT 0",
                [],
            )?;
        }
        // Planning is stored in the same canonical task row as identity/status.
        // Existing local databases can add these nullable fields losslessly.
        for column in [
            "estimated_minutes",
            "earliest_start_at_ms",
            "deadline_at_ms",
        ] {
            if !task_columns.iter().any(|existing| existing == column) {
                conn.execute(
                    &format!("ALTER TABLE tasks ADD COLUMN {column} INTEGER"),
                    [],
                )?;
            }
        }
        // Greenfield cutover: reject the old duplicated authority tables rather
        // than silently reading two potentially inconsistent entity sources.
        let duplicated: i64 = conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name IN ('authority_projects','authority_tasks','authority_habits','authority_habit_occurrences','authority_metrics','authority_metric_observations')",
            [], |r| r.get(0))?;
        if duplicated != 0 {
            return Err(AuthorityError::MigrationConflict);
        }
        // A legacy SQL-authoritative knowledge database must not be opened
        // with the Markdown-only API: doing so would strand invisible notes.
        // Greenfield deployments may initialize a fresh vault instead.
        let retired_table: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='authority_knowledge_items')",
            [], |row| row.get(0))?;
        if retired_table {
            return Err(AuthorityError::MigrationConflict);
        }
        conn.execute_batch("PRAGMA foreign_keys=ON;")?;
        Ok(Self { conn })
    }
    /// Atomically replace the *derived* file-backed note index. Markdown files
    /// (including orbit_id frontmatter) remain the source of truth. No Markdown
    /// body or authoritative knowledge revision is written here.
    pub fn reconcile_markdown_note_index(
        &mut self,
        notes: &[crate::knowledge_service::KnowledgeDocumentInfo],
    ) -> Result<(), AuthorityError> {
        // Validate the entire snapshot before modifying the index; discovery
        // rejects duplicate IDs, but this method must also defend its boundary.
        let mut ids = std::collections::HashSet::new();
        let mut paths = std::collections::HashSet::new();
        for note in notes {
            if note.id.is_empty()
                || note.path.is_empty()
                || note.title.is_empty()
                || !ids.insert(&note.id)
                || !paths.insert(&note.path)
            {
                return Err(AuthorityError::Invalid("ambiguous markdown note index"));
            }
        }
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        tx.execute("DELETE FROM markdown_note_index", [])?;
        {
            let mut insert =
                tx.prepare("INSERT INTO markdown_note_index(id,path,title) VALUES(?1,?2,?3)")?;
            for note in notes {
                insert.execute(params![note.id, note.path, note.title])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn markdown_note_index(
        &self,
    ) -> Result<Vec<crate::knowledge_service::KnowledgeDocumentInfo>, AuthorityError> {
        let mut query = self
            .conn
            .prepare("SELECT id,path,title FROM markdown_note_index ORDER BY id")?;
        let rows = query.query_map([], |r| {
            Ok(crate::knowledge_service::KnowledgeDocumentInfo {
                id: r.get(0)?,
                path: r.get(1)?,
                title: r.get(2)?,
            })
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    /// Produce a consistent, standalone SQLite backup while the relay may be running.
    /// VACUUM INTO captures committed WAL transactions without copying a live WAL file.
    pub fn backup_to(&self, destination: &Path) -> Result<(), AuthorityError> {
        if destination.exists() {
            return Err(AuthorityError::Invalid("backup destination already exists"));
        }
        self.conn.execute(
            "VACUUM main INTO ?1",
            [destination.to_string_lossy().as_ref()],
        )?;
        Ok(())
    }
    /// Validate a standalone backup before any offline operator restore.
    pub fn validate_backup(path: &Path) -> Result<(), AuthorityError> {
        let connection =
            Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        let integrity: String = connection.query_row("PRAGMA integrity_check", [], |r| r.get(0))?;
        if integrity != "ok" {
            return Err(AuthorityError::Invalid("backup integrity failure"));
        }
        for table in ["devices", "authority_history"] {
            let count: i64 = connection.query_row(
                "SELECT count(*) FROM sqlite_master WHERE type='table' AND name=?1",
                [table],
                |r| r.get(0),
            )?;
            if count != 1 {
                return Err(AuthorityError::Invalid("not an Orbit relay backup"));
            }
        }
        Ok(())
    }
    pub fn enroll(&mut self, id: &str, token: &str) -> Result<(), AuthorityError> {
        if id.trim().is_empty() || token.len() < 32 {
            return Err(AuthorityError::Invalid("device id or token"));
        }
        self.conn.execute(
            "INSERT INTO devices(id,token_hash) VALUES(?1,?2)",
            params![id, digest(token)],
        )?;
        Ok(())
    }
    /// Atomically replace the credential of an active device. Revoked devices
    /// cannot be revived by rotation; they require explicit new enrollment.
    pub fn rotate_token(&mut self, id: &str, token: &str) -> Result<bool, AuthorityError> {
        if id.trim().is_empty() || token.len() < 32 {
            return Err(AuthorityError::Invalid("device id or token"));
        }
        Ok(self.conn.execute(
            "UPDATE devices SET token_hash=?2 WHERE id=?1 AND revoked=0",
            params![id, digest(token)],
        )? == 1)
    }
    pub fn revoke(&mut self, id: &str) -> Result<bool, AuthorityError> {
        Ok(self.conn.execute(
            "UPDATE devices SET revoked=1 WHERE id=?1 AND revoked=0",
            [id],
        )? == 1)
    }
    pub fn authorized(&self, id: &str, token: &str) -> Result<bool, AuthorityError> {
        use subtle::ConstantTimeEq;
        let record: Option<(Vec<u8>, i64)> = self
            .conn
            .query_row(
                "SELECT token_hash,revoked FROM devices WHERE id=?1",
                [id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        Ok(record
            .map(|(hash, revoked)| revoked == 0 && bool::from(hash.ct_eq(&digest(token))))
            .unwrap_or(false))
    }
    #[cfg(test)]
    pub fn append(
        &mut self,
        submitting_device: &str,
        kind: &str,
        wire: &str,
    ) -> Result<AppendResult, AuthorityError> {
        let (device, sequence) = match kind {
            "change" => {
                let change = ChangeEnvelope::from_wire_json(wire)
                    .map_err(|_| AuthorityError::Invalid("invalid semantic change"))?;
                (
                    change.id.device_id().as_str().to_owned(),
                    change.id.sequence(),
                )
            }
            "resolution" => {
                let resolution = ConflictResolution::from_wire_json(wire)
                    .map_err(|_| AuthorityError::Invalid("invalid semantic resolution"))?;
                (
                    resolution.resolved_by.device_id().as_str().to_owned(),
                    resolution.resolved_by.sequence(),
                )
            }
            _ => return Err(AuthorityError::Invalid("unsupported record kind")),
        };
        if device != submitting_device {
            return Err(AuthorityError::Unauthorized);
        }
        let sequence =
            i64::try_from(sequence).map_err(|_| AuthorityError::Invalid("sequence overflow"))?;
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let existing: Option<(i64, String, String)> = tx
            .query_row(
                "SELECT cursor,kind,wire FROM authority_history WHERE device_id=?1 AND sequence=?2",
                params![device, sequence],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()?;
        if let Some((cursor, existing_kind, existing_wire)) = existing {
            return if existing_kind == kind && existing_wire == wire {
                Ok(AppendResult {
                    cursor,
                    duplicate: true,
                })
            } else {
                Err(AuthorityError::ImmutableConflict)
            };
        }
        // Fail closed after preview domain commands were committed. The two
        // migration-stage write models cannot be mixed without a canonical
        // shared transaction and an explicit import/cutover procedure.
        if Self::has_domain_commands(&tx)? || Self::has_authority_commands(&tx)? {
            return Err(AuthorityError::MigrationConflict);
        }
        // Relay is append-only; out-of-order arrival is safe because clients apply
        // Orbit's semantic sequence/dependency rules when consuming the feed.
        tx.execute(
            "INSERT INTO authority_history(device_id,sequence,kind,wire) VALUES(?1,?2,?3,?4)",
            params![device, sequence, kind, wire],
        )?;
        let cursor = tx.last_insert_rowid();
        tx.commit()?;
        Ok(AppendResult {
            cursor,
            duplicate: false,
        })
    }
    pub(crate) fn has_domain_commands(tx: &rusqlite::Connection) -> Result<bool, AuthorityError> {
        let has_table: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='domain_command_log')",
            [], |row| row.get(0))?;
        if !has_table {
            return Ok(false);
        }
        Ok(tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM domain_command_log)",
            [],
            |row| row.get(0),
        )?)
    }
    #[cfg(test)]
    fn has_authority_commands(tx: &rusqlite::Transaction<'_>) -> Result<bool, AuthorityError> {
        Ok(tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM authority_commands)",
            [],
            |row| row.get(0),
        )?)
    }
    /// Read both canonical collections from one SQLite snapshot. This is the
    /// connected client's replication boundary: separate HTTP reads can span
    /// different revisions even when each collection is internally consistent.
    /// A single-transaction accepted snapshot plus a fingerprint of its entire
    /// immutable history. A restored/replaced relay can have the same revision
    /// as a previously cached view; a revision alone is not an identity.
    /// Fingerprint includes the relay generation and length-delimited event fields.
    pub fn authority_full_snapshot(&self) -> Result<FullSnapshot, AuthorityError> {
        use sha2::{Digest, Sha256};
        let tx = self.conn.unchecked_transaction()?;
        let revision: i64 = tx.query_row(
            "SELECT COALESCE(MAX(cursor),0) FROM authority_history",
            [],
            |r| r.get(0),
        )?;
        let generation: String = tx.query_row(
            "SELECT generation FROM authority_identity WHERE id=1",
            [],
            |r| r.get(0),
        )?;
        let mut hasher = Sha256::new();
        hasher.update(b"orbit-authority-history-v1\0");
        hasher.update((generation.len() as u64).to_be_bytes());
        hasher.update(generation.as_bytes());
        {
            let mut stmt = tx.prepare(
                "SELECT cursor,device_id,sequence,kind,wire FROM authority_history ORDER BY cursor",
            )?;
            let mut rows = stmt.query([])?;
            while let Some(row) = rows.next()? {
                let cursor: i64 = row.get(0)?;
                let device: String = row.get(1)?;
                let sequence: i64 = row.get(2)?;
                let kind: String = row.get(3)?;
                let wire: String = row.get(4)?;
                hasher.update(cursor.to_be_bytes());
                hasher.update(sequence.to_be_bytes());
                for field in [&device, &kind, &wire] {
                    hasher.update((field.len() as u64).to_be_bytes());
                    hasher.update(field.as_bytes());
                }
            }
        }
        let projects = {
            let mut stmt = tx.prepare("SELECT id,name,status FROM projects ORDER BY id")?;
            let rows = stmt.query_map([], |r| {
                Ok(crate::domain_commands::ProjectRecord {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    status: r.get(2)?,
                })
            })?;
            rows.collect::<Result<Vec<_>, _>>()?
        };
        let tasks = {
            let mut stmt = tx.prepare("SELECT id,title,status,project_id,milestone_id,estimated_minutes,earliest_start_at_ms,deadline_at_ms FROM tasks ORDER BY id")?;
            let rows = stmt.query_map([], |r| {
                Ok(crate::domain_commands::TaskRecord {
                    id: r.get(0)?,
                    title: r.get(1)?,
                    status: r.get(2)?,
                    project_id: r.get(3)?,
                    milestone_id: r.get(4)?,
                    estimated_minutes: r.get(5)?,
                    earliest_start_at_ms: r.get(6)?,
                    deadline_at_ms: r.get(7)?,
                })
            })?;
            rows.collect::<Result<Vec<_>, _>>()?
        };
        // Do not return a canonical claim if the relay was populated outside
        // the accepted command path or its receipt history is incomplete.
        let command_count: i64 =
            tx.query_row("SELECT COUNT(*) FROM authority_commands", [], |r| r.get(0))?;
        let event_count: i64 =
            tx.query_row("SELECT COUNT(*) FROM authority_history", [], |r| r.get(0))?;
        if command_count != event_count || revision != event_count {
            return Err(AuthorityError::MigrationConflict);
        }
        let fingerprint = format!("{:x}", hasher.finalize());
        let habits = {
            let mut stmt = tx.prepare("SELECT id,name,CASE WHEN cadence_kind='daily' THEN 'daily' ELSE 'weekly:'||target_per_week END FROM habits ORDER BY id")?;
            let rows = stmt.query_map([], |r| {
                Ok(crate::authority_evidence::HabitRecord {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    cadence: r.get(2)?,
                })
            })?;
            rows.collect::<Result<Vec<_>, _>>()?
        };
        let habit_occurrences = {
            let mut stmt = tx.prepare("SELECT id,habit_id,occurred_at_ms FROM habit_occurrences ORDER BY occurred_at_ms,id")?;
            let rows = stmt.query_map([], |r| {
                Ok(crate::authority_evidence::HabitOccurrenceRecord {
                    id: r.get(0)?,
                    habit_id: r.get(1)?,
                    occurred_at_ms: r.get(2)?,
                })
            })?;
            rows.collect::<Result<Vec<_>, _>>()?
        };
        let metrics = {
            let mut stmt = tx.prepare("SELECT metric_id,baseline,target,CASE WHEN direction='at_least' THEN 'increasing' ELSE 'decreasing' END FROM metric_targets ORDER BY metric_id")?;
            let rows = stmt.query_map([], |r| {
                Ok(crate::authority_evidence::MetricRecord {
                    id: r.get(0)?,
                    baseline: r.get(1)?,
                    target: r.get(2)?,
                    direction: r.get(3)?,
                })
            })?;
            rows.collect::<Result<Vec<_>, _>>()?
        };
        let metric_observations = {
            let mut stmt = tx.prepare("SELECT metric_id,observed_at_ms,value FROM metric_observations ORDER BY metric_id,observed_at_ms")?;
            let rows = stmt.query_map([], |r| {
                Ok(crate::authority_evidence::MetricObservationRecord {
                    metric_id: r.get(0)?,
                    observed_at_ms: r.get(1)?,
                    value: r.get(2)?,
                })
            })?;
            rows.collect::<Result<Vec<_>, _>>()?
        };
        let evidence = crate::authority_evidence::EvidenceSnapshot {
            revision,
            history_fingerprint: fingerprint.clone(),
            habits,
            habit_occurrences,
            metrics,
            metric_observations,
        };
        tx.commit()?;
        Ok((revision, fingerprint, projects, tasks, evidence))
    }

    /// Verify an accepted client prefix against the CURRENT server history.
    /// The client must require this proof before accepting a newer revision.
    /// Generation is included so even an identically replayed replacement server
    /// cannot silently impersonate the original authority.
    pub fn authority_history_prefix(&self, revision: i64) -> Result<String, AuthorityError> {
        use sha2::{Digest, Sha256};
        let tx = self.conn.unchecked_transaction()?;
        let tip: i64 = tx.query_row(
            "SELECT COALESCE(MAX(cursor),0) FROM authority_history",
            [],
            |r| r.get(0),
        )?;
        let commands: i64 =
            tx.query_row("SELECT COUNT(*) FROM authority_commands", [], |r| r.get(0))?;
        let events: i64 =
            tx.query_row("SELECT COUNT(*) FROM authority_history", [], |r| r.get(0))?;
        if revision < 0 || revision > tip || commands != events || tip != events {
            return Err(AuthorityError::MigrationConflict);
        }
        let generation: String = tx.query_row(
            "SELECT generation FROM authority_identity WHERE id=1",
            [],
            |r| r.get(0),
        )?;
        let mut hash = Sha256::new();
        hash.update(b"orbit-authority-history-v1\0");
        hash.update((generation.len() as u64).to_be_bytes());
        hash.update(generation.as_bytes());
        {
            let mut stmt=tx.prepare("SELECT cursor,device_id,sequence,kind,wire FROM authority_history WHERE cursor<=?1 ORDER BY cursor")?;
            let mut rows = stmt.query([revision])?;
            while let Some(row) = rows.next()? {
                let cursor: i64 = row.get(0)?;
                let device: String = row.get(1)?;
                let sequence: i64 = row.get(2)?;
                let kind: String = row.get(3)?;
                let wire: String = row.get(4)?;
                hash.update(cursor.to_be_bytes());
                hash.update(sequence.to_be_bytes());
                for field in [&device, &kind, &wire] {
                    hash.update((field.len() as u64).to_be_bytes());
                    hash.update(field.as_bytes());
                }
            }
        }
        tx.commit()?;
        Ok(format!("{:x}", hash.finalize()))
    }

    #[cfg(test)]
    pub fn authority_verified_snapshot(
        &self,
    ) -> Result<
        (
            i64,
            String,
            Vec<crate::domain_commands::ProjectRecord>,
            Vec<crate::domain_commands::TaskRecord>,
        ),
        AuthorityError,
    > {
        let (revision, fingerprint, projects, tasks, _) = self.authority_full_snapshot()?;
        Ok((revision, fingerprint, projects, tasks))
    }

    #[cfg(test)]
    pub fn authority_snapshot(
        &self,
    ) -> Result<
        (
            i64,
            Vec<crate::domain_commands::ProjectRecord>,
            Vec<crate::domain_commands::TaskRecord>,
        ),
        AuthorityError,
    > {
        let tx = self.conn.unchecked_transaction()?;
        let revision: i64 = tx.query_row(
            "SELECT COALESCE(MAX(cursor),0) FROM authority_history",
            [],
            |r| r.get(0),
        )?;
        let projects = {
            let mut stmt = tx.prepare("SELECT id,name,status FROM projects ORDER BY id")?;
            let rows = stmt.query_map([], |r| {
                Ok(crate::domain_commands::ProjectRecord {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    status: r.get(2)?,
                })
            })?;
            rows.collect::<Result<Vec<_>, _>>()?
        };
        let tasks = {
            let mut stmt = tx.prepare("SELECT id,title,status,project_id,milestone_id,estimated_minutes,earliest_start_at_ms,deadline_at_ms FROM tasks ORDER BY id")?;
            let rows = stmt.query_map([], |r| {
                Ok(crate::domain_commands::TaskRecord {
                    id: r.get(0)?,
                    title: r.get(1)?,
                    status: r.get(2)?,
                    project_id: r.get(3)?,
                    milestone_id: r.get(4)?,
                    estimated_minutes: r.get(5)?,
                    earliest_start_at_ms: r.get(6)?,
                    deadline_at_ms: r.get(7)?,
                })
            })?;
            rows.collect::<Result<Vec<_>, _>>()?
        };
        tx.commit()?;
        Ok((revision, projects, tasks))
    }

    #[cfg(test)]
    pub fn authority_command_count(&self) -> Result<i64, AuthorityError> {
        Ok(self
            .conn
            .query_row("SELECT COUNT(*) FROM authority_commands", [], |r| r.get(0))?)
    }

    pub fn projects(
        &self,
    ) -> Result<(i64, Vec<crate::domain_commands::ProjectRecord>), AuthorityError> {
        let mut stmt = self
            .conn
            .prepare("SELECT id,name,status FROM projects ORDER BY id")?;
        let projects = stmt
            .query_map([], |r| {
                Ok(crate::domain_commands::ProjectRecord {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    status: r.get(2)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok((self.tip()?, projects))
    }
    /// Create a canonical project; state, event, receipt and replay snapshot
    /// share the same immediate SQLite transaction as authoritative tasks.
    pub fn authority_project_command(
        &mut self,
        actor: &str,
        command: crate::domain_commands::ProjectCommand,
    ) -> Result<crate::domain_commands::ProjectResult, AuthorityError> {
        use crate::domain_commands::{ProjectRecord, ProjectResult};
        use orbit_domain::{Project, ProjectId};
        use orbit_sync::{EntityKind, EntityRef, FieldMutation, SemanticChange};
        if actor.trim().is_empty()
            || command.command_id.trim().is_empty()
            || command.command_id.len() > 128
            || command.expected_revision < 0
        {
            return Err(AuthorityError::Invalid("invalid project command"));
        }
        let payload = serde_json::to_string(&command)
            .map_err(|_| AuthorityError::Invalid("invalid project command"))?;
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let pending = crate::command_engine::PendingCommand {
            command_id: &command.command_id,
            actor,
            payload: &payload,
            expected_revision: command.expected_revision,
        };
        let next = match pending.check(&tx)? {
            crate::command_engine::CommandAdmission::Duplicate(json) => {
                let mut result: ProjectResult = serde_json::from_str(&json)
                    .map_err(|_| AuthorityError::Invalid("invalid stored project receipt"))?;
                result.duplicate = true;
                return Ok(result);
            }
            crate::command_engine::CommandAdmission::New(sequence) => sequence,
        };
        if command.project_id.len() > 256 || command.name.len() > 4096 {
            return Err(AuthorityError::Invalid("project field too long"));
        }
        let project = Project::new(
            ProjectId::new(command.project_id.clone())
                .map_err(|_| AuthorityError::Invalid("invalid project id"))?,
            command.name.clone(),
        )
        .map_err(|_| AuthorityError::Invalid("invalid project name"))?;
        let record = ProjectRecord {
            id: project.id().as_str().to_owned(),
            name: project.name().to_owned(),
            status: project.status().as_str().to_owned(),
        };
        if tx.execute(
            "INSERT OR IGNORE INTO projects(id,name,status) VALUES(?1,?2,?3)",
            params![record.id, record.name, record.status],
        )? != 1
        {
            return Err(AuthorityError::ImmutableConflict);
        }
        let change = SemanticChange::new(
            EntityRef::new(EntityKind::Project, record.id.clone())
                .map_err(|_| AuthorityError::Invalid("invalid project reference"))?,
            vec![
                FieldMutation::new("name", Some(record.name.clone()), None)
                    .map_err(|_| AuthorityError::Invalid("invalid project name"))?,
                FieldMutation::new("status", Some(record.status.clone()), None)
                    .map_err(|_| AuthorityError::Invalid("invalid project status"))?,
            ],
        )
        .map_err(|_| AuthorityError::Invalid("invalid project semantic change"))?;
        let result = ProjectResult {
            revision: next,
            duplicate: false,
            project: record,
        };
        let result_json = serde_json::to_string(&result)
            .map_err(|_| AuthorityError::Invalid("result serialization"))?;
        pending.commit(&tx, next, change, &result_json)?;
        tx.commit()?;
        Ok(result)
    }
    pub fn tasks(&self) -> Result<(i64, Vec<crate::domain_commands::TaskRecord>), AuthorityError> {
        let mut stmt = self.conn.prepare("SELECT id,title,status,project_id,milestone_id,estimated_minutes,earliest_start_at_ms,deadline_at_ms FROM tasks ORDER BY id")?;
        let tasks = stmt
            .query_map([], |r| {
                Ok(crate::domain_commands::TaskRecord {
                    id: r.get(0)?,
                    title: r.get(1)?,
                    status: r.get(2)?,
                    project_id: r.get(3)?,
                    milestone_id: r.get(4)?,
                    estimated_minutes: r.get(5)?,
                    earliest_start_at_ms: r.get(6)?,
                    deadline_at_ms: r.get(7)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok((self.tip()?, tasks))
    }
    /// Single authority transaction: validate domain rules, commit the canonical
    /// task, immutable semantic event and command acknowledgement together.
    /// This cutover path intentionally rejects any pre-existing legacy history;
    /// migration/import must occur before it can be enabled on an existing vault.
    pub fn authority_task_command(
        &mut self,
        actor: &str,
        command: crate::domain_commands::TaskCommand,
    ) -> Result<crate::domain_commands::TaskResult, AuthorityError> {
        use crate::domain_commands::{TaskAction, TaskRecord, TaskResult};
        use orbit_domain::{Task, TaskId, TaskScope};
        use orbit_sync::{EntityKind, EntityRef, FieldMutation, SemanticChange};
        if actor.trim().is_empty()
            || command.command_id.trim().is_empty()
            || command.command_id.len() > 128
            || command.expected_revision < 0
        {
            return Err(AuthorityError::Invalid("invalid command envelope"));
        }
        let payload = serde_json::to_string(&command)
            .map_err(|_| AuthorityError::Invalid("command serialization"))?;
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let pending = crate::command_engine::PendingCommand {
            command_id: &command.command_id,
            actor,
            payload: &payload,
            expected_revision: command.expected_revision,
        };
        let sequence = match pending.check(&tx)? {
            crate::command_engine::CommandAdmission::Duplicate(json) => {
                let mut result: TaskResult = serde_json::from_str(&json)
                    .map_err(|_| AuthorityError::Invalid("corrupt command receipt"))?;
                result.duplicate = true;
                return Ok(result);
            }
            crate::command_engine::CommandAdmission::New(sequence) => sequence,
        };
        let (record, mutations) = match &command.action {
            TaskAction::Create {
                task_id,
                title,
                project_id,
            } => {
                let scope = if let Some(project_id) = project_id {
                    let id = orbit_domain::ProjectId::new(project_id.clone())
                        .map_err(|_| AuthorityError::Invalid("invalid project id"))?;
                    let exists: bool = tx.query_row(
                        "SELECT EXISTS(SELECT 1 FROM projects WHERE id=?1)",
                        [project_id],
                        |r| r.get(0),
                    )?;
                    if !exists {
                        return Err(AuthorityError::Invalid("project does not exist"));
                    }
                    TaskScope::in_project(id)
                } else {
                    TaskScope::standalone()
                };
                if task_id.len() > 256 || title.len() > 4096 {
                    return Err(AuthorityError::Invalid("task field too long"));
                }
                let task = Task::new(
                    TaskId::new(task_id.clone())
                        .map_err(|_| AuthorityError::Invalid("invalid task id"))?,
                    title.clone(),
                    scope,
                )
                .map_err(|_| AuthorityError::Invalid("invalid task"))?;
                let record = TaskRecord {
                    id: task.id().as_str().into(),
                    title: task.title().into(),
                    status: task.status().as_str().into(),
                    project_id: project_id.clone(),
                    milestone_id: None,
                    estimated_minutes: None,
                    earliest_start_at_ms: None,
                    deadline_at_ms: None,
                };
                let changed=tx.execute("INSERT OR IGNORE INTO tasks(id,title,status,status_sequence,project_id) VALUES(?1,?2,?3,?4,?5)", params![record.id,record.title,record.status,sequence,record.project_id])?;
                if changed != 1 {
                    return Err(AuthorityError::ImmutableConflict);
                }
                (
                    record,
                    vec![
                        FieldMutation::new("title", Some(title.clone()), None).unwrap(),
                        FieldMutation::new("status", Some("inbox".into()), None).unwrap(),
                        FieldMutation::new("project_id", project_id.clone(), None).unwrap(),
                    ],
                )
            }
            TaskAction::Rename { task_id, title } => {
                if title.trim().is_empty() || title.len() > 4096 {
                    return Err(AuthorityError::Invalid("invalid task title"));
                }
                let prior: Option<(String, String, Option<String>)> = tx
                    .query_row(
                        "SELECT title,status,project_id FROM tasks WHERE id=?1",
                        [task_id],
                        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                    )
                    .optional()?;
                let (_old_title, status, project_id) =
                    prior.ok_or(AuthorityError::Invalid("task does not exist"))?;
                let base = latest_task_field_version(&tx, task_id, "title")?;
                tx.execute(
                    "UPDATE tasks SET title=?1 WHERE id=?2",
                    params![title, task_id],
                )?;
                (
                    TaskRecord {
                        id: task_id.clone(),
                        title: title.clone(),
                        status,
                        project_id,
                        milestone_id: None,
                        estimated_minutes: None,
                        earliest_start_at_ms: None,
                        deadline_at_ms: None,
                    },
                    vec![
                        FieldMutation::new("title", Some(title.clone()), base)
                            .map_err(|_| AuthorityError::Invalid("invalid title mutation"))?,
                    ],
                )
            }
            TaskAction::AssignProject {
                task_id,
                project_id,
            } => {
                if let Some(id) = project_id {
                    orbit_domain::ProjectId::new(id.clone())
                        .map_err(|_| AuthorityError::Invalid("invalid project id"))?;
                    let exists: bool = tx.query_row(
                        "SELECT EXISTS(SELECT 1 FROM projects WHERE id=?1)",
                        [id],
                        |row| row.get(0),
                    )?;
                    if !exists {
                        return Err(AuthorityError::Invalid("project does not exist"));
                    }
                }
                let prior: Option<(String, String)> = tx
                    .query_row(
                        "SELECT title,status FROM tasks WHERE id=?1",
                        [task_id],
                        |row| Ok((row.get(0)?, row.get(1)?)),
                    )
                    .optional()?;
                let (title, status) =
                    prior.ok_or(AuthorityError::Invalid("task does not exist"))?;
                let current_milestone: Option<String> = tx.query_row(
                    "SELECT milestone_id FROM tasks WHERE id=?1",
                    [task_id],
                    |row| row.get(0),
                )?;
                if current_milestone.is_some() {
                    let same_project: bool = tx.query_row(
                        "SELECT EXISTS(SELECT 1 FROM milestones WHERE id=?1 AND project_id IS ?2)",
                        params![current_milestone, project_id],
                        |row| row.get(0),
                    )?;
                    if !same_project {
                        return Err(AuthorityError::Invalid(
                            "detach milestone before moving task",
                        ));
                    }
                }
                let base = latest_task_field_version(&tx, task_id, "project_id")?;
                tx.execute(
                    "UPDATE tasks SET project_id=?1 WHERE id=?2",
                    params![project_id, task_id],
                )?;
                (
                    TaskRecord {
                        id: task_id.clone(),
                        title,
                        status,
                        project_id: project_id.clone(),
                        milestone_id: None,
                        estimated_minutes: None,
                        earliest_start_at_ms: None,
                        deadline_at_ms: None,
                    },
                    vec![
                        FieldMutation::new("project_id", project_id.clone(), base)
                            .map_err(|_| AuthorityError::Invalid("invalid project mutation"))?,
                    ],
                )
            }
            TaskAction::AssignMilestone {
                task_id,
                milestone_id,
            } => {
                let prior: Option<(String, String, Option<String>)> = tx
                    .query_row(
                        "SELECT title,status,project_id FROM tasks WHERE id=?1",
                        [task_id],
                        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                    )
                    .optional()?;
                let (title, status, project_id) =
                    prior.ok_or(AuthorityError::Invalid("task does not exist"))?;
                if let Some(id) = milestone_id {
                    if id.is_empty()
                        || id.len() > 128
                        || !id
                            .bytes()
                            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
                    {
                        return Err(AuthorityError::Invalid("invalid milestone ID"));
                    }
                    let matches_project: bool = tx.query_row(
                        "SELECT EXISTS(SELECT 1 FROM milestones WHERE id=?1 AND project_id IS ?2)",
                        params![id, project_id],
                        |row| row.get(0),
                    )?;
                    if !matches_project {
                        return Err(AuthorityError::Invalid(
                            "milestone must belong to the task project",
                        ));
                    }
                }
                let base = latest_task_field_version(&tx, task_id, "milestone_id")?;
                let mutation = FieldMutation::new("milestone_id", milestone_id.clone(), base)
                    .map_err(|_| AuthorityError::Invalid("invalid milestone mutation"))?;
                tx.execute(
                    "UPDATE tasks SET milestone_id=?1 WHERE id=?2",
                    params![milestone_id, task_id],
                )?;
                (
                    TaskRecord {
                        id: task_id.clone(),
                        title,
                        status,
                        project_id,
                        milestone_id: milestone_id.clone(),
                        estimated_minutes: None,
                        earliest_start_at_ms: None,
                        deadline_at_ms: None,
                    },
                    vec![mutation],
                )
            }
            TaskAction::SetPlanning {
                task_id,
                estimated_minutes,
                earliest_start_at_ms,
                deadline_at_ms,
            } => {
                if estimated_minutes.is_some_and(|v| v <= 0 || v > 525600)
                    || earliest_start_at_ms.is_some_and(|v| v < 0)
                    || deadline_at_ms.is_some_and(|v| v < 0)
                    || matches!((earliest_start_at_ms, deadline_at_ms), (Some(start), Some(end)) if start > end)
                {
                    return Err(AuthorityError::Invalid("invalid planning window"));
                }
                let prior: Option<(String, String, Option<String>)> = tx
                    .query_row(
                        "SELECT title,status,project_id FROM tasks WHERE id=?1",
                        [task_id],
                        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                    )
                    .optional()?;
                let (title, status, project_id) =
                    prior.ok_or(AuthorityError::Invalid("task does not exist"))?;
                let keys = [
                    "estimated_minutes",
                    "earliest_start_at_ms",
                    "deadline_at_ms",
                ];
                let values = [*estimated_minutes, *earliest_start_at_ms, *deadline_at_ms];
                let mut mutations = Vec::with_capacity(3);
                for (key, value) in keys.into_iter().zip(values) {
                    let base = latest_task_field_version(&tx, task_id, key)?;
                    mutations.push(
                        FieldMutation::new(key, value.map(|v| v.to_string()), base)
                            .map_err(|_| AuthorityError::Invalid("invalid planning mutation"))?,
                    );
                }
                tx.execute("UPDATE tasks SET estimated_minutes=?1,earliest_start_at_ms=?2,deadline_at_ms=?3 WHERE id=?4",
                    params![estimated_minutes, earliest_start_at_ms, deadline_at_ms, task_id])?;
                (
                    TaskRecord {
                        id: task_id.clone(),
                        title,
                        status,
                        project_id,
                        milestone_id: None,
                        estimated_minutes: *estimated_minutes,
                        earliest_start_at_ms: *earliest_start_at_ms,
                        deadline_at_ms: *deadline_at_ms,
                    },
                    mutations,
                )
            }
            TaskAction::AddPrerequisite {
                task_id,
                prerequisite_id,
            }
            | TaskAction::RemovePrerequisite {
                task_id,
                prerequisite_id,
            } => {
                let add = matches!(&command.action, TaskAction::AddPrerequisite { .. });
                let saved: Option<(String, String, Option<String>)> = tx
                    .query_row(
                        "SELECT title,status,project_id FROM tasks WHERE id=?1",
                        [task_id],
                        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                    )
                    .optional()?;
                let (title, status, project_id) =
                    saved.ok_or(AuthorityError::Invalid("dependent task not found"))?;
                let prerequisite_exists: bool = tx.query_row(
                    "SELECT EXISTS(SELECT 1 FROM tasks WHERE id=?1)",
                    [prerequisite_id],
                    |row| row.get(0),
                )?;
                if !prerequisite_exists {
                    return Err(AuthorityError::Invalid("prerequisite task not found"));
                }
                if task_id == prerequisite_id {
                    return Err(AuthorityError::Invalid("self dependency"));
                }
                let already_exists:bool=tx.query_row(
                    "SELECT EXISTS(SELECT 1 FROM task_prerequisites WHERE task_id=?1 AND prerequisite_id=?2)",
                    params![task_id,prerequisite_id], |row|row.get(0))?;
                if add {
                    if already_exists {
                        return Err(AuthorityError::ImmutableConflict);
                    }
                    // Traversing prerequisite edges determines whether the new edge
                    // would make the dependent reachable from its prerequisite.
                    let cyclic:bool=tx.query_row(
                        "WITH RECURSIVE ancestors(id) AS (                           SELECT prerequisite_id FROM task_prerequisites WHERE task_id=?1                           UNION                           SELECT edge.prerequisite_id FROM task_prerequisites edge JOIN ancestors a ON edge.task_id=a.id                         ) SELECT EXISTS(SELECT 1 FROM ancestors WHERE id=?2)",
                        params![prerequisite_id,task_id],|row|row.get(0))?;
                    if cyclic {
                        return Err(AuthorityError::Invalid("dependency cycle"));
                    }
                    tx.execute(
                        "INSERT INTO task_prerequisites(task_id,prerequisite_id) VALUES(?1,?2)",
                        params![task_id, prerequisite_id],
                    )?;
                } else {
                    if !already_exists {
                        return Err(AuthorityError::Invalid("dependency not found"));
                    }
                    tx.execute(
                        "DELETE FROM task_prerequisites WHERE task_id=?1 AND prerequisite_id=?2",
                        params![task_id, prerequisite_id],
                    )?;
                }
                // Keep the complete sorted edge set as one field so the replay
                // projection and offline replicas share an observable change.
                let dependencies = {
                    let mut stmt=tx.prepare("SELECT prerequisite_id FROM task_prerequisites WHERE task_id=?1 ORDER BY prerequisite_id")?;
                    stmt.query_map([task_id], |row| row.get::<_, String>(0))?
                        .collect::<Result<Vec<_>, _>>()?
                };
                let base = latest_task_field_version(&tx, task_id, "prerequisites")?;
                let mutation = FieldMutation::new(
                    "prerequisites",
                    Some(
                        serde_json::to_string(&dependencies)
                            .map_err(|_| AuthorityError::Invalid("prerequisite serialization"))?,
                    ),
                    base,
                )
                .map_err(|_| AuthorityError::Invalid("invalid prerequisite mutation"))?;
                (
                    TaskRecord {
                        id: task_id.clone(),
                        title,
                        status,
                        project_id,
                        milestone_id: None,
                        estimated_minutes: None,
                        earliest_start_at_ms: None,
                        deadline_at_ms: None,
                    },
                    vec![mutation],
                )
            }
            TaskAction::ChangeStatus { task_id, status } => {
                let prior: Option<(String, String, i64, Option<String>)> = tx
                    .query_row(
                        "SELECT title,status,status_sequence,project_id FROM tasks WHERE id=?1",
                        [task_id],
                        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
                    )
                    .optional()?;
                let (title, old, old_seq, project_id) =
                    prior.ok_or(AuthorityError::Invalid("task does not exist"))?;
                let from = crate::domain_commands::parse_status(&old)
                    .ok_or(AuthorityError::Invalid("invalid saved status"))?;
                let to = crate::domain_commands::parse_status(status)
                    .ok_or(AuthorityError::Invalid("invalid status"))?;
                if !from.can_transition_to(to) {
                    return Err(AuthorityError::Invalid("invalid task transition"));
                }
                tx.execute(
                    "UPDATE tasks SET status=?1,status_sequence=?2 WHERE id=?3",
                    params![to.as_str(), sequence, task_id],
                )?;
                let base = ChangeId::new(
                    DeviceId::new("orbit-server").unwrap(),
                    u64::try_from(old_seq)
                        .map_err(|_| AuthorityError::Invalid("invalid status version"))?,
                )
                .map_err(|_| AuthorityError::Invalid("invalid status version"))?;
                (
                    TaskRecord {
                        id: task_id.clone(),
                        title,
                        status: to.as_str().into(),
                        project_id,
                        milestone_id: None,
                        estimated_minutes: None,
                        earliest_start_at_ms: None,
                        deadline_at_ms: None,
                    },
                    vec![
                        FieldMutation::new("status", Some(to.as_str().into()), Some(base)).unwrap(),
                    ],
                )
            }
        };
        // Return the actual persisted planning values for every task mutation,
        // including renames, status changes and project reassignment.
        let mut record = record;
        (record.milestone_id, record.estimated_minutes, record.earliest_start_at_ms, record.deadline_at_ms) = tx.query_row(
            "SELECT milestone_id,estimated_minutes,earliest_start_at_ms,deadline_at_ms FROM tasks WHERE id=?1",
            [&record.id], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)))?;
        let change = SemanticChange::new(
            EntityRef::new(EntityKind::Task, record.id.clone())
                .map_err(|_| AuthorityError::Invalid("invalid task ref"))?,
            mutations,
        )
        .map_err(|_| AuthorityError::Invalid("invalid task mutation"))?;
        let result = TaskResult {
            revision: sequence,
            duplicate: false,
            task: record,
        };
        let result_json = serde_json::to_string(&result)
            .map_err(|_| AuthorityError::Invalid("command result"))?;
        pending.commit(&tx, sequence, change, &result_json)?;
        tx.commit()?;
        Ok(result)
    }
    #[cfg(test)]
    pub fn event_count(&self) -> Result<i64, AuthorityError> {
        Ok(self
            .conn
            .query_row("SELECT COUNT(*) FROM authority_history", [], |row| {
                row.get(0)
            })?)
    }

    /// Atomically validate and commit a semantic candidate and its accepted projection.
    /// This is still a migration-only gate: full domain commands must be validated
    /// by the shared application services before it can become the main write API.
    #[cfg(test)]
    pub fn commit_semantic(
        &mut self,
        submitting_device: &str,
        expected_tip: i64,
        kind: &str,
        wire: &str,
    ) -> Result<AppendResult, AuthorityError> {
        if expected_tip < 0 {
            return Err(AuthorityError::Invalid("negative expected revision"));
        }
        let (device, sequence) =
            crate::canonical::decode_and_identify(submitting_device, kind, wire)?;
        let sequence_sql =
            i64::try_from(sequence).map_err(|_| AuthorityError::Invalid("sequence overflow"))?;
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        if Self::has_domain_commands(&tx)? || Self::has_authority_commands(&tx)? {
            return Err(AuthorityError::MigrationConflict);
        }
        let mut history = {
            let mut stmt = tx.prepare(
                "SELECT cursor,device_id,sequence,kind,wire FROM authority_history ORDER BY cursor",
            )?;
            let rows = stmt.query_map([], |row| {
                Ok(AuthorityEvent {
                    cursor: row.get(0)?,
                    device_id: row.get(1)?,
                    sequence: row.get(2)?,
                    kind: row.get(3)?,
                    wire: row.get(4)?,
                })
            })?;
            rows.collect::<Result<Vec<_>, _>>()?
        };
        let tip = history.last().map_or(0, |event| event.cursor);
        if tip != expected_tip {
            return Err(AuthorityError::StaleRevision);
        }
        if history
            .iter()
            .any(|event| event.device_id == device && event.sequence == sequence)
        {
            return Err(AuthorityError::ImmutableConflict);
        }
        history.push(AuthorityEvent {
            cursor: tip + 1,
            device_id: device.clone(),
            sequence,
            kind: kind.to_owned(),
            wire: wire.to_owned(),
        });
        let view = crate::canonical::project(&history)
            .map_err(|_| AuthorityError::Invalid("invalid semantic history"))?;
        if !view.complete || view.unresolved_conflicts != 0 {
            return Err(AuthorityError::IncompleteProjection);
        }
        tx.execute(
            "INSERT INTO authority_history(device_id,sequence,kind,wire) VALUES(?1,?2,?3,?4)",
            params![device, sequence_sql, kind, wire],
        )?;
        let cursor = tx.last_insert_rowid();
        let projection_json = serde_json::to_string(&view)
            .map_err(|_| AuthorityError::Invalid("projection serialization failed"))?;
        tx.execute(
            "INSERT INTO authority_projection(id,authority_revision,view_json) VALUES(1,?1,?2) \
             ON CONFLICT(id) DO UPDATE SET authority_revision=excluded.authority_revision, view_json=excluded.view_json",
            params![cursor, projection_json],
        )?;
        tx.commit()?;
        Ok(AppendResult {
            cursor,
            duplicate: false,
        })
    }

    /// Returns the last strictly accepted projection only if no legacy relay
    /// writes have advanced history since acceptance. Otherwise it is stale.
    #[cfg(test)]
    pub fn authority_projection(&self) -> Result<Option<serde_json::Value>, AuthorityError> {
        let stored: Option<(i64, String)> = self
            .conn
            .query_row(
                "SELECT authority_revision,view_json FROM authority_projection WHERE id=1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        let Some((accepted_tip, json)) = stored else {
            return Ok(None);
        };
        if accepted_tip != self.tip()? {
            return Ok(None);
        }
        let value = serde_json::from_str(&json)
            .map_err(|_| AuthorityError::Invalid("corrupt accepted projection"))?;
        Ok(Some(value))
    }

    /// Read committed semantic history in one stable SQLite query for canonical projection.
    /// This is deliberately independent of the per-device HTTP pagination cursor.
    #[cfg(test)]
    pub fn history(&self) -> Result<Vec<AuthorityEvent>, AuthorityError> {
        let mut statement = self.conn.prepare(
            "SELECT cursor,device_id,sequence,kind,wire FROM authority_history ORDER BY cursor",
        )?;
        let rows = statement.query_map([], |row| {
            Ok(AuthorityEvent {
                cursor: row.get(0)?,
                device_id: row.get(1)?,
                sequence: row.get(2)?,
                kind: row.get(3)?,
                wire: row.get(4)?,
            })
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }
    #[cfg(test)]
    pub fn generation(&self) -> Result<String, AuthorityError> {
        Ok(self.conn.query_row(
            "SELECT generation FROM authority_identity WHERE id=1",
            [],
            |row| row.get(0),
        )?)
    }
    pub fn tip(&self) -> Result<i64, AuthorityError> {
        Ok(self.conn.query_row(
            "SELECT COALESCE(MAX(cursor), 0) FROM authority_history",
            [],
            |row| row.get(0),
        )?)
    }
    #[cfg(test)]
    pub fn pull(&self, after: i64, limit: u32) -> Result<Vec<AuthorityEvent>, AuthorityError> {
        if after < 0 || limit == 0 || limit > 1000 {
            return Err(AuthorityError::Invalid("invalid cursor or limit"));
        }
        let mut query = self.conn.prepare("SELECT cursor,device_id,sequence,kind,wire FROM authority_history WHERE cursor>?1 ORDER BY cursor LIMIT ?2")?;
        let rows = query.query_map(params![after, limit], |row| {
            Ok(AuthorityEvent {
                cursor: row.get(0)?,
                device_id: row.get(1)?,
                sequence: row.get(2)?,
                kind: row.get(3)?,
                wire: row.get(4)?,
            })
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }
}

/// Resolve optimistic semantic field bases for task edits using accepted history.
/// A missing field is valid only for a previously untracked field.
fn latest_task_field_version(
    tx: &rusqlite::Transaction<'_>,
    task_id: &str,
    field: &str,
) -> Result<Option<orbit_sync::ChangeId>, AuthorityError> {
    use orbit_sync::{ChangeEnvelope, EntityKind};
    let mut stmt =
        tx.prepare("SELECT wire FROM authority_history WHERE kind='change' ORDER BY cursor DESC")?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
    for row in rows {
        let envelope =
            ChangeEnvelope::from_wire_json(&row?).map_err(|_| AuthorityError::MigrationConflict)?;
        if envelope.change.entity.kind == EntityKind::Task
            && envelope.change.entity.id == task_id
            && envelope
                .change
                .mutations
                .iter()
                .any(|mutation| mutation.field == field)
        {
            return Ok(Some(envelope.id));
        }
    }
    Ok(None)
}

#[cfg(test)]
#[path = "store_tests.rs"]
mod extracted_store_tests;
