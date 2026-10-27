//! Browser-authorized execution contracts (M32 I3): executor read, machine
//! validation at start, preserved refusal text, claim state and the objective.
//!
//! Every test drives the host boundary the HTTP handlers call, so a transport
//! mapping bug shows up as a wrong `AuthorityError`, not just wrong storage.
use super::*;
use orbit_application::{
    CreateProjectCommand, CreateProjectWithActivityCommand, CreateTaskCommand,
    CreateTaskWithActivityCommand, RepositoryError, TransitionTaskCommand,
    TransitionTaskWithActivityCommand,
};
use orbit_domain::{ActivityId, ActivityOrigin, TaskStatus};

const OWNER: &str = "browser-1";
const EXECUTOR: &str = "dev-1";
const EXECUTOR_TOKEN: &str = "executor-token-0123456789abcdef01";
const OWNER_TOKEN: &str = "browser-token-0123456789abcdef01";

/// Enrolled owner plus enrolled executor, and a revision cursor because domain
/// commands are revision-checked across the whole authority ledger.
struct Fixture {
    host: AuthoritativeHost,
    revision: i64,
}

impl Fixture {
    fn new() -> Self {
        let mut host = AuthoritativeHost::open(":memory:").unwrap();
        host.enroll_device(OWNER, OWNER_TOKEN).unwrap();
        host.enroll_device(EXECUTOR, EXECUTOR_TOKEN).unwrap();
        Self { host, revision: 0 }
    }

    fn project(&mut self, prefix: &str) -> String {
        let project = format!("{prefix}-project");
        self.host
            .create_project_accepted(
                OWNER,
                &format!("{prefix}-project-command"),
                self.revision,
                CreateProjectWithActivityCommand {
                    project: CreateProjectCommand {
                        id: ProjectId::new(project.clone()).unwrap(),
                        name: "Executor contracts".into(),
                    },
                    activity_id: ActivityId::new(format!("{prefix}-activity-project")).unwrap(),
                    origin: ActivityOrigin::Human,
                    summary: "project".into(),
                },
            )
            .unwrap();
        self.revision += 1;
        project
    }

    /// A task in `project`, left in the inbox status.
    fn task(&mut self, prefix: &str, project: &str) -> String {
        let task = format!("{prefix}-task");
        self.host
            .create_task_accepted(
                OWNER,
                &format!("{prefix}-task-command"),
                self.revision,
                CreateTaskWithActivityCommand {
                    task: CreateTaskCommand {
                        id: TaskId::new(task.clone()).unwrap(),
                        title: "Launch me".into(),
                        scope: orbit_domain::TaskScope::try_new(
                            Some(ProjectId::new(project).unwrap()),
                            None,
                        )
                        .unwrap(),
                    },
                    activity_id: ActivityId::new(format!("{prefix}-activity-task")).unwrap(),
                    origin: ActivityOrigin::Human,
                    summary: "task".into(),
                },
            )
            .unwrap();
        self.revision += 1;
        task
    }

    fn ready(&mut self, prefix: &str, task: &str) {
        self.host
            .transition_task_accepted(
                OWNER,
                &format!("{prefix}-ready-command"),
                self.revision,
                TransitionTaskWithActivityCommand {
                    task: TransitionTaskCommand {
                        id: TaskId::new(task.to_owned()).unwrap(),
                        status: TaskStatus::Ready,
                    },
                    activity_id: ActivityId::new(format!("{prefix}-activity-ready")).unwrap(),
                    origin: ActivityOrigin::Human,
                    summary: "ready".into(),
                },
            )
            .unwrap();
        self.revision += 1;
    }

    /// A ready task in its own project: the only state start accepts.
    fn ready_task(&mut self, prefix: &str) -> String {
        let project = self.project(prefix);
        let task = self.task(prefix, &project);
        self.ready(prefix, &task);
        task
    }

    /// The job must not exist, whoever asks; ownership is checked before reads,
    /// so this goes through storage rather than the owner-scoped read.
    fn assert_no_job(&self, id: &str) {
        assert!(
            self.host
                .authority
                .conn
                .get_agent_job(id)
                .unwrap()
                .is_none()
        );
    }
}

fn start(job_id: &str, task_id: &str, machine: &str) -> StartAgentJob {
    StartAgentJob {
        job_id: job_id.into(),
        task_id: task_id.into(),
        agent_kind: "codex".into(),
        machine_ref: machine.into(),
        repository_ref: "/srv/repo".into(),
        worktree: false,
        at_ms: 1_000,
        objective: None,
    }
}

fn claim(job_id: &str, revision: i64, token: &str, at_ms: i64) -> ClaimMachineAgentJob {
    ClaimMachineAgentJob {
        job_id: job_id.into(),
        expected_revision: revision,
        claim_token: token.into(),
        at_ms,
    }
}

fn refusal(error: AuthorityError) -> &'static str {
    match error {
        AuthorityError::Invalid(message) => message,
        other => panic!("expected a fixed invalid refusal, got {other:?}"),
    }
}

#[test]
fn executor_read_is_empty_then_reports_active_revoked_and_counters() {
    let host = AuthoritativeHost::open(":memory:").unwrap();
    // No enrolled device: an honest, valid empty answer.
    assert!(
        host.agent_executors_authoritatively(OWNER)
            .unwrap()
            .executors
            .is_empty()
    );

    let mut fixture = Fixture::new();
    fixture
        .host
        .enroll_device("dev-2", "second-token-0123456789abcdef012")
        .unwrap();
    fixture.host.revoke_device("dev-2").unwrap();
    let unclaimed = fixture.ready_task("executors");
    fixture
        .host
        .start_agent_job_authoritatively(OWNER, start("job-1", &unclaimed, EXECUTOR))
        .unwrap();
    let claimed = fixture.ready_task("executors-claimed");
    fixture
        .host
        .start_agent_job_authoritatively(OWNER, start("job-2", &claimed, EXECUTOR))
        .unwrap();
    fixture
        .host
        .machine_agent_claim_authoritatively(EXECUTOR, claim("job-2", 1, "claim-2", 2_000))
        .unwrap();

    let executors = fixture
        .host
        .agent_executors_authoritatively(OWNER)
        .unwrap()
        .executors;
    // The authority's own revoked `orbit-server` provenance credential is not an
    // executor and must not appear as a phantom inactive device.
    assert_eq!(
        executors
            .iter()
            .map(|e| e.device_id.as_str())
            .collect::<Vec<_>>(),
        [OWNER, EXECUTOR, "dev-2"]
    );
    let executor = &executors[1];
    assert!(executor.active);
    assert_eq!(executor.last_claim_at_ms, Some(2_000));
    assert_eq!(executor.claimed_jobs, 1);
    assert_eq!(executor.preparing_jobs, 1);
    let revoked = &executors[2];
    assert!(!revoked.active, "a revoked device is reported inactive");
    assert_eq!((revoked.claimed_jobs, revoked.preparing_jobs), (0, 0));
    // An unknown executor id is not invented by the read.
    assert!(
        executors
            .iter()
            .all(|executor| executor.device_id != "ghost")
    );
}

#[test]
fn start_refuses_an_unknown_executor_and_creates_no_job() {
    let mut fixture = Fixture::new();
    let task = fixture.ready_task("unknown-executor");
    let error = fixture
        .host
        .start_agent_job_authoritatively(OWNER, start("job-1", &task, "ghost"))
        .unwrap_err();
    assert_eq!(refusal(error), "unknown agent executor");
    fixture.assert_no_job("job-1");
    // The refusal happens before the task is even read, so no brief exists.
    assert!(
        fixture
            .host
            .authority
            .conn
            .agent_job_brief("job-1")
            .unwrap()
            .is_none()
    );
}

#[test]
fn start_refuses_a_revoked_executor_and_creates_no_job() {
    let mut fixture = Fixture::new();
    let task = fixture.ready_task("revoked-executor");
    fixture.host.revoke_device(EXECUTOR).unwrap();
    let error = fixture
        .host
        .start_agent_job_authoritatively(OWNER, start("job-1", &task, EXECUTOR))
        .unwrap_err();
    assert_eq!(refusal(error), "inactive agent executor");
    fixture.assert_no_job("job-1");
}

#[test]
fn start_keeps_the_fixture_contract_of_machine_ref_equal_to_the_caller_device() {
    // The web fixture starts a job with `machine_ref` set to the authenticated
    // device itself; that must stay valid.
    let mut fixture = Fixture::new();
    let task = fixture.ready_task("self-executor");
    let job = fixture
        .host
        .start_agent_job_authoritatively(OWNER, start("job-1", &task, OWNER))
        .unwrap();
    assert_eq!(job.machine_ref, OWNER);
    assert_eq!(job.claim_state, None);
}

#[test]
fn preserved_refusal_text_for_every_launch_gate() {
    let mut fixture = Fixture::new();
    let project = fixture.project("gates");
    let task = fixture.task("gates", &project);
    fixture.ready("gates", &task);
    let inbox = fixture.task("gates-inbox", &project);

    // Wrong task status: the task is still in the inbox, but its project matches.
    let error = fixture
        .host
        .start_agent_job_authoritatively(OWNER, start("job-status", &inbox, EXECUTOR))
        .unwrap_err();
    assert_eq!(refusal(error), "task not eligible for connected start");
    fixture.assert_no_job("job-status");

    // Open prerequisite.
    fixture
        .host
        .authority
        .conn
        .execute(
            "INSERT INTO task_prerequisites(task_id,prerequisite_id) VALUES (?1,?2)",
            [&task, &inbox],
        )
        .unwrap();
    let error = fixture
        .host
        .start_agent_job_authoritatively(OWNER, start("job-prereq", &task, EXECUTOR))
        .unwrap_err();
    assert_eq!(refusal(error), "agent task has unfinished prerequisites");
    fixture.assert_no_job("job-prereq");
    fixture
        .host
        .authority
        .conn
        .execute("DELETE FROM task_prerequisites", [])
        .unwrap();

    // Active sibling job for the same task, then a conflicting replay of it.
    fixture
        .host
        .start_agent_job_authoritatively(OWNER, start("job-a", &task, EXECUTOR))
        .unwrap();
    let error = fixture
        .host
        .start_agent_job_authoritatively(OWNER, start("job-b", &task, EXECUTOR))
        .unwrap_err();
    assert_eq!(refusal(error), "task already has active agent");
    fixture.assert_no_job("job-b");

    let mut replay = start("job-a", &task, EXECUTOR);
    replay.worktree = true;
    let error = fixture
        .host
        .start_agent_job_authoritatively(OWNER, replay)
        .unwrap_err();
    assert_eq!(refusal(error), "conflicting connected start replay");
}

#[test]
fn resume_preserves_the_unconfirmed_launch_refusal() {
    let mut fixture = Fixture::new();
    let task = fixture.ready_task("resume-gate");
    fixture
        .host
        .start_agent_job_authoritatively(OWNER, start("prior", &task, EXECUTOR))
        .unwrap();
    // Claimed but never confirmed, then driven terminal by the operator.
    fixture
        .host
        .machine_agent_claim_authoritatively(EXECUTOR, claim("prior", 1, "claim-prior", 2_000))
        .unwrap();
    fixture
        .host
        .authority
        .conn
        .transition_agent_job(
            "prior",
            2,
            orbit_domain::AgentJobState::Interrupted,
            3_000,
            "operator stop",
        )
        .unwrap();
    fixture
        .host
        .authority
        .conn
        .record_agent_job_handoff(
            "prior",
            "handoff-prior",
            &serde_json::json!({"summary":"partial","validation":{},"next_steps":[]}),
            3_100,
        )
        .unwrap();
    let error = fixture
        .host
        .resume_agent_job_authoritatively(
            OWNER,
            ResumeAgentJob {
                prior_job_id: "prior".into(),
                new_job_id: "successor".into(),
                at_ms: 4_000,
            },
        )
        .unwrap_err();
    assert_eq!(
        refusal(error),
        "predecessor launch is unconfirmed; reconcile or sweep the runtime before resuming"
    );
    fixture.assert_no_job("successor");
}

#[test]
fn command_and_machine_routes_preserve_their_fixed_refusals() {
    let mut fixture = Fixture::new();
    let task = fixture.ready_task("route-gates");
    fixture
        .host
        .start_agent_job_authoritatively(OWNER, start("job-1", &task, EXECUTOR))
        .unwrap();

    // Stale command revision: the job is already at revision 1.
    let error = fixture
        .host
        .accept_agent_job_command_authoritatively(
            OWNER,
            AgentJobCommand {
                command_id: "cmd-1".into(),
                job_id: "job-1".into(),
                expected_revision: 0,
                action: "interrupt".into(),
                at_ms: 2_000,
            },
        )
        .unwrap_err();
    assert_eq!(refusal(error), "stale agent command revision");

    // A different machine may not claim this job.
    let error = fixture
        .host
        .machine_agent_claim_authoritatively("dev-2", claim("job-1", 1, "claim-1", 2_000))
        .unwrap_err();
    assert_eq!(refusal(error), "executor machine mismatch");

    // The conflicting review decision keeps its text so the browser can quote it.
    fixture
        .host
        .authority
        .conn
        .record_agent_job_handoff(
            "job-1",
            "handoff-1",
            &serde_json::json!({"summary":"done","validation":{},"next_steps":[]}),
            2_100,
        )
        .unwrap();
    let review = |decision: &str, at_ms: i64| ReviewAgentHandoff {
        job_id: "job-1".into(),
        handoff_id: "handoff-1".into(),
        decision: decision.into(),
        reason: "reviewed".into(),
        at_ms,
    };
    fixture
        .host
        .review_agent_handoff_authoritatively(OWNER, review("approved", 2_200))
        .unwrap();
    let error = fixture
        .host
        .review_agent_handoff_authoritatively(OWNER, review("changes_requested", 2_300))
        .unwrap_err();
    assert_eq!(
        refusal(error),
        "review already recorded; conflicting decision"
    );
}

#[test]
fn unmapped_storage_errors_never_reach_the_transport() {
    // The mapping is an allowlist: anything else, including detail-carrying
    // storage text, collapses to the generic refusal.
    assert_eq!(
        refusal(storage_refusal(RepositoryError::Other(
            "no such table: agent_jobs /home/xliu/private.sqlite".into()
        ))),
        "invalid agent job request"
    );
    assert_eq!(
        refusal(storage_refusal(RepositoryError::Conflict)),
        "invalid agent job request"
    );
    assert_eq!(
        refusal(storage_refusal(RepositoryError::CorruptData {
            detail: "row 4".into()
        })),
        "invalid agent job request"
    );
    assert_eq!(
        refusal(storage_refusal(RepositoryError::Other(
            "stale agent command revision".into()
        ))),
        "stale agent command revision"
    );
}

#[test]
fn claim_state_moves_from_null_to_claimed_to_confirmed() {
    let mut fixture = Fixture::new();
    let task = fixture.ready_task("claim-state");
    let untouched_task = fixture.ready_task("claim-state-untouched");
    let started = fixture
        .host
        .start_agent_job_authoritatively(OWNER, start("job-1", &task, EXECUTOR))
        .unwrap();
    assert_eq!(started.state, "preparing");
    assert_eq!(started.claim_state, None);

    fixture
        .host
        .start_agent_job_authoritatively(OWNER, start("job-2", &untouched_task, EXECUTOR))
        .unwrap();
    fixture
        .host
        .machine_agent_claim_authoritatively(EXECUTOR, claim("job-1", 1, "claim-1", 2_000))
        .unwrap();
    let claimed = fixture
        .host
        .agent_job_authoritative(OWNER, "job-1")
        .unwrap()
        .unwrap();
    assert_eq!(claimed.state, "launching");
    assert_eq!(claimed.claim_state.as_deref(), Some("claimed"));

    fixture
        .host
        .machine_agent_launch_result_authoritatively(
            EXECUTOR,
            ReportMachineAgentLaunch {
                job_id: "job-1".into(),
                claim_token: "claim-1".into(),
                session_ref: Some("orbit0000000000000abc".into()),
                at_ms: 2_100,
            },
        )
        .unwrap();
    let confirmed = fixture
        .host
        .agent_job_authoritative(OWNER, "job-1")
        .unwrap()
        .unwrap();
    assert_eq!(confirmed.claim_state.as_deref(), Some("confirmed"));

    // A job nobody claimed keeps a null claim, whatever happens to its sibling.
    let untouched = fixture
        .host
        .agent_job_authoritative(OWNER, "job-2")
        .unwrap()
        .unwrap();
    assert_eq!(untouched.claim_state, None);
    // The resume context reports the same durable claim.
    let context = fixture
        .host
        .agent_resume_context_authoritatively(OWNER, "job-1")
        .unwrap();
    assert_eq!(context["job"]["claim_state"], "confirmed");
}

#[test]
fn overview_awaits_an_executor_for_an_unclaimed_preparing_job() {
    let mut fixture = Fixture::new();
    let task = fixture.ready_task("await-executor");
    fixture
        .host
        .start_agent_job_authoritatively(OWNER, start("job-1", &task, EXECUTOR))
        .unwrap();
    let overview = fixture.host.agent_overview_authoritatively(OWNER).unwrap();
    assert_eq!(overview.len(), 1);
    assert_eq!(overview[0]["next_action"], "await_executor");
    assert_eq!(overview[0]["needs_attention"], false);

    // Once an executor claims it, the job is no longer waiting for one.
    fixture
        .host
        .machine_agent_claim_authoritatively(EXECUTOR, claim("job-1", 1, "claim-1", 2_000))
        .unwrap();
    let overview = fixture.host.agent_overview_authoritatively(OWNER).unwrap();
    assert_ne!(overview[0]["next_action"], "await_executor");
    assert_eq!(overview[0]["next_action"], "inspect_runtime");
}

#[test]
fn objective_is_stored_trimmed_refused_when_long_and_reaches_resume_context() {
    let mut fixture = Fixture::new();
    let task = fixture.ready_task("objective");
    let mut request = start("job-1", &task, EXECUTOR);
    request.objective = Some("  Fix the login redirect and add a regression test.  ".into());
    fixture
        .host
        .start_agent_job_authoritatively(OWNER, request)
        .unwrap();
    let context = fixture
        .host
        .agent_resume_context_authoritatively(OWNER, "job-1")
        .unwrap();
    assert_eq!(
        context["original_brief"]["objective"],
        "Fix the login redirect and add a regression test."
    );

    // An empty objective is absent, not an empty string in the brief.
    let other = fixture.ready_task("objective-empty");
    let mut blank = start("job-2", &other, EXECUTOR);
    blank.objective = Some("   \n ".into());
    fixture
        .host
        .start_agent_job_authoritatively(OWNER, blank)
        .unwrap();
    let context = fixture
        .host
        .agent_resume_context_authoritatively(OWNER, "job-2")
        .unwrap();
    assert!(context["original_brief"].get("objective").is_none());

    // Over the cap is refused before any job exists.
    let too_long = fixture.ready_task("objective-long");
    let mut request = start("job-3", &too_long, EXECUTOR);
    request.objective = Some("x".repeat(4001));
    let error = fixture
        .host
        .start_agent_job_authoritatively(OWNER, request)
        .unwrap_err();
    assert_eq!(refusal(error), "objective is too long");
    fixture.assert_no_job("job-3");

    // Exactly at the cap is accepted.
    let at_cap = fixture.ready_task("objective-cap");
    let mut request = start("job-4", &at_cap, EXECUTOR);
    request.objective = Some("y".repeat(4000));
    fixture
        .host
        .start_agent_job_authoritatively(OWNER, request)
        .unwrap();
}

#[test]
fn resumed_job_keeps_the_predecessor_objective_in_its_brief() {
    let mut fixture = Fixture::new();
    let task = fixture.ready_task("objective-resume");
    let mut request = start("prior", &task, EXECUTOR);
    request.objective = Some("Keep this objective across the resume".into());
    fixture
        .host
        .start_agent_job_authoritatively(OWNER, request)
        .unwrap();
    fixture
        .host
        .authority
        .conn
        .transition_agent_job(
            "prior",
            1,
            orbit_domain::AgentJobState::Interrupted,
            2_000,
            "operator stop",
        )
        .unwrap();
    fixture
        .host
        .authority
        .conn
        .record_agent_job_handoff(
            "prior",
            "handoff-prior",
            &serde_json::json!({"summary":"partial","validation":{},"next_steps":[]}),
            2_100,
        )
        .unwrap();
    let resumed = fixture
        .host
        .resume_agent_job_authoritatively(
            OWNER,
            ResumeAgentJob {
                prior_job_id: "prior".into(),
                new_job_id: "successor".into(),
                at_ms: 3_000,
            },
        )
        .unwrap();
    assert_eq!(resumed.claim_state, None);
    let context = fixture
        .host
        .agent_resume_context_authoritatively(OWNER, "successor")
        .unwrap();
    assert_eq!(
        context["original_brief"]["objective"],
        "Keep this objective across the resume"
    );
    assert_eq!(context["original_brief"]["resume"]["prior_job_id"], "prior");
}
