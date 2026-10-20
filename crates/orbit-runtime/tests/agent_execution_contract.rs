//! Executable M15 agent-execution contract with deterministic fake runtimes.
//!
//! Proves reservation -> prep -> claimed launch -> observation -> handoff ->
//! review -> resume sequencing without Herdr, and that an agent session is
//! never treated as task completion. Also proves ambiguous launches are fenced
//! and never automatically relaunched.
use orbit_application::agent_runtime::{
    AgentLaunchOutcome, AgentLaunchRequest, AgentRuntime, AgentSessionObservation,
};
use orbit_application::{
    CreateProjectCommand, CreateProjectWithActivityCommand, CreateTaskCommand,
    CreateTaskWithActivityCommand, TransitionTaskCommand, TransitionTaskWithActivityCommand,
};
use orbit_domain::{ActivityId, ActivityOrigin, ProjectId, TaskId, TaskScope, TaskStatus};
use orbit_runtime::AuthoritativeHost;
use serde_json::json;
use std::path::Path;

struct FakeRuntime {
    launches: usize,
    inspection: AgentSessionObservation,
}

impl AgentRuntime for FakeRuntime {
    fn launch(&mut self, _request: &AgentLaunchRequest) -> Result<AgentLaunchOutcome, String> {
        self.launches += 1;
        Ok(AgentLaunchOutcome::Confirmed {
            session_ref: format!("session-{}", self.launches),
        })
    }
    fn inspect(&mut self, _machine: &str, _token: &str) -> Result<AgentSessionObservation, String> {
        Ok(self.inspection.clone())
    }
}

struct AmbiguousRuntime {
    launches: usize,
}

impl AgentRuntime for AmbiguousRuntime {
    fn launch(&mut self, _request: &AgentLaunchRequest) -> Result<AgentLaunchOutcome, String> {
        self.launches += 1;
        Ok(AgentLaunchOutcome::Ambiguous {
            diagnostic: "transport lost after request".into(),
        })
    }
    fn inspect(&mut self, _machine: &str, _token: &str) -> Result<AgentSessionObservation, String> {
        Ok(AgentSessionObservation::AbsentConfirmed)
    }
}

fn ready_task(host: &mut AuthoritativeHost, prefix: &str) -> (ProjectId, TaskId) {
    let project = ProjectId::new(format!("{prefix}-project")).unwrap();
    let task = TaskId::new(format!("{prefix}-task")).unwrap();
    host.create_project_accepted(
        "contract-test",
        &format!("{prefix}-project-command"),
        0,
        CreateProjectWithActivityCommand {
            project: CreateProjectCommand {
                id: project.clone(),
                name: "Agent contract".into(),
            },
            activity_id: ActivityId::new(format!("{prefix}-activity-project")).unwrap(),
            origin: ActivityOrigin::Human,
            summary: "project".into(),
        },
    )
    .unwrap();
    host.create_task_accepted(
        "contract-test",
        &format!("{prefix}-task-command"),
        1,
        CreateTaskWithActivityCommand {
            task: CreateTaskCommand {
                id: task.clone(),
                title: "Implement the contract".into(),
                scope: TaskScope::try_new(Some(project.clone()), None).unwrap(),
            },
            activity_id: ActivityId::new(format!("{prefix}-activity-task")).unwrap(),
            origin: ActivityOrigin::Human,
            summary: "task".into(),
        },
    )
    .unwrap();
    host.transition_task_accepted(
        "contract-test",
        &format!("{prefix}-ready-command"),
        2,
        TransitionTaskWithActivityCommand {
            task: TransitionTaskCommand {
                id: task.clone(),
                status: TaskStatus::Ready,
            },
            activity_id: ActivityId::new(format!("{prefix}-activity-ready")).unwrap(),
            origin: ActivityOrigin::Human,
            summary: "ready".into(),
        },
    )
    .unwrap();
    (project, task)
}

fn task_status(host: &AuthoritativeHost, id: &TaskId) -> String {
    host.tasks()
        .unwrap()
        .1
        .into_iter()
        .find(|task| task.id == id.as_str())
        .map(|task| task.status)
        .expect("task exists")
}

#[test]
fn agent_lifecycle_requires_review_and_never_completes_the_task() {
    let mut host = AuthoritativeHost::open(Path::new(":memory:")).unwrap();
    let (_project, task) = ready_task(&mut host, "lifecycle");
    let repository = std::env::temp_dir().join("orbit-agent-contract");
    let repository = repository.to_str().unwrap();

    let mut runtime = FakeRuntime {
        launches: 0,
        inspection: AgentSessionObservation::Present {
            session_ref: "session-1".into(),
            runtime_state: "working".into(),
        },
    };
    let started = host
        .start_local_agent_job(
            orbit_runtime::agent_authority::LocalAgentLaunch {
                task_id: task.as_str(),
                job_id: "job-1",
                agent: "codex",
                machine: "local",
                repository,
                worktree: false,
                at_ms: 1_000,
            },
            &mut runtime,
        )
        .unwrap();
    assert_eq!(started["outcome"], "confirmed");
    assert_eq!(runtime.launches, 1);
    let job = host.local_agent_job("job-1").unwrap().unwrap();
    // The durable claim is committed before the runtime call; the first
    // authoritative observation confirms the session.
    assert_eq!(job.state, "launching");
    let observed = host
        .inspect_local_agent_job(&mut runtime, "job-1", job.revision, 1_500)
        .unwrap();
    assert_eq!(observed.state, "running");
    assert_eq!(runtime.launches, 1, "inspection must never launch");
    // A confirmed agent session is never task completion.
    assert_eq!(task_status(&host, &task), "ready");

    // The same reservation must never launch a second runtime session.
    let duplicate = host
        .start_local_agent_job(
            orbit_runtime::agent_authority::LocalAgentLaunch {
                task_id: task.as_str(),
                job_id: "job-1",
                agent: "codex",
                machine: "local",
                repository,
                worktree: false,
                at_ms: 1_100,
            },
            &mut runtime,
        )
        .unwrap();
    assert_eq!(duplicate["outcome"], "already_reserved");
    assert_eq!(runtime.launches, 1);

    let mut absent = FakeRuntime {
        launches: 0,
        inspection: AgentSessionObservation::AbsentConfirmed,
    };
    let reconciled = host
        .inspect_local_agent_job(&mut absent, "job-1", observed.revision, 2_000)
        .unwrap();
    assert_eq!(reconciled.state, "interrupted");
    assert_eq!(absent.launches, 0, "inspection must never launch");

    host.record_local_agent_handoff(
        "job-1",
        "handoff-1",
        &json!({"summary": "Implemented", "validation": "cargo test", "next_steps": "Finish review"}),
        3_000,
    )
    .unwrap();
    let review = host
        .review_local_agent_job(
            "job-1",
            "handoff-1",
            "changes_requested",
            "reviewer",
            "needs follow-up",
            4_000,
        )
        .unwrap();
    assert_eq!(review["task_completed"], false);
    assert_eq!(task_status(&host, &task), "ready");

    let mut resumed_runtime = FakeRuntime {
        launches: 0,
        inspection: AgentSessionObservation::Present {
            session_ref: "session-2".into(),
            runtime_state: "working".into(),
        },
    };
    let resumed = host
        .resume_local_agent_job("job-1", "job-2", &mut resumed_runtime, 5_000)
        .unwrap();
    assert_eq!(resumed["outcome"], "confirmed");
    assert_eq!(resumed_runtime.launches, 1);
    let context = host.local_agent_resume_context("job-2").unwrap();
    assert!(context.to_string().contains("Finish review"), "{context}");
    assert_eq!(task_status(&host, &task), "ready");
}

#[test]
fn ambiguous_launch_is_fenced_and_never_automatically_relaunched() {
    let mut host = AuthoritativeHost::open(Path::new(":memory:")).unwrap();
    let (_project, task) = ready_task(&mut host, "ambiguous");
    let repository = std::env::temp_dir().join("orbit-agent-contract");
    let repository = repository.to_str().unwrap();
    let mut runtime = AmbiguousRuntime { launches: 0 };

    let first = host
        .start_local_agent_job(
            orbit_runtime::agent_authority::LocalAgentLaunch {
                task_id: task.as_str(),
                job_id: "ambiguous-job",
                agent: "opencode",
                machine: "local",
                repository,
                worktree: false,
                at_ms: 1_000,
            },
            &mut runtime,
        )
        .unwrap();
    assert_eq!(first["outcome"], "reconcile");
    assert_eq!(runtime.launches, 1);

    // Retrying the same job preserves the claim; no second runtime invocation.
    let retry = host
        .start_local_agent_job(
            orbit_runtime::agent_authority::LocalAgentLaunch {
                task_id: task.as_str(),
                job_id: "ambiguous-job",
                agent: "opencode",
                machine: "local",
                repository,
                worktree: false,
                at_ms: 1_100,
            },
            &mut runtime,
        )
        .unwrap();
    assert_eq!(retry["outcome"], "already_reserved");
    assert_eq!(runtime.launches, 1);

    // An absent machine is not proof the launch failed: ownership is retained.
    let job = host.local_agent_job("ambiguous-job").unwrap().unwrap();
    let observed = host
        .inspect_local_agent_job(&mut runtime, "ambiguous-job", job.revision, 2_000)
        .unwrap();
    assert_eq!(observed.state, "unreachable");
    assert_eq!(runtime.launches, 1);
    assert_eq!(task_status(&host, &task), "ready");
}
