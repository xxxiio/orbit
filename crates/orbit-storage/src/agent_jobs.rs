//! Authority-local durable job reservations. Runtime effects occur only after reservation.
use crate::{SqliteStore, repo_error};
use orbit_application::RepositoryError;
use orbit_domain::{AgentJob, AgentJobState, ProjectId, TaskId};
use rusqlite::{OptionalExtension, params};

pub(crate) const AGENT_JOB_SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS agent_jobs (
 id TEXT PRIMARY KEY NOT NULL,
 request_id TEXT NOT NULL UNIQUE,
 project_id TEXT NOT NULL,
 task_id TEXT NOT NULL,
 launch_revision INTEGER NOT NULL CHECK(launch_revision >= 0),
 agent_kind TEXT NOT NULL CHECK(length(trim(agent_kind)) > 0),
 machine_ref TEXT NOT NULL CHECK(length(trim(machine_ref)) > 0),
 runtime_kind TEXT NOT NULL CHECK(length(trim(runtime_kind)) > 0),
 repository_ref TEXT NOT NULL CHECK(length(trim(repository_ref)) > 0),
 state TEXT NOT NULL CHECK(state IN ('queued','preparing','launching','running','waiting_input','blocked','completed','failed','interrupted','unreachable')),
 revision INTEGER NOT NULL DEFAULT 0,
 created_at_ms INTEGER NOT NULL,
 updated_at_ms INTEGER NOT NULL,
 last_seen_at_ms INTEGER,
 runtime_session_ref TEXT
);
CREATE UNIQUE INDEX IF NOT EXISTS agent_jobs_one_active_per_task ON agent_jobs(task_id)
 WHERE state NOT IN ('completed','failed','interrupted');
CREATE INDEX IF NOT EXISTS agent_jobs_project ON agent_jobs(project_id, updated_at_ms);
CREATE TABLE IF NOT EXISTS agent_job_execution_options (
 job_id TEXT PRIMARY KEY NOT NULL REFERENCES agent_jobs(id),
 worktree INTEGER NOT NULL CHECK(worktree IN (0,1))
);
CREATE TABLE IF NOT EXISTS agent_job_handoffs (
 job_id TEXT NOT NULL REFERENCES agent_jobs(id),
 handoff_id TEXT NOT NULL,
 reported_at_ms INTEGER NOT NULL CHECK(reported_at_ms>=0),
 payload_json TEXT NOT NULL CHECK(json_valid(payload_json)),
 PRIMARY KEY(job_id,handoff_id)
);
CREATE INDEX IF NOT EXISTS agent_job_handoffs_recent ON agent_job_handoffs(job_id,reported_at_ms);
CREATE TABLE IF NOT EXISTS agent_job_reviews (
 job_id TEXT PRIMARY KEY REFERENCES agent_jobs(id),
 decision TEXT NOT NULL CHECK(decision IN ('approved','changes_requested')),
 reviewer TEXT NOT NULL CHECK(length(trim(reviewer)) > 0),
 reason TEXT NOT NULL,
 reviewed_at_ms INTEGER NOT NULL CHECK(reviewed_at_ms >= 0),
 handoff_id TEXT NOT NULL,
 FOREIGN KEY(job_id,handoff_id) REFERENCES agent_job_handoffs(job_id,handoff_id)
);
CREATE TABLE IF NOT EXISTS agent_job_owners (
 job_id TEXT PRIMARY KEY NOT NULL REFERENCES agent_jobs(id),
 owner_device_id TEXT NOT NULL CHECK(length(trim(owner_device_id)) > 0)
);
CREATE TABLE IF NOT EXISTS agent_job_launch_claims (
 job_id TEXT PRIMARY KEY NOT NULL REFERENCES agent_jobs(id),
 claim_token TEXT NOT NULL UNIQUE,
 claimed_at_ms INTEGER NOT NULL,
 dispatch_state TEXT NOT NULL CHECK(dispatch_state IN ('claimed','confirmed','ambiguous')),
 session_ref TEXT
);
CREATE TABLE IF NOT EXISTS agent_job_briefs (
 job_id TEXT PRIMARY KEY NOT NULL REFERENCES agent_jobs(id),
 task_title TEXT NOT NULL,
 task_status TEXT NOT NULL,
 brief_json TEXT NOT NULL CHECK(json_valid(brief_json)),
 recorded_at_ms INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS agent_job_commands (
 command_id TEXT PRIMARY KEY NOT NULL,
 job_id TEXT NOT NULL REFERENCES agent_jobs(id),
 expected_revision INTEGER NOT NULL CHECK(expected_revision >= 0),
 command_kind TEXT NOT NULL CHECK(command_kind IN ('prepare','interrupt','mark_failed')),
 received_at_ms INTEGER NOT NULL,
 resulting_revision INTEGER NOT NULL,
 resulting_state TEXT NOT NULL,
 UNIQUE(job_id, resulting_revision)
);
CREATE INDEX IF NOT EXISTS agent_job_commands_job ON agent_job_commands(job_id, received_at_ms);
CREATE TABLE IF NOT EXISTS agent_session_prompts (
 command_id TEXT PRIMARY KEY NOT NULL,
 job_id TEXT NOT NULL REFERENCES agent_jobs(id),
 machine_ref TEXT NOT NULL,
 session_ref TEXT NOT NULL,
 prompt TEXT NOT NULL CHECK(length(prompt)>0 AND length(prompt)<=16384),
 state TEXT NOT NULL CHECK(state IN ('pending','dispatched','confirmed','uncertain')),
 created_at_ms INTEGER NOT NULL CHECK(created_at_ms>=0),
 updated_at_ms INTEGER NOT NULL CHECK(updated_at_ms>=0)
);
CREATE INDEX IF NOT EXISTS agent_session_prompts_machine ON agent_session_prompts(machine_ref,state,created_at_ms);
CREATE TABLE IF NOT EXISTS agent_job_events (
 job_id TEXT NOT NULL REFERENCES agent_jobs(id),
 revision INTEGER NOT NULL,
 state TEXT NOT NULL,
 recorded_at_ms INTEGER NOT NULL,
 reason TEXT NOT NULL,
 PRIMARY KEY(job_id, revision)
);
"#;

fn invalid(message: impl Into<String>) -> RepositoryError {
    RepositoryError::Other(message.into())
}
fn nonempty(s: &str) -> bool {
    !s.trim().is_empty()
}

/// Verify all direct prerequisites inside the caller's write transaction.
/// The standalone storage tests do not install the authority-only dependency
/// schema, so this check is applied when that schema is present. Connected
/// launch routing must still enforce the authenticated authority boundary.
fn ensure_prerequisites_complete(
    tx: &rusqlite::Transaction<'_>,
    task_id: &str,
) -> Result<(), RepositoryError> {
    let exists: i64 = tx
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='task_prerequisites'",
            [],
            |r| r.get(0),
        )
        .map_err(repo_error)?;
    if exists == 0 {
        return Ok(());
    }
    let unfinished: i64 = tx
        .query_row(
            "SELECT COUNT(*) FROM task_prerequisites p LEFT JOIN tasks t ON t.id=p.prerequisite_id \
         WHERE p.task_id=?1 AND (t.id IS NULL OR t.status != 'done')",
            [task_id],
            |r| r.get(0),
        )
        .map_err(repo_error)?;
    if unfinished != 0 {
        return Err(invalid("agent task has unfinished prerequisites"));
    }
    Ok(())
}

/// Normalized runtime evidence; do not treat a session status as task acceptance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeActivity {
    Working,
    WaitingInput,
    Blocked,
    Idle,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentRuntimeObservation {
    Present {
        session_ref: String,
        state: RuntimeActivity,
    },
    Unreachable,
    AbsentConfirmed,
}

/// One enrolled device as an agent executor, derived purely from persisted
/// records. A browser needs this to answer "who could claim this job?" without
/// guessing from the machine reference stored on a job.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentExecutorRecord {
    pub device_id: String,
    /// Enrolled and not revoked. A revoked device row is still reported so a
    /// stale `machine_ref` is visible instead of silently disappearing.
    pub active: bool,
    /// Most recent claim by that machine, regardless of which device owns the job.
    pub last_claim_at_ms: Option<i64>,
    /// Non-terminal jobs in flight on that machine.
    pub claimed_jobs: i64,
    /// `preparing` jobs on that machine that no executor has claimed yet.
    pub preparing_jobs: i64,
}

/// The authority's own provenance credential. It is seeded revoked so it can
/// never authenticate, and nobody enrolled it as an executor, so it is excluded
/// from the roster instead of appearing as a phantom inactive device.
const AUTHORITY_IDENTITY: &str = "orbit-server";

/// The `devices` table belongs to the authority schema, which standalone
/// storage tests do not install. A missing table means "no executor configured".
fn table_exists(conn: &rusqlite::Connection, table: &str) -> Result<bool, RepositoryError> {
    conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
        [table],
        |r| r.get::<_, i64>(0),
    )
    .map(|count| count != 0)
    .map_err(repo_error)
}

impl SqliteStore {
    /// Enqueue a prompt only for an already-confirmed active session. A repeated
    /// command ID must contain exactly the same immutable request body.
    pub fn queue_agent_session_prompt(
        &mut self,
        owner: &str,
        command_id: &str,
        job_id: &str,
        prompt: &str,
        at_ms: i64,
    ) -> Result<serde_json::Value, RepositoryError> {
        if command_id.is_empty()
            || command_id.len() > 256
            || prompt.trim().is_empty()
            || prompt.len() > 16384
            || at_ms < 0
        {
            return Err(invalid("invalid session prompt"));
        }
        let tx = self.connection.transaction().map_err(repo_error)?;
        let owned: i64 = tx
            .query_row(
                "SELECT count(*) FROM agent_job_owners WHERE job_id=?1 AND owner_device_id=?2",
                params![job_id, owner],
                |r| r.get(0),
            )
            .map_err(repo_error)?;
        if owned != 1 {
            return Err(invalid("not agent job owner"));
        }
        let (machine, session, state): (String, Option<String>, String) = tx
            .query_row(
                "SELECT machine_ref,runtime_session_ref,state FROM agent_jobs WHERE id=?1",
                [job_id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .map_err(repo_error)?;
        let session = session.ok_or_else(|| invalid("no confirmed agent session"))?;
        let previous:Option<(String,String,String,String)>=tx.query_row(
            "SELECT job_id,machine_ref,session_ref,prompt FROM agent_session_prompts WHERE command_id=?1",
            [command_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?)))
            .optional().map_err(repo_error)?;
        if let Some((pjob, pmachine, psession, ptext)) = previous {
            if (
                pjob.as_str(),
                pmachine.as_str(),
                psession.as_str(),
                ptext.as_str(),
            ) != (job_id, machine.as_str(), session.as_str(), prompt)
            {
                return Err(invalid("conflicting session prompt replay"));
            }
        } else {
            if !matches!(state.as_str(), "running" | "waiting_input" | "blocked") {
                return Err(invalid("session not confirmed active"));
            }
            tx.execute("INSERT INTO agent_session_prompts(command_id,job_id,machine_ref,session_ref,prompt,state,created_at_ms,updated_at_ms) VALUES(?1,?2,?3,?4,?5,'pending',?6,?6)",
                params![command_id,job_id,machine,session,prompt,at_ms]).map_err(repo_error)?;
        }
        tx.commit().map_err(repo_error)?;
        Ok(serde_json::json!({"command_id":command_id,"job_id":job_id,"queued":true}))
    }
    /// Return only queued prompts for the authenticated execution machine.
    pub fn agent_session_prompt_inbox(
        &self,
        machine: &str,
    ) -> Result<Vec<serde_json::Value>, RepositoryError> {
        let mut statement=self.connection.prepare("SELECT command_id,job_id,session_ref,prompt FROM agent_session_prompts WHERE machine_ref=?1 AND state='pending' ORDER BY created_at_ms,command_id LIMIT 100").map_err(repo_error)?;
        let rows=statement.query_map([machine],|r|Ok(serde_json::json!({"command_id":r.get::<_,String>(0)?,"job_id":r.get::<_,String>(1)?,"session_ref":r.get::<_,String>(2)?,"prompt":r.get::<_,String>(3)?}))).map_err(repo_error)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(repo_error)
    }
    /// Fence side effects before delivering a prompt. A lost acknowledgement
    /// leaves the command dispatched and requires explicit reconciliation.
    pub fn claim_agent_session_prompt(
        &mut self,
        machine: &str,
        command_id: &str,
        at_ms: i64,
    ) -> Result<bool, RepositoryError> {
        if at_ms < 0 {
            return Err(invalid("invalid timestamp"));
        }
        let updated=self.connection.execute("UPDATE agent_session_prompts SET state='dispatched',updated_at_ms=?3 WHERE command_id=?1 AND machine_ref=?2 AND state='pending' AND EXISTS (SELECT 1 FROM agent_jobs j WHERE j.id=agent_session_prompts.job_id AND j.runtime_session_ref=agent_session_prompts.session_ref AND j.state IN ('running','waiting_input','blocked'))",
            params![command_id,machine,at_ms]).map_err(repo_error)?;
        Ok(updated == 1)
    }
    pub fn report_agent_session_prompt(
        &mut self,
        machine: &str,
        command_id: &str,
        delivered: bool,
        at_ms: i64,
    ) -> Result<(), RepositoryError> {
        if at_ms < 0 {
            return Err(invalid("invalid timestamp"));
        }
        let state = if delivered { "confirmed" } else { "uncertain" };
        let updated=self.connection.execute("UPDATE agent_session_prompts SET state=?3,updated_at_ms=?4 WHERE command_id=?1 AND machine_ref=?2 AND state='dispatched'",
            params![command_id,machine,state,at_ms]).map_err(repo_error)?;
        if updated != 1 {
            let prior:Option<String>=self.connection.query_row(
                "SELECT state FROM agent_session_prompts WHERE command_id=?1 AND machine_ref=?2",
                params![command_id,machine],|r|r.get(0)).optional().map_err(repo_error)?;
            if prior.as_deref() != Some(state) {
                return Err(invalid("session prompt not dispatched by machine"));
            }
        }
        Ok(())
    }

    /// Idempotent creation; rejects an active reservation on another request.
    /// Store a durable implementation handoff. Handoffs are append-only and do
    /// not transition either the job or its task to completed status.
    pub fn record_agent_job_handoff(
        &mut self,
        job_id: &str,
        handoff_id: &str,
        payload: &serde_json::Value,
        at_ms: i64,
    ) -> Result<(), RepositoryError> {
        if job_id.trim().is_empty() || handoff_id.trim().is_empty() || at_ms < 0 {
            return Err(invalid("invalid handoff identity or timestamp"));
        }
        let object = payload
            .as_object()
            .ok_or_else(|| invalid("handoff must be a JSON object"))?;
        for field in ["summary", "validation", "next_steps"] {
            if !object.contains_key(field) {
                return Err(invalid(format!("handoff missing {field}")));
            }
        }
        let serialized = serde_json::to_string(payload).map_err(|e| invalid(e.to_string()))?;
        if serialized.len() > 256 * 1024 {
            return Err(invalid("handoff exceeds 256 KiB"));
        }
        let tx = self.connection.transaction().map_err(repo_error)?;
        let exists: Option<String> = tx
            .query_row("SELECT id FROM agent_jobs WHERE id=?1", [job_id], |r| {
                r.get(0)
            })
            .optional()
            .map_err(repo_error)?;
        if exists.is_none() {
            return Err(invalid("unknown agent job"));
        }
        let previous:Option<(String,i64)>=tx.query_row(
            "SELECT payload_json,reported_at_ms FROM agent_job_handoffs WHERE job_id=?1 AND handoff_id=?2",
            params![job_id,handoff_id],|r|Ok((r.get(0)?,r.get(1)?))
        ).optional().map_err(repo_error)?;
        if let Some((raw, timestamp)) = previous {
            if raw != serialized || timestamp != at_ms {
                return Err(invalid("handoff ID reused with different contents"));
            }
        } else {
            tx.execute("INSERT INTO agent_job_handoffs(job_id,handoff_id,reported_at_ms,payload_json) VALUES (?1,?2,?3,?4)",
                params![job_id,handoff_id,at_ms,serialized]).map_err(repo_error)?;
        }
        tx.commit().map_err(repo_error)
    }

    pub fn list_agent_job_handoffs(
        &self,
        job_id: &str,
    ) -> Result<Vec<serde_json::Value>, RepositoryError> {
        let mut statement=self.connection.prepare(
            "SELECT handoff_id,reported_at_ms,payload_json FROM agent_job_handoffs WHERE job_id=?1 ORDER BY reported_at_ms,handoff_id"
        ).map_err(repo_error)?;
        let rows = statement
            .query_map([job_id], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, String>(2)?,
                ))
            })
            .map_err(repo_error)?;
        let mut result = Vec::new();
        for row in rows {
            let (id, at_ms, raw) = row.map_err(repo_error)?;
            let payload: serde_json::Value =
                serde_json::from_str(&raw).map_err(|e| invalid(e.to_string()))?;
            result.push(
                serde_json::json!({"handoff_id":id,"reported_at_ms":at_ms,"payload":payload}),
            );
        }
        Ok(result)
    }

    /// Review requires an existing handoff and is immutable. Approval is never task completion.
    pub fn review_agent_job(
        &mut self,
        job_id: &str,
        handoff_id: &str,
        decision: &str,
        reviewer: &str,
        reason: &str,
        at_ms: i64,
    ) -> Result<serde_json::Value, RepositoryError> {
        if !matches!(decision, "approved" | "changes_requested")
            || reviewer.trim().is_empty()
            || handoff_id.trim().is_empty()
            || at_ms < 0
            || reason.len() > 8192
        {
            return Err(invalid("invalid review request"));
        }
        let tx = self.connection.transaction().map_err(repo_error)?;
        let exists: Option<i64> = tx
            .query_row(
                "SELECT 1 FROM agent_job_handoffs WHERE job_id=?1 AND handoff_id=?2",
                params![job_id, handoff_id],
                |r| r.get(0),
            )
            .optional()
            .map_err(repo_error)?;
        if exists.is_none() {
            return Err(invalid("review requires a persisted handoff"));
        }
        let previous:Option<(String,String,String,i64,String)>=tx.query_row(
            "SELECT decision,reviewer,reason,reviewed_at_ms,handoff_id FROM agent_job_reviews WHERE job_id=?1",
            [job_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?)))
            .optional().map_err(repo_error)?;
        if let Some((d, u, r, t, h)) = previous {
            if (d.as_str(), u.as_str(), r.as_str(), t, h.as_str())
                != (decision, reviewer, reason, at_ms, handoff_id)
            {
                return Err(invalid("review already recorded; conflicting decision"));
            }
        } else {
            tx.execute("INSERT INTO agent_job_reviews(job_id,handoff_id,decision,reviewer,reason,reviewed_at_ms) VALUES (?1,?2,?3,?4,?5,?6)",
                params![job_id,handoff_id,decision,reviewer,reason,at_ms]).map_err(repo_error)?;
        }
        tx.commit().map_err(repo_error)?;
        Ok(
            serde_json::json!({"job_id":job_id,"handoff_id":handoff_id,"decision":decision,
            "reviewer":reviewer,"reason":reason,"reviewed_at_ms":at_ms,"task_completed":false}),
        )
    }

    pub fn agent_job_review(
        &self,
        job_id: &str,
    ) -> Result<Option<serde_json::Value>, RepositoryError> {
        let result=self.connection.query_row(
            "SELECT handoff_id,decision,reviewer,reason,reviewed_at_ms FROM agent_job_reviews WHERE job_id=?1",
            [job_id],|r|Ok(serde_json::json!({"job_id":job_id,"handoff_id":r.get::<_,String>(0)?,
                "decision":r.get::<_,String>(1)?,"reviewer":r.get::<_,String>(2)?,
                "reason":r.get::<_,String>(3)?,"reviewed_at_ms":r.get::<_,i64>(4)?,"task_completed":false})))
            .optional().map_err(repo_error)?;
        Ok(result)
    }

    /// Execution policy is frozen before preparation; a conflicting replay is rejected.
    /// Existing v16 jobs default to an ordinary workspace (no worktree).
    pub fn set_agent_job_worktree(
        &mut self,
        id: &str,
        enabled: bool,
    ) -> Result<(), RepositoryError> {
        let tx = self.connection.transaction().map_err(repo_error)?;
        let state: Option<String> = tx
            .query_row("SELECT state FROM agent_jobs WHERE id=?1", [id], |r| {
                r.get(0)
            })
            .optional()
            .map_err(repo_error)?;
        if state.as_deref() != Some("queued") {
            return Err(invalid("execution options can only be set while queued"));
        }
        let existing: Option<i64> = tx
            .query_row(
                "SELECT worktree FROM agent_job_execution_options WHERE job_id=?1",
                [id],
                |r| r.get(0),
            )
            .optional()
            .map_err(repo_error)?;
        if let Some(value) = existing {
            if (value != 0) != enabled {
                return Err(invalid("conflicting worktree policy replay"));
            }
        } else {
            tx.execute(
                "INSERT INTO agent_job_execution_options(job_id,worktree) VALUES (?1,?2)",
                params![id, i64::from(enabled)],
            )
            .map_err(repo_error)?;
        }
        tx.commit().map_err(repo_error)
    }

    /// Read all dispatch inputs in one SQLite read transaction. The launch claim
    /// itself remains a separate atomic write that fences external side effects.
    pub fn agent_launch_context(
        &self,
        id: &str,
    ) -> Result<orbit_application::agent_runtime::AgentLaunchContext, RepositoryError> {
        use orbit_application::agent_runtime::AgentLaunchContext;
        let tx = self
            .connection
            .unchecked_transaction()
            .map_err(repo_error)?;
        let job = read_job(&tx, id)?.ok_or_else(|| invalid("claimed job missing"))?;
        let brief_json: String = tx
            .query_row(
                "SELECT brief_json FROM agent_job_briefs WHERE job_id=?1",
                [id],
                |r| r.get(0),
            )
            .map_err(repo_error)?;
        let enabled: Option<i64> = tx
            .query_row(
                "SELECT worktree FROM agent_job_execution_options WHERE job_id=?1",
                [id],
                |r| r.get(0),
            )
            .optional()
            .map_err(repo_error)?;
        tx.commit().map_err(repo_error)?;
        Ok(AgentLaunchContext {
            job,
            brief_json,
            worktree: enabled.unwrap_or(0) != 0,
        })
    }

    /// Read job and associated claim together. Never pair a stale claim with
    /// a newer job revision during monitoring.
    pub fn agent_monitoring_context(
        &self,
        id: &str,
    ) -> Result<orbit_application::agent_runtime::AgentMonitoringContext, RepositoryError> {
        use orbit_application::agent_runtime::AgentMonitoringContext;
        let tx = self
            .connection
            .unchecked_transaction()
            .map_err(repo_error)?;
        let job = read_job(&tx, id)?.ok_or_else(|| invalid("agent job not found"))?;
        let claim_token: Option<String> = tx
            .query_row(
                "SELECT claim_token FROM agent_job_launch_claims WHERE job_id=?1",
                [id],
                |r| r.get(0),
            )
            .optional()
            .map_err(repo_error)?;
        tx.commit().map_err(repo_error)?;
        Ok(AgentMonitoringContext { job, claim_token })
    }

    pub fn agent_job_worktree(&self, id: &str) -> Result<bool, RepositoryError> {
        let enabled: Option<i64> = self
            .connection
            .query_row(
                "SELECT worktree FROM agent_job_execution_options WHERE job_id=?1",
                [id],
                |r| r.get(0),
            )
            .optional()
            .map_err(repo_error)?;
        Ok(enabled.unwrap_or(0) != 0)
    }

    /// Whether an enrolled device exists and may still act as an executor.
    /// `None` means no such device; `Some(false)` means revoked. Start uses this
    /// before its job transaction so a bad `machine_ref` cannot create a
    /// `preparing` job that nothing will ever claim.
    pub fn agent_executor_active(&self, device: &str) -> Result<Option<bool>, RepositoryError> {
        if !nonempty(device) {
            return Ok(None);
        }
        if !table_exists(&self.connection, "devices")? {
            return Ok(None);
        }
        let revoked: Option<i64> = self
            .connection
            .query_row("SELECT revoked FROM devices WHERE id=?1", [device], |r| {
                r.get(0)
            })
            .optional()
            .map_err(repo_error)?;
        Ok(revoked.map(|value| value == 0))
    }

    /// Every enrolled device with its durable claim and job counters. This is
    /// three plain passes (devices, claims, jobs) merged in memory: one query per
    /// table, never one query per device. The deployment is single-user and
    /// self-hosted, so the device count is small and no pagination is offered.
    pub fn agent_executors(&self) -> Result<Vec<AgentExecutorRecord>, RepositoryError> {
        if !table_exists(&self.connection, "devices")? {
            return Ok(Vec::new());
        }
        let mut devices = self
            .connection
            .prepare("SELECT id,revoked FROM devices WHERE id<>?1 ORDER BY id")
            .map_err(repo_error)?;
        let devices = devices
            .query_map([AUTHORITY_IDENTITY], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
            })
            .map_err(repo_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(repo_error)?;

        let mut last_claim: std::collections::HashMap<String, i64> = Default::default();
        let mut claims = self
            .connection
            .prepare(
                "SELECT j.machine_ref,MAX(c.claimed_at_ms) FROM agent_job_launch_claims c \
                 JOIN agent_jobs j ON j.id=c.job_id GROUP BY j.machine_ref",
            )
            .map_err(repo_error)?;
        for row in claims
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))
            .map_err(repo_error)?
        {
            let (machine, at_ms) = row.map_err(repo_error)?;
            last_claim.insert(machine, at_ms);
        }

        let mut counts: std::collections::HashMap<String, (i64, i64)> = Default::default();
        let mut jobs = self
            .connection
            .prepare(
                "SELECT j.machine_ref, \
                 SUM(CASE WHEN j.state IN ('launching','running','waiting_input','blocked','unreachable') \
                     THEN 1 ELSE 0 END), \
                 SUM(CASE WHEN j.state='preparing' AND NOT EXISTS \
                     (SELECT 1 FROM agent_job_launch_claims c WHERE c.job_id=j.id) \
                     THEN 1 ELSE 0 END) \
                 FROM agent_jobs j GROUP BY j.machine_ref",
            )
            .map_err(repo_error)?;
        for row in jobs
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, i64>(2)?,
                ))
            })
            .map_err(repo_error)?
        {
            let (machine, claimed, preparing) = row.map_err(repo_error)?;
            counts.insert(machine, (claimed, preparing));
        }

        Ok(devices
            .into_iter()
            .map(|(device_id, revoked)| {
                let (claimed_jobs, preparing_jobs) =
                    counts.get(&device_id).copied().unwrap_or((0, 0));
                AgentExecutorRecord {
                    last_claim_at_ms: last_claim.get(&device_id).copied(),
                    active: revoked == 0,
                    claimed_jobs,
                    preparing_jobs,
                    device_id,
                }
            })
            .collect())
    }

    /// Durable launch-claim state for one job. `None` means no machine has
    /// claimed it, which is what lets a client say "waiting for an executor"
    /// instead of implying server-side progress.
    pub fn agent_job_claim_state(&self, id: &str) -> Result<Option<String>, RepositoryError> {
        self.connection
            .query_row(
                "SELECT dispatch_state FROM agent_job_launch_claims WHERE job_id=?1",
                [id],
                |r| r.get(0),
            )
            .optional()
            .map_err(repo_error)
    }

    /// Claim states for a bounded job list in one statement, so a job list never
    /// becomes one claim query per row.
    pub fn agent_claim_states(
        &self,
        ids: &[&str],
    ) -> Result<Vec<(String, String)>, RepositoryError> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let placeholders = vec!["?"; ids.len()].join(",");
        let mut statement = self
            .connection
            .prepare(&format!(
                "SELECT job_id,dispatch_state FROM agent_job_launch_claims WHERE job_id IN ({placeholders})"
            ))
            .map_err(repo_error)?;
        statement
            .query_map(rusqlite::params_from_iter(ids.iter()), |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })
            .map_err(repo_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(repo_error)
    }

    /// No subprocess is started here. A caller must persist this before dispatch.
    pub fn reserve_agent_job(&mut self, job: &AgentJob) -> Result<AgentJob, RepositoryError> {
        self.reserve_agent_job_inner(job, None)
    }

    fn reserve_agent_job_inner(
        &mut self,
        job: &AgentJob,
        owner: Option<&str>,
    ) -> Result<AgentJob, RepositoryError> {
        if job.state != AgentJobState::Queued
            || job.revision != 0
            || job.launch_revision < 0
            || ![
                &job.id,
                &job.request_id,
                &job.agent_kind,
                &job.machine_ref,
                &job.runtime_kind,
                &job.repository_ref,
            ]
            .iter()
            .all(|s| nonempty(s))
        {
            return Err(invalid("invalid initial agent job"));
        }
        let tx = self.connection.transaction().map_err(repo_error)?;
        let existing: Option<String> = tx
            .query_row(
                "SELECT id FROM agent_jobs WHERE request_id = ?1",
                [&job.request_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(repo_error)?;
        if let Some(id) = existing {
            if id != job.id {
                return Err(invalid("idempotency key already belongs to another job"));
            }
            let saved = read_job(&tx, &id)?.ok_or_else(|| invalid("reservation missing"))?;
            if saved.task_id != job.task_id
                || saved.project_id != job.project_id
                || saved.agent_kind != job.agent_kind
                || saved.machine_ref != job.machine_ref
                || saved.runtime_kind != job.runtime_kind
                || saved.repository_ref != job.repository_ref
                || saved.launch_revision != job.launch_revision
            {
                return Err(invalid(
                    "idempotency key reused with different launch payload",
                ));
            }
            if let Some(owner) = owner {
                let actual: Option<String> = tx
                    .query_row(
                        "SELECT owner_device_id FROM agent_job_owners WHERE job_id=?1",
                        [&id],
                        |r| r.get(0),
                    )
                    .optional()
                    .map_err(repo_error)?;
                if actual.as_deref() != Some(owner) {
                    return Err(invalid("reservation owner mismatch"));
                }
            }
            return Ok(saved);
        }
        // The authority checks the task within the same write transaction as the reservation.
        // A standalone, blocked or completed task cannot be dispatched.
        let task: Option<(Option<String>, String)> = tx
            .query_row(
                "SELECT project_id,status FROM tasks WHERE id=?1",
                [job.task_id.as_str()],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()
            .map_err(repo_error)?;
        let Some((project_id, status)) = task else {
            return Err(invalid("agent task not found"));
        };
        if project_id.as_deref() != Some(job.project_id.as_str()) {
            return Err(invalid("task project mismatch"));
        }
        if !matches!(status.as_str(), "ready" | "in_progress" | "review") {
            return Err(invalid("task must be ready, in progress, or in review"));
        }
        ensure_prerequisites_complete(&tx, job.task_id.as_str())?;
        let active: i64 = tx.query_row(
            "SELECT COUNT(*) FROM agent_jobs WHERE task_id = ?1 AND state NOT IN ('completed','failed','interrupted')",
            [job.task_id.as_str()], |r| r.get(0)).map_err(repo_error)?;
        if active > 0 {
            return Err(invalid("task has an active agent reservation"));
        }
        tx.execute("INSERT INTO agent_jobs (id,request_id,project_id,task_id,launch_revision,agent_kind,machine_ref,runtime_kind,repository_ref,state,revision,created_at_ms,updated_at_ms,last_seen_at_ms,runtime_session_ref) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,0,?11,?12,?13,?14)", params![job.id,job.request_id,job.project_id.as_str(),job.task_id.as_str(),job.launch_revision,job.agent_kind,job.machine_ref,job.runtime_kind,job.repository_ref,job.state.as_str(),job.created_at_ms,job.updated_at_ms,job.last_seen_at_ms,job.runtime_session_ref]).map_err(repo_error)?;
        // Store the authority-observed task brief in the reservation transaction.
        // The brief is immutable even when task metadata changes later.
        let title: String = tx
            .query_row(
                "SELECT title FROM tasks WHERE id=?1",
                [job.task_id.as_str()],
                |r| r.get(0),
            )
            .map_err(repo_error)?;
        let brief_json = serde_json::json!({"task_id":job.task_id.as_str(),"project_id":job.project_id.as_str(),"title":title,"status_at_reservation":status,"launch_revision":job.launch_revision}).to_string();
        tx.execute("INSERT INTO agent_job_briefs(job_id,task_title,task_status,brief_json,recorded_at_ms) VALUES (?1,?2,?3,?4,?5)",
            params![job.id,title,status,brief_json,job.created_at_ms]).map_err(repo_error)?;
        tx.execute("INSERT INTO agent_job_events(job_id,revision,state,recorded_at_ms,reason) VALUES (?1,0,'queued',?2,'reserved')", params![job.id,job.created_at_ms]).map_err(repo_error)?;
        if let Some(owner) = owner {
            tx.execute(
                "INSERT INTO agent_job_owners(job_id,owner_device_id) VALUES (?1,?2)",
                params![job.id, owner],
            )
            .map_err(repo_error)?;
        }
        tx.commit().map_err(repo_error)?;
        Ok(job.clone())
    }

    /// Reserve with ownership committed atomically with the job and brief.
    pub fn reserve_agent_job_for_device(
        &mut self,
        owner: &str,
        job: &AgentJob,
    ) -> Result<AgentJob, RepositoryError> {
        if !nonempty(owner) || owner.len() > 256 {
            return Err(invalid("invalid job owner"));
        }
        self.reserve_agent_job_inner(job, Some(owner))
    }

    /// A connected start is one durable authority transaction. A failure in
    /// worktree-policy or preparation rolls back the reservation as well.
    /// The machine worker only sees committed `preparing` jobs.
    ///
    /// The optional objective is frozen into the same brief as the task snapshot
    /// so a resumed job inherits it and `resume-context` can replay it verbatim.
    pub fn start_agent_job_for_device(
        &mut self,
        owner: &str,
        job: &AgentJob,
        worktree: bool,
        objective: Option<&str>,
    ) -> Result<AgentJob, RepositoryError> {
        if !nonempty(owner)
            || owner.len() > 256
            || job.state != AgentJobState::Queued
            || job.revision != 0
            || job.launch_revision < 0
            || job.created_at_ms < 0
            || ![
                &job.id,
                &job.request_id,
                &job.agent_kind,
                &job.machine_ref,
                &job.runtime_kind,
                &job.repository_ref,
            ]
            .iter()
            .all(|s| nonempty(s))
        {
            return Err(invalid("invalid connected agent start"));
        }
        let tx = self.connection.transaction().map_err(repo_error)?;
        if let Some(existing) = read_job(&tx, &job.id)? {
            let actual: Option<String> = tx
                .query_row(
                    "SELECT owner_device_id FROM agent_job_owners WHERE job_id=?1",
                    [&job.id],
                    |r| r.get(0),
                )
                .optional()
                .map_err(repo_error)?;
            let option: Option<i64> = tx
                .query_row(
                    "SELECT worktree FROM agent_job_execution_options WHERE job_id=?1",
                    [&job.id],
                    |r| r.get(0),
                )
                .optional()
                .map_err(repo_error)?;
            if actual.as_deref() != Some(owner)
                || existing.request_id != job.request_id
                || existing.task_id != job.task_id
                || existing.project_id != job.project_id
                || existing.machine_ref != job.machine_ref
                || existing.agent_kind != job.agent_kind
                || existing.runtime_kind != job.runtime_kind
                || existing.repository_ref != job.repository_ref
                || existing.launch_revision != job.launch_revision
                || option.map(|v| v != 0) != Some(worktree)
            {
                return Err(invalid("conflicting connected start replay"));
            }
            return Ok(existing);
        }
        let task: Option<(Option<String>, String, String)> = tx
            .query_row(
                "SELECT project_id,status,title FROM tasks WHERE id=?1",
                [job.task_id.as_str()],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()
            .map_err(repo_error)?;
        let Some((project, status, title)) = task else {
            return Err(invalid("agent task not found"));
        };
        if project.as_deref() != Some(job.project_id.as_str())
            || !matches!(status.as_str(), "ready" | "in_progress" | "review")
        {
            return Err(invalid("task not eligible for connected start"));
        }
        ensure_prerequisites_complete(&tx, job.task_id.as_str())?;
        let active:i64=tx.query_row("SELECT COUNT(*) FROM agent_jobs WHERE task_id=?1 AND state NOT IN ('completed','failed','interrupted')",[job.task_id.as_str()],|r|r.get(0)).map_err(repo_error)?;
        if active != 0 {
            return Err(invalid("task already has active agent"));
        }
        tx.execute("INSERT INTO agent_jobs (id,request_id,project_id,task_id,launch_revision,agent_kind,machine_ref,runtime_kind,repository_ref,state,revision,created_at_ms,updated_at_ms,last_seen_at_ms,runtime_session_ref) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,'preparing',1,?10,?10,NULL,NULL)",params![job.id,job.request_id,job.project_id.as_str(),job.task_id.as_str(),job.launch_revision,job.agent_kind,job.machine_ref,job.runtime_kind,job.repository_ref,job.created_at_ms]).map_err(repo_error)?;
        let mut brief = serde_json::json!({"task_id":job.task_id.as_str(),"project_id":job.project_id.as_str(),"title":title,"status_at_reservation":status,"launch_revision":job.launch_revision});
        // Additive: an absent or blank objective leaves the brief byte-identical
        // to the pre-objective shape, so existing callers keep working.
        if let Some(objective) = objective.map(str::trim).filter(|text| !text.is_empty()) {
            brief["objective"] = serde_json::json!(objective);
        }
        tx.execute("INSERT INTO agent_job_briefs(job_id,task_title,task_status,brief_json,recorded_at_ms) VALUES (?1,?2,?3,?4,?5)",params![job.id,title,status,brief.to_string(),job.created_at_ms]).map_err(repo_error)?;
        tx.execute(
            "INSERT INTO agent_job_owners(job_id,owner_device_id) VALUES (?1,?2)",
            params![job.id, owner],
        )
        .map_err(repo_error)?;
        tx.execute(
            "INSERT INTO agent_job_execution_options(job_id,worktree) VALUES (?1,?2)",
            params![job.id, i64::from(worktree)],
        )
        .map_err(repo_error)?;
        tx.execute("INSERT INTO agent_job_events(job_id,revision,state,recorded_at_ms,reason) VALUES (?1,0,'queued',?2,'reserved'),(?1,1,'preparing',?2,'accepted:prepare')",params![job.id,job.created_at_ms]).map_err(repo_error)?;
        tx.execute("INSERT INTO agent_job_commands(command_id,job_id,expected_revision,command_kind,received_at_ms,resulting_revision,resulting_state) VALUES (?1,?2,0,'prepare',?3,1,'preparing')",params![format!("{}:prepare",job.id),job.id,job.created_at_ms]).map_err(repo_error)?;
        tx.commit().map_err(repo_error)?;
        self.get_agent_job(&job.id)?
            .ok_or_else(|| invalid("started agent job missing"))
    }

    /// Resume is atomic: the successor, frozen handoff, worktree policy and
    /// machine inbox visibility commit together or not at all.
    pub fn resume_agent_job_for_device(
        &mut self,
        owner: &str,
        prior_id: &str,
        new_id: &str,
        at_ms: i64,
    ) -> Result<AgentJob, RepositoryError> {
        if !nonempty(owner)
            || !nonempty(prior_id)
            || !nonempty(new_id)
            || prior_id == new_id
            || at_ms < 0
        {
            return Err(invalid("invalid resume request"));
        }
        let tx = self.connection.transaction().map_err(repo_error)?;
        let prior = read_job(&tx, prior_id)?.ok_or_else(|| invalid("predecessor missing"))?;
        let actual: Option<String> = tx
            .query_row(
                "SELECT owner_device_id FROM agent_job_owners WHERE job_id=?1",
                [prior_id],
                |r| r.get(0),
            )
            .optional()
            .map_err(repo_error)?;
        if actual.as_deref() != Some(owner) {
            return Err(invalid("predecessor owner mismatch"));
        }
        if let Some(existing) = read_job(&tx, new_id)? {
            let new_owner: Option<String> = tx
                .query_row(
                    "SELECT owner_device_id FROM agent_job_owners WHERE job_id=?1",
                    [new_id],
                    |r| r.get(0),
                )
                .optional()
                .map_err(repo_error)?;
            let brief: String = tx
                .query_row(
                    "SELECT brief_json FROM agent_job_briefs WHERE job_id=?1",
                    [new_id],
                    |r| r.get(0),
                )
                .map_err(repo_error)?;
            let brief: serde_json::Value =
                serde_json::from_str(&brief).map_err(|e| invalid(e.to_string()))?;
            if new_owner.as_deref() != Some(owner)
                || existing.task_id != prior.task_id
                || brief
                    .pointer("/resume/prior_job_id")
                    .and_then(|v| v.as_str())
                    != Some(prior_id)
            {
                return Err(invalid("conflicting resume replay"));
            }
            return Ok(existing);
        }
        if !prior.state.is_terminal() {
            return Err(invalid("predecessor still active"));
        }
        // A claimed or ambiguous launch may already have created an external
        // session. A successor must not start until that claim is reconciled.
        let claim: Option<(String, Option<String>)> = tx
            .query_row(
                "SELECT dispatch_state,session_ref FROM agent_job_launch_claims WHERE job_id=?1",
                [prior_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()
            .map_err(repo_error)?;
        if let Some((dispatch_state, session_ref)) = claim
            && matches!(dispatch_state.as_str(), "claimed" | "ambiguous")
            && session_ref.is_none()
            && prior.runtime_session_ref.is_none()
        {
            return Err(invalid(
                "predecessor launch is unconfirmed; reconcile or sweep the runtime before resuming",
            ));
        }
        let latest:Option<(String,String)>=tx.query_row("SELECT handoff_id,payload_json FROM agent_job_handoffs WHERE job_id=?1 ORDER BY reported_at_ms DESC,handoff_id DESC LIMIT 1",[prior_id],|r|Ok((r.get(0)?,r.get(1)?))).optional().map_err(repo_error)?;
        let Some((handoff_id, payload)) = latest else {
            return Err(invalid("resume requires handoff"));
        };
        let old_brief: String = tx
            .query_row(
                "SELECT brief_json FROM agent_job_briefs WHERE job_id=?1",
                [prior_id],
                |r| r.get(0),
            )
            .map_err(repo_error)?;
        let mut brief: serde_json::Value =
            serde_json::from_str(&old_brief).map_err(|e| invalid(e.to_string()))?;
        let payload: serde_json::Value =
            serde_json::from_str(&payload).map_err(|e| invalid(e.to_string()))?;
        brief["launch_revision"] = serde_json::json!(prior.launch_revision + 1);
        brief["resume"] =
            serde_json::json!({"prior_job_id":prior_id,"handoff_id":handoff_id,"handoff":payload});
        let title: String = tx
            .query_row(
                "SELECT task_title FROM agent_job_briefs WHERE job_id=?1",
                [prior_id],
                |r| r.get(0),
            )
            .map_err(repo_error)?;
        let status: String = tx
            .query_row(
                "SELECT task_status FROM agent_job_briefs WHERE job_id=?1",
                [prior_id],
                |r| r.get(0),
            )
            .map_err(repo_error)?;
        let observed: Option<(String, String)> = tx
            .query_row(
                "SELECT title,status FROM tasks WHERE id=?1",
                [prior.task_id.as_str()],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()
            .map_err(repo_error)?;
        if observed != Some((title.clone(), status.clone())) {
            return Err(invalid("task context changed before resume"));
        }
        ensure_prerequisites_complete(&tx, prior.task_id.as_str())?;
        let active:i64=tx.query_row("SELECT COUNT(*) FROM agent_jobs WHERE task_id=?1 AND state NOT IN ('completed','failed','interrupted')",[prior.task_id.as_str()],|r|r.get(0)).map_err(repo_error)?;
        if active != 0 {
            return Err(invalid("task already has active agent"));
        }
        let worktree: Option<i64> = tx
            .query_row(
                "SELECT worktree FROM agent_job_execution_options WHERE job_id=?1",
                [prior_id],
                |r| r.get(0),
            )
            .optional()
            .map_err(repo_error)?;
        tx.execute("INSERT INTO agent_jobs (id,request_id,project_id,task_id,launch_revision,agent_kind,machine_ref,runtime_kind,repository_ref,state,revision,created_at_ms,updated_at_ms) VALUES (?1,?1,?2,?3,?4,?5,?6,?7,?8,'preparing',1,?9,?9)",params![new_id,prior.project_id.as_str(),prior.task_id.as_str(),prior.launch_revision+1,prior.agent_kind,prior.machine_ref,prior.runtime_kind,prior.repository_ref,at_ms]).map_err(repo_error)?;
        tx.execute(
            "INSERT INTO agent_job_owners(job_id,owner_device_id) VALUES (?1,?2)",
            params![new_id, owner],
        )
        .map_err(repo_error)?;
        tx.execute("INSERT INTO agent_job_briefs(job_id,task_title,task_status,brief_json,recorded_at_ms) VALUES (?1,?2,?3,?4,?5)",params![new_id,title,status,brief.to_string(),at_ms]).map_err(repo_error)?;
        tx.execute(
            "INSERT INTO agent_job_execution_options(job_id,worktree) VALUES (?1,?2)",
            params![new_id, worktree.unwrap_or(0)],
        )
        .map_err(repo_error)?;
        tx.execute("INSERT INTO agent_job_events(job_id,revision,state,recorded_at_ms,reason) VALUES (?1,0,'queued',?2,'reserved:resume'),(?1,1,'preparing',?2,'accepted:prepare')",params![new_id,at_ms]).map_err(repo_error)?;
        tx.execute("INSERT INTO agent_job_commands(command_id,job_id,expected_revision,command_kind,received_at_ms,resulting_revision,resulting_state) VALUES (?1,?2,0,'prepare',?3,1,'preparing')",params![format!("{new_id}:prepare"),new_id,at_ms]).map_err(repo_error)?;
        tx.commit().map_err(repo_error)?;
        self.get_agent_job(new_id)?
            .ok_or_else(|| invalid("resumed agent job missing"))
    }

    pub fn require_agent_job_owner(
        &self,
        owner: &str,
        job_id: &str,
    ) -> Result<(), RepositoryError> {
        let actual: Option<String> = self
            .connection
            .query_row(
                "SELECT owner_device_id FROM agent_job_owners WHERE job_id=?1",
                [job_id],
                |r| r.get(0),
            )
            .optional()
            .map_err(repo_error)?;
        if actual.as_deref() != Some(owner) {
            return Err(invalid("agent job not owned by this device"));
        }
        Ok(())
    }

    /// Immutable execution brief captured at reservation time, independent of later edits.
    /// Freeze the predecessor handoff into a new queued job's execution brief.
    /// This must happen before preparation, so the prompt is crash-stable.
    pub fn attach_agent_resume_context(
        &mut self,
        new_id: &str,
        prior_id: &str,
    ) -> Result<(), RepositoryError> {
        let tx = self.connection.transaction().map_err(repo_error)?;
        let new_job = read_job(&tx, new_id)?.ok_or_else(|| invalid("resume job missing"))?;
        let prior_job =
            read_job(&tx, prior_id)?.ok_or_else(|| invalid("predecessor job missing"))?;
        if new_id == prior_id
            || new_job.task_id != prior_job.task_id
            || new_job.project_id != prior_job.project_id
        {
            return Err(invalid(
                "resume must target the predecessor's task and project",
            ));
        }
        if new_job.state != AgentJobState::Queued || !prior_job.state.is_terminal() {
            return Err(invalid(
                "resume requires a queued successor and terminal predecessor",
            ));
        }
        let latest:Option<(String,String)>=tx.query_row(
            "SELECT handoff_id,payload_json FROM agent_job_handoffs WHERE job_id=?1 ORDER BY reported_at_ms DESC,handoff_id DESC LIMIT 1",
            [prior_id],|r|Ok((r.get(0)?,r.get(1)?))).optional().map_err(repo_error)?;
        let Some((handoff_id, payload)) = latest else {
            return Err(invalid("resume requires a durable handoff"));
        };
        let current: String = tx
            .query_row(
                "SELECT brief_json FROM agent_job_briefs WHERE job_id=?1",
                [new_id],
                |r| r.get(0),
            )
            .map_err(repo_error)?;
        let mut brief: serde_json::Value =
            serde_json::from_str(&current).map_err(|e| invalid(e.to_string()))?;
        if let Some(existing) = brief.get("resume") {
            if existing.get("prior_job_id").and_then(|v| v.as_str()) == Some(prior_id)
                && existing.get("handoff_id").and_then(|v| v.as_str()) == Some(handoff_id.as_str())
            {
                return Ok(());
            }
            return Err(invalid("conflicting resume context"));
        }
        brief["resume"] = serde_json::json!({"prior_job_id":prior_id,"handoff_id":handoff_id,
            "handoff":serde_json::from_str::<serde_json::Value>(&payload).map_err(|e|invalid(e.to_string()))?});
        tx.execute(
            "UPDATE agent_job_briefs SET brief_json=?1 WHERE job_id=?2",
            params![brief.to_string(), new_id],
        )
        .map_err(repo_error)?;
        tx.commit().map_err(repo_error)
    }

    pub fn agent_job_brief(&self, id: &str) -> Result<Option<String>, RepositoryError> {
        self.connection
            .query_row(
                "SELECT brief_json FROM agent_job_briefs WHERE job_id=?1",
                [id],
                |r| r.get(0),
            )
            .optional()
            .map_err(repo_error)
    }

    /// A launch is forbidden if the authority-observed task has changed since
    /// reservation. This guards the exact snapshot fields available in v15;
    /// it is not a substitute for future semantic task revisions.
    pub fn verify_agent_job_brief_current(&self, id: &str) -> Result<(), RepositoryError> {
        let stored: Option<(String, String, String)> = self.connection.query_row(
            "SELECT b.task_title,b.task_status,j.task_id FROM agent_job_briefs b JOIN agent_jobs j ON j.id=b.job_id WHERE j.id=?1",
            [id], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?))
        ).optional().map_err(repo_error)?;
        let Some((title, status, task_id)) = stored else {
            return Err(invalid("missing job execution brief"));
        };
        let observed: Option<(String, String)> = self
            .connection
            .query_row(
                "SELECT title,status FROM tasks WHERE id=?1",
                [task_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()
            .map_err(repo_error)?;
        if observed != Some((title, status)) {
            return Err(invalid("task changed since agent reservation"));
        }
        Ok(())
    }

    /// An immutable command receipt is distinguishable from the live job projection.
    pub fn agent_job_command_receipt(
        &self,
        command_id: &str,
    ) -> Result<Option<(String, i64, String)>, RepositoryError> {
        self.connection.query_row(
            "SELECT job_id,resulting_revision,resulting_state FROM agent_job_commands WHERE command_id=?1",
            [command_id], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?))
        ).optional().map_err(repo_error)
    }

    /// Authority-local accepted command receipt. Clients must wait for this
    /// result before treating a queued/offline command as committed.
    /// Repeated command IDs return the original outcome, without reapplying it.
    pub fn accept_agent_job_command(
        &mut self,
        command_id: &str,
        id: &str,
        expected_revision: i64,
        kind: &str,
        at_ms: i64,
    ) -> Result<AgentJob, RepositoryError> {
        if !nonempty(command_id) || expected_revision < 0 {
            return Err(invalid("invalid agent command"));
        }
        let next = match kind {
            "prepare" => AgentJobState::Preparing,
            "interrupt" => AgentJobState::Interrupted,
            "mark_failed" => AgentJobState::Failed,
            _ => return Err(invalid("unsupported agent command")),
        };
        let tx = self.connection.transaction().map_err(repo_error)?;
        let existing: Option<(String,i64,String,i64,String)> = tx.query_row(
            "SELECT job_id,expected_revision,command_kind,resulting_revision,resulting_state FROM agent_job_commands WHERE command_id=?1",
            [command_id], |r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?)))
            .optional().map_err(repo_error)?;
        if let Some((original_id, original_revision, original_kind, _, _)) = existing {
            if original_id != id || original_revision != expected_revision || original_kind != kind
            {
                return Err(invalid("command identity reused with different arguments"));
            }
            // If the job has subsequently advanced, return the current projection;
            // the receipt remains the durable record of the original acceptance.
            return read_job(&tx, id)?.ok_or_else(|| invalid("missing accepted command job"));
        }
        let job = read_job(&tx, id)?.ok_or_else(|| invalid("unknown agent job"))?;
        if kind == "prepare" {
            // Snapshot verification is performed within the same write transaction.
            let observed: Option<(String, String)> = tx
                .query_row(
                    "SELECT title,status FROM tasks WHERE id=?1",
                    [job.task_id.as_str()],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .optional()
                .map_err(repo_error)?;
            let captured: Option<(String, String)> = tx
                .query_row(
                    "SELECT task_title,task_status FROM agent_job_briefs WHERE job_id=?1",
                    [id],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .optional()
                .map_err(repo_error)?;
            if observed.is_none() || observed != captured {
                return Err(invalid("task context changed before prepare"));
            }
            ensure_prerequisites_complete(&tx, job.task_id.as_str())?;
        }
        if job.revision != expected_revision {
            return Err(invalid("stale agent command revision"));
        }
        if !job.state.can_transition(next) || job.state == next {
            return Err(invalid("invalid agent command transition"));
        }
        if at_ms < job.updated_at_ms {
            return Err(invalid("command timestamp regression"));
        }
        tx.execute("UPDATE agent_jobs SET state=?1,revision=revision+1,updated_at_ms=?2 WHERE id=?3 AND revision=?4",
            params![next.as_str(),at_ms,id,expected_revision]).map_err(repo_error)?;
        tx.execute("INSERT INTO agent_job_events(job_id,revision,state,recorded_at_ms,reason) VALUES (?1,?2,?3,?4,?5)",
            params![id,expected_revision+1,next.as_str(),at_ms,format!("accepted:{kind}")]).map_err(repo_error)?;
        tx.execute("INSERT INTO agent_job_commands(command_id,job_id,expected_revision,command_kind,received_at_ms,resulting_revision,resulting_state) VALUES (?1,?2,?3,?4,?5,?6,?7)",
            params![command_id,id,expected_revision,kind,at_ms,expected_revision+1,next.as_str()]).map_err(repo_error)?;
        tx.commit().map_err(repo_error)?;
        self.get_agent_job(id)?
            .ok_or_else(|| invalid("missing agent after accepted command"))
    }

    /// A machine device sees only jobs explicitly targeting its authenticated ID.
    /// Claiming is handled by claim_agent_launch, which owns the write transaction.
    pub fn pending_agent_jobs_for_machine(
        &self,
        machine: &str,
    ) -> Result<Vec<AgentJob>, RepositoryError> {
        if !nonempty(machine) {
            return Err(invalid("empty machine identity"));
        }
        let mut statement = self
            .connection
            .prepare(
                "SELECT id FROM agent_jobs WHERE machine_ref=?1 AND state='preparing' \
             AND NOT EXISTS (SELECT 1 FROM agent_job_launch_claims c WHERE c.job_id=agent_jobs.id) \
             ORDER BY created_at_ms,id LIMIT 100",
            )
            .map_err(repo_error)?;
        let ids = statement
            .query_map([machine], |row| row.get::<_, String>(0))
            .map_err(repo_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(repo_error)?;
        ids.into_iter()
            .map(|id| {
                self.get_agent_job(&id)?
                    .ok_or_else(|| invalid("missing inbox job"))
            })
            .collect()
    }

    /// Bind a claim to the target machine inside the same write transaction.
    pub fn claim_agent_launch_for_machine(
        &mut self,
        machine: &str,
        id: &str,
        revision: i64,
        token: &str,
        at_ms: i64,
    ) -> Result<bool, RepositoryError> {
        if !nonempty(machine) {
            return Err(invalid("empty executor identity"));
        }
        let job = self
            .get_agent_job(id)?
            .ok_or_else(|| invalid("unknown agent job"))?;
        if job.machine_ref != machine {
            return Err(invalid("executor machine mismatch"));
        }
        self.claim_agent_launch(id, revision, token, at_ms)
    }

    /// Persist a launch result only from the device assigned to the job.
    pub fn record_agent_launch_for_machine(
        &mut self,
        machine: &str,
        id: &str,
        token: &str,
        session: Option<&str>,
        at_ms: i64,
    ) -> Result<(), RepositoryError> {
        let job = self
            .get_agent_job(id)?
            .ok_or_else(|| invalid("unknown agent job"))?;
        if job.machine_ref != machine {
            return Err(invalid("executor machine mismatch"));
        }
        self.record_launch_result(id, token, session, at_ms)
    }

    pub fn observe_agent_for_machine(
        &mut self,
        machine: &str,
        id: &str,
        revision: i64,
        token: &str,
        observation: AgentRuntimeObservation,
        at_ms: i64,
    ) -> Result<AgentJob, RepositoryError> {
        let job = self
            .get_agent_job(id)?
            .ok_or_else(|| invalid("unknown agent job"))?;
        if job.machine_ref != machine {
            return Err(invalid("executor machine mismatch"));
        }
        self.observe_agent_session(id, revision, token, observation, at_ms)
    }

    /// Atomically claim a prepared job for an external side effect. A retry with
    /// the same token is safe, but a different token never dispatches again.
    /// An uncertain Herdr response must be reconciled, not blindly retried.
    pub fn claim_agent_launch(
        &mut self,
        id: &str,
        expected_revision: i64,
        token: &str,
        at_ms: i64,
    ) -> Result<bool, RepositoryError> {
        if !nonempty(token) {
            return Err(invalid("empty launch token"));
        }
        let tx = self.connection.transaction().map_err(repo_error)?;
        let job = read_job(&tx, id)?.ok_or_else(|| invalid("unknown agent job"))?;
        let current: Option<String> = tx
            .query_row(
                "SELECT claim_token FROM agent_job_launch_claims WHERE job_id=?1",
                [id],
                |r| r.get(0),
            )
            .optional()
            .map_err(repo_error)?;
        if let Some(prior) = current {
            if prior == token {
                return Ok(false);
            }
            return Err(invalid("job already has a different launch claim"));
        }
        let observed: Option<(String, String)> = tx
            .query_row(
                "SELECT title,status FROM tasks WHERE id=?1",
                [job.task_id.as_str()],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()
            .map_err(repo_error)?;
        let captured: Option<(String, String)> = tx
            .query_row(
                "SELECT task_title,task_status FROM agent_job_briefs WHERE job_id=?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()
            .map_err(repo_error)?;
        if observed.is_none() || observed != captured {
            return Err(invalid("task context changed before launch"));
        }
        ensure_prerequisites_complete(&tx, job.task_id.as_str())?;
        if job.revision != expected_revision
            || job.state != AgentJobState::Preparing
            || at_ms < job.updated_at_ms
        {
            return Err(invalid("launch precondition failed"));
        }
        tx.execute("INSERT INTO agent_job_launch_claims(job_id,claim_token,claimed_at_ms,dispatch_state) VALUES (?1,?2,?3,'claimed')",
            params![id,token,at_ms]).map_err(repo_error)?;
        tx.execute("UPDATE agent_jobs SET state='launching',revision=revision+1,updated_at_ms=?2 WHERE id=?1",
            params![id,at_ms]).map_err(repo_error)?;
        tx.execute("INSERT INTO agent_job_events(job_id,revision,state,recorded_at_ms,reason) VALUES (?1,?2,'launching',?3,'launch_claimed')",
            params![id,expected_revision+1,at_ms]).map_err(repo_error)?;
        tx.commit().map_err(repo_error)?;
        Ok(true)
    }

    /// Persist a confirmed session or an ambiguous launch response. Neither
    /// observation implies task completion or authorizes another launch.
    pub fn record_launch_result(
        &mut self,
        id: &str,
        token: &str,
        session_ref: Option<&str>,
        at_ms: i64,
    ) -> Result<(), RepositoryError> {
        if let Some(s) = session_ref
            && !nonempty(s)
        {
            return Err(invalid("empty session reference"));
        }
        let tx = self.connection.transaction().map_err(repo_error)?;
        let job = read_job(&tx, id)?.ok_or_else(|| invalid("unknown agent job"))?;
        if job.state != AgentJobState::Launching || at_ms < job.updated_at_ms {
            return Err(invalid("launch result not expected"));
        }
        let n = tx.execute("UPDATE agent_job_launch_claims SET dispatch_state=?3,session_ref=?4 WHERE job_id=?1 AND claim_token=?2 AND dispatch_state IN ('claimed','ambiguous')",
            params![id,token,if session_ref.is_some(){"confirmed"}else{"ambiguous"},session_ref]).map_err(repo_error)?;
        if n != 1 {
            return Err(invalid("launch token mismatch or already confirmed"));
        }
        if let Some(session) = session_ref {
            tx.execute(
                "UPDATE agent_jobs SET runtime_session_ref=?2,last_seen_at_ms=?3 WHERE id=?1",
                params![id, session, at_ms],
            )
            .map_err(repo_error)?;
        }
        tx.commit().map_err(repo_error)
    }

    /// Read the durable launch attempt before attempting runtime reconciliation.
    /// A claimed or ambiguous attempt must NEVER be dispatched a second time.
    pub fn agent_launch_claim(
        &self,
        id: &str,
    ) -> Result<Option<(String, String, Option<String>)>, RepositoryError> {
        self.connection.query_row(
            "SELECT claim_token,dispatch_state,session_ref FROM agent_job_launch_claims WHERE job_id=?1",
            [id], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?))
        ).optional().map_err(repo_error)
    }

    /// Persist a runtime observation using compare-and-swap against the authority
    /// revision. An unreachable machine is not proof a session is gone.
    pub fn observe_agent_session(
        &mut self,
        id: &str,
        expected_revision: i64,
        token: &str,
        observation: AgentRuntimeObservation,
        at_ms: i64,
    ) -> Result<AgentJob, RepositoryError> {
        if !nonempty(token) {
            return Err(invalid("empty reconciliation token"));
        }
        let tx = self.connection.transaction().map_err(repo_error)?;
        let job = read_job(&tx, id)?.ok_or_else(|| invalid("unknown agent job"))?;
        let claim: Option<(String,String,Option<String>)> = tx.query_row(
            "SELECT claim_token,dispatch_state,session_ref FROM agent_job_launch_claims WHERE job_id=?1",
            [id], |r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))
        ).optional().map_err(repo_error)?;
        let Some((saved_token, dispatch, saved_session)) = claim else {
            return Err(invalid("missing launch claim"));
        };
        if saved_token != token {
            return Err(invalid("reconciliation token mismatch"));
        }
        if job.revision != expected_revision {
            return Err(invalid("stale reconciliation revision"));
        }
        if job.state.is_terminal() {
            return Err(invalid("terminal agent cannot be reconciled"));
        }
        if at_ms < job.updated_at_ms || job.last_seen_at_ms.is_some_and(|last| at_ms < last) {
            return Err(invalid("stale runtime observation"));
        }
        let (next, reason, session) = match observation {
            AgentRuntimeObservation::Present { session_ref, state } => {
                if !nonempty(&session_ref) {
                    return Err(invalid("empty runtime session"));
                }
                if saved_session
                    .as_deref()
                    .is_some_and(|saved| saved != session_ref.as_str())
                {
                    return Err(invalid("runtime session identity changed"));
                }
                let next = match state {
                    RuntimeActivity::Working => AgentJobState::Running,
                    RuntimeActivity::WaitingInput => AgentJobState::WaitingInput,
                    RuntimeActivity::Blocked => AgentJobState::Blocked,
                    // Agent idleness or process exit is not task acceptance. Keep
                    // it in waiting-input until a separate review closes the job.
                    RuntimeActivity::Idle => AgentJobState::WaitingInput,
                };
                (next, "runtime_present", Some(session_ref))
            }
            AgentRuntimeObservation::Unreachable => {
                (AgentJobState::Unreachable, "runtime_unreachable", None)
            }
            AgentRuntimeObservation::AbsentConfirmed => {
                if dispatch == "claimed" || dispatch == "ambiguous" {
                    // A lookup that finds no session is not proof the launch
                    // never happened. Hold ownership for operator reconciliation.
                    (
                        AgentJobState::Unreachable,
                        "runtime_absent_unconfirmed",
                        None,
                    )
                } else {
                    (AgentJobState::Interrupted, "runtime_session_absent", None)
                }
            }
        };
        if !job.state.can_transition(next) {
            return Err(invalid("invalid runtime observation transition"));
        }
        if let Some(ref session_ref) = session {
            tx.execute("UPDATE agent_job_launch_claims SET dispatch_state='confirmed',session_ref=?2 WHERE job_id=?1",
                params![id,session_ref]).map_err(repo_error)?;
        }
        let changed = next != job.state;
        if changed {
            tx.execute("UPDATE agent_jobs SET state=?2,revision=revision+1,updated_at_ms=?3 WHERE id=?1 AND revision=?4",
                params![id,next.as_str(),at_ms,expected_revision]).map_err(repo_error)?;
            tx.execute("INSERT INTO agent_job_events(job_id,revision,state,recorded_at_ms,reason) VALUES (?1,?2,?3,?4,?5)",
                params![id,expected_revision+1,next.as_str(),at_ms,reason]).map_err(repo_error)?;
        }
        if let Some(ref session_ref) = session {
            tx.execute(
                "UPDATE agent_jobs SET runtime_session_ref=?2,last_seen_at_ms=?3 WHERE id=?1",
                params![id, session_ref, at_ms],
            )
            .map_err(repo_error)?;
        }
        tx.commit().map_err(repo_error)?;
        self.get_agent_job(id)?
            .ok_or_else(|| invalid("missing reconciled job"))
    }

    /// Deterministic cross-project catch-up, derived entirely from persisted Orbit state.
    /// No remote runtime probing and no LLM calls occur in this read path.
    pub fn agent_job_overview(&self) -> Result<Vec<serde_json::Value>, RepositoryError> {
        let mut stmt = self
            .connection
            .prepare(
                "SELECT j.id,j.project_id,j.task_id,j.state,j.updated_at_ms, \
             (SELECT COUNT(*) FROM agent_job_handoffs h WHERE h.job_id=j.id), \
             (SELECT decision FROM agent_job_reviews r WHERE r.job_id=j.id), \
             (SELECT dispatch_state FROM agent_job_launch_claims c WHERE c.job_id=j.id) \
             FROM agent_jobs j ORDER BY j.updated_at_ms DESC,j.id DESC",
            )
            .map_err(repo_error)?;
        let rows = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, i64>(4)?,
                    r.get::<_, i64>(5)?,
                    r.get::<_, Option<String>>(6)?,
                    r.get::<_, Option<String>>(7)?,
                ))
            })
            .map_err(repo_error)?;
        let mut items = Vec::new();
        for row in rows {
            let (id, project_id, task_id, state, updated_at_ms, handoff_count, review, claim) =
                row.map_err(repo_error)?;
            let needs_review = handoff_count > 0 && review.is_none();
            let needs_attention = matches!(
                state.as_str(),
                "waiting_input" | "blocked" | "unreachable" | "launching"
            ) || needs_review
                || review.as_deref() == Some("changes_requested");
            let next_action = if review.as_deref() == Some("changes_requested") {
                "resume_or_rework"
            } else if needs_review {
                "review_handoff"
            } else {
                match state.as_str() {
                    // A job nobody claimed is waiting for its executor. That is
                    // not a person's problem yet, so it must not be reported as
                    // attention, but "none / nothing to do" would be a lie.
                    "queued" | "preparing" if claim.is_none() => "await_executor",
                    "waiting_input" | "blocked" => "respond_to_agent",
                    "unreachable" | "launching" => "inspect_runtime",
                    // Completion without a handoff is allowed, but it is not
                    // acceptance-ready evidence by itself.
                    "completed" if handoff_count == 0 => "inspect_missing_handoff",
                    "completed" => "check_task_acceptance",
                    "failed" | "interrupted" => "inspect_and_resume",
                    _ => "none",
                }
            };
            items.push(serde_json::json!({"job_id":id,"project_id":project_id,"task_id":task_id,
                "state":state,"updated_at_ms":updated_at_ms,"handoff_count":handoff_count,
                "review_decision":review,"needs_attention":needs_attention,"next_action":next_action}));
        }
        Ok(items)
    }

    /// Jobs needing an operator or polling worker, stable order for catch-up.
    pub fn list_agent_jobs_needing_attention(&self) -> Result<Vec<AgentJob>, RepositoryError> {
        let mut stmt = self.connection.prepare(
            "SELECT j.id FROM agent_jobs j WHERE j.state IN ('waiting_input','blocked','unreachable','launching') OR EXISTS (SELECT 1 FROM agent_job_handoffs h WHERE h.job_id=j.id AND NOT EXISTS (SELECT 1 FROM agent_job_reviews r WHERE r.job_id=j.id)) OR EXISTS (SELECT 1 FROM agent_job_reviews r WHERE r.job_id=j.id AND r.decision='changes_requested') ORDER BY j.updated_at_ms,j.id"
        ).map_err(repo_error)?;
        let ids = stmt
            .query_map([], |r| r.get::<_, String>(0))
            .map_err(repo_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(repo_error)?;
        ids.iter()
            .map(|id| {
                self.get_agent_job(id)?
                    .ok_or_else(|| invalid("missing attention job"))
            })
            .collect()
    }

    pub fn list_agent_jobs_for_project(
        &self,
        project: &ProjectId,
    ) -> Result<Vec<AgentJob>, RepositoryError> {
        let mut stmt = self
            .connection
            .prepare(
                "SELECT id FROM agent_jobs WHERE project_id=?1 ORDER BY created_at_ms DESC,id DESC",
            )
            .map_err(repo_error)?;
        let ids = stmt
            .query_map([project.as_str()], |r| r.get::<_, String>(0))
            .map_err(repo_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(repo_error)?;
        ids.iter()
            .map(|id| {
                self.get_agent_job(id)?
                    .ok_or_else(|| invalid("missing project job"))
            })
            .collect()
    }

    pub fn get_agent_job(&self, id: &str) -> Result<Option<AgentJob>, RepositoryError> {
        read_job(&self.connection, id)
    }

    /// Poll candidates with a durable session claim; never dispatch from this query.
    pub fn agent_job_poll_candidates(&self) -> Result<Vec<AgentJob>, RepositoryError> {
        let mut statement=self.connection.prepare(
            "SELECT j.id FROM agent_jobs j INNER JOIN agent_job_launch_claims c ON c.job_id=j.id \
             WHERE j.state IN ('launching','running','waiting_input','blocked','unreachable') \
             ORDER BY j.updated_at_ms,j.id"
        ).map_err(repo_error)?;
        let ids = statement
            .query_map([], |r| r.get::<_, String>(0))
            .map_err(repo_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(repo_error)?;
        ids.iter()
            .map(|id| {
                self.get_agent_job(id)?
                    .ok_or_else(|| invalid("missing polling job"))
            })
            .collect()
    }

    /// Compare-and-swap transition; terminal job outcomes never modify Orbit task status.
    pub fn transition_agent_job(
        &mut self,
        id: &str,
        expected_revision: i64,
        next: AgentJobState,
        at_ms: i64,
        reason: &str,
    ) -> Result<AgentJob, RepositoryError> {
        if !nonempty(reason) {
            return Err(invalid("job event reason required"));
        }
        let tx = self.connection.transaction().map_err(repo_error)?;
        let job = read_job(&tx, id)?.ok_or_else(|| invalid("unknown agent job"))?;
        if job.revision != expected_revision {
            return Err(invalid("stale agent job revision"));
        }
        if !job.state.can_transition(next) {
            return Err(invalid("invalid agent job lifecycle transition"));
        }
        if at_ms < job.updated_at_ms {
            return Err(invalid("job timestamp regression"));
        }
        if next == job.state {
            return Ok(job);
        }
        let updated = tx.execute("UPDATE agent_jobs SET state=?1, revision=revision+1, updated_at_ms=?2 WHERE id=?3 AND revision=?4", params![next.as_str(),at_ms,id,expected_revision]).map_err(repo_error)?;
        if updated != 1 {
            return Err(invalid("concurrent agent job modification"));
        }
        tx.execute("INSERT INTO agent_job_events(job_id,revision,state,recorded_at_ms,reason) VALUES (?1,?2,?3,?4,?5)", params![id,expected_revision+1,next.as_str(),at_ms,reason]).map_err(repo_error)?;
        tx.commit().map_err(repo_error)?;
        self.get_agent_job(id)?
            .ok_or_else(|| invalid("job missing after transition"))
    }

    /// Explicit operator completion of a non-terminal local job. Completion is
    /// terminal for the job only: durable handoffs, the recorded review and the
    /// Orbit task are untouched, so task acceptance stays a separate decision.
    /// A terminal job is refused instead of silently replayed; use
    /// `resume_agent_job_for_device` (or the local resume path) for a successor.
    /// Completion is recorded in the append-only job event timeline, which is
    /// the same activity trail every other lifecycle transition uses.
    pub fn complete_agent_job(
        &mut self,
        job_id: &str,
        completed_at_ms: i64,
        summary: &str,
    ) -> Result<AgentJob, RepositoryError> {
        if !nonempty(job_id) || completed_at_ms < 0 || summary.len() > 4096 {
            return Err(invalid("invalid agent completion request"));
        }
        let summary = summary.trim();
        let reason = if summary.is_empty() {
            "completed:operator".to_owned()
        } else {
            format!("completed:{summary}")
        };
        let tx = self.connection.transaction().map_err(repo_error)?;
        let job = read_job(&tx, job_id)?.ok_or_else(|| invalid("unknown agent job"))?;
        if job.state.is_terminal() {
            return Err(invalid(format!(
                "agent job is already terminal ({}); resume it to create a successor",
                job.state.as_str()
            )));
        }
        // Queued/preparing/launching are not completable: an agent that never
        // reached a running observation has no completion evidence to record.
        if !job.state.can_transition(AgentJobState::Completed) {
            return Err(invalid(format!(
                "agent job cannot be completed from state {}; launch and inspect it first",
                job.state.as_str()
            )));
        }
        if completed_at_ms < job.updated_at_ms {
            return Err(invalid("agent completion timestamp regression"));
        }
        let updated = tx
            .execute(
                "UPDATE agent_jobs SET state='completed',revision=revision+1,updated_at_ms=?2 WHERE id=?1 AND revision=?3",
                params![job_id, completed_at_ms, job.revision],
            )
            .map_err(repo_error)?;
        if updated != 1 {
            return Err(invalid("concurrent agent job modification"));
        }
        tx.execute(
            "INSERT INTO agent_job_events(job_id,revision,state,recorded_at_ms,reason) VALUES (?1,?2,'completed',?3,?4)",
            params![job_id, job.revision + 1, completed_at_ms, reason],
        )
        .map_err(repo_error)?;
        tx.commit().map_err(repo_error)?;
        self.get_agent_job(job_id)?
            .ok_or_else(|| invalid("job missing after completion"))
    }
}

struct AgentJobRow {
    id: String,
    request_id: String,
    project_id: String,
    task_id: String,
    launch_revision: i64,
    agent_kind: String,
    machine_ref: String,
    runtime_kind: String,
    repository_ref: String,
    state: String,
    revision: i64,
    created_at_ms: i64,
    updated_at_ms: i64,
    last_seen_at_ms: Option<i64>,
    runtime_session_ref: Option<String>,
}

fn read_job(conn: &rusqlite::Connection, id: &str) -> Result<Option<AgentJob>, RepositoryError> {
    let raw: Option<AgentJobRow> = conn.query_row(
        "SELECT id,request_id,project_id,task_id,launch_revision,agent_kind,machine_ref,runtime_kind,repository_ref,state,revision,created_at_ms,updated_at_ms,last_seen_at_ms,runtime_session_ref FROM agent_jobs WHERE id=?1",
        [id], |r| Ok(AgentJobRow {
            id: r.get(0)?,
            request_id: r.get(1)?,
            project_id: r.get(2)?,
            task_id: r.get(3)?,
            launch_revision: r.get(4)?,
            agent_kind: r.get(5)?,
            machine_ref: r.get(6)?,
            runtime_kind: r.get(7)?,
            repository_ref: r.get(8)?,
            state: r.get(9)?,
            revision: r.get(10)?,
            created_at_ms: r.get(11)?,
            updated_at_ms: r.get(12)?,
            last_seen_at_ms: r.get(13)?,
            runtime_session_ref: r.get(14)?,
        })
    ).optional().map_err(repo_error)?;
    raw.map(|v| {
        Ok(AgentJob {
            id: v.id,
            request_id: v.request_id,
            project_id: ProjectId::new(v.project_id).map_err(|e| invalid(e.to_string()))?,
            task_id: TaskId::new(v.task_id).map_err(|e| invalid(e.to_string()))?,
            launch_revision: v.launch_revision,
            agent_kind: v.agent_kind,
            machine_ref: v.machine_ref,
            runtime_kind: v.runtime_kind,
            repository_ref: v.repository_ref,
            state: AgentJobState::parse(&v.state)
                .ok_or_else(|| invalid("invalid persisted job state"))?,
            revision: v.revision,
            created_at_ms: v.created_at_ms,
            updated_at_ms: v.updated_at_ms,
            last_seen_at_ms: v.last_seen_at_ms,
            runtime_session_ref: v.runtime_session_ref,
        })
    })
    .transpose()
}

#[cfg(test)]
mod tests {
    use super::*;
    pub(super) fn job(id: &str, task: &str) -> AgentJob {
        AgentJob {
            id: id.into(),
            request_id: format!("req-{id}"),
            project_id: ProjectId::new("project").unwrap(),
            task_id: TaskId::new(task).unwrap(),
            launch_revision: 0,
            agent_kind: "codex".into(),
            machine_ref: "local".into(),
            runtime_kind: "herdr".into(),
            repository_ref: "repo".into(),
            state: AgentJobState::Queued,
            revision: 0,
            created_at_ms: 100,
            updated_at_ms: 100,
            last_seen_at_ms: None,
            runtime_session_ref: None,
        }
    }
    pub(super) fn ready_task(store: &SqliteStore, task: &str) {
        store.connection.execute("INSERT INTO projects(id,name,status) VALUES ('project','Project','active') ON CONFLICT(id) DO NOTHING",[]).unwrap();
        store.connection.execute("INSERT INTO tasks(id,title,status,project_id) VALUES (?1,'Task','ready','project')",[task]).unwrap();
    }
    #[test]
    fn connected_start_is_atomic_and_replay_safe() {
        let mut store = SqliteStore::in_memory().unwrap();
        ready_task(&store, "task");
        let request = job("connected", "task");
        let started = store
            .start_agent_job_for_device("device-a", &request, true, None)
            .unwrap();
        assert_eq!(started.state, AgentJobState::Preparing);
        assert_eq!(started.revision, 1);
        assert!(store.agent_job_worktree("connected").unwrap());
        assert_eq!(
            store.pending_agent_jobs_for_machine("local").unwrap().len(),
            1
        );
        assert_eq!(
            store
                .start_agent_job_for_device("device-a", &request, true, None)
                .unwrap()
                .revision,
            1
        );
        assert!(
            store
                .start_agent_job_for_device("device-b", &request, true, None)
                .is_err()
        );
        assert!(
            store
                .start_agent_job_for_device("device-a", &request, false, None)
                .is_err()
        );
        let counts: i64 = store
            .connection
            .query_row(
                "SELECT COUNT(*) FROM agent_job_events WHERE job_id='connected'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(counts, 2);
        store
            .connection
            .execute("UPDATE tasks SET status='done' WHERE id='task'", [])
            .unwrap();
        let other = job("new", "task");
        assert!(
            store
                .start_agent_job_for_device("device-a", &other, false, None)
                .is_err()
        );
        assert!(store.get_agent_job("new").unwrap().is_none());
    }
    #[test]
    fn connected_resume_is_atomic_and_preserves_frozen_handoff() {
        let mut store = SqliteStore::in_memory().unwrap();
        ready_task(&store, "task");
        store
            .start_agent_job_for_device("device-a", &job("first", "task"), true, None)
            .unwrap();
        store
            .transition_agent_job("first", 1, AgentJobState::Interrupted, 101, "stopped")
            .unwrap();
        assert!(
            store
                .resume_agent_job_for_device("device-a", "first", "second", 102)
                .is_err()
        );
        assert!(store.get_agent_job("second").unwrap().is_none());
        store
            .record_agent_job_handoff(
                "first",
                "handoff-one",
                &serde_json::json!({"summary":"continue","validation":{},"next_steps":["test"]}),
                102,
            )
            .unwrap();
        let resumed = store
            .resume_agent_job_for_device("device-a", "first", "second", 103)
            .unwrap();
        assert_eq!(resumed.state, AgentJobState::Preparing);
        assert_eq!(resumed.revision, 1);
        assert!(store.agent_job_worktree("second").unwrap());
        assert_eq!(
            store
                .resume_agent_job_for_device("device-a", "first", "second", 103)
                .unwrap()
                .id,
            "second"
        );
        assert!(
            store
                .resume_agent_job_for_device("device-b", "first", "third", 104)
                .is_err()
        );
        let brief: String = store.agent_job_brief("second").unwrap().unwrap();
        let brief: serde_json::Value = serde_json::from_str(&brief).unwrap();
        assert_eq!(brief["resume"]["prior_job_id"], "first");
        assert_eq!(brief["resume"]["handoff"]["summary"], "continue");
        assert_eq!(
            store.pending_agent_jobs_for_machine("local").unwrap().len(),
            1
        );
    }
    #[test]
    fn resume_context_is_frozen_from_latest_handoff_and_replay_safe() {
        let mut store = SqliteStore::in_memory().unwrap();
        ready_task(&store, "task");
        store.reserve_agent_job(&job("first", "task")).unwrap();
        store
            .transition_agent_job("first", 0, AgentJobState::Interrupted, 101, "stopped")
            .unwrap();
        store.record_agent_job_handoff("first","handoff-one",
            &serde_json::json!({"summary":"continue here","validation":{},"next_steps":["test"]}),102).unwrap();
        store.reserve_agent_job(&job("second", "task")).unwrap();
        store
            .attach_agent_resume_context("second", "first")
            .unwrap();
        store
            .attach_agent_resume_context("second", "first")
            .unwrap();
        let brief: String = store.agent_job_brief("second").unwrap().unwrap();
        let value: serde_json::Value = serde_json::from_str(&brief).unwrap();
        assert_eq!(value["resume"]["prior_job_id"], "first");
        assert_eq!(value["resume"]["handoff"]["summary"], "continue here");
        assert!(
            store
                .attach_agent_resume_context("second", "second")
                .is_err()
        );
        store
            .accept_agent_job_command("prepare-second", "second", 0, "prepare", 103)
            .unwrap();
        assert!(
            store
                .attach_agent_resume_context("second", "first")
                .is_err()
        );
    }
    #[test]
    fn resume_rejects_active_predecessor_and_missing_handoff() {
        let mut store = SqliteStore::in_memory().unwrap();
        ready_task(&store, "task");
        store.reserve_agent_job(&job("first", "task")).unwrap();
        assert!(store.reserve_agent_job(&job("second", "task")).is_err());
        store
            .transition_agent_job("first", 0, AgentJobState::Interrupted, 101, "stopped")
            .unwrap();
        store.reserve_agent_job(&job("second", "task")).unwrap();
        assert!(
            store
                .attach_agent_resume_context("second", "first")
                .is_err()
        );
    }
    #[test]
    fn resume_refuses_an_unconfirmed_launch_claim() {
        let mut store = SqliteStore::in_memory().unwrap();
        ready_task(&store, "task");
        store
            .start_agent_job_for_device("device-a", &job("first", "task"), false, None)
            .unwrap();
        assert!(
            store
                .claim_agent_launch("first", 1, "ambiguous-token", 101)
                .unwrap()
        );
        store
            .transition_agent_job("first", 2, AgentJobState::Interrupted, 102, "stopped")
            .unwrap();
        store
            .record_agent_job_handoff(
                "first",
                "handoff-one",
                &serde_json::json!({"summary":"continue","validation":{},"next_steps":["test"]}),
                103,
            )
            .unwrap();
        let error = store
            .resume_agent_job_for_device("device-a", "first", "second", 104)
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("reconcile") && error.contains("sweep"),
            "{error}"
        );
        assert!(store.get_agent_job("second").unwrap().is_none());
    }

    #[test]
    fn authenticated_owner_binding_is_atomic_and_replay_safe() {
        let mut store = SqliteStore::in_memory().unwrap();
        ready_task(&store, "task");
        let j = job("owned", "task");
        store.reserve_agent_job_for_device("device-a", &j).unwrap();
        store.require_agent_job_owner("device-a", "owned").unwrap();
        assert!(store.require_agent_job_owner("device-b", "owned").is_err());
        assert!(store.reserve_agent_job_for_device("device-b", &j).is_err());
        assert!(store.reserve_agent_job_for_device("device-a", &j).is_ok());
        assert_eq!(
            store
                .connection
                .query_row(
                    "SELECT COUNT(*) FROM agent_job_owners WHERE job_id='owned'",
                    [],
                    |r| r.get::<_, i64>(0)
                )
                .unwrap(),
            1
        );
    }
    #[test]
    fn prerequisite_gate_checks_reservation_prepare_and_claim() {
        let mut store = SqliteStore::in_memory().unwrap();
        ready_task(&store, "task");
        ready_task(&store, "prereq");
        store.connection.execute_batch("CREATE TABLE task_prerequisites (task_id TEXT NOT NULL, prerequisite_id TEXT NOT NULL, PRIMARY KEY(task_id,prerequisite_id)); INSERT INTO task_prerequisites VALUES('task','prereq');").unwrap();
        assert!(store.reserve_agent_job(&job("a", "task")).is_err());
        store
            .connection
            .execute("UPDATE tasks SET status='done' WHERE id='prereq'", [])
            .unwrap();
        store.reserve_agent_job(&job("a", "task")).unwrap();
        store
            .connection
            .execute(
                "UPDATE tasks SET status='in_progress' WHERE id='prereq'",
                [],
            )
            .unwrap();
        assert!(
            store
                .accept_agent_job_command("prepare-a", "a", 0, "prepare", 101)
                .is_err()
        );
        store
            .connection
            .execute("UPDATE tasks SET status='done' WHERE id='prereq'", [])
            .unwrap();
        store
            .accept_agent_job_command("prepare-a", "a", 0, "prepare", 101)
            .unwrap();
        store
            .connection
            .execute("UPDATE tasks SET status='ready' WHERE id='prereq'", [])
            .unwrap();
        assert!(store.claim_agent_launch("a", 1, "launch-a", 102).is_err());
        store
            .connection
            .execute("UPDATE tasks SET status='done' WHERE id='prereq'", [])
            .unwrap();
        assert!(store.claim_agent_launch("a", 1, "launch-a", 102).unwrap());
    }
    #[test]
    fn changed_task_context_refuses_prepare_and_launch() {
        let mut store = SqliteStore::in_memory().unwrap();
        ready_task(&store, "task");
        store.reserve_agent_job(&job("j", "task")).unwrap();
        assert!(store.verify_agent_job_brief_current("j").is_ok());
        store
            .connection
            .execute("UPDATE tasks SET title='Retitled' WHERE id='task'", [])
            .unwrap();
        assert!(store.verify_agent_job_brief_current("j").is_err());
        assert!(
            store
                .accept_agent_job_command("prepare-1", "j", 0, "prepare", 101)
                .is_err()
        );
        store
            .connection
            .execute("UPDATE tasks SET title='Task' WHERE id='task'", [])
            .unwrap();
        let prepared = store
            .accept_agent_job_command("prepare-1", "j", 0, "prepare", 101)
            .unwrap();
        assert_eq!(
            store.agent_job_command_receipt("prepare-1").unwrap(),
            Some(("j".into(), 1, "preparing".into()))
        );
        store
            .connection
            .execute("UPDATE tasks SET status='done' WHERE id='task'", [])
            .unwrap();
        assert!(
            store
                .claim_agent_launch("j", prepared.revision, "claim", 102)
                .is_err()
        );
    }
    #[test]
    fn idempotent_reservations_and_task_ownership() {
        let mut store = SqliteStore::in_memory().unwrap();
        ready_task(&store, "task");
        let a = job("a", "task");
        assert_eq!(store.reserve_agent_job(&a).unwrap(), a);
        assert_eq!(store.reserve_agent_job(&a).unwrap(), a);
        assert!(store.reserve_agent_job(&job("b", "task")).is_err());
        let a = store
            .transition_agent_job("a", 0, AgentJobState::Preparing, 101, "prepare")
            .unwrap();
        assert_eq!(a.revision, 1);
        assert!(
            store
                .transition_agent_job("a", 0, AgentJobState::Launching, 102, "stale")
                .is_err()
        );
        assert!(
            store
                .transition_agent_job("a", 1, AgentJobState::Completed, 102, "skip")
                .is_err()
        );
        let a = store
            .transition_agent_job("a", 1, AgentJobState::Interrupted, 102, "cancel")
            .unwrap();
        assert_eq!(a.state, AgentJobState::Interrupted);
        store.reserve_agent_job(&job("b", "task")).unwrap();
        assert_eq!(
            store
                .list_agent_jobs_for_project(&ProjectId::new("project").unwrap())
                .unwrap()
                .len(),
            2
        );
    }
    #[test]
    fn explicit_completion_is_terminal_and_preserves_handoffs() {
        let mut store = SqliteStore::in_memory().unwrap();
        ready_task(&store, "task");
        store.reserve_agent_job(&job("a", "task")).unwrap();
        // An agent that never launched has no completion evidence to record.
        assert!(store.complete_agent_job("a", 101, "done").is_err());
        store
            .transition_agent_job("a", 0, AgentJobState::Preparing, 101, "prepare")
            .unwrap();
        store
            .transition_agent_job("a", 1, AgentJobState::Launching, 102, "launch")
            .unwrap();
        let running = store
            .transition_agent_job("a", 2, AgentJobState::Running, 103, "runtime_present")
            .unwrap();
        store
            .record_agent_job_handoff(
                "a",
                "handoff-one",
                &serde_json::json!({"summary":"implemented","validation":{"status":"cargo test"},"next_steps":[]}),
                104,
            )
            .unwrap();
        let completed = store
            .complete_agent_job("a", 105, "  agent reported implementation  ")
            .unwrap();
        assert_eq!(completed.state, AgentJobState::Completed);
        assert_eq!(completed.revision, running.revision + 1);
        assert_eq!(completed.updated_at_ms, 105);
        // Handoffs survive completion; the task is never completed implicitly.
        assert_eq!(store.list_agent_job_handoffs("a").unwrap().len(), 1);
        let task_status: String = store
            .connection
            .query_row("SELECT status FROM tasks WHERE id='task'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(task_status, "ready");
        let event: (String, String) = store
            .connection
            .query_row(
                "SELECT state,reason FROM agent_job_events WHERE job_id='a' AND revision=?1",
                [completed.revision],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(
            event,
            (
                "completed".to_owned(),
                "completed:agent reported implementation".to_owned()
            )
        );
        // Terminal jobs are refused rather than re-completed.
        assert!(store.complete_agent_job("a", 106, "again").is_err());
        assert!(store.complete_agent_job("missing", 106, "x").is_err());
        assert!(store.complete_agent_job("a", 90, "regression").is_err());
    }
    #[test]
    fn launch_claims_are_single_dispatch_and_recoverable() {
        let mut store = SqliteStore::in_memory().unwrap();
        ready_task(&store, "task");
        store.reserve_agent_job(&job("a", "task")).unwrap();
        store
            .transition_agent_job("a", 0, AgentJobState::Preparing, 101, "prepare")
            .unwrap();
        assert!(store.claim_agent_launch("a", 1, "token-a", 102).unwrap());
        assert!(!store.claim_agent_launch("a", 1, "token-a", 102).unwrap());
        assert!(store.claim_agent_launch("a", 2, "token-b", 103).is_err());
        store
            .record_launch_result("a", "token-a", None, 103)
            .unwrap();
        store
            .record_launch_result("a", "token-a", Some("pane-1"), 104)
            .unwrap();
        assert_eq!(
            store
                .get_agent_job("a")
                .unwrap()
                .unwrap()
                .runtime_session_ref
                .as_deref(),
            Some("pane-1")
        );
        assert!(
            store
                .record_launch_result("a", "token-a", Some("pane-2"), 105)
                .is_err()
        );
    }
    #[test]
    fn accepted_commands_are_idempotent_revision_safe_and_audited() {
        let mut store = SqliteStore::in_memory().unwrap();
        ready_task(&store, "task");
        store.reserve_agent_job(&job("a", "task")).unwrap();
        let before = store.agent_job_brief("a").unwrap().unwrap();
        store
            .connection
            .execute("UPDATE tasks SET title='Revised task' WHERE id='task'", [])
            .unwrap();
        assert_eq!(store.agent_job_brief("a").unwrap().unwrap(), before);
        store
            .connection
            .execute("UPDATE tasks SET title='Task' WHERE id='task'", [])
            .unwrap();
        let prepared = store
            .accept_agent_job_command("command-a", "a", 0, "prepare", 101)
            .unwrap();
        assert_eq!(prepared.revision, 1);
        assert_eq!(
            store
                .accept_agent_job_command("command-a", "a", 0, "prepare", 101)
                .unwrap()
                .revision,
            1
        );
        assert!(
            store
                .accept_agent_job_command("command-a", "a", 0, "interrupt", 102)
                .is_err()
        );
        assert!(
            store
                .accept_agent_job_command("command-b", "a", 0, "interrupt", 102)
                .is_err()
        );
        let interrupted = store
            .accept_agent_job_command("command-c", "a", 1, "interrupt", 103)
            .unwrap();
        assert_eq!(interrupted.state, AgentJobState::Interrupted);
        let count: i64 = store
            .connection
            .query_row(
                "SELECT COUNT(*) FROM agent_job_events WHERE job_id='a'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 3);
        let task_status: String = store
            .connection
            .query_row("SELECT status FROM tasks WHERE id='task'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(task_status, "ready");
    }
    #[test]
    fn reconciliation_preserves_owner_during_ambiguous_launch() {
        let mut store = SqliteStore::in_memory().unwrap();
        ready_task(&store, "task");
        store.reserve_agent_job(&job("a", "task")).unwrap();
        store
            .accept_agent_job_command("prep", "a", 0, "prepare", 101)
            .unwrap();
        store.claim_agent_launch("a", 1, "token", 102).unwrap();
        store.record_launch_result("a", "token", None, 103).unwrap();
        let observed = store
            .observe_agent_session(
                "a",
                2,
                "token",
                AgentRuntimeObservation::AbsentConfirmed,
                104,
            )
            .unwrap();
        assert_eq!(observed.state, AgentJobState::Unreachable);
        assert!(store.reserve_agent_job(&job("b", "task")).is_err());
        assert_eq!(
            store.agent_launch_claim("a").unwrap().unwrap().1,
            "ambiguous"
        );
        let recovered = store
            .observe_agent_session(
                "a",
                3,
                "token",
                AgentRuntimeObservation::Present {
                    session_ref: "pane".into(),
                    state: RuntimeActivity::Working,
                },
                105,
            )
            .unwrap();
        assert_eq!(recovered.state, AgentJobState::Running);
        assert_eq!(recovered.runtime_session_ref.as_deref(), Some("pane"));
        assert!(
            store
                .observe_agent_session("a", 3, "token", AgentRuntimeObservation::Unreachable, 106)
                .is_err()
        );
        assert!(
            store
                .observe_agent_session("a", 4, "wrong", AgentRuntimeObservation::Unreachable, 106)
                .is_err()
        );
        assert_eq!(store.list_agent_jobs_needing_attention().unwrap().len(), 0);
    }
    #[test]
    fn confirmed_missing_session_interrupts_without_completing_task() {
        let mut store = SqliteStore::in_memory().unwrap();
        ready_task(&store, "task");
        store.reserve_agent_job(&job("a", "task")).unwrap();
        store
            .accept_agent_job_command("prep", "a", 0, "prepare", 101)
            .unwrap();
        store.claim_agent_launch("a", 1, "token", 102).unwrap();
        store
            .record_launch_result("a", "token", Some("pane"), 103)
            .unwrap();
        let running = store
            .observe_agent_session(
                "a",
                2,
                "token",
                AgentRuntimeObservation::Present {
                    session_ref: "pane".into(),
                    state: RuntimeActivity::Working,
                },
                104,
            )
            .unwrap();
        assert_eq!(running.state, AgentJobState::Running);
        assert!(
            store
                .observe_agent_session(
                    "a",
                    running.revision,
                    "token",
                    AgentRuntimeObservation::Present {
                        session_ref: "other".into(),
                        state: RuntimeActivity::Working
                    },
                    105
                )
                .is_err()
        );
        let interrupted = store
            .observe_agent_session(
                "a",
                running.revision,
                "token",
                AgentRuntimeObservation::AbsentConfirmed,
                106,
            )
            .unwrap();
        assert_eq!(interrupted.state, AgentJobState::Interrupted);
        let status: String = store
            .connection
            .query_row("SELECT status FROM tasks WHERE id='task'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(status, "ready");
    }
    #[test]
    fn cannot_launch_missing_or_nonready_tasks() {
        let mut store = SqliteStore::in_memory().unwrap();
        assert!(store.reserve_agent_job(&job("a", "missing")).is_err());
        ready_task(&store, "blocked");
        store
            .connection
            .execute("UPDATE tasks SET status='blocked' WHERE id='blocked'", [])
            .unwrap();
        assert!(store.reserve_agent_job(&job("b", "blocked")).is_err());
    }
}

#[cfg(test)]
mod m23_execution_policy_tests {
    use super::*;
    #[test]
    fn execution_policy_is_persisted_and_conflicting_replay_is_refused() {
        let mut store = SqliteStore::in_memory().unwrap();
        // Missing jobs cannot acquire execution policy.
        assert!(store.set_agent_job_worktree("missing", true).is_err());
        assert!(!store.agent_job_worktree("missing").unwrap());
    }
    #[test]
    fn handoffs_require_a_known_job_and_structured_payload() {
        let mut store = SqliteStore::in_memory().unwrap();
        assert!(
            store
                .record_agent_job_handoff(
                    "missing",
                    "h1",
                    &serde_json::json!({"summary":"x","validation":[],"next_steps":[]}),
                    0
                )
                .is_err()
        );
        assert!(
            store
                .record_agent_job_handoff("missing", "h2", &serde_json::json!({"summary":"x"}), 0)
                .is_err()
        );
    }
}

#[cfg(test)]
mod m23_catchup_tests {
    use super::*;
    #[test]
    fn catchup_exposes_pending_review_and_changes_requested_without_completing_task() {
        let mut store = SqliteStore::in_memory().unwrap();
        store.connection.execute("INSERT INTO projects(id,title,status,created_at_ms,updated_at_ms) VALUES ('proj','Project','active',0,0)",[]).ok();
        // Exercise the read model independently of project/task command setup.
        store.connection.execute("INSERT INTO agent_jobs(id,request_id,project_id,task_id,launch_revision,agent_kind,machine_ref,runtime_kind,repository_ref,state,revision,created_at_ms,updated_at_ms) VALUES ('job','req','proj','task',0,'codex','local','herdr','/repo','running',0,1,2)",[]).unwrap();
        let initially = store.agent_job_overview().unwrap();
        assert_eq!(initially[0]["needs_attention"], false);
        store
            .record_agent_job_handoff(
                "job",
                "h1",
                &serde_json::json!({"summary":"done","validation":{},"next_steps":[]}),
                3,
            )
            .unwrap();
        let pending = store.agent_job_overview().unwrap();
        assert_eq!(pending[0]["needs_attention"], true);
        assert_eq!(pending[0]["next_action"], "review_handoff");
        assert_eq!(store.list_agent_jobs_needing_attention().unwrap().len(), 1);
        store
            .review_agent_job("job", "h1", "changes_requested", "reviewer", "fix tests", 4)
            .unwrap();
        let changed = store.agent_job_overview().unwrap();
        assert_eq!(changed[0]["next_action"], "resume_or_rework");
        assert_eq!(changed[0]["review_decision"], "changes_requested");
        assert_eq!(store.list_agent_jobs_needing_attention().unwrap().len(), 1);
    }

    #[test]
    fn completed_job_without_a_handoff_is_not_acceptance_ready() {
        let store = SqliteStore::in_memory().unwrap();
        store
            .connection
            .execute(
                "INSERT INTO agent_jobs(id,request_id,project_id,task_id,launch_revision,\
                 agent_kind,machine_ref,runtime_kind,repository_ref,state,revision,\
                 created_at_ms,updated_at_ms) \
                 VALUES ('job','req','proj','task',0,'codex','local','herdr','/repo',\
                 'completed',4,1,2)",
                [],
            )
            .unwrap();
        let overview = store.agent_job_overview().unwrap();
        assert_eq!(overview[0]["next_action"], "inspect_missing_handoff");
    }
}

#[cfg(test)]
mod m23_polling_tests {
    use super::*;
    #[test]
    fn sweep_only_selects_claimed_nonterminal_jobs() {
        let store = SqliteStore::in_memory().unwrap();
        store.connection.execute_batch("\
            INSERT INTO agent_jobs (id,request_id,project_id,task_id,launch_revision,agent_kind,machine_ref,runtime_kind,repository_ref,state,revision,created_at_ms,updated_at_ms)\
            VALUES ('ready','r1','p','t1',0,'codex','local','herdr','/repo','running',1,1,2),\
                   ('queued','r2','p','t2',0,'codex','local','herdr','/repo','queued',0,1,2),\
                   ('finished','r3','p','t3',0,'codex','local','herdr','/repo','completed',2,1,2);\
            INSERT INTO agent_job_launch_claims(job_id,claim_token,claimed_at_ms,dispatch_state)\
            VALUES ('ready','token-a',2,'confirmed'),('finished','token-b',2,'confirmed');").unwrap();
        let candidates = store.agent_job_poll_candidates().unwrap();
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].id, "ready");
        // The query is read-only; task status cannot change from monitoring.
        assert_eq!(
            store.get_agent_job("finished").unwrap().unwrap().state,
            AgentJobState::Completed
        );
    }
}

#[cfg(test)]
mod m32_executor_read_tests {
    use super::tests::{job, ready_task};
    use super::*;

    /// The shared fixture targets `local`; executor reads are keyed by the
    /// `machine_ref` stored on the job, so tests bind it explicitly.
    fn job_on(machine: &str, id: &str, task: &str) -> AgentJob {
        AgentJob {
            machine_ref: machine.into(),
            ..job(id, task)
        }
    }

    /// The authority schema owns `devices`; standalone storage tests install the
    /// same columns so the read model is exercised against real SQL.
    fn enroll(store: &SqliteStore, id: &str, revoked: i64) {
        store
            .connection
            .execute_batch(
                "CREATE TABLE IF NOT EXISTS devices (id TEXT PRIMARY KEY NOT NULL, \
                 token_hash BLOB NOT NULL, revoked INTEGER NOT NULL DEFAULT 0)",
            )
            .unwrap();
        store
            .connection
            .execute(
                "INSERT INTO devices(id,token_hash,revoked) VALUES (?1,zeroblob(32),?2)",
                rusqlite::params![id, revoked],
            )
            .unwrap();
    }

    #[test]
    fn executor_read_is_empty_without_any_enrolled_device() {
        let store = SqliteStore::in_memory().unwrap();
        assert!(store.agent_executors().unwrap().is_empty());
        assert_eq!(store.agent_executor_active("ghost").unwrap(), None);
    }

    #[test]
    fn executor_read_reports_active_revoked_claims_and_counts() {
        let mut store = SqliteStore::in_memory().unwrap();
        // Inserted out of order: the read must order by device id, not by insert.
        enroll(&store, "device-b", 0);
        enroll(&store, "device-a", 0);
        enroll(&store, "device-c", 1);
        for task in ["t1", "t2", "t3"] {
            ready_task(&store, task);
        }
        store
            .start_agent_job_for_device(
                "device-a",
                &job_on("device-a", "unclaimed", "t1"),
                false,
                None,
            )
            .unwrap();
        store
            .start_agent_job_for_device(
                "device-a",
                &job_on("device-a", "claimed", "t2"),
                false,
                None,
            )
            .unwrap();
        store
            .claim_agent_launch_for_machine("device-a", "claimed", 1, "token-a", 110)
            .unwrap();
        store
            .start_agent_job_for_device("device-b", &job_on("device-b", "other", "t3"), false, None)
            .unwrap();

        let executors = store.agent_executors().unwrap();
        assert_eq!(
            executors
                .iter()
                .map(|e| e.device_id.as_str())
                .collect::<Vec<_>>(),
            ["device-a", "device-b", "device-c"]
        );
        let a = &executors[0];
        assert!(a.active);
        assert_eq!(a.last_claim_at_ms, Some(110));
        // `claimed` is in launching; `unclaimed` is still preparing.
        assert_eq!(a.claimed_jobs, 1);
        assert_eq!(a.preparing_jobs, 1);
        let b = &executors[1];
        assert!(b.active);
        assert_eq!(b.last_claim_at_ms, None);
        assert_eq!((b.claimed_jobs, b.preparing_jobs), (0, 1));
        let c = &executors[2];
        assert!(!c.active, "a revoked device must be reported inactive");
        assert_eq!((c.claimed_jobs, c.preparing_jobs), (0, 0));

        assert_eq!(store.agent_executor_active("device-a").unwrap(), Some(true));
        assert_eq!(
            store.agent_executor_active("device-c").unwrap(),
            Some(false)
        );
        assert_eq!(store.agent_executor_active("ghost").unwrap(), None);
        assert_eq!(store.agent_executor_active("").unwrap(), None);
    }

    #[test]
    fn claim_state_is_null_until_claimed_then_confirmed() {
        let mut store = SqliteStore::in_memory().unwrap();
        ready_task(&store, "task");
        ready_task(&store, "other");
        store
            .start_agent_job_for_device("device-a", &job_on("device-a", "a", "task"), false, None)
            .unwrap();
        store
            .start_agent_job_for_device("device-a", &job_on("device-a", "b", "other"), false, None)
            .unwrap();
        assert_eq!(store.agent_job_claim_state("a").unwrap(), None);
        assert_eq!(
            store.agent_claim_states(&["a", "b", "missing"]).unwrap(),
            []
        );

        store
            .claim_agent_launch_for_machine("device-a", "a", 1, "token-a", 110)
            .unwrap();
        assert_eq!(
            store.agent_job_claim_state("a").unwrap().as_deref(),
            Some("claimed")
        );
        store
            .record_launch_result("a", "token-a", Some("pane-1"), 111)
            .unwrap();
        assert_eq!(
            store.agent_job_claim_state("a").unwrap().as_deref(),
            Some("confirmed")
        );
        // The unclaimed job is untouched by another job's claim.
        assert_eq!(store.agent_job_claim_state("b").unwrap(), None);
        assert_eq!(
            store.agent_claim_states(&["a", "b"]).unwrap(),
            [("a".to_owned(), "confirmed".to_owned())]
        );
        assert!(store.agent_claim_states(&[]).unwrap().is_empty());
    }

    #[test]
    fn unclaimed_preparing_job_awaits_an_executor_without_attention() {
        let mut store = SqliteStore::in_memory().unwrap();
        ready_task(&store, "task");
        store
            .start_agent_job_for_device(
                "device-a",
                &job_on("device-a", "waiting", "task"),
                false,
                None,
            )
            .unwrap();
        let overview = store.agent_job_overview().unwrap();
        assert_eq!(overview[0]["next_action"], "await_executor");
        assert_eq!(overview[0]["needs_attention"], false);

        // A queued job that already carries a claim is not waiting for anyone.
        store
            .connection
            .execute_batch(
                "INSERT INTO agent_jobs(id,request_id,project_id,task_id,launch_revision,\
                 agent_kind,machine_ref,runtime_kind,repository_ref,state,revision,\
                 created_at_ms,updated_at_ms) VALUES ('queued','rq','project','t-q',0,\
                 'codex','device-a','herdr','repo','queued',0,1,2);\
                 INSERT INTO agent_job_launch_claims(job_id,claim_token,claimed_at_ms,dispatch_state)\
                 VALUES ('queued','token-q',2,'claimed');",
            )
            .unwrap();
        let overview = store.agent_job_overview().unwrap();
        let queued = overview
            .iter()
            .find(|item| item["job_id"] == "queued")
            .expect("the queued job is listed");
        assert_eq!(queued["next_action"], "none");
    }

    #[test]
    fn objective_is_trimmed_stored_and_absent_when_blank() {
        let mut store = SqliteStore::in_memory().unwrap();
        for task in ["with-objective", "blank", "absent"] {
            ready_task(&store, task);
        }
        let read = |store: &SqliteStore, id: &str| -> serde_json::Value {
            serde_json::from_str(&store.agent_job_brief(id).unwrap().unwrap()).unwrap()
        };
        store
            .start_agent_job_for_device(
                "device-a",
                &job_on("device-a", "with-objective", "with-objective"),
                false,
                Some("  Fix the login redirect  "),
            )
            .unwrap();
        assert_eq!(
            read(&store, "with-objective")["objective"],
            "Fix the login redirect"
        );

        store
            .start_agent_job_for_device(
                "device-a",
                &job_on("device-a", "blank", "blank"),
                false,
                Some("   \n\t "),
            )
            .unwrap();
        assert!(read(&store, "blank").get("objective").is_none());

        store
            .start_agent_job_for_device(
                "device-a",
                &job_on("device-a", "absent", "absent"),
                false,
                None,
            )
            .unwrap();
        assert!(read(&store, "absent").get("objective").is_none());
    }

    #[test]
    fn resumed_job_inherits_the_predecessor_objective() {
        let mut store = SqliteStore::in_memory().unwrap();
        ready_task(&store, "task");
        store
            .start_agent_job_for_device(
                "device-a",
                &job_on("device-a", "first", "task"),
                false,
                Some("Keep the objective across resume"),
            )
            .unwrap();
        store
            .transition_agent_job("first", 1, AgentJobState::Interrupted, 110, "stopped")
            .unwrap();
        store
            .record_agent_job_handoff(
                "first",
                "handoff-one",
                &serde_json::json!({"summary":"continue","validation":{},"next_steps":[]}),
                111,
            )
            .unwrap();
        store
            .resume_agent_job_for_device("device-a", "first", "second", 112)
            .unwrap();
        let brief: serde_json::Value =
            serde_json::from_str(&store.agent_job_brief("second").unwrap().unwrap()).unwrap();
        assert_eq!(brief["objective"], "Keep the objective across resume");
        assert_eq!(brief["resume"]["prior_job_id"], "first");
        // The predecessor brief is immutable, so the objective did not move.
        let prior: serde_json::Value =
            serde_json::from_str(&store.agent_job_brief("first").unwrap().unwrap()).unwrap();
        assert_eq!(prior["objective"], "Keep the objective across resume");
    }
}
