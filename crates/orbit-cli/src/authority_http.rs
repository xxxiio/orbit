//! Explicit M18 server-authoritative task client. No Git workspace required.
//! Local/offline queuing is not enabled until pending commands can be reconciled safely.
use crate::commands::{AuthorityTaskAction, SyncAuthorityArgs};
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Debug, Deserialize, Serialize)]
struct ServerTasks {
    revision: i64,
    tasks: Vec<Value>,
}
#[derive(Debug, Deserialize)]
struct ServerProjects {
    revision: i64,
    projects: Vec<Value>,
}

pub fn invoke(args: SyncAuthorityArgs) -> Result<Value, String> {
    let url = reqwest::Url::parse(&args.server).map_err(|e| e.to_string())?;
    let loopback = matches!(
        url.host_str(),
        Some("127.0.0.1" | "localhost" | "::1" | "[::1]")
    );
    if url.scheme() != "https" && !(url.scheme() == "http" && loopback) {
        return Err("server authority requires HTTPS except for loopback development".into());
    }
    if !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err("server URL may not contain credentials, query or fragment".into());
    }
    if args.device.trim().is_empty() || args.token.is_empty() {
        return Err("device ID and token are required".into());
    }
    let base = url.as_str().trim_end_matches('/');
    let client = Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| e.to_string())?;
    let endpoint = match args.action {
        AuthorityTaskAction::Snapshot => {
            let response = client
                .get(format!("{base}/v1/authoritative/snapshot"))
                .bearer_auth(&args.token)
                .header("x-orbit-device-id", &args.device)
                .send()
                .map_err(|e| e.to_string())?;
            let decoded = decode(response)?;
            validate_snapshot(&decoded)?;
            return Ok(decoded);
        }
        AuthorityTaskAction::DomainRead { kind } => {
            let response = client
                .get(format!("{base}/v1/authoritative/{}", kind.reads_path()))
                .bearer_auth(&args.token)
                .header("x-orbit-device-id", &args.device)
                .send()
                .map_err(|e| e.to_string())?;
            let decoded = decode(response)?;
            validate_domain_read(kind, &decoded)?;
            return Ok(decoded);
        }
        AuthorityTaskAction::DomainCommand {
            kind,
            command_id,
            revision,
            action_json,
        } => {
            let payload = domain_payload(kind, &command_id, revision, &action_json)?;
            let response = client
                .post(format!("{base}/v1/authoritative/{}", kind.commands_path()))
                .bearer_auth(&args.token)
                .header("x-orbit-device-id", &args.device)
                .json(&payload)
                .send()
                .map_err(|e| e.to_string())?;
            let receipt = decode(response)?;
            if receipt.get("revision").and_then(Value::as_i64).is_none() {
                return Err("authority returned a malformed domain receipt".into());
            }
            return Ok(receipt);
        }
        AuthorityTaskAction::Tasks => {
            let response = client
                .get(format!("{base}/v1/authoritative/tasks"))
                .bearer_auth(&args.token)
                .header("x-orbit-device-id", &args.device)
                .send()
                .map_err(|e| e.to_string())?;
            let decoded = decode(response)?;
            let _: ServerTasks = serde_json::from_value(decoded.clone())
                .map_err(|e| format!("invalid task snapshot: {e}"))?;
            return Ok(decoded);
        }
        AuthorityTaskAction::Projects => {
            let response = client
                .get(format!("{base}/v1/authoritative/projects"))
                .bearer_auth(&args.token)
                .header("x-orbit-device-id", &args.device)
                .send()
                .map_err(|e| e.to_string())?;
            let decoded = decode(response)?;
            let projects: ServerProjects = serde_json::from_value(decoded.clone())
                .map_err(|e| format!("invalid project snapshot: {e}"))?;
            let _ = (projects.revision, projects.projects.len());
            return Ok(decoded);
        }
        AuthorityTaskAction::ProjectCreate {
            command_id,
            revision,
            id,
            name,
            workflow,
        } => {
            let mut body = json!({"command_id":command_id,"expected_revision":revision,"project_id":id,"name":name});
            if let Some(workflow) = workflow {
                body["workflow_id"] = serde_json::Value::String(workflow);
            }
            let response = client
                .post(format!("{base}/v1/authoritative/project-commands"))
                .bearer_auth(&args.token)
                .header("x-orbit-device-id", &args.device)
                .json(&body)
                .send()
                .map_err(|e| e.to_string())?;
            return decode(response);
        }
        AuthorityTaskAction::Create {
            command_id,
            revision,
            id,
            title,
            project_id,
        } => json!({
            "command_id": command_id, "expected_revision": revision,
            "action": {"type":"create", "task_id": id, "title":title, "project_id":project_id}
        }),
        AuthorityTaskAction::Status {
            command_id,
            revision,
            id,
            status,
        } => json!({
            "command_id":command_id, "expected_revision": revision,
            "action":{"type":"change_status","task_id":id,"status":status}
        }),
    };
    let response = client
        .post(format!("{base}/v1/authoritative/task-commands"))
        .bearer_auth(&args.token)
        .header("x-orbit-device-id", &args.device)
        .json(&endpoint)
        .send()
        .map_err(|e| e.to_string())?;
    decode(response)
}

/// Validate the wire contract before sending any remote mutation. A mistyped
/// family must never silently become an untyped or differently routed write.
fn domain_payload(
    kind: crate::commands::DirectDomainKind,
    command_id: &str,
    revision: i64,
    raw: &str,
) -> Result<Value, String> {
    if command_id.is_empty()
        || command_id.len() > 128
        || !command_id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
    {
        return Err("command ID must be 1-128 ASCII letters, digits, dashes or underscores".into());
    }
    if revision < 0 {
        return Err("expected revision must be nonnegative".into());
    }
    let action: Value =
        serde_json::from_str(raw).map_err(|e| format!("invalid JSON action: {e}"))?;
    if !action.is_object()
        || action.get("command_id").is_some()
        || action.get("expected_revision").is_some()
    {
        return Err("domain action must be a JSON object without envelope fields".into());
    }
    let payload = json!({"command_id":command_id,"expected_revision":revision,"action":action});
    // The same Rust types are consumed by the server's JSON handlers.
    // Deserialize and reserialize to require strict tagged actions before network I/O.
    match kind {
        crate::commands::DirectDomainKind::Schedule => {
            serde_json::from_value::<orbit_runtime::authority_schedule::ScheduleCommand>(
                payload.clone(),
            )
            .map_err(|e| format!("invalid schedule command: {e}"))?;
        }
        crate::commands::DirectDomainKind::Routine => {
            serde_json::from_value::<orbit_runtime::authority_routines::RoutineCommand>(
                payload.clone(),
            )
            .map_err(|e| format!("invalid routine command: {e}"))?;
        }
        crate::commands::DirectDomainKind::Milestone => {
            serde_json::from_value::<orbit_runtime::authority_milestones::MilestoneCommand>(
                payload.clone(),
            )
            .map_err(|e| format!("invalid milestone command: {e}"))?;
        }
        crate::commands::DirectDomainKind::Strategy => {
            serde_json::from_value::<orbit_runtime::authority_strategy::StrategyCommand>(
                payload.clone(),
            )
            .map_err(|e| format!("invalid strategy command: {e}"))?;
        }
    }
    Ok(payload)
}

fn validate_snapshot(value: &Value) -> Result<(), String> {
    if value
        .get("revision")
        .and_then(Value::as_i64)
        .filter(|r| *r >= 0)
        .is_none()
        || value
            .get("history_fingerprint")
            .and_then(Value::as_str)
            .filter(|s| {
                s.len() == 64
                    && s.bytes()
                        .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
            })
            .is_none()
    {
        return Err("invalid authoritative revision or fingerprint".into());
    }
    for key in [
        "projects",
        "tasks",
        "habits",
        "habit_occurrences",
        "metrics",
        "metric_observations",
        "schedule",
        "routines",
        "milestones",
        "strategies",
    ] {
        if !value.get(key).is_some_and(Value::is_array) {
            return Err(format!("authoritative snapshot missing {key}"));
        }
    }
    Ok(())
}

fn validate_domain_read(
    kind: crate::commands::DirectDomainKind,
    value: &Value,
) -> Result<(), String> {
    if !value.get("revision").is_some_and(Value::is_i64)
        || !value.get(kind.reads_path()).is_some_and(Value::is_array)
    {
        return Err("malformed authoritative domain projection".into());
    }
    Ok(())
}

fn decode(response: reqwest::blocking::Response) -> Result<Value, String> {
    let status = response.status();
    let body = response.text().map_err(|e| e.to_string())?;
    if !status.is_success() {
        // Do not echo arbitrary server responses or credentials in console output.
        let reason = match status.as_u16() {
            400 => "invalid command",
            401 | 403 => "device authentication failed",
            404 => "resource or endpoint not found",
            409 => "server revision, transition or migration conflict",
            _ => "server request failed",
        };
        return Err(format!(
            "{reason} (HTTP {status}); refresh server tasks before retrying"
        ));
    }
    serde_json::from_str(&body).map_err(|e| format!("invalid server response: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn strict_domain_commands_reject_wrong_family_and_envelope() {
        use crate::commands::DirectDomainKind;
        let correct = r#"{"type":"set_objective","project_id":"p1","objective":"Ship"}"#;
        assert!(domain_payload(DirectDomainKind::Strategy, "c1", 0, correct).is_ok());
        assert!(domain_payload(DirectDomainKind::Routine, "c1", 0, correct).is_err());
        assert!(domain_payload(DirectDomainKind::Strategy, "bad id", 0, correct).is_err());
        assert!(domain_payload(DirectDomainKind::Strategy, "c1", -1, correct).is_err());
        assert!(
            domain_payload(
                DirectDomainKind::Strategy,
                "c1",
                0,
                r#"{"expected_revision":1}"#
            )
            .is_err()
        );
        assert!(domain_payload(DirectDomainKind::Strategy, "c1", 0, "[]").is_err());
    }
    #[test]
    fn accepted_snapshot_cannot_masquerade_as_empty() {
        let complete = json!({"revision":0,"history_fingerprint":"0".repeat(64),
            "projects":[],"tasks":[],"habits":[],"habit_occurrences":[],
            "metrics":[],"metric_observations":[],"schedule":[],"routines":[],
            "milestones":[],"strategies":[]});
        assert!(validate_snapshot(&complete).is_ok());
        let mut missing = complete.clone();
        missing.as_object_mut().unwrap().remove("strategies");
        assert!(validate_snapshot(&missing).is_err());
    }
    #[test]
    fn server_response_is_strict_json() {
        assert!(serde_json::from_str::<ServerTasks>(r#"{"revision":1,"tasks":[]}"#).is_ok());
        assert!(serde_json::from_str::<ServerTasks>(r#"{"revision":"old","tasks":[]}"#).is_err());
    }
}
