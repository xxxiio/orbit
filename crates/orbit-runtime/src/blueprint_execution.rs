//! Explicit, authority-fenced capability execution for blueprint actions.
//!
//! Registration is trusted process composition, not blueprint-controlled.
//! This module never runs actions on workflow transitions or HTTP reads.
use crate::AuthoritativeHost;
use orbit_application::RepositoryError;
use orbit_blueprint::ActionInvocation;
use orbit_storage::blueprint_actions::BlueprintActionRequest;
use std::collections::BTreeMap;

/// Providers MUST NOT silently retry uncertain external side effects. The
/// command id is stable and may be used as a downstream idempotency key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionOutcome {
    Succeeded,
    Failed,
    Uncertain,
}

/// A provider supplied by trusted host composition, never an arbitrary
/// executable name or a command embedded in a blueprint manifest.
pub trait BlueprintCapabilityExecutor: Send {
    fn capability_id(&self) -> &str;
    fn execute(
        &mut self,
        host: &mut AuthoritativeHost,
        request: &BlueprintActionRequest,
        invocation: &ActionInvocation,
    ) -> ExecutionOutcome;
}

/// Type-erased, process-local adapters. No global mutable registry; the
/// caller injects the permitted providers into each executor composition.
#[derive(Default)]
pub struct BlueprintExecutorRegistry {
    providers: BTreeMap<String, Box<dyn BlueprintCapabilityExecutor>>,
}
impl BlueprintExecutorRegistry {
    pub fn register(
        &mut self,
        provider: Box<dyn BlueprintCapabilityExecutor>,
    ) -> Result<(), String> {
        let id = provider.capability_id().to_owned();
        if id.is_empty() || self.providers.contains_key(&id) {
            return Err(format!("invalid or duplicate capability executor: {id}"));
        }
        self.providers.insert(id, provider);
        Ok(())
    }

    pub fn contains(&self, id: &str) -> bool {
        self.providers.contains_key(id)
    }

    /// Dispatch ONLY a previously authorized, persisted request. The durable
    /// claim is committed before calling out to an external capability.
    /// If the provider is unavailable, leave the request pending for a future
    /// explicitly authorized executor; never claim it speculatively.
    pub fn execute_claimed(
        &mut self,
        host: &mut AuthoritativeHost,
        command_id: &str,
    ) -> Result<BlueprintActionRequest, RepositoryError> {
        let request = host
            .project_work_item_action(command_id)?
            .ok_or(RepositoryError::Conflict)?;
        if request.state != "pending" {
            return Err(RepositoryError::Conflict);
        }
        let provider = self
            .providers
            .get_mut(&request.capability)
            .ok_or(RepositoryError::Conflict)?;
        let invocation: ActionInvocation = serde_json::from_str(&request.plan_json)
            .map_err(|e| RepositoryError::Other(e.to_string()))?;
        // Defense in depth: persisted invocation must match its envelope.
        if invocation.command_id != request.command_id
            || invocation.capability != request.capability
            || invocation.action_id != request.action_id
        {
            return Err(RepositoryError::Conflict);
        }
        host.claim_project_work_item_action(command_id)?;
        // A provider panic/process crash after this point leaves `claimed`;
        // it must be reconciled rather than automatically re-executed.
        let outcome = provider.execute(host, &request, &invocation);
        let state = match outcome {
            ExecutionOutcome::Succeeded => "succeeded",
            ExecutionOutcome::Failed => "failed",
            ExecutionOutcome::Uncertain => "uncertain",
        };
        host.record_project_work_item_action_result(command_id, state)
    }
}

/// Opt-in bridge to Orbit's existing local Herdr launch service. The blueprint
/// never supplies an executable or shell string: the trusted host composes the
/// runtime and approves repository paths ahead of time.
///
/// A blueprint work item is NOT a core Task. The explicitly selected core task
/// must belong to the same project as the work item that requested the action.
pub struct LocalAgentLaunchCapability<R> {
    runtime: R,
    allowed_repositories: std::collections::BTreeSet<String>,
}

impl<R: orbit_application::agent_runtime::AgentRuntime + Send> LocalAgentLaunchCapability<R> {
    pub fn new(runtime: R, allowed_repositories: std::collections::BTreeSet<String>) -> Self {
        Self {
            runtime,
            allowed_repositories,
        }
    }
}

impl<R: orbit_application::agent_runtime::AgentRuntime + Send> BlueprintCapabilityExecutor
    for LocalAgentLaunchCapability<R>
{
    fn capability_id(&self) -> &str {
        "agent.launch"
    }

    fn execute(
        &mut self,
        host: &mut AuthoritativeHost,
        request: &BlueprintActionRequest,
        invocation: &ActionInvocation,
    ) -> ExecutionOutcome {
        // Fail closed before calling an external runtime if the trusted
        // composition or target association is invalid.
        let get = |name: &str| invocation.parameters.get(name).and_then(|v| v.as_str());
        let (Some(task_id), Some(repository), Some(agent)) =
            (get("task_id"), get("repository"), get("agent"))
        else {
            return ExecutionOutcome::Failed;
        };
        if !self.allowed_repositories.contains(repository) || !matches!(agent, "codex" | "opencode")
        {
            return ExecutionOutcome::Failed;
        }
        let worktree = match invocation.parameters.get("worktree") {
            None | Some(serde_json::Value::Bool(false)) => false,
            Some(serde_json::Value::Bool(true)) => true,
            _ => return ExecutionOutcome::Failed,
        };
        // Prevent confusing a blueprint work-item identifier with a Task ID,
        // and reject cross-project execution requests.
        let item = match host.authority.conn.blueprint_work_item(&request.item_id) {
            Ok(Some(item)) => item,
            _ => return ExecutionOutcome::Failed,
        };
        let (_, tasks) = match host.tasks() {
            Ok(tasks) => tasks,
            Err(_) => return ExecutionOutcome::Failed,
        };
        if !tasks
            .iter()
            .any(|t| t.id == task_id && t.project_id.as_deref() == Some(item.project_id.as_str()))
        {
            return ExecutionOutcome::Failed;
        }
        let now_ms = match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
            Ok(time) => match i64::try_from(time.as_millis()) {
                Ok(ms) => ms,
                Err(_) => return ExecutionOutcome::Failed,
            },
            Err(_) => return ExecutionOutcome::Failed,
        };
        // The action's durable command ID is also the agent job ID. A replay
        // cannot allocate a fresh job, and the action claim prevents a second
        // invocation if an earlier Herdr response was lost.
        match host.start_local_agent_job(
            crate::agent_authority::LocalAgentLaunch {
                task_id,
                job_id: &request.command_id,
                agent,
                machine: "local",
                repository,
                worktree,
                at_ms: now_ms,
            },
            &mut self.runtime,
        ) {
            Ok(outcome) => match outcome["outcome"].as_str() {
                Some("confirmed") => ExecutionOutcome::Succeeded,
                Some("reconcile") | Some("already_claimed") | Some("already_reserved") => {
                    ExecutionOutcome::Uncertain
                }
                _ => ExecutionOutcome::Uncertain,
            },
            // A launch can fail after Herdr has received the request. Never
            // mark such a failure as safe-to-retry.
            Err(_) => ExecutionOutcome::Uncertain,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct TestProvider(&'static str);
    impl BlueprintCapabilityExecutor for TestProvider {
        fn capability_id(&self) -> &str {
            self.0
        }
        fn execute(
            &mut self,
            _host: &mut AuthoritativeHost,
            _request: &BlueprintActionRequest,
            _invocation: &ActionInvocation,
        ) -> ExecutionOutcome {
            ExecutionOutcome::Succeeded
        }
    }
    #[test]
    fn registry_rejects_duplicate_capabilities() {
        let mut registry = BlueprintExecutorRegistry::default();
        assert!(
            registry
                .register(Box::new(TestProvider("test.capability")))
                .is_ok()
        );
        assert!(registry.contains("test.capability"));
        assert!(
            registry
                .register(Box::new(TestProvider("test.capability")))
                .is_err()
        );
        assert!(!registry.contains("not.installed"));
    }
    #[test]
    fn empty_capability_not_registered() {
        let mut registry = BlueprintExecutorRegistry::default();
        assert!(registry.register(Box::new(TestProvider(""))).is_err());
    }
}

#[cfg(test)]
mod composition_tests {
    use super::*;
    use orbit_application::agent_runtime::{
        AgentLaunchOutcome, AgentLaunchRequest, AgentRuntime, AgentSessionObservation,
    };
    struct DummyRuntime;
    impl AgentRuntime for DummyRuntime {
        fn launch(&mut self, _: &AgentLaunchRequest) -> Result<AgentLaunchOutcome, String> {
            Err("not installed".into())
        }
        fn inspect(&mut self, _: &str, _: &str) -> Result<AgentSessionObservation, String> {
            Err("not installed".into())
        }
    }
    #[test]
    fn opt_in_adapter_registers_with_generic_registry() {
        let mut registry = BlueprintExecutorRegistry::default();
        registry
            .register(Box::new(LocalAgentLaunchCapability::new(
                DummyRuntime,
                std::collections::BTreeSet::new(),
            )))
            .unwrap();
        assert!(registry.contains("agent.launch"));
    }
}
