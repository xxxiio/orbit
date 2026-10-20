//! Authoritative schedule-block lifecycle.
//!
//! This module is deliberately transport independent. The same command is
//! accepted by an embedded authority and by the optional self-hosted HTTP
//! authority. Every mutation, semantic change, receipt and projection is
//! committed in one immediate SQLite transaction.
//!
//! The schedule is a set of half-open intervals [start,end). Adjacent blocks
//! are legal, overlapping blocks are not. The overlap predicate executes
//! inside the write transaction, not in a client-side preflight query.
use crate::command_engine::{CommandAdmission, PendingCommand};
use crate::store::{AuthorityError, AuthorityStore};
use orbit_sync::{EntityKind, EntityRef, FieldMutation, SemanticChange};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScheduleCommand {
    pub command_id: String,
    pub expected_revision: i64,
    pub action: ScheduleAction,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum ScheduleAction {
    Create {
        block_id: String,
        title: String,
        starts_at_ms: i64,
        duration_minutes: u32,
        flexibility: String,
        target_kind: String,
        target_id: Option<String>,
    },
    Reschedule {
        block_id: String,
        starts_at_ms: i64,
    },
    Resize {
        block_id: String,
        duration_minutes: u32,
    },
    Rename {
        block_id: String,
        title: String,
    },
    Retarget {
        block_id: String,
        target_kind: String,
        target_id: Option<String>,
    },
    Delete {
        block_id: String,
    },
}

impl ScheduleAction {
    pub fn block_id(&self) -> &str {
        match self {
            Self::Create { block_id, .. }
            | Self::Reschedule { block_id, .. }
            | Self::Resize { block_id, .. }
            | Self::Rename { block_id, .. }
            | Self::Retarget { block_id, .. }
            | Self::Delete { block_id } => block_id,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScheduleRecord {
    pub id: String,
    pub title: String,
    pub starts_at_ms: i64,
    pub ends_at_ms: i64,
    pub duration_minutes: u32,
    pub flexibility: String,
    pub target_kind: String,
    pub target_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScheduleResult {
    pub revision: i64,
    pub duplicate: bool,
    pub block: Option<ScheduleRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScheduleSnapshot {
    pub revision: i64,
    pub blocks: Vec<ScheduleRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScheduleConflict {
    pub id: String,
    pub title: String,
    pub starts_at_ms: i64,
    pub ends_at_ms: i64,
}

fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 256
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
}

fn checked_end(start: i64, duration: u32) -> Result<i64, AuthorityError> {
    if duration == 0 {
        return Err(AuthorityError::Invalid(
            "schedule duration must be positive",
        ));
    }
    let delta = i64::from(duration)
        .checked_mul(60_000)
        .ok_or(AuthorityError::Invalid("schedule duration overflow"))?;
    start
        .checked_add(delta)
        .ok_or(AuthorityError::Invalid("schedule timestamp overflow"))
}

fn check_title(title: &str) -> Result<(), AuthorityError> {
    if title.trim().is_empty() || title.len() > 4096 {
        return Err(AuthorityError::Invalid("invalid schedule title"));
    }
    Ok(())
}

fn check_flexibility(value: &str) -> Result<(), AuthorityError> {
    if value != "fixed" && value != "movable" {
        return Err(AuthorityError::Invalid("invalid schedule flexibility"));
    }
    Ok(())
}

fn check_target(tx: &Connection, kind: &str, id: Option<&str>) -> Result<(), AuthorityError> {
    let table = match (kind, id) {
        ("standalone", None) => return Ok(()),
        ("task", Some(id)) if valid_id(id) => "tasks",
        ("habit", Some(id)) if valid_id(id) => "habits",
        _ => return Err(AuthorityError::Invalid("invalid schedule target")),
    };
    let found: bool = tx.query_row(
        &format!("SELECT EXISTS(SELECT 1 FROM {table} WHERE id=?1)"),
        [id.unwrap_or_default()],
        |row| row.get(0),
    )?;
    if !found {
        return Err(AuthorityError::Invalid("schedule target does not exist"));
    }
    Ok(())
}

fn row_to_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<ScheduleRecord> {
    let start: i64 = row.get(2)?;
    let raw_duration: i64 = row.get(3)?;
    let duration = u32::try_from(raw_duration)
        .map_err(|_| rusqlite::Error::IntegralValueOutOfRange(3, raw_duration))?;
    let end = checked_end(start, duration)
        .map_err(|_| rusqlite::Error::IntegralValueOutOfRange(3, raw_duration))?;
    Ok(ScheduleRecord {
        id: row.get(0)?,
        title: row.get(1)?,
        starts_at_ms: start,
        ends_at_ms: end,
        duration_minutes: duration,
        flexibility: row.get(4)?,
        target_kind: row.get(5)?,
        target_id: row.get(6)?,
    })
}

fn find_block(tx: &Connection, id: &str) -> Result<Option<ScheduleRecord>, AuthorityError> {
    let mut statement = tx.prepare(
        "SELECT id,title,starts_at_ms,duration_minutes,flexibility,target_kind,target_id \
         FROM schedule_blocks WHERE id=?1",
    )?;
    Ok(statement.query_row([id], row_to_record).optional()?)
}

fn require_block(tx: &Connection, id: &str) -> Result<ScheduleRecord, AuthorityError> {
    find_block(tx, id)?.ok_or(AuthorityError::Invalid("schedule block does not exist"))
}

fn find_conflicts(
    tx: &Connection,
    start: i64,
    end: i64,
    exclude: Option<&str>,
) -> Result<Vec<ScheduleConflict>, AuthorityError> {
    // Guard against malformed stored values: an overflowed interval must not
    // be silently ignored by a range predicate. The write path only produces
    // valid records; this is defense-in-depth against external DB modification.
    let mut statement = tx.prepare(
        "SELECT id,title,starts_at_ms,duration_minutes,flexibility,target_kind,target_id \
         FROM schedule_blocks ORDER BY starts_at_ms,id",
    )?;
    let rows = statement.query_map([], row_to_record)?;
    let mut conflicts = Vec::new();
    for row in rows {
        let existing = row?;
        if exclude == Some(existing.id.as_str()) {
            continue;
        }
        if start < existing.ends_at_ms && existing.starts_at_ms < end {
            conflicts.push(ScheduleConflict {
                id: existing.id,
                title: existing.title,
                starts_at_ms: existing.starts_at_ms,
                ends_at_ms: existing.ends_at_ms,
            });
        }
    }
    Ok(conflicts)
}

fn ensure_free(
    tx: &Connection,
    start: i64,
    end: i64,
    exclude: Option<&str>,
) -> Result<(), AuthorityError> {
    if !find_conflicts(tx, start, end, exclude)?.is_empty() {
        return Err(AuthorityError::ImmutableConflict);
    }
    Ok(())
}

fn mutation(name: &str, value: Option<String>) -> Result<FieldMutation, AuthorityError> {
    FieldMutation::new(name, value, None)
        .map_err(|_| AuthorityError::Invalid("invalid schedule semantic field"))
}

/// Preserve optimistic semantic field ancestry. Without a field base, a
/// later rename or reschedule may conflict with the very earlier change it
/// is supposed to extend, even though the SQLite revision check succeeded.
fn latest_schedule_field_version(
    tx: &Connection,
    block_id: &str,
    field: &str,
) -> Result<Option<orbit_sync::ChangeId>, AuthorityError> {
    use orbit_sync::ChangeEnvelope;
    let mut statement =
        tx.prepare("SELECT wire FROM authority_history WHERE kind='change' ORDER BY cursor DESC")?;
    let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
    for row in rows {
        let envelope =
            ChangeEnvelope::from_wire_json(&row?).map_err(|_| AuthorityError::MigrationConflict)?;
        if envelope.change.entity.kind == EntityKind::ScheduleBlock
            && envelope.change.entity.id == block_id
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

fn mutation_with_base(
    tx: &Connection,
    id: &str,
    name: &str,
    value: Option<String>,
) -> Result<FieldMutation, AuthorityError> {
    FieldMutation::new(name, value, latest_schedule_field_version(tx, id, name)?)
        .map_err(|_| AuthorityError::Invalid("invalid schedule field ancestry"))
}

fn mutations_create(record: &ScheduleRecord) -> Result<Vec<FieldMutation>, AuthorityError> {
    Ok(vec![
        mutation("title", Some(record.title.clone()))?,
        mutation("starts_at_ms", Some(record.starts_at_ms.to_string()))?,
        mutation(
            "duration_minutes",
            Some(record.duration_minutes.to_string()),
        )?,
        mutation("flexibility", Some(record.flexibility.clone()))?,
        mutation("target_kind", Some(record.target_kind.clone()))?,
        mutation("target_id", record.target_id.clone())?,
    ])
}

fn validate_stored(record: &ScheduleRecord) -> Result<(), AuthorityError> {
    if !valid_id(&record.id) {
        return Err(AuthorityError::Invalid("invalid stored schedule ID"));
    }
    check_title(&record.title)?;
    check_flexibility(&record.flexibility)?;
    if checked_end(record.starts_at_ms, record.duration_minutes)? != record.ends_at_ms {
        return Err(AuthorityError::Invalid("inconsistent stored schedule end"));
    }
    Ok(())
}

impl AuthorityStore {
    /// Read a consistent schedule snapshot from the canonical SQLite store.
    pub fn authority_schedule(&self) -> Result<ScheduleSnapshot, AuthorityError> {
        let tx = self.conn.unchecked_transaction()?;
        let revision: i64 = tx.query_row(
            "SELECT COALESCE(MAX(cursor),0) FROM authority_history",
            [],
            |row| row.get(0),
        )?;
        let blocks = {
            let mut statement = tx.prepare(
                "SELECT id,title,starts_at_ms,duration_minutes,flexibility,target_kind,target_id \
                 FROM schedule_blocks ORDER BY starts_at_ms,id",
            )?;
            let rows = statement.query_map([], row_to_record)?;
            rows.collect::<Result<Vec<_>, _>>()?
        };
        for record in &blocks {
            validate_stored(record)?;
            check_target(&tx, &record.target_kind, record.target_id.as_deref())?;
        }
        // Fail closed if a corrupted DB already violates exclusivity.
        for pair in blocks.windows(2) {
            if pair[0].ends_at_ms > pair[1].starts_at_ms {
                return Err(AuthorityError::Invalid(
                    "overlapping stored schedule blocks",
                ));
            }
        }
        tx.commit()?;
        Ok(ScheduleSnapshot { revision, blocks })
    }

    /// Query overlaps using half-open intervals; query windows must be valid.
    pub fn authority_schedule_conflicts(
        &self,
        starts_at_ms: i64,
        duration_minutes: u32,
        excluding_id: Option<&str>,
    ) -> Result<Vec<ScheduleConflict>, AuthorityError> {
        let end = checked_end(starts_at_ms, duration_minutes)?;
        let tx = self.conn.unchecked_transaction()?;
        let conflicts = find_conflicts(&tx, starts_at_ms, end, excluding_id)?;
        tx.commit()?;
        Ok(conflicts)
    }

    /// Apply one schedule mutation under the same accepted revision and
    /// immutable-receipt rules as tasks, projects, and evidence commands.
    pub fn authority_schedule_command(
        &mut self,
        actor: &str,
        command: ScheduleCommand,
    ) -> Result<ScheduleResult, AuthorityError> {
        if actor.trim().is_empty()
            || command.command_id.is_empty()
            || command.command_id.len() > 128
            || command.expected_revision < 0
            || !valid_id(command.action.block_id())
        {
            return Err(AuthorityError::Invalid("invalid schedule command envelope"));
        }
        let payload = serde_json::to_string(&command)
            .map_err(|_| AuthorityError::Invalid("schedule serialization"))?;
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
                let mut result: ScheduleResult = serde_json::from_str(&raw)
                    .map_err(|_| AuthorityError::Invalid("corrupt schedule receipt"))?;
                result.duplicate = true;
                return Ok(result);
            }
            CommandAdmission::New(revision) => revision,
        };
        let id = command.action.block_id().to_owned();
        let (block, mutations) = match &command.action {
            ScheduleAction::Create {
                title,
                starts_at_ms,
                duration_minutes,
                flexibility,
                target_kind,
                target_id,
                ..
            } => {
                check_title(title)?;
                check_flexibility(flexibility)?;
                check_target(&tx, target_kind, target_id.as_deref())?;
                let end = checked_end(*starts_at_ms, *duration_minutes)?;
                ensure_free(&tx, *starts_at_ms, end, None)?;
                let changed = tx.execute(
                    "INSERT OR IGNORE INTO schedule_blocks( \
                     id,title,starts_at_ms,duration_minutes,flexibility,target_kind,target_id) \
                     VALUES(?1,?2,?3,?4,?5,?6,?7)",
                    params![
                        id,
                        title,
                        starts_at_ms,
                        duration_minutes,
                        flexibility,
                        target_kind,
                        target_id
                    ],
                )?;
                if changed != 1 {
                    return Err(AuthorityError::ImmutableConflict);
                }
                let record = require_block(&tx, &id)?;
                let changes = mutations_create(&record)?;
                (Some(record), changes)
            }
            ScheduleAction::Reschedule { starts_at_ms, .. } => {
                let before = require_block(&tx, &id)?;
                if before.flexibility != "movable" {
                    return Err(AuthorityError::Invalid("fixed schedule cannot move"));
                }
                let end = checked_end(*starts_at_ms, before.duration_minutes)?;
                ensure_free(&tx, *starts_at_ms, end, Some(&id))?;
                tx.execute(
                    "UPDATE schedule_blocks SET starts_at_ms=?1 WHERE id=?2",
                    params![starts_at_ms, id],
                )?;
                (
                    Some(require_block(&tx, &id)?),
                    vec![mutation_with_base(
                        &tx,
                        &id,
                        "starts_at_ms",
                        Some(starts_at_ms.to_string()),
                    )?],
                )
            }
            ScheduleAction::Resize {
                duration_minutes, ..
            } => {
                let before = require_block(&tx, &id)?;
                if before.flexibility != "movable" {
                    return Err(AuthorityError::Invalid("fixed schedule cannot resize"));
                }
                let end = checked_end(before.starts_at_ms, *duration_minutes)?;
                ensure_free(&tx, before.starts_at_ms, end, Some(&id))?;
                tx.execute(
                    "UPDATE schedule_blocks SET duration_minutes=?1 WHERE id=?2",
                    params![duration_minutes, id],
                )?;
                (
                    Some(require_block(&tx, &id)?),
                    vec![mutation_with_base(
                        &tx,
                        &id,
                        "duration_minutes",
                        Some(duration_minutes.to_string()),
                    )?],
                )
            }
            ScheduleAction::Rename { title, .. } => {
                require_block(&tx, &id)?;
                check_title(title)?;
                tx.execute(
                    "UPDATE schedule_blocks SET title=?1 WHERE id=?2",
                    params![title, id],
                )?;
                (
                    Some(require_block(&tx, &id)?),
                    vec![mutation_with_base(&tx, &id, "title", Some(title.clone()))?],
                )
            }
            ScheduleAction::Retarget {
                target_kind,
                target_id,
                ..
            } => {
                require_block(&tx, &id)?;
                check_target(&tx, target_kind, target_id.as_deref())?;
                tx.execute(
                    "UPDATE schedule_blocks SET target_kind=?1,target_id=?2 WHERE id=?3",
                    params![target_kind, target_id, id],
                )?;
                (
                    Some(require_block(&tx, &id)?),
                    vec![
                        mutation_with_base(&tx, &id, "target_kind", Some(target_kind.clone()))?,
                        mutation_with_base(&tx, &id, "target_id", target_id.clone())?,
                    ],
                )
            }
            ScheduleAction::Delete { .. } => {
                require_block(&tx, &id)?;
                tx.execute("DELETE FROM schedule_blocks WHERE id=?1", [&id])?;
                (
                    None,
                    vec![mutation_with_base(
                        &tx,
                        &id,
                        "deleted",
                        Some("true".into()),
                    )?],
                )
            }
        };
        let semantic = SemanticChange::new(
            EntityRef::new(EntityKind::ScheduleBlock, id)
                .map_err(|_| AuthorityError::Invalid("invalid schedule entity"))?,
            mutations,
        )
        .map_err(|_| AuthorityError::Invalid("invalid schedule semantic mutation"))?;
        let result = ScheduleResult {
            revision,
            duplicate: false,
            block,
        };
        let receipt = serde_json::to_string(&result)
            .map_err(|_| AuthorityError::Invalid("schedule receipt serialization"))?;
        pending.commit(&tx, revision, semantic, &receipt)?;
        tx.commit()?;
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain_commands::{TaskAction, TaskCommand};
    use std::path::Path;

    fn store() -> AuthorityStore {
        AuthorityStore::open(Path::new(":memory:")).unwrap()
    }

    fn create(id: &str, revision: i64, start: i64, duration: u32) -> ScheduleCommand {
        ScheduleCommand {
            command_id: format!("create-{id}"),
            expected_revision: revision,
            action: ScheduleAction::Create {
                block_id: id.into(),
                title: format!("Schedule {id}"),
                starts_at_ms: start,
                duration_minutes: duration,
                flexibility: "movable".into(),
                target_kind: "standalone".into(),
                target_id: None,
            },
        }
    }

    fn action(id: &str, revision: i64, name: &str, action: ScheduleAction) -> ScheduleCommand {
        ScheduleCommand {
            command_id: format!("{name}-{id}"),
            expected_revision: revision,
            action,
        }
    }

    fn saved(store: &AuthorityStore) -> Vec<ScheduleRecord> {
        store.authority_schedule().unwrap().blocks
    }

    #[test]
    fn creation_accepts_and_lists_a_block() {
        let mut db = store();
        let receipt = db
            .authority_schedule_command("alice", create("morning", 0, 0, 30))
            .unwrap();
        assert_eq!(receipt.revision, 1);
        assert!(!receipt.duplicate);
        assert_eq!(saved(&db).len(), 1);
        assert_eq!(saved(&db)[0].ends_at_ms, 1_800_000);
        assert_eq!(db.authority_schedule().unwrap().revision, 1);
    }

    #[test]
    fn adjacent_intervals_are_valid() {
        let mut db = store();
        db.authority_schedule_command("alice", create("first", 0, 0, 30))
            .unwrap();
        db.authority_schedule_command("alice", create("second", 1, 1_800_000, 30))
            .unwrap();
        assert_eq!(saved(&db).len(), 2);
    }

    #[test]
    fn partially_overlapping_intervals_are_rejected() {
        let mut db = store();
        db.authority_schedule_command("alice", create("first", 0, 0, 30))
            .unwrap();
        assert!(
            db.authority_schedule_command("alice", create("overlap", 1, 900_000, 30))
                .is_err()
        );
        assert_eq!(saved(&db).len(), 1);
        assert_eq!(db.authority_command_count().unwrap(), 1);
    }

    #[test]
    fn containing_intervals_are_rejected() {
        let mut db = store();
        db.authority_schedule_command("alice", create("first", 0, 900_000, 15))
            .unwrap();
        assert!(
            db.authority_schedule_command("alice", create("large", 1, 0, 60))
                .is_err()
        );
        assert_eq!(saved(&db).len(), 1);
    }

    #[test]
    fn retry_with_same_id_returns_immutable_receipt() {
        let mut db = store();
        let command = create("first", 0, 0, 30);
        let first = db
            .authority_schedule_command("alice", command.clone())
            .unwrap();
        let replay = db.authority_schedule_command("alice", command).unwrap();
        assert_eq!(first.revision, replay.revision);
        assert!(replay.duplicate);
        assert_eq!(db.authority_command_count().unwrap(), 1);
    }

    #[test]
    fn immutable_command_id_cannot_be_reused_for_another_action() {
        let mut db = store();
        let command = create("first", 0, 0, 30);
        db.authority_schedule_command("alice", command.clone())
            .unwrap();
        let mut changed = command;
        if let ScheduleAction::Create { ref mut title, .. } = changed.action {
            *title = "Other".into();
        }
        assert!(db.authority_schedule_command("alice", changed).is_err());
        assert_eq!(saved(&db)[0].title, "Schedule first");
    }

    #[test]
    fn changing_actor_cannot_replay_another_users_receipt() {
        let mut db = store();
        let command = create("first", 0, 0, 30);
        db.authority_schedule_command("alice", command.clone())
            .unwrap();
        assert!(db.authority_schedule_command("bob", command).is_err());
    }

    #[test]
    fn invalid_revision_cannot_mutate_schedule() {
        let mut db = store();
        db.authority_schedule_command("alice", create("first", 0, 0, 30))
            .unwrap();
        assert!(
            db.authority_schedule_command("alice", create("second", 0, 4_000_000, 30))
                .is_err()
        );
        assert_eq!(saved(&db).len(), 1);
    }

    #[test]
    fn reschedule_moves_block_and_rechecks_collision() {
        let mut db = store();
        db.authority_schedule_command("alice", create("first", 0, 0, 30))
            .unwrap();
        db.authority_schedule_command("alice", create("second", 1, 3_600_000, 30))
            .unwrap();
        let bad = action(
            "first",
            2,
            "move-bad",
            ScheduleAction::Reschedule {
                block_id: "first".into(),
                starts_at_ms: 3_600_000,
            },
        );
        assert!(db.authority_schedule_command("alice", bad).is_err());
        let good = action(
            "first",
            2,
            "move-good",
            ScheduleAction::Reschedule {
                block_id: "first".into(),
                starts_at_ms: 7_200_000,
            },
        );
        db.authority_schedule_command("alice", good).unwrap();
        assert_eq!(saved(&db).last().unwrap().starts_at_ms, 7_200_000);
    }

    #[test]
    fn fixed_block_cannot_move() {
        let mut db = store();
        let mut command = create("fixed", 0, 0, 30);
        if let ScheduleAction::Create {
            ref mut flexibility,
            ..
        } = command.action
        {
            *flexibility = "fixed".into();
        }
        db.authority_schedule_command("alice", command).unwrap();
        let attempt = action(
            "fixed",
            1,
            "move",
            ScheduleAction::Reschedule {
                block_id: "fixed".into(),
                starts_at_ms: 7_200_000,
            },
        );
        assert!(db.authority_schedule_command("alice", attempt).is_err());
        assert_eq!(saved(&db)[0].starts_at_ms, 0);
    }

    #[test]
    fn resize_checks_overlaps_without_mutating_on_failure() {
        let mut db = store();
        db.authority_schedule_command("alice", create("first", 0, 0, 30))
            .unwrap();
        db.authority_schedule_command("alice", create("second", 1, 3_600_000, 30))
            .unwrap();
        let bad = action(
            "first",
            2,
            "resize-bad",
            ScheduleAction::Resize {
                block_id: "first".into(),
                duration_minutes: 90,
            },
        );
        assert!(db.authority_schedule_command("alice", bad).is_err());
        let good = action(
            "first",
            2,
            "resize-good",
            ScheduleAction::Resize {
                block_id: "first".into(),
                duration_minutes: 60,
            },
        );
        db.authority_schedule_command("alice", good).unwrap();
        assert_eq!(saved(&db)[0].duration_minutes, 60);
    }

    #[test]
    fn zero_duration_and_overflow_are_invalid() {
        let mut db = store();
        assert!(
            db.authority_schedule_command("alice", create("zero", 0, 0, 0))
                .is_err()
        );
        assert!(
            db.authority_schedule_command("alice", create("overflow", 0, i64::MAX, 2))
                .is_err()
        );
        assert_eq!(db.authority_command_count().unwrap(), 0);
    }

    #[test]
    fn rename_preserves_other_fields() {
        let mut db = store();
        db.authority_schedule_command("alice", create("first", 0, 0, 30))
            .unwrap();
        let rename = action(
            "first",
            1,
            "rename",
            ScheduleAction::Rename {
                block_id: "first".into(),
                title: "Renamed".into(),
            },
        );
        db.authority_schedule_command("alice", rename).unwrap();
        let block = &saved(&db)[0];
        assert_eq!(block.title, "Renamed");
        assert_eq!(block.starts_at_ms, 0);
        assert_eq!(block.duration_minutes, 30);
    }

    #[test]
    fn delete_removes_block_but_retains_receipt_and_history() {
        let mut db = store();
        db.authority_schedule_command("alice", create("first", 0, 0, 30))
            .unwrap();
        let delete = action(
            "first",
            1,
            "delete",
            ScheduleAction::Delete {
                block_id: "first".into(),
            },
        );
        let accepted = db
            .authority_schedule_command("alice", delete.clone())
            .unwrap();
        assert!(accepted.block.is_none());
        assert!(saved(&db).is_empty());
        assert_eq!(db.authority_command_count().unwrap(), 2);
        assert!(
            db.authority_schedule_command("alice", delete)
                .unwrap()
                .duplicate
        );
    }

    #[test]
    fn deleting_missing_block_cannot_advance_revision() {
        let mut db = store();
        let delete = action(
            "missing",
            0,
            "delete",
            ScheduleAction::Delete {
                block_id: "missing".into(),
            },
        );
        assert!(db.authority_schedule_command("alice", delete).is_err());
        assert_eq!(db.authority_schedule().unwrap().revision, 0);
    }

    #[test]
    fn create_with_missing_target_is_rejected() {
        let mut db = store();
        let mut command = create("targeted", 0, 0, 30);
        if let ScheduleAction::Create {
            ref mut target_kind,
            ref mut target_id,
            ..
        } = command.action
        {
            *target_kind = "task".into();
            *target_id = Some("missing".into());
        }
        assert!(db.authority_schedule_command("alice", command).is_err());
        assert_eq!(db.authority_command_count().unwrap(), 0);
    }

    #[test]
    fn task_target_succeeds_only_after_task_commit() {
        let mut db = store();
        db.authority_task_command(
            "alice",
            TaskCommand {
                command_id: "create-task".into(),
                expected_revision: 0,
                action: TaskAction::Create {
                    task_id: "my-task".into(),
                    title: "My Task".into(),
                    project_id: None,
                },
            },
        )
        .unwrap();
        let mut command = create("targeted", 1, 0, 30);
        if let ScheduleAction::Create {
            ref mut target_kind,
            ref mut target_id,
            ..
        } = command.action
        {
            *target_kind = "task".into();
            *target_id = Some("my-task".into());
        }
        db.authority_schedule_command("alice", command).unwrap();
        assert_eq!(saved(&db)[0].target_id.as_deref(), Some("my-task"));
    }

    #[test]
    fn retarget_to_standalone_is_supported() {
        let mut db = store();
        db.authority_schedule_command("alice", create("first", 0, 0, 30))
            .unwrap();
        let retarget = action(
            "first",
            1,
            "retarget",
            ScheduleAction::Retarget {
                block_id: "first".into(),
                target_kind: "standalone".into(),
                target_id: None,
            },
        );
        db.authority_schedule_command("alice", retarget).unwrap();
        assert_eq!(saved(&db)[0].target_kind, "standalone");
    }

    #[test]
    fn read_conflicts_use_half_open_intervals() {
        let mut db = store();
        db.authority_schedule_command("alice", create("first", 0, 0, 30))
            .unwrap();
        let at_end = db
            .authority_schedule_conflicts(1_800_000, 30, None)
            .unwrap();
        assert!(at_end.is_empty());
        let partial = db
            .authority_schedule_conflicts(1_799_999, 30, None)
            .unwrap();
        assert_eq!(partial.len(), 1);
        let excluded = db
            .authority_schedule_conflicts(0, 30, Some("first"))
            .unwrap();
        assert!(excluded.is_empty());
    }

    #[test]
    fn stored_snapshots_order_by_time_and_id() {
        let mut db = store();
        db.authority_schedule_command("alice", create("late", 0, 7_200_000, 30))
            .unwrap();
        db.authority_schedule_command("alice", create("early", 1, 0, 30))
            .unwrap();
        let ids: Vec<_> = saved(&db).into_iter().map(|record| record.id).collect();
        assert_eq!(ids, vec!["early", "late"]);
    }

    #[test]
    fn schedule_and_task_commands_share_one_revision_sequence() {
        let mut db = store();
        db.authority_schedule_command("alice", create("block", 0, 0, 30))
            .unwrap();
        let task = TaskCommand {
            command_id: "create-task".into(),
            expected_revision: 1,
            action: TaskAction::Create {
                task_id: "task".into(),
                title: "Task".into(),
                project_id: None,
            },
        };
        assert_eq!(
            db.authority_task_command("alice", task).unwrap().revision,
            2
        );
        assert_eq!(db.authority_schedule().unwrap().revision, 2);
    }
}

#[cfg(test)]
mod extended_acceptance {
    use super::*;
    use std::path::Path;

    fn create_command(id: &str, revision: i64, start: i64) -> ScheduleCommand {
        ScheduleCommand {
            command_id: format!("create-{id}"),
            expected_revision: revision,
            action: ScheduleAction::Create {
                block_id: id.to_string(),
                title: id.to_string(),
                starts_at_ms: start,
                duration_minutes: 30,
                flexibility: "movable".to_string(),
                target_kind: "standalone".to_string(),
                target_id: None,
            },
        }
    }

    fn apply(
        db: &mut AuthorityStore,
        revision: i64,
        id: &str,
        suffix: &str,
        action: ScheduleAction,
    ) -> Result<ScheduleResult, AuthorityError> {
        db.authority_schedule_command(
            "owner",
            ScheduleCommand {
                command_id: format!("{id}-{suffix}"),
                expected_revision: revision,
                action,
            },
        )
    }

    #[test]
    fn failed_mutation_does_not_consume_command_identity() {
        let mut db = AuthorityStore::open(Path::new(":memory:")).unwrap();
        db.authority_schedule_command("owner", create_command("one", 0, 0))
            .unwrap();
        let failed = ScheduleCommand {
            command_id: "create-two".into(),
            expected_revision: 1,
            action: ScheduleAction::Create {
                block_id: "two".into(),
                title: "two".into(),
                starts_at_ms: 1,
                duration_minutes: 30,
                flexibility: "movable".into(),
                target_kind: "standalone".into(),
                target_id: None,
            },
        };
        assert!(db.authority_schedule_command("owner", failed).is_err());
        let accepted = db
            .authority_schedule_command("owner", create_command("two", 1, 1_800_000))
            .unwrap();
        assert_eq!(accepted.revision, 2);
        assert_eq!(db.authority_command_count().unwrap(), 2);
    }

    #[test]
    fn rename_then_reschedule_then_delete_is_one_history_chain() {
        let mut db = AuthorityStore::open(Path::new(":memory:")).unwrap();
        db.authority_schedule_command("owner", create_command("meeting", 0, 0))
            .unwrap();
        apply(
            &mut db,
            1,
            "meeting",
            "rename",
            ScheduleAction::Rename {
                block_id: "meeting".into(),
                title: "Updated meeting".into(),
            },
        )
        .unwrap();
        apply(
            &mut db,
            2,
            "meeting",
            "move",
            ScheduleAction::Reschedule {
                block_id: "meeting".into(),
                starts_at_ms: 3_600_000,
            },
        )
        .unwrap();
        let after = db.authority_schedule().unwrap();
        assert_eq!(after.revision, 3);
        assert_eq!(after.blocks[0].title, "Updated meeting");
        assert_eq!(after.blocks[0].starts_at_ms, 3_600_000);
        apply(
            &mut db,
            3,
            "meeting",
            "delete",
            ScheduleAction::Delete {
                block_id: "meeting".into(),
            },
        )
        .unwrap();
        assert_eq!(db.authority_schedule().unwrap().revision, 4);
        assert!(db.authority_schedule().unwrap().blocks.is_empty());
        assert_eq!(db.authority_command_count().unwrap(), 4);
    }

    #[test]
    fn invalid_target_kind_and_target_pair_are_rejected() {
        let mut db = AuthorityStore::open(Path::new(":memory:")).unwrap();
        for (kind, id) in [
            ("standalone", Some("task")),
            ("task", None),
            ("habit", None),
            ("unknown", Some("task")),
        ] {
            let mut command = create_command("invalid", 0, 0);
            if let ScheduleAction::Create {
                target_kind,
                target_id,
                ..
            } = &mut command.action
            {
                *target_kind = kind.to_string();
                *target_id = id.map(str::to_string);
            }
            assert!(db.authority_schedule_command("owner", command).is_err());
            assert_eq!(db.authority_schedule().unwrap().revision, 0);
        }
    }

    #[test]
    fn no_schedule_operation_is_allowed_to_reuse_an_id_from_a_different_actor() {
        let mut db = AuthorityStore::open(Path::new(":memory:")).unwrap();
        let command = create_command("meeting", 0, 0);
        db.authority_schedule_command("owner", command.clone())
            .unwrap();
        assert!(db.authority_schedule_command("stranger", command).is_err());
        assert_eq!(db.authority_schedule().unwrap().revision, 1);
    }

    #[test]
    fn deleting_one_block_releases_its_previous_interval() {
        let mut db = AuthorityStore::open(Path::new(":memory:")).unwrap();
        db.authority_schedule_command("owner", create_command("one", 0, 0))
            .unwrap();
        apply(
            &mut db,
            1,
            "one",
            "delete",
            ScheduleAction::Delete {
                block_id: "one".into(),
            },
        )
        .unwrap();
        db.authority_schedule_command("owner", create_command("replacement", 2, 0))
            .unwrap();
        assert_eq!(db.authority_schedule().unwrap().blocks.len(), 1);
        assert_eq!(db.authority_schedule().unwrap().blocks[0].id, "replacement");
    }

    #[test]
    fn cannot_mutate_deleted_block_without_recreating_it() {
        let mut db = AuthorityStore::open(Path::new(":memory:")).unwrap();
        db.authority_schedule_command("owner", create_command("one", 0, 0))
            .unwrap();
        apply(
            &mut db,
            1,
            "one",
            "delete",
            ScheduleAction::Delete {
                block_id: "one".into(),
            },
        )
        .unwrap();
        assert!(
            apply(
                &mut db,
                2,
                "one",
                "rename",
                ScheduleAction::Rename {
                    block_id: "one".into(),
                    title: "Impossible".into(),
                }
            )
            .is_err()
        );
        assert_eq!(db.authority_schedule().unwrap().revision, 2);
    }

    #[test]
    fn enormous_positive_duration_cannot_wrap() {
        let mut db = AuthorityStore::open(Path::new(":memory:")).unwrap();
        let mut command = create_command("long", 0, i64::MAX - 10);
        if let ScheduleAction::Create {
            duration_minutes, ..
        } = &mut command.action
        {
            *duration_minutes = u32::MAX;
        }
        assert!(db.authority_schedule_command("owner", command).is_err());
        assert_eq!(db.authority_command_count().unwrap(), 0);
    }

    #[test]
    fn invalid_identity_is_rejected_before_database_mutation() {
        let mut db = AuthorityStore::open(Path::new(":memory:")).unwrap();
        let mut command = create_command("bad identifier", 0, 0);
        command.command_id = "identity".into();
        assert!(db.authority_schedule_command("owner", command).is_err());
        assert!(db.authority_schedule().unwrap().blocks.is_empty());
    }

    #[test]
    fn negative_starts_are_allowed_if_interval_is_well_formed() {
        let mut db = AuthorityStore::open(Path::new(":memory:")).unwrap();
        let accepted = db
            .authority_schedule_command("owner", create_command("ancient", 0, -60_000))
            .unwrap();
        assert_eq!(accepted.block.unwrap().starts_at_ms, -60_000);
    }

    #[test]
    fn schedule_conflict_query_rejects_invalid_duration() {
        let db = AuthorityStore::open(Path::new(":memory:")).unwrap();
        assert!(db.authority_schedule_conflicts(0, 0, None).is_err());
        assert!(db.authority_schedule_conflicts(i64::MAX, 1, None).is_err());
    }
}
