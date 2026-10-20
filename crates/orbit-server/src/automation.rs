//! Durable single-user automation queue. The server schedules named, declarative
//! jobs; authenticated devices execute them explicitly, outside this process.
//! No arbitrary shell commands or remote code are accepted.
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Deserialize)]
pub struct JobInput {
    pub id: String,
    pub action: String,
    pub interval_seconds: i64,
    pub enabled: bool,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct Job {
    pub id: String,
    pub action: String,
    pub interval_seconds: i64,
    pub enabled: bool,
    pub next_run_ms: i64,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct Run {
    pub id: i64,
    pub job_id: String,
    pub action: String,
    pub state: String,
    pub scheduled_at_ms: i64,
    pub lease_until_ms: Option<i64>,
    pub worker_id: Option<String>,
    pub attempt: i64,
    pub result_json: Option<String>,
}
#[derive(Debug, Deserialize)]
pub struct SettleInput {
    pub succeeded: bool,
    pub result_json: Option<String>,
}

#[derive(Debug)]
#[allow(dead_code)]
pub enum QueueError {
    Sql(rusqlite::Error),
    Invalid(&'static str),
    Conflict,
}
impl From<rusqlite::Error> for QueueError {
    fn from(e: rusqlite::Error) -> Self {
        Self::Sql(e)
    }
}
pub struct AutomationStore {
    conn: Connection,
}
impl AutomationStore {
    pub fn open(path: &Path) -> Result<Self, QueueError> {
        let conn = Connection::open(path)?;
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.execute_batch("\
          CREATE TABLE IF NOT EXISTS automation_jobs(\
            id TEXT PRIMARY KEY, action TEXT NOT NULL, interval_seconds INTEGER NOT NULL,\
            enabled INTEGER NOT NULL, next_run_ms INTEGER NOT NULL\
          );\
          CREATE TABLE IF NOT EXISTS automation_runs(\
            id INTEGER PRIMARY KEY AUTOINCREMENT,\
            job_id TEXT NOT NULL REFERENCES automation_jobs(id),\
            action TEXT NOT NULL, state TEXT NOT NULL, scheduled_at_ms INTEGER NOT NULL,\
            lease_until_ms INTEGER, worker_id TEXT, attempt INTEGER NOT NULL DEFAULT 0,\
            UNIQUE(job_id,scheduled_at_ms)\
          );\
          CREATE INDEX IF NOT EXISTS idx_automation_ready ON automation_runs(state,scheduled_at_ms);")?;
        // Additive migration for pre-worker M17 installations.
        let has_result = conn
            .prepare("PRAGMA table_info(automation_runs)")?
            .query_map([], |r| r.get::<_, String>(1))?
            .collect::<Result<Vec<_>, _>>()?
            .iter()
            .any(|c| c == "result_json");
        if !has_result {
            conn.execute(
                "ALTER TABLE automation_runs ADD COLUMN result_json TEXT",
                [],
            )?;
        }
        Ok(Self { conn })
    }
    pub fn put(&mut self, input: JobInput, now_ms: i64) -> Result<Job, QueueError> {
        if input.id.is_empty()
            || input.id.len() > 128
            || !input
                .id
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
        {
            return Err(QueueError::Invalid("invalid job id"));
        }
        // Actions are a deliberately narrow allow-list; later M17 adapters map
        // these names onto Orbit application services with separate permissions.
        if !matches!(
            input.action.as_str(),
            "project-health-refresh" | "daily-briefing" | "habit-summary"
        ) {
            return Err(QueueError::Invalid("unknown action"));
        }
        if !(60..=31_536_000).contains(&input.interval_seconds) {
            return Err(QueueError::Invalid("invalid interval"));
        }
        let next = now_ms
            .checked_add(
                input
                    .interval_seconds
                    .checked_mul(1000)
                    .ok_or(QueueError::Invalid("interval overflow"))?,
            )
            .ok_or(QueueError::Invalid("timestamp overflow"))?;
        self.conn.execute("INSERT INTO automation_jobs(id,action,interval_seconds,enabled,next_run_ms) VALUES(?1,?2,?3,?4,?5)\
          ON CONFLICT(id) DO UPDATE SET action=excluded.action,interval_seconds=excluded.interval_seconds,enabled=excluded.enabled,next_run_ms=excluded.next_run_ms",
          params![input.id,input.action,input.interval_seconds,input.enabled,next])?;
        Ok(Job {
            id: input.id,
            action: input.action,
            interval_seconds: input.interval_seconds,
            enabled: input.enabled,
            next_run_ms: next,
        })
    }
    /// Request an immediate, worker-executed refresh for a configured job.
    /// Coalesce outstanding requests, without interrupting the regular schedule.
    /// Failed/expired worker execution follows the existing bounded retry policy.
    pub fn request_refresh(&mut self, id: &str, now_ms: i64) -> Result<Run, QueueError> {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let action: Option<String> = tx
            .query_row(
                "SELECT action FROM automation_jobs WHERE id=?1 AND enabled=1",
                [id],
                |r| r.get(0),
            )
            .optional()?;
        let action = action.ok_or(QueueError::Invalid("enabled job not found"))?;
        let existing: Option<Run> = tx.query_row(
            "SELECT id,job_id,action,state,scheduled_at_ms,lease_until_ms,worker_id,attempt,result_json FROM automation_runs WHERE job_id=?1 AND state IN ('ready','leased') ORDER BY id DESC LIMIT 1",
            [id], read_run
        ).optional()?;
        if let Some(run) = existing {
            tx.commit()?;
            return Ok(run);
        }
        // The job has a unique (job_id, scheduled_at_ms) identity; advance by
        // one millisecond if a completed periodic run shares this exact instant.
        let previous: Option<i64> = tx.query_row(
            "SELECT MAX(scheduled_at_ms) FROM automation_runs WHERE job_id=?1",
            [id],
            |r| r.get(0),
        )?;
        let scheduled = if let Some(last) = previous {
            now_ms.max(
                last.checked_add(1)
                    .ok_or(QueueError::Invalid("timestamp overflow"))?,
            )
        } else {
            now_ms
        };
        tx.execute("INSERT INTO automation_runs(job_id,action,state,scheduled_at_ms) VALUES(?1,?2,'ready',?3)",params![id,action,scheduled])?;
        let run = tx.query_row(
            "SELECT id,job_id,action,state,scheduled_at_ms,lease_until_ms,worker_id,attempt,result_json FROM automation_runs WHERE job_id=?1 AND scheduled_at_ms=?2",
            params![id,scheduled], read_run
        )?;
        tx.commit()?;
        Ok(run)
    }
    pub fn jobs(&self) -> Result<Vec<Job>, QueueError> {
        let mut stmt=self.conn.prepare("SELECT id,action,interval_seconds,enabled,next_run_ms FROM automation_jobs ORDER BY id")?;
        let rows = stmt.query_map([], |r| {
            Ok(Job {
                id: r.get(0)?,
                action: r.get(1)?,
                interval_seconds: r.get(2)?,
                enabled: r.get(3)?,
                next_run_ms: r.get(4)?,
            })
        })?;
        let jobs = rows.collect::<Result<Vec<_>, _>>()?;
        Ok(jobs)
    }
    pub fn delete(&mut self, id: &str) -> Result<bool, QueueError> {
        // Retain historical runs for audit; disable instead of deleting jobs.
        Ok(self.conn.execute(
            "UPDATE automation_jobs SET enabled=0 WHERE id=?1 AND enabled=1",
            [id],
        )? == 1)
    }
    /// Schedule each due interval exactly once, even after process restarts.
    /// Limit catch-up to one run/job per tick to avoid storming after downtime.
    pub fn tick(&mut self, now_ms: i64) -> Result<usize, QueueError> {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut stmt=tx.prepare("SELECT id,action,interval_seconds,next_run_ms FROM automation_jobs WHERE enabled=1 AND next_run_ms<=?1 ORDER BY id")?;
        let due = stmt
            .query_map([now_ms], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, i64>(2)?,
                    r.get::<_, i64>(3)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        drop(stmt);
        for (id, action, interval, next) in &due {
            tx.execute("INSERT OR IGNORE INTO automation_runs(job_id,action,state,scheduled_at_ms) VALUES(?1,?2,'ready',?3)",params![id,action,next])?;
            let next_time = now_ms
                .checked_add(
                    interval
                        .checked_mul(1000)
                        .ok_or(QueueError::Invalid("interval overflow"))?,
                )
                .ok_or(QueueError::Invalid("timestamp overflow"))?;
            tx.execute(
                "UPDATE automation_jobs SET next_run_ms=?2 WHERE id=?1",
                params![id, next_time],
            )?;
        }
        tx.commit()?;
        Ok(due.len())
    }
    /// Return a run to a worker at most once per active lease. Expired leases
    /// are retried; execution must use run.id as its idempotency key.
    pub fn claim(&mut self, worker: &str, now_ms: i64) -> Result<Option<Run>, QueueError> {
        if worker.trim().is_empty() || worker.len() > 128 {
            return Err(QueueError::Invalid("worker id"));
        }
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let found:Option<i64>=tx.query_row("SELECT id FROM automation_runs WHERE state='ready' OR (state='leased' AND lease_until_ms<=?1 AND attempt<3) ORDER BY scheduled_at_ms,id LIMIT 1",[now_ms],|r|r.get(0)).optional()?;
        let Some(id) = found else { return Ok(None) };
        let until = now_ms
            .checked_add(300_000)
            .ok_or(QueueError::Invalid("timestamp overflow"))?;
        tx.execute("UPDATE automation_runs SET state='leased',worker_id=?2,lease_until_ms=?3,attempt=attempt+1 WHERE id=?1",params![id,worker,until])?;
        let run=tx.query_row("SELECT id,job_id,action,state,scheduled_at_ms,lease_until_ms,worker_id,attempt,result_json FROM automation_runs WHERE id=?1",[id],read_run)?;
        tx.commit()?;
        Ok(Some(run))
    }
    pub fn settle(
        &mut self,
        id: i64,
        worker: &str,
        now_ms: i64,
        succeeded: bool,
        result_json: Option<&str>,
    ) -> Result<bool, QueueError> {
        if result_json.is_some_and(|v| {
            v.len() > 65_536 || serde_json::from_str::<serde_json::Value>(v).is_err()
        }) {
            return Err(QueueError::Invalid("result must be JSON <=64 KiB"));
        }
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let attempts:Option<i64>=tx.query_row("SELECT attempt FROM automation_runs WHERE id=?1 AND worker_id=?2 AND state='leased' AND lease_until_ms>?3",params![id,worker,now_ms],|r|r.get(0)).optional()?;
        let Some(attempts) = attempts else {
            return Ok(false);
        };
        let target = if succeeded {
            "succeeded"
        } else if attempts >= 3 {
            "failed"
        } else {
            "ready"
        };
        let changed=tx.execute("UPDATE automation_runs SET state=?4,lease_until_ms=NULL,worker_id=NULL,result_json=?5 WHERE id=?1 AND worker_id=?2 AND state='leased' AND lease_until_ms>?3",params![id,worker,now_ms,target,result_json])?==1;
        tx.commit()?;
        Ok(changed)
    }
    pub fn runs(&self, limit: i64) -> Result<Vec<Run>, QueueError> {
        if !(1..=200).contains(&limit) {
            return Err(QueueError::Invalid("invalid limit"));
        }
        let mut stmt=self.conn.prepare("SELECT id,job_id,action,state,scheduled_at_ms,lease_until_ms,worker_id,attempt,result_json FROM automation_runs ORDER BY id DESC LIMIT ?1")?;
        let rows = stmt.query_map([limit], read_run)?;
        let runs = rows.collect::<Result<Vec<_>, _>>()?;
        Ok(runs)
    }
}
fn read_run(r: &rusqlite::Row<'_>) -> rusqlite::Result<Run> {
    Ok(Run {
        id: r.get(0)?,
        job_id: r.get(1)?,
        action: r.get(2)?,
        state: r.get(3)?,
        scheduled_at_ms: r.get(4)?,
        lease_until_ms: r.get(5)?,
        worker_id: r.get(6)?,
        attempt: r.get(7)?,
        result_json: r.get(8)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn requested_refresh_is_durable_coalesced_and_does_not_change_schedule() {
        let mut q = AutomationStore::open(Path::new(":memory:")).unwrap();
        let job = q
            .put(
                JobInput {
                    id: "daily".into(),
                    action: "daily-briefing".into(),
                    interval_seconds: 3600,
                    enabled: true,
                },
                1000,
            )
            .unwrap();
        let first = q.request_refresh("daily", 2000).unwrap();
        assert_eq!(first.id, q.request_refresh("daily", 2001).unwrap().id);
        assert_eq!(first.id, q.claim("worker", 2002).unwrap().unwrap().id);
        assert_eq!(first.id, q.request_refresh("daily", 2003).unwrap().id);
        assert!(q.settle(first.id, "worker", 2004, true, None).unwrap());
        let second = q.request_refresh("daily", 2004).unwrap();
        assert_ne!(first.id, second.id);
        assert_eq!(q.jobs().unwrap()[0].next_run_ms, job.next_run_ms);
        q.delete("daily").unwrap();
        assert!(matches!(
            q.request_refresh("daily", 3000),
            Err(QueueError::Invalid(_))
        ));
    }
    #[test]
    fn durable_schedule_claim_and_settle() {
        let mut q = AutomationStore::open(Path::new(":memory:")).unwrap();
        q.put(
            JobInput {
                id: "daily".into(),
                action: "daily-briefing".into(),
                interval_seconds: 60,
                enabled: true,
            },
            1_000,
        )
        .unwrap();
        assert_eq!(q.tick(60_999).unwrap(), 0);
        assert_eq!(q.tick(61_000).unwrap(), 1);
        assert_eq!(q.tick(61_000).unwrap(), 0);
        let run = q.claim("device-a", 61_001).unwrap().unwrap();
        assert_eq!(run.attempt, 1);
        assert!(q.claim("device-b", 61_002).unwrap().is_none());
        assert!(!q.settle(run.id, "device-b", 61_003, true, None).unwrap());
        assert!(q.settle(run.id, "device-a", 61_003, true, None).unwrap());
        assert!(!q.settle(run.id, "device-a", 61_004, true, None).unwrap());
        assert_eq!(q.runs(5).unwrap()[0].state, "succeeded");
    }
    #[test]
    fn failed_run_retries_then_preserves_audit() {
        let mut q = AutomationStore::open(Path::new(":memory:")).unwrap();
        q.put(
            JobInput {
                id: "retry".into(),
                action: "daily-briefing".into(),
                interval_seconds: 60,
                enabled: true,
            },
            0,
        )
        .unwrap();
        q.tick(60_000).unwrap();
        for attempt in 1..=3 {
            let at = 60_000 + attempt;
            let run = q.claim("worker", at).unwrap().unwrap();
            assert_eq!(run.attempt, attempt);
            assert!(
                q.settle(
                    run.id,
                    "worker",
                    at + 1,
                    false,
                    Some(r#"{"error":"temporary"}"#)
                )
                .unwrap()
            );
        }
        assert!(q.claim("worker", 70_000).unwrap().is_none());
        let item = &q.runs(10).unwrap()[0];
        assert_eq!(item.state, "failed");
        assert_eq!(
            item.result_json.as_deref(),
            Some(r#"{"error":"temporary"}"#)
        );
    }
    #[test]
    fn expired_lease_is_reclaimed() {
        let mut q = AutomationStore::open(Path::new(":memory:")).unwrap();
        q.put(
            JobInput {
                id: "daily".into(),
                action: "habit-summary".into(),
                interval_seconds: 60,
                enabled: true,
            },
            0,
        )
        .unwrap();
        q.tick(60_000).unwrap();
        let first = q.claim("a", 60_000).unwrap().unwrap();
        let second = q.claim("b", 360_000).unwrap().unwrap();
        assert_eq!(first.id, second.id);
        assert_eq!(second.attempt, 2);
        assert!(!q.settle(first.id, "a", 360_001, true, None).unwrap());
    }
}
