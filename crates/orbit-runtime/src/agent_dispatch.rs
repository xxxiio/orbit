//! Machine-side dispatch coordinator. The caller must authorize the executor
//! and task before invoking this; server HTTP handlers never run subprocesses.
#[cfg(test)]
use orbit_application::agent_runtime::AgentLaunchContext;
use orbit_application::agent_runtime::{
    AgentLaunchAuthority, AgentLaunchOutcome, AgentLaunchRequest, AgentLauncher,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DispatchResult {
    /// Durable claim already exists: must inspect, never run launch again.
    AlreadyClaimed,
    Confirmed(String),
    NeedsReconciliation(String),
}

/// Exactly one launch attempt per durable claim. Crashes between the claim and
/// runtime invocation leave a claim for operator reconciliation; they do not
/// trigger another launch. The caller must ensure the local store is the
/// authoritative store (or use an authenticated executor lease in server mode).
pub fn dispatch_prepared_job(
    store: &mut impl AgentLaunchAuthority,
    runtime: &mut impl AgentLauncher,
    job_id: &str,
    expected_revision: i64,
    launch_token: &str,
    at_ms: i64,
) -> Result<DispatchResult, String> {
    let claimed = store.claim_launch(job_id, expected_revision, launch_token, at_ms)?;
    if !claimed {
        return Ok(DispatchResult::AlreadyClaimed);
    }
    let context = store.launch_context(job_id)?;
    let job = &context.job;
    if job.runtime_kind != "herdr" {
        store.record_launch_outcome(job_id, launch_token, None, at_ms)?;
        return Ok(DispatchResult::NeedsReconciliation(
            "unsupported runtime".into(),
        ));
    }
    let request =
        AgentLaunchRequest::from_reserved(job, launch_token, context.brief_json, context.worktree)?;
    // Even a transport error after invoking a runtime is ambiguous; do not
    // assume no side effect. Reconcile via Herdr before further actions.
    let outcome = runtime
        .launch(&request)
        .unwrap_or_else(|e| AgentLaunchOutcome::Ambiguous { diagnostic: e });
    match outcome {
        AgentLaunchOutcome::Confirmed { session_ref } => {
            store.record_launch_outcome(job_id, launch_token, Some(&session_ref), at_ms)?;
            Ok(DispatchResult::Confirmed(session_ref))
        }
        AgentLaunchOutcome::Ambiguous { diagnostic }
        | AgentLaunchOutcome::Rejected { diagnostic } => {
            store.record_launch_outcome(job_id, launch_token, None, at_ms)?;
            Ok(DispatchResult::NeedsReconciliation(diagnostic))
        }
    }
}

#[cfg(test)]
mod boundary_tests {
    use super::*;
    use orbit_application::agent_runtime::{AgentRuntime, AgentSessionObservation};
    use orbit_domain::{AgentJob, AgentJobState, ProjectId, TaskId};

    struct MemoryAuthority {
        claimed: bool,
        outcomes: Vec<Option<String>>,
    }
    impl AgentLaunchAuthority for MemoryAuthority {
        fn claim_launch(&mut self, _: &str, _: i64, _: &str, _: i64) -> Result<bool, String> {
            if self.claimed {
                Ok(false)
            } else {
                self.claimed = true;
                Ok(true)
            }
        }
        fn launch_context(&self, _: &str) -> Result<AgentLaunchContext, String> {
            Ok(AgentLaunchContext {
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
                    state: AgentJobState::Preparing,
                    revision: 1,
                    created_at_ms: 0,
                    updated_at_ms: 0,
                    last_seen_at_ms: None,
                    runtime_session_ref: None,
                },
                brief_json: "{}".into(),
                worktree: true,
            })
        }
        fn record_launch_outcome(
            &mut self,
            _: &str,
            _: &str,
            session: Option<&str>,
            _: i64,
        ) -> Result<(), String> {
            self.outcomes.push(session.map(str::to_owned));
            Ok(())
        }
    }
    struct FakeRuntime {
        launches: usize,
        ambiguous: bool,
    }
    impl AgentRuntime for FakeRuntime {
        fn launch(&mut self, _: &AgentLaunchRequest) -> Result<AgentLaunchOutcome, String> {
            self.launches += 1;
            if self.ambiguous {
                Ok(AgentLaunchOutcome::Ambiguous {
                    diagnostic: "timeout".into(),
                })
            } else {
                Ok(AgentLaunchOutcome::Confirmed {
                    session_ref: "session".into(),
                })
            }
        }
        fn inspect(&mut self, _: &str, _: &str) -> Result<AgentSessionObservation, String> {
            Ok(AgentSessionObservation::Unreachable)
        }
    }
    #[test]
    fn duplicate_dispatch_does_not_invoke_runtime_twice() {
        let mut authority = MemoryAuthority {
            claimed: false,
            outcomes: vec![],
        };
        let mut runtime = FakeRuntime {
            launches: 0,
            ambiguous: false,
        };
        assert_eq!(
            dispatch_prepared_job(&mut authority, &mut runtime, "job", 1, "token", 10).unwrap(),
            DispatchResult::Confirmed("session".into())
        );
        assert_eq!(
            dispatch_prepared_job(&mut authority, &mut runtime, "job", 1, "token", 11).unwrap(),
            DispatchResult::AlreadyClaimed
        );
        assert_eq!(runtime.launches, 1);
        assert_eq!(authority.outcomes, vec![Some("session".to_string())]);
    }
    #[test]
    fn ambiguous_outcome_is_not_relaunched() {
        let mut authority = MemoryAuthority {
            claimed: false,
            outcomes: vec![],
        };
        let mut runtime = FakeRuntime {
            launches: 0,
            ambiguous: true,
        };
        assert!(matches!(
            dispatch_prepared_job(&mut authority, &mut runtime, "job", 1, "token", 10),
            Ok(DispatchResult::NeedsReconciliation(_))
        ));
        assert_eq!(
            dispatch_prepared_job(&mut authority, &mut runtime, "job", 1, "token", 11).unwrap(),
            DispatchResult::AlreadyClaimed
        );
        assert_eq!(runtime.launches, 1);
        assert_eq!(authority.outcomes, vec![None]);
    }
}
