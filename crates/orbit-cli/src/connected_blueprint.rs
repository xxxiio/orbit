//! Connected blueprint CLI transport. All authoritative mutations are performed
//! by the server's AuthoritativeHost, never against a local client SQLite file.
use crate::{
    CliError,
    blueprint_commands::{BlueprintSubcommand, CapabilityActionSubcommand, CapabilitySubcommand},
    generate_id,
};
use orbit_client::authority::{AuthorityHttp, AuthorityReplica, FlushOutcome};
use reqwest::blocking::{Client, RequestBuilder};
use serde_json::{Value, json};
use std::{fs, path::Path, time::Duration};

pub(super) fn server_url(server: &str) -> Result<String, CliError> {
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

/// Bearer credential resolution for every connected blueprint/capability call,
/// including `orbit capability serve`. One place, so a read, a mutation and the
/// executor can never disagree about which credential is in use. The token is
/// never printed, logged or echoed into a response.
pub(super) fn bearer_token() -> Result<String, CliError> {
    let token = std::env::var("ORBIT_SYNC_HTTP_TOKEN")
        .map_err(|_| CliError::Usage("ORBIT_SYNC_HTTP_TOKEN required".into()))?;
    if token.is_empty() {
        return Err(CliError::Usage("ORBIT_SYNC_HTTP_TOKEN required".into()));
    }
    Ok(token)
}

/// The enrolled device this process acts as (bearer + `x-orbit-device-id`).
pub(super) fn device_identity() -> Result<String, CliError> {
    let device = std::env::var("ORBIT_AUTHORITY_DEVICE")
        .map_err(|_| CliError::Usage("ORBIT_AUTHORITY_DEVICE required".into()))?;
    if device.trim().is_empty() || device.chars().any(char::is_control) {
        return Err(CliError::Usage(
            "invalid authority device credentials".into(),
        ));
    }
    Ok(device)
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
            // Handled by the capability transport below; this is a programming
            // guard, not a user-facing refusal.
            return Err(CliError::Usage(
                "capability commands use the capability transport, not a blueprint route".into(),
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
    // Capability reads, requests and reconciliations exist on the connected
    // authority. Execution does not: provider code runs in the enrolled executor.
    if let BlueprintSubcommand::Capability { command } = command {
        if offline {
            return Err(CliError::Usage(
                "--offline is only valid for blueprint mutations".into(),
            ));
        }
        return run_capability(server, command);
    }
    if let BlueprintSubcommand::Action { command } = command {
        if offline {
            return Err(CliError::Usage(
                "--offline is only valid for blueprint mutations".into(),
            ));
        }
        return run_capability_action(server, command);
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
    let token = bearer_token()?;
    let device = device_identity()?;
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

/// Quote the connected authority's own fixed refusal text. The text is fixed by
/// design and never carries SQL, a path or a debug dump, so repeating it
/// verbatim is both safe and the only way a script can branch on the code.
fn capability_refusal(status: u16, body: &str) -> CliError {
    let message = serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|value| {
            value
                .get("error")
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
        .unwrap_or_else(|| {
            body.chars()
                .map(|character| {
                    if character.is_control() {
                        ' '
                    } else {
                        character
                    }
                })
                .take(300)
                .collect::<String>()
                .trim()
                .to_owned()
        });
    CliError::Usage(format!(
        "capability authority rejected request (HTTP {status}): {message}"
    ))
}

/// One connected capability call. Reads and writes share the credential and URL
/// resolution of the rest of the connected transport.
fn capability_call(
    base: &str,
    token: &str,
    device: &str,
    method: &str,
    path: &str,
    query: &[(&str, String)],
    body: Option<&Value>,
) -> Result<Value, CliError> {
    let client = Client::builder()
        .timeout(Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| CliError::Usage(e.to_string()))?;
    let uri = format!("{base}/v1/authoritative/{path}");
    let request = if method == "POST" {
        client.post(uri).json(body.unwrap_or(&Value::Null))
    } else {
        client.get(uri).query(query)
    };
    let response = request
        .bearer_auth(token)
        .header("x-orbit-device-id", device)
        .send()
        .map_err(|e| CliError::Usage(e.to_string()))?;
    let status = response.status();
    let text = response
        .text()
        .map_err(|e| CliError::Serialization(e.to_string()))?;
    if !status.is_success() {
        return Err(capability_refusal(status.as_u16(), &text));
    }
    serde_json::from_str(&text).map_err(|e| CliError::Serialization(e.to_string()))
}

fn capability_get(
    base: &str,
    token: &str,
    device: &str,
    path: &str,
    query: &[(&str, String)],
) -> Result<Value, CliError> {
    capability_call(base, token, device, "GET", path, query, None)
}

fn capability_post(
    base: &str,
    token: &str,
    device: &str,
    path: &str,
    body: &Value,
) -> Result<Value, CliError> {
    capability_call(base, token, device, "POST", path, &[], Some(body))
}

/// Credentials are resolved once per command so a failure is a usage error
/// before any partial request is sent.
fn capability_context(server: &str) -> Result<(String, String, String), CliError> {
    Ok((server_url(server)?, bearer_token()?, device_identity()?))
}

/// `orbit blueprint capability …` in connected mode: status is a read; install,
/// authorization and revocation are operator writes this transport does not drive
/// yet, so they keep refusing with their own message instead of a blanket one.
fn run_capability(server: &str, command: CapabilitySubcommand) -> Result<Value, CliError> {
    let (base, token, device) = capability_context(server)?;
    run_capability_with(&base, &token, &device, command)
}

fn run_capability_with(
    base: &str,
    token: &str,
    device: &str,
    command: CapabilitySubcommand,
) -> Result<Value, CliError> {
    match command {
        CapabilitySubcommand::List { project } => {
            let mut query = Vec::new();
            if let Some(project) = project {
                query.push(("project_id", project));
            }
            capability_get(base, token, device, "capabilities", &query)
        }
        CapabilitySubcommand::Install { .. }
        | CapabilitySubcommand::Uninstall { .. }
        | CapabilitySubcommand::Authorize { .. }
        | CapabilitySubcommand::Revoke { .. } => Err(CliError::Usage(
            super::blueprint_commands::CONNECTED_CAPABILITY_WRITE_REFUSAL.into(),
        )),
    }
}

/// `orbit blueprint action …` in connected mode. Offers, request, show, list and
/// reconcile are the operations the authority now owns; execution is refused
/// because the authority never dispatches provider code.
fn run_capability_action(
    server: &str,
    command: CapabilityActionSubcommand,
) -> Result<Value, CliError> {
    let (base, token, device) = capability_context(server)?;
    run_capability_action_with(&base, &token, &device, command)
}

fn run_capability_action_with(
    base: &str,
    token: &str,
    device: &str,
    command: CapabilityActionSubcommand,
) -> Result<Value, CliError> {
    match command {
        CapabilityActionSubcommand::Offers { item_id } => capability_get(
            base,
            token,
            device,
            "capability-actions",
            &[("item_id", item_id)],
        ),
        CapabilityActionSubcommand::Request {
            item_id,
            action,
            revision,
            command,
            input,
            confirm,
        } => {
            let inputs = super::blueprint_commands::parse_action_inputs(input.as_deref())?;
            let command_id = command.unwrap_or_else(|| generate_id("action"));
            // Offers are advisory on both sides; the authority re-checks every
            // fence. Reading them first keeps the confirmation discipline
            // identical to the local command: same copies, same refusals.
            let offers = capability_get(
                base,
                token,
                device,
                "capability-actions",
                &[("item_id", item_id.clone())],
            )?;
            let offer = offers
                .get("offers")
                .and_then(Value::as_array)
                .and_then(|offers| {
                    offers.iter().find(|offer| {
                        offer.get("action_id").and_then(Value::as_str) == Some(&action)
                    })
                })
                .ok_or_else(|| {
                    CliError::Usage(format!(
                        "action {action} is not offered for work item {item_id}"
                    ))
                })?;
            if offer.get("available").and_then(Value::as_bool) != Some(true) {
                let code = offer
                    .get("code")
                    .and_then(Value::as_str)
                    .unwrap_or("unavailable");
                let reason = offer
                    .get("reason")
                    .and_then(Value::as_str)
                    .unwrap_or("the authority did not offer this action");
                return Err(CliError::Usage(format!(
                    "capability action unavailable: {code}: {reason}"
                )));
            }
            // Same rule as the local command: an effectful or explicitly
            // destructive action needs --confirm.
            let destructive = offer.get("external_effect").and_then(Value::as_bool) == Some(true)
                || offer
                    .get("input_schema")
                    .and_then(|schema| schema.get("destructive"))
                    .and_then(Value::as_bool)
                    == Some(true);
            if destructive && !confirm {
                return Err(CliError::Usage(format!(
                    "capability action {action} has external effects; re-run with --confirm"
                )));
            }
            capability_post(
                base,
                token,
                device,
                "capability-actions/request",
                &json!({
                    "command_id": command_id,
                    "item_id": item_id,
                    "expected_revision": revision,
                    "action_id": action,
                    "inputs": inputs,
                    "confirm": confirm,
                }),
            )
        }
        CapabilityActionSubcommand::Execute { .. } => Err(CliError::Usage(
            super::blueprint_commands::CONNECTED_EXECUTE_REFUSAL.into(),
        )),
        CapabilityActionSubcommand::Show { command_id } => capability_get(
            base,
            token,
            device,
            "capability-actions/show",
            &[("command_id", command_id)],
        ),
        CapabilityActionSubcommand::List { item, state, limit } => {
            let mut query = Vec::new();
            if let Some(item) = item {
                query.push(("item_id", item));
            }
            if let Some(state) = state {
                query.push(("state", state));
            }
            let mut document =
                capability_get(base, token, device, "capability-actions/list", &query)?;
            // The authority returns at most 200. `--limit` keeps its local
            // meaning (default 50) by truncating the returned page.
            let limit = limit.unwrap_or(50);
            if let Some(requests) = document.get_mut("requests").and_then(Value::as_array_mut) {
                requests.truncate(limit);
            }
            Ok(document)
        }
        CapabilityActionSubcommand::Reconcile {
            command_id,
            succeed,
            fail,
            confirm,
        } => {
            let decision = match (succeed, fail) {
                (true, false) => "confirm_succeeded",
                (false, true) => "confirm_failed",
                _ => {
                    return Err(CliError::Usage(
                        "choose exactly one of --succeed or --fail".into(),
                    ));
                }
            };
            if !confirm {
                return Err(CliError::Usage(
                    "reconciling an uncertain or lost-acknowledgement action requires --confirm"
                        .into(),
                ));
            }
            // The authority records the authenticated device as the deciding
            // actor; the CLI never claims the decision for itself.
            let document = capability_post(
                base,
                token,
                device,
                "capability-actions/reconcile",
                &json!({"command_id": command_id, "decision": decision, "detail": ""}),
            )?;
            let receipt = document.get("receipt").cloned().unwrap_or(Value::Null);
            let recovered_from = receipt
                .get("detail")
                .and_then(|detail| detail.get("recovered_from"))
                .and_then(Value::as_str)
                .unwrap_or("settled")
                .to_owned();
            Ok(json!({
                "recovered_from": recovered_from,
                "receipt": receipt,
            }))
        }
    }
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
    let device = device_identity()?;
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
    let device = device_identity()?;
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

/// Connected capability parity: reads, request and reconcile reach the authority.
/// Execution is refused with the enrolled executor named, because the authority
/// never dispatches provider code.
#[cfg(test)]
mod capability_transport_tests {
    use super::*;
    use crate::connected_test_authority::Stub;

    const CAPABILITY: &str = "orbit.reference.work-item-note";
    const DEVICE: &str = "operator-1";
    const BEARER: &str = "operator-secret-0123456789abcdefghij";

    fn offers(available: bool, external_effect: bool) -> Value {
        json!({
            "item_id": "item-1",
            "item_revision": 3,
            "offers": [{
                "action_id": "append_note",
                "capability_id": CAPABILITY,
                "from": "open",
                "to": "closed",
                "description": "Append a note",
                "input_schema": {"type": "object", "properties": {"note": {"type": "string"}}},
                "external_effect": external_effect,
                "available": available,
                "code": if available { Value::Null } else { json!("executor_missing") },
                "reason": if available {
                    Value::Null
                } else {
                    json!("no active enrolled executor announced orbit.reference.work-item-note")
                },
            }],
        })
    }

    fn list() -> Value {
        let request = |id: &str| {
            json!({
                "command_id": id,
                "item_id": "item-1",
                "item_revision": 3,
                "action_id": "append_note",
                "capability_id": CAPABILITY,
                "from": "open",
                "to": "closed",
                "state": "pending",
                "inputs": {"note": "hello"},
                "created_at_ms": 10,
                "executor_device_id": Value::Null,
                "claim_token": Value::Null,
            })
        };
        json!({"requests": [request("cap-1"), request("cap-2"), request("cap-3")]})
    }

    #[test]
    fn capability_status_is_a_connected_read() {
        let stub = Stub::start(|call| {
            if call.route() == "capabilities" {
                return (
                    200,
                    json!({"capabilities": [{"capability_id": CAPABILITY}]}),
                );
            }
            (404, json!({"error": "route absent"}))
        });
        let document = run_capability_with(
            &stub.base(),
            BEARER,
            DEVICE,
            CapabilitySubcommand::List {
                project: Some("project-1".into()),
            },
        )
        .expect("status read");
        assert_eq!(document["capabilities"][0]["capability_id"], CAPABILITY);
        let calls = stub.calls();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].method, "GET");
        assert_eq!(calls[0].query, "project_id=project-1");
    }

    #[test]
    fn offers_are_read_from_the_bound_workflow() {
        let stub = Stub::start(|call| {
            if call.route() == "capability-actions" {
                return (200, offers(true, false));
            }
            (404, json!({"error": "route absent"}))
        });
        let document = run_capability_action_with(
            &stub.base(),
            BEARER,
            DEVICE,
            CapabilityActionSubcommand::Offers {
                item_id: "item-1".into(),
            },
        )
        .expect("offers read");
        assert_eq!(document["offers"][0]["action_id"], "append_note");
        assert_eq!(stub.calls()[0].query, "item_id=item-1");
    }

    #[test]
    fn request_keeps_the_local_confirmation_discipline() {
        let stub = Stub::start(|call| {
            if call.route() == "capability-actions" {
                return (200, offers(true, true));
            }
            if call.route() == "request" {
                return (
                    200,
                    json!({"request": {"command_id": call.body["command_id"]}}),
                );
            }
            (404, json!({"error": "route absent"}))
        });
        // An effectful action without --confirm never reaches the authority.
        let error = run_capability_action_with(
            &stub.base(),
            BEARER,
            DEVICE,
            CapabilityActionSubcommand::Request {
                item_id: "item-1".into(),
                action: "append_note".into(),
                revision: 3,
                command: Some("cap-cmd-1".into()),
                input: Some(json!({"note": "hello"}).to_string()),
                confirm: false,
            },
        )
        .expect_err("confirmation required");
        assert_eq!(
            error.to_string(),
            "capability action append_note has external effects; re-run with --confirm"
        );
        assert!(stub.calls_to("/request").is_empty());

        let document = run_capability_action_with(
            &stub.base(),
            BEARER,
            DEVICE,
            CapabilityActionSubcommand::Request {
                item_id: "item-1".into(),
                action: "append_note".into(),
                revision: 3,
                command: Some("cap-cmd-1".into()),
                input: Some(json!({"note": "hello"}).to_string()),
                confirm: true,
            },
        )
        .expect("confirmed request");
        assert_eq!(document["request"]["command_id"], "cap-cmd-1");
        let posted = stub.calls_to("/request");
        assert_eq!(posted.len(), 1);
        assert_eq!(posted[0].body["command_id"], "cap-cmd-1");
        assert_eq!(posted[0].body["expected_revision"], 3);
        assert_eq!(posted[0].body["inputs"]["note"], "hello");
        assert_eq!(posted[0].body["confirm"], true);
    }

    #[test]
    fn request_refuses_an_unavailable_offer_with_its_code() {
        let stub = Stub::start(|call| {
            if call.route() == "capability-actions" {
                return (200, offers(false, false));
            }
            (404, json!({"error": "route absent"}))
        });
        let error = run_capability_action_with(
            &stub.base(),
            BEARER,
            DEVICE,
            CapabilityActionSubcommand::Request {
                item_id: "item-1".into(),
                action: "append_note".into(),
                revision: 3,
                command: None,
                input: None,
                confirm: false,
            },
        )
        .expect_err("unavailable offer");
        assert_eq!(
            error.to_string(),
            "capability action unavailable: executor_missing: no active enrolled executor announced orbit.reference.work-item-note"
        );
        assert!(stub.calls_to("/request").is_empty());
    }

    #[test]
    fn request_refuses_an_action_the_workflow_does_not_offer() {
        let stub = Stub::start(|call| {
            if call.route() == "capability-actions" {
                return (200, offers(true, false));
            }
            (404, json!({"error": "route absent"}))
        });
        let error = run_capability_action_with(
            &stub.base(),
            BEARER,
            DEVICE,
            CapabilityActionSubcommand::Request {
                item_id: "item-1".into(),
                action: "not_offered".into(),
                revision: 3,
                command: None,
                input: None,
                confirm: false,
            },
        )
        .expect_err("not offered");
        assert_eq!(
            error.to_string(),
            "action not_offered is not offered for work item item-1"
        );
    }

    #[test]
    fn the_authority_refusal_text_is_quoted_verbatim() {
        let stub = Stub::start(|call| {
            if call.route() == "capability-actions" {
                return (200, offers(true, false));
            }
            if call.route() == "request" {
                return (409, json!({"error": "capability request conflict"}));
            }
            (404, json!({"error": "route absent"}))
        });
        let error = run_capability_action_with(
            &stub.base(),
            BEARER,
            DEVICE,
            CapabilityActionSubcommand::Request {
                item_id: "item-1".into(),
                action: "append_note".into(),
                revision: 3,
                command: Some("cap-cmd-1".into()),
                input: None,
                confirm: false,
            },
        )
        .expect_err("stale revision");
        assert_eq!(
            error.to_string(),
            "capability authority rejected request (HTTP 409): capability request conflict"
        );
    }

    #[test]
    fn show_and_list_read_the_authority_documents() {
        let stub = Stub::start(|call| {
            if call.route() == "show" {
                return (
                    200,
                    json!({"request": {"command_id": "cap-1", "state": "succeeded"}, "receipt": {"outcome": "succeeded"}}),
                );
            }
            if call.route() == "list" {
                return (200, list());
            }
            (404, json!({"error": "route absent"}))
        });
        let shown = run_capability_action_with(
            &stub.base(),
            BEARER,
            DEVICE,
            CapabilityActionSubcommand::Show {
                command_id: "cap-1".into(),
            },
        )
        .expect("show");
        assert_eq!(shown["receipt"]["outcome"], "succeeded");
        assert_eq!(stub.calls()[0].query, "command_id=cap-1");

        // `--limit` keeps its local meaning by truncating the returned page.
        let listed = run_capability_action_with(
            &stub.base(),
            BEARER,
            DEVICE,
            CapabilityActionSubcommand::List {
                item: Some("item-1".into()),
                state: Some("pending".into()),
                limit: Some(2),
            },
        )
        .expect("list");
        assert_eq!(listed["requests"].as_array().map(Vec::len), Some(2));
        let call = stub.calls_to("/list");
        assert_eq!(call[0].query, "item_id=item-1&state=pending");
    }

    #[test]
    fn execution_refuses_and_names_the_enrolled_executor() {
        let stub = Stub::start(|_| (404, json!({"error": "route absent"})));
        let error = run_capability_action_with(
            &stub.base(),
            BEARER,
            DEVICE,
            CapabilityActionSubcommand::Execute {
                command_id: "cap-1".into(),
            },
        )
        .expect_err("connected execution refusal");
        assert_eq!(
            error.to_string(),
            crate::blueprint_commands::CONNECTED_EXECUTE_REFUSAL
        );
        assert!(error.to_string().contains("orbit capability serve"));
        assert!(error.to_string().contains("never dispatches provider code"));
        // Refused before any request, so nothing was queued or sent.
        assert!(stub.calls().is_empty());
    }

    #[test]
    fn reconcile_records_the_decision_and_requires_confirmation() {
        let stub = Stub::start(|call| {
            if call.route() == "reconcile" {
                return (
                    200,
                    json!({
                        "request": {"command_id": "cap-1", "state": "failed"},
                        "receipt": {
                            "outcome": "failed",
                            "detail": {
                                "reconciled": true,
                                "reconciled_by": DEVICE,
                                "recovered_from": "uncertain",
                            },
                            "reported_at_ms": 30,
                            "reported_by": DEVICE,
                        },
                    }),
                );
            }
            (404, json!({"error": "route absent"}))
        });
        let ambiguous = run_capability_action_with(
            &stub.base(),
            BEARER,
            DEVICE,
            CapabilityActionSubcommand::Reconcile {
                command_id: "cap-1".into(),
                succeed: true,
                fail: true,
                confirm: true,
            },
        )
        .expect_err("exactly one decision");
        assert_eq!(
            ambiguous.to_string(),
            "choose exactly one of --succeed or --fail"
        );
        let unconfirmed = run_capability_action_with(
            &stub.base(),
            BEARER,
            DEVICE,
            CapabilityActionSubcommand::Reconcile {
                command_id: "cap-1".into(),
                succeed: false,
                fail: true,
                confirm: false,
            },
        )
        .expect_err("confirmation required");
        assert_eq!(
            unconfirmed.to_string(),
            "reconciling an uncertain or lost-acknowledgement action requires --confirm"
        );
        assert!(stub.calls_to("/reconcile").is_empty());

        let document = run_capability_action_with(
            &stub.base(),
            BEARER,
            DEVICE,
            CapabilityActionSubcommand::Reconcile {
                command_id: "cap-1".into(),
                succeed: false,
                fail: true,
                confirm: true,
            },
        )
        .expect("reconcile");
        assert_eq!(document["recovered_from"], "uncertain");
        assert_eq!(document["receipt"]["outcome"], "failed");
        let posted = stub.calls_to("/reconcile");
        assert_eq!(posted.len(), 1);
        assert_eq!(posted[0].body["decision"], "confirm_failed");
        assert_eq!(posted[0].body["command_id"], "cap-1");
    }

    #[test]
    fn capability_writes_stay_operator_work() {
        let stub = Stub::start(|_| (404, json!({"error": "route absent"})));
        let error = run_capability_with(
            &stub.base(),
            BEARER,
            DEVICE,
            CapabilitySubcommand::Authorize {
                capability_id: CAPABILITY.into(),
                project: "project-1".into(),
                confirm: true,
            },
        )
        .expect_err("operator write");
        assert_eq!(
            error.to_string(),
            crate::blueprint_commands::CONNECTED_CAPABILITY_WRITE_REFUSAL
        );
        assert!(stub.calls().is_empty());
    }
}
