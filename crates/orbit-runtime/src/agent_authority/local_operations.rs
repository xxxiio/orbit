//! Local-authority execution facade. The canonical host owns SQLite and the
//! external runtime adapter executes only after a durable launch claim.
use super::*;

/// Inputs for one local-authority agent launch.
pub struct LocalAgentLaunch<'a> {
    pub task_id: &'a str,
    pub job_id: &'a str,
    pub agent: &'a str,
    pub machine: &'a str,
    pub repository: &'a str,
    pub worktree: bool,
    pub at_ms: i64,
}

impl AuthoritativeHost {
    /// Local authority one-step launch. The repository and machine are explicit;
    /// no arbitrary command string crosses the authority boundary. A durable
    /// reservation and preparation are committed before any Herdr side effect.
    pub fn start_local_agent_job<R: orbit_application::agent_runtime::AgentRuntime>(
        &mut self,
        launch: LocalAgentLaunch<'_>,
        runtime: &mut R,
    ) -> Result<serde_json::Value, String> {
        use crate::agent_dispatch::{DispatchResult, dispatch_prepared_job};
        let LocalAgentLaunch {
            task_id,
            job_id,
            agent,
            machine,
            repository,
            worktree,
            at_ms,
        } = launch;
        if !matches!(agent, "codex" | "opencode") {
            return Err("unsupported agent kind".into());
        }
        for value in [task_id, job_id, machine, repository] {
            if value.is_empty() || value.len() > 1024 || value.chars().any(|c| c.is_control()) {
                return Err("invalid agent launch input".into());
            }
        }
        if !std::path::Path::new(repository).is_absolute() || at_ms < 0 {
            return Err("repository must be an absolute path and timestamp nonnegative".into());
        }
        let (_, tasks) = self
            .tasks()
            .map_err(|e| format!("task read failed: {e:?}"))?;
        let task = tasks
            .iter()
            .find(|t| t.id == task_id)
            .ok_or("task not found")?;
        let project = task.project_id.clone().ok_or("task has no project")?;
        let job = AgentJob {
            id: job_id.into(),
            request_id: job_id.into(),
            project_id: ProjectId::new(project).map_err(|e| format!("project ID: {e:?}"))?,
            task_id: TaskId::new(task_id.to_owned()).map_err(|e| format!("task ID: {e:?}"))?,
            launch_revision: 0,
            agent_kind: agent.into(),
            machine_ref: machine.into(),
            runtime_kind: "herdr".into(),
            repository_ref: repository.into(),
            state: orbit_domain::AgentJobState::Queued,
            revision: 0,
            created_at_ms: at_ms,
            updated_at_ms: at_ms,
            last_seen_at_ms: None,
            runtime_session_ref: None,
        };
        let saved = self
            .authority
            .conn
            .reserve_agent_job(&job)
            .map_err(|e| e.to_string())?;
        // A duplicate request must never result in a second execution.
        if saved.state != orbit_domain::AgentJobState::Queued {
            return Ok(
                serde_json::json!({"job_id":job_id,"outcome":"already_reserved","state":saved.state.as_str()}),
            );
        }
        self.authority
            .conn
            .set_agent_job_worktree(job_id, worktree)
            .map_err(|e| e.to_string())?;
        let prepared = self
            .authority
            .conn
            .accept_agent_job_command(&format!("{job_id}:prepare"), job_id, 0, "prepare", at_ms)
            .map_err(|e| e.to_string())?;
        let result = dispatch_prepared_job(
            &mut self.authority.conn,
            runtime,
            job_id,
            prepared.revision,
            &format!("{job_id}:launch"),
            at_ms,
        )?;
        Ok(match result {
            DispatchResult::Confirmed(session) => {
                serde_json::json!({"job_id":job_id,"outcome":"confirmed","session":session})
            }
            DispatchResult::AlreadyClaimed => {
                serde_json::json!({"job_id":job_id,"outcome":"already_claimed"})
            }
            DispatchResult::NeedsReconciliation(reason) => {
                serde_json::json!({"job_id":job_id,"outcome":"reconcile","reason":reason})
            }
        })
    }

    /// Resume a terminal job via a new job, preserving the last handoff in its
    /// immutable execution brief before Herdr is invoked.
    pub fn resume_local_agent_job<R: orbit_application::agent_runtime::AgentRuntime>(
        &mut self,
        prior_id: &str,
        new_id: &str,
        runtime: &mut R,
        at_ms: i64,
    ) -> Result<serde_json::Value, String> {
        use crate::agent_dispatch::{DispatchResult, dispatch_prepared_job};
        let previous = self
            .authority
            .conn
            .get_agent_job(prior_id)
            .map_err(|e| e.to_string())?
            .ok_or("previous job not found")?;
        if !previous.state.is_terminal() {
            return Err("previous job must be terminal before resuming".into());
        }
        if previous.machine_ref != "local" {
            return Err("remote resumes require a machine-side executor".into());
        }
        if self
            .authority
            .conn
            .list_agent_job_handoffs(prior_id)
            .map_err(|e| e.to_string())?
            .is_empty()
        {
            return Err("previous job has no durable handoff".into());
        }
        let new_job = AgentJob {
            id: new_id.into(),
            request_id: new_id.into(),
            state: orbit_domain::AgentJobState::Queued,
            revision: 0,
            launch_revision: previous.launch_revision + 1,
            created_at_ms: at_ms,
            updated_at_ms: at_ms,
            last_seen_at_ms: None,
            runtime_session_ref: None,
            ..previous
        };
        self.authority
            .conn
            .reserve_agent_job(&new_job)
            .map_err(|e| e.to_string())?;
        self.authority
            .conn
            .attach_agent_resume_context(new_id, prior_id)
            .map_err(|e| e.to_string())?;
        let worktree = self
            .authority
            .conn
            .agent_job_worktree(prior_id)
            .map_err(|e| e.to_string())?;
        self.authority
            .conn
            .set_agent_job_worktree(new_id, worktree)
            .map_err(|e| e.to_string())?;
        let prepared = self
            .authority
            .conn
            .accept_agent_job_command(&format!("{new_id}:prepare"), new_id, 0, "prepare", at_ms)
            .map_err(|e| e.to_string())?;
        let outcome = dispatch_prepared_job(
            &mut self.authority.conn,
            runtime,
            new_id,
            prepared.revision,
            &format!("{new_id}:launch"),
            at_ms,
        )?;
        Ok(match outcome {
            DispatchResult::Confirmed(session) => {
                serde_json::json!({"job_id":new_id,"resumed_from":prior_id,"outcome":"confirmed","session":session})
            }
            DispatchResult::AlreadyClaimed => {
                serde_json::json!({"job_id":new_id,"outcome":"already_claimed"})
            }
            DispatchResult::NeedsReconciliation(reason) => {
                serde_json::json!({"job_id":new_id,"outcome":"reconcile","reason":reason})
            }
        })
    }

    pub fn dispatch_local_agent_job<R: orbit_application::agent_runtime::AgentLauncher>(
        &mut self,
        runtime: &mut R,
        id: &str,
        revision: i64,
        token: &str,
        at_ms: i64,
    ) -> Result<crate::agent_dispatch::DispatchResult, String> {
        crate::agent_dispatch::dispatch_prepared_job(
            &mut self.authority.conn,
            runtime,
            id,
            revision,
            token,
            at_ms,
        )
    }
    pub fn record_local_agent_handoff(
        &mut self,
        job_id: &str,
        handoff_id: &str,
        payload: &serde_json::Value,
        at_ms: i64,
    ) -> Result<(), String> {
        self.authority
            .conn
            .record_agent_job_handoff(job_id, handoff_id, payload, at_ms)
            .map_err(|e| e.to_string())
    }
    pub fn review_local_agent_job(
        &mut self,
        job_id: &str,
        handoff_id: &str,
        decision: &str,
        reviewer: &str,
        reason: &str,
        at_ms: i64,
    ) -> Result<serde_json::Value, String> {
        self.authority
            .conn
            .review_agent_job(job_id, handoff_id, decision, reviewer, reason, at_ms)
            .map_err(|e| e.to_string())
    }
    pub fn local_agent_repository(&self, job_id: &str) -> Result<String, String> {
        self.authority
            .conn
            .get_agent_job(job_id)
            .map_err(|e| e.to_string())?
            .map(|j| j.repository_ref)
            .ok_or_else(|| "unknown agent job".into())
    }
    /// Return the trusted machine-local evidence selector; never substitute the
    /// original repository when this job requested isolated worktree execution.
    pub fn local_agent_evidence_location(
        &self,
        job_id: &str,
    ) -> Result<(String, Option<String>), String> {
        let job = self
            .authority
            .conn
            .get_agent_job(job_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "unknown agent job".to_string())?;
        if job.machine_ref != "local" {
            return Err("Git evidence requires a trusted executor on the target machine; remote capture is not supported locally".into());
        }
        if !self
            .authority
            .conn
            .agent_job_worktree(job_id)
            .map_err(|e| e.to_string())?
        {
            return Ok((job.repository_ref, None));
        }
        let (token, _, _) = self
            .authority
            .conn
            .agent_launch_claim(job_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "worktree job has no launch claim".to_string())?;
        // This naming contract matches the deterministic branch in HerdrRuntime.
        let mut hash: u64 = 0xcbf29ce484222325;
        for b in token.bytes() {
            hash ^= u64::from(b);
            hash = hash.wrapping_mul(0x100000001b3);
        }
        Ok((
            job.repository_ref,
            Some(format!("refs/heads/orbit/orbit{hash:016x}")),
        ))
    }
    pub fn local_agent_review(&self, job_id: &str) -> Result<Option<serde_json::Value>, String> {
        self.authority
            .conn
            .agent_job_review(job_id)
            .map_err(|e| e.to_string())
    }
    pub fn local_agent_handoffs(&self, job_id: &str) -> Result<Vec<serde_json::Value>, String> {
        self.authority
            .conn
            .list_agent_job_handoffs(job_id)
            .map_err(|e| e.to_string())
    }
    pub fn local_agent_job(&self, job_id: &str) -> Result<Option<AgentJobView>, String> {
        self.authority
            .conn
            .get_agent_job(job_id)
            .map(|j| j.map(Into::into))
            .map_err(|e| e.to_string())
    }
    /// Agent polling is explicit and never launches any new sessions.
    pub fn local_agent_poll_candidates(&self) -> Result<Vec<AgentJobView>, String> {
        self.authority
            .conn
            .agent_job_poll_candidates()
            .map(|jobs| jobs.into_iter().map(Into::into).collect())
            .map_err(|e| e.to_string())
    }
    /// Record a stable automatic snapshot after a runtime becomes reviewable.
    /// The snapshot is repository evidence, never an assertion of agent authorship.
    pub fn record_local_agent_auto_evidence(
        &mut self,
        job_id: &str,
        revision: i64,
        payload: &serde_json::Value,
        at_ms: i64,
    ) -> Result<String, String> {
        let id = format!("auto-evidence-{job_id}-{revision}");
        self.record_local_agent_handoff(job_id, &id, payload, at_ms)?;
        Ok(id)
    }
    pub fn local_agent_overview(&self) -> Result<Vec<serde_json::Value>, String> {
        self.authority
            .conn
            .agent_job_overview()
            .map_err(|e| e.to_string())
    }
    pub fn local_agent_attention(&self) -> Result<Vec<AgentJobView>, String> {
        self.authority
            .conn
            .list_agent_jobs_needing_attention()
            .map(|v| v.into_iter().map(Into::into).collect())
            .map_err(|e| e.to_string())
    }
    pub fn inspect_local_agent_job<R: orbit_application::agent_runtime::AgentInspector>(
        &mut self,
        runtime: &mut R,
        job_id: &str,
        revision: i64,
        at_ms: i64,
    ) -> Result<AgentJobView, String> {
        crate::agent_monitor::reconcile_job(
            &mut self.authority.conn,
            runtime,
            job_id,
            revision,
            at_ms,
        )
        .map(Into::into)
    }
}
