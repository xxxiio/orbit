//! A single atomic command acceptance path for all authoritative command families.
//! Domain handlers validate and mutate their own records inside the same SQLite
//! transaction; this module owns retries, revision fencing, the semantic ledger,
//! immutable receipts, and the accepted replay projection.
use crate::store::{AuthorityError, AuthorityEvent, AuthorityStore};
use orbit_sync::{ChangeEnvelope, ChangeId, DeviceId, SemanticChange};
use rusqlite::{Connection, OptionalExtension, params};

pub(crate) struct PendingCommand<'a> {
    pub command_id: &'a str,
    pub actor: &'a str,
    pub payload: &'a str,
    pub expected_revision: i64,
}

impl PendingCommand<'_> {
    pub fn check(&self, tx: &Connection) -> Result<CommandAdmission, AuthorityError> {
        if self.actor.trim().is_empty()
            || self.command_id.trim().is_empty()
            || self.command_id.len() > 128
            || self.expected_revision < 0
        {
            return Err(AuthorityError::Invalid("invalid command envelope"));
        }
        let previous: Option<(String, String, String)> = tx
            .query_row(
                "SELECT actor,payload,result_json FROM authority_commands WHERE command_id=?1",
                [self.command_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()?;
        if let Some((actor, payload, result)) = previous {
            if actor != self.actor || payload != self.payload {
                return Err(AuthorityError::ImmutableConflict);
            }
            return Ok(CommandAdmission::Duplicate(result));
        }
        if AuthorityStore::has_domain_commands(tx)? {
            return Err(AuthorityError::MigrationConflict);
        }
        let tip: i64 = tx.query_row(
            "SELECT COALESCE(MAX(cursor),0) FROM authority_history",
            [],
            |r| r.get(0),
        )?;
        let accepted: i64 =
            tx.query_row("SELECT COUNT(*) FROM authority_commands", [], |r| r.get(0))?;
        // Any missing or extraneous receipt indicates a torn or mixed command history.
        // Reject writes instead of silently continuing from an inconsistent tip.
        if accepted != tip {
            return Err(AuthorityError::MigrationConflict);
        }
        // A committed command must have exactly one ledger record and an
        // accepted replay at that same revision. Never extend an inconsistent
        // history or silently rebuild a missing projection during admission.
        let history_rows: i64 =
            tx.query_row("SELECT COUNT(*) FROM authority_history", [], |r| r.get(0))?;
        if history_rows != tip {
            return Err(AuthorityError::MigrationConflict);
        }
        if tip > 0 {
            let projection_tip: Option<i64> = tx
                .query_row(
                    "SELECT authority_revision FROM authority_projection WHERE id=1",
                    [],
                    |r| r.get(0),
                )
                .optional()?;
            if projection_tip != Some(tip) {
                return Err(AuthorityError::MigrationConflict);
            }
        }
        if tip != self.expected_revision {
            return Err(AuthorityError::StaleRevision);
        }
        Ok(CommandAdmission::New(
            tip.checked_add(1)
                .ok_or(AuthorityError::Invalid("revision overflow"))?,
        ))
    }

    /// Must run in the *same* transaction as the validated domain update.
    /// The receipt is persisted only after projection replay succeeds; a failure
    /// rolls back every domain row, semantic event and receipt together.
    pub fn commit(
        &self,
        tx: &Connection,
        sequence: i64,
        semantic_change: SemanticChange,
        result_json: &str,
    ) -> Result<(), AuthorityError> {
        let change_id = ChangeId::new(
            DeviceId::new("orbit-server").map_err(|_| AuthorityError::Invalid("invalid device"))?,
            u64::try_from(sequence).map_err(|_| AuthorityError::Invalid("invalid sequence"))?,
        )
        .map_err(|_| AuthorityError::Invalid("invalid sequence"))?;
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| AuthorityError::Invalid("invalid clock"))?
            .as_millis()
            .try_into()
            .map_err(|_| AuthorityError::Invalid("invalid clock"))?;
        let wire = ChangeEnvelope::new(change_id, timestamp, semantic_change)
            .to_wire_json()
            .map_err(|_| AuthorityError::Invalid("invalid semantic wire"))?;
        tx.execute(
            "INSERT OR IGNORE INTO devices(id,token_hash,revoked) VALUES('orbit-server',zeroblob(32),1)", [],
        )?;
        tx.execute(
            "INSERT INTO authority_history(device_id,sequence,kind,wire) VALUES('orbit-server',?1,'change',?2)",
            params![sequence, wire],
        )?;
        let cursor = tx.last_insert_rowid();
        if cursor != sequence {
            return Err(AuthorityError::MigrationConflict);
        }
        tx.execute(
            "INSERT INTO authority_commands(command_id,actor,payload,result_json) VALUES(?1,?2,?3,?4)",
            params![self.command_id, self.actor, self.payload, result_json],
        )?;
        let history = {
            let mut stmt = tx.prepare(
                "SELECT cursor,device_id,sequence,kind,wire FROM authority_history ORDER BY cursor",
            )?;
            let rows = stmt.query_map([], |r| {
                Ok(AuthorityEvent {
                    cursor: r.get(0)?,
                    device_id: r.get(1)?,
                    sequence: r.get(2)?,
                    kind: r.get(3)?,
                    wire: r.get(4)?,
                })
            })?;
            rows.collect::<Result<Vec<_>, _>>()?
        };
        let view = crate::canonical::project(&history)
            .map_err(|_| AuthorityError::IncompleteProjection)?;
        if !view.complete || view.unresolved_conflicts != 0 {
            return Err(AuthorityError::IncompleteProjection);
        }
        let view_json = serde_json::to_string(&view)
            .map_err(|_| AuthorityError::Invalid("projection serialization"))?;
        tx.execute(
            "INSERT INTO authority_projection(id,authority_revision,view_json) VALUES(1,?1,?2) \
             ON CONFLICT(id) DO UPDATE SET authority_revision=excluded.authority_revision,view_json=excluded.view_json",
            params![cursor, view_json],
        )?;
        Ok(())
    }
}

pub(crate) enum CommandAdmission {
    Duplicate(String),
    New(i64),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::authority_evidence::{EvidenceAction, EvidenceCommand};
    use crate::domain_commands::{ProjectCommand, TaskAction, TaskCommand};
    use std::path::Path;

    fn project(id: &str, revision: i64, entity: &str) -> ProjectCommand {
        ProjectCommand {
            workflow_id: None,
            command_id: id.into(),
            expected_revision: revision,
            project_id: entity.into(),
            name: format!("Project {entity}"),
        }
    }
    fn task(id: &str, revision: i64, entity: &str, project: Option<&str>) -> TaskCommand {
        TaskCommand {
            command_id: id.into(),
            expected_revision: revision,
            action: TaskAction::Create {
                task_id: entity.into(),
                title: format!("Task {entity}"),
                project_id: project.map(str::to_owned),
                milestone_id: None,
                description: String::new(),
                status: None,
                priority: None,
                estimated_minutes: None,
                deadline_at_ms: None,
                earliest_start_at_ms: None,
                acceptance_criteria: Vec::new(),
                prerequisite_ids: Vec::new(),
            },
        }
    }
    fn habit(id: &str, revision: i64, entity: &str) -> EvidenceCommand {
        EvidenceCommand {
            command_id: id.into(),
            expected_revision: revision,
            action: EvidenceAction::CreateHabit {
                habit_id: entity.into(),
                name: "Daily reading".into(),
                cadence: "daily".into(),
            },
        }
    }

    #[test]
    fn mixed_domains_have_one_monotonic_revision_and_one_atomic_receipt_log() {
        let mut host = AuthorityStore::open(Path::new(":memory:")).unwrap();
        let p = host
            .authority_project_command("device-a", project("p-1", 0, "p"))
            .unwrap();
        assert_eq!(p.revision, 1);
        let t = host
            .authority_task_command("device-a", task("t-1", 1, "t", Some("p")))
            .unwrap();
        assert_eq!(t.revision, 2);
        let h = host
            .authority_evidence_command("device-a", habit("h-1", 2, "h"))
            .unwrap();
        assert_eq!(h.revision, 3);
        assert_eq!(host.authority_command_count().unwrap(), 3);
        assert_eq!(host.event_count().unwrap(), 3);
        assert_eq!(host.authority_full_snapshot().unwrap().0, 3);
    }

    #[test]
    fn cross_domain_command_id_collision_is_rejected_without_mutation() {
        let mut host = AuthorityStore::open(Path::new(":memory:")).unwrap();
        host.authority_project_command("device-a", project("same", 0, "p"))
            .unwrap();
        assert!(matches!(
            host.authority_task_command("device-a", task("same", 1, "t", None)),
            Err(AuthorityError::ImmutableConflict),
        ));
        assert!(matches!(
            host.authority_evidence_command("device-a", habit("same", 1, "h")),
            Err(AuthorityError::ImmutableConflict),
        ));
        assert_eq!(host.authority_command_count().unwrap(), 1);
        assert_eq!(host.event_count().unwrap(), 1);
        assert!(host.tasks().unwrap().1.is_empty());
        assert!(
            host.authority_evidence_snapshot()
                .unwrap()
                .habits
                .is_empty()
        );
    }

    #[test]
    fn retry_uses_original_receipt_regardless_of_current_revision() {
        let mut host = AuthorityStore::open(Path::new(":memory:")).unwrap();
        let cmd = project("create-p", 0, "p");
        host.authority_project_command("device-a", cmd.clone())
            .unwrap();
        host.authority_task_command("device-b", task("create-t", 1, "t", None))
            .unwrap();
        let retry = host.authority_project_command("device-a", cmd).unwrap();
        assert_eq!(retry.revision, 1);
        assert!(retry.duplicate);
        assert_eq!(host.event_count().unwrap(), 2);
    }

    #[test]
    fn stale_and_failed_domain_updates_do_not_advance_shared_history() {
        let mut host = AuthorityStore::open(Path::new(":memory:")).unwrap();
        host.authority_project_command("device-a", project("p-1", 0, "p"))
            .unwrap();
        assert!(matches!(
            host.authority_task_command("device-a", task("stale", 0, "t", None)),
            Err(AuthorityError::StaleRevision),
        ));
        assert!(matches!(
            host.authority_task_command("device-a", task("missing", 1, "t", Some("missing"))),
            Err(AuthorityError::Invalid(_)),
        ));
        assert_eq!(host.authority_command_count().unwrap(), 1);
        assert_eq!(host.event_count().unwrap(), 1);
        assert!(host.tasks().unwrap().1.is_empty());
        assert_eq!(host.authority_full_snapshot().unwrap().0, 1);
        assert_eq!(
            host.authority_task_command("device-a", task("valid", 1, "t", Some("p")))
                .unwrap()
                .revision,
            2
        );
    }

    #[test]
    fn an_immutable_receipt_is_bound_to_its_original_actor_and_payload() {
        let mut host = AuthorityStore::open(Path::new(":memory:")).unwrap();
        let cmd = project("first", 0, "p");
        host.authority_project_command("device-a", cmd.clone())
            .unwrap();
        assert!(matches!(
            host.authority_project_command("device-b", cmd),
            Err(AuthorityError::ImmutableConflict),
        ));
        assert!(matches!(
            host.authority_project_command("device-a", project("first", 0, "other")),
            Err(AuthorityError::ImmutableConflict),
        ));
        assert_eq!(host.event_count().unwrap(), 1);
    }
    #[test]
    fn missing_receipt_for_existing_history_fails_closed() {
        let mut host = AuthorityStore::open(Path::new(":memory:")).unwrap();
        host.authority_project_command("device-a", project("p-1", 0, "p"))
            .unwrap();
        host.conn
            .execute("DELETE FROM authority_commands", [])
            .unwrap();
        assert!(matches!(
            host.authority_task_command("device-a", task("t-1", 1, "t", None)),
            Err(AuthorityError::MigrationConflict)
        ));
        assert!(host.tasks().unwrap().1.is_empty());
        assert_eq!(host.event_count().unwrap(), 1);
    }

    #[test]
    fn receipt_without_corresponding_history_fails_closed() {
        let mut host = AuthorityStore::open(Path::new(":memory:")).unwrap();
        host.authority_project_command("device-a", project("p-1", 0, "p"))
            .unwrap();
        host.conn
            .execute("DELETE FROM authority_history", [])
            .unwrap();
        assert!(matches!(
            host.authority_task_command("device-a", task("t-1", 0, "t", None)),
            Err(AuthorityError::MigrationConflict)
        ));
        assert!(host.tasks().unwrap().1.is_empty());
    }

    #[test]
    fn missing_accepted_projection_rejects_new_commands() {
        let mut host = AuthorityStore::open(Path::new(":memory:")).unwrap();
        host.authority_project_command("device-a", project("p-1", 0, "p"))
            .unwrap();
        host.conn
            .execute("DELETE FROM authority_projection", [])
            .unwrap();
        assert!(matches!(
            host.authority_task_command("device-a", task("t-1", 1, "t", None)),
            Err(AuthorityError::MigrationConflict)
        ));
        assert_eq!(host.authority_command_count().unwrap(), 1);
        assert!(host.tasks().unwrap().1.is_empty());
    }

    #[test]
    fn forged_projection_revision_rejects_new_commands() {
        let mut host = AuthorityStore::open(Path::new(":memory:")).unwrap();
        host.authority_project_command("device-a", project("p-1", 0, "p"))
            .unwrap();
        host.conn
            .execute("UPDATE authority_projection SET authority_revision=99", [])
            .unwrap();
        assert!(matches!(
            host.authority_task_command("device-a", task("t-1", 1, "t", None)),
            Err(AuthorityError::MigrationConflict)
        ));
        assert!(host.tasks().unwrap().1.is_empty());
    }
}
