//! Local agent commands through the authoritative host.
//! Transport routing remains in the CLI entrypoint; no independent SQLite authority.
use super::*;

pub(super) fn run_agent(
    host: &mut orbit_runtime::AuthoritativeHost,
    command: AgentSubcommand,
    default_agent_provider: Option<&str>,
) -> Result<Response, CliError> {
    use orbit_runtime::agent_dispatch::DispatchResult;
    use orbit_runtime::herdr_runtime::HerdrRuntime;
    let result = match command {
        AgentSubcommand::Start {
            task_id,
            agent,
            machine,
            repository,
            worktree,
        } => {
            let agent = agent
                .or_else(|| default_agent_provider.map(ToOwned::to_owned))
                .unwrap_or_else(|| "codex".to_owned());
            let id = generate_id("agent-job");
            let mut runtime = HerdrRuntime::default();
            host.start_local_agent_job(
                orbit_runtime::agent_authority::LocalAgentLaunch {
                    task_id: &task_id,
                    job_id: &id,
                    agent: &agent,
                    machine: &machine,
                    repository: &repository.to_string_lossy(),
                    worktree,
                    at_ms: now_ms(),
                },
                &mut runtime,
            )
            .map_err(CliError::Usage)?
        }
        AgentSubcommand::Launch {
            job_id,
            revision,
            claim_token,
            at_ms,
        } => {
            if revision < 0 || at_ms < 0 || claim_token.trim().is_empty() {
                return Err(CliError::Usage(
                    "revision, timestamp and claim token must be valid".into(),
                ));
            }
            let mut runtime = HerdrRuntime::default();
            let outcome = host
                .dispatch_local_agent_job(&mut runtime, &job_id, revision, &claim_token, at_ms)
                .map_err(CliError::Usage)?;
            match outcome {
                DispatchResult::Confirmed(session) => {
                    serde_json::json!({"outcome":"confirmed","session":session})
                }
                DispatchResult::AlreadyClaimed => {
                    serde_json::json!({"outcome":"already_claimed","action":"inspect"})
                }
                DispatchResult::NeedsReconciliation(reason) => {
                    serde_json::json!({"outcome":"reconcile","reason":reason})
                }
            }
        }
        AgentSubcommand::Inspect {
            job_id,
            revision,
            at_ms,
        } => {
            if revision < 0 || at_ms < 0 {
                return Err(CliError::Usage(
                    "revision and timestamp must be nonnegative".into(),
                ));
            }
            let mut runtime = HerdrRuntime::default();
            let job = host
                .inspect_local_agent_job(&mut runtime, &job_id, revision, at_ms)
                .map_err(CliError::Usage)?;
            let evidence = auto_agent_evidence(host, &job_id, &job, 20);
            serde_json::json!({"job_id":job.id,"state":job.state,"revision":job.revision,"session":job.runtime_session_ref,"automatic_evidence":evidence})
        }
        AgentSubcommand::Sweep { max_commits } => {
            if max_commits == 0 || max_commits > 100 {
                return Err(CliError::Usage("max-commits must be 1..=100".into()));
            }
            let candidates = host
                .local_agent_poll_candidates()
                .map_err(CliError::Usage)?;
            let mut results = Vec::new();
            for candidate in candidates {
                // Per-job failures are recorded without aborting monitoring of other jobs.
                let mut runtime = HerdrRuntime::default();
                let result = match host.inspect_local_agent_job(
                    &mut runtime,
                    &candidate.id,
                    candidate.revision,
                    now_ms(),
                ) {
                    Ok(job) => {
                        let evidence = auto_agent_evidence(host, &job.id, &job, max_commits);
                        serde_json::json!({"job_id":job.id,"state":job.state,"revision":job.revision,"automatic_evidence":evidence})
                    }
                    Err(e) => serde_json::json!({"job_id":candidate.id,"error":e}),
                };
                results.push(result);
            }
            serde_json::json!({"polled":results.len(),"jobs":results})
        }
        AgentSubcommand::Status { job_id } => {
            let job = host
                .local_agent_job(&job_id)
                .map_err(CliError::Usage)?
                .ok_or_else(|| CliError::Usage("agent job not found".into()))?;
            serde_json::json!({"job_id":job.id,"task_id":job.task_id.as_str(),"project_id":job.project_id.as_str(),"state":job.state.as_str(),"revision":job.revision,"session":job.runtime_session_ref,"machine":job.machine_ref})
        }
        AgentSubcommand::Handoff { job_id, file } => {
            let bytes =
                fs::read(&file).map_err(|e| CliError::Io(format!("cannot read handoff: {e}")))?;
            if bytes.len() > 256 * 1024 {
                return Err(CliError::Usage("handoff exceeds 256 KiB".into()));
            }
            let payload: serde_json::Value = serde_json::from_slice(&bytes)
                .map_err(|e| CliError::Usage(format!("invalid handoff JSON: {e}")))?;
            let handoff_id = generate_id("handoff");
            host.record_local_agent_handoff(&job_id, &handoff_id, &payload, now_ms())
                .map_err(CliError::Usage)?;
            serde_json::json!({"job_id":job_id,"handoff_id":handoff_id,"saved":true})
        }
        AgentSubcommand::Continue { job_id, message } => {
            let job = host
                .local_agent_job(&job_id)
                .map_err(CliError::Usage)?
                .ok_or_else(|| CliError::Usage("agent job not found".into()))?;
            let session = job
                .runtime_session_ref
                .ok_or_else(|| CliError::Usage("job has no confirmed Herdr session".into()))?;
            if !matches!(job.state.as_str(), "running" | "waiting_input" | "blocked") {
                return Err(CliError::Usage(
                    "agent must be confirmed active before continuation; inspect first".into(),
                ));
            }
            let mut runtime = HerdrRuntime::default();
            let report = runtime
                .continue_existing(&job.machine_ref, &session, &message)
                .map_err(|e| {
                    CliError::Usage(format!(
                        "continuation delivery uncertain: {e}; inspect before retry"
                    ))
                })?;
            serde_json::json!({"job_id":job_id,"session":session,"prompt_result":report,
                "task_completed":false})
        }
        AgentSubcommand::Handoffs { job_id } => serde_json::Value::Array(
            host.local_agent_handoffs(&job_id)
                .map_err(CliError::Usage)?,
        ),
        AgentSubcommand::Resume { job_id } => {
            let new_id = generate_id("agent-job");
            let mut runtime = HerdrRuntime::default();
            host.resume_local_agent_job(&job_id, &new_id, &mut runtime, now_ms())
                .map_err(CliError::Usage)?
        }
        AgentSubcommand::ResumeContext { job_id } => host
            .local_agent_resume_context(&job_id)
            .map_err(CliError::Usage)?,
        AgentSubcommand::Capture {
            job_id,
            max_commits,
        } => {
            if max_commits == 0 || max_commits > 100 {
                return Err(CliError::Usage("max-commits must be 1..=100".into()));
            }
            let job = host
                .local_agent_job(&job_id)
                .map_err(CliError::Usage)?
                .ok_or_else(|| CliError::Usage("unknown agent job".into()))?;
            // Read repository path from the authoritative record, never from a supplied command.
            let (repo, branch) = host
                .local_agent_evidence_location(&job_id)
                .map_err(CliError::Usage)?;
            let evidence = git_evidence_for_job(&repo, branch.as_deref(), max_commits)?;
            let payload = serde_json::json!({"summary":"Git evidence snapshot (no agent claims inferred)",
                "validation":{"status":"not_recorded"},"next_steps":[],"git":evidence});
            let handoff_id = generate_id("handoff");
            host.record_local_agent_handoff(&job_id, &handoff_id, &payload, now_ms())
                .map_err(CliError::Usage)?;
            serde_json::json!({"job_id":job.id,"handoff_id":handoff_id,"evidence":evidence})
        }
        AgentSubcommand::Review {
            job_id,
            handoff_id,
            reviewer,
            decision,
            reason,
        } => host
            .review_local_agent_job(
                &job_id,
                &handoff_id,
                &decision,
                &reviewer,
                &reason,
                now_ms(),
            )
            .map_err(CliError::Usage)?,
        AgentSubcommand::ReviewStatus { job_id } => host
            .local_agent_review(&job_id)
            .map_err(CliError::Usage)?
            .unwrap_or(serde_json::Value::Null),
        AgentSubcommand::Overview => {
            serde_json::Value::Array(host.local_agent_overview().map_err(CliError::Usage)?)
        }
        AgentSubcommand::Attention => {
            let jobs = host.local_agent_attention().map_err(CliError::Usage)?;
            serde_json::Value::Array(jobs.into_iter().map(|j|serde_json::json!({"job_id":j.id,"task_id":j.task_id.as_str(),"state":j.state.as_str(),"revision":j.revision,"machine":j.machine_ref})).collect())
        }
        AgentSubcommand::QueueStart { .. }
        | AgentSubcommand::QueueResume { .. }
        | AgentSubcommand::QueueContinue { .. }
        | AgentSubcommand::QueueHandoff { .. }
        | AgentSubcommand::QueueReview { .. }
        | AgentSubcommand::Pending
        | AgentSubcommand::Flush => {
            return Err(CliError::Usage(
                "agent offline queue is only available in connected mode".into(),
            ));
        }
    };
    Ok(Response::Authority { result })
}

/// Capture read-only repository evidence after the agent becomes idle/completed.
/// Never auto-approve or mark the Orbit task complete, and never create a second launch.
fn auto_agent_evidence(
    host: &mut orbit_runtime::AuthoritativeHost,
    job_id: &str,
    job: &orbit_runtime::agent_authority::AgentJobView,
    max_commits: usize,
) -> serde_json::Value {
    if !matches!(job.state.as_str(), "waiting_input" | "completed") {
        return serde_json::json!({"outcome":"not_reviewable"});
    }
    let (repo, branch) = match host.local_agent_evidence_location(job_id) {
        Ok(repo) => repo,
        Err(e) => return serde_json::json!({"outcome":"error","reason":e}),
    };
    let git = match git_evidence_for_job(&repo, branch.as_deref(), max_commits) {
        Ok(evidence) => evidence,
        Err(e) => return serde_json::json!({"outcome":"git_unavailable","reason":format!("{e:?}")}),
    };
    // Avoid flooding the audit log with identical snapshots on repeated polls.
    match host.local_agent_handoffs(job_id) {
        Ok(existing)
            if existing.iter().any(|entry| {
                let previous = &entry["payload"];
                previous["capture"]["kind"] == "automatic" && previous["git"] == git
            }) =>
        {
            return serde_json::json!({"outcome":"unchanged"});
        }
        Err(e) => return serde_json::json!({"outcome":"error","reason":e}),
        _ => {}
    }
    let payload = serde_json::json!({
        "summary":"Automatically captured repository snapshot; agent attribution unverified",
        "validation":{"status":"not_recorded"},"next_steps":["Review the evidence and confirm task acceptance criteria"],
        "git":git,"capture":{"kind":"automatic","job_revision":job.revision}
    });
    // Deterministic identity plus current state protects against repeated polling.
    // A conflicting replay is surfaced; it cannot silently overwrite evidence.
    match host.record_local_agent_auto_evidence(job_id, job.revision, &payload, job.updated_at_ms) {
        Ok(id) => serde_json::json!({"outcome":"saved","handoff_id":id}),
        Err(e) => serde_json::json!({"outcome":"error","reason":e}),
    }
}
