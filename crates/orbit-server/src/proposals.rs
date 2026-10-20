//! Persistent approval inbox for remote agent intents. Approval is not execution:
//! adapters must separately claim an approved intent and report the outcome.
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug)]
pub enum ProposalError {
    Sql(rusqlite::Error),
    Invalid,
    Conflict,
}
impl From<rusqlite::Error> for ProposalError {
    fn from(v: rusqlite::Error) -> Self {
        Self::Sql(v)
    }
}
#[derive(Debug, Deserialize)]
pub struct NewProposal {
    pub intent: String,
    pub target: String,
    pub payload: serde_json::Value,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct Proposal {
    pub id: i64,
    pub device_id: String,
    pub intent: String,
    pub target: String,
    pub payload: serde_json::Value,
    pub state: String,
    pub created_ms: i64,
    pub decided_ms: Option<i64>,
    pub decided_by: Option<String>,
}
#[derive(Debug, Serialize)]
pub struct Audit {
    pub id: i64,
    pub proposal_id: i64,
    pub action: String,
    pub actor: String,
    pub at_ms: i64,
}
pub struct ProposalStore {
    conn: Connection,
}
impl ProposalStore {
    pub fn open(path: &Path) -> Result<Self, ProposalError> {
        let conn = Connection::open(path)?;
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.execute_batch("CREATE TABLE IF NOT EXISTS agent_proposals(\
          id INTEGER PRIMARY KEY AUTOINCREMENT, device_id TEXT NOT NULL, intent TEXT NOT NULL,\
          target TEXT NOT NULL, payload TEXT NOT NULL, state TEXT NOT NULL CHECK(state IN ('pending','approved','rejected','leased','succeeded','failed')),\
          created_ms INTEGER NOT NULL, decided_ms INTEGER, decided_by TEXT);\
          CREATE TABLE IF NOT EXISTS agent_proposal_audit(\
          id INTEGER PRIMARY KEY AUTOINCREMENT, proposal_id INTEGER NOT NULL REFERENCES agent_proposals(id),\
          action TEXT NOT NULL, actor TEXT NOT NULL, at_ms INTEGER NOT NULL);\
          CREATE INDEX IF NOT EXISTS idx_agent_proposals_state ON agent_proposals(state,id);")?;
        // Existing v060 CHECK constraints do not allow leased/terminal states.
        // Rebuild the table inside a transaction without dropping audit history.
        let legacy: String = conn.query_row(
            "SELECT sql FROM sqlite_master WHERE name='agent_proposals'",
            [],
            |r| r.get(0),
        )?;
        if !legacy.contains("'leased'") {
            conn.execute_batch("PRAGMA legacy_alter_table=ON; BEGIN IMMEDIATE; ALTER TABLE agent_proposals RENAME TO agent_proposals_v060; \
                CREATE TABLE agent_proposals(id INTEGER PRIMARY KEY AUTOINCREMENT, device_id TEXT NOT NULL, \
                intent TEXT NOT NULL,target TEXT NOT NULL,payload TEXT NOT NULL, \
                state TEXT NOT NULL CHECK(state IN ('pending','approved','rejected','leased','succeeded','failed')), \
                created_ms INTEGER NOT NULL,decided_ms INTEGER,decided_by TEXT); \
                INSERT INTO agent_proposals SELECT * FROM agent_proposals_v060; \
                DROP TABLE agent_proposals_v060; \
                CREATE INDEX IF NOT EXISTS idx_agent_proposals_state ON agent_proposals(state,id); COMMIT;")?;
        }
        // Additive migration: original v060 proposal rows remain readable.
        let columns = {
            let mut stmt = conn.prepare("PRAGMA table_info(agent_proposals)")?;
            stmt.query_map([], |r| r.get::<_, String>(1))?
                .collect::<Result<Vec<_>, _>>()?
        };
        for (column, definition) in [
            ("lease_owner", "TEXT"),
            ("lease_until_ms", "INTEGER"),
            ("attempts", "INTEGER NOT NULL DEFAULT 0"),
            ("result_json", "TEXT"),
        ] {
            if !columns.iter().any(|c| c == column) {
                conn.execute_batch(&format!(
                    "ALTER TABLE agent_proposals ADD COLUMN {column} {definition}"
                ))?;
            }
        }
        Ok(Self { conn })
    }
    pub fn propose(
        &mut self,
        device: &str,
        req: NewProposal,
        now: i64,
    ) -> Result<Proposal, ProposalError> {
        if !matches!(
            req.intent.as_str(),
            "task-create" | "task-update" | "habit-record" | "metric-observe"
        ) || req.target.trim().is_empty()
            || req.target.len() > 256
            || req.payload.is_null()
        {
            return Err(ProposalError::Invalid);
        }
        // Executable task-status proposals use exactly the worker's allowlisted
        // payload grammar, so malformed requests fail before human approval.
        if req.intent == "task-update" {
            let valid_status = req
                .payload
                .as_object()
                .and_then(|obj| {
                    if obj.len() != 1 {
                        return None;
                    }
                    obj.get("status")?.as_str()
                })
                .is_some_and(|status| {
                    matches!(
                        status,
                        "inbox"
                            | "planned"
                            | "ready"
                            | "in-progress"
                            | "waiting"
                            | "blocked"
                            | "review"
                            | "deferred"
                            | "done"
                            | "cancelled"
                    )
                });
            if !valid_status {
                return Err(ProposalError::Invalid);
            }
        }
        if req.intent == "task-create" {
            let valid_title = req
                .payload
                .as_object()
                .and_then(|obj| {
                    if obj.len() != 1 {
                        return None;
                    }
                    obj.get("title")?.as_str()
                })
                .is_some_and(|title| !title.trim().is_empty() && title.len() <= 250);
            if !valid_title {
                return Err(ProposalError::Invalid);
            }
        }
        if req.intent == "habit-record" {
            let valid = req
                .payload
                .as_object()
                .and_then(|obj| {
                    if obj.len() != 1 {
                        return None;
                    }
                    obj.get("at_ms")?.as_i64()
                })
                .is_some_and(|at_ms| at_ms > 0);
            if !valid {
                return Err(ProposalError::Invalid);
            }
        }
        if req.intent == "metric-observe" {
            let valid = req
                .payload
                .as_object()
                .and_then(|obj| {
                    if obj.len() != 2 {
                        return None;
                    }
                    Some((obj.get("at_ms")?.as_i64()?, obj.get("value")?.as_i64()?))
                })
                .is_some_and(|(at_ms, _)| at_ms > 0);
            if !valid {
                return Err(ProposalError::Invalid);
            }
        }
        let payload = serde_json::to_string(&req.payload).map_err(|_| ProposalError::Invalid)?;
        if payload.len() > 16_384 {
            return Err(ProposalError::Invalid);
        }
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        tx.execute("INSERT INTO agent_proposals(device_id,intent,target,payload,state,created_ms) VALUES(?1,?2,?3,?4,'pending',?5)",
          params![device,req.intent,req.target,payload,now])?;
        let id = tx.last_insert_rowid();
        tx.execute("INSERT INTO agent_proposal_audit(proposal_id,action,actor,at_ms) VALUES(?1,'proposed',?2,?3)",params![id,device,now])?;
        tx.commit()?;
        self.get(id)?.ok_or(ProposalError::Conflict)
    }
    fn decode(r: &rusqlite::Row<'_>) -> rusqlite::Result<Proposal> {
        let payload: String = r.get(4)?;
        let parsed = serde_json::from_str(&payload).map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(4, rusqlite::types::Type::Text, Box::new(e))
        })?;
        Ok(Proposal {
            id: r.get(0)?,
            device_id: r.get(1)?,
            intent: r.get(2)?,
            target: r.get(3)?,
            payload: parsed,
            state: r.get(5)?,
            created_ms: r.get(6)?,
            decided_ms: r.get(7)?,
            decided_by: r.get(8)?,
        })
    }
    pub fn get(&self, id: i64) -> Result<Option<Proposal>, ProposalError> {
        Ok(self.conn.query_row("SELECT id,device_id,intent,target,payload,state,created_ms,decided_ms,decided_by FROM agent_proposals WHERE id=?1",[id],Self::decode).optional()?)
    }
    pub fn list(&self) -> Result<Vec<Proposal>, ProposalError> {
        let mut stmt=self.conn.prepare("SELECT id,device_id,intent,target,payload,state,created_ms,decided_ms,decided_by FROM agent_proposals ORDER BY id DESC LIMIT 200")?;
        Ok(stmt
            .query_map([], Self::decode)?
            .collect::<Result<Vec<_>, _>>()?)
    }
    pub fn decide(&mut self, id: i64, approve: bool, now: i64) -> Result<bool, ProposalError> {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let state = if approve { "approved" } else { "rejected" };
        let changed=tx.execute("UPDATE agent_proposals SET state=?1,decided_ms=?2,decided_by='admin' WHERE id=?3 AND state='pending'",params![state,now,id])?;
        if changed == 1 {
            tx.execute("INSERT INTO agent_proposal_audit(proposal_id,action,actor,at_ms) VALUES(?1,?2,'admin',?3)",params![id,state,now])?;
        }
        tx.commit()?;
        Ok(changed == 1)
    }
    // Only an administrator-approved proposal can be dispatched. Leases are
    // transactional and scoped to the authenticated device that claims them.
    pub fn claim(&mut self, worker: &str, now: i64) -> Result<Option<Proposal>, ProposalError> {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let id: Option<i64> = tx
            .query_row(
                "SELECT id FROM agent_proposals WHERE state='approved' ORDER BY id LIMIT 1",
                [],
                |r| r.get(0),
            )
            .optional()?;
        if let Some(id) = id {
            let changed=tx.execute("UPDATE agent_proposals SET state='leased',lease_owner=?1,lease_until_ms=?2,attempts=attempts+1 WHERE id=?3 AND state='approved'",
                params![worker,now.saturating_add(120_000),id])?;
            if changed != 1 {
                return Err(ProposalError::Conflict);
            }
            tx.execute("INSERT INTO agent_proposal_audit(proposal_id,action,actor,at_ms) VALUES(?1,'claimed',?2,?3)",params![id,worker,now])?;
            tx.commit()?;
            return self.get(id);
        }
        tx.commit()?;
        Ok(None)
    }
    pub fn settle(
        &mut self,
        id: i64,
        worker: &str,
        now: i64,
        succeeded: bool,
        result: &serde_json::Value,
    ) -> Result<bool, ProposalError> {
        let payload = serde_json::to_string(result).map_err(|_| ProposalError::Invalid)?;
        if payload.len() > 60_000 {
            return Err(ProposalError::Invalid);
        }
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let final_state = if succeeded { "succeeded" } else { "failed" };
        let changed=tx.execute("UPDATE agent_proposals SET state=?1,result_json=?2,lease_until_ms=NULL WHERE id=?3 AND state='leased' AND lease_owner=?4 AND lease_until_ms>=?5",
            params![final_state,payload,id,worker,now])?;
        if changed == 1 {
            tx.execute("INSERT INTO agent_proposal_audit(proposal_id,action,actor,at_ms) VALUES(?1,?2,?3,?4)",params![id,final_state,worker,now])?;
        }
        tx.commit()?;
        Ok(changed == 1)
    }
    // Expired leases need operator reconciliation. Mutating actions are not
    // automatically retried: the prior worker may have committed locally.
    #[allow(dead_code)]
    pub fn expired_leases(&self, now: i64) -> Result<Vec<i64>, ProposalError> {
        let mut stmt = self.conn.prepare(
            "SELECT id FROM agent_proposals WHERE state='leased' AND lease_until_ms<?1 ORDER BY id",
        )?;
        Ok(stmt
            .query_map([now], |r| r.get(0))?
            .collect::<Result<Vec<_>, _>>()?)
    }
    pub fn audit(&self, id: i64) -> Result<Vec<Audit>, ProposalError> {
        let mut stmt=self.conn.prepare("SELECT id,proposal_id,action,actor,at_ms FROM agent_proposal_audit WHERE proposal_id=?1 ORDER BY id")?;
        Ok(stmt
            .query_map([id], |r| {
                Ok(Audit {
                    id: r.get(0)?,
                    proposal_id: r.get(1)?,
                    action: r.get(2)?,
                    actor: r.get(3)?,
                    at_ms: r.get(4)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn approval_is_explicit_and_audited() {
        let mut s = ProposalStore::open(Path::new(":memory:")).unwrap();
        let p = s
            .propose(
                "device-a",
                NewProposal {
                    intent: "task-create".into(),
                    target: "project-1".into(),
                    payload: serde_json::json!({"title":"check"}),
                },
                100,
            )
            .unwrap();
        assert_eq!(p.state, "pending");
        assert!(s.decide(p.id, true, 101).unwrap());
        assert!(!s.decide(p.id, false, 102).unwrap());
        assert_eq!(s.get(p.id).unwrap().unwrap().state, "approved");
        assert_eq!(s.audit(p.id).unwrap().len(), 2);
    }
    #[test]
    fn disallow_unrecognized_actions() {
        let mut s = ProposalStore::open(Path::new(":memory:")).unwrap();
        assert!(matches!(
            s.propose(
                "device",
                NewProposal {
                    intent: "execute-shell".into(),
                    target: "x".into(),
                    payload: serde_json::json!({"cmd":"sh"})
                },
                1
            ),
            Err(ProposalError::Invalid)
        ));
    }
}
#[cfg(test)]
mod execution_tests {
    use super::*;
    #[test]
    fn approved_proposal_is_leased_once_and_settled_by_owner() {
        let mut s = ProposalStore::open(Path::new(":memory:")).unwrap();
        let p = s
            .propose(
                "device-a",
                NewProposal {
                    intent: "task-update".into(),
                    target: "task-1".into(),
                    payload: serde_json::json!({"status":"done"}),
                },
                1,
            )
            .unwrap();
        assert!(s.claim("worker-1", 2).unwrap().is_none());
        s.decide(p.id, true, 3).unwrap();
        assert_eq!(s.claim("worker-1", 4).unwrap().unwrap().id, p.id);
        assert!(s.claim("worker-2", 5).unwrap().is_none());
        assert!(
            !s.settle(p.id, "worker-2", 6, true, &serde_json::json!({"ok":true}))
                .unwrap()
        );
        assert!(
            s.settle(p.id, "worker-1", 7, true, &serde_json::json!({"ok":true}))
                .unwrap()
        );
        assert!(
            !s.settle(p.id, "worker-1", 8, true, &serde_json::json!({"ok":true}))
                .unwrap()
        );
        assert_eq!(s.get(p.id).unwrap().unwrap().state, "succeeded");
        assert_eq!(s.audit(p.id).unwrap().len(), 4);
    }
    #[test]
    fn expired_mutation_remains_held_for_manual_reconciliation() {
        let mut s = ProposalStore::open(Path::new(":memory:")).unwrap();
        let p = s
            .propose(
                "d",
                NewProposal {
                    intent: "task-update".into(),
                    target: "t".into(),
                    payload: serde_json::json!({"status":"done"}),
                },
                0,
            )
            .unwrap();
        s.decide(p.id, true, 1).unwrap();
        s.claim("a", 2).unwrap();
        assert_eq!(s.expired_leases(120_003).unwrap(), vec![p.id]);
        assert!(s.claim("b", 120_004).unwrap().is_none());
    }
}

#[cfg(test)]
mod browser_proposal_tests {
    use super::*;
    #[test]
    fn browser_task_proposals_are_pending_and_validate_worker_grammar() {
        let mut store = ProposalStore::open(Path::new(":memory:")).unwrap();
        for payload in [
            serde_json::json!({"status":"done","extra":true}),
            serde_json::json!({"status":"invalid"}),
            serde_json::json!({"status":5}),
        ] {
            assert!(matches!(
                store.propose(
                    "admin-console",
                    NewProposal {
                        intent: "task-update".into(),
                        target: "task-1".into(),
                        payload
                    },
                    100
                ),
                Err(ProposalError::Invalid)
            ));
        }
        let p = store
            .propose(
                "admin-console",
                NewProposal {
                    intent: "task-update".into(),
                    target: "task-1".into(),
                    payload: serde_json::json!({"status":"done"}),
                },
                101,
            )
            .unwrap();
        assert_eq!(p.state, "pending");
        assert_eq!(p.device_id, "admin-console");
        assert!(store.claim("worker-1", 102).unwrap().is_none());
        assert!(store.decide(p.id, true, 103).unwrap());
        assert_eq!(store.claim("worker-1", 104).unwrap().unwrap().id, p.id);
    }
}
