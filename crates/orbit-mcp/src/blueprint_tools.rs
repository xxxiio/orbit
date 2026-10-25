//! Blueprint MCP tools use the same host services and HTTP API as the orbit CLI.
//! No second persistence authority and no implicit external capability grants.
//!
//! Connected blueprint writes have exactly one path: `blueprint_write`, which
//! queues the frozen route and payload in the canonical durable replica outbox
//! and only then attempts the canonical flush. There is deliberately no direct
//! `http.post` for a blueprint mutation in this module (M25 EC-004).
use orbit_client::authority::AuthorityError;
use orbit_client::unified::ClientConnectionError;

use super::*;

fn required_version(args: &Value, key: &str) -> Result<u32, String> {
    let value = args
        .get(key)
        .and_then(Value::as_u64)
        .ok_or_else(|| format!("missing or invalid {key}"))?;
    u32::try_from(value).map_err(|_| format!("{key} exceeds u32"))
}
fn revision(args: &Value) -> Result<u64, String> {
    args.get("expected_revision")
        .and_then(Value::as_u64)
        .ok_or("missing or invalid expected_revision".into())
}
fn map_properties(
    args: &Value,
    key: &str,
) -> Result<std::collections::BTreeMap<String, Value>, String> {
    let v = args.get(key).cloned().unwrap_or_else(|| json!({}));
    serde_json::from_value(v).map_err(debug_err)
}
fn query(value: &str) -> String {
    value
        .bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
                char::from(b).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}
fn workspace(policy: &AgentPolicy) -> Result<(), String> {
    if !policy.projects.is_empty() || !policy.allow_unscoped_execution {
        return Err("blueprint registry requires unrestricted workspace scope".into());
    }
    Ok(())
}
fn project(policy: &AgentPolicy, project_id: &str) -> Result<(), String> {
    if project_id.is_empty()
        || (!policy.projects.is_empty() && !policy.projects.contains(project_id))
    {
        return Err("project outside agent scope".into());
    }
    Ok(())
}
impl Server {
    pub(super) fn blueprint_dispatch(&mut self, name: &str, args: &Value) -> Result<Value, String> {
        match name {
            "blueprint.list" => {
                workspace(&self.policy)?;
                serde_json::to_value(
                    self.authority
                        .available_blueprint_versions(args.get("id").and_then(Value::as_str))
                        .map_err(debug_err)?,
                )
                .map_err(debug_err)
            }
            "blueprint.show" => {
                workspace(&self.policy)?;
                serde_json::to_value(
                    self.authority
                        .project_blueprint(
                            &required_str(args, "id")?,
                            required_version(args, "version")?,
                        )
                        .map_err(debug_err)?,
                )
                .map_err(debug_err)
            }
            "blueprint.binding" => {
                let id = required_str(args, "project_id")?;
                project(&self.policy, &id)?;
                serde_json::to_value(
                    self.authority
                        .active_project_blueprint(&id)
                        .map_err(debug_err)?,
                )
                .map_err(debug_err)
            }
            "blueprint.item" => {
                let id = required_str(args, "id")?;
                let item = self
                    .authority
                    .project_work_item(&id)
                    .map_err(debug_err)?
                    .ok_or("work item not found")?;
                project(&self.policy, &item.project_id)?;
                serde_json::to_value(item).map_err(debug_err)
            }
            "blueprint.publish" => {
                self.policy.write_allowed()?;
                workspace(&self.policy)?;
                let definition: orbit_blueprint::Blueprint = serde_json::from_value(
                    args.get("definition").ok_or("missing definition")?.clone(),
                )
                .map_err(debug_err)?;
                self.authority
                    .publish_project_blueprint(&definition, &BTreeSet::new())
                    .map_err(debug_err)?;
                serde_json::to_value(definition).map_err(debug_err)
            }
            "blueprint.install" => {
                self.policy.write_allowed()?;
                workspace(&self.policy)?;
                serde_json::to_value(
                    self.authority
                        .install_bundled_project_blueprint(&required_str(args, "id")?)
                        .map_err(debug_err)?,
                )
                .map_err(debug_err)
            }
            "blueprint.bind" => {
                self.policy.write_allowed()?;
                let project_id = required_str(args, "project_id")?;
                project(&self.policy, &project_id)?;
                serde_json::to_value(
                    self.authority
                        .activate_project_blueprint(
                            &project_id,
                            &required_str(args, "id")?,
                            required_version(args, "version")?,
                        )
                        .map_err(debug_err)?,
                )
                .map_err(debug_err)
            }
            "blueprint.create_item" => {
                self.policy.write_allowed()?;
                let project_id = required_str(args, "project_id")?;
                project(&self.policy, &project_id)?;
                serde_json::to_value(
                    self.authority
                        .create_project_work_item(
                            &required_str(args, "id")?,
                            &project_id,
                            &required_str(args, "kind")?,
                            map_properties(args, "properties")?,
                        )
                        .map_err(debug_err)?,
                )
                .map_err(debug_err)
            }
            "blueprint.transition" | "blueprint.properties" => {
                self.policy.write_allowed()?;
                let id = required_str(args, "id")?;
                let item = self
                    .authority
                    .project_work_item(&id)
                    .map_err(debug_err)?
                    .ok_or("work item not found")?;
                project(&self.policy, &item.project_id)?;
                if name == "blueprint.transition" {
                    serde_json::to_value(
                        self.authority
                            .transition_project_work_item_authorized(
                                &id,
                                &required_str(args, "to")?,
                                revision(args)?,
                            )
                            .map_err(debug_err)?,
                    )
                    .map_err(debug_err)
                } else {
                    serde_json::to_value(
                        self.authority
                            .update_project_work_item_properties(
                                &id,
                                revision(args)?,
                                map_properties(args, "changes")?,
                            )
                            .map_err(debug_err)?,
                    )
                    .map_err(debug_err)
                }
            }
            "blueprint.template" => {
                self.policy.write_allowed()?;
                let project_id = required_str(args, "project_id")?;
                project(&self.policy, &project_id)?;
                serde_json::to_value(
                    self.authority
                        .apply_project_blueprint_template(
                            &project_id,
                            &required_str(args, "template_id")?,
                            &required_str(args, "namespace")?,
                        )
                        .map_err(debug_err)?,
                )
                .map_err(debug_err)
            }
            _ => Err("unknown blueprint tool".into()),
        }
    }
}
/// The complete disposition vocabulary a connected blueprint write may report.
///
/// Only `Accepted` carries a payload from the server, and only because the
/// durable outbox holds an immutable acceptance receipt for that exact command
/// identity. `Pending` never carries server data: a queued intention does not
/// describe accepted state.
enum BlueprintDisposition {
    Accepted { receipt: Value, result: Value },
    Pending { state: String, reason: String },
    Conflict { state: String, reason: String },
}

impl BlueprintDisposition {
    fn body(self, command_id: &str) -> Value {
        match self {
            Self::Accepted { receipt, result } => json!({
                "command_id": command_id,
                "disposition": "accepted",
                "accepted": true,
                "pending": false,
                "receipt": receipt,
                "result": result,
            }),
            Self::Pending { state, reason } => json!({
                "command_id": command_id,
                "disposition": "pending",
                "accepted": false,
                "pending": true,
                "state": state,
                "reason": reason,
                "message": format!(
                    "queued as {command_id} ({state}) in the durable outbox; the blueprint change is not accepted until authority.flush is acknowledged"
                ),
            }),
            Self::Conflict { state, reason } => json!({
                "command_id": command_id,
                "disposition": "conflict",
                "accepted": false,
                "pending": false,
                "state": state,
                "reason": reason,
                "message": format!(
                    "queued change {command_id} rejected ({state}): {reason}; inspect authority.unresolved or authority.conflict_review before retrying"
                ),
            }),
        }
    }
}

impl ConnectedMcp {
    /// Project scope for an existing blueprint work item.
    ///
    /// A live authoritative read is required only when this agent's policy
    /// actually restricts projects. With an empty allowlist [`project`] accepts
    /// every non-empty project id, so the live read could never change the
    /// decision; skipping it is what keeps an authorized offline queue possible.
    fn check_blueprint_item_scope(&self, id: &str) -> Result<(), String> {
        if self.policy.projects.is_empty() {
            return Ok(());
        }
        let item = self.blueprint_http_get(&format!(
            "/v1/authoritative/blueprints/items?id={}",
            query(id)
        ))?;
        project(
            &self.policy,
            item.get("project_id")
                .and_then(Value::as_str)
                .ok_or("work item lacks project scope")?,
        )
    }

    /// Freeze the route and wire payload of one connected blueprint write.
    ///
    /// This is the ONLY description of a connected blueprint mutation in this
    /// adapter, and its result must be handed to [`Self::blueprint_write`].
    /// Payload construction, argument validation and scope/permission checks are
    /// exactly the ones the previous direct POST performed.
    fn blueprint_write_request(
        &self,
        name: &str,
        args: &Value,
    ) -> Result<(&'static str, Value), String> {
        match name {
            "blueprint.publish" => {
                self.policy.write_allowed()?;
                workspace(&self.policy)?;
                Ok((
                    "blueprints",
                    json!({"definition":args.get("definition").ok_or("missing definition")?}),
                ))
            }
            "blueprint.install" => {
                self.policy.write_allowed()?;
                workspace(&self.policy)?;
                Ok(("blueprints/install", json!({"id":required_str(args,"id")?})))
            }
            "blueprint.bind" => {
                self.policy.write_allowed()?;
                let project_id = required_str(args, "project_id")?;
                project(&self.policy, &project_id)?;
                Ok((
                    "blueprints/bind",
                    json!({"project_id":project_id,"id":required_str(args,"id")?,
                        "version":required_version(args,"version")?}),
                ))
            }
            "blueprint.create_item" => {
                self.policy.write_allowed()?;
                let project_id = required_str(args, "project_id")?;
                project(&self.policy, &project_id)?;
                Ok((
                    "blueprints/items",
                    json!({"project_id":project_id,"id":required_str(args,"id")?,
                        "kind":required_str(args,"kind")?,
                        "properties":map_properties(args,"properties")?}),
                ))
            }
            "blueprint.transition" | "blueprint.properties" => {
                self.policy.write_allowed()?;
                let id = required_str(args, "id")?;
                self.check_blueprint_item_scope(&id)?;
                if name == "blueprint.transition" {
                    Ok((
                        "blueprints/transition",
                        json!({"id":id,"to":required_str(args,"to")?,
                            "expected_revision":revision(args)?}),
                    ))
                } else {
                    Ok((
                        "blueprints/properties",
                        json!({"id":id,"changes":map_properties(args,"changes")?,
                            "expected_revision":revision(args)?}),
                    ))
                }
            }
            "blueprint.template" => {
                self.policy.write_allowed()?;
                let project_id = required_str(args, "project_id")?;
                project(&self.policy, &project_id)?;
                Ok((
                    "blueprints/template",
                    json!({"project_id":project_id,"template_id":required_str(args,"template_id")?,
                        "namespace":required_str(args,"namespace")?}),
                ))
            }
            _ => Err("unknown blueprint tool".into()),
        }
    }

    /// The single connected blueprint write path (M25 EC-004).
    ///
    /// The intention is queued FIRST in the canonical globally ordered replica
    /// outbox, so an unreachable server cannot lose it, then the canonical
    /// flush is attempted. The reported disposition is read back from that exact
    /// command identity:
    ///
    /// * `accepted` only when the outbox holds the server's immutable receipt;
    /// * `pending` when the row is still pending or its state is unobservable;
    /// * `conflict` when the outbox (or the server) permanently rejected it.
    ///
    /// A repeated identity with an identical frozen payload is a no-op here and
    /// on the server, so a retry can never duplicate the mutation.
    fn blueprint_write(
        &mut self,
        route: &str,
        body: Value,
        command_id: String,
    ) -> Result<Value, String> {
        if let Err(error) = self
            .client
            .queue_blueprint_transport(&command_id, route, &body)
        {
            return match error {
                ClientConnectionError::Remote(AuthorityError::Conflict(reason)) => {
                    Ok(BlueprintDisposition::Conflict {
                        state: "conflict".into(),
                        reason,
                    }
                    .body(&command_id))
                }
                other => Err(format!("blueprint write was not queued: {other:?}")),
            };
        }
        // The write is durable from this point on. A failed or offline flush can
        // only ever downgrade the report to pending; it cannot lose the intent.
        let flush_error = self
            .client
            .flush()
            .err()
            .map(|error| format!("authority unreachable or acceptance uncertain: {error:?}"));
        let disposition = self.blueprint_disposition(&command_id, flush_error);
        Ok(disposition.body(&command_id))
    }

    /// Read the disposition of one queued blueprint command back from its own
    /// outbox row. Aggregate queue depth is never used to attribute a state.
    fn blueprint_disposition(
        &self,
        command_id: &str,
        flush_error: Option<String>,
    ) -> BlueprintDisposition {
        let state = match self.client.command_state(command_id) {
            Ok(Some(state)) => state,
            Ok(None) | Err(_) => "unavailable".to_owned(),
        };
        match state.as_str() {
            "accepted" => match self.client.accepted_receipt(command_id) {
                Ok(Some(stored)) => {
                    // The outbox receipt is `{command_id, response}` and the
                    // server acceptance envelope is `{command_id, response}`,
                    // where the inner `response` is the applied payload.
                    let receipt = match stored.get("response") {
                        Some(response) => response.clone(),
                        None => stored.clone(),
                    };
                    let result = receipt.get("response").cloned().unwrap_or(Value::Null);
                    BlueprintDisposition::Accepted { receipt, result }
                }
                // Never report acceptance without the immutable receipt that
                // proves which payload the authority applied.
                _ => BlueprintDisposition::Pending {
                    state,
                    reason: "accepted outbox row has no readable receipt; inspect the replica"
                        .into(),
                },
            },
            "conflict" => {
                let reason = self
                    .client
                    .command_status(command_id)
                    .ok()
                    .flatten()
                    .and_then(|status| {
                        status
                            .pointer("/receipt/error")
                            .and_then(Value::as_str)
                            .map(str::to_owned)
                    })
                    .filter(|reason| !reason.trim().is_empty())
                    .unwrap_or_else(|| {
                        "command conflict requires explicit reconciliation; inspect the outbox"
                            .to_owned()
                    });
                BlueprintDisposition::Conflict { state, reason }
            }
            _ => BlueprintDisposition::Pending {
                state,
                reason: flush_error.unwrap_or_else(|| {
                    "queued intention is not yet accepted by the authority".to_owned()
                }),
            },
        }
    }
}

impl ConnectedMcp {
    pub(super) fn blueprint_dispatch(&mut self, name: &str, args: &Value) -> Result<Value, String> {
        let base = "/v1/authoritative/blueprints";
        // Reads never create state, so they stay direct and never queue.
        match name {
            "blueprint.list" => {
                workspace(&self.policy)?;
                let path = format!(
                    "{base}{}",
                    args.get("id")
                        .and_then(Value::as_str)
                        .map(|id| format!("?id={}", query(id)))
                        .unwrap_or_default()
                );
                return self.blueprint_http_get(&path);
            }
            "blueprint.show" => {
                workspace(&self.policy)?;
                let path = format!(
                    "{base}/show?id={}&version={}",
                    query(&required_str(args, "id")?),
                    required_version(args, "version")?
                );
                return self.blueprint_http_get(&path);
            }
            "blueprint.binding" => {
                let id = required_str(args, "project_id")?;
                project(&self.policy, &id)?;
                return self
                    .blueprint_http_get(&format!("{base}/binding?project_id={}", query(&id)));
            }
            "blueprint.item" => {
                let id = required_str(args, "id")?;
                let item = self.blueprint_http_get(&format!("{base}/items?id={}", query(&id)))?;
                project(
                    &self.policy,
                    item.get("project_id")
                        .and_then(Value::as_str)
                        .ok_or("work item lacks project scope")?,
                )?;
                return Ok(item);
            }
            _ => {}
        }
        let (route, body) = self.blueprint_write_request(name, args)?;
        let command_id = mcp_command_id(args);
        self.blueprint_write(route, body, command_id)
    }
    fn blueprint_http_get(&self, path: &str) -> Result<Value, String> {
        match &self.client {
            AuthorityConnection::Remote {
                http: Some(http), ..
            } => http.get(path).map_err(debug_err),
            _ => Err("blueprint management requires an online authenticated authority".into()),
        }
    }
}

#[cfg(test)]
mod blueprint_outbox_tests {
    use super::*;
    use std::{
        collections::HashMap,
        io::{Read, Write},
        net::{Shutdown, TcpListener},
        sync::{
            Arc, Mutex,
            atomic::{AtomicBool, Ordering},
        },
        thread,
        time::Duration,
    };

    const COMMAND_PATH: &str = "/v1/authoritative/blueprints/commands";

    fn developer(projects: &[&str], unscoped: bool) -> AgentPolicy {
        AgentPolicy {
            agent_id: "blueprint-agent".into(),
            session_id: "outbox-tests".into(),
            mode: Mode::Developer,
            projects: projects.iter().map(|id| (*id).to_owned()).collect(),
            allow_unscoped_execution: unscoped,
            allow_unscoped_knowledge: false,
        }
    }

    /// One recorded HTTP request: method, path and decoded JSON body.
    type Record = (String, String, Option<Value>);

    #[derive(Default)]
    struct Inner {
        requests: Vec<Record>,
        /// The fake authority's own idempotency table: one applied result per
        /// command identity, exactly like the server's immutable receipts.
        applied: HashMap<String, Value>,
    }

    /// Loopback blueprint authority that can be unreachable, can lose one
    /// response after applying a command, and records every request it served.
    struct FakeAuthority {
        base: String,
        online: Arc<AtomicBool>,
        lose_next_response: Arc<AtomicBool>,
        /// Command identities the authority permanently rejects with 409.
        conflicts: Arc<Mutex<std::collections::BTreeSet<String>>>,
        inner: Arc<Mutex<Inner>>,
        stop: Arc<AtomicBool>,
        handle: Option<thread::JoinHandle<()>>,
    }

    impl FakeAuthority {
        fn start() -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
            listener
                .set_nonblocking(true)
                .expect("nonblocking listener");
            let base = format!("http://{}", listener.local_addr().unwrap());
            let online = Arc::new(AtomicBool::new(false));
            let lose_next_response = Arc::new(AtomicBool::new(false));
            let conflicts = Arc::new(Mutex::new(std::collections::BTreeSet::new()));
            let inner = Arc::new(Mutex::new(Inner::default()));
            let stop = Arc::new(AtomicBool::new(false));
            let handle = {
                let (online, lose_next_response, conflicts, inner, stop) = (
                    Arc::clone(&online),
                    Arc::clone(&lose_next_response),
                    Arc::clone(&conflicts),
                    Arc::clone(&inner),
                    Arc::clone(&stop),
                );
                thread::spawn(move || {
                    while !stop.load(Ordering::SeqCst) {
                        let (mut socket, _) = match listener.accept() {
                            Ok(connection) => connection,
                            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                                thread::sleep(Duration::from_millis(2));
                                continue;
                            }
                            Err(_) => break,
                        };
                        if !online.load(Ordering::SeqCst) {
                            // Unreachable authority: abort instead of answering,
                            // so a flush fails with a transport error.
                            let _ = socket.shutdown(Shutdown::Both);
                            continue;
                        }
                        let response = serve(&mut socket, &inner, &lose_next_response, &conflicts);
                        if let Some((status, payload)) = response {
                            let body = payload.to_string();
                            let raw = format!(
                                "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                                body.len()
                            );
                            let _ = socket.write_all(raw.as_bytes());
                            let _ = socket.flush();
                        }
                    }
                })
            };
            Self {
                base,
                online,
                lose_next_response,
                conflicts,
                inner,
                stop,
                handle: Some(handle),
            }
        }

        fn go_online(&self) {
            self.online.store(true, Ordering::SeqCst);
        }

        fn reject_with_conflict(&self, command_id: &str) {
            self.conflicts
                .lock()
                .expect("fake authority state")
                .insert(command_id.to_owned());
        }

        fn requests(&self) -> Vec<Record> {
            self.inner
                .lock()
                .expect("fake authority state")
                .requests
                .clone()
        }

        fn posts(&self) -> Vec<Record> {
            self.requests()
                .into_iter()
                .filter(|(method, _, _)| method == "POST")
                .collect()
        }

        fn applied(&self) -> HashMap<String, Value> {
            self.inner
                .lock()
                .expect("fake authority state")
                .applied
                .clone()
        }
    }

    impl Drop for FakeAuthority {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::SeqCst);
            if let Some(handle) = self.handle.take() {
                let _ = handle.join();
            }
        }
    }

    /// Serve exactly one request. `None` means the response was deliberately
    /// lost after the command was applied.
    fn serve(
        socket: &mut std::net::TcpStream,
        inner: &Arc<Mutex<Inner>>,
        lose_next_response: &Arc<AtomicBool>,
        conflicts: &Arc<Mutex<std::collections::BTreeSet<String>>>,
    ) -> Option<(&'static str, Value)> {
        socket
            .set_read_timeout(Some(Duration::from_secs(5)))
            .expect("read timeout");
        let mut request = Vec::new();
        let mut buffer = [0_u8; 4096];
        let head_end = loop {
            if let Some(position) = request.windows(4).position(|part| part == b"\r\n\r\n") {
                break position;
            }
            let read = socket.read(&mut buffer).unwrap_or(0);
            if read == 0 || request.len() > 1 << 20 {
                return Some(("400 Bad Request", json!({"error":"incomplete request"})));
            }
            request.extend_from_slice(&buffer[..read]);
        };
        let head = String::from_utf8_lossy(&request[..head_end]).to_string();
        let mut line = head.lines().next().unwrap_or_default().split_whitespace();
        let method = line.next().unwrap_or_default().to_owned();
        let target = line.next().unwrap_or_default().to_owned();
        let path = target.split('?').next().unwrap_or_default().to_owned();
        let length = head
            .lines()
            .filter_map(|header| header.split_once(':'))
            .find(|(name, _)| name.eq_ignore_ascii_case("content-length"))
            .and_then(|(_, value)| value.trim().parse::<usize>().ok())
            .unwrap_or(0);
        let mut body_bytes = request[head_end + 4..].to_vec();
        while body_bytes.len() < length {
            let read = socket.read(&mut buffer).unwrap_or(0);
            if read == 0 {
                break;
            }
            body_bytes.extend_from_slice(&buffer[..read]);
        }
        let body: Option<Value> = if length == 0 {
            None
        } else {
            serde_json::from_slice(&body_bytes[..length.min(body_bytes.len())]).ok()
        };
        inner.lock().expect("fake authority state").requests.push((
            method.clone(),
            path.clone(),
            body.clone(),
        ));
        if method == "POST" && path == COMMAND_PATH {
            let body = body.unwrap_or(Value::Null);
            let command_id = body
                .get("command_id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned();
            let route = body
                .get("route")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned();
            let payload = body.get("body").cloned().unwrap_or(Value::Null);
            if conflicts
                .lock()
                .expect("fake authority state")
                .contains(&command_id)
            {
                // Mirrors the server's 409 revision/ownership fence.
                return Some(("409 Conflict", json!({"error":"blueprint conflict"})));
            }
            let mut state = inner.lock().expect("fake authority state");
            // Immutable per-command result: a replay returns this exact value.
            let sequence = state.applied.len() + 1;
            let result = state
                .applied
                .entry(command_id.clone())
                .or_insert_with(
                    || json!({"route":route,"echo":payload,"applied_sequence":sequence}),
                )
                .clone();
            drop(state);
            if lose_next_response.swap(false, Ordering::SeqCst) {
                let _ = socket.shutdown(Shutdown::Both);
                return None;
            }
            return Some(("200 OK", json!({"command_id":command_id,"response":result})));
        }
        match (method.as_str(), path.as_str()) {
            ("GET", "/v1/authoritative/blueprints") => Some(("200 OK", json!([]))),
            ("GET", "/v1/authoritative/blueprints/binding") => Some(("200 OK", Value::Null)),
            ("GET", "/v1/authoritative/blueprints/items") => Some((
                "200 OK",
                json!({"id":"item-1","project_id":"visible","state":"planned","revision":0}),
            )),
            _ => Some(("404 Not Found", json!({"error":"route absent"}))),
        }
    }

    fn connected(authority: &FakeAuthority, policy: AgentPolicy) -> ConnectedMcp {
        ConnectedMcp {
            client: AuthorityConnection::remote(
                ":memory:",
                &authority.base,
                Some(("device-1", "token")),
            )
            .expect("remote client"),
            policy,
        }
    }

    #[test]
    fn connected_blueprint_write_queues_first_and_reports_pending_until_accepted() {
        let authority = FakeAuthority::start();
        let mut server = connected(&authority, developer(&[], true));
        let args = json!({"id":"development","command_id":"bp-install-1"});

        // 1. The authority is unreachable. The write must still be durable.
        let pending = server.dispatch("blueprint.install", &args).unwrap();
        assert_eq!(pending["disposition"], "pending", "{pending}");
        assert_eq!(pending["accepted"], false, "{pending}");
        assert_eq!(pending["pending"], true, "{pending}");
        assert_eq!(pending["command_id"], "bp-install-1", "{pending}");
        assert!(pending.get("receipt").is_none(), "{pending}");
        assert!(pending.get("result").is_none(), "{pending}");
        assert!(
            authority.posts().is_empty(),
            "no write may reach the server"
        );

        let queued = server.dispatch("authority.unresolved", &json!({})).unwrap();
        assert_eq!(queued.as_array().unwrap().len(), 1, "{queued}");
        assert_eq!(queued[0]["kind"], "blueprint", "{queued}");
        assert_eq!(
            queued[0]["action"]["route"], "blueprints/install",
            "{queued}"
        );
        assert_eq!(
            queued[0]["action"]["body"],
            json!({"id":"development"}),
            "{queued}"
        );

        // 2. A retry of the same identity while still offline is a no-op.
        let retried = server.dispatch("blueprint.install", &args).unwrap();
        assert_eq!(retried["disposition"], "pending", "{retried}");
        assert_eq!(
            server
                .dispatch("authority.unresolved", &json!({}))
                .unwrap()
                .as_array()
                .unwrap()
                .len(),
            1,
            "the same command identity must not queue twice"
        );

        // 3. The server recovers: the retry queues idempotently and the
        //    canonical flush accepts it exactly once.
        authority.go_online();
        let accepted = server.dispatch("blueprint.install", &args).unwrap();
        assert_eq!(accepted["disposition"], "accepted", "{accepted}");
        assert_eq!(accepted["accepted"], true, "{accepted}");
        assert_eq!(accepted["pending"], false, "{accepted}");
        assert_eq!(accepted["command_id"], "bp-install-1", "{accepted}");
        assert_eq!(
            accepted["receipt"]["command_id"], "bp-install-1",
            "{accepted}"
        );
        assert_eq!(
            accepted["result"]["echo"],
            json!({"id":"development"}),
            "{accepted}"
        );
        assert_eq!(authority.posts().len(), 1, "the mutation is applied once");
        assert_eq!(authority.applied().len(), 1);

        // 4. Another flush has nothing left to send, and replaying the same
        //    command returns the identical immutable receipt.
        server.dispatch("authority.flush", &json!({})).unwrap();
        assert_eq!(authority.posts().len(), 1);
        let replayed = server.dispatch("blueprint.install", &args).unwrap();
        assert_eq!(replayed, accepted, "the accepted receipt is immutable");
        assert_eq!(authority.posts().len(), 1);
        assert_eq!(authority.applied().len(), 1);
        assert_eq!(
            server.dispatch("authority.unresolved", &json!({})).unwrap(),
            json!([])
        );
    }

    #[test]
    fn lost_acknowledgement_replay_returns_the_original_receipt_and_applies_once() {
        let authority = FakeAuthority::start();
        authority.go_online();
        let mut server = connected(&authority, developer(&[], true));
        let args = json!({"id":"learning","command_id":"bp-lost-ack"});

        // The server applies the command but the acknowledgement never arrives.
        authority.lose_next_response.store(true, Ordering::SeqCst);
        let lost = server.dispatch("blueprint.install", &args).unwrap();
        assert_eq!(lost["disposition"], "pending", "{lost}");
        assert_eq!(lost["accepted"], false, "{lost}");
        assert_eq!(authority.applied().len(), 1, "the server did apply it");

        // The client still holds the exact frozen intention.
        let queued = server.dispatch("authority.unresolved", &json!({})).unwrap();
        assert_eq!(queued[0]["command_id"], "bp-lost-ack", "{queued}");

        // The retry replays the same identity and receives the original receipt.
        let retried = server.dispatch("blueprint.install", &args).unwrap();
        assert_eq!(retried["disposition"], "accepted", "{retried}");
        assert_eq!(retried["receipt"]["command_id"], "bp-lost-ack", "{retried}");
        assert_eq!(
            retried["receipt"],
            json!({
                "command_id":"bp-lost-ack",
                "response":{"route":"blueprints/install","echo":{"id":"learning"},"applied_sequence":1},
            }),
            "{retried}"
        );
        assert_eq!(authority.applied().len(), 1, "applied once, never twice");
        assert_eq!(authority.posts().len(), 2, "one attempt plus one replay");
    }

    #[test]
    fn reusing_a_blueprint_command_id_for_another_body_conflicts_without_reapplying() {
        let authority = FakeAuthority::start();
        authority.go_online();
        let mut server = connected(&authority, developer(&[], true));
        let first = json!({"id":"development","command_id":"bp-reused"});
        assert_eq!(
            server.dispatch("blueprint.install", &first).unwrap()["disposition"],
            "accepted"
        );
        assert_eq!(authority.applied().len(), 1);

        let changed = json!({"id":"learning","command_id":"bp-reused"});
        let conflict = server.dispatch("blueprint.install", &changed).unwrap();
        assert_eq!(conflict["disposition"], "conflict", "{conflict}");
        assert_eq!(conflict["accepted"], false, "{conflict}");
        assert_eq!(conflict["command_id"], "bp-reused", "{conflict}");
        assert!(
            conflict["reason"]
                .as_str()
                .is_some_and(|reason| reason.contains("another intention")),
            "{conflict}"
        );
        assert!(conflict.get("receipt").is_none(), "{conflict}");
        assert_eq!(authority.applied().len(), 1, "no second mutation");
        assert_eq!(authority.posts().len(), 1, "nothing new was sent");

        // The accepted row still carries its original receipt.
        assert_eq!(
            server.dispatch("blueprint.install", &first).unwrap()["disposition"],
            "accepted"
        );
        assert_eq!(authority.posts().len(), 1);
    }

    #[test]
    fn server_side_conflict_is_reported_with_its_reviewable_reason() {
        let authority = FakeAuthority::start();
        authority.go_online();
        let mut server = connected(&authority, developer(&[], true));
        // The authority permanently rejects this identity (for example a stale
        // revision fence). The row is reviewable, never reported as accepted.
        authority.reject_with_conflict("bp-server-conflict");
        let outcome = server
            .dispatch(
                "blueprint.install",
                &json!({"id":"development","command_id":"bp-server-conflict"}),
            )
            .unwrap();
        assert_eq!(outcome["disposition"], "conflict", "{outcome}");
        assert_eq!(outcome["accepted"], false, "{outcome}");
        assert_eq!(outcome["command_id"], "bp-server-conflict", "{outcome}");
        assert!(
            outcome["reason"]
                .as_str()
                .is_some_and(|reason| reason.contains("blueprint conflict")),
            "{outcome}"
        );
        assert!(outcome.get("receipt").is_none(), "{outcome}");
        assert!(authority.applied().is_empty(), "nothing was applied");
        assert_eq!(authority.posts().len(), 1);

        // A pending row with a recorded reason maps to `pending` with that
        // reason: only observable state is ever reported.
        server
            .client
            .queue_blueprint_transport(
                "bp-pending-1",
                "blueprints/install",
                &json!({"id":"development"}),
            )
            .unwrap();
        let pending = server
            .blueprint_disposition("bp-pending-1", Some("authority unreachable".into()))
            .body("bp-pending-1");
        assert_eq!(pending["disposition"], "pending", "{pending}");
        assert_eq!(pending["reason"], "authority unreachable", "{pending}");
        assert_eq!(pending["state"], "pending", "{pending}");
    }

    #[test]
    fn connected_blueprint_reads_stay_direct_and_never_queue() {
        let authority = FakeAuthority::start();
        authority.go_online();
        let mut server = connected(&authority, developer(&[], true));
        assert_eq!(
            server.dispatch("blueprint.list", &json!({})).unwrap(),
            json!([])
        );
        assert_eq!(
            server
                .dispatch("blueprint.item", &json!({"id":"item-1"}))
                .unwrap()["project_id"],
            "visible"
        );
        assert_eq!(
            server.dispatch("authority.unresolved", &json!({})).unwrap(),
            json!([])
        );
        assert!(authority.posts().is_empty());
        assert_eq!(authority.requests().len(), 2);
    }

    #[test]
    fn connected_blueprint_writes_keep_policy_and_scope_checks_before_queueing() {
        let authority = FakeAuthority::start();
        authority.go_online();
        let mut read_only = connected(&authority, developer(&[], true));
        read_only.policy.mode = Mode::ReadOnly;
        for (name, args) in [
            ("blueprint.install", json!({"id":"development"})),
            (
                "blueprint.bind",
                json!({"project_id":"visible","id":"development","version":1}),
            ),
            (
                "blueprint.template",
                json!({"project_id":"visible","template_id":"starter","namespace":"demo"}),
            ),
        ] {
            assert!(
                read_only
                    .dispatch(name, &args)
                    .unwrap_err()
                    .contains("read-only"),
                "{name}"
            );
        }
        assert_eq!(
            read_only
                .client
                .blueprint_pending()
                .expect("pending blueprint rows")["queued"],
            0
        );

        // A scoped agent may not mutate an item in a project it cannot see.
        let mut scoped = connected(&authority, developer(&["other"], false));
        let denied = scoped.dispatch(
            "blueprint.transition",
            &json!({"id":"item-1","to":"review","expected_revision":0}),
        );
        assert!(
            denied.unwrap_err().contains("outside agent scope"),
            "scope denial"
        );
        assert_eq!(
            scoped.dispatch("authority.unresolved", &json!({})).unwrap(),
            json!([])
        );
        // Publishing needs unrestricted workspace scope.
        assert!(
            scoped
                .dispatch("blueprint.publish", &json!({"definition":{}}))
                .unwrap_err()
                .contains("workspace scope")
        );
    }

    #[test]
    fn connected_blueprint_write_schemas_advertise_the_optional_command_id() {
        let catalogue = connected_tool_list();
        let tools = catalogue.as_array().expect("connected tool catalogue");
        for name in [
            "blueprint.publish",
            "blueprint.install",
            "blueprint.bind",
            "blueprint.create_item",
            "blueprint.transition",
            "blueprint.properties",
            "blueprint.template",
        ] {
            let tool = tools
                .iter()
                .find(|tool| tool["name"] == name)
                .unwrap_or_else(|| panic!("missing connected blueprint capability {name}"));
            let schema = &tool["inputSchema"];
            assert_eq!(schema["type"], "object", "{name}");
            assert_eq!(
                schema["properties"]["command_id"]["type"], "string",
                "{name}"
            );
            assert!(
                !schema["required"]
                    .as_array()
                    .expect("required list")
                    .iter()
                    .any(|field| field == "command_id"),
                "{name}: command_id must stay optional"
            );
            assert!(
                tool["description"]
                    .as_str()
                    .is_some_and(|text| text.contains("disposition")),
                "{name}: the disposition contract must be documented"
            );
        }
        // Reads stay schemaless-optional and never carry a command identity.
        for name in [
            "blueprint.list",
            "blueprint.show",
            "blueprint.binding",
            "blueprint.item",
        ] {
            let tool = tools
                .iter()
                .find(|tool| tool["name"] == name)
                .unwrap_or_else(|| panic!("missing connected blueprint read {name}"));
            assert!(
                tool["inputSchema"]["properties"]
                    .get("command_id")
                    .is_none(),
                "{name}: a read must not advertise a command identity"
            );
        }
    }

    #[test]
    fn local_embedded_blueprint_writes_stay_synchronous_without_an_outbox() {
        let mut server = Server {
            authority: AuthoritativeHost::open(":memory:").unwrap(),
            policy: developer(&[], true),
        };
        let installed = server
            .dispatch("blueprint.install", &json!({"id":"development"}))
            .unwrap();
        // Local mode returns the applied blueprint itself, not a queue envelope.
        assert_eq!(installed["id"], "development", "{installed}");
        assert!(installed.get("disposition").is_none(), "{installed}");
        assert!(installed.get("command_id").is_none(), "{installed}");
        assert_eq!(
            server
                .dispatch("blueprint.list", &json!({}))
                .unwrap()
                .as_array()
                .unwrap()
                .len(),
            1
        );
    }
}
