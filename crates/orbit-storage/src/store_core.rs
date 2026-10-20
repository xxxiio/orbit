//! Internal SQLite persistence implementation; not a public API boundary.
use super::*;

impl SqliteStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, RepositoryError> {
        let connection = Connection::open(path).map_err(repo_error)?;
        let mut store = Self { connection };
        store.configure_connection(SqliteConcurrencyPolicy::default(), true)?;
        store.migrate()?;
        store.ensure_blueprint_schema()?;
        store.ensure_blueprint_work_item_schema()?;
        store.ensure_blueprint_actions_schema()?;
        store.ensure_blueprint_receipts_schema()?;
        Ok(store)
    }

    pub fn in_memory() -> Result<Self, RepositoryError> {
        let connection = Connection::open_in_memory().map_err(repo_error)?;
        let mut store = Self { connection };
        // WAL is not meaningful for SQLite in-memory databases. Preserve the
        // same timeout/foreign-key behavior while allowing SQLite's memory
        // journal mode for deterministic tests.
        store.configure_connection(SqliteConcurrencyPolicy::default(), false)?;
        store.migrate()?;
        store.ensure_blueprint_schema()?;
        store.ensure_blueprint_work_item_schema()?;
        store.ensure_blueprint_actions_schema()?;
        store.ensure_blueprint_receipts_schema()?;
        Ok(store)
    }

    /// Publish pre-sync metric records created by schema v11 clients into the
    /// durable semantic outbox. Opt-in to avoid unexpected sync traffic during
    /// database opening. The marker and all outbox writes commit atomically.
    /// The caller must initialize the local sync device first.
    pub fn backfill_legacy_metric_sync(
        &mut self,
        recorded_at_ms: i64,
    ) -> Result<usize, RepositoryError> {
        if self.sync_device()?.is_none() {
            return Err(RepositoryError::Other(
                "initialize sync before metric backfill".into(),
            ));
        }
        self.connection
            .execute_batch("BEGIN IMMEDIATE TRANSACTION")
            .map_err(repo_error)?;
        let result = (|| -> Result<usize, RepositoryError> {
            self.connection
                .execute_batch(
                    "CREATE TABLE IF NOT EXISTS orbit_local_backfills (name TEXT PRIMARY KEY)",
                )
                .map_err(repo_error)?;
            let already_done: bool = self.connection.query_row(
                "SELECT EXISTS(SELECT 1 FROM orbit_local_backfills WHERE name='metric_semantic_v11')",
                [], |row| row.get(0),
            ).map_err(repo_error)?;
            if already_done {
                return Ok(0);
            }
            let mut changes = Vec::new();
            {
                let mut stmt = self.connection.prepare(
                    "SELECT metric_id, baseline, target, direction FROM metric_targets ORDER BY metric_id"
                ).map_err(repo_error)?;
                let rows = stmt
                    .query_map([], |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, i64>(1)?,
                            row.get::<_, i64>(2)?,
                            row.get::<_, String>(3)?,
                        ))
                    })
                    .map_err(repo_error)?;
                for row in rows {
                    let (id, baseline, target, direction) = row.map_err(repo_error)?;
                    if direction != "at_least" && direction != "at_most" {
                        return Err(RepositoryError::CorruptData {
                            detail: "invalid legacy metric direction".into(),
                        });
                    }
                    changes.push(PendingSemanticChange::new(
                        SemanticEntityKind::Metric,
                        id,
                        vec![SemanticFieldMutation::new(
                            "target",
                            Some(format!("{baseline}:{target}:{direction}")),
                        )?],
                    )?);
                }
            }
            {
                let mut stmt = self.connection.prepare(
                    "SELECT metric_id, observed_at_ms, value FROM metric_observations ORDER BY metric_id, observed_at_ms"
                ).map_err(repo_error)?;
                let rows = stmt
                    .query_map([], |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, i64>(1)?,
                            row.get::<_, i64>(2)?,
                        ))
                    })
                    .map_err(repo_error)?;
                for row in rows {
                    let (id, at, value) = row.map_err(repo_error)?;
                    changes.push(PendingSemanticChange::new(
                        SemanticEntityKind::Metric,
                        id,
                        vec![SemanticFieldMutation::new(
                            format!("observation:{at}"),
                            Some(value.to_string()),
                        )?],
                    )?);
                }
            }
            let count = changes.len();
            for change in changes {
                self.enqueue_semantic_change(recorded_at_ms, change)?;
            }
            self.connection
                .execute(
                    "INSERT INTO orbit_local_backfills(name) VALUES ('metric_semantic_v11')",
                    [],
                )
                .map_err(repo_error)?;
            Ok(count)
        })();
        match result {
            Ok(count) => {
                if let Err(error) = self.connection.execute_batch("COMMIT") {
                    let _ = self.connection.execute_batch("ROLLBACK");
                    return Err(repo_error(error));
                }
                Ok(count)
            }
            Err(error) => {
                let _ = self.connection.execute_batch("ROLLBACK");
                Err(error)
            }
        }
    }

    pub fn concurrency_policy(&self) -> Result<SqliteConcurrencyPolicy, RepositoryError> {
        let journal_mode: String = self
            .connection
            .query_row("PRAGMA journal_mode", [], |row| row.get(0))
            .map_err(repo_error)?;
        let foreign_keys: i64 = self
            .connection
            .query_row("PRAGMA foreign_keys", [], |row| row.get(0))
            .map_err(repo_error)?;
        let busy_timeout_ms: i64 = self
            .connection
            .query_row("PRAGMA busy_timeout", [], |row| row.get(0))
            .map_err(repo_error)?;

        Ok(SqliteConcurrencyPolicy {
            busy_timeout_ms: busy_timeout_ms.max(0) as u64,
            wal_enabled: journal_mode.eq_ignore_ascii_case("wal"),
            foreign_keys_enabled: foreign_keys == 1,
        })
    }

    fn configure_connection(
        &mut self,
        policy: SqliteConcurrencyPolicy,
        allow_wal: bool,
    ) -> Result<(), RepositoryError> {
        self.connection
            .busy_timeout(std::time::Duration::from_millis(policy.busy_timeout_ms))
            .map_err(repo_error)?;
        self.connection
            .execute_batch(if policy.foreign_keys_enabled {
                "PRAGMA foreign_keys = ON;"
            } else {
                "PRAGMA foreign_keys = OFF;"
            })
            .map_err(repo_error)?;

        if policy.wal_enabled && allow_wal {
            let mode: String = self
                .connection
                .query_row("PRAGMA journal_mode = WAL", [], |row| row.get(0))
                .map_err(repo_error)?;
            if !mode.eq_ignore_ascii_case("wal") {
                return Err(RepositoryError::Unavailable);
            }
            self.connection
                .execute_batch("PRAGMA synchronous = NORMAL;")
                .map_err(repo_error)?;
        }

        Ok(())
    }

    pub fn schema_version(&self) -> Result<i64, RepositoryError> {
        self.connection
            .query_row("SELECT version FROM orbit_schema WHERE id = 1", [], |row| {
                row.get(0)
            })
            .map_err(repo_error)
    }

    /// Exports a validated point-in-time backup to `destination`.
    ///
    /// Backups are a portability/recovery artifact only. They are not sync
    /// payloads and must never be used by the sync engine as a merge unit.
    /// The destination must not already exist. A sibling temporary file is
    /// fully written and fsynced before it is atomically renamed into place.
    pub fn export_backup(&self, destination: impl AsRef<Path>) -> Result<PathBuf, RepositoryError> {
        let destination = destination.as_ref();
        if destination.exists() {
            return Err(RepositoryError::Conflict);
        }

        let report = self.integrity_report();
        if !report.is_healthy() {
            return Err(RepositoryError::CorruptData {
                detail: format!("refusing to export unhealthy database: {:?}", report.issues),
            });
        }

        let temporary = temporary_sibling(destination, "export");
        if temporary.exists() {
            fs::remove_file(&temporary).map_err(io_repo_error)?;
        }
        let temporary_text = temporary.to_string_lossy().into_owned();
        let result = self
            .connection
            .execute("VACUUM INTO ?1", [temporary_text.as_str()])
            .map_err(repo_error);
        if let Err(error) = result {
            let _ = fs::remove_file(&temporary);
            return Err(error);
        }

        let backup = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&temporary)
            .map_err(io_repo_error)?;
        backup.sync_all().map_err(io_repo_error)?;
        drop(backup);

        fs::rename(&temporary, destination).map_err(|error| {
            let _ = fs::remove_file(&temporary);
            io_repo_error(error)
        })?;
        sync_parent(destination)?;
        Ok(destination.to_path_buf())
    }

    /// Restores a backup into a new database path.
    ///
    /// The source backup is never modified. The destination must not already
    /// exist, which prevents an accidental overwrite of a live workspace. The
    /// copied snapshot is migrated and integrity-checked in a temporary sibling
    /// before becoming visible at the destination path.
    pub fn restore_backup(
        source: impl AsRef<Path>,
        destination: impl AsRef<Path>,
    ) -> Result<PathBuf, RepositoryError> {
        let source = source.as_ref();
        let destination = destination.as_ref();
        if destination.exists() {
            return Err(RepositoryError::Conflict);
        }
        if !source.is_file() {
            return Err(RepositoryError::Unavailable);
        }

        let temporary = temporary_sibling(destination, "restore");
        if temporary.exists() {
            fs::remove_file(&temporary).map_err(io_repo_error)?;
        }
        fs::copy(source, &temporary).map_err(io_repo_error)?;

        let validation = (|| -> Result<(), RepositoryError> {
            let store = SqliteStore::open(&temporary)?;
            let report = store.integrity_report();
            if !report.is_healthy() {
                return Err(RepositoryError::CorruptData {
                    detail: format!("backup failed integrity validation: {:?}", report.issues),
                });
            }
            drop(store);
            let file = fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(&temporary)
                .map_err(io_repo_error)?;
            file.sync_all().map_err(io_repo_error)
        })();

        if let Err(error) = validation {
            let _ = fs::remove_file(&temporary);
            return Err(error);
        }

        fs::rename(&temporary, destination).map_err(|error| {
            let _ = fs::remove_file(&temporary);
            io_repo_error(error)
        })?;
        sync_parent(destination)?;
        Ok(destination.to_path_buf())
    }

    /// Performs a read-only integrity assessment. This never mutates or repairs
    /// persisted state; recovery decisions remain explicit at a higher layer.
    pub fn integrity_report(&self) -> StorageIntegrityReport {
        let mut report = StorageIntegrityReport::default();

        match self
            .connection
            .query_row("PRAGMA integrity_check", [], |row| row.get::<_, String>(0))
        {
            Ok(result) if result == "ok" => {}
            Ok(result) => report.issues.push(StorageIntegrityIssue {
                area: "sqlite",
                detail: result,
            }),
            Err(error) => report.issues.push(StorageIntegrityIssue {
                area: "sqlite",
                detail: error.to_string(),
            }),
        }

        for (area, result) in [
            ("projects", ProjectRepository::list(self).map(|_| ())),
            ("tasks", TaskRepository::list(self).map(|_| ())),
            ("habits", HabitRepository::list(self).map(|_| ())),
            ("activities", ActivityRepository::list(self).map(|_| ())),
        ] {
            if let Err(error) = result {
                report.issues.push(StorageIntegrityIssue {
                    area,
                    detail: format!("{error:?}"),
                });
            }
        }

        // Milestones and schedule blocks have scoped/list-between APIs, so
        // validate their rows directly while reusing the canonical decoders.
        if let Err(error) = self.scan_all_milestones() {
            report.issues.push(StorageIntegrityIssue {
                area: "milestones",
                detail: format!("{error:?}"),
            });
        }
        if let Err(error) = self.scan_all_schedule_blocks() {
            report.issues.push(StorageIntegrityIssue {
                area: "schedule_blocks",
                detail: format!("{error:?}"),
            });
        }
        if let Err(error) = self.scan_all_habit_occurrences() {
            report.issues.push(StorageIntegrityIssue {
                area: "habit_occurrences",
                detail: format!("{error:?}"),
            });
        }
        if let Err(error) = RoutineRepository::list(self) {
            report.issues.push(StorageIntegrityIssue {
                area: "routines",
                detail: format!("{error:?}"),
            });
        }
        report
    }

    fn scan_all_milestones(&self) -> Result<Vec<Milestone>, RepositoryError> {
        let mut statement = self
            .connection
            .prepare("SELECT id, project_id, name, status FROM milestones ORDER BY id")
            .map_err(repo_error)?;
        collect_rows(
            statement
                .query_map([], milestone_from_row)
                .map_err(repo_error)?,
        )
    }

    fn scan_all_schedule_blocks(&self) -> Result<Vec<ScheduleBlock>, RepositoryError> {
        let mut statement = self.connection
            .prepare(r#"SELECT id, title, starts_at_ms, duration_minutes, flexibility, target_kind, target_id
                       FROM schedule_blocks ORDER BY id"#)
            .map_err(repo_error)?;
        collect_rows(
            statement
                .query_map([], schedule_from_row)
                .map_err(repo_error)?,
        )
    }

    fn scan_all_habit_occurrences(&self) -> Result<Vec<HabitOccurrence>, RepositoryError> {
        let mut statement = self.connection
            .prepare("SELECT id, habit_id, occurred_at_ms, duration_minutes FROM habit_occurrences ORDER BY habit_id, occurred_at_ms, id")
            .map_err(repo_error)?;
        collect_rows(
            statement
                .query_map([], habit_occurrence_from_row)
                .map_err(repo_error)?,
        )
    }

    pub(crate) fn migrate(&mut self) -> Result<(), RepositoryError> {
        // Bootstrap only the schema ledger first so we can fail closed when a
        // database was produced by a newer Orbit build.
        self.connection
            .execute_batch(
                r#"
                PRAGMA foreign_keys = ON;
                CREATE TABLE IF NOT EXISTS orbit_schema (
                    id INTEGER PRIMARY KEY CHECK (id = 1),
                    version INTEGER NOT NULL
                );
                INSERT OR IGNORE INTO orbit_schema(id, version) VALUES (1, 0);
                "#,
            )
            .map_err(repo_error)?;

        let existing_version: i64 = self
            .connection
            .query_row("SELECT version FROM orbit_schema WHERE id = 1", [], |row| {
                row.get(0)
            })
            .map_err(repo_error)?;

        // M19: the Markdown vault is authoritative. A database with SQL-backed
        // knowledge belongs to the retired architecture. Never copy, silently
        // discard, or allow parallel mutation of that content.
        let retired_knowledge_count: i64 = self
            .connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type IN ('table','view') AND name IN \
             ('knowledge_items','knowledge_relations','knowledge_resources',\
              'knowledge_resource_local_locators','knowledge_fts')",
                [],
                |row| row.get(0),
            )
            .map_err(repo_error)?;
        if retired_knowledge_count != 0 {
            return Err(RepositoryError::Other(
                "retired SQL knowledge schema detected; create a fresh Orbit workspace and import Markdown files".into(),
            ));
        }

        if existing_version > SCHEMA_VERSION {
            return Err(RepositoryError::Other(format!(
                "database schema version {existing_version} is newer than supported version {SCHEMA_VERSION}"
            )));
        }

        if existing_version == SCHEMA_VERSION {
            return Ok(());
        }

        if (13..=18).contains(&existing_version) {
            let transaction = self.connection.transaction().map_err(repo_error)?;
            transaction
                .execute_batch(agent_jobs::AGENT_JOB_SCHEMA)
                .map_err(repo_error)?;
            transaction
                .execute("UPDATE orbit_schema SET version = 19 WHERE id = 1", [])
                .map_err(repo_error)?;
            return transaction.commit().map_err(repo_error);
        }
        let transaction = self.connection.transaction().map_err(repo_error)?;
        transaction
            .execute_batch(
                r#"
                CREATE TABLE IF NOT EXISTS projects (
                    id TEXT PRIMARY KEY NOT NULL,
                    name TEXT NOT NULL,
                    status TEXT NOT NULL
                );

                CREATE TABLE IF NOT EXISTS milestones (
                    id TEXT PRIMARY KEY NOT NULL,
                    project_id TEXT NOT NULL,
                    name TEXT NOT NULL,
                    status TEXT NOT NULL,
                    FOREIGN KEY(project_id) REFERENCES projects(id)
                );

                CREATE INDEX IF NOT EXISTS idx_milestones_project
                    ON milestones(project_id);

                CREATE TABLE IF NOT EXISTS tasks (
                    id TEXT PRIMARY KEY NOT NULL,
                    title TEXT NOT NULL,
                    status TEXT NOT NULL,
                    project_id TEXT,
                    milestone_id TEXT,
                    CHECK (milestone_id IS NULL OR project_id IS NOT NULL)
                );

                CREATE INDEX IF NOT EXISTS idx_tasks_project
                    ON tasks(project_id);
                CREATE INDEX IF NOT EXISTS idx_tasks_milestone
                    ON tasks(milestone_id);

                CREATE TABLE IF NOT EXISTS habits (
                    id TEXT PRIMARY KEY NOT NULL,
                    name TEXT NOT NULL,
                    cadence_kind TEXT NOT NULL,
                    target_per_week INTEGER,
                    active INTEGER NOT NULL CHECK(active IN (0, 1))
                );

                CREATE TABLE IF NOT EXISTS routines (
                    id TEXT PRIMARY KEY NOT NULL,
                    name TEXT NOT NULL
                );

                CREATE TABLE IF NOT EXISTS routine_habits (
                    routine_id TEXT NOT NULL,
                    habit_id TEXT NOT NULL,
                    position INTEGER NOT NULL CHECK(position >= 0),
                    PRIMARY KEY(routine_id, habit_id),
                    UNIQUE(routine_id, position),
                    FOREIGN KEY(routine_id) REFERENCES routines(id) ON DELETE CASCADE,
                    FOREIGN KEY(habit_id) REFERENCES habits(id)
                );

                CREATE INDEX IF NOT EXISTS idx_routine_habits_routine_position
                    ON routine_habits(routine_id, position);

                CREATE TABLE IF NOT EXISTS habit_occurrences (
                    id TEXT PRIMARY KEY NOT NULL,
                    habit_id TEXT NOT NULL,
                    occurred_at_ms INTEGER NOT NULL,
                    duration_minutes INTEGER,
                    CHECK(duration_minutes IS NULL OR duration_minutes > 0),
                    FOREIGN KEY(habit_id) REFERENCES habits(id)
                );

                CREATE INDEX IF NOT EXISTS idx_habit_occurrences_habit_time
                    ON habit_occurrences(habit_id, occurred_at_ms, id);

                CREATE TABLE IF NOT EXISTS schedule_blocks (
                    id TEXT PRIMARY KEY NOT NULL,
                    title TEXT NOT NULL,
                    starts_at_ms INTEGER NOT NULL,
                    duration_minutes INTEGER NOT NULL CHECK(duration_minutes > 0),
                    flexibility TEXT NOT NULL,
                    target_kind TEXT NOT NULL,
                    target_id TEXT
                );

                CREATE INDEX IF NOT EXISTS idx_schedule_start
                    ON schedule_blocks(starts_at_ms);

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

                CREATE INDEX IF NOT EXISTS idx_activities_occurred
                    ON activities(occurred_at_ms, sequence);
                CREATE INDEX IF NOT EXISTS idx_activities_subject
                    ON activities(subject_kind, subject_id, sequence);
                "#,
            )
            .map_err(repo_error)?;
        if existing_version < 2 {
            transaction
                .execute_batch(
                    r#"
                    ALTER TABLE tasks ADD COLUMN estimated_minutes INTEGER;
                    ALTER TABLE tasks ADD COLUMN earliest_start_at_ms INTEGER;
                    ALTER TABLE tasks ADD COLUMN deadline_at_ms INTEGER;
                    "#,
                )
                .map_err(repo_error)?;
        }
        if existing_version < 4 {
            transaction
                .execute_batch(
                    r#"
                    CREATE TABLE IF NOT EXISTS routines (
                        id TEXT PRIMARY KEY NOT NULL,
                        name TEXT NOT NULL
                    );
                    CREATE TABLE IF NOT EXISTS routine_habits (
                        routine_id TEXT NOT NULL,
                        habit_id TEXT NOT NULL,
                        position INTEGER NOT NULL CHECK(position >= 0),
                        PRIMARY KEY(routine_id, habit_id),
                        UNIQUE(routine_id, position),
                        FOREIGN KEY(routine_id) REFERENCES routines(id) ON DELETE CASCADE,
                        FOREIGN KEY(habit_id) REFERENCES habits(id)
                    );
                    CREATE INDEX IF NOT EXISTS idx_routine_habits_routine_position
                        ON routine_habits(routine_id, position);
                    "#,
                )
                .map_err(repo_error)?;
        }
        if existing_version < 5 {
            transaction
                .execute_batch(
                    r#"
                    CREATE TABLE IF NOT EXISTS sync_local_state (
                        id INTEGER PRIMARY KEY CHECK(id = 1),
                        device_id TEXT NOT NULL UNIQUE,
                        next_sequence INTEGER NOT NULL CHECK(next_sequence > 0)
                    );
                    CREATE TABLE IF NOT EXISTS sync_outbox (
                        device_id TEXT NOT NULL,
                        sequence INTEGER NOT NULL CHECK(sequence > 0),
                        recorded_at_ms INTEGER NOT NULL,
                        entity_kind TEXT NOT NULL,
                        entity_id TEXT NOT NULL,
                        exported_at_ms INTEGER,
                        PRIMARY KEY(device_id, sequence)
                    );
                    CREATE TABLE IF NOT EXISTS sync_outbox_fields (
                        device_id TEXT NOT NULL,
                        sequence INTEGER NOT NULL,
                        position INTEGER NOT NULL CHECK(position >= 0),
                        field TEXT NOT NULL,
                        value TEXT,
                        base_device_id TEXT,
                        base_sequence INTEGER,
                        CHECK ((base_device_id IS NULL AND base_sequence IS NULL) OR (base_device_id IS NOT NULL AND base_sequence > 0)),
                        PRIMARY KEY(device_id, sequence, position),
                        UNIQUE(device_id, sequence, field),
                        FOREIGN KEY(device_id, sequence) REFERENCES sync_outbox(device_id, sequence) ON DELETE CASCADE
                    );
                    CREATE TABLE IF NOT EXISTS sync_field_versions (
                        entity_kind TEXT NOT NULL,
                        entity_id TEXT NOT NULL,
                        field TEXT NOT NULL,
                        device_id TEXT NOT NULL,
                        sequence INTEGER NOT NULL CHECK(sequence > 0),
                        PRIMARY KEY(entity_kind, entity_id, field)
                    );
                    CREATE INDEX IF NOT EXISTS idx_sync_outbox_pending
                        ON sync_outbox(exported_at_ms, device_id, sequence);
                    "#,
                )
                .map_err(repo_error)?;
        }
        if existing_version < 9 {
            transaction
                .execute_batch(
                    r#"
                    CREATE TABLE IF NOT EXISTS sync_resolution_outbox (
                        device_id TEXT NOT NULL,
                        sequence INTEGER NOT NULL CHECK(sequence > 0),
                        recorded_at_ms INTEGER NOT NULL,
                        conflict_id_json TEXT NOT NULL,
                        value TEXT,
                        exported_at_ms INTEGER,
                        PRIMARY KEY(device_id, sequence)
                    );
                    CREATE INDEX IF NOT EXISTS idx_sync_resolution_outbox_pending
                        ON sync_resolution_outbox(exported_at_ms, device_id, sequence);
                    "#,
                )
                .map_err(repo_error)?;
        }
        // Retired v6 SQL knowledge storage is not created in fresh workspaces.
        // Retired v7 SQL knowledge storage is not created in fresh workspaces.
        if existing_version < 8 {
            transaction
                .execute_batch(
                    r#"
                    ALTER TABLE projects ADD COLUMN mode TEXT NOT NULL DEFAULT 'standard';
                    ALTER TABLE projects ADD COLUMN objective TEXT;
                    ALTER TABLE projects ADD COLUMN progress_signals TEXT NOT NULL DEFAULT 'milestones,tasks,blockers,forecast';
                    CREATE TABLE IF NOT EXISTS project_success_criteria (
                        project_id TEXT NOT NULL,
                        position INTEGER NOT NULL CHECK(position >= 0),
                        criterion_key TEXT NOT NULL,
                        description TEXT NOT NULL,
                        required INTEGER NOT NULL CHECK(required IN (0, 1)),
                        status TEXT NOT NULL,
                        PRIMARY KEY(project_id, position),
                        UNIQUE(project_id, criterion_key),
                        FOREIGN KEY(project_id) REFERENCES projects(id) ON DELETE CASCADE
                    );
                    CREATE INDEX IF NOT EXISTS idx_project_success_criteria_project
                        ON project_success_criteria(project_id, position);
                    "#,
                )
                .map_err(repo_error)?;
        }
        if existing_version < 10 {
            transaction
                .execute_batch(
                    r#"
                CREATE TABLE IF NOT EXISTS project_evidence_bindings (
                    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
                    evidence_kind TEXT NOT NULL CHECK(evidence_kind IN ('habit', 'metric')),
                    evidence_id TEXT NOT NULL CHECK(length(trim(evidence_id)) > 0),
                    PRIMARY KEY(project_id, evidence_kind, evidence_id)
                );
                CREATE INDEX IF NOT EXISTS idx_project_evidence_kind
                    ON project_evidence_bindings(evidence_kind, evidence_id);
            "#,
                )
                .map_err(repo_error)?;
        }
        if existing_version < 11 {
            transaction
                .execute_batch(
                    r#"
                CREATE TABLE metric_targets (
                    metric_id TEXT PRIMARY KEY NOT NULL CHECK(length(trim(metric_id)) > 0),
                    baseline INTEGER NOT NULL,
                    target INTEGER NOT NULL,
                    direction TEXT NOT NULL CHECK(direction IN ('at_least','at_most'))
                );
                CREATE TABLE metric_observations (
                    metric_id TEXT NOT NULL REFERENCES metric_targets(metric_id) ON DELETE CASCADE,
                    observed_at_ms INTEGER NOT NULL,
                    value INTEGER NOT NULL,
                    PRIMARY KEY(metric_id, observed_at_ms)
                );
            "#,
                )
                .map_err(repo_error)?;
        }
        transaction
            .execute_batch(agent_jobs::AGENT_JOB_SCHEMA)
            .map_err(repo_error)?;
        // Retired v12 SQL knowledge storage is not created in fresh workspaces.
        transaction
            .execute(
                "UPDATE orbit_schema SET version = ?1 WHERE id = 1",
                [SCHEMA_VERSION],
            )
            .map_err(repo_error)?;
        transaction.commit().map_err(repo_error)
    }
}
