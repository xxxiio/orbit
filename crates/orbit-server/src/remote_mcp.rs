//! Authenticated MCP adapter to canonical authoritative application operations.
//! Mutations call exactly the same AuthorityStore methods as REST commands.
use crate::authority_evidence::EvidenceCommand;
use crate::domain_commands::{ProjectCommand, TaskCommand};
use crate::{ApiError, AppState, admin, enrolled_device, error};
use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode},
};
use orbit_runtime::{
    authority_milestones::MilestoneCommand, authority_routines::RoutineCommand,
    authority_schedule::ScheduleCommand, authority_strategy::StrategyCommand,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Deserialize)]
pub struct RpcRequest {
    jsonrpc: String,
    id: Option<Value>,
    method: String,
    #[serde(default)]
    params: Value,
}
#[derive(Serialize)]
pub struct RpcResponse {
    jsonrpc: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<Value>,
}
fn result(id: Option<Value>, value: Value) -> RpcResponse {
    RpcResponse {
        jsonrpc: "2.0",
        id,
        result: Some(value),
        error: None,
    }
}
fn failure(id: Option<Value>, code: i64, message: &str) -> RpcResponse {
    RpcResponse {
        jsonrpc: "2.0",
        id,
        result: None,
        error: Some(json!({"code":code,"message":message})),
    }
}
fn tool(name: &str, description: &str, schema: Value) -> Value {
    json!({"name":name,"description":description,"inputSchema":schema})
}
fn empty_schema() -> Value {
    json!({"type":"object","properties":{},"additionalProperties":false})
}
fn command_schema() -> Value {
    json!({"type":"object","required":["command_id","expected_revision","action"],"properties":{"command_id":{"type":"string"},"expected_revision":{"type":"integer","minimum":0},"action":{"type":"object"}},"additionalProperties":false})
}
fn project_schema() -> Value {
    json!({"type":"object","required":["command_id","expected_revision","project_id","name"],"properties":{"command_id":{"type":"string"},"expected_revision":{"type":"integer","minimum":0},"project_id":{"type":"string"},"name":{"type":"string"}},"additionalProperties":false})
}
fn tools() -> Value {
    json!({"tools":[
     tool("orbit_get_workspace","Read a consistent canonical project/task/habit/metric snapshot",empty_schema()),
     tool("orbit_list_projects","Read canonical projects and revision",empty_schema()),
     tool("orbit_list_tasks","Read canonical tasks and revision",empty_schema()),
     tool("orbit_get_evidence","Read canonical habits and metric evidence",empty_schema()),
     tool("orbit_create_project","Submit a canonical project command with idempotency key and expected revision",project_schema()),
     tool("orbit_task_command","Submit create or status-change task action",command_schema()),
     tool("orbit_evidence_command","Submit create habit, record habit, metric target or metric observation action",command_schema()),
     tool("orbit_get_schedule","Read accepted schedule blocks and their authoritative revision",empty_schema()),
     tool("orbit_get_routines","Read accepted routines and their authoritative revision",empty_schema()),
     tool("orbit_get_milestones","Read accepted project milestones and their authoritative revision",empty_schema()),
     tool("orbit_get_strategies","Read accepted project strategies and their authoritative revision",empty_schema()),
     tool("orbit_schedule_command","Submit a version-checked authoritative schedule operation",command_schema()),
     tool("orbit_routine_command","Submit a version-checked authoritative routine operation",command_schema()),
     tool("orbit_milestone_command","Submit a version-checked authoritative milestone operation",command_schema()),
     tool("orbit_strategy_command","Submit a version-checked authoritative project strategy operation",command_schema()),
     tool("orbit_list_automations","Read server automation schedules",empty_schema()),
     tool("orbit_list_automation_runs","Read server automation execution history",empty_schema())
    ]})
}
fn tool_output(value: Value) -> Value {
    json!({"content":[{"type":"text","text":value.to_string()}],"structuredContent":value,"isError":false})
}
fn tool_failure(message: &str) -> Value {
    json!({"content":[{"type":"text","text":message}],"isError":true})
}
fn execute(
    state: &AppState,
    actor: Option<&str>,
    name: &str,
    arguments: Value,
) -> Result<Value, &'static str> {
    let empty = arguments.as_object().is_some_and(|o| o.is_empty());
    let is_write = matches!(
        name,
        "orbit_create_project"
            | "orbit_task_command"
            | "orbit_evidence_command"
            | "orbit_schedule_command"
            | "orbit_routine_command"
            | "orbit_milestone_command"
            | "orbit_strategy_command"
    );
    if is_write && actor.is_none() {
        return Err("enrolled device required for mutations");
    }
    if !is_write && !empty {
        return Err("tool requires empty arguments");
    }
    match name {
        "orbit_get_workspace" => {
            let store = state.store.lock().map_err(|_| "storage unavailable")?;
            let (revision, history_fingerprint, projects, tasks, evidence) = store
                .snapshot()
                .map_err(|_| "canonical snapshot unavailable")?;
            Ok(
                json!({"revision":revision,"history_fingerprint":history_fingerprint,"projects":projects,"tasks":tasks,"habits":evidence.habits,"habit_occurrences":evidence.habit_occurrences,"metrics":evidence.metrics,"metric_observations":evidence.metric_observations}),
            )
        }
        "orbit_list_projects" => {
            let store = state.store.lock().map_err(|_| "storage unavailable")?;
            let (revision, projects) = store
                .projects()
                .map_err(|_| "canonical projects unavailable")?;
            Ok(json!({"revision":revision,"projects":projects}))
        }
        "orbit_list_tasks" => {
            let store = state.store.lock().map_err(|_| "storage unavailable")?;
            let (revision, tasks) = store.tasks().map_err(|_| "canonical tasks unavailable")?;
            Ok(json!({"revision":revision,"tasks":tasks}))
        }
        "orbit_get_evidence" => {
            let store = state.store.lock().map_err(|_| "storage unavailable")?;
            serde_json::to_value(
                store
                    .evidence()
                    .map_err(|_| "canonical evidence unavailable")?,
            )
            .map_err(|_| "serialization unavailable")
        }
        "orbit_create_project" => {
            let command: ProjectCommand =
                serde_json::from_value(arguments).map_err(|_| "invalid project command")?;
            let mut store = state.store.lock().map_err(|_| "storage unavailable")?;
            let receipt = store
                .project_command(actor.unwrap_or_default(), command)
                .map_err(
                    |_| "project command rejected; refresh authoritative revision and review",
                )?;
            serde_json::to_value(receipt).map_err(|_| "serialization unavailable")
        }
        "orbit_task_command" => {
            let command: TaskCommand =
                serde_json::from_value(arguments).map_err(|_| "invalid task command")?;
            let mut store = state.store.lock().map_err(|_| "storage unavailable")?;
            let receipt = store
                .task_command(actor.unwrap_or_default(), command)
                .map_err(|_| "task command rejected; refresh authoritative revision and review")?;
            serde_json::to_value(receipt).map_err(|_| "serialization unavailable")
        }
        "orbit_evidence_command" => {
            let command: EvidenceCommand =
                serde_json::from_value(arguments).map_err(|_| "invalid evidence command")?;
            let mut store = state.store.lock().map_err(|_| "storage unavailable")?;
            let receipt = store
                .evidence_command(actor.unwrap_or_default(), command)
                .map_err(
                    |_| "evidence command rejected; refresh authoritative revision and review",
                )?;
            serde_json::to_value(receipt).map_err(|_| "serialization unavailable")
        }
        "orbit_get_schedule" => {
            let store = state.store.lock().map_err(|_| "storage unavailable")?;
            serde_json::to_value(
                store
                    .schedule()
                    .map_err(|_| "canonical schedule unavailable")?,
            )
            .map_err(|_| "serialization unavailable")
        }
        "orbit_get_routines" => {
            let store = state.store.lock().map_err(|_| "storage unavailable")?;
            serde_json::to_value(
                store
                    .routines()
                    .map_err(|_| "canonical routines unavailable")?,
            )
            .map_err(|_| "serialization unavailable")
        }
        "orbit_get_milestones" => {
            let store = state.store.lock().map_err(|_| "storage unavailable")?;
            serde_json::to_value(
                store
                    .milestones()
                    .map_err(|_| "canonical milestones unavailable")?,
            )
            .map_err(|_| "serialization unavailable")
        }
        "orbit_get_strategies" => {
            let store = state.store.lock().map_err(|_| "storage unavailable")?;
            serde_json::to_value(
                store
                    .strategies()
                    .map_err(|_| "canonical strategies unavailable")?,
            )
            .map_err(|_| "serialization unavailable")
        }
        "orbit_schedule_command" => {
            let command: ScheduleCommand =
                serde_json::from_value(arguments).map_err(|_| "invalid schedule command")?;
            let mut store = state.store.lock().map_err(|_| "storage unavailable")?;
            let receipt = store
                .schedule_command(actor.unwrap_or_default(), command)
                .map_err(|_| "schedule command rejected; refresh accepted revision and review")?;
            serde_json::to_value(receipt).map_err(|_| "serialization unavailable")
        }
        "orbit_routine_command" => {
            let command: RoutineCommand =
                serde_json::from_value(arguments).map_err(|_| "invalid routine command")?;
            let mut store = state.store.lock().map_err(|_| "storage unavailable")?;
            let receipt = store
                .routine_command(actor.unwrap_or_default(), command)
                .map_err(|_| "routine command rejected; refresh accepted revision and review")?;
            serde_json::to_value(receipt).map_err(|_| "serialization unavailable")
        }
        "orbit_milestone_command" => {
            let command: MilestoneCommand =
                serde_json::from_value(arguments).map_err(|_| "invalid milestone command")?;
            let mut store = state.store.lock().map_err(|_| "storage unavailable")?;
            let receipt = store
                .milestone_command(actor.unwrap_or_default(), command)
                .map_err(|_| "milestone command rejected; refresh accepted revision and review")?;
            serde_json::to_value(receipt).map_err(|_| "serialization unavailable")
        }
        "orbit_strategy_command" => {
            let command: StrategyCommand =
                serde_json::from_value(arguments).map_err(|_| "invalid strategy command")?;
            let mut store = state.store.lock().map_err(|_| "storage unavailable")?;
            let receipt = store
                .strategy_command(actor.unwrap_or_default(), command)
                .map_err(|_| "strategy command rejected; refresh accepted revision and review")?;
            serde_json::to_value(receipt).map_err(|_| "serialization unavailable")
        }
        "orbit_list_automations" => {
            let store = state.automation.lock().map_err(|_| "storage unavailable")?;
            serde_json::to_value(store.jobs().map_err(|_| "automation data unavailable")?)
                .map_err(|_| "serialization unavailable")
        }
        "orbit_list_automation_runs" => {
            let store = state.automation.lock().map_err(|_| "storage unavailable")?;
            serde_json::to_value(store.runs(100).map_err(|_| "automation data unavailable")?)
                .map_err(|_| "serialization unavailable")
        }
        _ => Err("unknown tool"),
    }
}
fn dispatch(state: &AppState, actor: Option<&str>, req: RpcRequest) -> RpcResponse {
    let id = req.id;
    if req.jsonrpc != "2.0" {
        return failure(id, -32600, "invalid jsonrpc version");
    }
    if id
        .as_ref()
        .is_some_and(|v| !v.is_string() && !v.is_number() && !v.is_null())
    {
        return failure(None, -32600, "invalid request id");
    }
    match req.method.as_str() {
        "initialize" => result(
            id,
            json!({"protocolVersion":"2025-03-26","capabilities":{"tools":{"listChanged":false}},"serverInfo":{"name":"orbit-server","version":env!("CARGO_PKG_VERSION")}}),
        ),
        "ping" => result(id, json!({})),
        "tools/list" => result(id, tools()),
        "tools/call" => {
            let Some(name) = req.params.get("name").and_then(Value::as_str) else {
                return failure(id, -32602, "tool name required");
            };
            let args = req
                .params
                .get("arguments")
                .cloned()
                .unwrap_or_else(|| json!({}));
            if !args.is_object() {
                return failure(id, -32602, "arguments must be an object");
            }
            if !tools()["tools"]
                .as_array()
                .is_some_and(|xs| xs.iter().any(|x| x["name"] == name))
            {
                return failure(id, -32602, "unknown tool");
            }
            match execute(state, actor, name, args) {
                Ok(value) => result(id, tool_output(value)),
                Err(message) => result(id, tool_failure(message)),
            }
        }
        _ => failure(id, -32601, "method not found"),
    }
}
pub async fn endpoint(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<RpcRequest>,
) -> Result<Json<RpcResponse>, ApiError> {
    if headers.contains_key("origin") {
        return Err(error(
            StatusCode::FORBIDDEN,
            "browser origins not supported",
        ));
    }
    // An administrator may inspect state, but only an enrolled device can mutate it.
    let actor = match enrolled_device(&state, &headers) {
        Ok(id) => Some(id),
        Err(_) => {
            admin(&state, &headers)?;
            None
        }
    };
    if req.id.is_none() {
        if req.method == "notifications/initialized" {
            return Ok(Json(RpcResponse {
                jsonrpc: "2.0",
                id: None,
                result: None,
                error: None,
            }));
        }
        return Err(error(StatusCode::BAD_REQUEST, "request id required"));
    }
    Ok(Json(dispatch(&state, actor.as_deref(), req)))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn canonical_tools_are_advertised() {
        let t = tools();
        let a = t["tools"].as_array().unwrap();
        for name in [
            "orbit_get_workspace",
            "orbit_create_project",
            "orbit_task_command",
            "orbit_evidence_command",
        ] {
            assert!(a.iter().any(|v| v["name"] == name));
        }
        assert!(!a.iter().any(|v| v["name"] == "orbit_relay_status"));
    }
    #[test]
    fn write_requires_device() {
        let path = std::env::temp_dir().join(format!(
            "orbit-mcp-{}-{}.sqlite",
            std::process::id(),
            "write"
        ));
        let state = AppState {
            store: std::sync::Arc::new(std::sync::Mutex::new(
                orbit_runtime::AuthoritativeHost::open(&path).unwrap(),
            )),
            automation: std::sync::Arc::new(std::sync::Mutex::new(
                crate::automation::AutomationStore::open(&path.with_extension("automation.sqlite"))
                    .unwrap(),
            )),
            proposals: std::sync::Arc::new(std::sync::Mutex::new(
                crate::proposals::ProposalStore::open(&path.with_extension("proposals.sqlite"))
                    .unwrap(),
            )),
            sessions: std::sync::Arc::new(std::sync::Mutex::new(
                crate::session::SessionStore::open(&path.with_extension("sessions.sqlite"))
                    .unwrap(),
            )),
            login_attempts: std::sync::Arc::new(std::sync::Mutex::new(
                crate::session::LoginThrottle::new(),
            )),
            web_dist: std::sync::Arc::new(crate::web::WebDist::new(
                std::env::temp_dir().join("orbit-server-absent-dist"),
            )),
            cookie_secure: false,
            admin_key_hash: vec![0; 32],
            attachment_body_limit: 64 * 1024 * 1024,
            auth_mode: crate::auth::AuthenticationMode::Authenticated,
        };
        let r = dispatch(
            &state,
            None,
            RpcRequest {
                jsonrpc: "2.0".into(),
                id: Some(json!(1)),
                method: "tools/call".into(),
                params: json!({"name":"orbit_create_project","arguments":{"command_id":"x","expected_revision":0,"project_id":"p","name":"project"}}),
            },
        );
        assert_eq!(r.result.unwrap()["isError"], true);
    }
}

#[cfg(test)]
mod rich_domain_mcp_contract {
    use super::*;

    fn state(label: &str) -> AppState {
        let path = std::env::temp_dir().join(format!(
            "orbit-rich-mcp-{}-{label}.sqlite",
            std::process::id()
        ));
        AppState {
            store: std::sync::Arc::new(std::sync::Mutex::new(
                orbit_runtime::AuthoritativeHost::open(&path).unwrap(),
            )),
            automation: std::sync::Arc::new(std::sync::Mutex::new(
                crate::automation::AutomationStore::open(&path.with_extension("automation.sqlite"))
                    .unwrap(),
            )),
            proposals: std::sync::Arc::new(std::sync::Mutex::new(
                crate::proposals::ProposalStore::open(&path.with_extension("proposals.sqlite"))
                    .unwrap(),
            )),
            sessions: std::sync::Arc::new(std::sync::Mutex::new(
                crate::session::SessionStore::open(&path.with_extension("sessions.sqlite"))
                    .unwrap(),
            )),
            login_attempts: std::sync::Arc::new(std::sync::Mutex::new(
                crate::session::LoginThrottle::new(),
            )),
            web_dist: std::sync::Arc::new(crate::web::WebDist::new(
                std::env::temp_dir().join("orbit-server-absent-dist"),
            )),
            cookie_secure: false,
            admin_key_hash: vec![0; 32],
            attachment_body_limit: 64 * 1024 * 1024,
            auth_mode: crate::auth::AuthenticationMode::Authenticated,
        }
    }

    fn call(state: &AppState, actor: Option<&str>, name: &str, arguments: Value) -> Value {
        dispatch(
            state,
            actor,
            RpcRequest {
                jsonrpc: "2.0".into(),
                id: Some(json!(1)),
                method: "tools/call".into(),
                params: json!({"name":name,"arguments":arguments}),
            },
        )
        .result
        .unwrap()
    }

    #[test]
    fn every_rich_domain_advertises_read_and_write_tools() {
        let tools = tools();
        let entries = tools["tools"].as_array().unwrap();
        for domain in ["schedule", "routines", "milestones", "strategies"] {
            let name = format!("orbit_get_{domain}");
            assert!(entries.iter().any(|x| x["name"] == name), "missing {name}");
        }
        for domain in ["schedule", "routine", "milestone", "strategy"] {
            let name = format!("orbit_{domain}_command");
            let entry = entries.iter().find(|x| x["name"] == name).unwrap();
            assert_eq!(
                entry["inputSchema"]["required"],
                json!(["command_id", "expected_revision", "action"])
            );
        }
    }

    #[test]
    fn admin_can_read_but_cannot_mutate_rich_domains() {
        let state = state("administrator");
        for name in [
            "orbit_get_schedule",
            "orbit_get_routines",
            "orbit_get_milestones",
            "orbit_get_strategies",
        ] {
            let response = call(&state, None, name, json!({}));
            assert_eq!(response["isError"], false, "{name}: {response}");
            assert!(response["structuredContent"].is_object());
        }
        for name in [
            "orbit_schedule_command",
            "orbit_routine_command",
            "orbit_milestone_command",
            "orbit_strategy_command",
        ] {
            let response = call(
                &state,
                None,
                name,
                json!({"command_id":"x","expected_revision":0,"action":{}}),
            );
            assert_eq!(response["isError"], true);
            assert!(
                response["content"][0]["text"]
                    .as_str()
                    .unwrap()
                    .contains("enrolled device")
            );
        }
    }

    #[test]
    fn rich_domain_reads_reject_nonempty_arguments() {
        let state = state("read-arguments");
        for name in [
            "orbit_get_schedule",
            "orbit_get_routines",
            "orbit_get_milestones",
            "orbit_get_strategies",
        ] {
            let result = call(&state, Some("device"), name, json!({"unknown":true}));
            assert_eq!(result["isError"], true);
        }
    }

    #[test]
    fn malformed_writes_do_not_mutate_accepted_state() {
        let state = state("malformed");
        for (write, read) in [
            ("orbit_schedule_command", "orbit_get_schedule"),
            ("orbit_routine_command", "orbit_get_routines"),
            ("orbit_milestone_command", "orbit_get_milestones"),
            ("orbit_strategy_command", "orbit_get_strategies"),
        ] {
            let before = call(&state, Some("device"), read, json!({}));
            let invalid = call(
                &state,
                Some("device"),
                write,
                json!({"command_id":3,"expected_revision":-1,"action":{}}),
            );
            assert_eq!(invalid["isError"], true);
            let after = call(&state, Some("device"), read, json!({}));
            assert_eq!(before["structuredContent"], after["structuredContent"]);
        }
    }

    #[test]
    fn routine_create_exposes_accepted_projection_and_stable_receipt() {
        let state = state("routine-roundtrip");
        let command = json!({
            "command_id":"mcp-routine-create-once",
            "expected_revision":0,
            "action":{"type":"create","routine_id":"mcp-morning","name":"Morning","habit_ids":[]}
        });
        let first = call(
            &state,
            Some("device-one"),
            "orbit_routine_command",
            command.clone(),
        );
        assert_eq!(first["isError"], false, "{first}");
        let view = call(&state, Some("device-one"), "orbit_get_routines", json!({}));
        assert_eq!(view["isError"], false, "{view}");
        assert!(
            view["structuredContent"]
                .to_string()
                .contains("mcp-morning")
        );
        let second = call(&state, Some("device-one"), "orbit_routine_command", command);
        assert_eq!(second["isError"], false, "{second}");
        let original = &first["structuredContent"];
        let replay = &second["structuredContent"];
        // The first acceptance is not a replay; the retry must report the
        // duplicate while returning the original stable revision and payload.
        assert_eq!(original["duplicate"], false, "{first}");
        assert_eq!(replay["duplicate"], true, "{second}");
        assert_eq!(replay["revision"], original["revision"], "{second}");
        assert_eq!(replay["routine"], original["routine"], "{second}");
    }
}
