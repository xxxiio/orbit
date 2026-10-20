//! Authenticated transport adapters call this host boundary; they must not
//! open a second SQLite connection or issue runtime side effects themselves.
use crate::{AuthoritativeHost, AuthorityError};
use orbit_domain::{AgentJob, ProjectId, TaskId};
use serde::{Deserialize, Serialize};

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
}
impl From<AgentJob> for AgentJobView {
    fn from(j: AgentJob) -> Self {
        Self {
            id: j.id,
            request_id: j.request_id,
            project_id: j.project_id.as_str().into(),
            task_id: j.task_id.as_str().into(),
            state: j.state.as_str().into(),
            revision: j.revision,
            agent_kind: j.agent_kind,
            machine_ref: j.machine_ref,
            runtime_kind: j.runtime_kind,
            repository_ref: j.repository_ref,
            runtime_session_ref: j.runtime_session_ref,
            created_at_ms: j.created_at_ms,
            updated_at_ms: j.updated_at_ms,
        }
    }
}
fn bad_input() -> AuthorityError {
    AuthorityError::Invalid("invalid agent job request")
}
fn validate_ref(value: &str) -> Result<(), AuthorityError> {
    if value.is_empty() || value.len() > 256 || value.chars().any(|c| c.is_control()) {
        return Err(bad_input());
    }
    Ok(())
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
            serde_json::json!({"job":AgentJobView::from(job),"original_brief":brief,
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
        self.authority
            .conn
            .start_agent_job_for_device(owner, &job, request.worktree)
            .map(Into::into)
            .map_err(|_| bad_input())
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
            .map_err(|_| bad_input())?;
        self.authority
            .conn
            .get_agent_job(&request.new_job_id)
            .map_err(|_| bad_input())?
            .map(Into::into)
            .ok_or_else(bad_input)
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
