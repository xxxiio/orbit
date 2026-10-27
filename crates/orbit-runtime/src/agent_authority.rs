//! Authenticated transport adapters call this host boundary; they must not
//! open a second SQLite connection or issue runtime side effects themselves.
use crate::{AuthoritativeHost, AuthorityError};
use orbit_domain::{AgentJob, ProjectId, TaskId};
use serde::{Deserialize, Serialize};

#[cfg(test)]
mod authority_tests;
mod local_operations;
mod machine_operations;
mod owner_operations;

pub use local_operations::LocalAgentLaunch;

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReserveAgentJob {
    pub job_id: String,
    pub request_id: String,
    pub project_id: String,
    pub task_id: String,
    pub agent_kind: String,
    pub machine_ref: String,
    pub runtime_kind: String,
    pub repository_ref: String,
    pub launch_revision: i64,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StartAgentJob {
    pub job_id: String,
    pub task_id: String,
    pub agent_kind: String,
    pub machine_ref: String,
    pub repository_ref: String,
    pub worktree: bool,
    pub at_ms: i64,
    /// Optional launch objective. Additive: existing payloads omit it and keep
    /// working, and the task brief captured at reservation stays authoritative.
    #[serde(default)]
    pub objective: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResumeAgentJob {
    pub prior_job_id: String,
    pub new_job_id: String,
    pub at_ms: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentJobCommand {
    pub command_id: String,
    pub job_id: String,
    pub expected_revision: i64,
    pub action: String,
    pub at_ms: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SubmitAgentHandoff {
    pub job_id: String,
    pub handoff_id: String,
    pub payload: serde_json::Value,
    pub at_ms: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewAgentHandoff {
    pub job_id: String,
    pub handoff_id: String,
    pub decision: String,
    pub reason: String,
    pub at_ms: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReportMachineAgentHandoff {
    pub job_id: String,
    pub claim_token: String,
    pub session_ref: String,
    pub handoff_id: String,
    pub payload: serde_json::Value,
    pub at_ms: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QueueSessionPrompt {
    pub command_id: String,
    pub job_id: String,
    pub message: String,
    pub at_ms: i64,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClaimSessionPrompt {
    pub command_id: String,
    pub at_ms: i64,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReportSessionPrompt {
    pub command_id: String,
    pub delivered: bool,
    pub at_ms: i64,
}

/// A registered Orbit device can act as a machine executor only for jobs
/// whose machine_ref equals its authenticated device ID. No server shell access.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClaimMachineAgentJob {
    pub job_id: String,
    pub expected_revision: i64,
    pub claim_token: String,
    pub at_ms: i64,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReportMachineAgentLaunch {
    pub job_id: String,
    pub claim_token: String,
    pub session_ref: Option<String>,
    pub at_ms: i64,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReportMachineAgentObservation {
    pub job_id: String,
    pub expected_revision: i64,
    pub claim_token: String,
    pub session_ref: Option<String>,
    pub activity: String,
    pub at_ms: i64,
}
#[derive(Debug, Clone, Serialize)]
pub struct MachineLaunchClaimReceipt {
    pub job: AgentJobView,
    pub claimed: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct AgentJobView {
    pub id: String,
    pub request_id: String,
    pub project_id: String,
    pub task_id: String,
    pub state: String,
    pub revision: i64,
    pub agent_kind: String,
    pub machine_ref: String,
    pub runtime_kind: String,
    pub repository_ref: String,
    pub runtime_session_ref: Option<String>,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
    /// Durable launch-claim state: `claimed`, `confirmed`, `ambiguous`, or null
    /// when no machine has claimed the job. A client needs this to say "waiting
    /// for an executor" honestly and to never retry an ambiguous launch.
    pub claim_state: Option<String>,
}

impl AgentJobView {
    /// Claim state is always supplied by the caller from the durable claim table;
    /// there is deliberately no `From<AgentJob>` because guessing `None` would
    /// misreport an in-flight launch.
    fn new(job: AgentJob, claim_state: Option<String>) -> Self {
        Self {
            id: job.id,
            request_id: job.request_id,
            project_id: job.project_id.as_str().into(),
            task_id: job.task_id.as_str().into(),
            state: job.state.as_str().into(),
            revision: job.revision,
            agent_kind: job.agent_kind,
            machine_ref: job.machine_ref,
            runtime_kind: job.runtime_kind,
            repository_ref: job.repository_ref,
            runtime_session_ref: job.runtime_session_ref,
            created_at_ms: job.created_at_ms,
            updated_at_ms: job.updated_at_ms,
            claim_state,
        }
    }
}

/// One enrolled device as a candidate agent executor.
#[derive(Debug, Clone, Serialize)]
pub struct AgentExecutorView {
    pub device_id: String,
    /// Enrolled and not revoked.
    pub active: bool,
    pub last_claim_at_ms: Option<i64>,
    /// Non-terminal jobs already in flight on that machine.
    pub claimed_jobs: i64,
    /// `preparing` jobs on that machine that nobody has claimed yet.
    pub preparing_jobs: i64,
}

/// An empty list is the honest answer for "no executor is configured"; the
/// client must present that as a state, not as an error.
#[derive(Debug, Clone, Serialize)]
pub struct AgentExecutorList {
    pub executors: Vec<AgentExecutorView>,
}

fn bad_input() -> AuthorityError {
    AuthorityError::Invalid("invalid agent job request")
}

/// Fixed refusal texts a person can act on. Transport mapping is an allowlist,
/// never `error.to_string()`: arbitrary storage errors can carry SQL, schema or
/// filesystem detail. Entries are constant and never interpolate ids or paths.
const AGENT_REFUSALS: &[&str] = &[
    // start
    "invalid connected agent start",
    "conflicting connected start replay",
    "task not eligible for connected start",
    "agent task not found",
    "task already has active agent",
    "agent task has unfinished prerequisites",
    // reserve
    "invalid initial agent job",
    "idempotency key already belongs to another job",
    "idempotency key reused with different launch payload",
    "reservation owner mismatch",
    "task project mismatch",
    "task must be ready, in progress, or in review",
    "task has an active agent reservation",
    // resume
    "invalid resume request",
    "predecessor missing",
    "predecessor owner mismatch",
    "conflicting resume replay",
    "predecessor still active",
    "predecessor launch is unconfirmed; reconcile or sweep the runtime before resuming",
    "resume requires handoff",
    "task context changed before resume",
    "resume requires a durable handoff",
    "resume must target the predecessor's task and project",
    "resume requires a queued successor and terminal predecessor",
    "conflicting resume context",
    // commands
    "invalid agent command",
    "unsupported agent command",
    "command identity reused with different arguments",
    "unknown agent job",
    "task context changed before prepare",
    "stale agent command revision",
    "invalid agent command transition",
    "command timestamp regression",
    // machine claim / launch result / observe / command ownership
    "empty executor identity",
    "executor machine mismatch",
    "empty launch token",
    "job already has a different launch claim",
    "task context changed before launch",
    "launch precondition failed",
    "empty session reference",
    "launch result not expected",
    "launch token mismatch or already confirmed",
    "empty reconciliation token",
    "missing launch claim",
    "reconciliation token mismatch",
    "stale reconciliation revision",
    "terminal agent cannot be reconciled",
    "stale runtime observation",
    "empty runtime session",
    "runtime session identity changed",
    "invalid runtime observation transition",
    // reviews and handoffs owned by the browser
    "review already recorded; conflicting decision",
    "invalid review request",
    "review requires a persisted handoff",
    "handoff ID reused with different contents",
];

/// Map a storage refusal to the transport error, preserving only allowlisted
/// fixed text. Everything else (including every SQL failure) keeps the generic
/// `invalid agent job request` body. The returned text is always a `'static`
/// entry from the allowlist, never the storage message itself.
fn storage_refusal(error: orbit_application::RepositoryError) -> AuthorityError {
    match error {
        orbit_application::RepositoryError::Other(message) => {
            match AGENT_REFUSALS.iter().find(|known| **known == message) {
                Some(known) => AuthorityError::Invalid(known),
                None => bad_input(),
            }
        }
        _ => bad_input(),
    }
}

fn validate_ref(value: &str) -> Result<(), AuthorityError> {
    if value.is_empty() || value.len() > 256 || value.chars().any(|c| c.is_control()) {
        return Err(bad_input());
    }
    Ok(())
}

/// Trimmed objective, or `None` when it is blank. The cap is a character count
/// so a long multi-byte objective is refused honestly rather than truncated.
fn validate_objective(objective: Option<String>) -> Result<Option<String>, AuthorityError> {
    let Some(objective) = objective else {
        return Ok(None);
    };
    let objective = objective.trim();
    if objective.is_empty() {
        return Ok(None);
    }
    if objective.chars().count() > 4000 {
        return Err(AuthorityError::Invalid("objective is too long"));
    }
    Ok(Some(objective.to_owned()))
}

impl AuthoritativeHost {
    /// One job view. The claim read is a single primary-key lookup.
    fn agent_job_view(&self, job: AgentJob) -> Result<AgentJobView, AuthorityError> {
        let claim = self
            .authority
            .conn
            .agent_job_claim_state(&job.id)
            .map_err(storage_refusal)?;
        Ok(AgentJobView::new(job, claim))
    }

    /// Job list views sharing one bounded claim query, so a list is never one
    /// claim lookup per row.
    fn agent_job_views(&self, jobs: Vec<AgentJob>) -> Result<Vec<AgentJobView>, AuthorityError> {
        let ids = jobs.iter().map(|job| job.id.as_str()).collect::<Vec<_>>();
        let claims: std::collections::HashMap<String, String> = self
            .authority
            .conn
            .agent_claim_states(&ids)
            .map_err(storage_refusal)?
            .into_iter()
            .collect();
        Ok(jobs
            .into_iter()
            .map(|job| {
                let claim = claims.get(&job.id).cloned();
                AgentJobView::new(job, claim)
            })
            .collect())
    }

    /// Owner-scoped executor roster. Any authenticated device may read it: it is
    /// derived from enrollment and durable jobs, and it is how the browser knows
    /// whether a launch is possible at all.
    pub fn agent_executors_authoritatively(
        &self,
        owner: &str,
    ) -> Result<AgentExecutorList, AuthorityError> {
        validate_ref(owner)?;
        let executors = self
            .authority
            .conn
            .agent_executors()
            .map_err(storage_refusal)?
            .into_iter()
            .map(|row| AgentExecutorView {
                device_id: row.device_id,
                active: row.active,
                last_claim_at_ms: row.last_claim_at_ms,
                claimed_jobs: row.claimed_jobs,
                preparing_jobs: row.preparing_jobs,
            })
            .collect();
        Ok(AgentExecutorList { executors })
    }
}
impl AuthoritativeHost {
    /// Read-only, durable resume context; never infers task acceptance from a runtime state.
    pub fn local_agent_resume_context(&self, id: &str) -> Result<serde_json::Value, String> {
        self.build_agent_resume_context(id)
            .map_err(|e| format!("resume context: {e:?}"))
    }

    fn build_agent_resume_context(&self, id: &str) -> Result<serde_json::Value, AuthorityError> {
        validate_ref(id)?;
        let job = self
            .authority
            .conn
            .get_agent_job(id)
            .map_err(|_| bad_input())?
            .ok_or_else(bad_input)?;
        let brief = self
            .authority
            .conn
            .agent_job_brief(id)
            .map_err(|_| bad_input())?
            .ok_or_else(bad_input)?;
        let brief: serde_json::Value = serde_json::from_str(&brief).map_err(|_| bad_input())?;
        let handoffs = self
            .authority
            .conn
            .list_agent_job_handoffs(id)
            .map_err(|_| bad_input())?;
        let latest = handoffs.last().cloned();
        let review = self
            .authority
            .conn
            .agent_job_review(id)
            .map_err(|_| bad_input())?;
        let worktree = self
            .authority
            .conn
            .agent_job_worktree(id)
            .map_err(|_| bad_input())?;
        Ok(
            serde_json::json!({"job":self.agent_job_view(job)?,"original_brief":brief,
            "latest_handoff":latest,"handoff_count":handoffs.len(),
            "review":review,"worktree":worktree,
            "task_completion_inferred":false,"runtime_launch_authorized":false}),
        )
    }

    /// Owner-scoped remote resume context. Client controls cannot bypass job ownership.
    pub fn agent_resume_context_authoritatively(
        &self,
        owner: &str,
        id: &str,
    ) -> Result<serde_json::Value, AuthorityError> {
        validate_ref(owner)?;
        validate_ref(id)?;
        self.authority
            .conn
            .require_agent_job_owner(owner, id)
            .map_err(|_| AuthorityError::Unauthorized)?;
        self.build_agent_resume_context(id)
    }

    /// One user-facing connected start, committed in one SQLite transaction.
    /// This only queues the trusted worker; no Herdr execution in the server.
    pub fn start_agent_job_authoritatively(
        &mut self,
        owner: &str,
        request: StartAgentJob,
    ) -> Result<AgentJobView, AuthorityError> {
        validate_ref(owner)?;
        for v in [
            &request.job_id,
            &request.task_id,
            &request.agent_kind,
            &request.machine_ref,
            &request.repository_ref,
        ] {
            validate_ref(v)?;
        }
        if request.at_ms < 0 || !matches!(request.agent_kind.as_str(), "codex" | "opencode") {
            return Err(bad_input());
        }
        let objective = validate_objective(request.objective)?;
        // Executor validation happens before the job transaction: a job assigned
        // to a device nobody will ever poll must not be created at all.
        match self
            .authority
            .conn
            .agent_executor_active(&request.machine_ref)
            .map_err(storage_refusal)?
        {
            Some(true) => {}
            Some(false) => return Err(AuthorityError::Invalid("inactive agent executor")),
            None => return Err(AuthorityError::Invalid("unknown agent executor")),
        }
        use rusqlite::OptionalExtension;
        let project: Option<String> = self
            .authority
            .conn
            .query_row(
                "SELECT project_id FROM tasks WHERE id=?1",
                [&request.task_id],
                |r| r.get(0),
            )
            .optional()
            .map_err(|_| bad_input())?
            .flatten();
        let project = project.ok_or_else(bad_input)?;
        let job = AgentJob {
            id: request.job_id.clone(),
            request_id: request.job_id,
            project_id: ProjectId::new(project).map_err(|_| bad_input())?,
            task_id: TaskId::new(request.task_id).map_err(|_| bad_input())?,
            state: orbit_domain::AgentJobState::Queued,
            revision: 0,
            launch_revision: 0,
            agent_kind: request.agent_kind,
            machine_ref: request.machine_ref,
            runtime_kind: "herdr".into(),
            repository_ref: request.repository_ref,
            created_at_ms: request.at_ms,
            updated_at_ms: request.at_ms,
            last_seen_at_ms: None,
            runtime_session_ref: None,
        };
        let started = self
            .authority
            .conn
            .start_agent_job_for_device(owner, &job, request.worktree, objective.as_deref())
            .map_err(storage_refusal)?;
        self.agent_job_view(started)
    }

    /// Connected resume is authority-only: the executor polls the newly queued
    /// job. The HTTP server never invokes Herdr or starts a shell process.
    pub fn resume_agent_job_authoritatively(
        &mut self,
        owner: &str,
        request: ResumeAgentJob,
    ) -> Result<AgentJobView, AuthorityError> {
        validate_ref(owner)?;
        validate_ref(&request.prior_job_id)?;
        validate_ref(&request.new_job_id)?;
        if request.at_ms < 0 || request.prior_job_id == request.new_job_id {
            return Err(bad_input());
        }
        self.authority
            .conn
            .require_agent_job_owner(owner, &request.prior_job_id)
            .map_err(|_| AuthorityError::Unauthorized)?;
        self.authority
            .conn
            .resume_agent_job_for_device(
                owner,
                &request.prior_job_id,
                &request.new_job_id,
                request.at_ms,
            )
            .map_err(storage_refusal)?;
        let job = self
            .authority
            .conn
            .get_agent_job(&request.new_job_id)
            .map_err(storage_refusal)?
            .ok_or_else(bad_input)?;
        self.agent_job_view(job)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_invalid_agent_identifiers() {
        assert!(validate_ref("").is_err());
        assert!(validate_ref("machine\nforged").is_err());
        assert!(validate_ref(&"x".repeat(257)).is_err());
        assert!(validate_ref("remote-01").is_ok());
    }
    #[test]
    fn rejects_invalid_revisions_before_storage_access() {
        let mut host = AuthoritativeHost::open(":memory:").unwrap();
        let command = AgentJobCommand {
            command_id: "cmd-1".into(),
            job_id: "job-1".into(),
            expected_revision: -1,
            action: "prepare".into(),
            at_ms: 100,
        };
        assert!(
            host.accept_agent_job_command_authoritatively("device-1", command)
                .is_err()
        );
    }
    #[test]
    fn unknown_job_does_not_create_runtime_session() {
        let host = AuthoritativeHost::open(":memory:").unwrap();
        assert!(host.agent_job_authoritative("device-1", "unknown").is_err());
        assert!(
            host.agent_jobs_needing_attention_authoritatively("device-1")
                .unwrap()
                .is_empty()
        );
    }
}
