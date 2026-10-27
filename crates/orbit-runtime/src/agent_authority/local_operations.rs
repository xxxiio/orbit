//! Local-authority execution facade. The canonical host owns SQLite and the
//! external runtime adapter executes only after a durable launch claim.
use super::*;

/// Inputs for one local-authority agent launch.
pub struct LocalAgentLaunch<'a> {
    pub task_id: &'a str,
    pub job_id: &'a str,
    pub agent: &'a str,
    pub machine: &'a str,
    pub repository: &'a str,
    pub worktree: bool,
    pub at_ms: i64,
}

impl AuthoritativeHost {
    /// Local authority one-step launch. The repository and machine are explicit;
    /// no arbitrary command string crosses the authority boundary. A durable
    /// reservation and preparation are committed before any Herdr side effect.
    pub fn start_local_agent_job<R: orbit_application::agent_runtime::AgentRuntime>(
        &mut self,
        launch: LocalAgentLaunch<'_>,
        runtime: &mut R,
    ) -> Result<serde_json::Value, String> {
        use crate::agent_dispatch::{DispatchResult, dispatch_prepared_job};
        let LocalAgentLaunch {
            task_id,
            job_id,
            agent,
            machine,
            repository,
            worktree,
            at_ms,
        } = launch;
        if !matches!(agent, "codex" | "opencode") {
            return Err("unsupported agent kind".into());
        }
        for value in [task_id, job_id, machine, repository] {
            if value.is_empty() || value.len() > 1024 || value.chars().any(|c| c.is_control()) {
                return Err("invalid agent launch input".into());
            }
        }
        if !std::path::Path::new(repository).is_absolute() || at_ms < 0 {
            return Err("repository must be an absolute path and timestamp nonnegative".into());
        }
        let (_, tasks) = self
            .tasks()
            .map_err(|e| format!("task read failed: {e:?}"))?;
        let task = tasks
            .iter()
            .find(|t| t.id == task_id)
            .ok_or("task not found")?;
        let project = task.project_id.clone().ok_or("task has no project")?;
        let job = AgentJob {
            id: job_id.into(),
            request_id: job_id.into(),
            project_id: ProjectId::new(project).map_err(|e| format!("project ID: {e:?}"))?,
            task_id: TaskId::new(task_id.to_owned()).map_err(|e| format!("task ID: {e:?}"))?,
            launch_revision: 0,
            agent_kind: agent.into(),
            machine_ref: machine.into(),
            runtime_kind: "herdr".into(),
            repository_ref: repository.into(),
            state: orbit_domain::AgentJobState::Queued,
            revision: 0,
            created_at_ms: at_ms,
            updated_at_ms: at_ms,
            last_seen_at_ms: None,
            runtime_session_ref: None,
        };
        let saved = self
            .authority
            .conn
            .reserve_agent_job(&job)
            .map_err(|e| e.to_string())?;
        // A duplicate request must never result in a second execution.
        if saved.state != orbit_domain::AgentJobState::Queued {
            return Ok(
                serde_json::json!({"job_id":job_id,"outcome":"already_reserved","state":saved.state.as_str()}),
            );
        }
        self.authority
            .conn
            .set_agent_job_worktree(job_id, worktree)
            .map_err(|e| e.to_string())?;
        let prepared = self
            .authority
            .conn
            .accept_agent_job_command(&format!("{job_id}:prepare"), job_id, 0, "prepare", at_ms)
            .map_err(|e| e.to_string())?;
        let result = dispatch_prepared_job(
            &mut self.authority.conn,
            runtime,
            job_id,
            prepared.revision,
            &format!("{job_id}:launch"),
            at_ms,
        )?;
        Ok(match result {
            DispatchResult::Confirmed(session) => {
                serde_json::json!({"job_id":job_id,"outcome":"confirmed","session":session})
            }
            DispatchResult::AlreadyClaimed => {
                serde_json::json!({"job_id":job_id,"outcome":"already_claimed"})
            }
            DispatchResult::NeedsReconciliation(reason) => {
                serde_json::json!({"job_id":job_id,"outcome":"reconcile","reason":reason})
            }
        })
    }

    /// Resume a terminal job via a new job, preserving the last handoff in its
    /// immutable execution brief before Herdr is invoked.
    pub fn resume_local_agent_job<R: orbit_application::agent_runtime::AgentRuntime>(
        &mut self,
        prior_id: &str,
        new_id: &str,
        runtime: &mut R,
        at_ms: i64,
    ) -> Result<serde_json::Value, String> {
        use crate::agent_dispatch::{DispatchResult, dispatch_prepared_job};
        let previous = self
            .authority
            .conn
            .get_agent_job(prior_id)
            .map_err(|e| e.to_string())?
            .ok_or("previous job not found")?;
        if !previous.state.is_terminal() {
            return Err("previous job must be terminal before resuming".into());
        }
        if previous.machine_ref != "local" {
            return Err("remote resumes require a machine-side executor".into());
        }
        if self
            .authority
            .conn
            .list_agent_job_handoffs(prior_id)
            .map_err(|e| e.to_string())?
            .is_empty()
        {
            return Err("previous job has no durable handoff".into());
        }
        // A claimed or ambiguous launch may already have created an external
        // session. Resuming it would risk a second live agent, so require the
        // operator to reconcile the runtime first.
        if let Some((_, dispatch_state, session_ref)) = self
            .authority
            .conn
            .agent_launch_claim(prior_id)
            .map_err(|e| e.to_string())?
            && matches!(dispatch_state.as_str(), "claimed" | "ambiguous")
            && session_ref.is_none()
            && previous.runtime_session_ref.is_none()
        {
            return Err(format!(
                "previous job has an unconfirmed launch claim ({dispatch_state}); the external \
                 session may still be running. Run `orbit agent sweep` or reconcile the runtime \
                 before resuming"
            ));
        }
        let new_job = AgentJob {
            id: new_id.into(),
            request_id: new_id.into(),
            state: orbit_domain::AgentJobState::Queued,
            revision: 0,
            launch_revision: previous.launch_revision + 1,
            created_at_ms: at_ms,
            updated_at_ms: at_ms,
            last_seen_at_ms: None,
            runtime_session_ref: None,
            ..previous
        };
        self.authority
            .conn
            .reserve_agent_job(&new_job)
            .map_err(|e| e.to_string())?;
        self.authority
            .conn
            .attach_agent_resume_context(new_id, prior_id)
            .map_err(|e| e.to_string())?;
        let worktree = self
            .authority
            .conn
            .agent_job_worktree(prior_id)
            .map_err(|e| e.to_string())?;
        self.authority
            .conn
            .set_agent_job_worktree(new_id, worktree)
            .map_err(|e| e.to_string())?;
        let prepared = self
            .authority
            .conn
            .accept_agent_job_command(&format!("{new_id}:prepare"), new_id, 0, "prepare", at_ms)
            .map_err(|e| e.to_string())?;
        let outcome = dispatch_prepared_job(
            &mut self.authority.conn,
            runtime,
            new_id,
            prepared.revision,
            &format!("{new_id}:launch"),
            at_ms,
        )?;
        Ok(match outcome {
            DispatchResult::Confirmed(session) => {
                serde_json::json!({"job_id":new_id,"resumed_from":prior_id,"outcome":"confirmed","session":session})
            }
            DispatchResult::AlreadyClaimed => {
                serde_json::json!({"job_id":new_id,"outcome":"already_claimed"})
            }
            DispatchResult::NeedsReconciliation(reason) => {
                serde_json::json!({"job_id":new_id,"outcome":"reconcile","reason":reason})
            }
        })
    }

    pub fn dispatch_local_agent_job<R: orbit_application::agent_runtime::AgentLauncher>(
        &mut self,
        runtime: &mut R,
        id: &str,
        revision: i64,
        token: &str,
        at_ms: i64,
    ) -> Result<crate::agent_dispatch::DispatchResult, String> {
        crate::agent_dispatch::dispatch_prepared_job(
            &mut self.authority.conn,
            runtime,
            id,
            revision,
            token,
            at_ms,
        )
    }
    pub fn record_local_agent_handoff(
        &mut self,
        job_id: &str,
        handoff_id: &str,
        payload: &serde_json::Value,
        at_ms: i64,
    ) -> Result<(), String> {
        self.authority
            .conn
            .record_agent_job_handoff(job_id, handoff_id, payload, at_ms)
            .map_err(|e| e.to_string())
    }

    /// Operator commands mutate only the local authority's job record. A job
    /// assigned to another machine must be handled by that machine's executor.
    fn require_local_agent_job(&self, job_id: &str, action: &str) -> Result<AgentJobView, String> {
        let job = self
            .local_agent_job(job_id)?
            .ok_or_else(|| "unknown agent job".to_string())?;
        if job.machine_ref != "local" {
            return Err(format!(
                "cannot {action} agent job {job_id}: it targets machine '{}', not the local \
                 machine; use the machine-side executor for remote jobs",
                job.machine_ref
            ));
        }
        Ok(job)
    }

    /// Explicit local completion of a non-terminal job. The job record becomes
    /// terminal; durable handoffs, the review record and the Orbit task are
    /// untouched, and the external Herdr session is not stopped. A terminal job
    /// is refused with an actionable message instead of being completed twice.
    pub fn complete_local_agent_job(
        &mut self,
        job_id: &str,
        summary: &str,
        at_ms: i64,
    ) -> Result<serde_json::Value, String> {
        let previous = self.require_local_agent_job(job_id, "complete")?;
        let job = self
            .authority
            .conn
            .complete_agent_job(job_id, at_ms, summary)
            .map_err(|e| e.to_string())?;
        let handoffs = self
            .authority
            .conn
            .list_agent_job_handoffs(job_id)
            .map_err(|e| e.to_string())?
            .len();
        Ok(serde_json::json!({
            "job_id": job.id,
            "previous_state": previous.state,
            "state": job.state.as_str(),
            "revision": job.revision,
            "event": "completed",
            "handoffs_preserved": handoffs,
            "task_completed": false,
            "runtime_stopped": false,
            "note": "job record completed; the external session may keep running"
        }))
    }

    /// Interrupt the Orbit job record of a non-terminal local job. Reuses the
    /// revision-guarded lifecycle transition, so the operator reason is the
    /// recorded event reason and an already terminal job is refused. This does
    /// not stop the external Herdr session: the session may keep running.
    pub fn interrupt_local_agent_job(
        &mut self,
        job_id: &str,
        reason: &str,
        at_ms: i64,
    ) -> Result<serde_json::Value, String> {
        let job = self.require_local_agent_job(job_id, "interrupt")?;
        if matches!(job.state.as_str(), "completed" | "failed" | "interrupted") {
            return Err(format!(
                "agent job is already terminal ({}); resume it to create a successor",
                job.state
            ));
        }
        let reason = reason.trim();
        if reason.len() > 4096 {
            return Err("interrupt reason exceeds 4096 bytes".into());
        }
        let reason = if reason.is_empty() {
            "operator interrupt"
        } else {
            reason
        };
        let interrupted = self
            .authority
            .conn
            .transition_agent_job(
                job_id,
                job.revision,
                orbit_domain::AgentJobState::Interrupted,
                at_ms,
                reason,
            )
            .map_err(|e| e.to_string())?;
        let handoffs = self
            .authority
            .conn
            .list_agent_job_handoffs(job_id)
            .map_err(|e| e.to_string())?
            .len();
        Ok(serde_json::json!({
            "job_id": interrupted.id,
            "previous_state": job.state,
            "state": interrupted.state.as_str(),
            "revision": interrupted.revision,
            "reason": reason,
            "event": "interrupted",
            "handoffs_preserved": handoffs,
            "task_completed": false,
            "runtime_stopped": false,
            "note": "job record interrupted; the external session may keep running"
        }))
    }
    pub fn review_local_agent_job(
        &mut self,
        job_id: &str,
        handoff_id: &str,
        decision: &str,
        reviewer: &str,
        reason: &str,
        at_ms: i64,
    ) -> Result<serde_json::Value, String> {
        self.authority
            .conn
            .review_agent_job(job_id, handoff_id, decision, reviewer, reason, at_ms)
            .map_err(|e| e.to_string())
    }
    pub fn local_agent_repository(&self, job_id: &str) -> Result<String, String> {
        self.authority
            .conn
            .get_agent_job(job_id)
            .map_err(|e| e.to_string())?
            .map(|j| j.repository_ref)
            .ok_or_else(|| "unknown agent job".into())
    }
    /// Return the trusted machine-local evidence selector; never substitute the
    /// original repository when this job requested isolated worktree execution.
    pub fn local_agent_evidence_location(
        &self,
        job_id: &str,
    ) -> Result<(String, Option<String>), String> {
        let job = self
            .authority
            .conn
            .get_agent_job(job_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "unknown agent job".to_string())?;
        if job.machine_ref != "local" {
            return Err("Git evidence requires a trusted executor on the target machine; remote capture is not supported locally".into());
        }
        if !self
            .authority
            .conn
            .agent_job_worktree(job_id)
            .map_err(|e| e.to_string())?
        {
            return Ok((job.repository_ref, None));
        }
        let (token, _, _) = self
            .authority
            .conn
            .agent_launch_claim(job_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "worktree job has no launch claim".to_string())?;
        // This naming contract matches the deterministic branch in HerdrRuntime.
        let mut hash: u64 = 0xcbf29ce484222325;
        for b in token.bytes() {
            hash ^= u64::from(b);
            hash = hash.wrapping_mul(0x100000001b3);
        }
        Ok((
            job.repository_ref,
            Some(format!("refs/heads/orbit/orbit{hash:016x}")),
        ))
    }
    pub fn local_agent_review(&self, job_id: &str) -> Result<Option<serde_json::Value>, String> {
        self.authority
            .conn
            .agent_job_review(job_id)
            .map_err(|e| e.to_string())
    }
    pub fn local_agent_handoffs(&self, job_id: &str) -> Result<Vec<serde_json::Value>, String> {
        self.authority
            .conn
            .list_agent_job_handoffs(job_id)
            .map_err(|e| e.to_string())
    }
    /// Local operator read of one job, annotated with its durable claim state so
    /// a watching operator sees "waiting for an executor" instead of guessing.
    pub fn local_agent_job(&self, job_id: &str) -> Result<Option<AgentJobView>, String> {
        let job = self
            .authority
            .conn
            .get_agent_job(job_id)
            .map_err(|e| e.to_string())?;
        match job {
            Some(job) => self
                .agent_job_view(job)
                .map(Some)
                .map_err(|e| format!("{e:?}")),
            None => Ok(None),
        }
    }
    /// Agent polling is explicit and never launches any new sessions.
    pub fn local_agent_poll_candidates(&self) -> Result<Vec<AgentJobView>, String> {
        let jobs = self
            .authority
            .conn
            .agent_job_poll_candidates()
            .map_err(|e| e.to_string())?;
        self.agent_job_views(jobs).map_err(|e| format!("{e:?}"))
    }
    /// Record a stable automatic snapshot after a runtime becomes reviewable.
    /// The snapshot is repository evidence, never an assertion of agent authorship.
    pub fn record_local_agent_auto_evidence(
        &mut self,
        job_id: &str,
        revision: i64,
        payload: &serde_json::Value,
        at_ms: i64,
    ) -> Result<String, String> {
        let id = format!("auto-evidence-{job_id}-{revision}");
        self.record_local_agent_handoff(job_id, &id, payload, at_ms)?;
        Ok(id)
    }
    pub fn local_agent_overview(&self) -> Result<Vec<serde_json::Value>, String> {
        self.authority
            .conn
            .agent_job_overview()
            .map_err(|e| e.to_string())
    }
    pub fn local_agent_attention(&self) -> Result<Vec<AgentJobView>, String> {
        let jobs = self
            .authority
            .conn
            .list_agent_jobs_needing_attention()
            .map_err(|e| e.to_string())?;
        self.agent_job_views(jobs).map_err(|e| format!("{e:?}"))
    }
    pub fn inspect_local_agent_job<R: orbit_application::agent_runtime::AgentInspector>(
        &mut self,
        runtime: &mut R,
        job_id: &str,
        revision: i64,
        at_ms: i64,
    ) -> Result<AgentJobView, String> {
        crate::agent_monitor::reconcile_job(
            &mut self.authority.conn,
            runtime,
            job_id,
            revision,
            at_ms,
        )
        .map_err(|e| e.to_string())
        .and_then(|job| self.agent_job_view(job).map_err(|e| format!("{e:?}")))
    }
}

#[cfg(test)]
mod operator_tests {
    use super::*;
    use orbit_application::agent_runtime::{
        AgentLaunchOutcome, AgentLaunchRequest, AgentRuntime, AgentSessionObservation,
    };
    use orbit_application::{
        CreateProjectCommand, CreateProjectWithActivityCommand, CreateTaskCommand,
        CreateTaskWithActivityCommand, TransitionTaskCommand, TransitionTaskWithActivityCommand,
    };
    use orbit_domain::{ActivityId, ActivityOrigin, ProjectId, TaskId, TaskScope, TaskStatus};

    struct FakeRuntime {
        launches: usize,
        ambiguous: bool,
        inspection: AgentSessionObservation,
    }

    impl AgentRuntime for FakeRuntime {
        fn launch(&mut self, _: &AgentLaunchRequest) -> Result<AgentLaunchOutcome, String> {
            self.launches += 1;
            if self.ambiguous {
                Ok(AgentLaunchOutcome::Ambiguous {
                    diagnostic: "transport lost after request".into(),
                })
            } else {
                Ok(AgentLaunchOutcome::Confirmed {
                    session_ref: format!("session-{}", self.launches),
                })
            }
        }

        fn inspect(&mut self, _: &str, _: &str) -> Result<AgentSessionObservation, String> {
            Ok(self.inspection.clone())
        }
    }

    fn ready_task(host: &mut AuthoritativeHost, prefix: &str) -> (ProjectId, TaskId) {
        let project = ProjectId::new(format!("{prefix}-project")).unwrap();
        let task = TaskId::new(format!("{prefix}-task")).unwrap();
        host.create_project_accepted(
            "operator-test",
            &format!("{prefix}-project-command"),
            0,
            CreateProjectWithActivityCommand {
                project: CreateProjectCommand {
                    id: project.clone(),
                    name: "Operator guards".into(),
                },
                activity_id: ActivityId::new(format!("{prefix}-activity-project")).unwrap(),
                origin: ActivityOrigin::Human,
                summary: "project".into(),
            },
        )
        .unwrap();
        host.create_task_accepted(
            "operator-test",
            &format!("{prefix}-task-command"),
            1,
            CreateTaskWithActivityCommand {
                task: CreateTaskCommand {
                    id: task.clone(),
                    title: "Guard operator commands".into(),
                    scope: TaskScope::try_new(Some(project.clone()), None).unwrap(),
                },
                activity_id: ActivityId::new(format!("{prefix}-activity-task")).unwrap(),
                origin: ActivityOrigin::Human,
                summary: "task".into(),
            },
        )
        .unwrap();
        host.transition_task_accepted(
            "operator-test",
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

    fn handoff(host: &mut AuthoritativeHost, job_id: &str, at_ms: i64) {
        host.record_local_agent_handoff(
            job_id,
            &format!("handoff-{job_id}"),
            &serde_json::json!({
                "summary": "Implemented the guarded operation",
                "validation": {"status": "not_run"},
                "next_steps": ["Review the change"]
            }),
            at_ms,
        )
        .unwrap();
    }

    fn start(
        host: &mut AuthoritativeHost,
        task: &TaskId,
        job_id: &str,
        machine: &str,
        runtime: &mut FakeRuntime,
    ) {
        let repository = std::env::temp_dir()
            .join("orbit-agent-operator-guards")
            .to_string_lossy()
            .into_owned();
        host.start_local_agent_job(
            LocalAgentLaunch {
                task_id: task.as_str(),
                job_id,
                agent: "codex",
                machine,
                repository: &repository,
                worktree: false,
                at_ms: 1_000,
            },
            runtime,
        )
        .unwrap();
    }

    #[test]
    fn resume_refuses_an_unconfirmed_launch_claim() {
        let mut host = AuthoritativeHost::open(":memory:").unwrap();
        let (_project, task) = ready_task(&mut host, "ambiguous");
        let mut runtime = FakeRuntime {
            launches: 0,
            ambiguous: true,
            inspection: AgentSessionObservation::AbsentConfirmed,
        };
        start(&mut host, &task, "ambiguous-job", "local", &mut runtime);

        // A lookup that finds no session cannot prove the ambiguous launch
        // never happened; the job is held for reconciliation.
        let job = host.local_agent_job("ambiguous-job").unwrap().unwrap();
        let observed = host
            .inspect_local_agent_job(&mut runtime, "ambiguous-job", job.revision, 2_000)
            .unwrap();
        assert_eq!(observed.state, "unreachable");

        // Completion is still allowed, but resuming the same predecessor would
        // risk a second live session.
        let completed = host
            .complete_local_agent_job("ambiguous-job", "operator declared done", 3_000)
            .unwrap();
        assert_eq!(completed["state"], "completed");
        handoff(&mut host, "ambiguous-job", 3_100);

        let mut successor = FakeRuntime {
            launches: 0,
            ambiguous: false,
            inspection: AgentSessionObservation::Unreachable,
        };
        let error = host
            .resume_local_agent_job(
                "ambiguous-job",
                "ambiguous-successor",
                &mut successor,
                4_000,
            )
            .unwrap_err();
        assert!(
            error.contains("reconcile") && error.contains("sweep"),
            "{error}"
        );
        assert_eq!(successor.launches, 0);
        assert!(
            host.local_agent_job("ambiguous-successor")
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn resume_allows_a_confirmed_session_predecessor() {
        let mut host = AuthoritativeHost::open(":memory:").unwrap();
        let (_project, task) = ready_task(&mut host, "confirmed");
        let mut runtime = FakeRuntime {
            launches: 0,
            ambiguous: false,
            inspection: AgentSessionObservation::Present {
                session_ref: "session-1".into(),
                runtime_state: "working".into(),
            },
        };
        start(&mut host, &task, "confirmed-job", "local", &mut runtime);
        let job = host.local_agent_job("confirmed-job").unwrap().unwrap();
        let observed = host
            .inspect_local_agent_job(&mut runtime, "confirmed-job", job.revision, 2_000)
            .unwrap();
        assert_eq!(observed.state, "running");

        let completed = host
            .complete_local_agent_job("confirmed-job", "reviewed", 3_000)
            .unwrap();
        assert_eq!(completed["state"], "completed");
        handoff(&mut host, "confirmed-job", 3_100);

        let mut successor = FakeRuntime {
            launches: 0,
            ambiguous: false,
            inspection: AgentSessionObservation::Present {
                session_ref: "session-2".into(),
                runtime_state: "working".into(),
            },
        };
        let resumed = host
            .resume_local_agent_job(
                "confirmed-job",
                "confirmed-successor",
                &mut successor,
                4_000,
            )
            .unwrap();
        assert_eq!(resumed["outcome"], "confirmed");
        assert_eq!(resumed["resumed_from"], "confirmed-job");
        assert_eq!(successor.launches, 1);
    }

    #[test]
    fn local_operator_commands_refuse_remote_machine_jobs() {
        let mut host = AuthoritativeHost::open(":memory:").unwrap();
        let (_project, task) = ready_task(&mut host, "remote");
        let mut runtime = FakeRuntime {
            launches: 0,
            ambiguous: false,
            inspection: AgentSessionObservation::Unreachable,
        };
        start(&mut host, &task, "remote-job", "remote-01", &mut runtime);

        let before = host.local_agent_job("remote-job").unwrap().unwrap();
        let complete_error = host
            .complete_local_agent_job("remote-job", "done", 2_000)
            .unwrap_err();
        assert!(
            complete_error.contains("local machine")
                && complete_error.contains("machine-side executor"),
            "{complete_error}"
        );
        let interrupt_error = host
            .interrupt_local_agent_job("remote-job", "operator stop", 2_000)
            .unwrap_err();
        assert!(
            interrupt_error.contains("local machine")
                && interrupt_error.contains("machine-side executor"),
            "{interrupt_error}"
        );

        let after = host.local_agent_job("remote-job").unwrap().unwrap();
        assert_eq!(after.state, before.state);
        assert_eq!(after.revision, before.revision);
    }
}
