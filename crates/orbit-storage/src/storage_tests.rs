//! Storage regression suites grouped outside the public crate entry point.
use super::*;

#[cfg(test)]
mod authority_transaction_tests {
    use super::*;

    fn rows(store: &SqliteStore) -> i64 {
        store
            .query_row("SELECT COUNT(*) FROM projects", [], |r| r.get(0))
            .unwrap()
    }

    fn insert(store: &mut SqliteStore, id: &str) -> Result<(), ApplicationError> {
        store
            .execute(
                "INSERT INTO projects(id,name,status) VALUES (?1,'Transaction test','proposed')",
                [id],
            )
            .map_err(repo_error)?;
        Ok(())
    }

    #[test]
    fn standalone_application_transaction_commits() {
        let mut db = SqliteStore::in_memory().unwrap();
        db.transact(|db| insert(db, "standalone")).unwrap();
        assert_eq!(rows(&db), 1);
        assert!(db.is_autocommit());
    }

    #[test]
    fn standalone_application_failure_rolls_back() {
        let mut db = SqliteStore::in_memory().unwrap();
        let result: Result<(), ApplicationError> = db.transact(|db| {
            insert(db, "cancelled")?;
            Err(ApplicationError::Repository(RepositoryError::Other(
                "reject".into(),
            )))
        });
        assert!(result.is_err());
        assert_eq!(rows(&db), 0);
        assert!(db.is_autocommit());
    }

    #[test]
    fn rich_application_success_is_not_committed_before_authority_commit() {
        let mut db = SqliteStore::in_memory().unwrap();
        db.execute_batch("BEGIN IMMEDIATE").unwrap();
        db.transact(|db| insert(db, "pending")).unwrap();
        assert_eq!(rows(&db), 1);
        assert!(!db.is_autocommit());
        db.execute_batch("ROLLBACK").unwrap();
        assert_eq!(rows(&db), 0);
    }

    #[test]
    fn failing_rich_application_rolls_back_only_its_savepoint() {
        let mut db = SqliteStore::in_memory().unwrap();
        db.execute_batch("BEGIN IMMEDIATE").unwrap();
        insert(&mut db, "before").unwrap();
        let result: Result<(), ApplicationError> = db.transact(|db| {
            insert(db, "failed")?;
            Err(ApplicationError::Repository(RepositoryError::Other(
                "fail".into(),
            )))
        });
        assert!(result.is_err());
        assert!(!db.is_autocommit());
        assert_eq!(rows(&db), 1);
        db.transact(|db| insert(db, "after")).unwrap();
        db.execute_batch("COMMIT").unwrap();
        assert_eq!(rows(&db), 2);
    }

    #[test]
    fn deeply_nested_application_transactions_are_isolated() {
        let mut db = SqliteStore::in_memory().unwrap();
        db.transact(|db| {
            insert(db, "outer")?;
            db.transact(|db| insert(db, "inner"))?;
            let result: Result<(), ApplicationError> = db.transact(|db| {
                insert(db, "discard")?;
                Err(ApplicationError::Repository(RepositoryError::Other(
                    "fail".into(),
                )))
            });
            assert!(result.is_err());
            assert_eq!(rows(db), 2);
            Ok(())
        })
        .unwrap();
        assert_eq!(rows(&db), 2);
        assert!(db.is_autocommit());
    }

    #[test]
    fn outer_failure_rolls_back_inner_success() {
        let mut db = SqliteStore::in_memory().unwrap();
        let result: Result<(), ApplicationError> = db.transact(|db| {
            db.transact(|db| insert(db, "inner"))?;
            Err(ApplicationError::Repository(RepositoryError::Other(
                "outer reject".into(),
            )))
        });
        assert!(result.is_err());
        assert_eq!(rows(&db), 0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn task_id(value: &str) -> TaskId {
        TaskId::new(value).expect("valid task id")
    }

    #[test]
    fn migrations_are_applied_on_open() {
        let store = SqliteStore::in_memory().expect("open database");
        assert_eq!(
            store.schema_version().expect("schema version"),
            SCHEMA_VERSION
        );
    }

    #[test]
    fn standalone_task_round_trips_with_status() {
        let mut store = SqliteStore::in_memory().expect("open database");
        let mut task = Task::new(task_id("task-1"), "Renew passport", TaskScope::standalone())
            .expect("valid task");
        task.transition_to(TaskStatus::Ready).expect("ready");
        task.transition_to(TaskStatus::InProgress)
            .expect("in progress");

        TaskRepository::save(&mut store, task.clone()).expect("save task");
        let restored = TaskRepository::get(&store, task.id())
            .expect("read task")
            .expect("task exists");

        assert_eq!(restored, task);
    }

    #[test]
    fn project_milestone_and_scoped_task_round_trip() {
        let mut store = SqliteStore::in_memory().expect("open database");
        let project =
            Project::new(ProjectId::new("project-1").unwrap(), "Study JLPT").expect("project");
        ProjectRepository::save(&mut store, project.clone()).expect("save project");
        let milestone = Milestone::new(
            MilestoneId::new("milestone-1").unwrap(),
            project.id().clone(),
            "Grammar",
        )
        .expect("milestone");
        MilestoneRepository::save(&mut store, milestone.clone()).expect("save milestone");
        let task = Task::new(
            task_id("task-1"),
            "Chapter 14",
            TaskScope::in_milestone(project.id().clone(), milestone.id().clone()),
        )
        .expect("task");
        TaskRepository::save(&mut store, task.clone()).expect("save task");

        assert_eq!(
            ProjectRepository::get(&store, project.id()).unwrap(),
            Some(project)
        );
        assert_eq!(
            MilestoneRepository::get(&store, milestone.id()).unwrap(),
            Some(milestone)
        );
        assert_eq!(TaskRepository::get(&store, task.id()).unwrap(), Some(task));
    }

    #[test]
    fn habits_and_overlapping_schedule_blocks_round_trip() {
        let mut store = SqliteStore::in_memory().expect("open database");
        let mut habit = Habit::new(
            HabitId::new("habit-1").unwrap(),
            "Exercise",
            HabitCadence::Weekly { target_per_week: 4 },
        )
        .expect("habit");
        habit.pause();
        HabitRepository::save(&mut store, habit.clone()).expect("save habit");

        let block = ScheduleBlock::new(
            ScheduleBlockId::new("block-1").unwrap(),
            "Run",
            1_000,
            30,
            ScheduleFlexibility::Movable,
            ScheduleTarget::Habit(habit.id().clone()),
        )
        .expect("block");
        ScheduleRepository::save(&mut store, block.clone()).expect("save block");

        assert_eq!(
            HabitRepository::get(&store, habit.id()).unwrap(),
            Some(habit)
        );
        assert_eq!(
            ScheduleRepository::list_between(&store, 0, 2_000).unwrap(),
            vec![block]
        );
    }

    #[test]
    fn activity_journal_is_append_only_and_ordered() {
        let mut store = SqliteStore::in_memory().expect("open database");
        let first = ActivityRecord::new(
            ActivityId::new("activity-1").unwrap(),
            ActivitySubject::Task(task_id("task-1")),
            ActivityKind::Created,
            ActivityOrigin::Human,
            100,
            "Created task",
        )
        .expect("activity");
        let second = ActivityRecord::new(
            ActivityId::new("activity-2").unwrap(),
            ActivitySubject::Task(task_id("task-1")),
            ActivityKind::StatusChanged,
            ActivityOrigin::Agent("planner".to_owned()),
            100,
            "Moved task to ready",
        )
        .expect("activity");

        ActivityRepository::append(&mut store, first.clone()).expect("append first");
        ActivityRepository::append(&mut store, second.clone()).expect("append second");
        assert_eq!(
            ActivityRepository::list(&store).unwrap(),
            vec![first.clone(), second]
        );
        assert_eq!(
            ActivityRepository::append(&mut store, first),
            Err(RepositoryError::Conflict)
        );
    }

    struct FixedClock(i64);
    impl orbit_application::Clock for FixedClock {
        fn now_ms(&self) -> i64 {
            self.0
        }
    }

    #[test]
    fn task_transition_and_activity_append_commit_atomically() {
        let mut store = SqliteStore::in_memory().expect("open database");
        let mut task = Task::new(
            task_id("task-atomic"),
            "Atomic transition",
            TaskScope::standalone(),
        )
        .expect("valid task");
        task.transition_to(TaskStatus::Ready).expect("ready");
        TaskRepository::save(&mut store, task.clone()).expect("save task");

        let updated = orbit_application::transition_task_with_activity(
            &mut store,
            &FixedClock(42),
            orbit_application::TransitionTaskWithActivityCommand {
                task: orbit_application::TransitionTaskCommand {
                    id: task.id().clone(),
                    status: TaskStatus::InProgress,
                },
                activity_id: ActivityId::new("activity-atomic").unwrap(),
                origin: ActivityOrigin::Human,
                summary: "Started task".to_owned(),
            },
        )
        .expect("atomic transition");

        assert_eq!(updated.status(), TaskStatus::InProgress);
        assert_eq!(
            TaskRepository::get(&store, task.id())
                .unwrap()
                .unwrap()
                .status(),
            TaskStatus::InProgress
        );
        let activities = ActivityRepository::list(&store).unwrap();
        assert_eq!(activities.len(), 1);
        assert_eq!(activities[0].summary(), "Started task");
    }

    #[test]
    fn activity_failure_rolls_back_task_mutation() {
        let mut store = SqliteStore::in_memory().expect("open database");
        let mut task = Task::new(
            task_id("task-rollback"),
            "Rollback transition",
            TaskScope::standalone(),
        )
        .expect("valid task");
        task.transition_to(TaskStatus::Ready).expect("ready");
        TaskRepository::save(&mut store, task.clone()).expect("save task");

        store
            .connection
            .execute_batch(
                r#"
                CREATE TRIGGER fail_activity_insert
                BEFORE INSERT ON activities
                BEGIN
                    SELECT RAISE(ABORT, 'forced activity failure');
                END;
                "#,
            )
            .expect("install failure trigger");

        let result = orbit_application::transition_task_with_activity(
            &mut store,
            &FixedClock(42),
            orbit_application::TransitionTaskWithActivityCommand {
                task: orbit_application::TransitionTaskCommand {
                    id: task.id().clone(),
                    status: TaskStatus::InProgress,
                },
                activity_id: ActivityId::new("activity-rollback").unwrap(),
                origin: ActivityOrigin::System,
                summary: "Should roll back".to_owned(),
            },
        );

        assert!(matches!(result, Err(ApplicationError::Repository(_))));
        assert_eq!(
            TaskRepository::get(&store, task.id())
                .unwrap()
                .unwrap()
                .status(),
            TaskStatus::Ready
        );
        assert!(ActivityRepository::list(&store).unwrap().is_empty());
    }

    #[test]
    fn newer_schema_versions_fail_closed() {
        let connection = Connection::open_in_memory().expect("open database");
        connection
            .execute_batch(
                r#"
                CREATE TABLE orbit_schema (
                    id INTEGER PRIMARY KEY CHECK (id = 1),
                    version INTEGER NOT NULL
                );
                INSERT INTO orbit_schema(id, version) VALUES (1, 999);
                "#,
            )
            .expect("seed future schema");
        let mut store = SqliteStore { connection };

        let result = store.migrate();
        assert!(
            matches!(result, Err(RepositoryError::Other(message)) if message.contains("newer than supported"))
        );
        assert_eq!(store.schema_version().unwrap(), 999);
    }

    #[test]
    fn all_task_statuses_round_trip() {
        let statuses = [
            TaskStatus::Inbox,
            TaskStatus::Planned,
            TaskStatus::Ready,
            TaskStatus::InProgress,
            TaskStatus::Waiting,
            TaskStatus::Blocked,
            TaskStatus::Review,
            TaskStatus::Deferred,
            TaskStatus::Done,
            TaskStatus::Cancelled,
        ];

        for (index, status) in statuses.into_iter().enumerate() {
            let store = SqliteStore::in_memory().expect("open database");
            let id = task_id(&format!("task-{index}"));
            store.connection.execute(
                "INSERT INTO tasks(id, title, status, project_id, milestone_id) VALUES (?1, 'x', ?2, NULL, NULL)",
                params![id.as_str(), status.as_str()],
            ).expect("seed task");
            let restored = TaskRepository::get(&store, &id)
                .expect("read task")
                .expect("exists");
            assert_eq!(restored.status(), status);
        }
    }

    #[test]
    fn all_project_and_milestone_statuses_round_trip() {
        for (index, status) in [
            ProjectStatus::Proposed,
            ProjectStatus::Active,
            ProjectStatus::OnHold,
            ProjectStatus::Completed,
            ProjectStatus::Cancelled,
            ProjectStatus::Archived,
        ]
        .into_iter()
        .enumerate()
        {
            let store = SqliteStore::in_memory().expect("open database");
            let id = ProjectId::new(format!("project-{index}")).unwrap();
            store
                .connection
                .execute(
                    "INSERT INTO projects(id, name, status) VALUES (?1, 'p', ?2)",
                    params![id.as_str(), status.as_str()],
                )
                .unwrap();
            assert_eq!(
                ProjectRepository::get(&store, &id)
                    .unwrap()
                    .unwrap()
                    .status(),
                status
            );
        }

        for (index, status) in [
            MilestoneStatus::Planned,
            MilestoneStatus::InProgress,
            MilestoneStatus::Blocked,
            MilestoneStatus::Completed,
            MilestoneStatus::Cancelled,
        ]
        .into_iter()
        .enumerate()
        {
            let store = SqliteStore::in_memory().expect("open database");
            let project_id = ProjectId::new("project-m").unwrap();
            store
                .connection
                .execute(
                    "INSERT INTO projects(id, name, status) VALUES (?1, 'p', 'proposed')",
                    [project_id.as_str()],
                )
                .unwrap();
            let id = MilestoneId::new(format!("milestone-{index}")).unwrap();
            store
                .connection
                .execute(
                    "INSERT INTO milestones(id, project_id, name, status) VALUES (?1, ?2, 'm', ?3)",
                    params![id.as_str(), project_id.as_str(), status.as_str()],
                )
                .unwrap();
            assert_eq!(
                MilestoneRepository::get(&store, &id)
                    .unwrap()
                    .unwrap()
                    .status(),
                status
            );
        }
    }

    #[test]
    fn corrupt_rows_are_reported_explicitly_and_never_repaired() {
        let store = SqliteStore::in_memory().expect("open database");
        store.connection.execute(
            "INSERT INTO tasks(id, title, status, project_id, milestone_id) VALUES ('bad-task', 'x', 'not-a-status', NULL, NULL)",
            [],
        ).unwrap();

        let error = TaskRepository::get(&store, &TaskId::new("bad-task").unwrap()).unwrap_err();
        assert!(matches!(error, RepositoryError::CorruptData { .. }));

        let report = store.integrity_report();
        assert!(!report.is_healthy());
        assert!(report.issues.iter().any(|issue| issue.area == "tasks"));

        let persisted: String = store
            .connection
            .query_row(
                "SELECT status FROM tasks WHERE id = 'bad-task'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(persisted, "not-a-status");
    }

    fn unique_temp_path(label: &str) -> std::path::PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(1);
        std::env::temp_dir().join(format!(
            "orbit-{label}-{}-{}.db",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ))
    }

    fn cleanup_sqlite_files(path: &std::path::Path) {
        let _ = fs::remove_file(path);
        let _ = fs::remove_file(format!("{}-wal", path.display()));
        let _ = fs::remove_file(format!("{}-shm", path.display()));
    }

    #[test]
    fn backup_export_and_restore_round_trip_without_mutating_source() {
        let live_path = unique_temp_path("live");
        let backup_path = unique_temp_path("backup");
        let restored_path = unique_temp_path("restored");

        {
            let mut store = SqliteStore::open(&live_path).expect("open live database");
            let task = Task::new(
                task_id("task-backup"),
                "Portable backup",
                TaskScope::standalone(),
            )
            .unwrap();
            TaskRepository::save(&mut store, task).unwrap();
            assert_eq!(store.export_backup(&backup_path).unwrap(), backup_path);
        }

        let source_len_before = fs::metadata(&backup_path).unwrap().len();
        assert_eq!(
            SqliteStore::restore_backup(&backup_path, &restored_path).unwrap(),
            restored_path
        );
        let source_len_after = fs::metadata(&backup_path).unwrap().len();
        assert_eq!(source_len_before, source_len_after);

        let restored = SqliteStore::open(&restored_path).expect("open restored database");
        assert!(
            TaskRepository::get(&restored, &task_id("task-backup"))
                .unwrap()
                .is_some()
        );
        assert!(restored.integrity_report().is_healthy());

        let _ = fs::remove_file(live_path);
        let _ = fs::remove_file(backup_path);
        let _ = fs::remove_file(restored_path);
    }

    #[test]
    fn backup_operations_never_overwrite_existing_destinations() {
        let live_path = unique_temp_path("live-conflict");
        let backup_path = unique_temp_path("backup-conflict");
        let restored_path = unique_temp_path("restore-conflict");

        let store = SqliteStore::open(&live_path).unwrap();
        fs::write(&backup_path, b"existing").unwrap();
        assert_eq!(
            store.export_backup(&backup_path),
            Err(RepositoryError::Conflict)
        );

        let clean_backup = unique_temp_path("clean-backup");
        store.export_backup(&clean_backup).unwrap();
        fs::write(&restored_path, b"existing").unwrap();
        assert_eq!(
            SqliteStore::restore_backup(&clean_backup, &restored_path),
            Err(RepositoryError::Conflict)
        );

        let _ = fs::remove_file(live_path);
        let _ = fs::remove_file(backup_path);
        let _ = fs::remove_file(clean_backup);
        let _ = fs::remove_file(restored_path);
    }

    #[test]
    fn restore_rejects_future_schema_and_leaves_destination_absent() {
        let source = unique_temp_path("future-backup");
        let destination = unique_temp_path("future-restore");
        {
            let connection = Connection::open(&source).unwrap();
            connection
                .execute_batch(
                    r#"
                    CREATE TABLE orbit_schema (
                        id INTEGER PRIMARY KEY CHECK (id = 1),
                        version INTEGER NOT NULL
                    );
                    INSERT INTO orbit_schema(id, version) VALUES (1, 999);
                    "#,
                )
                .unwrap();
        }

        let result = SqliteStore::restore_backup(&source, &destination);
        assert!(matches!(
            result,
            Err(RepositoryError::Other(message)) if message.contains("newer than supported")
        ));
        assert!(!destination.exists());

        let version: i64 = Connection::open(&source)
            .unwrap()
            .query_row("SELECT version FROM orbit_schema WHERE id = 1", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(version, 999);
        let _ = fs::remove_file(source);
    }

    #[test]
    fn file_backed_connections_apply_shared_workspace_policy() {
        let path = unique_temp_path("concurrency-policy");
        let store = SqliteStore::open(&path).expect("open database");
        let policy = store.concurrency_policy().expect("read policy");
        assert_eq!(policy.busy_timeout_ms, DEFAULT_BUSY_TIMEOUT_MS);
        assert!(policy.wal_enabled);
        assert!(policy.foreign_keys_enabled);
        drop(store);
        cleanup_sqlite_files(&path);
    }

    #[test]
    fn in_memory_connections_keep_timeout_and_foreign_keys_without_requiring_wal() {
        let store = SqliteStore::in_memory().expect("open database");
        let policy = store.concurrency_policy().expect("read policy");
        assert_eq!(policy.busy_timeout_ms, DEFAULT_BUSY_TIMEOUT_MS);
        assert!(!policy.wal_enabled);
        assert!(policy.foreign_keys_enabled);
    }

    #[test]
    fn concurrent_writer_waits_for_bounded_timeout_instead_of_spinning() {
        let path = unique_temp_path("busy-timeout");
        let first = SqliteStore::open(&path).expect("open first connection");
        let second = SqliteStore::open(&path).expect("open second connection");

        first.connection.execute_batch("BEGIN IMMEDIATE;").unwrap();
        let start = std::time::Instant::now();
        let result = second.connection.execute(
            "INSERT INTO projects(id, name, status) VALUES ('writer-two', 'x', 'proposed')",
            [],
        );
        let elapsed = start.elapsed();
        first.connection.execute_batch("ROLLBACK;").unwrap();

        assert!(result.is_err());
        assert!(elapsed >= std::time::Duration::from_millis(4_000));
        assert!(elapsed < std::time::Duration::from_secs(8));
        drop(second);
        drop(first);
        cleanup_sqlite_files(&path);
    }

    #[test]
    fn active_flags_and_schedule_target_variants_round_trip() {
        let mut store = SqliteStore::in_memory().expect("open database");
        for active in [true, false] {
            let mut habit = Habit::new(
                HabitId::new(if active {
                    "habit-active"
                } else {
                    "habit-paused"
                })
                .unwrap(),
                "Exercise",
                HabitCadence::Daily,
            )
            .unwrap();
            if !active {
                habit.pause();
            }
            HabitRepository::save(&mut store, habit.clone()).unwrap();
            assert_eq!(
                HabitRepository::get(&store, habit.id()).unwrap(),
                Some(habit)
            );
        }

        let task = Task::new(task_id("task-target"), "Target", TaskScope::standalone()).unwrap();
        TaskRepository::save(&mut store, task.clone()).unwrap();
        let habit = Habit::new(
            HabitId::new("habit-target").unwrap(),
            "Run",
            HabitCadence::Daily,
        )
        .unwrap();
        HabitRepository::save(&mut store, habit.clone()).unwrap();
        let targets = [
            ScheduleTarget::Standalone,
            ScheduleTarget::Task(task.id().clone()),
            ScheduleTarget::Habit(habit.id().clone()),
        ];
        for (index, target) in targets.into_iter().enumerate() {
            let block = ScheduleBlock::new(
                ScheduleBlockId::new(format!("block-{index}")).unwrap(),
                "Block",
                index as i64 * 10_000,
                30,
                if index % 2 == 0 {
                    ScheduleFlexibility::Fixed
                } else {
                    ScheduleFlexibility::Movable
                },
                target,
            )
            .unwrap();
            ScheduleRepository::save(&mut store, block.clone()).unwrap();
            assert_eq!(
                ScheduleRepository::get(&store, block.id()).unwrap(),
                Some(block)
            );
        }
    }

    #[test]
    fn application_create_and_start_commands_work_against_one_sqlite_unit_of_work() {
        use orbit_application::{
            CreateTaskCommand, CreateTaskWithActivityCommand, StartTaskWithActivityCommand,
            create_task_with_activity, start_task_with_activity,
        };

        struct FixedClock;
        impl orbit_application::Clock for FixedClock {
            fn now_ms(&self) -> i64 {
                42
            }
        }

        let mut store = SqliteStore::in_memory().expect("open database");
        let task_id = TaskId::new("task-cli").unwrap();
        let created = create_task_with_activity(
            &mut store,
            &FixedClock,
            CreateTaskWithActivityCommand {
                task: CreateTaskCommand {
                    id: task_id.clone(),
                    title: "CLI vertical slice".to_owned(),
                    scope: TaskScope::standalone(),
                },
                activity_id: ActivityId::new("activity-create").unwrap(),
                origin: ActivityOrigin::Human,
                summary: "Task created".to_owned(),
            },
        )
        .expect("create task");
        assert_eq!(created.status(), TaskStatus::Inbox);

        let started = start_task_with_activity(
            &mut store,
            &FixedClock,
            StartTaskWithActivityCommand {
                id: task_id,
                activity_id: ActivityId::new("activity-start").unwrap(),
                origin: ActivityOrigin::Human,
                summary: "Task started".to_owned(),
            },
        )
        .expect("start task");
        assert_eq!(started.status(), TaskStatus::InProgress);
        assert_eq!(ActivityRepository::list(&store).unwrap().len(), 2);
    }

    #[test]
    fn task_planning_metadata_round_trips() {
        let mut store = SqliteStore::in_memory().expect("open database");
        let mut task = Task::new(task_id("task-plan"), "Plan me", TaskScope::standalone()).unwrap();
        let planning = TaskPlanning::new(Some(75), Some(1_000), Some(9_000)).unwrap();
        task.set_planning(planning);
        TaskRepository::save(&mut store, task.clone()).unwrap();

        let loaded = TaskRepository::get(&store, task.id())
            .unwrap()
            .expect("task");
        assert_eq!(loaded.planning(), planning);
    }

    #[test]
    fn schema_v1_migrates_task_planning_columns_without_changing_existing_task() {
        let path = unique_temp_path("schema-v1-planning");
        {
            let connection = Connection::open(&path).unwrap();
            connection
                .execute_batch(
                    r#"
                CREATE TABLE orbit_schema (
                    id INTEGER PRIMARY KEY CHECK (id = 1),
                    version INTEGER NOT NULL
                );
                INSERT INTO orbit_schema(id, version) VALUES (1, 1);
                CREATE TABLE tasks (
                    id TEXT PRIMARY KEY NOT NULL,
                    title TEXT NOT NULL,
                    status TEXT NOT NULL,
                    project_id TEXT,
                    milestone_id TEXT,
                    CHECK (milestone_id IS NULL OR project_id IS NOT NULL)
                );
                INSERT INTO tasks(id, title, status, project_id, milestone_id)
                VALUES ('legacy-task', 'Legacy', 'inbox', NULL, NULL);
                "#,
                )
                .unwrap();
        }

        let store = SqliteStore::open(&path).expect("migrate v1");
        assert_eq!(store.schema_version().unwrap(), SCHEMA_VERSION);
        let task = TaskRepository::get(&store, &task_id("legacy-task"))
            .unwrap()
            .expect("task");
        assert_eq!(task.title(), "Legacy");
        assert_eq!(task.planning(), TaskPlanning::default());
        drop(store);
        cleanup_sqlite_files(&path);
    }

    #[test]
    fn validated_plan_apply_rechecks_and_persists_schedule_and_activity_atomically() {
        use orbit_application::{
            ApplyValidatedPlanCommand, Clock, PlanApplicationIdentity, apply_validated_plan,
        };
        use orbit_planner::{CapacityPolicy, PlanningWindow, propose_tasks, validate_proposal};

        struct FixedClock;
        impl Clock for FixedClock {
            fn now_ms(&self) -> i64 {
                42
            }
        }

        let mut store = SqliteStore::in_memory().expect("store");
        let mut task =
            Task::new(task_id("task-plan"), "Plan me", TaskScope::standalone()).expect("task");
        task.transition_to(TaskStatus::Ready).expect("ready");
        task.set_planning(TaskPlanning::new(Some(30), None, None).expect("planning"));
        TaskRepository::save(&mut store, task.clone()).expect("save task");

        let window = PlanningWindow::new(0, 120 * 60_000).expect("window");
        let proposal = propose_tasks(window, &[], &[task.clone()], CapacityPolicy::default());
        let validated =
            validate_proposal(window, &[], &[task], CapacityPolicy::default(), proposal)
                .expect("valid proposal");

        let blocks = apply_validated_plan(
            &mut store,
            &FixedClock,
            ApplyValidatedPlanCommand {
                window,
                policy: CapacityPolicy::default(),
                fixed_buffers: orbit_planner::FixedBlockBufferPolicy::default(),
                proposal: validated,
                identities: vec![PlanApplicationIdentity {
                    task_id: task_id("task-plan"),
                    schedule_block_id: ScheduleBlockId::new("plan-block-1").expect("block id"),
                    activity_id: ActivityId::new("plan-activity-1").expect("activity id"),
                }],
                origin: ActivityOrigin::System,
            },
        )
        .expect("apply");

        assert_eq!(blocks.len(), 1);
        assert_eq!(
            ScheduleRepository::list_between(&store, 0, 120 * 60_000)
                .expect("schedule")
                .len(),
            1
        );
        assert_eq!(
            ActivityRepository::list(&store).expect("activities").len(),
            1
        );
    }

    #[test]
    fn validated_plan_apply_rejects_stale_proposal_after_schedule_changes() {
        use orbit_application::{
            ApplyValidatedPlanCommand, Clock, PlanApplicationIdentity, apply_validated_plan,
        };
        use orbit_planner::{CapacityPolicy, PlanningWindow, propose_tasks, validate_proposal};

        struct FixedClock;
        impl Clock for FixedClock {
            fn now_ms(&self) -> i64 {
                42
            }
        }

        let mut store = SqliteStore::in_memory().expect("store");
        let mut task =
            Task::new(task_id("task-plan"), "Plan me", TaskScope::standalone()).expect("task");
        task.transition_to(TaskStatus::Ready).expect("ready");
        task.set_planning(TaskPlanning::new(Some(30), None, None).expect("planning"));
        TaskRepository::save(&mut store, task.clone()).expect("save task");

        let window = PlanningWindow::new(0, 120 * 60_000).expect("window");
        let proposal = propose_tasks(window, &[], &[task.clone()], CapacityPolicy::default());
        let validated =
            validate_proposal(window, &[], &[task], CapacityPolicy::default(), proposal)
                .expect("valid proposal");

        let blocker = ScheduleBlock::new(
            ScheduleBlockId::new("new-fixed").expect("id"),
            "New meeting",
            0,
            60,
            ScheduleFlexibility::Fixed,
            ScheduleTarget::Standalone,
        )
        .expect("blocker");
        ScheduleRepository::save(&mut store, blocker).expect("save blocker");

        let result = apply_validated_plan(
            &mut store,
            &FixedClock,
            ApplyValidatedPlanCommand {
                window,
                policy: CapacityPolicy::default(),
                fixed_buffers: orbit_planner::FixedBlockBufferPolicy::default(),
                proposal: validated,
                identities: vec![PlanApplicationIdentity {
                    task_id: task_id("task-plan"),
                    schedule_block_id: ScheduleBlockId::new("plan-block-1").expect("block id"),
                    activity_id: ActivityId::new("plan-activity-1").expect("activity id"),
                }],
                origin: ActivityOrigin::System,
            },
        );

        assert!(matches!(result, Err(ApplicationError::PlanRejected { .. })));
        assert_eq!(
            ActivityRepository::list(&store).expect("activities").len(),
            0
        );
    }

    #[test]
    fn habit_occurrence_round_trips_and_orders_by_time() {
        let mut store = SqliteStore::in_memory().expect("store");
        let habit = Habit::new(
            HabitId::new("habit-run").unwrap(),
            "Run",
            HabitCadence::Daily,
        )
        .unwrap();
        HabitRepository::save(&mut store, habit.clone()).unwrap();

        let later = HabitOccurrence::new(
            HabitOccurrenceId::new("occ-2").unwrap(),
            habit.id().clone(),
            200,
            Some(30),
        )
        .unwrap();
        let earlier = HabitOccurrence::new(
            HabitOccurrenceId::new("occ-1").unwrap(),
            habit.id().clone(),
            100,
            None,
        )
        .unwrap();
        HabitOccurrenceRepository::append(&mut store, later).unwrap();
        HabitOccurrenceRepository::append(&mut store, earlier).unwrap();

        let history = HabitOccurrenceRepository::list_for_habit(&store, habit.id()).unwrap();
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].id().as_str(), "occ-1");
        assert_eq!(history[1].duration_minutes(), Some(30));
    }

    #[test]
    fn habit_occurrence_and_activity_commit_atomically() {
        use orbit_application::{
            Clock, RecordHabitOccurrenceWithActivityCommand, record_habit_occurrence_with_activity,
        };

        struct FixedClock;
        impl Clock for FixedClock {
            fn now_ms(&self) -> i64 {
                500
            }
        }

        let mut store = SqliteStore::in_memory().expect("store");
        let habit = Habit::new(
            HabitId::new("habit-read").unwrap(),
            "Read",
            HabitCadence::Daily,
        )
        .unwrap();
        HabitRepository::save(&mut store, habit.clone()).unwrap();

        let occurrence = record_habit_occurrence_with_activity(
            &mut store,
            &FixedClock,
            RecordHabitOccurrenceWithActivityCommand {
                id: HabitOccurrenceId::new("occ-1").unwrap(),
                habit_id: habit.id().clone(),
                occurred_at_ms: None,
                duration_minutes: Some(25),
                activity_id: ActivityId::new("activity-occ-1").unwrap(),
                origin: ActivityOrigin::Human,
                summary: "Habit completed".to_owned(),
            },
        )
        .unwrap();
        assert_eq!(occurrence.occurred_at_ms(), 500);
        assert_eq!(
            HabitOccurrenceRepository::list_for_habit(&store, habit.id())
                .unwrap()
                .len(),
            1
        );
        assert_eq!(ActivityRepository::list(&store).unwrap().len(), 1);
    }

    #[test]
    fn schema_v11_preserves_metric_evidence_and_installs_agent_jobs() {
        let path = unique_temp_path("schema-v11-metrics");
        {
            let connection = Connection::open(&path).unwrap();
            connection
                .execute_batch(
                    r#"
                CREATE TABLE orbit_schema (id INTEGER PRIMARY KEY CHECK (id = 1), version INTEGER NOT NULL);
                INSERT INTO orbit_schema(id, version) VALUES (1, 11);
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
                INSERT INTO metric_targets VALUES ('legacy-metric', 0, 10, 'at_least');
                INSERT INTO metric_observations VALUES ('legacy-metric', 500, 4);
                "#,
                )
                .unwrap();
        }
        let store = SqliteStore::open(&path).expect("migrate v11");
        assert_eq!(store.schema_version().unwrap(), SCHEMA_VERSION);
        let targets: i64 = store
            .query_row("SELECT COUNT(*) FROM metric_targets", [], |row| row.get(0))
            .unwrap();
        let observations: i64 = store
            .query_row("SELECT COUNT(*) FROM metric_observations", [], |row| {
                row.get(0)
            })
            .unwrap();
        let agent_jobs: i64 = store
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='agent_jobs'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!((targets, observations, agent_jobs), (1, 1, 1));
        drop(store);
        cleanup_sqlite_files(&path);
    }

    /// Representative execution-only schema for versions 9–12. Each version
    /// adds its own tables; the migration path must preserve existing rows and
    /// install everything introduced later (evidence bindings, metrics, agent
    /// jobs) without touching unrelated tables.
    fn legacy_execution_fixture(path: &Path, version: i64, with_evidence_binding: bool) {
        let mut sql = format!(
            r#"
            CREATE TABLE orbit_schema (id INTEGER PRIMARY KEY CHECK (id = 1), version INTEGER NOT NULL);
            INSERT INTO orbit_schema(id, version) VALUES (1, {version});
            CREATE TABLE projects (
                id TEXT PRIMARY KEY NOT NULL, name TEXT NOT NULL, status TEXT NOT NULL,
                mode TEXT NOT NULL DEFAULT 'standard', objective TEXT,
                progress_signals TEXT NOT NULL DEFAULT 'milestones,tasks,blockers,forecast'
            );
            CREATE TABLE tasks (
                id TEXT PRIMARY KEY NOT NULL, title TEXT NOT NULL, status TEXT NOT NULL,
                project_id TEXT, milestone_id TEXT, estimated_minutes INTEGER,
                earliest_start_at_ms INTEGER, deadline_at_ms INTEGER,
                CHECK (milestone_id IS NULL OR project_id IS NOT NULL)
            );
            CREATE TABLE project_success_criteria (
                project_id TEXT NOT NULL, position INTEGER NOT NULL CHECK(position >= 0),
                criterion_key TEXT NOT NULL, description TEXT NOT NULL,
                required INTEGER NOT NULL CHECK(required IN (0, 1)), status TEXT NOT NULL,
                PRIMARY KEY(project_id, position), UNIQUE(project_id, criterion_key)
            );
            CREATE TABLE sync_local_state (
                id INTEGER PRIMARY KEY CHECK(id = 1), device_id TEXT NOT NULL UNIQUE,
                next_sequence INTEGER NOT NULL CHECK(next_sequence > 0)
            );
            CREATE TABLE sync_outbox (
                device_id TEXT NOT NULL, sequence INTEGER NOT NULL CHECK(sequence > 0),
                recorded_at_ms INTEGER NOT NULL, entity_kind TEXT NOT NULL, entity_id TEXT NOT NULL,
                exported_at_ms INTEGER, PRIMARY KEY(device_id, sequence)
            );
            CREATE TABLE sync_resolution_outbox (
                device_id TEXT NOT NULL, sequence INTEGER NOT NULL CHECK(sequence > 0),
                recorded_at_ms INTEGER NOT NULL, conflict_id_json TEXT NOT NULL,
                value TEXT, exported_at_ms INTEGER, PRIMARY KEY(device_id, sequence)
            );
            INSERT INTO projects(id, name, status) VALUES ('legacy-project', 'Legacy project', 'active');
            INSERT INTO tasks(id, title, status, project_id, milestone_id, estimated_minutes,
                earliest_start_at_ms, deadline_at_ms)
                VALUES ('legacy-task', 'Legacy task', 'ready', 'legacy-project', NULL, 45, 100, 900);
            INSERT INTO sync_resolution_outbox(device_id, sequence, recorded_at_ms, conflict_id_json, value)
                VALUES ('legacy-device', 1, 500, '{{"entity":"task"}}', 'value');
            "#
        );
        if with_evidence_binding {
            sql.push_str(
                r#"
                CREATE TABLE project_evidence_bindings (
                    project_id TEXT NOT NULL, evidence_kind TEXT NOT NULL
                        CHECK(evidence_kind IN ('habit', 'metric')),
                    evidence_id TEXT NOT NULL CHECK(length(trim(evidence_id)) > 0),
                    PRIMARY KEY(project_id, evidence_kind, evidence_id)
                );
                INSERT INTO project_evidence_bindings VALUES ('legacy-project', 'metric', 'legacy-metric');
                "#,
            );
        }
        Connection::open(path).unwrap().execute_batch(&sql).unwrap();
    }

    fn assert_migrated_execution_task(store: &SqliteStore) {
        assert_eq!(store.schema_version().unwrap(), SCHEMA_VERSION);
        let task = TaskRepository::get(store, &task_id("legacy-task"))
            .unwrap()
            .expect("legacy task");
        assert_eq!(task.title(), "Legacy task");
        assert_eq!(task.status(), TaskStatus::Ready);
        assert_eq!(
            task.planning(),
            TaskPlanning::new(Some(45), Some(100), Some(900)).unwrap()
        );
        let agent_jobs: i64 = store
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='agent_jobs'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(agent_jobs, 1);
    }

    #[test]
    fn schema_v9_migrates_execution_history_and_adds_later_tables() {
        let path = unique_temp_path("schema-v9-execution");
        legacy_execution_fixture(&path, 9, false);
        let store = SqliteStore::open(&path).expect("migrate v9");
        assert_migrated_execution_task(&store);
        let resolutions: i64 = store
            .query_row("SELECT COUNT(*) FROM sync_resolution_outbox", [], |row| {
                row.get(0)
            })
            .unwrap();
        let evidence_bindings: i64 = store
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='project_evidence_bindings'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let metric_targets: i64 = store
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='metric_targets'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!((resolutions, evidence_bindings, metric_targets), (1, 1, 1));
        drop(store);
        cleanup_sqlite_files(&path);
    }

    #[test]
    fn schema_v10_preserves_evidence_bindings_and_installs_metric_tables() {
        let path = unique_temp_path("schema-v10-evidence");
        legacy_execution_fixture(&path, 10, true);
        let store = SqliteStore::open(&path).expect("migrate v10");
        assert_migrated_execution_task(&store);
        let bindings: i64 = store
            .query_row(
                "SELECT COUNT(*) FROM project_evidence_bindings",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let metric_targets: i64 = store
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='metric_targets'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!((bindings, metric_targets), (1, 1));
        drop(store);
        cleanup_sqlite_files(&path);
    }

    #[test]
    fn schema_v12_execution_state_migrates_without_touching_tasks() {
        let path = unique_temp_path("schema-v12-execution");
        legacy_execution_fixture(&path, 12, true);
        let store = SqliteStore::open(&path).expect("migrate v12");
        assert_migrated_execution_task(&store);
        let bindings: i64 = store
            .query_row(
                "SELECT COUNT(*) FROM project_evidence_bindings",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(bindings, 1);
        drop(store);
        cleanup_sqlite_files(&path);
    }

    #[test]
    fn schema_v2_migrates_habit_occurrence_table() {
        let path = unique_temp_path("schema-v2-habit-occurrence");
        {
            let connection = Connection::open(&path).unwrap();
            connection.execute_batch(
                r#"
                CREATE TABLE orbit_schema (id INTEGER PRIMARY KEY CHECK (id = 1), version INTEGER NOT NULL);
                INSERT INTO orbit_schema(id, version) VALUES (1, 2);
                CREATE TABLE habits (
                    id TEXT PRIMARY KEY NOT NULL,
                    name TEXT NOT NULL,
                    cadence_kind TEXT NOT NULL,
                    target_per_week INTEGER,
                    active INTEGER NOT NULL CHECK(active IN (0, 1))
                );
                INSERT INTO habits(id, name, cadence_kind, target_per_week, active)
                VALUES ('habit-legacy', 'Legacy', 'daily', NULL, 1);
                "#,
            ).unwrap();
        }
        let mut store = SqliteStore::open(&path).expect("migrate v2");
        assert_eq!(store.schema_version().unwrap(), SCHEMA_VERSION);
        let occurrence = HabitOccurrence::new(
            HabitOccurrenceId::new("occ-new").unwrap(),
            HabitId::new("habit-legacy").unwrap(),
            1,
            None,
        )
        .unwrap();
        HabitOccurrenceRepository::append(&mut store, occurrence).unwrap();
        drop(store);
        cleanup_sqlite_files(&path);
    }
}

#[cfg(test)]
mod routine_storage_tests {
    use super::*;

    #[test]
    fn routine_round_trips_ordered_habit_membership() {
        let mut store = SqliteStore::in_memory().expect("store");
        let first = Habit::new(HabitId::new("habit-a").unwrap(), "A", HabitCadence::Daily).unwrap();
        let second =
            Habit::new(HabitId::new("habit-b").unwrap(), "B", HabitCadence::Daily).unwrap();
        HabitRepository::save(&mut store, first.clone()).unwrap();
        HabitRepository::save(&mut store, second.clone()).unwrap();
        let mut routine = Routine::new(RoutineId::new("routine-1").unwrap(), "Morning").unwrap();
        routine.add_habit(first.id().clone()).unwrap();
        routine.add_habit(second.id().clone()).unwrap();
        RoutineRepository::save(&mut store, routine.clone()).unwrap();

        let loaded = RoutineRepository::get(&store, routine.id())
            .unwrap()
            .unwrap();
        assert_eq!(loaded, routine);
        assert_eq!(store.schema_version().unwrap(), SCHEMA_VERSION);
    }
}

#[cfg(test)]
mod sync_outbox_storage_tests {
    use super::*;
    use orbit_application::{
        Clock, CreateTaskCommand, CreateTaskWithActivityCommand, PendingConflictResolution,
        PendingSemanticChange, SemanticEntityKind, SyncOutboxRepository, create_task_with_activity,
        sync_field,
    };

    struct FixedClock;
    impl Clock for FixedClock {
        fn now_ms(&self) -> i64 {
            777
        }
    }

    #[test]
    fn sync_outbox_reserves_contiguous_sequences_and_marks_exported() {
        let mut store = SqliteStore::in_memory().expect("store");
        SyncOutboxRepository::initialize_sync_device(&mut store, "device-a").expect("device");
        let first = SyncOutboxRepository::enqueue_semantic_change(
            &mut store,
            10,
            PendingSemanticChange::new(
                SemanticEntityKind::Task,
                "task-a",
                vec![sync_field("status", Some("ready".to_owned())).unwrap()],
            )
            .unwrap(),
        )
        .expect("first");
        let second = SyncOutboxRepository::enqueue_semantic_change(
            &mut store,
            11,
            PendingSemanticChange::new(
                SemanticEntityKind::Task,
                "task-a",
                vec![sync_field("status", Some("in_progress".to_owned())).unwrap()],
            )
            .unwrap(),
        )
        .expect("second");
        assert_eq!((first.sequence, second.sequence), (1, 2));
        assert_eq!(
            SyncOutboxRepository::sync_device(&store).unwrap(),
            Some(("device-a".to_owned(), 3))
        );
        assert_eq!(
            SyncOutboxRepository::list_pending_semantic_changes(&store)
                .unwrap()
                .len(),
            2
        );
        SyncOutboxRepository::mark_semantic_change_exported(&mut store, "device-a", 1, 99).unwrap();
        let pending = SyncOutboxRepository::list_pending_semantic_changes(&store).unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].sequence, 2);
    }

    #[test]
    fn conflict_resolution_outbox_shares_the_authoritative_device_sequence() {
        let mut store = SqliteStore::in_memory().expect("store");
        SyncOutboxRepository::initialize_sync_device(&mut store, "device-a").expect("device");
        let first = SyncOutboxRepository::enqueue_semantic_change(
            &mut store,
            10,
            PendingSemanticChange::new(
                SemanticEntityKind::Task,
                "task-a",
                vec![sync_field("status", Some("ready".to_owned())).unwrap()],
            )
            .unwrap(),
        )
        .expect("change");
        let resolution = SyncOutboxRepository::enqueue_conflict_resolution(
            &mut store,
            11,
            PendingConflictResolution::new("{\"conflict\":1}", Some("incoming".into())).unwrap(),
        )
        .expect("resolution");
        assert_eq!((first.sequence, resolution.sequence), (1, 2));
        assert_eq!(
            SyncOutboxRepository::sync_device(&store).unwrap(),
            Some(("device-a".to_owned(), 3))
        );
        let pending = SyncOutboxRepository::list_pending_conflict_resolutions(&store).unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].resolution.value.as_deref(), Some("incoming"));
        SyncOutboxRepository::mark_conflict_resolution_exported(&mut store, "device-a", 2, 99)
            .unwrap();
        assert!(
            SyncOutboxRepository::list_pending_conflict_resolutions(&store)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn failed_unit_of_work_rolls_back_outbox_and_sequence_reservation() {
        let mut store = SqliteStore::in_memory().expect("store");
        SyncOutboxRepository::initialize_sync_device(&mut store, "device-a").expect("device");
        let result: Result<(), ApplicationError> =
            UnitOfWork::transact(&mut store, |repositories| {
                SyncOutboxRepository::enqueue_semantic_change(
                    repositories,
                    10,
                    PendingSemanticChange::new(
                        SemanticEntityKind::Task,
                        "task-a",
                        vec![sync_field("status", Some("ready".to_owned())).unwrap()],
                    )
                    .unwrap(),
                )?;
                Err(ApplicationError::Repository(RepositoryError::Other(
                    "forced rollback".into(),
                )))
            });
        assert!(result.is_err());
        assert!(
            SyncOutboxRepository::list_pending_semantic_changes(&store)
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            SyncOutboxRepository::sync_device(&store).unwrap(),
            Some(("device-a".to_owned(), 1))
        );
    }

    #[test]
    fn synced_application_mutation_commits_state_activity_and_outbox_together() {
        let mut store = SqliteStore::in_memory().expect("store");
        SyncOutboxRepository::initialize_sync_device(&mut store, "device-a").expect("device");
        let task = create_task_with_activity(
            &mut store,
            &FixedClock,
            CreateTaskWithActivityCommand {
                task: CreateTaskCommand {
                    id: TaskId::new("task-sync").unwrap(),
                    title: "Sync me".to_owned(),
                    scope: TaskScope::standalone(),
                },
                activity_id: ActivityId::new("activity-sync").unwrap(),
                origin: ActivityOrigin::Human,
                summary: "Created".to_owned(),
            },
        )
        .expect("create");
        assert!(TaskRepository::get(&store, task.id()).unwrap().is_some());
        assert_eq!(ActivityRepository::list(&store).unwrap().len(), 1);
        let pending = SyncOutboxRepository::list_pending_semantic_changes(&store).unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].sequence, 1);
        assert_eq!(pending[0].change.entity_kind, SemanticEntityKind::Task);
        assert_eq!(pending[0].change.entity_id, "task-sync");
    }
}

#[cfg(test)]
mod inbound_sync_materialization_tests {
    use orbit_application::{
        MaterializedSemanticEntity, MaterializedSemanticField, SemanticEntityKind,
        SyncOutboxRepository, TransitionProjectCommand, TransitionProjectWithActivityCommand,
        materialize_inbound_semantic_state, transition_project_with_activity,
    };
    use orbit_domain::{ActivityId, ActivityOrigin, ProjectId, ProjectStatus};

    use super::*;

    struct FixedClock(i64);
    impl orbit_application::Clock for FixedClock {
        fn now_ms(&self) -> i64 {
            self.0
        }
    }

    fn remote_project(name: &str, status: &str) -> MaterializedSemanticEntity {
        MaterializedSemanticEntity {
            kind: SemanticEntityKind::Project,
            id: "project-remote".into(),
            fields: vec![
                MaterializedSemanticField {
                    field: "name".into(),
                    value: Some(name.into()),
                    device_id: "remote-device".into(),
                    sequence: 1,
                },
                MaterializedSemanticField {
                    field: "status".into(),
                    value: Some(status.into()),
                    device_id: "remote-device".into(),
                    sequence: 1,
                },
            ],
        }
    }

    #[test]
    fn inbound_project_materializes_without_outbound_echo() {
        let mut store = SqliteStore::in_memory().expect("store");
        SyncOutboxRepository::initialize_sync_device(&mut store, "local-device").expect("device");

        let report = materialize_inbound_semantic_state(
            &mut store,
            vec![remote_project("Remote Project", "active")],
        )
        .expect("materialize");
        assert_eq!(report.applied_entities, 1);
        assert_eq!(report.protected_local_fields, 0);

        let project = ProjectRepository::get(&store, &ProjectId::new("project-remote").unwrap())
            .unwrap()
            .unwrap();
        assert_eq!(project.name(), "Remote Project");
        assert_eq!(project.status(), ProjectStatus::Active);
        assert!(
            SyncOutboxRepository::list_pending_semantic_changes(&store)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn next_local_edit_uses_imported_field_version_as_its_sync_base() {
        let mut store = SqliteStore::in_memory().expect("store");
        SyncOutboxRepository::initialize_sync_device(&mut store, "local-device").expect("device");
        materialize_inbound_semantic_state(
            &mut store,
            vec![remote_project("Remote Project", "active")],
        )
        .expect("materialize");

        transition_project_with_activity(
            &mut store,
            &FixedClock(10),
            TransitionProjectWithActivityCommand {
                project: TransitionProjectCommand {
                    id: ProjectId::new("project-remote").unwrap(),
                    status: ProjectStatus::OnHold,
                },
                activity_id: ActivityId::new("activity-local-transition").unwrap(),
                origin: ActivityOrigin::Human,
                summary: "Pause project".into(),
            },
        )
        .expect("local transition");

        let pending = SyncOutboxRepository::list_pending_semantic_changes(&store).unwrap();
        assert_eq!(pending.len(), 1);
        let status = pending[0]
            .change
            .mutations
            .iter()
            .find(|field| field.field == "status")
            .unwrap();
        assert_eq!(status.base_device_id.as_deref(), Some("remote-device"));
        assert_eq!(status.base_sequence, Some(1));
    }

    #[test]
    fn pending_local_field_is_not_overwritten_by_pull_materialization() {
        let mut store = SqliteStore::in_memory().expect("store");
        SyncOutboxRepository::initialize_sync_device(&mut store, "local-device").expect("device");
        materialize_inbound_semantic_state(
            &mut store,
            vec![remote_project("Remote Project", "active")],
        )
        .expect("materialize");
        transition_project_with_activity(
            &mut store,
            &FixedClock(10),
            TransitionProjectWithActivityCommand {
                project: TransitionProjectCommand {
                    id: ProjectId::new("project-remote").unwrap(),
                    status: ProjectStatus::OnHold,
                },
                activity_id: ActivityId::new("activity-local-pending").unwrap(),
                origin: ActivityOrigin::Human,
                summary: "Pause project locally".into(),
            },
        )
        .expect("local transition");

        let report = materialize_inbound_semantic_state(
            &mut store,
            vec![MaterializedSemanticEntity {
                kind: SemanticEntityKind::Project,
                id: "project-remote".into(),
                fields: vec![MaterializedSemanticField {
                    field: "status".into(),
                    value: Some("completed".into()),
                    device_id: "remote-device".into(),
                    sequence: 2,
                }],
            }],
        )
        .expect("materialize");
        assert_eq!(report.protected_local_fields, 1);
        let project = ProjectRepository::get(&store, &ProjectId::new("project-remote").unwrap())
            .unwrap()
            .unwrap();
        assert_eq!(project.status(), ProjectStatus::OnHold);
    }
}

#[cfg(test)]
#[cfg(test)]
#[cfg(test)]
mod project_mode_storage_tests {
    use super::*;
    use orbit_application::{
        ConfigureProjectStrategyCommand, ConfigureProjectStrategyWithActivityCommand,
        MaterializedSemanticEntity, MaterializedSemanticField,
        configure_project_strategy_with_activity, materialize_inbound_semantic_state,
    };
    use orbit_domain::{
        ProjectMode, ProjectProgressSignal, ProjectSuccessCriterion, SuccessCriterionStatus,
    };

    struct TestClock(i64);
    impl orbit_application::Clock for TestClock {
        fn now_ms(&self) -> i64 {
            self.0
        }
    }

    #[test]
    fn project_mode_strategy_and_criteria_round_trip() {
        let mut store = SqliteStore::in_memory().unwrap();
        let id = ProjectId::new("learning-project").unwrap();
        ProjectRepository::save(
            &mut store,
            Project::new(id.clone(), "Learn Japanese").unwrap(),
        )
        .unwrap();
        let mut exam = ProjectSuccessCriterion::new("exam", "Pass N2 mock exam", true).unwrap();
        exam.set_status(SuccessCriterionStatus::Satisfied);
        let mut project = ProjectRepository::get(&store, &id).unwrap().unwrap();
        project.set_mode(ProjectMode::Learning, true);
        project.set_objective(Some("Pass JLPT N2".into())).unwrap();
        project.replace_success_criteria(vec![exam]).unwrap();
        ProjectRepository::save(&mut store, project).unwrap();

        let restored = ProjectRepository::get(&store, &id).unwrap().unwrap();
        assert_eq!(restored.mode(), ProjectMode::Learning);
        assert_eq!(restored.objective(), Some("Pass JLPT N2"));
        assert!(
            restored
                .progress_signals()
                .contains(&ProjectProgressSignal::LearningEvidence)
        );
        assert_eq!(
            restored.success_criteria()[0].status(),
            SuccessCriterionStatus::Satisfied
        );
    }

    #[test]
    fn strategy_change_emits_semantic_fields_not_opaque_blob() {
        let mut store = SqliteStore::in_memory().unwrap();
        SyncOutboxRepository::initialize_sync_device(&mut store, "device-a").unwrap();
        let id = ProjectId::new("project-mode-sync").unwrap();
        ProjectRepository::save(&mut store, Project::new(id.clone(), "Ship Orbit").unwrap())
            .unwrap();
        configure_project_strategy_with_activity(
            &mut store,
            &TestClock(100),
            ConfigureProjectStrategyWithActivityCommand {
                project: ConfigureProjectStrategyCommand {
                    id: id.clone(),
                    mode: ProjectMode::Standard,
                    objective: Some("Release beta".into()),
                    progress_signals: ProjectMode::Standard.default_signals(),
                    success_criteria: vec![
                        ProjectSuccessCriterion::new("smoke", "Core smoke tests pass", true)
                            .unwrap(),
                    ],
                },
                activity_id: ActivityId::new("activity-mode").unwrap(),
                origin: ActivityOrigin::Human,
                summary: "Configured project strategy".into(),
            },
        )
        .unwrap();
        let pending = SyncOutboxRepository::list_pending_semantic_changes(&store).unwrap();
        let change = pending
            .iter()
            .find(|item| item.change.entity_kind == SemanticEntityKind::Project)
            .unwrap();
        assert!(
            change
                .change
                .mutations
                .iter()
                .any(|field| field.field == "mode" && field.value.as_deref() == Some("standard"))
        );
        assert!(
            change
                .change
                .mutations
                .iter()
                .any(|field| field.field == "criterion:smoke")
        );
    }

    #[test]
    fn inbound_project_strategy_materializes_without_echo() {
        let mut store = SqliteStore::in_memory().unwrap();
        SyncOutboxRepository::initialize_sync_device(&mut store, "local").unwrap();
        let fields = vec![
            ("name", Some("Learn Rust")),
            ("status", Some("proposed")),
            ("mode", Some("learning")),
            ("objective", Some("Master async Rust")),
            (
                "progress_signals",
                Some("milestones,learning_evidence,time_invested"),
            ),
            ("criterion:capstone", Some("1|pending|Complete%20capstone")),
        ]
        .into_iter()
        .map(|(field, value)| MaterializedSemanticField {
            field: field.into(),
            value: value.map(str::to_owned),
            device_id: "remote".into(),
            sequence: 1,
        })
        .collect();
        let report = materialize_inbound_semantic_state(
            &mut store,
            vec![MaterializedSemanticEntity {
                kind: SemanticEntityKind::Project,
                id: "remote-learning".into(),
                fields,
            }],
        )
        .unwrap();
        assert_eq!(report.applied_entities, 1);
        let project = ProjectRepository::get(&store, &ProjectId::new("remote-learning").unwrap())
            .unwrap()
            .unwrap();
        assert_eq!(project.mode(), ProjectMode::Learning);
        assert_eq!(project.success_criteria().len(), 1);
        assert!(
            SyncOutboxRepository::list_pending_semantic_changes(&store)
                .unwrap()
                .is_empty()
        );
    }
}

#[cfg(test)]
mod project_evidence_binding_tests {
    use super::*;

    #[test]
    fn bindings_are_unique_ordered_scoped_and_removable() {
        let mut store = SqliteStore::in_memory().unwrap();
        let p1 = ProjectId::new("p1").unwrap();
        let p2 = ProjectId::new("p2").unwrap();
        for id in [&p1, &p2] {
            ProjectRepository::save(&mut store, Project::new(id.clone(), "Example").unwrap())
                .unwrap();
        }
        let habit = ProjectEvidenceBinding::Habit(HabitId::new("daily").unwrap());
        let metric = ProjectEvidenceBinding::Metric("weekly-hours".into());
        store.add_project_evidence(&p1, &metric).unwrap();
        store.add_project_evidence(&p1, &habit).unwrap();
        store.add_project_evidence(&p1, &habit).unwrap();
        assert_eq!(
            store.list_project_evidence(&p1).unwrap(),
            vec![habit.clone(), metric]
        );
        assert!(store.list_project_evidence(&p2).unwrap().is_empty());
        store.remove_project_evidence(&p1, &habit).unwrap();
        assert_eq!(store.list_project_evidence(&p1).unwrap().len(), 1);
    }

    #[test]
    fn bindings_require_existing_project_and_nonblank_metric() {
        let mut store = SqliteStore::in_memory().unwrap();
        let missing = ProjectId::new("absent").unwrap();
        assert!(
            store
                .add_project_evidence(&missing, &ProjectEvidenceBinding::Metric("x".into()))
                .is_err()
        );
        ProjectRepository::save(
            &mut store,
            Project::new(missing.clone(), "Now present").unwrap(),
        )
        .unwrap();
        assert!(
            store
                .add_project_evidence(&missing, &ProjectEvidenceBinding::Metric(" ".into()))
                .is_err()
        );
    }
}

#[cfg(test)]
mod validated_project_evidence_commands_tests {
    use super::*;
    use orbit_application::{
        ChangeProjectEvidenceCommand, bind_project_evidence, list_project_evidence,
        unbind_project_evidence,
    };
    struct TestClock;
    impl orbit_application::Clock for TestClock {
        fn now_ms(&self) -> i64 {
            1234
        }
    }

    #[test]
    fn commands_validate_references_and_support_idempotent_roundtrip() {
        let mut store = SqliteStore::in_memory().unwrap();
        let project_id = ProjectId::new("project-evidence-commands").unwrap();
        let project = Project::new(project_id.clone(), "Project").unwrap();
        ProjectRepository::save(&mut store, project).unwrap();
        let metric = ProjectEvidenceBinding::Metric("focus-hours".into());
        let command = ChangeProjectEvidenceCommand {
            project_id: project_id.clone(),
            binding: metric.clone(),
        };
        bind_project_evidence(&mut store, &TestClock, command.clone()).unwrap();
        bind_project_evidence(&mut store, &TestClock, command.clone()).unwrap();
        assert_eq!(
            list_project_evidence(&store, &project_id).unwrap(),
            vec![metric]
        );
        unbind_project_evidence(&mut store, &TestClock, command).unwrap();
        assert!(
            list_project_evidence(&store, &project_id)
                .unwrap()
                .is_empty()
        );
        let missing = ProjectId::new("not-a-project").unwrap();
        assert!(list_project_evidence(&store, &missing).is_err());
        let bad = ChangeProjectEvidenceCommand {
            project_id,
            binding: ProjectEvidenceBinding::Metric(" ".into()),
        };
        assert!(bind_project_evidence(&mut store, &TestClock, bad).is_err());
    }

    #[test]
    fn project_evidence_is_transactional_and_replays_to_other_replica() {
        use orbit_application::{
            MaterializedSemanticEntity, MaterializedSemanticField, SemanticEntityKind,
            SyncOutboxRepository, materialize_inbound_semantic_state,
        };
        let id = ProjectId::new("evidence-sync-project").unwrap();
        let binding = ProjectEvidenceBinding::Metric("hours".into());
        let command = ChangeProjectEvidenceCommand {
            project_id: id.clone(),
            binding: binding.clone(),
        };
        let mut local = SqliteStore::in_memory().unwrap();
        let mut remote = SqliteStore::in_memory().unwrap();
        for store in [&mut local, &mut remote] {
            ProjectRepository::save(store, Project::new(id.clone(), "Project").unwrap()).unwrap();
        }
        SyncOutboxRepository::initialize_sync_device(&mut local, "local").unwrap();
        bind_project_evidence(&mut local, &TestClock, command.clone()).unwrap();
        bind_project_evidence(&mut local, &TestClock, command.clone()).unwrap();
        let pending = SyncOutboxRepository::list_pending_semantic_changes(&local).unwrap();
        assert_eq!(
            pending.len(),
            1,
            "idempotent rebinding must not duplicate sync events"
        );
        assert_eq!(pending[0].change.entity_kind, SemanticEntityKind::Project);
        assert_eq!(
            pending[0].change.mutations[0].field,
            "evidence:metric:hours"
        );
        let inbound = |value: Option<&str>, sequence| MaterializedSemanticEntity {
            kind: SemanticEntityKind::Project,
            id: id.to_string(),
            fields: vec![MaterializedSemanticField {
                field: "evidence:metric:hours".into(),
                value: value.map(str::to_owned),
                device_id: "local".into(),
                sequence,
            }],
        };
        materialize_inbound_semantic_state(&mut remote, vec![inbound(Some("1"), 1)]).unwrap();
        assert_eq!(
            list_project_evidence(&remote, &id).unwrap(),
            vec![binding.clone()]
        );
        unbind_project_evidence(&mut local, &TestClock, command).unwrap();
        assert_eq!(
            SyncOutboxRepository::list_pending_semantic_changes(&local)
                .unwrap()
                .len(),
            2
        );
        materialize_inbound_semantic_state(&mut remote, vec![inbound(None, 2)]).unwrap();
        assert!(list_project_evidence(&remote, &id).unwrap().is_empty());
    }
}

#[cfg(test)]
mod metric_evidence_v11_tests {
    use super::*;
    #[test]
    fn metric_targets_and_as_of_observations_round_trip() {
        let mut store = SqliteStore::in_memory().unwrap();
        let target = MetricTargetRecord {
            metric_id: "study-hours".into(),
            baseline: 10,
            target: 30,
            direction: orbit_progress::MetricTargetDirection::AtLeast,
        };
        store.put_metric_target(&target).unwrap();
        assert_eq!(
            store.get_metric_target("study-hours").unwrap(),
            Some(target)
        );
        store
            .append_metric_observation(&MetricObservation {
                metric_id: "study-hours".into(),
                observed_at_ms: 100,
                value: 15,
            })
            .unwrap();
        store
            .append_metric_observation(&MetricObservation {
                metric_id: "study-hours".into(),
                observed_at_ms: 200,
                value: 22,
            })
            .unwrap();
        assert_eq!(
            store.latest_metric_observation("study-hours", 50).unwrap(),
            None
        );
        assert_eq!(
            store
                .latest_metric_observation("study-hours", 150)
                .unwrap()
                .unwrap()
                .value,
            15
        );
        assert_eq!(
            store
                .latest_metric_observation("study-hours", 250)
                .unwrap()
                .unwrap()
                .value,
            22
        );
        assert!(
            store
                .append_metric_observation(&MetricObservation {
                    metric_id: "study-hours".into(),
                    observed_at_ms: 200,
                    value: 99
                })
                .is_err()
        );
        assert!(
            store
                .put_metric_target(&MetricTargetRecord {
                    metric_id: "bad".into(),
                    baseline: 20,
                    target: 10,
                    direction: orbit_progress::MetricTargetDirection::AtLeast
                })
                .is_err()
        );
    }
}

#[cfg(test)]
mod metric_semantic_sync_tests {
    use super::*;
    use orbit_application::{
        Clock, MaterializedSemanticEntity, MaterializedSemanticField, SemanticEntityKind,
        SyncOutboxRepository, materialize_inbound_semantic_state, record_metric_observation,
        set_metric_target,
    };
    use orbit_progress::MetricTargetDirection;
    struct TestClock;
    impl Clock for TestClock {
        fn now_ms(&self) -> i64 {
            500
        }
    }
    fn inbound(id: &str, field: &str, value: &str, seq: u64) -> MaterializedSemanticEntity {
        MaterializedSemanticEntity {
            kind: SemanticEntityKind::Metric,
            id: id.into(),
            fields: vec![MaterializedSemanticField {
                field: field.into(),
                value: Some(value.into()),
                device_id: "author".into(),
                sequence: seq,
            }],
        }
    }
    #[test]
    fn metric_target_and_observation_sync_to_other_replica() {
        let mut local = SqliteStore::in_memory().unwrap();
        let mut remote = SqliteStore::in_memory().unwrap();
        SyncOutboxRepository::initialize_sync_device(&mut local, "author").unwrap();
        let target = MetricTargetRecord {
            metric_id: "research-hours".into(),
            baseline: 0,
            target: 100,
            direction: MetricTargetDirection::AtLeast,
        };
        set_metric_target(&mut local, &TestClock, target.clone()).unwrap();
        set_metric_target(&mut local, &TestClock, target.clone()).unwrap();
        let observation = MetricObservation {
            metric_id: "research-hours".into(),
            observed_at_ms: 400,
            value: 25,
        };
        record_metric_observation(&mut local, &TestClock, observation.clone()).unwrap();
        record_metric_observation(&mut local, &TestClock, observation.clone()).unwrap();
        let outbox = SyncOutboxRepository::list_pending_semantic_changes(&local).unwrap();
        assert_eq!(
            outbox.len(),
            2,
            "duplicate writes must not allocate sync sequences"
        );
        assert_eq!(outbox[0].change.entity_kind, SemanticEntityKind::Metric);
        assert_eq!(outbox[1].change.mutations[0].field, "observation:400");
        let missing_target = materialize_inbound_semantic_state(
            &mut remote,
            vec![inbound("research-hours", "observation:400", "25", 2)],
        )
        .unwrap();
        assert_eq!(missing_target.deferred_entities, 1);
        materialize_inbound_semantic_state(
            &mut remote,
            vec![
                inbound("research-hours", "target", "0:100:at_least", 1),
                inbound("research-hours", "observation:400", "25", 2),
            ],
        )
        .unwrap();
        assert_eq!(
            remote.get_metric_target("research-hours").unwrap(),
            Some(target)
        );
        assert_eq!(
            remote
                .latest_metric_observation("research-hours", 400)
                .unwrap(),
            Some(observation.clone())
        );
        materialize_inbound_semantic_state(
            &mut remote,
            vec![inbound("research-hours", "observation:400", "25", 2)],
        )
        .unwrap();
        assert_eq!(
            remote
                .latest_metric_observation("research-hours", 400)
                .unwrap(),
            Some(observation)
        );
    }

    #[test]
    fn semantic_resolution_can_replace_same_time_observation() {
        let mut remote = SqliteStore::in_memory().unwrap();
        materialize_inbound_semantic_state(
            &mut remote,
            vec![
                inbound("exercise", "target", "100:50:at_most", 1),
                inbound("exercise", "observation:200", "80", 2),
            ],
        )
        .unwrap();
        materialize_inbound_semantic_state(
            &mut remote,
            vec![inbound("exercise", "observation:200", "70", 3)],
        )
        .unwrap();
        assert_eq!(
            remote
                .latest_metric_observation("exercise", 200)
                .unwrap()
                .unwrap()
                .value,
            70
        );
    }
}

#[cfg(test)]
mod metric_legacy_backfill_tests {
    use super::*;

    #[test]
    fn legacy_metric_backfill_is_atomic_idempotent_and_requires_sync_identity() {
        let mut store = SqliteStore::in_memory().unwrap();
        store
            .put_metric_target(&MetricTargetRecord {
                metric_id: "legacy".into(),
                baseline: 0,
                target: 10,
                direction: orbit_progress::MetricTargetDirection::AtLeast,
            })
            .unwrap();
        store
            .append_metric_observation(&MetricObservation {
                metric_id: "legacy".into(),
                observed_at_ms: 123,
                value: 5,
            })
            .unwrap();
        assert!(store.backfill_legacy_metric_sync(200).is_err());
        SyncOutboxRepository::initialize_sync_device(&mut store, "local").unwrap();
        assert_eq!(store.backfill_legacy_metric_sync(200).unwrap(), 2);
        assert_eq!(store.backfill_legacy_metric_sync(201).unwrap(), 0);
        let events = SyncOutboxRepository::list_pending_semantic_changes(&store).unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].change.entity_kind, SemanticEntityKind::Metric);
        assert_eq!(events[0].change.mutations[0].field, "target");
        assert_eq!(events[1].change.mutations[0].field, "observation:123");
    }
}

#[cfg(test)]
mod m19_retired_sql_knowledge_contract {
    use super::*;

    #[test]
    fn fresh_database_creates_no_sql_knowledge_body_or_asset_tables() {
        let store = SqliteStore::in_memory().unwrap();
        let forbidden = [
            "knowledge_items",
            "knowledge_relations",
            "knowledge_resources",
            "knowledge_resource_local_locators",
            "knowledge_fts",
        ];
        for table in forbidden {
            let count: i64 = store
                .connection
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE name = ?1",
                    [table],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(count, 0, "retired SQL object: {table}");
        }
    }

    #[test]
    fn existing_legacy_sql_knowledge_is_rejected_not_discarded() {
        let path = std::env::temp_dir().join(format!(
            "orbit-m19-legacy-reject-{}-{}.sqlite",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch("CREATE TABLE knowledge_items(id TEXT PRIMARY KEY, body TEXT); INSERT INTO knowledge_items VALUES('existing','do not discard');").unwrap();
        drop(conn);
        assert!(matches!(
            SqliteStore::open(&path),
            Err(RepositoryError::Other(_))
        ));
        let conn = Connection::open(&path).unwrap();
        let body: String = conn
            .query_row(
                "SELECT body FROM knowledge_items WHERE id='existing'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(body, "do not discard");
        drop(conn);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn explicit_workflow_replacement_requires_published_manifest() {
        let mut store = SqliteStore::in_memory().expect("open database");
        let project = Project::new(
            ProjectId::new("workflow-project").unwrap(),
            "Workflow project",
        )
        .unwrap();
        ProjectRepository::save(&mut store, project.clone()).expect("save project");
        let basic = orbit_blueprint::bundled::bundled_blueprint("basic").unwrap();
        store
            .publish_blueprint(&basic, &Default::default())
            .unwrap();
        let development = orbit_blueprint::bundled::bundled_blueprint("development").unwrap();
        store
            .publish_blueprint(&development, &Default::default())
            .unwrap();
        store
            .bind_project_blueprint(&orbit_blueprint::BlueprintBinding {
                project_id: "workflow-project".into(),
                blueprint_id: "basic".into(),
                version: 1,
            })
            .unwrap();
        assert_eq!(
            store.latest_blueprint_version("development").unwrap(),
            Some(development.version)
        );
        assert_eq!(store.latest_blueprint_version("missing").unwrap(), None);
        store
            .replace_project_blueprint_binding(project.id(), "development", development.version)
            .unwrap();
        let binding = store
            .project_blueprint_binding("workflow-project")
            .unwrap()
            .unwrap();
        assert_eq!(
            (binding.blueprint_id.as_str(), binding.version),
            ("development", development.version)
        );
        // Unknown manifest is rejected and leaves the previous binding intact.
        assert!(
            store
                .replace_project_blueprint_binding(project.id(), "development", 99)
                .is_err()
        );
        let binding = store
            .project_blueprint_binding("workflow-project")
            .unwrap()
            .unwrap();
        assert_eq!(
            (binding.blueprint_id.as_str(), binding.version),
            ("development", development.version)
        );
    }
}
