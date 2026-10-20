//! Canonical routine composition through the command ledger.
//!
//! A routine owns an ordered selection of existing habits. Composition is
//! replaced atomically (never as a partially written list), in the same
//! immediate transaction as revision fencing and its immutable receipt.
use crate::command_engine::{CommandAdmission, PendingCommand};
use crate::store::{AuthorityError, AuthorityStore};
use orbit_sync::{ChangeEnvelope, EntityKind, EntityRef, FieldMutation, SemanticChange};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RoutineCommand {
    pub command_id: String,
    pub expected_revision: i64,
    pub action: RoutineAction,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum RoutineAction {
    Create {
        routine_id: String,
        name: String,
        habit_ids: Vec<String>,
    },
    Rename {
        routine_id: String,
        name: String,
    },
    SetHabits {
        routine_id: String,
        habit_ids: Vec<String>,
    },
    Delete {
        routine_id: String,
    },
}
impl RoutineAction {
    pub fn routine_id(&self) -> &str {
        match self {
            Self::Create { routine_id, .. }
            | Self::Rename { routine_id, .. }
            | Self::SetHabits { routine_id, .. }
            | Self::Delete { routine_id } => routine_id,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RoutineRecord {
    pub id: String,
    pub name: String,
    pub habit_ids: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RoutineResult {
    pub revision: i64,
    pub duplicate: bool,
    pub routine: Option<RoutineRecord>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RoutineSnapshot {
    pub revision: i64,
    pub routines: Vec<RoutineRecord>,
}

fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
}
fn check_name(name: &str) -> Result<(), AuthorityError> {
    if name.trim().is_empty() || name.len() > 4096 {
        Err(AuthorityError::Invalid("invalid routine name"))
    } else {
        Ok(())
    }
}
fn check_habits(conn: &Connection, habits: &[String]) -> Result<(), AuthorityError> {
    if habits.len() > 500 {
        return Err(AuthorityError::Invalid("too many habits in routine"));
    }
    let mut unique = HashSet::new();
    for id in habits {
        if !valid_id(id) || !unique.insert(id.as_str()) {
            return Err(AuthorityError::Invalid(
                "invalid or duplicate routine habit",
            ));
        }
        let present: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM habits WHERE id=?1)",
            [id],
            |row| row.get(0),
        )?;
        if !present {
            return Err(AuthorityError::Invalid("routine references missing habit"));
        }
    }
    Ok(())
}
fn fetch(conn: &Connection, id: &str) -> Result<Option<RoutineRecord>, AuthorityError> {
    let name: Option<String> = conn
        .query_row("SELECT name FROM routines WHERE id=?1", [id], |row| {
            row.get(0)
        })
        .optional()?;
    let Some(name) = name else {
        return Ok(None);
    };
    let mut statement = conn.prepare(
        "SELECT habit_id FROM routine_habits WHERE routine_id=?1 ORDER BY position,habit_id",
    )?;
    let habit_ids = statement
        .query_map([id], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Some(RoutineRecord {
        id: id.into(),
        name,
        habit_ids,
    }))
}
fn required(conn: &Connection, id: &str) -> Result<RoutineRecord, AuthorityError> {
    fetch(conn, id)?.ok_or(AuthorityError::Invalid("routine does not exist"))
}
fn replace_habits(conn: &Connection, id: &str, habits: &[String]) -> Result<(), AuthorityError> {
    // Validate *before* removing existing composition. The enclosing tx
    // protects both from errors and concurrent command acceptance.
    check_habits(conn, habits)?;
    conn.execute("DELETE FROM routine_habits WHERE routine_id=?1", [id])?;
    for (position, habit_id) in habits.iter().enumerate() {
        conn.execute(
            "INSERT INTO routine_habits(routine_id,habit_id,position) VALUES(?1,?2,?3)",
            params![id, habit_id, position as i64],
        )?;
    }
    Ok(())
}
fn latest_field_version(
    conn: &Connection,
    id: &str,
    field: &str,
) -> Result<Option<orbit_sync::ChangeId>, AuthorityError> {
    let mut statement = conn
        .prepare("SELECT wire FROM authority_history WHERE kind='change' ORDER BY cursor DESC")?;
    let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
    for row in rows {
        let envelope =
            ChangeEnvelope::from_wire_json(&row?).map_err(|_| AuthorityError::MigrationConflict)?;
        if envelope.change.entity.kind == EntityKind::Routine
            && envelope.change.entity.id == id
            && envelope
                .change
                .mutations
                .iter()
                .any(|item| item.field == field)
        {
            return Ok(Some(envelope.id));
        }
    }
    Ok(None)
}
fn field(
    conn: &Connection,
    id: &str,
    name: &str,
    value: Option<String>,
) -> Result<FieldMutation, AuthorityError> {
    FieldMutation::new(name, value, latest_field_version(conn, id, name)?)
        .map_err(|_| AuthorityError::Invalid("invalid routine mutation"))
}
fn serialized_habits(habits: &[String]) -> Result<String, AuthorityError> {
    serde_json::to_string(habits)
        .map_err(|_| AuthorityError::Invalid("routine habit serialization"))
}

impl AuthorityStore {
    /// Accepted routines and membership order, read at one SQLite revision.
    pub fn authority_routines(&self) -> Result<RoutineSnapshot, AuthorityError> {
        let tx = self.conn.unchecked_transaction()?;
        let revision: i64 = tx.query_row(
            "SELECT COALESCE(MAX(cursor),0) FROM authority_history",
            [],
            |row| row.get(0),
        )?;
        let ids = {
            let mut statement = tx.prepare("SELECT id FROM routines ORDER BY id")?;
            statement
                .query_map([], |row| row.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?
        };
        let mut routines = Vec::with_capacity(ids.len());
        for id in ids {
            if !valid_id(&id) {
                return Err(AuthorityError::Invalid("invalid stored routine ID"));
            }
            let record = required(&tx, &id)?;
            check_name(&record.name)?;
            check_habits(&tx, &record.habit_ids)?;
            routines.push(record);
        }
        tx.commit()?;
        Ok(RoutineSnapshot { revision, routines })
    }

    /// The routine entity, membership rows, semantic event and retry receipt
    /// commit atomically. Failed operations cannot advance the revision.
    pub fn authority_routine_command(
        &mut self,
        actor: &str,
        command: RoutineCommand,
    ) -> Result<RoutineResult, AuthorityError> {
        if actor.trim().is_empty()
            || !valid_id(&command.command_id)
            || command.expected_revision < 0
            || !valid_id(command.action.routine_id())
        {
            return Err(AuthorityError::Invalid("invalid routine command envelope"));
        }
        let payload = serde_json::to_string(&command)
            .map_err(|_| AuthorityError::Invalid("routine serialization"))?;
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let pending = PendingCommand {
            command_id: &command.command_id,
            actor,
            payload: &payload,
            expected_revision: command.expected_revision,
        };
        let revision = match pending.check(&tx)? {
            CommandAdmission::Duplicate(raw) => {
                let mut result: RoutineResult = serde_json::from_str(&raw)
                    .map_err(|_| AuthorityError::Invalid("invalid stored routine receipt"))?;
                result.duplicate = true;
                return Ok(result);
            }
            CommandAdmission::New(revision) => revision,
        };
        let id = command.action.routine_id().to_owned();
        let (routine, mutations) = match command.action {
            RoutineAction::Create {
                name, habit_ids, ..
            } => {
                check_name(&name)?;
                check_habits(&tx, &habit_ids)?;
                if tx.execute(
                    "INSERT OR IGNORE INTO routines(id,name) VALUES(?1,?2)",
                    params![id, name],
                )? != 1
                {
                    return Err(AuthorityError::ImmutableConflict);
                }
                replace_habits(&tx, &id, &habit_ids)?;
                let record = required(&tx, &id)?;
                let changes = vec![
                    field(&tx, &id, "name", Some(name))?,
                    field(&tx, &id, "habit_ids", Some(serialized_habits(&habit_ids)?))?,
                ];
                (Some(record), changes)
            }
            RoutineAction::Rename { name, .. } => {
                required(&tx, &id)?;
                check_name(&name)?;
                let mutation = field(&tx, &id, "name", Some(name.clone()))?;
                tx.execute("UPDATE routines SET name=?1 WHERE id=?2", params![name, id])?;
                (Some(required(&tx, &id)?), vec![mutation])
            }
            RoutineAction::SetHabits { habit_ids, .. } => {
                required(&tx, &id)?;
                check_habits(&tx, &habit_ids)?;
                let mutation = field(&tx, &id, "habit_ids", Some(serialized_habits(&habit_ids)?))?;
                replace_habits(&tx, &id, &habit_ids)?;
                (Some(required(&tx, &id)?), vec![mutation])
            }
            RoutineAction::Delete { .. } => {
                required(&tx, &id)?;
                let mutation = field(&tx, &id, "deleted", Some("true".into()))?;
                tx.execute("DELETE FROM routines WHERE id=?1", [&id])?;
                (None, vec![mutation])
            }
        };
        let semantic = SemanticChange::new(
            EntityRef::new(EntityKind::Routine, id)
                .map_err(|_| AuthorityError::Invalid("invalid routine entity"))?,
            mutations,
        )
        .map_err(|_| AuthorityError::Invalid("invalid routine semantic change"))?;
        let result = RoutineResult {
            revision,
            duplicate: false,
            routine,
        };
        let receipt = serde_json::to_string(&result)
            .map_err(|_| AuthorityError::Invalid("routine receipt serialization"))?;
        pending.commit(&tx, revision, semantic, &receipt)?;
        tx.commit()?;
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::authority_evidence::{EvidenceAction, EvidenceCommand};
    use std::path::Path;
    fn db() -> AuthorityStore {
        AuthorityStore::open(Path::new(":memory:")).unwrap()
    }
    fn command(id: &str, revision: i64, action: RoutineAction) -> RoutineCommand {
        RoutineCommand {
            command_id: id.into(),
            expected_revision: revision,
            action,
        }
    }
    fn create_habit(store: &mut AuthorityStore, id: &str, revision: i64) {
        store
            .authority_evidence_command(
                "actor",
                EvidenceCommand {
                    command_id: format!("cmd-{id}"),
                    expected_revision: revision,
                    action: EvidenceAction::CreateHabit {
                        habit_id: id.into(),
                        name: format!("habit {id}"),
                        cadence: "daily".into(),
                    },
                },
            )
            .unwrap();
    }
    #[test]
    fn full_routine_lifecycle_and_order_survive_reopening() {
        let path = std::env::temp_dir().join(format!(
            "orbit-routine-{}-{}.db",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        {
            let mut store = AuthorityStore::open(&path).unwrap();
            create_habit(&mut store, "a", 0);
            create_habit(&mut store, "b", 1);
            let create = command(
                "create",
                2,
                RoutineAction::Create {
                    routine_id: "morning".into(),
                    name: "Morning".into(),
                    habit_ids: vec!["b".into(), "a".into()],
                },
            );
            assert_eq!(
                store
                    .authority_routine_command("actor", create.clone())
                    .unwrap()
                    .revision,
                3
            );
            assert!(
                store
                    .authority_routine_command("actor", create)
                    .unwrap()
                    .duplicate
            );
            assert_eq!(
                store.authority_routines().unwrap().routines[0].habit_ids,
                vec!["b", "a"]
            );
            store
                .authority_routine_command(
                    "actor",
                    command(
                        "rename",
                        3,
                        RoutineAction::Rename {
                            routine_id: "morning".into(),
                            name: "Start day".into(),
                        },
                    ),
                )
                .unwrap();
            store
                .authority_routine_command(
                    "actor",
                    command(
                        "reorder",
                        4,
                        RoutineAction::SetHabits {
                            routine_id: "morning".into(),
                            habit_ids: vec!["a".into(), "b".into()],
                        },
                    ),
                )
                .unwrap();
        }
        {
            let mut store = AuthorityStore::open(&path).unwrap();
            let saved = store.authority_routines().unwrap();
            assert_eq!(saved.revision, 5);
            assert_eq!(saved.routines[0].name, "Start day");
            assert_eq!(saved.routines[0].habit_ids, vec!["a", "b"]);
            let deleted = store
                .authority_routine_command(
                    "actor",
                    command(
                        "delete",
                        5,
                        RoutineAction::Delete {
                            routine_id: "morning".into(),
                        },
                    ),
                )
                .unwrap();
            assert!(deleted.routine.is_none());
            assert!(store.authority_routines().unwrap().routines.is_empty());
            assert!(
                store
                    .authority_routine_command(
                        "actor",
                        command(
                            "delete",
                            5,
                            RoutineAction::Delete {
                                routine_id: "morning".into(),
                            }
                        )
                    )
                    .unwrap()
                    .duplicate
            );
        }
        let _ = std::fs::remove_file(path);
    }
    #[test]
    fn invalid_memberships_do_not_advance_revision_or_lose_existing_rows() {
        let mut store = db();
        create_habit(&mut store, "a", 0);
        store
            .authority_routine_command(
                "actor",
                command(
                    "create",
                    1,
                    RoutineAction::Create {
                        routine_id: "r".into(),
                        name: "Routine".into(),
                        habit_ids: vec!["a".into()],
                    },
                ),
            )
            .unwrap();
        for (index, habits) in [vec!["a".into(), "a".into()], vec!["missing".into()]]
            .into_iter()
            .enumerate()
        {
            assert!(
                store
                    .authority_routine_command(
                        "actor",
                        command(
                            &format!("bad-{index}"),
                            2,
                            RoutineAction::SetHabits {
                                routine_id: "r".into(),
                                habit_ids: habits
                            }
                        )
                    )
                    .is_err()
            );
            assert_eq!(store.authority_routines().unwrap().revision, 2);
            assert_eq!(
                store.authority_routines().unwrap().routines[0].habit_ids,
                vec!["a"]
            );
        }
    }
    #[test]
    fn stale_revision_and_changed_retry_are_rejected() {
        let mut store = db();
        let create = command(
            "first",
            0,
            RoutineAction::Create {
                routine_id: "r".into(),
                name: "Routine".into(),
                habit_ids: vec![],
            },
        );
        store
            .authority_routine_command("actor", create.clone())
            .unwrap();
        let mut changed = create.clone();
        changed.action = RoutineAction::Delete {
            routine_id: "r".into(),
        };
        assert!(store.authority_routine_command("actor", changed).is_err());
        assert!(store.authority_routine_command("other", create).is_err());
        assert!(
            store
                .authority_routine_command(
                    "actor",
                    command(
                        "stale",
                        0,
                        RoutineAction::Rename {
                            routine_id: "r".into(),
                            name: "New".into(),
                        }
                    )
                )
                .is_err()
        );
    }
    #[test]
    fn order_can_be_cleared_then_replaced_without_duplicate_memberships() {
        let mut store = db();
        create_habit(&mut store, "alpha", 0);
        create_habit(&mut store, "beta", 1);
        create_habit(&mut store, "gamma", 2);
        store
            .authority_routine_command(
                "actor",
                command(
                    "create",
                    3,
                    RoutineAction::Create {
                        routine_id: "r".into(),
                        name: "Checklist".into(),
                        habit_ids: vec!["alpha".into(), "beta".into(), "gamma".into()],
                    },
                ),
            )
            .unwrap();
        let steps = [
            ("clear", vec![]),
            (
                "reverse",
                vec!["gamma".into(), "beta".into(), "alpha".into()],
            ),
            ("shorten", vec!["beta".into()]),
        ];
        for (index, (id, habits)) in steps.into_iter().enumerate() {
            let result = store
                .authority_routine_command(
                    "actor",
                    command(
                        id,
                        4 + index as i64,
                        RoutineAction::SetHabits {
                            routine_id: "r".into(),
                            habit_ids: habits.clone(),
                        },
                    ),
                )
                .unwrap();
            assert_eq!(result.routine.unwrap().habit_ids, habits);
            assert_eq!(
                store.authority_routines().unwrap().routines[0].habit_ids,
                habits
            );
        }
        assert_eq!(store.event_count().unwrap(), 7);
    }

    #[test]
    fn failed_renames_and_cross_actor_retries_preserve_the_accepted_routine() {
        let mut store = db();
        let created = command(
            "create",
            0,
            RoutineAction::Create {
                routine_id: "r".into(),
                name: "Original".into(),
                habit_ids: vec![],
            },
        );
        store
            .authority_routine_command("alice", created.clone())
            .unwrap();
        for name in ["", "   "] {
            assert!(
                store
                    .authority_routine_command(
                        "alice",
                        command(
                            "bad-rename",
                            1,
                            RoutineAction::Rename {
                                routine_id: "r".into(),
                                name: name.into()
                            }
                        )
                    )
                    .is_err()
            );
        }
        assert!(store.authority_routine_command("bob", created).is_err());
        assert!(
            store
                .authority_routine_command(
                    "alice",
                    command(
                        "no-such-routine",
                        1,
                        RoutineAction::Rename {
                            routine_id: "other".into(),
                            name: "Changed".into()
                        }
                    )
                )
                .is_err()
        );
        let snapshot = store.authority_routines().unwrap();
        assert_eq!(snapshot.revision, 1);
        assert_eq!(snapshot.routines[0].name, "Original");
        assert_eq!(store.event_count().unwrap(), 1);
    }

    #[test]
    fn routine_create_rejects_unknown_habits_without_leaving_an_empty_routine() {
        let mut store = db();
        let invalid = command(
            "unknown",
            0,
            RoutineAction::Create {
                routine_id: "r".into(),
                name: "Routine".into(),
                habit_ids: vec!["missing".into()],
            },
        );
        assert!(store.authority_routine_command("actor", invalid).is_err());
        assert!(store.authority_routines().unwrap().routines.is_empty());
        assert_eq!(store.event_count().unwrap(), 0);
        create_habit(&mut store, "missing", 0);
        store
            .authority_routine_command(
                "actor",
                command(
                    "valid",
                    1,
                    RoutineAction::Create {
                        routine_id: "r".into(),
                        name: "Routine".into(),
                        habit_ids: vec!["missing".into()],
                    },
                ),
            )
            .unwrap();
        assert_eq!(
            store.authority_routines().unwrap().routines[0].habit_ids,
            vec!["missing"]
        );
    }
}
