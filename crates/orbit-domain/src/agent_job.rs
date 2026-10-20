//! Durable agent execution identity and lifecycle, independent from task acceptance.
use crate::{ProjectId, TaskId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentJobState {
    Queued,
    Preparing,
    Launching,
    Running,
    WaitingInput,
    Blocked,
    Completed,
    Failed,
    Interrupted,
    Unreachable,
}
impl AgentJobState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Preparing => "preparing",
            Self::Launching => "launching",
            Self::Running => "running",
            Self::WaitingInput => "waiting_input",
            Self::Blocked => "blocked",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Interrupted => "interrupted",
            Self::Unreachable => "unreachable",
        }
    }
    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "queued" => Self::Queued,
            "preparing" => Self::Preparing,
            "launching" => Self::Launching,
            "running" => Self::Running,
            "waiting_input" => Self::WaitingInput,
            "blocked" => Self::Blocked,
            "completed" => Self::Completed,
            "failed" => Self::Failed,
            "interrupted" => Self::Interrupted,
            "unreachable" => Self::Unreachable,
            _ => return None,
        })
    }
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Interrupted)
    }
    pub fn can_transition(self, next: Self) -> bool {
        use AgentJobState::*;
        if self == next {
            return true;
        }
        match self {
            Queued => matches!(next, Preparing | Interrupted),
            Preparing => matches!(next, Launching | Failed | Interrupted | Unreachable),
            Launching => matches!(
                next,
                Running | WaitingInput | Blocked | Failed | Interrupted | Unreachable
            ),
            Running | WaitingInput | Blocked => matches!(
                next,
                Running | WaitingInput | Blocked | Completed | Failed | Interrupted | Unreachable
            ),
            Unreachable => matches!(
                next,
                Preparing
                    | Launching
                    | Running
                    | WaitingInput
                    | Blocked
                    | Completed
                    | Failed
                    | Interrupted
            ),
            Completed | Failed | Interrupted => false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentJob {
    pub id: String,
    pub request_id: String,
    pub project_id: ProjectId,
    pub task_id: TaskId,
    pub launch_revision: i64,
    pub agent_kind: String,
    pub machine_ref: String,
    pub runtime_kind: String,
    pub repository_ref: String,
    pub state: AgentJobState,
    pub revision: i64,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
    pub last_seen_at_ms: Option<i64>,
    pub runtime_session_ref: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn job_completion_is_terminal_but_not_task_acceptance() {
        assert!(AgentJobState::Running.can_transition(AgentJobState::Completed));
        assert!(!AgentJobState::Completed.can_transition(AgentJobState::Running));
        assert!(AgentJobState::Unreachable.can_transition(AgentJobState::Running));
        assert!(!AgentJobState::Queued.can_transition(AgentJobState::Completed));
        for s in [
            AgentJobState::Queued,
            AgentJobState::Preparing,
            AgentJobState::Launching,
            AgentJobState::Running,
            AgentJobState::WaitingInput,
            AgentJobState::Blocked,
            AgentJobState::Completed,
            AgentJobState::Failed,
            AgentJobState::Interrupted,
            AgentJobState::Unreachable,
        ] {
            assert_eq!(AgentJobState::parse(s.as_str()), Some(s));
        }
    }
}
