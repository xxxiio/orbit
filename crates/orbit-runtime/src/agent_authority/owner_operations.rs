//! Device-owned agent-job commands and read models. Ownership is checked
//! before accessing job state; no executor side effects occur here.
use super::*;

impl AuthoritativeHost {
    /// Reservation is persisted before any executor receives permission to start.
    pub fn reserve_agent_job_authoritatively(
        &mut self,
        owner: &str,
        command: ReserveAgentJob,
    ) -> Result<AgentJobView, AuthorityError> {
        for v in [
            &command.job_id,
            &command.request_id,
            &command.project_id,
            &command.task_id,
            &command.agent_kind,
            &command.machine_ref,
            &command.runtime_kind,
            &command.repository_ref,
        ] {
            validate_ref(v)?;
        }
        if command.created_at_ms < 0 || command.launch_revision < 0 {
            return Err(bad_input());
        }
        validate_ref(owner)?;
        let job = AgentJob {
            id: command.job_id,
            request_id: command.request_id,
            project_id: ProjectId::new(command.project_id).map_err(|_| bad_input())?,
            task_id: TaskId::new(command.task_id).map_err(|_| bad_input())?,
            agent_kind: command.agent_kind,
            machine_ref: command.machine_ref,
            runtime_kind: command.runtime_kind,
            repository_ref: command.repository_ref,
            launch_revision: command.launch_revision,
            state: orbit_domain::AgentJobState::Queued,
            revision: 0,
            created_at_ms: command.created_at_ms,
            updated_at_ms: command.created_at_ms,
            last_seen_at_ms: None,
            runtime_session_ref: None,
        };
        let reserved = self
            .authority
            .conn
            .reserve_agent_job_for_device(owner, &job)
            .map_err(storage_refusal)?;
        self.agent_job_view(reserved)
    }
    /// Only explicitly supported authority-local transitions are exposed.
    /// Agent completion never updates task status.
    pub fn accept_agent_job_command_authoritatively(
        &mut self,
        owner: &str,
        command: AgentJobCommand,
    ) -> Result<AgentJobView, AuthorityError> {
        validate_ref(&command.command_id)?;
        validate_ref(&command.job_id)?;
        if command.expected_revision < 0 || command.at_ms < 0 {
            return Err(bad_input());
        }
        validate_ref(owner)?;
        self.authority
            .conn
            .require_agent_job_owner(owner, &command.job_id)
            .map_err(|_| AuthorityError::Unauthorized)?;
        let accepted = self
            .authority
            .conn
            .accept_agent_job_command(
                &command.command_id,
                &command.job_id,
                command.expected_revision,
                &command.action,
                command.at_ms,
            )
            .map_err(storage_refusal)?;
        self.agent_job_view(accepted)
    }
    pub fn agent_job_authoritative(
        &self,
        owner: &str,
        id: &str,
    ) -> Result<Option<AgentJobView>, AuthorityError> {
        validate_ref(id)?;
        validate_ref(owner)?;
        self.authority
            .conn
            .require_agent_job_owner(owner, id)
            .map_err(|_| AuthorityError::Unauthorized)?;
        match self
            .authority
            .conn
            .get_agent_job(id)
            .map_err(storage_refusal)?
        {
            Some(job) => self.agent_job_view(job).map(Some),
            None => Ok(None),
        }
    }
    /// Connected clients see only the handoffs of jobs owned by their device.
    pub fn agent_handoffs_authoritatively(
        &self,
        owner: &str,
        id: &str,
    ) -> Result<Vec<serde_json::Value>, AuthorityError> {
        validate_ref(owner)?;
        validate_ref(id)?;
        self.authority
            .conn
            .require_agent_job_owner(owner, id)
            .map_err(|_| AuthorityError::Unauthorized)?;
        self.authority
            .conn
            .list_agent_job_handoffs(id)
            .map_err(storage_refusal)
    }
    pub fn submit_agent_handoff_authoritatively(
        &mut self,
        owner: &str,
        command: SubmitAgentHandoff,
    ) -> Result<Vec<serde_json::Value>, AuthorityError> {
        validate_ref(owner)?;
        validate_ref(&command.job_id)?;
        validate_ref(&command.handoff_id)?;
        if command.at_ms < 0 {
            return Err(bad_input());
        }
        self.authority
            .conn
            .require_agent_job_owner(owner, &command.job_id)
            .map_err(|_| AuthorityError::Unauthorized)?;
        self.authority
            .conn
            .record_agent_job_handoff(
                &command.job_id,
                &command.handoff_id,
                &command.payload,
                command.at_ms,
            )
            .map_err(storage_refusal)?;
        self.authority
            .conn
            .list_agent_job_handoffs(&command.job_id)
            .map_err(storage_refusal)
    }
    /// The authenticated owner is the reviewer identity; callers cannot forge it.
    pub fn review_agent_handoff_authoritatively(
        &mut self,
        owner: &str,
        command: ReviewAgentHandoff,
    ) -> Result<serde_json::Value, AuthorityError> {
        validate_ref(owner)?;
        validate_ref(&command.job_id)?;
        validate_ref(&command.handoff_id)?;
        if command.at_ms < 0 {
            return Err(bad_input());
        }
        self.authority
            .conn
            .require_agent_job_owner(owner, &command.job_id)
            .map_err(|_| AuthorityError::Unauthorized)?;
        self.authority
            .conn
            .review_agent_job(
                &command.job_id,
                &command.handoff_id,
                &command.decision,
                owner,
                &command.reason,
                command.at_ms,
            )
            .map_err(storage_refusal)
    }
    pub fn agent_review_authoritatively(
        &self,
        owner: &str,
        id: &str,
    ) -> Result<Option<serde_json::Value>, AuthorityError> {
        validate_ref(owner)?;
        validate_ref(id)?;
        self.authority
            .conn
            .require_agent_job_owner(owner, id)
            .map_err(|_| AuthorityError::Unauthorized)?;
        self.authority
            .conn
            .agent_job_review(id)
            .map_err(storage_refusal)
    }
    pub fn agent_overview_authoritatively(
        &self,
        owner: &str,
    ) -> Result<Vec<serde_json::Value>, AuthorityError> {
        validate_ref(owner)?;
        let items = self
            .authority
            .conn
            .agent_job_overview()
            .map_err(|_| bad_input())?;
        Ok(items
            .into_iter()
            .filter(|item| {
                item.get("job_id")
                    .or_else(|| item.get("id"))
                    .and_then(serde_json::Value::as_str)
                    .is_some_and(|id| {
                        self.authority
                            .conn
                            .require_agent_job_owner(owner, id)
                            .is_ok()
                    })
            })
            .collect())
    }
    pub fn agent_jobs_needing_attention_authoritatively(
        &self,
        owner: &str,
    ) -> Result<Vec<AgentJobView>, AuthorityError> {
        validate_ref(owner)?;
        let jobs = self
            .authority
            .conn
            .list_agent_jobs_needing_attention()
            .map_err(storage_refusal)?;
        let mut visible = Vec::new();
        for job in jobs {
            if self
                .authority
                .conn
                .require_agent_job_owner(owner, &job.id)
                .is_ok()
            {
                visible.push(job);
            }
        }
        self.agent_job_views(visible)
    }
}
