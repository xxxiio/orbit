//! Local-authority Herdr monitoring. Polling never completes an Orbit task.
#[cfg(test)]
use orbit_application::agent_runtime::AgentMonitoringContext;
use orbit_application::agent_runtime::{
    AgentActivity, AgentInspector, AgentObservationAuthority, AgentObservedSession,
    AgentSessionObservation,
};
use orbit_domain::AgentJob;

/// Poll an existing claimed launch; never create a new launch claim or session.
/// A stale caller must not overwrite a newer observation.
pub fn reconcile_job(
    store: &mut impl AgentObservationAuthority,
    runtime: &mut impl AgentInspector,
    job_id: &str,
    expected_revision: i64,
    at_ms: i64,
) -> Result<AgentJob, String> {
    let context = store.monitoring_context(job_id)?;
    let job = context.job;
    if job.state.is_terminal() {
        return Err("terminal job cannot be polled".into());
    }
    if job.revision != expected_revision {
        return Err("stale poll revision".into());
    }
    let token = context
        .claim_token
        .ok_or_else(|| "no launch claim to inspect".to_string())?;
    let observation = match runtime.inspect(&job.machine_ref, &token) {
        Ok(AgentSessionObservation::Present {
            session_ref,
            runtime_state,
        }) => {
            let activity = match runtime_state.to_ascii_lowercase().as_str() {
                "running" | "working" | "active" | "busy" => AgentActivity::Working,
                "blocked" | "error" => AgentActivity::Blocked,
                "waiting_input" | "waiting-for-input" | "needs_input" => {
                    AgentActivity::WaitingInput
                }
                // "done" is an agent/session status, not task acceptance.
                "done" | "idle" | "completed" | "finished" | "unknown" => AgentActivity::Idle,
                _ => AgentActivity::Idle,
            };
            AgentObservedSession::Present {
                session_ref,
                activity,
            }
        }
        Ok(AgentSessionObservation::AbsentConfirmed) => AgentObservedSession::AbsentConfirmed,
        Ok(AgentSessionObservation::Unreachable) | Err(_) => AgentObservedSession::Unreachable,
    };
    store.record_observation(job_id, expected_revision, &token, observation, at_ms)
}

#[cfg(test)]
mod tests {
    #[test]
    fn agent_idleness_is_not_acceptance() {
        assert!(!orbit_domain::AgentJobState::WaitingInput.is_terminal());
        assert!(orbit_domain::AgentJobState::Completed.is_terminal());
    }
}

#[cfg(test)]
mod boundary_tests {
    use super::*;
    use orbit_application::agent_runtime::{AgentLaunchOutcome, AgentLaunchRequest, AgentRuntime};
    use orbit_domain::{AgentJobState, ProjectId, TaskId};

    struct MemoryObservationAuthority {
        job: AgentJob,
        claim: Option<String>,
        observed: Vec<AgentObservedSession>,
    }
    impl AgentObservationAuthority for MemoryObservationAuthority {
        fn monitoring_context(&self, _: &str) -> Result<AgentMonitoringContext, String> {
            Ok(AgentMonitoringContext {
                job: self.job.clone(),
                claim_token: self.claim.clone(),
            })
        }
        fn record_observation(
            &mut self,
            _: &str,
            revision: i64,
            token: &str,
            observation: AgentObservedSession,
            _: i64,
        ) -> Result<AgentJob, String> {
            if revision != self.job.revision {
                return Err("stale revision".into());
            }
            if Some(token) != self.claim.as_deref() {
                return Err("wrong token".into());
            }
            self.observed.push(observation);
            self.job.revision += 1;
            Ok(self.job.clone())
        }
    }
    struct StubRuntime {
        inspect_calls: usize,
        result: AgentSessionObservation,
    }
    impl AgentRuntime for StubRuntime {
        fn launch(&mut self, _: &AgentLaunchRequest) -> Result<AgentLaunchOutcome, String> {
            panic!("monitoring must never launch a session")
        }
        fn inspect(&mut self, _: &str, _: &str) -> Result<AgentSessionObservation, String> {
            self.inspect_calls += 1;
            Ok(self.result.clone())
        }
    }
    fn authority() -> MemoryObservationAuthority {
        MemoryObservationAuthority {
            job: AgentJob {
                id: "job".into(),
                request_id: "request".into(),
                project_id: ProjectId::new("project").unwrap(),
                task_id: TaskId::new("task").unwrap(),
                launch_revision: 0,
                agent_kind: "codex".into(),
                machine_ref: "local".into(),
                runtime_kind: "herdr".into(),
                repository_ref: "repo".into(),
                state: AgentJobState::Running,
                revision: 3,
                created_at_ms: 1,
                updated_at_ms: 2,
                last_seen_at_ms: None,
                runtime_session_ref: Some("session".into()),
            },
            claim: Some("claim".into()),
            observed: vec![],
        }
    }
    #[test]
    fn stale_poll_does_not_contact_runtime() {
        let mut authority = authority();
        let mut runtime = StubRuntime {
            inspect_calls: 0,
            result: AgentSessionObservation::Unreachable,
        };
        assert!(reconcile_job(&mut authority, &mut runtime, "job", 2, 10).is_err());
        assert_eq!(runtime.inspect_calls, 0);
        assert!(authority.observed.is_empty());
    }
    #[test]
    fn missing_claim_never_contacts_runtime() {
        let mut authority = authority();
        authority.claim = None;
        let mut runtime = StubRuntime {
            inspect_calls: 0,
            result: AgentSessionObservation::Unreachable,
        };
        assert!(reconcile_job(&mut authority, &mut runtime, "job", 3, 10).is_err());
        assert_eq!(runtime.inspect_calls, 0);
    }
    #[test]
    fn runtime_idle_preserves_review_boundary() {
        let mut authority = authority();
        let mut runtime = StubRuntime {
            inspect_calls: 0,
            result: AgentSessionObservation::Present {
                session_ref: "session".into(),
                runtime_state: "done".into(),
            },
        };
        let job = reconcile_job(&mut authority, &mut runtime, "job", 3, 10).unwrap();
        assert_eq!(job.revision, 4);
        assert_eq!(
            authority.observed,
            vec![AgentObservedSession::Present {
                session_ref: "session".into(),
                activity: AgentActivity::Idle,
            }]
        );
        assert_eq!(runtime.inspect_calls, 1);
    }
    #[test]
    fn connection_failure_does_not_mark_session_absent() {
        let mut authority = authority();
        let mut runtime = StubRuntime {
            inspect_calls: 0,
            result: AgentSessionObservation::Unreachable,
        };
        reconcile_job(&mut authority, &mut runtime, "job", 3, 10).unwrap();
        assert_eq!(authority.observed, vec![AgentObservedSession::Unreachable]);
    }
}
