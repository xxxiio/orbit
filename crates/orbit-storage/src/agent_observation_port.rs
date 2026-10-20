//! SQLite adapter for the application-owned observation port.
use crate::{AgentRuntimeObservation, RuntimeActivity, SqliteStore};
use orbit_application::agent_runtime::{
    AgentActivity, AgentMonitoringContext, AgentObservationAuthority, AgentObservedSession,
};
use orbit_domain::AgentJob;

impl AgentObservationAuthority for SqliteStore {
    fn monitoring_context(&self, id: &str) -> Result<AgentMonitoringContext, String> {
        self.agent_monitoring_context(id).map_err(|e| e.to_string())
    }

    fn record_observation(
        &mut self,
        id: &str,
        revision: i64,
        token: &str,
        observation: AgentObservedSession,
        at_ms: i64,
    ) -> Result<AgentJob, String> {
        let stored = match observation {
            AgentObservedSession::Present {
                session_ref,
                activity,
            } => {
                let state = match activity {
                    AgentActivity::Working => RuntimeActivity::Working,
                    AgentActivity::WaitingInput => RuntimeActivity::WaitingInput,
                    AgentActivity::Blocked => RuntimeActivity::Blocked,
                    AgentActivity::Idle => RuntimeActivity::Idle,
                };
                AgentRuntimeObservation::Present { session_ref, state }
            }
            AgentObservedSession::Unreachable => AgentRuntimeObservation::Unreachable,
            AgentObservedSession::AbsentConfirmed => AgentRuntimeObservation::AbsentConfirmed,
        };
        self.observe_agent_session(id, revision, token, stored, at_ms)
            .map_err(|e| e.to_string())
    }
}
