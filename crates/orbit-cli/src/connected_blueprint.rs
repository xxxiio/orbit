//! Connected blueprint CLI transport. All authoritative mutations are performed
//! by the server's AuthoritativeHost, never against a local client SQLite file.
use crate::{CliError, blueprint_commands::BlueprintSubcommand, generate_id};
use orbit_client::authority::{AuthorityHttp, AuthorityReplica, FlushOutcome};
use reqwest::blocking::{Client, RequestBuilder};
use serde_json::{Value, json};
use std::{fs, path::Path, time::Duration};

fn server_url(server: &str) -> Result<String, CliError> {
    let parsed = reqwest::Url::parse(server).map_err(|e| CliError::Usage(e.to_string()))?;
    let loopback = matches!(parsed.host_str(), Some("localhost" | "127.0.0.1" | "::1"));
    if !(parsed.scheme() == "https" || (loopback && parsed.scheme() == "http"))
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.query().is_some()
        || parsed.fragment().is_some()
    {
        return Err(CliError::Usage("blueprint authority requires HTTPS (HTTP only on loopback), with no URL credentials, query or fragment".into()));
    }
    Ok(server.trim_end_matches('/').to_string())
}
fn read_properties(file: Option<&Path>) -> Result<Value, CliError> {
    match file {
        None => Ok(json!({})),
        Some(path) => {
            let bytes = fs::read(path).map_err(|e| CliError::Io(e.to_string()))?;
            if bytes.len() > 256 * 1024 {
                return Err(CliError::Usage(
                    "blueprint properties exceed 256 KiB".into(),
                ));
            }
            let data: Value =
                serde_json::from_slice(&bytes).map_err(|e| CliError::Usage(e.to_string()))?;
            if !data.is_object() {
                return Err(CliError::Usage(
                    "blueprint properties must be a JSON object".into(),
                ));
            }
            Ok(data)
        }
    }
}
type BlueprintEndpoint = (
    &'static str,
    Vec<(String, String)>,
    Option<Value>,
    Option<std::path::PathBuf>,
);

fn endpoint(command: BlueprintSubcommand) -> Result<BlueprintEndpoint, CliError> {
    use BlueprintSubcommand::*;
    let mut query = Vec::new();
    let mut file = None;
    let (path, body) = match command {
        Pending | Flush => {
            return Err(CliError::Usage(
                "offline command cannot use direct transport".into(),
            ));
        }
        Validate { .. } => {
            return Err(CliError::Usage("blueprint validation is local-only".into()));
        }
        Bundled => ("blueprints/bundled", None),
        List { id } => {
            if let Some(id) = id {
                query.push(("id".into(), id));
            }
            ("blueprints", None)
        }
        Show { id, version } => {
            query.extend([("id".into(), id), ("version".into(), version.to_string())]);
            ("blueprints/show", None)
        }
        Export {
            id,
            version,
            file: target,
        } => {
            query.extend([("id".into(), id), ("version".into(), version.to_string())]);
            file = target;
            ("blueprints/show", None)
        }
        Install { id } => ("blueprints/install", Some(json!({"id": id}))),
        Import { file } => {
            let bytes = fs::read(&file).map_err(|e| CliError::Io(e.to_string()))?;
            if bytes.len() > 1024 * 1024 {
                return Err(CliError::Usage("blueprint manifest exceeds 1 MiB".into()));
            }
            let document =
                std::str::from_utf8(&bytes).map_err(|e| CliError::Usage(e.to_string()))?;
            let definition: Value = if super::blueprint_commands::is_yaml(&file) {
                let blueprint =
                    orbit_blueprint::Blueprint::from_yaml(document, &Default::default())
                        .map_err(|e| CliError::Usage(e.to_string()))?;
                serde_json::to_value(blueprint)
                    .map_err(|e| CliError::Serialization(e.to_string()))?
            } else {
                serde_json::from_slice(&bytes).map_err(|e| CliError::Usage(e.to_string()))?
            };
            ("blueprints", Some(json!({"definition": definition})))
        }
        Bind {
            project,
            id,
            version,
        } => (
            "blueprints/bind",
            Some(json!({"project_id": project, "id": id, "version": version})),
        ),
        Apply { .. } | Unbind { .. } => {
            return Err(CliError::Usage(
                "changing or unpinning a project workflow is local-authoritative in this build; use the local workspace"
                    .into(),
            ));
        }
        UpgradePreview { .. } | Upgrade { .. } => {
            // A version upgrade is fenced by an accepted-state preview and a
            // preview fingerprint. The connected transport cannot read that
            // state, so it refuses instead of queueing an unverifiable change.
            return Err(CliError::Usage(
                "workflow version upgrades are local-authoritative in this build; use the local workspace"
                    .into(),
            ));
        }
        Binding { project } => {
            query.push(("project_id".into(), project));
            ("blueprints/binding", None)
        }
        CreateItem {
            project,
            id,
            kind,
            properties,
        } => (
            "blueprints/items",
            Some(
                json!({"project_id": project, "id": id, "kind": kind, "properties": read_properties(properties.as_deref())?}),
            ),
        ),
        Item { id } => {
            query.push(("id".into(), id));
            ("blueprints/items", None)
        }
        Transition {
            id,
            state,
            revision,
        } => (
            "blueprints/transition",
            Some(json!({"id": id, "to": state, "expected_revision": revision})),
        ),
        SetProperties { id, revision, file } => (
            "blueprints/properties",
            Some(
                json!({"id": id, "expected_revision": revision, "changes": read_properties(Some(file.as_path()))?}),
            ),
        ),
        ApplyTemplate {
            project,
            template,
            namespace,
        } => (
            "blueprints/template",
            Some(json!({"project_id": project, "template_id": template, "namespace": namespace})),
        ),
        Capability { .. } | Action { .. } => {
            return Err(CliError::Usage(
                super::blueprint_commands::CONNECTED_CAPABILITY_REFUSAL.into(),
            ));
        }
    };
    Ok((path, query, body, file))
}

pub(super) fn run(
    server: &str,
    db: &Path,
    command: BlueprintSubcommand,
    offline: bool,
    require_online: bool,
) -> Result<Value, CliError> {
    if let BlueprintSubcommand::Validate { file } = &command {
        let bytes = fs::read(file).map_err(|e| CliError::Io(e.to_string()))?;
        if bytes.len() > 1024 * 1024 {
            return Err(CliError::Usage("blueprint manifest exceeds 1 MiB".into()));
        }
        let document = std::str::from_utf8(&bytes).map_err(|e| CliError::Usage(e.to_string()))?;
        let blueprint = if super::blueprint_commands::is_yaml(file) {
            orbit_blueprint::Blueprint::from_yaml(document, &Default::default())
        } else {
            orbit_blueprint::Blueprint::from_json(document, &Default::default())
        }
        .map_err(|e| CliError::Usage(e.to_string()))?;
        return Ok(
            json!({"valid":true,"id":blueprint.id,"version":blueprint.version,
            "work_item_types":blueprint.work_items.len()}),
        );
    }
    if matches!(
        &command,
        BlueprintSubcommand::Pending | BlueprintSubcommand::Flush
    ) {
        return run_outbox(server, db, command);
    }
    // Capability execution is local-authority only in this iteration. Refuse
    // explicitly instead of queueing a command that can never execute.
    if matches!(
        &command,
        BlueprintSubcommand::Capability { .. } | BlueprintSubcommand::Action { .. }
    ) {
        return Err(CliError::Usage(
            super::blueprint_commands::CONNECTED_CAPABILITY_REFUSAL.into(),
        ));
    }
    // Default to a durable intention, then attempt canonical global outbox replay.
    // This freezes file input and command identity before contacting the server.
    // Read-only commands remain direct reads and never queue stale results.
    if is_mutation(&command) && !require_online {
        let (route, _, body, _) = endpoint(command)?;
        let payload = body.ok_or_else(|| CliError::Usage("expected blueprint mutation".into()))?;
        return submit_durable(server, db, route, payload, offline);
    }
    if offline {
        return Err(CliError::Usage(
            "--offline is only valid for blueprint mutations".into(),
        ));
    }
    let base = server_url(server)?;
    let token = std::env::var("ORBIT_SYNC_HTTP_TOKEN")
        .map_err(|_| CliError::Usage("ORBIT_SYNC_HTTP_TOKEN required".into()))?;
    let device = std::env::var("ORBIT_AUTHORITY_DEVICE")
        .map_err(|_| CliError::Usage("ORBIT_AUTHORITY_DEVICE required".into()))?;
    if token.is_empty() || device.trim().is_empty() || device.chars().any(char::is_control) {
        return Err(CliError::Usage(
            "invalid authority device credentials".into(),
        ));
    }
    let client = Client::builder()
        .timeout(Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| CliError::Usage(e.to_string()))?;
    let (path, query, body, export_file) = endpoint(command)?;
    let uri = format!("{base}/v1/authoritative/{path}");
    let req: RequestBuilder = match body {
        Some(payload) => client.post(uri).json(&payload),
        None => client.get(uri).query(&query),
    };
    let response = req
        .bearer_auth(token)
        .header("x-orbit-device-id", device)
        .send()
        .map_err(|e| CliError::Usage(e.to_string()))?;
    if !response.status().is_success() {
        return Err(CliError::Usage(format!(
            "blueprint authority rejected request (HTTP {})",
            response.status().as_u16()
        )));
    }
    let result: Value = response
        .json()
        .map_err(|e| CliError::Serialization(e.to_string()))?;
    if let Some(path) = export_file {
        let encoded = if super::blueprint_commands::is_yaml(&path) {
            let blueprint: orbit_blueprint::Blueprint = serde_json::from_value(result)
                .map_err(|e| CliError::Serialization(e.to_string()))?;
            blueprint
                .to_yaml()
                .map_err(|e| CliError::Usage(e.to_string()))?
        } else {
            serde_json::to_string_pretty(&result)
                .map_err(|e| CliError::Serialization(e.to_string()))?
        };
        fs::write(&path, encoded.as_bytes()).map_err(|e| CliError::Io(e.to_string()))?;
        return Ok(json!({"exported":path}));
    }
    Ok(result)
}

fn is_mutation(command: &BlueprintSubcommand) -> bool {
    use BlueprintSubcommand::*;
    matches!(
        command,
        Install { .. }
            | Import { .. }
            | Bind { .. }
            | CreateItem { .. }
            | Transition { .. }
            | SetProperties { .. }
            | ApplyTemplate { .. }
    )
}

fn submit_durable(
    server: &str,
    db: &Path,
    route: &str,
    payload: Value,
    offline: bool,
) -> Result<Value, CliError> {
    let base = server_url(server)?;
    let device = std::env::var("ORBIT_AUTHORITY_DEVICE")
        .map_err(|_| CliError::Usage("ORBIT_AUTHORITY_DEVICE required".into()))?;
    if device.trim().is_empty() || device.chars().any(char::is_control) {
        return Err(CliError::Usage("invalid authority device".into()));
    }
    let mut replica = AuthorityReplica::open(db).map_err(|e| CliError::Usage(format!("{e:?}")))?;
    replica
        .bind(&base)
        .map_err(|e| CliError::Usage(format!("{e:?}")))?;
    let command_id = generate_id("blueprint-intent");
    replica
        .queue_blueprint_transport(&command_id, route, &payload)
        .map_err(|e| CliError::Usage(format!("{e:?}")))?;
    if offline {
        return Ok(json!({"command_id":command_id,"status":"pending","accepted":false}));
    }
    let token = match std::env::var("ORBIT_SYNC_HTTP_TOKEN") {
        Ok(token) if !token.is_empty() => token,
        _ => {
            return Ok(
                json!({"command_id":command_id,"status":"pending","accepted":false,
            "reason":"authority credentials unavailable"}),
            );
        }
    };
    let http = match AuthorityHttp::new(&base, &token, &device) {
        Ok(http) => http,
        Err(e) => {
            return Ok(
                json!({"command_id":command_id,"status":"pending","accepted":false,
            "reason":format!("authority transport unavailable: {e:?}")}),
            );
        }
    };
    match replica.flush(&http) {
        Ok(FlushOutcome::Empty) | Ok(FlushOutcome::Accepted(_)) => {
            let pending = replica
                .blueprint_pending()
                .map_err(|e| CliError::Usage(format!("{e:?}")))?;
            let remains = pending["commands"]
                .as_array()
                .map(|items| items.iter().any(|item| item["id"] == command_id))
                .unwrap_or(true);
            Ok(
                json!({"command_id":command_id,"status":if remains { "pending" } else { "submitted" },
                "accepted":!remains}),
            )
        }
        Ok(FlushOutcome::Blocked {
            command_id: blocked,
            reason,
            ..
        }) => Ok(
            json!({"command_id":command_id,"status":"blocked","accepted":false,
                "blocking_command":blocked,"reason":reason}),
        ),
        Err(e) => Ok(
            json!({"command_id":command_id,"status":"pending","accepted":false,
            "reason":format!("authority unreachable or acceptance uncertain: {e:?}")}),
        ),
    }
}

fn run_outbox(server: &str, db: &Path, command: BlueprintSubcommand) -> Result<Value, CliError> {
    let base = server_url(server)?;
    let device = std::env::var("ORBIT_AUTHORITY_DEVICE")
        .map_err(|_| CliError::Usage("ORBIT_AUTHORITY_DEVICE required".into()))?;
    if device.trim().is_empty() || device.chars().any(char::is_control) {
        return Err(CliError::Usage("invalid authority device".into()));
    }
    let mut replica = AuthorityReplica::open(db).map_err(|e| CliError::Usage(format!("{e:?}")))?;
    replica
        .bind(&base)
        .map_err(|e| CliError::Usage(format!("{e:?}")))?;
    match command {
        BlueprintSubcommand::Pending => replica
            .blueprint_pending()
            .map_err(|e| CliError::Usage(format!("{e:?}"))),
        BlueprintSubcommand::Flush => {
            let token = std::env::var("ORBIT_SYNC_HTTP_TOKEN")
                .map_err(|_| CliError::Usage("ORBIT_SYNC_HTTP_TOKEN required to flush".into()))?;
            let http = AuthorityHttp::new(&base, &token, &device)
                .map_err(|e| CliError::Usage(format!("{e:?}")))?;
            let outcome = replica
                .flush(&http)
                .map_err(|e| CliError::Usage(format!("{e:?}")))?;
            Ok(match outcome {
                FlushOutcome::Empty => json!({"accepted_count":0,"status":"empty"}),
                FlushOutcome::Accepted(n) => json!({"accepted_count":n,"status":"flushed"}),
                FlushOutcome::Blocked {
                    accepted,
                    command_id,
                    reason,
                } => {
                    json!({"accepted_count":accepted,"blocked_command":command_id,"reason":reason,"status":"blocked"})
                }
            })
        }
        _ => Err(CliError::Usage("expected pending or flush".into())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_insecure_server() {
        assert!(server_url("http://example.com").is_err());
        assert!(server_url("https://user:pw@example.com").is_err());
        assert!(server_url("http://localhost:5000").is_ok());
    }
    #[test]
    fn maps_transition_to_authority_schema() {
        let (path, _, body, _) = endpoint(BlueprintSubcommand::Transition {
            id: "item".into(),
            state: "review".into(),
            revision: 7,
        })
        .unwrap();
        assert_eq!(path, "blueprints/transition");
        assert_eq!(body.unwrap()["expected_revision"], 7);
    }
}
