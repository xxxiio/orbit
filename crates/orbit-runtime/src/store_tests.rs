//! AuthorityStore regression suites; production storage remains in store.rs.
use super::*;

#[cfg(test)]
mod tests {
    use super::*;
    use orbit_sync::{
        ChangeId, ConflictId, ConflictResolution, DeviceId, EntityKind, EntityRef, FieldMutation,
        SemanticChange,
    };
    #[test]
    fn legacy_relay_schema_fails_closed_before_authority_initialization() {
        use std::time::{SystemTime, UNIX_EPOCH};
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "orbit-m18-old-schema-{}-{unique}.db",
            std::process::id()
        ));
        {
            let db = rusqlite::Connection::open(&path).unwrap();
            db.execute_batch("CREATE TABLE relay_events(cursor INTEGER PRIMARY KEY); INSERT INTO relay_events VALUES(1);").unwrap();
        }
        assert!(matches!(
            AuthorityStore::open(&path),
            Err(AuthorityError::MigrationConflict)
        ));
        let db = rusqlite::Connection::open(&path).unwrap();
        let count: i64 = db.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='authority_history'",
            [], |row| row.get(0),
        ).unwrap();
        assert_eq!(count, 0);
        drop(db);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn fresh_database_uses_authority_history_not_legacy_relay_tables() {
        let store = AuthorityStore::open(Path::new(":memory:")).unwrap();
        for table in [
            "authority_identity",
            "authority_history",
            "authority_projection",
            "authority_commands",
        ] {
            let count: i64 = store
                .conn
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
                    [table],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(count, 1, "missing canonical table: {table}");
        }
        for table in [
            "relay_identity",
            "relay_events",
            "accepted_projection",
            "authority_projects",
            "authority_tasks",
        ] {
            let count: i64 = store
                .conn
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
                    [table],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(count, 0, "retired table should not exist: {table}");
        }
    }

    #[test]
    fn out_of_order_cross_device_resolution_is_durable_and_idempotent() {
        let mut relay = AuthorityStore::open(Path::new(":memory:")).unwrap();
        let token_a = "a".repeat(40);
        let token_b = "b".repeat(40);
        relay.enroll("device-a", &token_a).unwrap();
        relay.enroll("device-b", &token_b).unwrap();
        let entity = EntityRef::new(EntityKind::Project, "shared-project").unwrap();
        let a1 = ChangeId::new(DeviceId::new("device-a").unwrap(), 1).unwrap();
        let b1 = ChangeId::new(DeviceId::new("device-b").unwrap(), 1).unwrap();
        let a2 = ChangeId::new(DeviceId::new("device-a").unwrap(), 2).unwrap();
        let resolution = ConflictResolution::new(
            ConflictId {
                entity: entity.clone(),
                field: "name".into(),
                local_change: a1.clone(),
                incoming_change: b1.clone(),
            },
            a2.clone(),
            300,
            Some("resolved".into()),
        );
        let change_a = ChangeEnvelope::new(
            a1,
            100,
            SemanticChange::new(
                entity.clone(),
                vec![FieldMutation::new("name", Some("A".into()), None).unwrap()],
            )
            .unwrap(),
        );
        let change_b = ChangeEnvelope::new(
            b1,
            200,
            SemanticChange::new(
                entity,
                vec![FieldMutation::new("name", Some("B".into()), None).unwrap()],
            )
            .unwrap(),
        );
        // The relay never requires causal delivery order; semantic replay enforces it.
        let resolution_wire = resolution.to_wire_json().unwrap();
        let a_wire = change_a.to_wire_json().unwrap();
        let b_wire = change_b.to_wire_json().unwrap();
        assert!(
            !relay
                .append("device-a", "resolution", &resolution_wire)
                .unwrap()
                .duplicate
        );
        assert!(
            !relay
                .append("device-b", "change", &b_wire)
                .unwrap()
                .duplicate
        );
        assert!(
            !relay
                .append("device-a", "change", &a_wire)
                .unwrap()
                .duplicate
        );
        assert_eq!(relay.tip().unwrap(), 3);
        assert_eq!(
            relay
                .pull(0, 10)
                .unwrap()
                .iter()
                .map(|e| e.kind.as_str())
                .collect::<Vec<_>>(),
            vec!["resolution", "change", "change"]
        );
        assert!(
            relay
                .append("device-a", "resolution", &resolution_wire)
                .unwrap()
                .duplicate
        );
        assert!(
            relay
                .append("device-b", "change", &b_wire)
                .unwrap()
                .duplicate
        );
        assert_eq!(relay.tip().unwrap(), 3);
        assert!(matches!(
            relay.append("device-b", "resolution", &resolution_wire),
            Err(AuthorityError::Unauthorized)
        ));
        assert!(matches!(
            relay.append(
                "device-a",
                "change",
                &a_wire.replace("\"A\"", "\"tampered\"")
            ),
            Err(AuthorityError::Invalid(_)) | Err(AuthorityError::ImmutableConflict)
        ));
    }

    #[test]
    fn durable_idempotent_and_immutable() {
        let mut store = AuthorityStore::open(Path::new(":memory:")).unwrap();
        let generation = store.generation().unwrap();
        assert_eq!(generation.len(), 32);
        assert_eq!(store.tip().unwrap(), 0);
        store.enroll("d1", &"a".repeat(40)).unwrap();
        assert!(store.authorized("d1", &"a".repeat(40)).unwrap());
        let change = ChangeEnvelope::new(
            ChangeId::new(DeviceId::new("d1").unwrap(), 1).unwrap(),
            123,
            SemanticChange::new(
                EntityRef::new(EntityKind::Project, "p1").unwrap(),
                vec![FieldMutation::new("name", Some("foo".into()), None).unwrap()],
            )
            .unwrap(),
        );
        let wire = change.to_wire_json().unwrap();
        assert!(!store.append("d1", "change", &wire).unwrap().duplicate);
        assert!(store.append("d1", "change", &wire).unwrap().duplicate);
        let mut changed = change.clone();
        changed.recorded_at_ms = 124;
        assert!(matches!(
            store.append("d1", "change", &changed.to_wire_json().unwrap()),
            Err(AuthorityError::ImmutableConflict)
        ));
        assert!(matches!(
            store.append("other", "change", &wire),
            Err(AuthorityError::Unauthorized)
        ));
        let resolution = ConflictResolution::new(
            ConflictId {
                entity: EntityRef::new(EntityKind::Project, "p1").unwrap(),
                field: "name".into(),
                local_change: change.id.clone(),
                incoming_change: ChangeId::new(DeviceId::new("d2").unwrap(), 1).unwrap(),
            },
            ChangeId::new(DeviceId::new("d1").unwrap(), 2).unwrap(),
            200,
            Some("chosen".into()),
        );
        let resolution_wire = resolution.to_wire_json().unwrap();
        assert!(
            !store
                .append("d1", "resolution", &resolution_wire)
                .unwrap()
                .duplicate
        );
        assert!(
            store
                .append("d1", "resolution", &resolution_wire)
                .unwrap()
                .duplicate
        );
        assert_eq!(store.pull(0, 100).unwrap().len(), 2);
        assert_eq!(store.tip().unwrap(), 2);
        assert_eq!(store.generation().unwrap(), generation);
        assert_eq!(store.pull(1, 100).unwrap().len(), 1);
        assert_eq!(store.pull(2, 100).unwrap().len(), 0);
        assert!(store.rotate_token("d1", &"b".repeat(40)).unwrap());
        assert!(!store.authorized("d1", &"a".repeat(40)).unwrap());
        assert!(store.authorized("d1", &"b".repeat(40)).unwrap());
        assert!(matches!(
            store.rotate_token("d1", "short"),
            Err(AuthorityError::Invalid(_))
        ));
        store.revoke("d1").unwrap();
        assert!(!store.authorized("d1", &"b".repeat(40)).unwrap());
        assert!(!store.rotate_token("d1", &"c".repeat(40)).unwrap());
    }
}

#[cfg(test)]
mod atomic_projection_tests {
    use super::*;
    use orbit_sync::{ChangeId, DeviceId, EntityKind, EntityRef, FieldMutation, SemanticChange};

    fn change(sequence: u64) -> String {
        ChangeEnvelope::new(
            ChangeId::new(DeviceId::new("device-a").unwrap(), sequence).unwrap(),
            1,
            SemanticChange::new(
                EntityRef::new(EntityKind::Task, "task-1").unwrap(),
                vec![
                    FieldMutation::new(format!("field-{sequence}"), Some("value".into()), None)
                        .unwrap(),
                ],
            )
            .unwrap(),
        )
        .to_wire_json()
        .unwrap()
    }

    #[test]
    fn authority_projection_is_atomic_and_invalidated_by_legacy_relay() {
        let mut store = AuthorityStore::open(Path::new(":memory:")).unwrap();
        store.enroll("device-a", &"a".repeat(40)).unwrap();
        assert!(store.authority_projection().unwrap().is_none());
        let first = store
            .commit_semantic("device-a", 0, "change", &change(1))
            .unwrap();
        assert_eq!(first.cursor, 1);
        let state = store.authority_projection().unwrap().unwrap();
        assert_eq!(state["authority_revision"], 1);
        assert!(matches!(
            store.commit_semantic("device-a", 0, "change", &change(2)),
            Err(AuthorityError::StaleRevision)
        ));
        assert_eq!(store.tip().unwrap(), 1);
        assert_eq!(
            store.authority_projection().unwrap().unwrap()["authority_revision"],
            1
        );
        store.append("device-a", "change", &change(2)).unwrap();
        assert!(store.authority_projection().unwrap().is_none());
    }
}

#[cfg(test)]
mod authoritative_task_tests {
    use super::*;
    use crate::domain_commands::{TaskAction, TaskCommand};
    fn command(id: &str, revision: i64, action: TaskAction) -> TaskCommand {
        TaskCommand {
            command_id: id.into(),
            expected_revision: revision,
            action,
        }
    }
    #[test]
    fn task_state_event_and_ack_commit_as_one_revision() {
        let mut store = AuthorityStore::open(Path::new(":memory:")).unwrap();
        store.enroll("device-a", &"a".repeat(40)).unwrap();
        let create = command(
            "c1",
            0,
            TaskAction::Create {
                task_id: "t1".into(),
                title: "Alpha".into(),
                project_id: None,
            },
        );
        let first = store
            .authority_task_command("device-a", create.clone())
            .unwrap();
        assert_eq!(first.revision, 1);
        assert_eq!(store.tasks().unwrap().1.len(), 1);
        assert_eq!(store.event_count().unwrap(), 1);
        assert!(store.authority_projection().unwrap().is_some());
        assert!(
            store
                .authority_task_command("device-a", create.clone())
                .unwrap()
                .duplicate
        );
        assert_eq!(store.event_count().unwrap(), 1);
        assert!(matches!(
            store.authority_task_command("device-b", create),
            Err(AuthorityError::ImmutableConflict)
        ));
        let update = command(
            "c2",
            1,
            TaskAction::ChangeStatus {
                task_id: "t1".into(),
                status: "ready".into(),
            },
        );
        let second = store.authority_task_command("device-a", update).unwrap();
        assert_eq!(second.revision, 2);
        assert_eq!(second.task.status, "ready");
        assert_eq!(store.event_count().unwrap(), 2);
        assert_eq!(
            store.authority_projection().unwrap().unwrap()["authority_revision"],
            2
        );
        assert!(matches!(
            store.authority_task_command(
                "device-a",
                command(
                    "late",
                    1,
                    TaskAction::Create {
                        task_id: "t2".into(),
                        title: "Later".into(),
                        project_id: None
                    }
                )
            ),
            Err(AuthorityError::StaleRevision)
        ));
        assert!(matches!(
            store.append("device-a", "change", "not-a-change"),
            Err(AuthorityError::Invalid(_))
        ));
    }
    #[test]
    fn connected_task_commands_append_parity_activities_in_order() {
        let mut store = AuthorityStore::open(Path::new(":memory:")).unwrap();
        store.enroll("device-a", &"a".repeat(40)).unwrap();
        store
            .authority_task_command(
                "device-a",
                command(
                    "create-t",
                    0,
                    TaskAction::Create {
                        task_id: "t1".into(),
                        title: "Alpha".into(),
                        project_id: None,
                    },
                ),
            )
            .unwrap();
        let status = command(
            "status-t",
            1,
            TaskAction::ChangeStatus {
                task_id: "t1".into(),
                status: "ready".into(),
            },
        );
        store
            .authority_task_command("device-a", status.clone())
            .unwrap();
        store
            .authority_task_command(
                "device-a",
                command(
                    "plan-t",
                    2,
                    TaskAction::SetPlanning {
                        task_id: "t1".into(),
                        estimated_minutes: Some(30),
                        earliest_start_at_ms: None,
                        deadline_at_ms: None,
                    },
                ),
            )
            .unwrap();

        let rows: Vec<(String, String, String, String)> = {
            let mut stmt = store
                .conn
                .prepare(
                    "SELECT id, activity_kind, origin_kind, origin_value FROM activities \
                     ORDER BY sequence",
                )
                .unwrap();
            stmt.query_map([], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
            })
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
        };
        assert_eq!(
            rows.iter().map(|row| row.0.as_str()).collect::<Vec<_>>(),
            ["activity-create-t", "activity-status-t", "activity-plan-t"]
        );
        assert_eq!(
            rows.iter().map(|row| row.1.as_str()).collect::<Vec<_>>(),
            ["created", "status_changed", "updated"]
        );
        assert!(
            rows.iter()
                .all(|row| row.2 == "external" && row.3 == "device-a")
        );

        // Duplicate commands return the receipt before the activity append.
        assert!(
            store
                .authority_task_command("device-a", status)
                .unwrap()
                .duplicate
        );
        let count: i64 = store
            .conn
            .query_row("SELECT COUNT(*) FROM activities", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 3);
    }

    #[test]
    fn task_rename_and_project_assignment_are_authoritative_and_retry_safe() {
        let mut store = AuthorityStore::open(Path::new(":memory:")).unwrap();
        store.enroll("device-a", &"a".repeat(40)).unwrap();
        let p = crate::domain_commands::ProjectCommand {
            workflow_id: None,
            command_id: "p-create".into(),
            expected_revision: 0,
            project_id: "project-1".into(),
            name: "Main".into(),
        };
        store.authority_project_command("device-a", p).unwrap();
        store
            .authority_task_command(
                "device-a",
                command(
                    "t-create",
                    1,
                    TaskAction::Create {
                        task_id: "task-1".into(),
                        title: "Original".into(),
                        project_id: None,
                    },
                ),
            )
            .unwrap();
        let rename = command(
            "t-rename",
            2,
            TaskAction::Rename {
                task_id: "task-1".into(),
                title: "Updated".into(),
            },
        );
        let renamed = store
            .authority_task_command("device-a", rename.clone())
            .unwrap();
        assert_eq!(renamed.task.title, "Updated");
        assert!(
            store
                .authority_task_command("device-a", rename)
                .unwrap()
                .duplicate
        );
        let assign = command(
            "t-assign",
            3,
            TaskAction::AssignProject {
                task_id: "task-1".into(),
                project_id: Some("project-1".into()),
            },
        );
        assert_eq!(
            store
                .authority_task_command("device-a", assign)
                .unwrap()
                .task
                .project_id
                .as_deref(),
            Some("project-1")
        );
        assert!(matches!(
            store.authority_task_command(
                "device-a",
                command(
                    "bad",
                    4,
                    TaskAction::AssignProject {
                        task_id: "task-1".into(),
                        project_id: Some("missing".into())
                    }
                )
            ),
            Err(AuthorityError::Invalid(_))
        ));
        assert_eq!(store.tip().unwrap(), 4);
        assert_eq!(
            store
                .authority_task_command(
                    "device-a",
                    command(
                        "t-unassign",
                        4,
                        TaskAction::AssignProject {
                            task_id: "task-1".into(),
                            project_id: None
                        }
                    )
                )
                .unwrap()
                .task
                .project_id,
            None
        );
        assert_eq!(
            store
                .authority_task_command(
                    "device-a",
                    command(
                        "t-rename2",
                        5,
                        TaskAction::Rename {
                            task_id: "task-1".into(),
                            title: "Final".into()
                        }
                    )
                )
                .unwrap()
                .task
                .title,
            "Final"
        );
        assert_eq!(store.event_count().unwrap(), 6);
    }
    #[test]
    fn task_milestone_link_is_transactional_and_revision_scoped() {
        use crate::authority_milestones::{MilestoneAction, MilestoneCommand};
        use crate::domain_commands::ProjectCommand;
        let mut db = AuthorityStore::open(Path::new(":memory:")).unwrap();
        for (revision, id) in [(0, "one"), (1, "two")] {
            db.authority_project_command(
                "user",
                ProjectCommand {
                    workflow_id: None,
                    command_id: format!("new-{id}"),
                    expected_revision: revision,
                    project_id: id.into(),
                    name: id.into(),
                },
            )
            .unwrap();
        }
        db.authority_milestone_command(
            "user",
            MilestoneCommand {
                command_id: "new-checkpoint".into(),
                expected_revision: 2,
                action: MilestoneAction::Create {
                    milestone_id: "m1".into(),
                    project_id: "one".into(),
                    name: "Ship".into(),
                },
            },
        )
        .unwrap();
        db.authority_task_command(
            "user",
            command(
                "task",
                3,
                TaskAction::Create {
                    task_id: "t".into(),
                    title: "Deliver".into(),
                    project_id: Some("one".into()),
                },
            ),
        )
        .unwrap();
        let assign = command(
            "attach",
            4,
            TaskAction::AssignMilestone {
                task_id: "t".into(),
                milestone_id: Some("m1".into()),
            },
        );
        assert_eq!(
            db.authority_task_command("user", assign.clone())
                .unwrap()
                .task
                .milestone_id
                .as_deref(),
            Some("m1")
        );
        assert!(db.authority_task_command("user", assign).unwrap().duplicate);
        assert_eq!(db.tasks().unwrap().1[0].milestone_id.as_deref(), Some("m1"));
        assert_eq!(
            db.authority_full_snapshot().unwrap().3[0]
                .milestone_id
                .as_deref(),
            Some("m1")
        );
        assert!(matches!(
            db.authority_task_command(
                "user",
                command(
                    "move",
                    5,
                    TaskAction::AssignProject {
                        task_id: "t".into(),
                        project_id: Some("two".into()),
                    }
                )
            ),
            Err(AuthorityError::Invalid(_))
        ));
        assert!(matches!(
            db.authority_milestone_command(
                "user",
                MilestoneCommand {
                    command_id: "delete-m".into(),
                    expected_revision: 5,
                    action: MilestoneAction::Delete {
                        milestone_id: "m1".into()
                    },
                }
            ),
            Err(AuthorityError::Invalid(_))
        ));
        assert_eq!(db.tip().unwrap(), 5);
        db.authority_task_command(
            "user",
            command(
                "detach",
                5,
                TaskAction::AssignMilestone {
                    task_id: "t".into(),
                    milestone_id: None,
                },
            ),
        )
        .unwrap();
        db.authority_task_command(
            "user",
            command(
                "move",
                6,
                TaskAction::AssignProject {
                    task_id: "t".into(),
                    project_id: Some("two".into()),
                },
            ),
        )
        .unwrap();
        assert!(matches!(
            db.authority_task_command(
                "user",
                command(
                    "wrong-project",
                    7,
                    TaskAction::AssignMilestone {
                        task_id: "t".into(),
                        milestone_id: Some("m1".into())
                    }
                )
            ),
            Err(AuthorityError::Invalid(_))
        ));
        assert_eq!(db.tip().unwrap(), 7);
    }

    #[test]
    fn legacy_history_requires_migration_before_authority_activation() {
        use orbit_sync::{
            ChangeEnvelope, ChangeId, DeviceId, EntityKind, EntityRef, FieldMutation,
            SemanticChange,
        };
        let mut store = AuthorityStore::open(Path::new(":memory:")).unwrap();
        store.enroll("device-a", &"a".repeat(40)).unwrap();
        let wire = ChangeEnvelope::new(
            ChangeId::new(DeviceId::new("device-a").unwrap(), 1).unwrap(),
            1,
            SemanticChange::new(
                EntityRef::new(EntityKind::Task, "t1").unwrap(),
                vec![FieldMutation::new("title", Some("legacy".into()), None).unwrap()],
            )
            .unwrap(),
        )
        .to_wire_json()
        .unwrap();
        store.append("device-a", "change", &wire).unwrap();
        assert!(matches!(
            store.authority_task_command(
                "device-a",
                command(
                    "new",
                    1,
                    TaskAction::Create {
                        task_id: "t2".into(),
                        title: "New".into(),
                        project_id: None
                    }
                )
            ),
            Err(AuthorityError::MigrationConflict)
        ));
        assert_eq!(store.tasks().unwrap().1.len(), 0);
    }
}

#[cfg(test)]
mod authoritative_project_tests {
    use super::*;
    use crate::domain_commands::{ProjectCommand, TaskAction, TaskCommand};

    #[test]
    fn projects_and_tasks_share_revision_history_and_atomic_receipts() {
        let mut store = AuthorityStore::open(Path::new(":memory:")).unwrap();
        let create = ProjectCommand {
            workflow_id: None,
            command_id: "project-1".into(),
            expected_revision: 0,
            project_id: "p1".into(),
            name: "A project".into(),
        };
        let first = store
            .authority_project_command("device-a", create.clone())
            .unwrap();
        assert_eq!(first.revision, 1);
        assert_eq!(first.project.status, "proposed");
        assert!(
            store
                .authority_project_command("device-a", create.clone())
                .unwrap()
                .duplicate
        );
        assert_eq!(store.tip().unwrap(), 1);
        assert!(matches!(
            store.authority_project_command("device-b", create),
            Err(AuthorityError::ImmutableConflict)
        ));
        let task = TaskCommand {
            command_id: "task-1".into(),
            expected_revision: 1,
            action: TaskAction::Create {
                task_id: "t1".into(),
                title: "Work".into(),
                project_id: None,
            },
        };
        assert_eq!(
            store
                .authority_task_command("device-a", task)
                .unwrap()
                .revision,
            2
        );
        let project_task = TaskCommand {
            command_id: "project-task".into(),
            expected_revision: 2,
            action: TaskAction::Create {
                task_id: "t2".into(),
                title: "Project task".into(),
                project_id: Some("p1".into()),
            },
        };
        let added = store
            .authority_task_command("device-a", project_task.clone())
            .unwrap();
        assert_eq!(added.task.project_id.as_deref(), Some("p1"));
        assert!(
            store
                .authority_task_command("device-a", project_task)
                .unwrap()
                .duplicate
        );
        let missing = TaskCommand {
            command_id: "missing-project-task".into(),
            expected_revision: 3,
            action: TaskAction::Create {
                task_id: "t3".into(),
                title: "Invalid".into(),
                project_id: Some("absent".into()),
            },
        };
        assert!(matches!(
            store.authority_task_command("device-a", missing),
            Err(AuthorityError::Invalid(_))
        ));
        assert_eq!(store.tip().unwrap(), 3);
        assert_eq!(
            store
                .tasks()
                .unwrap()
                .1
                .iter()
                .find(|t| t.id == "t2")
                .unwrap()
                .project_id
                .as_deref(),
            Some("p1")
        );
        assert_eq!(store.projects().unwrap().1.len(), 1);
        assert_eq!(store.tasks().unwrap().1.len(), 2);
        assert_eq!(store.history().unwrap().len(), 3);
        assert!(store.authority_projection().unwrap().is_some());
        let bad = ProjectCommand {
            workflow_id: None,
            command_id: "project-2".into(),
            expected_revision: 1,
            project_id: "p2".into(),
            name: "Stale".into(),
        };
        assert!(matches!(
            store.authority_project_command("device-a", bad),
            Err(AuthorityError::StaleRevision)
        ));
        assert_eq!(store.tip().unwrap(), 3);
    }
}

#[cfg(test)]
mod authority_snapshot_tests {
    use super::*;
    use crate::domain_commands::{ProjectCommand, TaskAction, TaskCommand};

    #[test]
    fn snapshot_contains_both_domain_collections_at_one_revision() {
        let mut store = AuthorityStore::open(Path::new(":memory:")).unwrap();
        let (revision, projects, tasks) = store.authority_snapshot().unwrap();
        assert_eq!(revision, 0);
        assert!(projects.is_empty());
        assert!(tasks.is_empty());
        store
            .authority_project_command(
                "device-a",
                ProjectCommand {
                    workflow_id: None,
                    command_id: "p-command".into(),
                    expected_revision: 0,
                    project_id: "p".into(),
                    name: "Project".into(),
                },
            )
            .unwrap();
        store
            .authority_task_command(
                "device-a",
                TaskCommand {
                    command_id: "t-command".into(),
                    expected_revision: 1,
                    action: TaskAction::Create {
                        task_id: "t".into(),
                        title: "Task".into(),
                        project_id: Some("p".into()),
                    },
                },
            )
            .unwrap();
        let (revision, projects, tasks) = store.authority_snapshot().unwrap();
        assert_eq!(revision, 2);
        assert_eq!(projects.len(), 1);
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].project_id.as_deref(), Some("p"));
    }
}

#[cfg(test)]
mod verified_authority_snapshot_tests {
    use super::*;
    use crate::domain_commands::{ProjectCommand, TaskAction, TaskCommand};

    #[test]
    fn accepted_history_prefix_is_stable_as_new_commands_arrive() {
        let mut store = AuthorityStore::open(Path::new(":memory:")).unwrap();
        let genesis = store.authority_history_prefix(0).unwrap();
        assert_eq!(genesis, store.authority_full_snapshot().unwrap().1);
        store
            .authority_project_command(
                "device-a",
                ProjectCommand {
                    workflow_id: None,
                    command_id: "first".into(),
                    expected_revision: 0,
                    project_id: "p1".into(),
                    name: "Project".into(),
                },
            )
            .unwrap();
        let first = store.authority_history_prefix(1).unwrap();
        store
            .authority_project_command(
                "device-a",
                ProjectCommand {
                    workflow_id: None,
                    command_id: "second".into(),
                    expected_revision: 1,
                    project_id: "p2".into(),
                    name: "Another".into(),
                },
            )
            .unwrap();
        assert_eq!(store.authority_history_prefix(1).unwrap(), first);
        assert_eq!(store.authority_history_prefix(0).unwrap(), genesis);
        assert_eq!(
            store.authority_history_prefix(2).unwrap(),
            store.authority_full_snapshot().unwrap().1
        );
        assert!(store.authority_history_prefix(3).is_err());
        assert!(store.authority_history_prefix(-1).is_err());
    }
    #[test]
    fn fingerprint_is_stable_for_unchanged_accepted_history_and_advances_on_commits() {
        let mut store = AuthorityStore::open(Path::new(":memory:")).unwrap();
        let (initial_rev, initial, projects, tasks) = store.authority_verified_snapshot().unwrap();
        assert_eq!(initial_rev, 0);
        assert_eq!(initial.len(), 64);
        assert!(projects.is_empty() && tasks.is_empty());
        assert_eq!(store.authority_verified_snapshot().unwrap().1, initial);
        let project = ProjectCommand {
            workflow_id: None,
            command_id: "p1-command".into(),
            expected_revision: 0,
            project_id: "p1".into(),
            name: "Project".into(),
        };
        store
            .authority_project_command("device-a", project.clone())
            .unwrap();
        let (first_rev, first, _, _) = store.authority_verified_snapshot().unwrap();
        assert_eq!(first_rev, 1);
        assert_ne!(first, initial);
        store
            .authority_project_command("device-a", project)
            .unwrap();
        assert_eq!(store.authority_verified_snapshot().unwrap().1, first);
        store
            .authority_task_command(
                "device-a",
                TaskCommand {
                    command_id: "t1-command".into(),
                    expected_revision: 1,
                    action: TaskAction::Create {
                        task_id: "t1".into(),
                        title: "Task".into(),
                        project_id: Some("p1".into()),
                    },
                },
            )
            .unwrap();
        let (second_rev, second, _, tasks) = store.authority_verified_snapshot().unwrap();
        assert_eq!(second_rev, 2);
        assert_ne!(first, second);
        assert_eq!(tasks.len(), 1);
    }

    #[test]
    fn legacy_relay_is_never_reported_as_accepted_canonical_history() {
        use orbit_sync::{
            ChangeEnvelope, ChangeId, DeviceId, EntityKind, EntityRef, FieldMutation,
            SemanticChange,
        };
        let mut store = AuthorityStore::open(Path::new(":memory:")).unwrap();
        store
            .enroll("device-a", "legacy-relay-device-token-0123456789")
            .unwrap();
        let wire = ChangeEnvelope::new(
            ChangeId::new(DeviceId::new("device-a").unwrap(), 1).unwrap(),
            10,
            SemanticChange::new(
                EntityRef::new(EntityKind::Task, "t1").unwrap(),
                vec![FieldMutation::new("title", Some("Task".into()), None).unwrap()],
            )
            .unwrap(),
        )
        .to_wire_json()
        .unwrap();
        store.append("device-a", "change", &wire).unwrap();
        assert!(matches!(
            store.authority_verified_snapshot(),
            Err(AuthorityError::MigrationConflict)
        ));
    }
}

#[cfg(test)]
mod markdown_only_schema_acceptance {
    use super::*;
    #[test]
    fn fresh_authority_has_no_sql_knowledge_body_table() {
        let store = AuthorityStore::open(Path::new(":memory:")).unwrap();
        let count: i64 = store.conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='authority_knowledge_items'",
            [], |row| row.get(0)).unwrap();
        assert_eq!(count, 0);
    }
    #[test]
    fn retired_sql_body_database_is_not_opened_silently() {
        // Reuse the actual file-backed opening path to exercise the guard.
        let path = std::env::temp_dir().join(format!(
            "orbit-retired-{}-{}.db",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let result = (|| -> Result<(), AuthorityError> {
            let disk = Connection::open(&path)?;
            disk.execute_batch("CREATE TABLE authority_knowledge_items(id TEXT, body TEXT);")?;
            drop(disk);
            assert!(matches!(
                AuthorityStore::open(&path),
                Err(AuthorityError::MigrationConflict)
            ));
            Ok(())
        })();
        let _ = std::fs::remove_file(&path);
        result.unwrap();
    }
}

#[cfg(test)]
mod m20_task_planning_acceptance {
    use super::*;
    use crate::domain_commands::{TaskAction, TaskCommand};
    fn cmd(id: &str, revision: i64, action: TaskAction) -> TaskCommand {
        TaskCommand {
            command_id: id.into(),
            expected_revision: revision,
            action,
        }
    }
    #[test]
    fn planning_is_atomic_queryable_retryable_and_history_backed() {
        let mut store = AuthorityStore::open(Path::new(":memory:")).unwrap();
        store
            .authority_task_command(
                "local",
                cmd(
                    "create",
                    0,
                    TaskAction::Create {
                        task_id: "task".into(),
                        title: "Prepare".into(),
                        project_id: None,
                    },
                ),
            )
            .unwrap();
        let set = cmd(
            "planning",
            1,
            TaskAction::SetPlanning {
                task_id: "task".into(),
                estimated_minutes: Some(45),
                earliest_start_at_ms: Some(1000),
                deadline_at_ms: Some(3000),
            },
        );
        let first = store.authority_task_command("local", set.clone()).unwrap();
        assert_eq!(first.revision, 2);
        assert_eq!(first.task.estimated_minutes, Some(45));
        assert!(
            store
                .authority_task_command("local", set)
                .unwrap()
                .duplicate
        );
        assert_eq!(
            store.authority_full_snapshot().unwrap().3[0].deadline_at_ms,
            Some(3000)
        );
        assert_eq!(store.event_count().unwrap(), 2);
        for (id, minutes, start, end) in [
            ("invalid-duration", Some(0), None, None),
            ("invalid-range", Some(30), Some(500), Some(200)),
            ("invalid-negative", None, Some(-1), None),
        ] {
            assert!(
                store
                    .authority_task_command(
                        "local",
                        cmd(
                            id,
                            2,
                            TaskAction::SetPlanning {
                                task_id: "task".into(),
                                estimated_minutes: minutes,
                                earliest_start_at_ms: start,
                                deadline_at_ms: end
                            }
                        )
                    )
                    .is_err()
            );
        }
        assert_eq!(store.event_count().unwrap(), 2);
        let renamed = store
            .authority_task_command(
                "local",
                cmd(
                    "rename",
                    2,
                    TaskAction::Rename {
                        task_id: "task".into(),
                        title: "Prepare well".into(),
                    },
                ),
            )
            .unwrap();
        assert_eq!(renamed.task.estimated_minutes, Some(45));
        let clear = store
            .authority_task_command(
                "local",
                cmd(
                    "clear",
                    3,
                    TaskAction::SetPlanning {
                        task_id: "task".into(),
                        estimated_minutes: None,
                        earliest_start_at_ms: None,
                        deadline_at_ms: None,
                    },
                ),
            )
            .unwrap();
        assert_eq!(clear.task.deadline_at_ms, None);
        assert_eq!(store.tasks().unwrap().1[0].estimated_minutes, None);
        assert_eq!(store.event_count().unwrap(), 4);
    }
}

#[cfg(test)]
mod database_mode_isolation_tests {
    use super::*;

    #[test]
    fn local_authority_refuses_replica_without_changing_its_schema() {
        let dir = std::env::temp_dir().join(format!(
            "orbit-mode-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("replica.sqlite");
        let conn = rusqlite::Connection::open(&path).unwrap();
        conn.execute_batch("CREATE TABLE authority_outbox (command_id TEXT PRIMARY KEY);")
            .unwrap();
        drop(conn);
        assert!(matches!(
            AuthorityStore::open(&path),
            Err(AuthorityError::Invalid(_))
        ));
        let check = rusqlite::Connection::open(&path).unwrap();
        let count: i64 = check.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='authority_identity'",
            [], |r| r.get(0),
        ).unwrap();
        assert_eq!(count, 0);
        drop(check);
        std::fs::remove_dir_all(dir).unwrap();
    }
}

mod authoritative_task_detail_tests {
    use crate::AuthoritativeHost;
    use crate::AuthorityError;
    use crate::domain_commands::{AcceptanceCriteriaAction, TaskAction, TaskCommand, TaskRecord};

    fn command(id: &str, revision: i64, action: TaskAction) -> TaskCommand {
        TaskCommand {
            command_id: id.into(),
            expected_revision: revision,
            action,
        }
    }

    fn create(host: &mut AuthoritativeHost, id: &str) {
        host.task_command(
            "device-a",
            command(
                &format!("create-{id}"),
                0,
                TaskAction::Create {
                    task_id: id.into(),
                    title: "Original".into(),
                    project_id: None,
                },
            ),
        )
        .expect("create task");
    }

    #[test]
    fn set_details_are_accepted_once_and_invalid_priority_has_no_ledger_entry() {
        let mut host = AuthoritativeHost::open(":memory:").unwrap();
        create(&mut host, "task-1");
        let details = command(
            "details-1",
            1,
            TaskAction::SetDetails {
                task_id: "task-1".into(),
                description: "Rich description".into(),
                priority: Some("high".into()),
                labels: vec!["backend".into(), "api".into()],
            },
        );
        let first = host
            .task_command("device-a", details.clone())
            .expect("set details");
        assert_eq!((first.revision, first.duplicate), (2, false));
        assert_eq!(first.task.description, "Rich description");
        assert_eq!(first.task.priority.as_deref(), Some("high"));
        assert_eq!(first.task.labels, ["api", "backend"]);
        // The connected create appended a Created activity; the rich detail
        // command appends its own Updated activity after it.
        assert_eq!(host.application_queries().activities().unwrap().len(), 2);
        let activity = &host.application_queries().activities().unwrap()[1];
        assert_eq!(
            activity.subject().to_owned(),
            orbit_domain::ActivitySubject::Task(orbit_domain::TaskId::new("task-1").unwrap())
        );
        assert!(matches!(
            activity.origin(),
            orbit_domain::ActivityOrigin::External(actor) if actor == "device-a"
        ));

        let duplicate = host
            .task_command("device-a", details)
            .expect("duplicate details receipt");
        assert_eq!((duplicate.revision, duplicate.duplicate), (2, true));
        assert_eq!(host.application_queries().activities().unwrap().len(), 2);
        assert_eq!(host.snapshot().unwrap().0, 2);

        let before = host.snapshot().unwrap().0;
        let rejected = host.task_command(
            "device-a",
            command(
                "details-bad-priority",
                2,
                TaskAction::SetDetails {
                    task_id: "task-1".into(),
                    description: "Never stored".into(),
                    priority: Some("whenever".into()),
                    labels: Vec::new(),
                },
            ),
        );
        assert!(matches!(rejected, Err(AuthorityError::Invalid(_))));
        assert_eq!(host.snapshot().unwrap().0, before);
        assert_eq!(
            host.task_record("task-1").unwrap().unwrap().description,
            "Rich description"
        );
    }

    #[test]
    fn acceptance_operations_are_ordered_idempotent_and_never_change_status() {
        let mut host = AuthoritativeHost::open(":memory:").unwrap();
        create(&mut host, "task-1");
        let add_first = command(
            "ac-add-first",
            1,
            TaskAction::AcceptanceCriteria {
                task_id: "task-1".into(),
                change: AcceptanceCriteriaAction::Add {
                    criterion_id: "ac-1".into(),
                    text: "Works offline".into(),
                },
            },
        );
        assert_eq!(
            host.task_command("device-a", add_first.clone())
                .unwrap()
                .revision,
            2
        );
        host.task_command(
            "device-a",
            command(
                "ac-add-second",
                2,
                TaskAction::AcceptanceCriteria {
                    task_id: "task-1".into(),
                    change: AcceptanceCriteriaAction::Add {
                        criterion_id: "ac-2".into(),
                        text: "Syncs safely".into(),
                    },
                },
            ),
        )
        .unwrap();
        let completed = host
            .task_command(
                "device-a",
                command(
                    "ac-complete",
                    3,
                    TaskAction::AcceptanceCriteria {
                        task_id: "task-1".into(),
                        change: AcceptanceCriteriaAction::SetCompleted {
                            criterion_id: "ac-2".into(),
                            completed: true,
                        },
                    },
                ),
            )
            .unwrap();
        assert_eq!(completed.revision, 4);
        assert_eq!(host.task_record("task-1").unwrap().unwrap().status, "inbox");
        host.task_command(
            "device-a",
            command(
                "ac-complete-first",
                4,
                TaskAction::AcceptanceCriteria {
                    task_id: "task-1".into(),
                    change: AcceptanceCriteriaAction::SetCompleted {
                        criterion_id: "ac-1".into(),
                        completed: true,
                    },
                },
            ),
        )
        .unwrap();
        let all_complete = host
            .acceptance_criteria(&orbit_domain::TaskId::new("task-1").unwrap())
            .unwrap();
        assert!(all_complete.all_completed());
        assert_eq!(host.task_record("task-1").unwrap().unwrap().status, "inbox");
        host.task_command(
            "device-a",
            command(
                "ac-reorder",
                5,
                TaskAction::AcceptanceCriteria {
                    task_id: "task-1".into(),
                    change: AcceptanceCriteriaAction::Reorder {
                        criterion_id: "ac-2".into(),
                        position: 0,
                    },
                },
            ),
        )
        .unwrap();
        let removed = host
            .task_command(
                "device-a",
                command(
                    "ac-remove",
                    6,
                    TaskAction::AcceptanceCriteria {
                        task_id: "task-1".into(),
                        change: AcceptanceCriteriaAction::Remove {
                            criterion_id: "ac-1".into(),
                        },
                    },
                ),
            )
            .unwrap();
        assert_eq!(removed.revision, 7);
        let criteria = host
            .acceptance_criteria(&orbit_domain::TaskId::new("task-1").unwrap())
            .unwrap();
        assert_eq!(criteria.len(), 1);
        assert_eq!(criteria.items()[0].id().as_str(), "ac-2");
        assert!(criteria.items()[0].is_completed());
        assert_eq!(host.task_record("task-1").unwrap().unwrap().status, "inbox");

        // Retrying the first command id returns the original receipt even
        // though the criteria list has moved on.
        let duplicate = host
            .task_command("device-a", add_first)
            .expect("duplicate acceptance receipt");
        assert_eq!((duplicate.revision, duplicate.duplicate), (2, true));
        assert_eq!(
            host.acceptance_criteria(&orbit_domain::TaskId::new("task-1").unwrap())
                .unwrap()
                .len(),
            1
        );
        assert_eq!(host.snapshot().unwrap().0, 7);
    }

    #[test]
    fn raw_and_rich_task_actions_replay_into_one_snapshot() {
        let mut host = AuthoritativeHost::open(":memory:").unwrap();
        create(&mut host, "task-1");
        host.task_command(
            "device-a",
            command(
                "details",
                1,
                TaskAction::SetDetails {
                    task_id: "task-1".into(),
                    description: "Snapshot description".into(),
                    priority: Some("urgent".into()),
                    labels: vec!["release".into()],
                },
            ),
        )
        .unwrap();
        host.task_command(
            "device-a",
            command(
                "acceptance",
                2,
                TaskAction::AcceptanceCriteria {
                    task_id: "task-1".into(),
                    change: AcceptanceCriteriaAction::Add {
                        criterion_id: "ac-1".into(),
                        text: "Ready".into(),
                    },
                },
            ),
        )
        .unwrap();
        let changed = host
            .task_command(
                "device-a",
                command(
                    "status",
                    3,
                    TaskAction::ChangeStatus {
                        task_id: "task-1".into(),
                        status: "ready".into(),
                    },
                ),
            )
            .unwrap();
        assert_eq!(changed.task.status, "ready");
        assert_eq!(changed.task.description, "Snapshot description");

        let (revision, _, _, tasks, _) = host.snapshot().unwrap();
        assert_eq!(revision, 4);
        let task = tasks.iter().find(|task| task.id == "task-1").unwrap();
        assert_eq!(task.status, "ready");
        assert_eq!(task.description, "Snapshot description");
        assert_eq!(task.priority.as_deref(), Some("urgent"));
        assert_eq!(task.labels, ["release"]);
        assert_eq!(
            host.tasks().unwrap().1[0].description,
            "Snapshot description"
        );
    }

    #[test]
    fn old_task_payloads_deserialize_and_new_records_serialize_additively() {
        let old = serde_json::json!({
            "id": "task-1",
            "title": "Original",
            "status": "inbox",
            "estimated_minutes": null,
            "earliest_start_at_ms": null,
            "deadline_at_ms": null
        });
        let record: TaskRecord = serde_json::from_value(old).unwrap();
        assert_eq!(record.description, "");
        assert_eq!(record.priority, None);
        assert!(record.labels.is_empty());

        let old_action: TaskAction = serde_json::from_value(serde_json::json!({
            "type": "rename",
            "task_id": "task-1",
            "title": "Renamed"
        }))
        .unwrap();
        assert!(matches!(old_action, TaskAction::Rename { .. }));

        let new_action: TaskAction = serde_json::from_value(serde_json::json!({
            "type": "acceptance_criteria",
            "task_id": "task-1",
            "change": {
                "operation": "set_completed",
                "criterion_id": "ac-1",
                "completed": true
            }
        }))
        .unwrap();
        assert!(matches!(
            new_action,
            TaskAction::AcceptanceCriteria {
                change: AcceptanceCriteriaAction::SetCompleted {
                    completed: true,
                    ..
                },
                ..
            }
        ));
        let details_command: TaskCommand = serde_json::from_value(serde_json::json!({
            "command_id": "details",
            "expected_revision": 1,
            "action": {
                "type": "set_details",
                "task_id": "task-1",
                "description": "Body",
                "priority": null,
                "labels": ["one"]
            }
        }))
        .unwrap();
        assert!(matches!(
            details_command.action,
            TaskAction::SetDetails { .. }
        ));

        let details = TaskRecord {
            id: "task-1".into(),
            title: "Original".into(),
            status: "inbox".into(),
            project_id: None,
            milestone_id: None,
            estimated_minutes: None,
            earliest_start_at_ms: None,
            deadline_at_ms: None,
            description: "Body".into(),
            priority: Some("low".into()),
            labels: vec!["one".into()],
        };
        let json = serde_json::to_value(&details).unwrap();
        assert_eq!(json["description"], "Body");
        assert_eq!(json["priority"], "low");
        assert_eq!(json["labels"], serde_json::json!(["one"]));
    }
}
