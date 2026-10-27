//! Machine-specific authority operations. Authentication is performed by the transport;
//! these operations enforce assigned-machine identity before executing side effects.
use super::*;

impl AuthoritativeHost {
    pub fn queue_agent_prompt_authoritatively(
        &mut self,
        owner: &str,
        request: QueueSessionPrompt,
    ) -> Result<serde_json::Value, AuthorityError> {
        validate_ref(owner)?;
        validate_ref(&request.job_id)?;
        validate_ref(&request.command_id)?;
        self.authority
            .conn
            .queue_agent_session_prompt(
                owner,
                &request.command_id,
                &request.job_id,
                &request.message,
                request.at_ms,
            )
            .map_err(|_| bad_input())
    }
    pub fn machine_prompt_inbox_authoritatively(
        &self,
        machine: &str,
    ) -> Result<Vec<serde_json::Value>, AuthorityError> {
        validate_ref(machine)?;
        self.authority
            .conn
            .agent_session_prompt_inbox(machine)
            .map_err(|_| bad_input())
    }
    pub fn machine_claim_prompt_authoritatively(
        &mut self,
        machine: &str,
        request: ClaimSessionPrompt,
    ) -> Result<serde_json::Value, AuthorityError> {
        validate_ref(machine)?;
        validate_ref(&request.command_id)?;
        let claimed = self
            .authority
            .conn
            .claim_agent_session_prompt(machine, &request.command_id, request.at_ms)
            .map_err(|_| bad_input())?;
        Ok(serde_json::json!({"command_id":request.command_id,"claimed":claimed}))
    }
    pub fn machine_report_prompt_authoritatively(
        &mut self,
        machine: &str,
        request: ReportSessionPrompt,
    ) -> Result<serde_json::Value, AuthorityError> {
        validate_ref(machine)?;
        validate_ref(&request.command_id)?;
        self.authority
            .conn
            .report_agent_session_prompt(
                machine,
                &request.command_id,
                request.delivered,
                request.at_ms,
            )
            .map_err(|_| bad_input())?;
        Ok(serde_json::json!({"command_id":request.command_id,"recorded":true}))
    }

    /// Inbox is scoped to the device authenticated by the HTTP transport.
    pub fn machine_agent_inbox_authoritatively(
        &self,
        machine: &str,
    ) -> Result<Vec<AgentJobView>, AuthorityError> {
        validate_ref(machine)?;
        let jobs = self
            .authority
            .conn
            .pending_agent_jobs_for_machine(machine)
            .map_err(storage_refusal)?;
        self.agent_job_views(jobs)
    }
    /// Work instructions for the assigned executor; owner clients may not impersonate machines.
    pub fn machine_agent_brief_authoritatively(
        &self,
        machine: &str,
        job_id: &str,
    ) -> Result<serde_json::Value, AuthorityError> {
        validate_ref(machine)?;
        validate_ref(job_id)?;
        let job = self
            .authority
            .conn
            .get_agent_job(job_id)
            .map_err(|_| bad_input())?
            .ok_or_else(bad_input)?;
        if job.machine_ref != machine {
            return Err(AuthorityError::Unauthorized);
        }
        let brief = self
            .authority
            .conn
            .agent_job_brief(job_id)
            .map_err(|_| bad_input())?
            .ok_or_else(bad_input)?;
        let context: serde_json::Value = serde_json::from_str(&brief).map_err(|_| bad_input())?;
        let worktree = self
            .authority
            .conn
            .agent_job_worktree(job_id)
            .map_err(|_| bad_input())?;
        Ok(serde_json::json!({"job":self.agent_job_view(job)?,"brief":context,"worktree":worktree}))
    }
    pub fn machine_agent_claim_authoritatively(
        &mut self,
        machine: &str,
        command: ClaimMachineAgentJob,
    ) -> Result<MachineLaunchClaimReceipt, AuthorityError> {
        validate_ref(machine)?;
        validate_ref(&command.job_id)?;
        validate_ref(&command.claim_token)?;
        if command.expected_revision < 0 || command.at_ms < 0 {
            return Err(bad_input());
        }
        let claimed = self
            .authority
            .conn
            .claim_agent_launch_for_machine(
                machine,
                &command.job_id,
                command.expected_revision,
                &command.claim_token,
                command.at_ms,
            )
            .map_err(storage_refusal)?;
        let job = self
            .authority
            .conn
            .get_agent_job(&command.job_id)
            .map_err(storage_refusal)?
            .ok_or_else(bad_input)?;
        Ok(MachineLaunchClaimReceipt {
            job: self.agent_job_view(job)?,
            claimed,
        })
    }
    pub fn machine_agent_launch_result_authoritatively(
        &mut self,
        machine: &str,
        command: ReportMachineAgentLaunch,
    ) -> Result<AgentJobView, AuthorityError> {
        validate_ref(machine)?;
        validate_ref(&command.job_id)?;
        validate_ref(&command.claim_token)?;
        if command.at_ms < 0 {
            return Err(bad_input());
        }
        if let Some(ref session) = command.session_ref {
            validate_ref(session)?;
        }
        self.authority
            .conn
            .record_agent_launch_for_machine(
                machine,
                &command.job_id,
                &command.claim_token,
                command.session_ref.as_deref(),
                command.at_ms,
            )
            .map_err(storage_refusal)?;
        let job = self
            .authority
            .conn
            .get_agent_job(&command.job_id)
            .map_err(storage_refusal)?
            .ok_or_else(bad_input)?;
        self.agent_job_view(job)
    }
    pub fn machine_agent_observe_authoritatively(
        &mut self,
        machine: &str,
        command: ReportMachineAgentObservation,
    ) -> Result<AgentJobView, AuthorityError> {
        use orbit_storage::agent_jobs::{AgentRuntimeObservation, RuntimeActivity};
        validate_ref(machine)?;
        validate_ref(&command.job_id)?;
        validate_ref(&command.claim_token)?;
        if command.expected_revision < 0 || command.at_ms < 0 {
            return Err(bad_input());
        }
        let observation = match command.activity.as_str() {
            "unreachable" => AgentRuntimeObservation::Unreachable,
            "absent" => AgentRuntimeObservation::AbsentConfirmed,
            "working" | "waiting_input" | "blocked" | "idle" => {
                let session = command.session_ref.ok_or_else(bad_input)?;
                validate_ref(&session)?;
                let state = match command.activity.as_str() {
                    "working" => RuntimeActivity::Working,
                    "waiting_input" => RuntimeActivity::WaitingInput,
                    "blocked" => RuntimeActivity::Blocked,
                    _ => RuntimeActivity::Idle,
                };
                AgentRuntimeObservation::Present {
                    session_ref: session,
                    state,
                }
            }
            _ => return Err(bad_input()),
        };
        let observed = self
            .authority
            .conn
            .observe_agent_for_machine(
                machine,
                &command.job_id,
                command.expected_revision,
                &command.claim_token,
                observation,
                command.at_ms,
            )
            .map_err(storage_refusal)?;
        self.agent_job_view(observed)
    }

    /// Only the assigned machine with the persisted launch token may submit
    /// evidence. This does not grant the machine authority to approve work.
    pub fn machine_agent_handoff_authoritatively(
        &mut self,
        machine: &str,
        command: ReportMachineAgentHandoff,
    ) -> Result<Vec<serde_json::Value>, AuthorityError> {
        validate_ref(machine)?;
        validate_ref(&command.job_id)?;
        validate_ref(&command.claim_token)?;
        validate_ref(&command.handoff_id)?;
        if command.at_ms < 0 {
            return Err(bad_input());
        }
        let job = self
            .authority
            .conn
            .get_agent_job(&command.job_id)
            .map_err(|_| bad_input())?
            .ok_or_else(bad_input)?;
        if job.machine_ref != machine {
            return Err(AuthorityError::Unauthorized);
        }
        let claim = self
            .authority
            .conn
            .agent_launch_claim(&command.job_id)
            .map_err(|_| bad_input())?
            .ok_or_else(bad_input)?;
        if claim.0 != command.claim_token
            || claim.2.as_deref() != Some(command.session_ref.as_str())
        {
            return Err(AuthorityError::Unauthorized);
        }
        self.authority
            .conn
            .record_agent_job_handoff(
                &command.job_id,
                &command.handoff_id,
                &command.payload,
                command.at_ms,
            )
            .map_err(|_| bad_input())?;
        self.authority
            .conn
            .list_agent_job_handoffs(&command.job_id)
            .map_err(|_| bad_input())
    }
}
