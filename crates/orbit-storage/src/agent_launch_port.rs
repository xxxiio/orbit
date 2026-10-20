//! Concrete local-authority adapter for the application launch port.
//! Remote executions use the authenticated machine protocol, not this store.
use crate::SqliteStore;
use orbit_application::agent_runtime::{AgentLaunchAuthority, AgentLaunchContext};

impl AgentLaunchAuthority for SqliteStore {
    fn claim_launch(
        &mut self,
        job_id: &str,
        revision: i64,
        token: &str,
        at_ms: i64,
    ) -> Result<bool, String> {
        self.claim_agent_launch(job_id, revision, token, at_ms)
            .map_err(|e| e.to_string())
    }
    fn launch_context(&self, job_id: &str) -> Result<AgentLaunchContext, String> {
        self.agent_launch_context(job_id).map_err(|e| e.to_string())
    }
    fn record_launch_outcome(
        &mut self,
        job_id: &str,
        token: &str,
        session_ref: Option<&str>,
        at_ms: i64,
    ) -> Result<(), String> {
        self.record_launch_result(job_id, token, session_ref, at_ms)
            .map_err(|e| e.to_string())
    }
}
