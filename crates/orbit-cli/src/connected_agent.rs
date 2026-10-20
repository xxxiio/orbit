//! Authenticated agent commands for server-authoritative deployments.
//! Never falls back to local SQLite or launches Herdr in the CLI process.
use crate::{AgentSubcommand, CliError, generate_id, now_ms};
use orbit_client::authority::{AuthorityHttp, AuthorityReplica, FlushOutcome};
use reqwest::blocking::Client;
use serde_json::{Value, json};
use std::{path::Path, time::Duration};

fn safe_server(server: &str) -> Result<String, CliError> {
    let url = reqwest::Url::parse(server).map_err(|e| CliError::Usage(e.to_string()))?;
    let loopback = matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "::1"));
    if !(url.scheme() == "https" || (loopback && url.scheme() == "http"))
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(CliError::Usage("connected Orbit authority requires HTTPS (HTTP allowed on loopback only) and a clean URL".into()));
    }
    Ok(server.trim_end_matches('/').to_owned())
}

pub fn run(
    server: &str,
    db: &Path,
    command: AgentSubcommand,
    default_agent_provider: Option<&str>,
) -> Result<Value, CliError> {
    let base = safe_server(server)?;
    // Explicit offline operations never need a network token. Commands are
    // bound to this authority/device and retain stable IDs for replay.
    let offline = matches!(
        &command,
        AgentSubcommand::QueueStart { .. }
            | AgentSubcommand::QueueResume { .. }
            | AgentSubcommand::QueueContinue { .. }
            | AgentSubcommand::QueueHandoff { .. }
            | AgentSubcommand::QueueReview { .. }
            | AgentSubcommand::Pending
            | AgentSubcommand::Flush
    );
    if offline {
        return run_outbox(&base, db, command, default_agent_provider);
    }
    let token = std::env::var("ORBIT_SYNC_HTTP_TOKEN")
        .map_err(|_| CliError::Usage("ORBIT_SYNC_HTTP_TOKEN is required".into()))?;
    let device = std::env::var("ORBIT_AUTHORITY_DEVICE")
        .map_err(|_| CliError::Usage("ORBIT_AUTHORITY_DEVICE is required".into()))?;
    if token.is_empty() || device.trim().is_empty() || device.chars().any(|c| c.is_control()) {
        return Err(CliError::Usage(
            "invalid connected device credentials".into(),
        ));
    }
    let client = Client::builder()
        .timeout(Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| CliError::Usage(e.to_string()))?;
    let (path,id,post)=match command {
        AgentSubcommand::Start{task_id,agent,machine,repository,worktree} => {
            let agent = agent
                .or_else(|| default_agent_provider.map(ToOwned::to_owned))
                .unwrap_or_else(|| "codex".to_owned());
            ("agent-jobs/start",None,Some(json!({"job_id":generate_id("agent-job"),
                "task_id":task_id,"agent_kind":agent,
                "machine_ref":machine,"repository_ref":repository.to_string_lossy(),
                "worktree":worktree,"at_ms":now_ms()})))
        },
        AgentSubcommand::Status{job_id} => ("agent-jobs",Some(job_id),None),
        AgentSubcommand::Handoffs{job_id} => ("agent-jobs/handoffs",Some(job_id),None),
        AgentSubcommand::ResumeContext{job_id} => ("agent-jobs/resume-context",Some(job_id),None),
        AgentSubcommand::ReviewStatus{job_id} => ("agent-jobs/review",Some(job_id),None),
        AgentSubcommand::Attention => ("agent-jobs/attention",None,None),
        AgentSubcommand::Overview => ("agent-jobs/overview",None,None),
        AgentSubcommand::Continue{job_id,message} => ("agent-jobs/continue",None,
            Some(json!({"command_id":generate_id("agent-prompt"),"job_id":job_id,"message":message,"at_ms":now_ms()}))),
        AgentSubcommand::Resume{job_id} => ("agent-jobs/resume",None,
            Some(json!({"prior_job_id":job_id,"new_job_id":generate_id("agent-job"),"at_ms":now_ms()}))),
        AgentSubcommand::Handoff{job_id,file} => {
            let bytes=std::fs::read(file).map_err(|e|CliError::Io(e.to_string()))?;
            if bytes.len()>256*1024 {return Err(CliError::Usage("handoff exceeds 256 KiB".into()));}
            let payload:Value=serde_json::from_slice(&bytes).map_err(|e|CliError::Usage(e.to_string()))?;
            ("agent-jobs/handoffs",None,Some(json!({"job_id":job_id,"handoff_id":generate_id("handoff"),"payload":payload,"at_ms":now_ms()})))
        },
        AgentSubcommand::Review{job_id,handoff_id,decision,reason,..} => ("agent-jobs/review",None,
            Some(json!({"job_id":job_id,"handoff_id":handoff_id,"decision":decision,"reason":reason,"at_ms":now_ms()}))),
        _ => return Err(CliError::Usage("this agent command requires local authority or a trusted machine executor; connected mode supports start, status, attention, overview, handoffs, review, resume-context and resume".into())),
    };
    let endpoint = format!("{base}/v1/authoritative/{path}");
    let request = if let Some(body) = post {
        client.post(endpoint).json(&body)
    } else {
        let req = client.get(endpoint);
        if let Some(id) = id {
            req.query(&[("id", id)])
        } else {
            req
        }
    };
    let response = request
        .bearer_auth(token)
        .header("x-orbit-device-id", device)
        .send()
        .map_err(|e| CliError::Usage(e.to_string()))?;
    let status = response.status();
    if !status.is_success() {
        return Err(CliError::Usage(format!(
            "connected agent authority rejected request (HTTP {})",
            status.as_u16()
        )));
    }
    response
        .json()
        .map_err(|e| CliError::Serialization(e.to_string()))
}

fn run_outbox(
    base: &str,
    db: &Path,
    command: AgentSubcommand,
    default_agent_provider: Option<&str>,
) -> Result<Value, CliError> {
    let device = std::env::var("ORBIT_AUTHORITY_DEVICE")
        .map_err(|_| CliError::Usage("ORBIT_AUTHORITY_DEVICE required for agent outbox".into()))?;
    let mut outbox = AuthorityReplica::open(db).map_err(|e| CliError::Usage(format!("{e:?}")))?;
    outbox
        .bind(base)
        .map_err(|e| CliError::Usage(format!("{e:?}")))?;
    let (route, command_id, body) = match command {
        AgentSubcommand::QueueStart {
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
            (
                "agent-jobs/start",
                id.clone(),
                json!({"job_id":id,"task_id":task_id,
                "agent_kind":agent,"machine_ref":machine,
                "repository_ref":repository.to_string_lossy(),"worktree":worktree,"at_ms":now_ms()}),
            )
        }
        AgentSubcommand::QueueResume { job_id } => {
            let id = generate_id("agent-job");
            (
                "agent-jobs/resume",
                id.clone(),
                json!({"prior_job_id":job_id,
                "new_job_id":id,"at_ms":now_ms()}),
            )
        }
        AgentSubcommand::QueueContinue { job_id, message } => {
            if message.trim().is_empty() {
                return Err(CliError::Usage("message cannot be empty".into()));
            }
            let id = generate_id("agent-prompt");
            (
                "agent-jobs/continue",
                id.clone(),
                json!({"command_id":id,
                "job_id":job_id,"message":message,"at_ms":now_ms()}),
            )
        }
        AgentSubcommand::QueueHandoff { job_id, file } => {
            let bytes = std::fs::read(&file).map_err(|e| CliError::Io(e.to_string()))?;
            if bytes.len() > 256 * 1024 {
                return Err(CliError::Usage("handoff exceeds 256 KiB".into()));
            }
            let payload: Value =
                serde_json::from_slice(&bytes).map_err(|e| CliError::Usage(e.to_string()))?;
            if !payload.is_object() {
                return Err(CliError::Usage("handoff must be a JSON object".into()));
            }
            let id = generate_id("handoff");
            (
                "agent-jobs/handoffs",
                id.clone(),
                json!({"job_id":job_id,
                "handoff_id":id,"payload":payload,"at_ms":now_ms()}),
            )
        }
        AgentSubcommand::QueueReview {
            job_id,
            handoff_id,
            decision,
            reason,
        } => {
            if !matches!(decision.as_str(), "approved" | "changes_requested") {
                return Err(CliError::Usage(
                    "decision must be approved or changes_requested".into(),
                ));
            }
            let id = generate_id("agent-review");
            (
                "agent-jobs/review",
                id,
                json!({"job_id":job_id,"handoff_id":handoff_id,
                "decision":decision,"reason":reason,"at_ms":now_ms()}),
            )
        }
        AgentSubcommand::Pending => {
            return outbox
                .agent_pending()
                .map_err(|e| CliError::Usage(format!("{e:?}")));
        }
        AgentSubcommand::Flush => {
            let token = std::env::var("ORBIT_SYNC_HTTP_TOKEN")
                .map_err(|_| CliError::Usage("ORBIT_SYNC_HTTP_TOKEN required to flush".into()))?;
            let http = AuthorityHttp::new(base, &token, &device)
                .map_err(|e| CliError::Usage(format!("{e:?}")))?;
            let outcome = outbox
                .flush(&http)
                .map_err(|e| CliError::Usage(format!("{e:?}")))?;
            return Ok(match outcome {
                FlushOutcome::Empty => json!({"accepted_count":0,"status":"empty"}),
                FlushOutcome::Accepted(n) => json!({"accepted_count":n,"status":"flushed"}),
                FlushOutcome::Blocked {
                    accepted,
                    command_id,
                    reason,
                } => json!({
                    "accepted_count":accepted,"blocked_command":command_id,"reason":reason,
                    "status":"blocked"}),
            });
        }
        _ => {
            return Err(CliError::Usage(
                "unsupported offline agent operation".into(),
            ));
        }
    };
    outbox
        .queue_agent_transport(&command_id, route, &body)
        .map_err(|e| CliError::Usage(format!("{e:?}")))?;
    Ok(json!({"queued":command_id,"accepted":false,"storage":"canonical_sqlite_outbox"}))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn connected_authority_rejects_insecure_remote_url() {
        assert!(safe_server("http://example.com").is_err());
        assert!(safe_server("https://user:pass@example.com").is_err());
        assert!(safe_server("http://localhost:8080").is_ok());
    }
}
