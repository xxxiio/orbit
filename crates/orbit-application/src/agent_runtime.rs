//! Optional agent execution boundary. Domain state is persisted by Orbit before
//! any external runtime call. Implementations must not mutate Orbit task state.
use orbit_domain::AgentJob;

/// Request built from an authority-captured brief, never from untrusted client prose.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentLaunchRequest {
    pub job_id: String,
    pub claim_token: String,
    pub agent_kind: String,
    pub machine_ref: String,
    pub repository_ref: String,
    pub brief_json: String,
    pub worktree: bool,
}

impl AgentLaunchRequest {
    pub fn from_reserved(
        job: &AgentJob,
        claim_token: &str,
        brief_json: String,
        worktree: bool,
    ) -> Result<Self, String> {
        if claim_token.trim().is_empty() || brief_json.trim().is_empty() {
            return Err("launch token and captured brief are required".into());
        }
        Ok(Self {
            job_id: job.id.clone(),
            claim_token: claim_token.into(),
            agent_kind: job.agent_kind.clone(),
            machine_ref: job.machine_ref.clone(),
            repository_ref: job.repository_ref.clone(),
            brief_json,
            worktree,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentLaunchOutcome {
    /// The runtime has positively identified the session.
    Confirmed { session_ref: String },
    /// A timeout or transport loss is NOT proof the launch failed.
    Ambiguous { diagnostic: String },
    /// Rejection is confirmed to have happened before any launch side effect.
    Rejected { diagnostic: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentSessionObservation {
    Present {
        session_ref: String,
        runtime_state: String,
    },
    Unreachable,
    AbsentConfirmed,
}

/// Complete immutable dispatch input, read from one authoritative snapshot.
/// Consumers cannot accidentally combine brief, job and policy from different reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentLaunchContext {
    pub job: AgentJob,
    pub brief_json: String,
    pub worktree: bool,
}

/// Monitor input captured from one authority snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentMonitoringContext {
    pub job: AgentJob,
    pub claim_token: Option<String>,
}

/// Authority-owned launch repository port. A claim is durable *before* the
/// runtime is called; adapters must serialize claims and task-readiness checks
/// in one authoritative transaction. The launch coordinator never opens a DB.
///
/// This deliberately does not expose arbitrary SQL or replica writes.
pub trait AgentLaunchAuthority {
    fn claim_launch(
        &mut self,
        job_id: &str,
        revision: i64,
        token: &str,
        at_ms: i64,
    ) -> Result<bool, String>;
    fn launch_context(&self, job_id: &str) -> Result<AgentLaunchContext, String>;
    fn record_launch_outcome(
        &mut self,
        job_id: &str,
        token: &str,
        session_ref: Option<&str>,
        at_ms: i64,
    ) -> Result<(), String>;
}

/// Authority-independent monitor observation. Infrastructure adapters convert
/// this value into their own persistence representation; the coordinator never
/// imports SQLite types or creates its own connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentActivity {
    Working,
    WaitingInput,
    Blocked,
    Idle,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentObservedSession {
    Present {
        session_ref: String,
        activity: AgentActivity,
    },
    Unreachable,
    AbsentConfirmed,
}

/// Read/compare-and-swap operations required for monitoring one claimed job.
/// An implementation must enforce revision and claim-token checks atomically.
pub trait AgentObservationAuthority {
    fn monitoring_context(&self, job_id: &str) -> Result<AgentMonitoringContext, String>;
    fn record_observation(
        &mut self,
        job_id: &str,
        revision: i64,
        token: &str,
        observation: AgentObservedSession,
        at_ms: i64,
    ) -> Result<AgentJob, String>;
}

/// Launch-only runtime capability. Dispatch does not need an inspection API.
/// Existing `AgentRuntime` implementations are supported through the blanket
/// adapter below while concrete runtime providers migrate independently.
pub trait AgentLauncher {
    fn launch(&mut self, request: &AgentLaunchRequest) -> Result<AgentLaunchOutcome, String>;
}

/// Observation-only runtime capability. Pollers cannot launch sessions through
/// this port, even when the concrete runtime supports both operations.
pub trait AgentInspector {
    fn inspect(
        &mut self,
        machine_ref: &str,
        claim_token: &str,
    ) -> Result<AgentSessionObservation, String>;
}

// Compatibility bridge: existing runtimes still implement AgentRuntime. The
// coordinators now ask only for the capability they actually exercise.
impl<T: AgentRuntime + ?Sized> AgentLauncher for T {
    fn launch(&mut self, request: &AgentLaunchRequest) -> Result<AgentLaunchOutcome, String> {
        AgentRuntime::launch(self, request)
    }
}
impl<T: AgentRuntime + ?Sized> AgentInspector for T {
    fn inspect(
        &mut self,
        machine_ref: &str,
        claim_token: &str,
    ) -> Result<AgentSessionObservation, String> {
        AgentRuntime::inspect(self, machine_ref, claim_token)
    }
}

/// Optional continuation capability. Sending a prompt can have side effects
/// despite losing its acknowledgement; callers must not automatically replay
/// ambiguous delivery failures.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentMessageOutcome {
    Delivered,
    Ambiguous { diagnostic: String },
}

pub trait AgentMessenger {
    fn send_message(
        &mut self,
        machine_ref: &str,
        session_ref: &str,
        message: &str,
    ) -> Result<AgentMessageOutcome, String>;
}

/// Swappable boundary for Herdr, test doubles and future runtimes.
/// The caller must claim a launch in authoritative storage first. Never retry
/// an ambiguous outcome with a fresh token: reconcile the existing token.
pub trait AgentRuntime {
    fn launch(&mut self, request: &AgentLaunchRequest) -> Result<AgentLaunchOutcome, String>;
    fn inspect(
        &mut self,
        machine_ref: &str,
        claim_token: &str,
    ) -> Result<AgentSessionObservation, String>;
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_blank_launch_claim() {
        let job = AgentJob {
            id: "job".into(),
            request_id: "request".into(),
            project_id: orbit_domain::ProjectId::new("project").unwrap(),
            task_id: orbit_domain::TaskId::new("task").unwrap(),
            launch_revision: 0,
            agent_kind: "codex".into(),
            machine_ref: "local".into(),
            runtime_kind: "herdr".into(),
            repository_ref: "repo".into(),
            state: orbit_domain::AgentJobState::Preparing,
            revision: 1,
            created_at_ms: 0,
            updated_at_ms: 0,
            last_seen_at_ms: None,
            runtime_session_ref: None,
        };
        assert!(AgentLaunchRequest::from_reserved(&job, "  ", "{}".into(), false).is_err());
        assert_eq!(
            AgentLaunchRequest::from_reserved(&job, "token", "{}".into(), false)
                .unwrap()
                .job_id,
            "job"
        );
    }
}

#[cfg(test)]
mod capability_contract_tests {
    use super::*;

    struct LaunchOnly;
    impl AgentLauncher for LaunchOnly {
        fn launch(&mut self, _: &AgentLaunchRequest) -> Result<AgentLaunchOutcome, String> {
            Ok(AgentLaunchOutcome::Rejected {
                diagnostic: "disabled".into(),
            })
        }
    }
    struct InspectionOnly;
    impl AgentInspector for InspectionOnly {
        fn inspect(&mut self, _: &str, _: &str) -> Result<AgentSessionObservation, String> {
            Ok(AgentSessionObservation::Unreachable)
        }
    }
    fn dispatch_capability<T: AgentLauncher>(
        adapter: &mut T,
        request: &AgentLaunchRequest,
    ) -> AgentLaunchOutcome {
        adapter.launch(request).unwrap()
    }
    fn inspection_capability<T: AgentInspector>(adapter: &mut T) -> AgentSessionObservation {
        adapter.inspect("local", "token").unwrap()
    }
    #[test]
    fn independent_capabilities_do_not_require_a_monolithic_runtime() {
        let request = AgentLaunchRequest {
            job_id: "job".into(),
            claim_token: "token".into(),
            agent_kind: "codex".into(),
            machine_ref: "local".into(),
            repository_ref: "/repo".into(),
            brief_json: "{}".into(),
            worktree: false,
        };
        assert!(matches!(
            dispatch_capability(&mut LaunchOnly, &request),
            AgentLaunchOutcome::Rejected { .. }
        ));
        assert_eq!(
            inspection_capability(&mut InspectionOnly),
            AgentSessionObservation::Unreachable
        );
    }
}
