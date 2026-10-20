//! Project milestone commands executed inside the canonical authority ledger.
//! A milestone is always attached to an existing project; status transitions
//! and moves are revision-fenced, replayable and acknowledged only on commit.
use crate::command_engine::{CommandAdmission, PendingCommand};
use crate::store::{AuthorityError, AuthorityStore};
use orbit_sync::{ChangeEnvelope, EntityKind, EntityRef, FieldMutation, SemanticChange};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct MilestoneCommand {
    pub command_id: String,
    pub expected_revision: i64,
    pub action: MilestoneAction,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum MilestoneAction {
    Create {
        milestone_id: String,
        project_id: String,
        name: String,
    },
    Rename {
        milestone_id: String,
        name: String,
    },
    ChangeStatus {
        milestone_id: String,
        status: String,
    },
    MoveProject {
        milestone_id: String,
        project_id: String,
    },
    Delete {
        milestone_id: String,
    },
}
impl MilestoneAction {
    pub fn milestone_id(&self) -> &str {
        match self {
            Self::Create { milestone_id, .. }
            | Self::Rename { milestone_id, .. }
            | Self::ChangeStatus { milestone_id, .. }
            | Self::MoveProject { milestone_id, .. }
            | Self::Delete { milestone_id } => milestone_id,
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct MilestoneRecord {
    pub id: String,
    pub project_id: String,
    pub name: String,
    pub status: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MilestoneResult {
    pub revision: i64,
    pub duplicate: bool,
    pub milestone: Option<MilestoneRecord>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MilestoneSnapshot {
    pub revision: i64,
    pub milestones: Vec<MilestoneRecord>,
}
fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
}
fn valid_name(name: &str) -> bool {
    !name.trim().is_empty() && name.len() <= 4096
}
fn status_allowed(status: &str) -> bool {
    matches!(
        status,
        "planned" | "in_progress" | "blocked" | "completed" | "cancelled"
    )
}
fn ensure_project(conn: &Connection, id: &str) -> Result<(), AuthorityError> {
    if !valid_id(id) {
        return Err(AuthorityError::Invalid("invalid project ID"));
    }
    let exists: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM projects WHERE id=?1)",
        [id],
        |r| r.get(0),
    )?;
    if !exists {
        return Err(AuthorityError::Invalid("milestone project does not exist"));
    }
    Ok(())
}
fn record(conn: &Connection, id: &str) -> Result<Option<MilestoneRecord>, AuthorityError> {
    Ok(conn
        .query_row(
            "SELECT id,project_id,name,status FROM milestones WHERE id=?1",
            [id],
            |row| {
                Ok(MilestoneRecord {
                    id: row.get(0)?,
                    project_id: row.get(1)?,
                    name: row.get(2)?,
                    status: row.get(3)?,
                })
            },
        )
        .optional()?)
}
fn required(conn: &Connection, id: &str) -> Result<MilestoneRecord, AuthorityError> {
    record(conn, id)?.ok_or(AuthorityError::Invalid("milestone does not exist"))
}
fn field_version(
    conn: &Connection,
    id: &str,
    field: &str,
) -> Result<Option<orbit_sync::ChangeId>, AuthorityError> {
    let mut stmt = conn
        .prepare("SELECT wire FROM authority_history WHERE kind='change' ORDER BY cursor DESC")?;
    let wires = stmt.query_map([], |r| r.get::<_, String>(0))?;
    for wire in wires {
        let envelope = ChangeEnvelope::from_wire_json(&wire?)
            .map_err(|_| AuthorityError::MigrationConflict)?;
        if envelope.change.entity.kind == EntityKind::Milestone
            && envelope.change.entity.id == id
            && envelope.change.mutations.iter().any(|m| m.field == field)
        {
            return Ok(Some(envelope.id));
        }
    }
    Ok(None)
}
fn mutation(
    conn: &Connection,
    id: &str,
    key: &str,
    value: Option<String>,
) -> Result<FieldMutation, AuthorityError> {
    FieldMutation::new(key, value, field_version(conn, id, key)?)
        .map_err(|_| AuthorityError::Invalid("invalid milestone semantic mutation"))
}
fn validate_record(conn: &Connection, m: &MilestoneRecord) -> Result<(), AuthorityError> {
    if !valid_id(&m.id) || !valid_name(&m.name) || !status_allowed(&m.status) {
        return Err(AuthorityError::Invalid("invalid stored milestone"));
    }
    ensure_project(conn, &m.project_id)
}
impl AuthorityStore {
    pub fn authority_milestones(&self) -> Result<MilestoneSnapshot, AuthorityError> {
        let tx = self.conn.unchecked_transaction()?;
        let revision: i64 = tx.query_row(
            "SELECT COALESCE(MAX(cursor),0) FROM authority_history",
            [],
            |r| r.get(0),
        )?;
        let milestones = {
            let mut stmt = tx.prepare(
                "SELECT id,project_id,name,status FROM milestones ORDER BY project_id,id",
            )?;
            stmt.query_map([], |r| {
                Ok(MilestoneRecord {
                    id: r.get(0)?,
                    project_id: r.get(1)?,
                    name: r.get(2)?,
                    status: r.get(3)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?
        };
        for item in &milestones {
            validate_record(&tx, item)?;
        }
        tx.commit()?;
        Ok(MilestoneSnapshot {
            revision,
            milestones,
        })
    }
    pub fn authority_milestone_command(
        &mut self,
        actor: &str,
        command: MilestoneCommand,
    ) -> Result<MilestoneResult, AuthorityError> {
        if actor.trim().is_empty()
            || !valid_id(&command.command_id)
            || command.expected_revision < 0
            || !valid_id(command.action.milestone_id())
        {
            return Err(AuthorityError::Invalid(
                "invalid milestone command envelope",
            ));
        }
        let payload = serde_json::to_string(&command)
            .map_err(|_| AuthorityError::Invalid("milestone serialization"))?;
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
                let mut receipt: MilestoneResult = serde_json::from_str(&raw)
                    .map_err(|_| AuthorityError::Invalid("invalid stored milestone receipt"))?;
                receipt.duplicate = true;
                return Ok(receipt);
            }
            CommandAdmission::New(n) => n,
        };
        let id = command.action.milestone_id().to_owned();
        let (milestone, changes) = match command.action {
            MilestoneAction::Create {
                project_id, name, ..
            } => {
                ensure_project(&tx, &project_id)?;
                if !valid_name(&name) {
                    return Err(AuthorityError::Invalid("invalid milestone name"));
                }
                if tx.execute("INSERT OR IGNORE INTO milestones(id,project_id,name,status) VALUES(?1,?2,?3,'planned')",
                    params![id,project_id,name])?!=1 {return Err(AuthorityError::ImmutableConflict);}
                let changes = vec![
                    mutation(&tx, &id, "project_id", Some(project_id))?,
                    mutation(&tx, &id, "name", Some(name))?,
                    mutation(&tx, &id, "status", Some("planned".into()))?,
                ];
                (Some(required(&tx, &id)?), changes)
            }
            MilestoneAction::Rename { name, .. } => {
                required(&tx, &id)?;
                if !valid_name(&name) {
                    return Err(AuthorityError::Invalid("invalid milestone name"));
                }
                let change = mutation(&tx, &id, "name", Some(name.clone()))?;
                tx.execute(
                    "UPDATE milestones SET name=?1 WHERE id=?2",
                    params![name, id],
                )?;
                (Some(required(&tx, &id)?), vec![change])
            }
            MilestoneAction::ChangeStatus { status, .. } => {
                let before = required(&tx, &id)?;
                if !status_allowed(&status) {
                    return Err(AuthorityError::Invalid("invalid milestone status"));
                }
                let allowed = before.status == status
                    || matches!(
                        (before.status.as_str(), status.as_str()),
                        ("planned", "in_progress" | "blocked" | "cancelled")
                            | ("in_progress", "blocked" | "completed" | "cancelled")
                            | ("blocked", "in_progress" | "cancelled")
                    );
                if !allowed {
                    return Err(AuthorityError::Invalid(
                        "invalid milestone status transition",
                    ));
                }
                let change = mutation(&tx, &id, "status", Some(status.clone()))?;
                tx.execute(
                    "UPDATE milestones SET status=?1 WHERE id=?2",
                    params![status, id],
                )?;
                (Some(required(&tx, &id)?), vec![change])
            }
            MilestoneAction::MoveProject { project_id, .. } => {
                required(&tx, &id)?;
                ensure_project(&tx, &project_id)?;
                let attached: i64 = tx.query_row(
                    "SELECT COUNT(*) FROM tasks WHERE milestone_id=?1",
                    [&id],
                    |r| r.get(0),
                )?;
                if attached > 0 {
                    return Err(AuthorityError::Invalid(
                        "cannot move milestone with attached tasks",
                    ));
                }
                let change = mutation(&tx, &id, "project_id", Some(project_id.clone()))?;
                tx.execute(
                    "UPDATE milestones SET project_id=?1 WHERE id=?2",
                    params![project_id, id],
                )?;
                (Some(required(&tx, &id)?), vec![change])
            }
            MilestoneAction::Delete { .. } => {
                required(&tx, &id)?;
                let attached: i64 = tx.query_row(
                    "SELECT COUNT(*) FROM tasks WHERE milestone_id=?1",
                    [&id],
                    |r| r.get(0),
                )?;
                if attached > 0 {
                    return Err(AuthorityError::Invalid(
                        "cannot delete milestone with attached tasks",
                    ));
                }
                let change = mutation(&tx, &id, "deleted", Some("true".into()))?;
                tx.execute("DELETE FROM milestones WHERE id=?1", [&id])?;
                (None, vec![change])
            }
        };
        let semantic = SemanticChange::new(
            EntityRef::new(EntityKind::Milestone, id)
                .map_err(|_| AuthorityError::Invalid("invalid milestone identity"))?,
            changes,
        )
        .map_err(|_| AuthorityError::Invalid("invalid milestone change"))?;
        let result = MilestoneResult {
            revision,
            duplicate: false,
            milestone,
        };
        let receipt = serde_json::to_string(&result)
            .map_err(|_| AuthorityError::Invalid("milestone receipt"))?;
        pending.commit(&tx, revision, semantic, &receipt)?;
        tx.commit()?;
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain_commands::ProjectCommand;
    use std::path::Path;
    fn store() -> AuthorityStore {
        AuthorityStore::open(Path::new(":memory:")).unwrap()
    }
    fn command(id: &str, rev: i64, action: MilestoneAction) -> MilestoneCommand {
        MilestoneCommand {
            command_id: id.into(),
            expected_revision: rev,
            action,
        }
    }
    fn project(db: &mut AuthorityStore, id: &str, rev: i64) {
        db.authority_project_command(
            "user",
            ProjectCommand {
                command_id: format!("project-{id}"),
                expected_revision: rev,
                project_id: id.into(),
                name: format!("project {id}"),
            },
        )
        .unwrap();
    }
    #[test]
    fn create_rename_move_finish_and_retry() {
        let mut db = store();
        project(&mut db, "one", 0);
        project(&mut db, "two", 1);
        let create = command(
            "create",
            2,
            MilestoneAction::Create {
                milestone_id: "m".into(),
                project_id: "one".into(),
                name: "Prototype".into(),
            },
        );
        let accepted = db
            .authority_milestone_command("user", create.clone())
            .unwrap();
        assert_eq!(accepted.revision, 3);
        assert_eq!(accepted.milestone.unwrap().status, "planned");
        assert!(
            db.authority_milestone_command("user", create)
                .unwrap()
                .duplicate
        );
        db.authority_milestone_command(
            "user",
            command(
                "rename",
                3,
                MilestoneAction::Rename {
                    milestone_id: "m".into(),
                    name: "Alpha".into(),
                },
            ),
        )
        .unwrap();
        db.authority_milestone_command(
            "user",
            command(
                "move",
                4,
                MilestoneAction::MoveProject {
                    milestone_id: "m".into(),
                    project_id: "two".into(),
                },
            ),
        )
        .unwrap();
        db.authority_milestone_command(
            "user",
            command(
                "progress",
                5,
                MilestoneAction::ChangeStatus {
                    milestone_id: "m".into(),
                    status: "in_progress".into(),
                },
            ),
        )
        .unwrap();
        db.authority_milestone_command(
            "user",
            command(
                "finish",
                6,
                MilestoneAction::ChangeStatus {
                    milestone_id: "m".into(),
                    status: "completed".into(),
                },
            ),
        )
        .unwrap();
        let snapshot = db.authority_milestones().unwrap();
        assert_eq!(snapshot.revision, 7);
        assert_eq!(snapshot.milestones[0].name, "Alpha");
        assert_eq!(snapshot.milestones[0].project_id, "two");
        assert_eq!(snapshot.milestones[0].status, "completed");
        assert!(
            db.authority_milestone_command(
                "user",
                command(
                    "reopen",
                    7,
                    MilestoneAction::ChangeStatus {
                        milestone_id: "m".into(),
                        status: "in_progress".into(),
                    }
                )
            )
            .is_err()
        );
        assert_eq!(db.authority_milestones().unwrap().revision, 7);
    }
    #[test]
    fn failed_missing_project_and_stale_revision_are_atomic() {
        let mut db = store();
        let no_project = command(
            "missing",
            0,
            MilestoneAction::Create {
                milestone_id: "m".into(),
                project_id: "none".into(),
                name: "Bad".into(),
            },
        );
        assert!(db.authority_milestone_command("user", no_project).is_err());
        assert_eq!(db.authority_milestones().unwrap().revision, 0);
        project(&mut db, "p", 0);
        let stale = command(
            "stale",
            0,
            MilestoneAction::Create {
                milestone_id: "m".into(),
                project_id: "p".into(),
                name: "Bad".into(),
            },
        );
        assert!(db.authority_milestone_command("user", stale).is_err());
        assert!(db.authority_milestones().unwrap().milestones.is_empty());
    }
    #[test]
    fn delete_clears_live_projection_but_preserves_command_receipt() {
        let mut db = store();
        project(&mut db, "p", 0);
        db.authority_milestone_command(
            "user",
            command(
                "create",
                1,
                MilestoneAction::Create {
                    milestone_id: "m".into(),
                    project_id: "p".into(),
                    name: "Ready".into(),
                },
            ),
        )
        .unwrap();
        let deletion = command(
            "delete",
            2,
            MilestoneAction::Delete {
                milestone_id: "m".into(),
            },
        );
        assert!(
            db.authority_milestone_command("user", deletion.clone())
                .unwrap()
                .milestone
                .is_none()
        );
        assert!(
            db.authority_milestone_command("user", deletion)
                .unwrap()
                .duplicate
        );
        assert!(db.authority_milestones().unwrap().milestones.is_empty());
        assert_eq!(db.event_count().unwrap(), 3);
    }
    #[test]
    fn milestone_serialization_rejects_unknown_fields() {
        let valid = r#"{"command_id":"cmd","expected_revision":0,"action":{"type":"delete","milestone_id":"m"}}"#;
        let decoded: MilestoneCommand = serde_json::from_str(valid).unwrap();
        assert_eq!(decoded.action.milestone_id(), "m");
        let bad = r#"{"command_id":"cmd","expected_revision":0,"unexpected":true,"action":{"type":"delete","milestone_id":"m"}}"#;
        assert!(serde_json::from_str::<MilestoneCommand>(bad).is_err());
    }

    #[test]
    fn milestone_state_machine_matches_domain_transitions() {
        let mut db = store();
        project(&mut db, "p", 0);
        db.authority_milestone_command(
            "user",
            command(
                "create",
                1,
                MilestoneAction::Create {
                    milestone_id: "m".into(),
                    project_id: "p".into(),
                    name: "Outcome".into(),
                },
            ),
        )
        .unwrap();
        // Planned milestones cannot skip directly to completion.
        assert!(
            db.authority_milestone_command(
                "user",
                command(
                    "skip",
                    2,
                    MilestoneAction::ChangeStatus {
                        milestone_id: "m".into(),
                        status: "completed".into()
                    }
                )
            )
            .is_err()
        );
        assert_eq!(db.authority_milestones().unwrap().revision, 2);
        let transitions = [
            ("block", "blocked"),
            ("resume", "in_progress"),
            ("reblock", "blocked"),
            ("continue", "in_progress"),
            ("complete", "completed"),
        ];
        for (index, (id, status)) in transitions.into_iter().enumerate() {
            db.authority_milestone_command(
                "user",
                command(
                    id,
                    2 + index as i64,
                    MilestoneAction::ChangeStatus {
                        milestone_id: "m".into(),
                        status: status.into(),
                    },
                ),
            )
            .unwrap();
            assert_eq!(
                db.authority_milestones().unwrap().milestones[0].status,
                status
            );
        }
        assert!(
            db.authority_milestone_command(
                "user",
                command(
                    "reset",
                    7,
                    MilestoneAction::ChangeStatus {
                        milestone_id: "m".into(),
                        status: "planned".into()
                    }
                )
            )
            .is_err()
        );
        assert_eq!(db.authority_milestones().unwrap().revision, 7);
    }

    #[test]
    fn milestone_movement_and_delete_respect_linked_task_references() {
        let mut db = store();
        project(&mut db, "p", 0);
        project(&mut db, "other", 1);
        db.authority_milestone_command(
            "user",
            command(
                "create",
                2,
                MilestoneAction::Create {
                    milestone_id: "m".into(),
                    project_id: "p".into(),
                    name: "Milestone".into(),
                },
            ),
        )
        .unwrap();
        // Simulate the existing rich task layer associating its task with a
        // milestone: the authority must not orphan or cross-link it.
        db.conn.execute("INSERT INTO tasks(id,title,status,project_id,milestone_id) VALUES('t','Task','ready','p','m')",[]).unwrap();
        assert!(
            db.authority_milestone_command(
                "user",
                command(
                    "move",
                    3,
                    MilestoneAction::MoveProject {
                        milestone_id: "m".into(),
                        project_id: "other".into()
                    }
                )
            )
            .is_err()
        );
        assert!(
            db.authority_milestone_command(
                "user",
                command(
                    "delete",
                    3,
                    MilestoneAction::Delete {
                        milestone_id: "m".into()
                    }
                )
            )
            .is_err()
        );
        assert_eq!(db.authority_milestones().unwrap().revision, 3);
        assert_eq!(
            db.authority_milestones().unwrap().milestones[0].project_id,
            "p"
        );
        db.conn
            .execute("UPDATE tasks SET milestone_id=NULL WHERE id='t'", [])
            .unwrap();
        db.authority_milestone_command(
            "user",
            command(
                "move",
                3,
                MilestoneAction::MoveProject {
                    milestone_id: "m".into(),
                    project_id: "other".into(),
                },
            ),
        )
        .unwrap();
        assert_eq!(
            db.authority_milestones().unwrap().milestones[0].project_id,
            "other"
        );
    }

    #[test]
    fn invalid_status_and_name_never_commit_an_authority_receipt() {
        let mut db = store();
        project(&mut db, "p", 0);
        db.authority_milestone_command(
            "user",
            command(
                "create",
                1,
                MilestoneAction::Create {
                    milestone_id: "m".into(),
                    project_id: "p".into(),
                    name: "Milestone".into(),
                },
            ),
        )
        .unwrap();
        assert!(
            db.authority_milestone_command(
                "user",
                command(
                    "bad-name",
                    2,
                    MilestoneAction::Rename {
                        milestone_id: "m".into(),
                        name: "  ".into()
                    }
                )
            )
            .is_err()
        );
        assert!(
            db.authority_milestone_command(
                "user",
                command(
                    "bad-status",
                    2,
                    MilestoneAction::ChangeStatus {
                        milestone_id: "m".into(),
                        status: "unknown".into()
                    }
                )
            )
            .is_err()
        );
        assert!(
            db.authority_milestone_command(
                "other",
                command(
                    "create",
                    1,
                    MilestoneAction::Create {
                        milestone_id: "m".into(),
                        project_id: "p".into(),
                        name: "Milestone".into(),
                    }
                )
            )
            .is_err()
        );
        assert_eq!(db.authority_milestones().unwrap().revision, 2);
        assert_eq!(db.event_count().unwrap(), 2);
    }
}
