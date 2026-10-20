//! Desktop/headless MCP adapter for Orbit.
//! Connected and embedded implementations live in separate modules,
//! sharing only the policy, protocol and domain contracts.
//!
//! Authorization is owned by Orbit server configuration, not by self-reported
//! MCP client identity. All mutations route through application services so
//! domain validation, activity audit, and sync-outbox semantics remain intact.

mod blueprint_tools;
mod connected_backend;
mod embedded_backend;

use std::{
    collections::BTreeSet,
    env,
    io::{self, BufRead, Write},
    process,
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use orbit_application::{ChangeProjectEvidenceCommand, ProjectEvidenceBinding};
use orbit_application::{
    ConfigureProjectStrategyCommand, ConfigureProjectStrategyWithActivityCommand,
    CreateTaskCommand, CreateTaskWithActivityCommand, SetHabitActiveWithActivityCommand,
    SetProjectCriterionStatusWithActivityCommand, TransitionMilestoneCommand,
    TransitionMilestoneWithActivityCommand, TransitionProjectCommand,
    TransitionProjectWithActivityCommand, TransitionTaskCommand, TransitionTaskWithActivityCommand,
};
use orbit_client::authority::CommandKind;
use orbit_client::unified::{AuthorityConnection, CommandDisposition, DomainWorkflowStep};
use orbit_domain::HabitPeriod;
use orbit_domain::{
    ActivityId, ActivityOrigin, HabitId, MilestoneId, MilestoneStatus, ProjectId, ProjectMode,
    ProjectProgressSignal, ProjectStatus, ProjectSuccessCriterion, SuccessCriterionStatus, TaskId,
    TaskScope, TaskStatus,
};
use orbit_progress::HealthPolicy;
use orbit_runtime::authority_evidence::{EvidenceAction, EvidenceCommand};
use orbit_runtime::authority_milestones::{MilestoneAction, MilestoneCommand};
use orbit_runtime::authority_routines::RoutineCommand;
use orbit_runtime::authority_schedule::ScheduleCommand;
use orbit_runtime::authority_strategy::StrategyCommand;
use orbit_runtime::domain_commands::{ProjectCommand, TaskAction, TaskCommand};
use orbit_runtime::{
    AuthoritativeHost,
    folder_commands::{FolderCreate, FolderMove},
    markdown_commands::MarkdownCommand,
};
use serde_json::{Value, json};

static IDS: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    ReadOnly,
    Developer,
}

#[derive(Debug, Clone)]
struct AgentPolicy {
    agent_id: String,
    session_id: String,
    mode: Mode,
    projects: BTreeSet<String>,
    allow_unscoped_execution: bool,
    allow_unscoped_knowledge: bool,
}

impl AgentPolicy {
    fn from_env() -> Result<Self, String> {
        let agent_id = env::var("ORBIT_MCP_AGENT_ID").unwrap_or_else(|_| "external-agent".into());
        let session_id = env::var("ORBIT_MCP_SESSION_ID").unwrap_or_else(|_| "default".into());
        if agent_id.trim().is_empty() || session_id.trim().is_empty() {
            return Err("agent/session id cannot be blank".into());
        }
        let mode = match env::var("ORBIT_MCP_MODE")
            .unwrap_or_else(|_| "read_only".into())
            .as_str()
        {
            "read_only" | "readonly" => Mode::ReadOnly,
            "developer" => Mode::Developer,
            other => return Err(format!("unsupported ORBIT_MCP_MODE: {other}")),
        };
        let projects = env::var("ORBIT_MCP_PROJECTS")
            .unwrap_or_default()
            .split(',')
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .map(str::to_owned)
            .collect();
        Ok(Self {
            agent_id,
            session_id,
            mode,
            projects,
            allow_unscoped_execution: env_bool("ORBIT_MCP_ALLOW_UNSCOPED_EXECUTION", false),
            allow_unscoped_knowledge: env_bool("ORBIT_MCP_ALLOW_UNSCOPED_KNOWLEDGE", false),
        })
    }
    fn origin(&self) -> ActivityOrigin {
        ActivityOrigin::Agent(format!("{}:{}", self.agent_id, self.session_id))
    }
    fn write_allowed(&self) -> Result<(), String> {
        if self.mode == Mode::Developer {
            Ok(())
        } else {
            Err("agent session is read-only".into())
        }
    }
    fn project_allowed(&self, project: &ProjectId) -> bool {
        self.projects.is_empty() || self.projects.contains(project.as_str())
    }
}

/// The desktop adapter has two mutually exclusive authority modes. Connected
/// mode NEVER opens the local authoritative store: ORBIT_DB is a disposable
/// accepted-state replica and durable outbox, not a second source of truth.
struct ConnectedMcp {
    client: AuthorityConnection,
    policy: AgentPolicy,
}

enum McpBackend {
    Local(Server),
    Connected(ConnectedMcp),
}
impl McpBackend {
    fn dispatch(&mut self, name: &str, args: &Value) -> Result<Value, String> {
        orbit_runtime::capabilities::validate_call(name, args).map_err(|violation| {
            serde_json::to_string(&violation).unwrap_or_else(|_| violation.to_string())
        })?;
        match self {
            Self::Local(server) => server.dispatch(name, args),
            Self::Connected(server) => server.dispatch(name, args),
        }
    }
    fn tools(&self) -> Value {
        match self {
            Self::Local(_) => tool_list(),
            Self::Connected(_) => connected_tool_list(),
        }
    }
}
fn connected_tool_list() -> Value {
    json!([
        tool(
            "agent.inspect",
            "Inspect effective permissions and authority mode"
        ),
        tool(
            "blueprint.list",
            "List published blueprint versions (workspace scope)"
        ),
        tool(
            "blueprint.show",
            "Read a published blueprint definition (workspace scope)"
        ),
        tool(
            "blueprint.binding",
            "Read a project's pinned blueprint version"
        ),
        tool(
            "blueprint.item",
            "Inspect a blueprint-defined work item in your project scope"
        ),
        tool(
            "blueprint.publish",
            "Publish a validated immutable blueprint (developer, workspace scope)"
        ),
        tool(
            "blueprint.install",
            "Install a bundled blueprint (developer, workspace scope)"
        ),
        tool(
            "blueprint.bind",
            "Bind a project to a pinned blueprint version (developer)"
        ),
        tool(
            "blueprint.create_item",
            "Create an item in a scoped project (developer)"
        ),
        tool(
            "blueprint.transition",
            "Transition a scoped work item with optimistic revision (developer)"
        ),
        tool(
            "blueprint.properties",
            "Update scoped item properties with optimistic revision (developer)"
        ),
        tool(
            "blueprint.template",
            "Apply an atomic project template to a scoped project (developer)"
        ),
        tool(
            "agent.job_context",
            "Read the job-scoped authoritative execution context (online only)"
        ),
        tool(
            "agent.handoff_submit",
            "Submit an immutable, job-scoped handoff to the authority (online only)"
        ),
        tool(
            "planning.capabilities",
            "Discover supported deterministic planning operations and their arguments"
        ),
        orbit_runtime::capabilities::mcp_tool("task.dependencies.add")
            .expect("registered planning tool"),
        orbit_runtime::capabilities::mcp_tool("task.dependencies.remove")
            .expect("registered planning tool"),
        orbit_runtime::capabilities::mcp_tool("task.readiness").expect("registered planning tool"),
        orbit_runtime::capabilities::mcp_tool("task.dependencies.preview")
            .expect("registered planning tool"),
        orbit_runtime::capabilities::mcp_tool("task.graph").expect("registered planning tool"),
        orbit_runtime::capabilities::mcp_tool("task.next").expect("registered planning tool"),
        orbit_runtime::capabilities::mcp_tool("planning.portfolio_actions")
            .expect("registered planning tool"),
        orbit_runtime::capabilities::mcp_tool("planning.portfolio_capacity")
            .expect("registered planning tool"),
        orbit_runtime::capabilities::mcp_tool("planning.compare_scenarios")
            .expect("registered planning tool"),
        orbit_runtime::capabilities::mcp_tool("planning.review_scenario")
            .expect("registered planning tool"),
        orbit_runtime::capabilities::mcp_tool("planning.review_scenario_options")
            .expect("registered planning tool"),
        orbit_runtime::capabilities::mcp_tool("planning.apply_approved_scenario")
            .expect("registered planning tool"),
        orbit_runtime::capabilities::mcp_tool("planning.day").expect("registered planning tool"),
        orbit_runtime::capabilities::mcp_tool("planning.schedule")
            .expect("registered planning tool"),
        orbit_runtime::capabilities::mcp_tool("planning.workload")
            .expect("registered planning tool"),
        tool(
            "projects.list",
            "Read accepted projects from last server snapshot"
        ),
        tool(
            "tasks.list",
            "Read accepted tasks from last server snapshot"
        ),
        orbit_runtime::capabilities::mcp_tool("planning.handoff_task")
            .expect("registered planning tool"),
        orbit_runtime::capabilities::mcp_tool("planning.handoff_group")
            .expect("registered planning tool"),
        orbit_runtime::capabilities::mcp_tool("planning.resolve_task_action")
            .expect("registered planning tool"),
        orbit_runtime::capabilities::mcp_tool("planning.resolve_group_action")
            .expect("registered planning tool"),
        orbit_runtime::capabilities::mcp_tool("planning.task_actions")
            .expect("registered planning tool"),
        orbit_runtime::capabilities::mcp_tool("planning.group_actions")
            .expect("registered planning tool"),
        orbit_runtime::capabilities::mcp_tool("planning.task_focus")
            .expect("registered planning tool"),
        orbit_runtime::capabilities::mcp_tool("planning.group_focus")
            .expect("registered planning tool"),
        orbit_runtime::capabilities::mcp_tool("planning.work_brief")
            .expect("registered planning tool"),
        orbit_runtime::capabilities::mcp_tool("planning.completion_impact")
            .expect("registered planning tool"),
        tool("habits.list", "Read accepted habits"),
        tool("habit_occurrences.list", "Read accepted habit occurrences"),
        tool("metrics.list", "Read accepted metric targets"),
        tool(
            "metric_observations.list",
            "Read accepted metric observations"
        ),
        tool(
            "project.command",
            "Queue canonical project creation with project scope enforcement"
        ),
        tool(
            "task.command",
            "Queue canonical task mutations with old and new project scope enforcement"
        ),
        tool(
            "evidence.command",
            "Queue canonical habit and metric evidence commands"
        ),
        tool("schedule.list", "Read accepted schedule blocks"),
        tool("routines.list", "Read accepted routines"),
        tool("milestones.list", "Read accepted milestones"),
        tool("strategies.list", "Read accepted project strategies"),
        tool(
            "schedule.command",
            "Queue a canonical schedule command with stable command ID"
        ),
        tool(
            "routine.command",
            "Queue a canonical routine command with stable command ID"
        ),
        tool(
            "milestone.command",
            "Queue a canonical milestone command with stable command ID"
        ),
        tool(
            "strategy.command",
            "Queue a canonical strategy command with stable command ID"
        ),
        tool(
            "authority.status",
            "Inspect accepted revision and unresolved queue size"
        ),
        tool("authority.unresolved", "Inspect unresolved remote commands"),
        tool(
            "authority.refresh",
            "Refresh last accepted projection from server"
        ),
        tool(
            "authority.flush",
            "Submit durable pending commands to server"
        ),
        tool(
            "authority.workflow",
            "Review, authorize and queue an ordered cross-domain workflow"
        ),
        tool(
            "authority.resolve",
            "Resolve with an explicit accepted-history review token (alias for authority.resolve_reviewed)"
        ),
        tool(
            "authority.workspace_sync",
            "Read per-domain pending and conflicted commands with accepted-state health"
        ),
        tool(
            "authority.observe_workspace_sync",
            "Inspect the same report against the current authority revision"
        ),
        tool(
            "authority.sync_health",
            "Read actionable offline synchronization blockers and recovery steps"
        ),
        tool(
            "authority.observe_sync_health",
            "Compare sync blockers against the live authority"
        ),
        tool(
            "authority.convergence",
            "Read local accepted state and unresolved sync status"
        ),
        tool(
            "authority.observe_convergence",
            "Compare local replica against current server accepted revision and fingerprint"
        ),
        tool(
            "authority.converge",
            "Replay pending writes, refresh cache and observe multi-client convergence"
        ),
        tool(
            "authority.review_online",
            "Refresh accepted server history before constructing conflict review"
        ),
        tool(
            "authority.conflict_review",
            "Inspect conflicted command with its accepted-history review token"
        ),
        tool(
            "authority.resolve_reviewed",
            "Resolve conflict only if server history still matches reviewed state"
        ),
        tool(
            "authority.resolve_reviewed_batch",
            "Stage several reviewed conflict resolutions in one local transaction"
        ),
    ])
}

struct Server {
    authority: AuthoritativeHost,
    policy: AgentPolicy,
}

fn tool_list() -> Value {
    json!([
        tool(
            "agent.inspect",
            "Inspect effective Orbit-owned agent/session permissions"
        ),
        tool(
            "blueprint.list",
            "List published blueprint versions (workspace scope)"
        ),
        tool(
            "blueprint.show",
            "Read a published blueprint definition (workspace scope)"
        ),
        tool(
            "blueprint.binding",
            "Read a project's pinned blueprint version"
        ),
        tool(
            "blueprint.item",
            "Inspect a blueprint-defined work item in your project scope"
        ),
        tool(
            "blueprint.publish",
            "Publish a validated immutable blueprint (developer, workspace scope)"
        ),
        tool(
            "blueprint.install",
            "Install a bundled blueprint (developer, workspace scope)"
        ),
        tool(
            "blueprint.bind",
            "Bind a project to a pinned blueprint version (developer)"
        ),
        tool(
            "blueprint.create_item",
            "Create an item in a scoped project (developer)"
        ),
        tool(
            "blueprint.transition",
            "Transition a scoped work item with optimistic revision (developer)"
        ),
        tool(
            "blueprint.properties",
            "Update scoped item properties with optimistic revision (developer)"
        ),
        tool(
            "blueprint.template",
            "Apply an atomic project template to a scoped project (developer)"
        ),
        tool(
            "agent.job_context",
            "Read the execution brief and latest handoff for the ORBIT_MCP_JOB_ID bound job"
        ),
        tool(
            "agent.handoff_submit",
            "Submit an append-only structured handoff for the bound job (developer mode)"
        ),
        tool(
            "planning.capabilities",
            "Discover supported deterministic planning operations and their arguments"
        ),
        orbit_runtime::capabilities::mcp_tool("task.dependencies.add")
            .expect("registered planning tool"),
        orbit_runtime::capabilities::mcp_tool("task.dependencies.remove")
            .expect("registered planning tool"),
        orbit_runtime::capabilities::mcp_tool("task.readiness").expect("registered planning tool"),
        orbit_runtime::capabilities::mcp_tool("task.dependencies.preview")
            .expect("registered planning tool"),
        orbit_runtime::capabilities::mcp_tool("task.graph").expect("registered planning tool"),
        orbit_runtime::capabilities::mcp_tool("task.next").expect("registered planning tool"),
        orbit_runtime::capabilities::mcp_tool("planning.portfolio_actions")
            .expect("registered planning tool"),
        orbit_runtime::capabilities::mcp_tool("planning.portfolio_capacity")
            .expect("registered planning tool"),
        orbit_runtime::capabilities::mcp_tool("planning.compare_scenarios")
            .expect("registered planning tool"),
        orbit_runtime::capabilities::mcp_tool("planning.review_scenario")
            .expect("registered planning tool"),
        orbit_runtime::capabilities::mcp_tool("planning.review_scenario_options")
            .expect("registered planning tool"),
        orbit_runtime::capabilities::mcp_tool("planning.apply_approved_scenario")
            .expect("registered planning tool"),
        orbit_runtime::capabilities::mcp_tool("planning.day").expect("registered planning tool"),
        orbit_runtime::capabilities::mcp_tool("planning.schedule")
            .expect("registered planning tool"),
        orbit_runtime::capabilities::mcp_tool("planning.workload")
            .expect("registered planning tool"),
        tool("projects.list", "List projects visible to this agent"),
        tool(
            "project.evidence_report",
            "Report bound habit and metric progress as of an explicit timestamp"
        ),
        tool(
            "project.health",
            "Get canonical project health and completion criteria"
        ),
        tool(
            "project.evidence_list",
            "List evidence bindings for a scoped project"
        ),
        tool(
            "project.evidence_bind",
            "Bind a habit or metric to a scoped project (developer mode)"
        ),
        tool(
            "project.evidence_unbind",
            "Unbind a habit or metric from a scoped project (developer mode)"
        ),
        tool(
            "project.metric_set_target",
            "Set a bound project metric target (developer mode)"
        ),
        tool(
            "project.metric_observe",
            "Record a bound project metric observation (developer mode)"
        ),
        tool("tasks.list", "List visible tasks"),
        orbit_runtime::capabilities::mcp_tool("planning.handoff_task")
            .expect("registered planning tool"),
        orbit_runtime::capabilities::mcp_tool("planning.handoff_group")
            .expect("registered planning tool"),
        orbit_runtime::capabilities::mcp_tool("planning.resolve_task_action")
            .expect("registered planning tool"),
        orbit_runtime::capabilities::mcp_tool("planning.resolve_group_action")
            .expect("registered planning tool"),
        orbit_runtime::capabilities::mcp_tool("planning.task_actions")
            .expect("registered planning tool"),
        orbit_runtime::capabilities::mcp_tool("planning.group_actions")
            .expect("registered planning tool"),
        orbit_runtime::capabilities::mcp_tool("planning.task_focus")
            .expect("registered planning tool"),
        orbit_runtime::capabilities::mcp_tool("planning.group_focus")
            .expect("registered planning tool"),
        orbit_runtime::capabilities::mcp_tool("planning.work_brief")
            .expect("registered planning tool"),
        orbit_runtime::capabilities::mcp_tool("planning.completion_impact")
            .expect("registered planning tool"),
        tool("milestones.list", "List milestones for a scoped project"),
        tool("habits.list", "List personal habits when permitted"),
        tool(
            "priorities.list",
            "List current visible in-progress/ready/planned/blocked tasks"
        ),
        tool("knowledge.tree", "List physical Markdown folders and notes"),
        tool("knowledge.read", "Read Markdown by stable note ID"),
        tool(
            "knowledge.search",
            "Search authoritative Markdown documents"
        ),
        tool("knowledge.relations", "Get stable-ID links and backlinks"),
        tool(
            "knowledge.command",
            "Create, edit or move a Markdown note with durable acceptance"
        ),
        tool("knowledge.folder_move", "Revisioned folder rename/move"),
        tool(
            "knowledge.folder_create",
            "Create a physical folder with durable acceptance"
        ),
        tool(
            "task.create",
            "Create a task through Orbit application services"
        ),
        tool("task.set_status", "Transition a task"),
        tool("project.set_status", "Transition a project"),
        tool(
            "project.configure_strategy",
            "Configure project mode, progress signals, and success criteria"
        ),
        tool(
            "project.set_criterion_status",
            "Update one project success criterion status"
        ),
        tool("milestone.set_status", "Transition a milestone"),
        tool("habit.set_active", "Pause/resume a habit"),
    ])
}
fn tool(name: &str, description: &str) -> Value {
    json!({"name":name,"description":description,"inputSchema":{"type":"object","additionalProperties":true}})
}

fn main() {
    if let Err(e) = run() {
        eprintln!("orbit-mcp: {e}");
        process::exit(2)
    }
}
fn run() -> Result<(), String> {
    let policy = AgentPolicy::from_env()?;
    let mut server =
        match orbit_client::deployment::from_env().map_err(|error| error.to_string())? {
            orbit_client::deployment::Deployment::Connected { replica, server } => {
                let device = env::var("ORBIT_DEVICE_ID").ok();
                let token = env::var("ORBIT_DEVICE_TOKEN").ok();
                let credentials = match (device.as_deref(), token.as_deref()) {
                (Some(d), Some(t)) if !d.is_empty() && !t.is_empty() => Some((d, t)),
                (None, None) => None,
                _ => return Err(
                    "set both ORBIT_DEVICE_ID and ORBIT_DEVICE_TOKEN, or neither for offline mode"
                        .into(),
                ),
            };
                let client = AuthorityConnection::remote(&replica, &server, credentials)
                    .map_err(debug_err)?;
                McpBackend::Connected(ConnectedMcp { client, policy })
            }
            orbit_client::deployment::Deployment::Local { database } => {
                let authority = AuthoritativeHost::open(&database).map_err(debug_err)?;
                McpBackend::Local(Server { authority, policy })
            }
        };
    let stdin = io::stdin();
    let mut out = io::stdout().lock();
    for line in stdin.lock().lines() {
        let line = line.map_err(|e| e.to_string())?;
        if line.trim().is_empty() {
            continue;
        }
        let req: Value = serde_json::from_str(&line).map_err(|e| e.to_string())?;
        let id = req.get("id").cloned().unwrap_or(Value::Null);
        let method = req.get("method").and_then(Value::as_str).unwrap_or("");
        let response = match method {
            "server/discover" => {
                json!({"jsonrpc":"2.0","id":id,"result":{"protocolVersion":"2026-07-28","serverInfo":{"name":"orbit-mcp","version":"0.1.0"},"capabilities":{"tools":{}}}})
            }
            "initialize" => {
                json!({"jsonrpc":"2.0","id":id,"result":{"protocolVersion":"2025-06-18","serverInfo":{"name":"orbit-mcp","version":"0.1.0"},"capabilities":{"tools":{}}}})
            }
            "notifications/initialized" => continue,
            "tools/list" => json!({"jsonrpc":"2.0","id":id,"result":{"tools":server.tools()}}),
            "tools/call" => {
                let params = req.get("params").cloned().unwrap_or_else(|| json!({}));
                let name = params.get("name").and_then(Value::as_str).unwrap_or("");
                let args = params
                    .get("arguments")
                    .cloned()
                    .unwrap_or_else(|| json!({}));
                match server.dispatch(name, &args) {
                    Ok(v) => {
                        json!({"jsonrpc":"2.0","id":id,"result":{"content":[{"type":"text","text":serde_json::to_string_pretty(&v).unwrap_or_default()}],"structuredContent":v,"isError":false}})
                    }
                    Err(e) => {
                        json!({"jsonrpc":"2.0","id":id,"result":{"content":[{"type":"text","text":e}],"isError":true}})
                    }
                }
            }
            _ => {
                json!({"jsonrpc":"2.0","id":id,"error":{"code":-32601,"message":"method not found"}})
            }
        };
        serde_json::to_writer(&mut out, &response).map_err(|e| e.to_string())?;
        writeln!(&mut out).map_err(|e| e.to_string())?;
        out.flush().map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn required_str(args: &Value, key: &str) -> Result<String, String> {
    args.get(key)
        .and_then(Value::as_str)
        .filter(|v| !v.trim().is_empty())
        .map(str::to_owned)
        .ok_or_else(|| format!("missing/blank {key}"))
}
fn required_i64(args: &Value, key: &str) -> Result<i64, String> {
    args.get(key)
        .and_then(Value::as_i64)
        .ok_or_else(|| format!("missing integer {key}"))
}
fn required_bool(args: &Value, key: &str) -> Result<bool, String> {
    args.get(key)
        .and_then(Value::as_bool)
        .ok_or_else(|| format!("missing boolean {key}"))
}
fn env_bool(key: &str, default: bool) -> bool {
    env::var(key)
        .ok()
        .map(|v| matches!(v.to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on"))
        .unwrap_or(default)
}
fn debug_err<E: std::fmt::Debug>(e: E) -> String {
    format!("{e:?}")
}
fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or_default()
}
fn new_id(prefix: &str) -> String {
    format!(
        "{prefix}-{}-{}-{}",
        now_ms(),
        process::id(),
        IDS.fetch_add(1, Ordering::Relaxed)
    )
}
/// Stable command and activity identities when MCP callers explicitly supply
/// a command_id. The same payload can be retried after transport failure.
/// Without an id, a fresh command/activity identity is generated as before.
fn mcp_command_id(args: &Value) -> String {
    args.get("command_id")
        .and_then(Value::as_str)
        .filter(|id| !id.trim().is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| new_id("command"))
}
fn mcp_entity_id(args: &Value, kind: &str) -> String {
    match args
        .get("command_id")
        .and_then(Value::as_str)
        .filter(|v| !v.trim().is_empty())
    {
        Some(id) => format!("{kind}-{id}"),
        None => new_id(kind),
    }
}
fn mcp_activity_id(args: &Value) -> Result<ActivityId, String> {
    match args
        .get("command_id")
        .and_then(Value::as_str)
        .filter(|v| !v.trim().is_empty())
    {
        Some(id) => ActivityId::new(format!("activity-{id}")).map_err(debug_err),
        None => new_activity_id(),
    }
}

fn new_activity_id() -> Result<ActivityId, String> {
    ActivityId::new(new_id("activity")).map_err(debug_err)
}

fn parse_task_status(v: &str) -> Result<TaskStatus, String> {
    match v {
        "inbox" => Ok(TaskStatus::Inbox),
        "planned" => Ok(TaskStatus::Planned),
        "ready" => Ok(TaskStatus::Ready),
        "in_progress" => Ok(TaskStatus::InProgress),
        "waiting" => Ok(TaskStatus::Waiting),
        "blocked" => Ok(TaskStatus::Blocked),
        "review" => Ok(TaskStatus::Review),
        "deferred" => Ok(TaskStatus::Deferred),
        "done" => Ok(TaskStatus::Done),
        "cancelled" => Ok(TaskStatus::Cancelled),
        _ => Err("invalid task status".into()),
    }
}
fn parse_project_status(v: &str) -> Result<ProjectStatus, String> {
    match v {
        "proposed" => Ok(ProjectStatus::Proposed),
        "active" => Ok(ProjectStatus::Active),
        "on_hold" => Ok(ProjectStatus::OnHold),
        "completed" => Ok(ProjectStatus::Completed),
        "cancelled" => Ok(ProjectStatus::Cancelled),
        "archived" => Ok(ProjectStatus::Archived),
        _ => Err("invalid project status".into()),
    }
}
fn parse_milestone_status(v: &str) -> Result<MilestoneStatus, String> {
    match v {
        "planned" => Ok(MilestoneStatus::Planned),
        "in_progress" => Ok(MilestoneStatus::InProgress),
        "blocked" => Ok(MilestoneStatus::Blocked),
        "completed" => Ok(MilestoneStatus::Completed),
        "cancelled" => Ok(MilestoneStatus::Cancelled),
        _ => Err("invalid milestone status".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn policy(mode: Mode) -> AgentPolicy {
        AgentPolicy {
            agent_id: "codex".into(),
            session_id: "test".into(),
            mode,
            projects: ["orbit-core".to_owned()].into_iter().collect(),
            allow_unscoped_execution: false,
            allow_unscoped_knowledge: false,
        }
    }
    #[test]
    fn explicit_mcp_command_id_stabilizes_activity_and_generated_entity_ids() {
        let args = json!({"command_id":"stable-agent-command"});
        assert_eq!(mcp_command_id(&args), "stable-agent-command");
        assert_eq!(mcp_entity_id(&args, "task"), "task-stable-agent-command");
        assert_eq!(
            mcp_activity_id(&args).unwrap(),
            mcp_activity_id(&args).unwrap()
        );
        let other = json!({"command_id":"another-command"});
        assert_ne!(
            mcp_activity_id(&args).unwrap(),
            mcp_activity_id(&other).unwrap()
        );
    }

    #[test]
    fn mcp_retry_does_not_repeat_task_or_activity_and_rejects_payload_change() {
        use orbit_application::{CreateProjectCommand, CreateProjectWithActivityCommand};
        let mut server = Server {
            authority: AuthoritativeHost::open(":memory:").unwrap(),
            policy: policy(Mode::Developer),
        };
        server
            .authority
            .create_project_accepted(
                "setup",
                "setup-project",
                0,
                CreateProjectWithActivityCommand {
                    project: CreateProjectCommand {
                        id: ProjectId::new("orbit-core").unwrap(),
                        name: "Orbit Core".into(),
                    },
                    activity_id: ActivityId::new("setup-activity").unwrap(),
                    origin: ActivityOrigin::Human,
                    summary: "Create allowed project".into(),
                },
            )
            .unwrap();
        let args = json!({
            "command_id":"task-mcp-retry",
            "project_id":"orbit-core",
            "title":"Accepted MCP task"
        });
        let first = server.dispatch("task.create", &args).unwrap();
        assert_eq!(first["id"], "task-task-mcp-retry");
        assert_eq!(server.authority.snapshot().unwrap().0, 2);
        assert_eq!(
            server
                .authority
                .application_queries()
                .activities()
                .unwrap()
                .len(),
            2
        );
        assert_eq!(server.dispatch("task.create", &args).unwrap(), first);
        assert_eq!(server.authority.snapshot().unwrap().0, 2);
        assert_eq!(
            server
                .authority
                .application_queries()
                .activities()
                .unwrap()
                .len(),
            2
        );
        let changed = json!({
            "command_id":"task-mcp-retry", "project_id":"orbit-core",
            "title":"Changed on retry",
        });
        assert!(server.dispatch("task.create", &changed).is_err());
        assert_eq!(
            server
                .authority
                .application_queries()
                .tasks()
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn mcp_readonly_policy_blocks_canonical_command_admission() {
        let mut server = Server {
            authority: AuthoritativeHost::open(":memory:").unwrap(),
            policy: policy(Mode::ReadOnly),
        };
        let args = json!({
            "command_id":"read-only-task", "project_id":"orbit-core", "title":"Denied"
        });
        assert!(server.dispatch("task.create", &args).is_err());
        assert_eq!(server.authority.snapshot().unwrap().0, 0);
    }

    #[test]
    fn evidence_endpoints_enforce_project_scope_and_read_only_mode() {
        let mut server = Server {
            authority: AuthoritativeHost::open(":memory:").unwrap(),
            policy: policy(Mode::ReadOnly),
        };
        assert!(
            server
                .dispatch("project.evidence_list", &json!({"project_id":"forbidden"}))
                .is_err()
        );
        assert!(
            server
                .dispatch(
                    "project.evidence_bind",
                    &json!({"project_id":"orbit-core","kind":"metric","evidence_id":"x"})
                )
                .is_err()
        );
        assert!(
            server
                .dispatch(
                    "project.evidence_report",
                    &json!({"project_id":"forbidden","start_ms":0,"end_ms":1,"as_of_ms":1})
                )
                .is_err()
        );
    }
    #[test]
    fn developer_and_project_scope_are_orbit_owned() {
        let p = policy(Mode::Developer);
        assert!(p.write_allowed().is_ok());
        assert!(p.project_allowed(&ProjectId::new("orbit-core").unwrap()));
        assert!(!p.project_allowed(&ProjectId::new("personal").unwrap()));
    }
    #[test]
    fn read_only_policy_rejects_mutation() {
        assert!(policy(Mode::ReadOnly).write_allowed().is_err());
    }
}

#[cfg(test)]
mod canonical_markdown_mcp_tests {
    use super::*;
    fn policy(mode: Mode, allow: bool) -> AgentPolicy {
        AgentPolicy {
            agent_id: "agent".into(),
            session_id: "session".into(),
            mode,
            projects: BTreeSet::new(),
            allow_unscoped_execution: false,
            allow_unscoped_knowledge: allow,
        }
    }
    fn server(mode: Mode, allow: bool) -> Server {
        Server {
            authority: AuthoritativeHost::open(":memory:").unwrap(),
            policy: policy(mode, allow),
        }
    }
    #[test]
    fn canonical_mcp_markdown_create_read_search_and_move() {
        let mut s = server(Mode::Developer, true);
        // Folder tree is the real vault, not the legacy SQL knowledge index.
        s.authority.knowledge_tree().unwrap();
        s.authority.markdown_knowledge_index().unwrap();
        assert_eq!(
            s.dispatch(
                "knowledge.folder_create",
                &json!({"command_id":"mkdir-1","command":{"path":"Work"}})
            )
            .unwrap()["path"],
            "Work"
        );
        let command =
            json!({"kind":"Create","note_id":"note-1","path":"note.md","body":"# One\n[[note-2]]"});
        let create = json!({"command_id":"create-1","command":command});
        let receipt = s.dispatch("knowledge.command", &create).unwrap();
        assert_eq!(receipt["revision"], 1);
        assert_eq!(receipt, s.dispatch("knowledge.command", &create).unwrap());
        assert!(
            s.dispatch("knowledge.read", &json!({"note_id":"note-1"}))
                .unwrap()["markdown"]
                .as_str()
                .unwrap()
                .contains("# One")
        );
        assert_eq!(
            s.dispatch("knowledge.search", &json!({"text":"One"}))
                .unwrap()
                .as_array()
                .unwrap()
                .len(),
            1
        );
        let update = json!({"command_id":"edit-1","command":{"kind":"Update","note_id":"note-1",
            "expected":"---\norbit_id: note-1\n---\n# One\n[[note-2]]",
            "replacement":"---\norbit_id: note-1\n---\n# Two"}});
        assert_eq!(
            s.dispatch("knowledge.command", &update).unwrap()["revision"],
            2
        );
        assert!(
            s.dispatch(
                "knowledge.command",
                &json!({"command_id":"edit-2", "command":{
            "kind":"Update","note_id":"note-1","expected":"stale","replacement":"tampered"}})
            )
            .is_err()
        );
        assert_eq!(
            s.dispatch("knowledge.search", &json!({"text":"Two"}))
                .unwrap()
                .as_array()
                .unwrap()
                .len(),
            1
        );
    }
    #[test]
    fn markdown_permissions_fail_closed() {
        let mut read_only = server(Mode::ReadOnly, true);
        assert!(
            read_only
                .dispatch("knowledge.command", &json!({"command_id":"c","command":{}}))
                .is_err()
        );
        let mut restricted = server(Mode::Developer, false);
        assert!(restricted.dispatch("knowledge.tree", &json!({})).is_err());
        assert!(
            restricted
                .dispatch("knowledge.search", &json!({"text":"anything"}))
                .is_err()
        );
        assert!(
            restricted
                .dispatch("knowledge.command", &json!({"command_id":"c","command":{}}))
                .is_err()
        );
        assert!(
            restricted
                .dispatch(
                    "knowledge.folder_move",
                    &json!({"command_id":"c","move":{}})
                )
                .is_err()
        );
        assert!(
            restricted
                .dispatch(
                    "knowledge.folder_create",
                    &json!({"command_id":"c","command":{}})
                )
                .is_err()
        );
        // Retired SQL knowledge capabilities are never available via dispatch.
        for old in [
            "knowledge.create",
            "knowledge.link",
            "knowledge.context",
            "knowledge.backlinks",
        ] {
            assert!(restricted.dispatch(old, &json!({})).is_err());
        }
    }
}

#[cfg(test)]
mod connected_authority_mcp_tests {
    use super::*;

    fn policy() -> AgentPolicy {
        AgentPolicy {
            agent_id: "integration-agent".into(),
            session_id: "test".into(),
            mode: Mode::Developer,
            projects: BTreeSet::new(),
            allow_unscoped_execution: true,
            allow_unscoped_knowledge: false,
        }
    }
    fn offline() -> ConnectedMcp {
        ConnectedMcp {
            client: AuthorityConnection::remote(":memory:", "https://example.invalid", None)
                .unwrap(),
            policy: policy(),
        }
    }
    #[test]
    fn disconnected_mcp_never_claims_queued_command_is_accepted() {
        let mut server = offline();
        let result = server
            .dispatch(
                "routine.command",
                &json!({
                    "command_id":"routine-test-1","expected_revision":0,
                    "action":{"type":"create","routine_id":"r1","name":"Morning","habit_ids":[]}
                }),
            )
            .unwrap();
        assert_eq!(result["disposition"], "pending");
        assert_eq!(result["accepted"], false);
        assert_eq!(
            server.dispatch("routines.list", &json!({})).unwrap(),
            json!([])
        );
        assert_eq!(
            server.dispatch("authority.status", &json!({})).unwrap()["has_accepted_snapshot"],
            false
        );
    }
    #[test]
    fn connected_schedule_mutation_requires_explicit_unscoped_access() {
        let mut server = offline();
        server.policy.allow_unscoped_execution = false;
        let err = server
            .dispatch(
                "schedule.command",
                &json!({
                    "command_id":"s1","expected_revision":0,
                    "action":{"type":"delete","block_id":"b1"}
                }),
            )
            .unwrap_err();
        assert!(err.contains("unscoped"));
        assert_eq!(
            server.dispatch("schedule.list", &json!({})).unwrap(),
            json!([])
        );
    }
    #[test]
    fn readonly_agent_cannot_queue_offline_mutations() {
        let mut server = offline();
        server.policy.mode = Mode::ReadOnly;
        let err = server
            .dispatch(
                "routine.command",
                &json!({
                    "command_id":"r1","expected_revision":0,
                    "action":{"type":"delete","routine_id":"r1"}
                }),
            )
            .unwrap_err();
        assert!(err.contains("read-only"));
    }
    #[test]
    fn scoped_agent_cannot_write_to_other_project_strategy() {
        let mut server = offline();
        server.policy.projects.insert("project-visible".into());
        let err = server
            .dispatch(
                "strategy.command",
                &json!({
                    "command_id":"s1","expected_revision":0,
                    "action":{"type":"set_objective","project_id":"other","objective":"wrong"}
                }),
            )
            .unwrap_err();
        assert!(err.contains("outside agent scope"));
    }
    #[test]
    fn local_preview_and_mutation_observe_distinct_revision_boundaries() {
        let mut server = Server {
            authority: AuthoritativeHost::open(":memory:").unwrap(),
            policy: AgentPolicy {
                agent_id: "agent".into(),
                session_id: "session".into(),
                mode: Mode::Developer,
                projects: Default::default(),
                allow_unscoped_execution: true,
                allow_unscoped_knowledge: false,
            },
        };
        for id in ["a", "b"] {
            let rev = server.authority.snapshot().unwrap().0;
            server
                .authority
                .task_command(
                    "setup",
                    TaskCommand {
                        command_id: format!("create-{id}"),
                        expected_revision: rev,
                        action: TaskAction::Create {
                            task_id: id.into(),
                            title: id.into(),
                            project_id: None,
                        },
                    },
                )
                .unwrap();
        }
        let args = json!({"task_id":"b","prerequisite_id":"a","edit":"add","now_ms":100});
        let before = server.authority.snapshot().unwrap().0;
        let preview = server.dispatch("task.dependencies.preview", &args).unwrap();
        assert_eq!(preview["accepted_revision"], before);
        assert_eq!(preview["changes_graph"], true);
        assert_eq!(preview["before_edge_count"], 0);
        assert_eq!(preview["after_edge_count"], 1);
        assert_eq!(server.authority.snapshot().unwrap().0, before);
        assert!(server.authority.task_dependency_edges().unwrap().is_empty());
        let command = json!({"command_id":"accepted-edge","expected_revision":before,
            "task_id":"b","prerequisite_id":"a"});
        assert_eq!(
            server.dispatch("task.dependencies.add", &command).unwrap()["duplicate"],
            false
        );
        assert_eq!(server.authority.snapshot().unwrap().0, before + 1);
        let after = server.dispatch("task.dependencies.preview", &args).unwrap();
        assert_eq!(after["changes_graph"], false);
        assert_eq!(after["accepted_revision"], before + 1);
    }

    #[test]
    fn local_preview_is_safe_for_readonly_agents_but_scoped_agent_is_denied() {
        let mut server = Server {
            authority: AuthoritativeHost::open(":memory:").unwrap(),
            policy: AgentPolicy {
                agent_id: "agent".into(),
                session_id: "session".into(),
                mode: Mode::ReadOnly,
                projects: Default::default(),
                allow_unscoped_execution: true,
                allow_unscoped_knowledge: false,
            },
        };
        let args = json!({"task_id":"a","prerequisite_id":"b","edit":"add","now_ms":100});
        // Readonly callers reach domain validation, not a blanket mutation denial.
        let err = server
            .dispatch("task.dependencies.preview", &args)
            .unwrap_err();
        assert!(!err.contains("read-only"));
        assert!(
            server
                .dispatch(
                    "task.dependencies.add",
                    &json!({
                        "command_id":"x","expected_revision":0,"task_id":"a","prerequisite_id":"b"
                    })
                )
                .unwrap_err()
                .contains("read-only")
        );
        server.policy.projects.insert("private".into());
        assert!(
            server
                .dispatch("task.dependencies.preview", &args)
                .unwrap_err()
                .contains("unrestricted")
        );
    }

    #[test]
    fn shared_mcp_backend_validates_before_invocation_in_both_modes() {
        let local = Server {
            authority: AuthoritativeHost::open(":memory:").unwrap(),
            policy: AgentPolicy {
                agent_id: "agent".into(),
                session_id: "session".into(),
                mode: Mode::Developer,
                projects: Default::default(),
                allow_unscoped_execution: true,
                allow_unscoped_knowledge: false,
            },
        };
        let mut backend = McpBackend::Local(local);
        let rejected = backend
            .dispatch(
                "task.dependencies.add",
                &json!({
                    "command_id":"x","expected_revision":"zero","task_id":"a","prerequisite_id":"b"
                }),
            )
            .unwrap_err();
        let validation: Value = serde_json::from_str(&rejected).unwrap();
        assert_eq!(validation["code"], "invalid_argument_type");
        assert_eq!(validation["argument"], "expected_revision");
        let unknown = backend
            .dispatch(
                "task.dependencies.add",
                &json!({
                    "command_id":"x","expected_revision":0,"task_id":"a","prerequisite_id":"b",
                    "dangerous_ignored_extra":true
                }),
            )
            .unwrap_err();
        let extra: Value = serde_json::from_str(&unknown).unwrap();
        assert_eq!(extra["code"], "unknown_mutation_argument");
        assert_eq!(extra["argument"], "dangerous_ignored_extra");
    }

    #[test]
    fn project_scoped_dependency_writes_check_both_endpoints() {
        let mut server = Server {
            authority: AuthoritativeHost::open(":memory:").unwrap(),
            policy: AgentPolicy {
                agent_id: "agent".into(),
                session_id: "session".into(),
                mode: Mode::Developer,
                projects: Default::default(),
                allow_unscoped_execution: false,
                allow_unscoped_knowledge: false,
            },
        };
        for project in ["visible", "private"] {
            let revision = server.authority.snapshot().unwrap().0;
            server
                .authority
                .project_command(
                    "setup",
                    ProjectCommand {
                        command_id: format!("project-{project}"),
                        expected_revision: revision,
                        project_id: project.into(),
                        name: project.into(),
                    },
                )
                .unwrap();
        }
        for (id, project) in [
            ("visible-1", "visible"),
            ("visible-2", "visible"),
            ("hidden-1", "private"),
        ] {
            let revision = server.authority.snapshot().unwrap().0;
            server
                .authority
                .task_command(
                    "setup",
                    TaskCommand {
                        command_id: format!("create-{id}"),
                        expected_revision: revision,
                        action: TaskAction::Create {
                            task_id: id.into(),
                            title: id.into(),
                            project_id: Some(project.into()),
                        },
                    },
                )
                .unwrap();
        }
        server.policy.projects.insert("visible".into());
        let before = server.authority.snapshot().unwrap().0;
        let forbidden = json!({"command_id":"forbidden-edge","expected_revision":before,
            "task_id":"visible-1","prerequisite_id":"hidden-1"});
        assert!(
            server
                .dispatch("task.dependencies.add", &forbidden)
                .unwrap_err()
                .contains("outside agent project scope")
        );
        let reversed = json!({"command_id":"reversed-edge","expected_revision":before,
            "task_id":"hidden-1","prerequisite_id":"visible-1"});
        assert!(server.dispatch("task.dependencies.add", &reversed).is_err());
        assert!(server.authority.task_dependency_edges().unwrap().is_empty());
        assert_eq!(server.authority.snapshot().unwrap().0, before);
        let permitted = json!({"command_id":"allowed-edge","expected_revision":before,
            "task_id":"visible-1","prerequisite_id":"visible-2"});
        assert_eq!(
            server
                .dispatch("task.dependencies.add", &permitted)
                .unwrap()["duplicate"],
            false
        );
        assert_eq!(server.authority.task_dependency_edges().unwrap().len(), 1);
    }

    #[test]
    fn local_dependency_mcp_mutations_are_idempotent_cycle_safe_and_readable() {
        let mut server = Server {
            authority: AuthoritativeHost::open(":memory:").unwrap(),
            policy: AgentPolicy {
                agent_id: "agent".into(),
                session_id: "session".into(),
                mode: Mode::Developer,
                projects: Default::default(),
                allow_unscoped_execution: true,
                allow_unscoped_knowledge: false,
            },
        };
        for (id, title) in [
            ("task-a", "First"),
            ("task-b", "Second"),
            ("task-c", "Third"),
        ] {
            let revision = server.authority.snapshot().unwrap().0;
            let command = TaskCommand {
                command_id: format!("create-{id}"),
                expected_revision: revision,
                action: TaskAction::Create {
                    task_id: id.into(),
                    title: title.into(),
                    project_id: None,
                },
            };
            server.authority.task_command("setup", command).unwrap();
        }
        let rev = server.authority.snapshot().unwrap().0;
        let dependency = json!({"command_id":"edge-b-a","expected_revision":rev,
            "task_id":"task-b","prerequisite_id":"task-a"});
        let first = server
            .dispatch("task.dependencies.add", &dependency)
            .unwrap();
        assert_eq!(first["duplicate"], false);
        assert_eq!(
            server
                .dispatch("task.dependencies.add", &dependency)
                .unwrap()["duplicate"],
            true
        );
        let second = json!({"command_id":"edge-c-b","expected_revision":rev+1,
            "task_id":"task-c","prerequisite_id":"task-b"});
        server.dispatch("task.dependencies.add", &second).unwrap();
        let before = server.authority.snapshot().unwrap().0;
        let cycle = json!({"command_id":"cycle","expected_revision":before,
            "task_id":"task-a","prerequisite_id":"task-c"});
        assert!(server.dispatch("task.dependencies.add", &cycle).is_err());
        assert_eq!(server.authority.snapshot().unwrap().0, before);
        assert_eq!(
            server
                .dispatch("task.graph", &json!({"now_ms":1000}))
                .unwrap()["edge_count"],
            2
        );
        let next = server
            .dispatch("task.next", &json!({"now_ms":1000,"limit":10}))
            .unwrap();
        assert!(next.is_object());
        let remove = json!({"command_id":"remove-b-a","expected_revision":before,
            "task_id":"task-b","prerequisite_id":"task-a"});
        server
            .dispatch("task.dependencies.remove", &remove)
            .unwrap();
        assert_eq!(
            server
                .dispatch("task.graph", &json!({"now_ms":1000}))
                .unwrap()["edge_count"],
            1
        );
    }

    #[test]
    fn local_dependency_mcp_rejects_stale_revisions_without_a_partial_edge() {
        let mut server = Server {
            authority: AuthoritativeHost::open(":memory:").unwrap(),
            policy: AgentPolicy {
                agent_id: "agent".into(),
                session_id: "session".into(),
                mode: Mode::Developer,
                projects: Default::default(),
                allow_unscoped_execution: true,
                allow_unscoped_knowledge: false,
            },
        };
        for id in ["a", "b"] {
            let revision = server.authority.snapshot().unwrap().0;
            server
                .authority
                .task_command(
                    "setup",
                    TaskCommand {
                        command_id: format!("create-{id}"),
                        expected_revision: revision,
                        action: TaskAction::Create {
                            task_id: id.into(),
                            title: id.into(),
                            project_id: None,
                        },
                    },
                )
                .unwrap();
        }
        let rejected = json!({"command_id":"stale-edge","expected_revision":0,
            "task_id":"a","prerequisite_id":"b"});
        assert!(server.dispatch("task.dependencies.add", &rejected).is_err());
        assert!(server.authority.task_dependency_edges().unwrap().is_empty());
        server.policy.mode = Mode::ReadOnly;
        let revision = server.authority.snapshot().unwrap().0;
        let denied = json!({"command_id":"read-only-edge","expected_revision":revision,
            "task_id":"a","prerequisite_id":"b"});
        assert!(
            server
                .dispatch("task.dependencies.add", &denied)
                .unwrap_err()
                .contains("read-only")
        );
        assert!(server.authority.task_dependency_edges().unwrap().is_empty());
    }

    #[test]
    fn dependency_tools_are_advertised_in_both_authority_modes() {
        for tools in [tool_list(), connected_tool_list()] {
            let list = tools.as_array().unwrap();
            for name in [
                "task.dependencies.add",
                "task.dependencies.remove",
                "task.graph",
                "task.next",
            ] {
                let tool = list
                    .iter()
                    .find(|row| row["name"] == name)
                    .unwrap_or_else(|| panic!("missing capability {name}"));
                assert_eq!(tool["inputSchema"]["type"], "object");
                assert_eq!(
                    tool["inputSchema"]["additionalProperties"],
                    !name.starts_with("task.dependencies.add")
                        && !name.starts_with("task.dependencies.remove")
                );
                if name.starts_with("task.dependencies") {
                    assert_eq!(
                        tool["inputSchema"]["properties"]["task_id"]["type"],
                        "string"
                    );
                }
            }
        }
    }

    #[test]
    fn connected_dependency_commands_refuse_absent_tasks_before_queueing() {
        let mut server = offline();
        for name in ["task.dependencies.add", "task.dependencies.remove"] {
            let args = json!({"command_id":format!("{name}-1"),"expected_revision":0,
                "task_id":"a","prerequisite_id":"b"});
            assert!(server.dispatch(name, &args).is_err());
        }
        assert_eq!(
            server.dispatch("authority.status", &json!({})).unwrap()["unresolved_count"],
            0
        );
    }

    #[test]
    fn connected_dependency_commands_are_write_protected() {
        let mut server = offline();
        server.policy.mode = Mode::ReadOnly;
        let args = json!({"command_id":"dep-readonly","expected_revision":0,
            "task_id":"a","prerequisite_id":"b"});
        for name in ["task.dependencies.add", "task.dependencies.remove"] {
            assert!(
                server
                    .dispatch(name, &args)
                    .unwrap_err()
                    .contains("read-only")
            );
        }
        assert_eq!(
            server.dispatch("authority.status", &json!({})).unwrap()["unresolved_count"],
            0
        );
    }

    #[test]
    fn connected_graph_reads_require_unrestricted_visibility() {
        let mut server = offline();
        server.policy.projects.insert("some-project".to_owned());
        for name in ["task.graph", "task.next"] {
            assert!(
                server
                    .dispatch(name, &json!({"now_ms":1000}))
                    .unwrap_err()
                    .contains("unrestricted")
            );
        }
        server.policy.projects.clear();
        server.policy.allow_unscoped_execution = false;
        for name in ["task.graph", "task.next"] {
            assert!(server.dispatch(name, &json!({"now_ms":1000})).is_err());
        }
    }

    #[test]
    fn connected_graph_reads_do_not_promote_pending_tasks() {
        let mut server = offline();
        let create = json!({"command_id":"dep-task","expected_revision":0,
            "action":{"type":"create","task_id":"new-task","title":"Queued","project_id":null}});
        assert_eq!(
            server.dispatch("task.command", &create).unwrap()["disposition"],
            "pending"
        );
        for name in ["task.graph", "task.next"] {
            let err = server.dispatch(name, &json!({"now_ms":1000})).unwrap_err();
            assert!(
                err.contains("accepted") || err.contains("snapshot"),
                "{err}"
            );
        }
        assert_eq!(
            server.dispatch("authority.status", &json!({})).unwrap()["unresolved_count"],
            1
        );
    }

    #[test]
    fn connected_mcp_tools_use_canonical_command_envelopes() {
        let items = connected_tool_list();
        for name in [
            "schedule.command",
            "routine.command",
            "milestone.command",
            "strategy.command",
            "authority.flush",
            "authority.refresh",
            "authority.unresolved",
        ] {
            assert!(
                items.as_array().unwrap().iter().any(|v| v["name"] == name),
                "missing {name}"
            );
        }
    }
    #[test]
    fn connected_project_task_and_evidence_queue_without_claiming_commit() {
        let mut server = offline();
        let project = json!({"command_id":"project-1","expected_revision":0,
            "project_id":"proj","name":"Project"});
        let project_receipt = server.dispatch("project.command", &project).unwrap();
        assert_eq!(project_receipt["disposition"], "pending");
        // Pending projects cannot be read from the authoritative projection.
        assert_eq!(
            server.dispatch("projects.list", &json!({})).unwrap(),
            json!([])
        );
        let task = json!({"command_id":"task-1","expected_revision":0,
            "action":{"type":"create","task_id":"task","title":"Task","project_id":"proj"}});
        assert_eq!(
            server.dispatch("task.command", &task).unwrap()["disposition"],
            "pending"
        );
        let evidence = json!({"command_id":"habit-1","expected_revision":0,
            "action":{"type":"create_habit","habit_id":"h1","name":"Daily","cadence":"daily"}});
        assert_eq!(
            server.dispatch("evidence.command", &evidence).unwrap()["disposition"],
            "pending"
        );
        assert_eq!(
            server.dispatch("tasks.list", &json!({})).unwrap(),
            json!([])
        );
        assert_eq!(
            server.dispatch("habits.list", &json!({})).unwrap(),
            json!([])
        );
        assert_eq!(
            server.dispatch("authority.status", &json!({})).unwrap()["unresolved_count"],
            3
        );
        assert_eq!(
            server.dispatch("project.command", &project).unwrap()["disposition"],
            "pending"
        );
        assert_eq!(
            server.dispatch("authority.status", &json!({})).unwrap()["unresolved_count"],
            3
        );
    }
    #[test]
    fn scoped_mcp_checks_both_ends_of_task_mutations() {
        let mut server = offline();
        server.policy.projects.insert("visible".into());
        let denied = json!({"command_id":"other-project","expected_revision":0,
            "project_id":"hidden","name":"Private"});
        assert!(server.dispatch("project.command", &denied).is_err());
        let denied_task = json!({"command_id":"other-task","expected_revision":0,
            "action":{"type":"create","task_id":"t","title":"Private","project_id":"hidden"}});
        assert!(server.dispatch("task.command", &denied_task).is_err());
        let unassigned = json!({"command_id":"unassigned","expected_revision":0,
            "action":{"type":"create","task_id":"t2","title":"Inbox","project_id":null}});
        server.policy.allow_unscoped_execution = false;
        assert!(server.dispatch("task.command", &unassigned).is_err());
        // Existing mutations require a visible accepted record: a pending
        // user assertion about a task ID cannot bypass project authorization.
        let rename = json!({"command_id":"rename","expected_revision":0,
            "action":{"type":"rename","task_id":"missing","title":"Changed"}});
        assert!(server.dispatch("task.command", &rename).is_err());
        assert_eq!(
            server.dispatch("authority.status", &json!({})).unwrap()["unresolved_count"],
            0
        );
    }
    #[test]
    fn evidence_requires_explicit_unscoped_authorization() {
        let mut server = offline();
        server.policy.allow_unscoped_execution = false;
        let command = json!({"command_id":"evidence-denied","expected_revision":0,
            "action":{"type":"record_metric","metric_id":"metric","observed_at_ms":100,"value":15}});
        assert!(
            server
                .dispatch("evidence.command", &command)
                .unwrap_err()
                .contains("unscoped")
        );
        assert_eq!(
            server.dispatch("habits.list", &json!({})).unwrap(),
            json!([])
        );
        assert_eq!(
            server.dispatch("authority.status", &json!({})).unwrap()["unresolved_count"],
            0
        );
    }
    #[test]
    fn connected_mcp_refuses_legacy_local_mutations() {
        let mut server = offline();
        let err = server
            .dispatch("task.create", &json!({"title":"danger"}))
            .unwrap_err();
        assert!(err.contains("not supported"));
    }
}

#[cfg(test)]
mod connected_workflow_m20_tests {
    use super::*;

    pub(crate) fn connected() -> ConnectedMcp {
        ConnectedMcp {
            client: AuthorityConnection::remote(":memory:", "https://example.invalid", None)
                .unwrap(),
            policy: AgentPolicy {
                agent_id: "workflow-test".into(),
                session_id: "unit".into(),
                mode: Mode::Developer,
                projects: BTreeSet::new(),
                allow_unscoped_execution: true,
                allow_unscoped_knowledge: false,
            },
        }
    }
    fn project(id: &str) -> Value {
        json!({"family":"project","command":{"command_id":id,"expected_revision":0,
            "project_id":"project","name":"My Project"}})
    }
    fn routine(id: &str) -> Value {
        json!({"family":"routine","command":{"command_id":id,"expected_revision":0,
            "action":{"type":"create","routine_id":"morning","name":"Morning","habit_ids":[]}}})
    }
    #[test]
    fn authorized_cross_domain_workflow_queues_ordered_intents_without_accepting() {
        let mut server = connected();
        let result = server
            .dispatch(
                "authority.workflow",
                &json!({"steps":[project("p1"),routine("r1")]}),
            )
            .unwrap();
        assert_eq!(result["failed_index"], Value::Null);
        assert_eq!(result["completed"].as_array().unwrap().len(), 2);
        assert_eq!(result["completed"][0]["disposition"], "pending");
        assert_eq!(
            server.dispatch("projects.list", &json!({})).unwrap(),
            json!([])
        );
        let queued = server.dispatch("authority.unresolved", &json!({})).unwrap();
        assert_eq!(queued[0]["kind"], "project");
        assert_eq!(queued[1]["kind"], "routine");
    }
    #[test]
    fn duplicate_ids_fail_before_any_durable_side_effect() {
        let mut server = connected();
        assert!(
            server
                .dispatch(
                    "authority.workflow",
                    &json!({"steps":[project("same"),routine("same")]})
                )
                .is_err()
        );
        assert_eq!(
            server.dispatch("authority.status", &json!({})).unwrap()["unresolved_count"],
            0
        );
    }
    #[test]
    fn malformed_final_step_does_not_queue_valid_prefix() {
        let mut server = connected();
        assert!(
            server
                .dispatch(
                    "authority.workflow",
                    &json!({"steps":[project("p"),
            {"family":"routine","command":{"command_id":"bad","action":{"type":"invalid"}}}]})
                )
                .is_err()
        );
        assert_eq!(
            server.dispatch("authority.status", &json!({})).unwrap()["unresolved_count"],
            0
        );
    }
    #[test]
    fn entire_batch_is_authorized_before_submission() {
        let mut server = connected();
        server.policy.allow_unscoped_execution = false;
        assert!(
            server
                .dispatch(
                    "authority.workflow",
                    &json!({"steps":[project("p"),routine("r")]})
                )
                .is_err()
        );
        assert_eq!(
            server.dispatch("authority.status", &json!({})).unwrap()["unresolved_count"],
            0
        );
    }
    #[test]
    fn scoped_agent_cannot_batch_create_outside_allowlist() {
        let mut server = connected();
        server.policy.projects.insert("visible".into());
        assert!(
            server
                .dispatch("authority.workflow", &json!({"steps":[project("p")]}))
                .is_err()
        );
        assert_eq!(
            server.dispatch("authority.status", &json!({})).unwrap()["unresolved_count"],
            0
        );
    }
    #[test]
    fn readonly_agent_cannot_queue_cross_domain_workflow() {
        let mut server = connected();
        server.policy.mode = Mode::ReadOnly;
        assert!(
            server
                .dispatch("authority.workflow", &json!({"steps":[project("p")]}))
                .is_err()
        );
    }
    #[test]
    fn conflict_replacement_rejects_unknown_and_pending_originals() {
        let mut server = connected();
        let replacement = json!({"rejected_id":"p","replacement_id":"p2","family":"project",
            "action":{"project_id":"project","name":"Renamed"}});
        assert!(server.dispatch("authority.resolve", &replacement).is_err());
        server
            .dispatch("project.command", &project("p")["command"])
            .unwrap();
        assert!(server.dispatch("authority.resolve", &replacement).is_err());
        assert_eq!(
            server.dispatch("authority.status", &json!({})).unwrap()["unresolved_count"],
            1
        );
    }
    #[test]
    fn replacement_scope_is_checked_before_touching_outbox() {
        let mut server = connected();
        server.policy.projects.insert("visible".into());
        let replacement = json!({"rejected_id":"old","replacement_id":"new","family":"project",
            "action":{"project_id":"hidden","name":"Bad"}});
        assert!(
            server
                .dispatch("authority.resolve", &replacement)
                .unwrap_err()
                .contains("review")
        );
    }
    #[test]
    fn connected_workflow_tools_are_discoverable() {
        let tools = connected_tool_list();
        for expected in ["authority.workflow", "authority.resolve"] {
            assert!(
                tools
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|t| t["name"] == expected)
            );
        }
    }
}

#[cfg(test)]
mod reviewed_batch_mcp_tests {
    use super::connected_workflow_m20_tests::connected;
    use super::*;
    fn review(id: &str, kind: &str) -> Value {
        json!({"rejected_id":id,"kind":kind,"rejected_action":{
            "type":"create","task_id":"original","title":"Original"},
            "ordinal":1,"accepted_revision":3,"history_fingerprint":"a".repeat(64)})
    }
    fn task_resolution() -> Value {
        json!({"review":review("old","task"),"replacement_id":"new",
            "family":"task","action":{"type":"create","task_id":"new-task","title":"New"}})
    }
    #[test]
    fn reviewed_batch_mcp_tool_is_discoverable() {
        let tools = connected_tool_list();
        assert!(
            tools
                .as_array()
                .unwrap()
                .iter()
                .any(|entry| entry["name"] == "authority.resolve_reviewed_batch")
        );
    }
    #[test]
    fn empty_reviewed_batch_cannot_be_submitted() {
        let mut server = connected();
        assert!(
            server
                .dispatch(
                    "authority.resolve_reviewed_batch",
                    &json!({"resolutions":[]})
                )
                .is_err()
        );
        assert_eq!(
            server.dispatch("authority.status", &json!({})).unwrap()["unresolved_count"],
            0
        );
    }
    #[test]
    fn missing_batch_payload_fails_without_queueing() {
        let mut server = connected();
        assert!(
            server
                .dispatch("authority.resolve_reviewed_batch", &json!({}))
                .is_err()
        );
        assert_eq!(
            server.dispatch("authority.status", &json!({})).unwrap()["unresolved_count"],
            0
        );
    }
    #[test]
    fn readonly_agent_cannot_submit_reviewed_batch() {
        let mut server = connected();
        server.policy.mode = Mode::ReadOnly;
        let batch = json!({"resolutions":[task_resolution()]});
        assert!(
            server
                .dispatch("authority.resolve_reviewed_batch", &batch)
                .is_err()
        );
        assert_eq!(
            server.dispatch("authority.status", &json!({})).unwrap()["unresolved_count"],
            0
        );
    }
    #[test]
    fn scoped_agent_cannot_submit_unscoped_evidence_resolution() {
        let mut server = connected();
        server.policy.allow_unscoped_execution = false;
        let decision = json!({"review":review("old","evidence"),"replacement_id":"new",
            "family":"evidence","action":{"type":"create_habit","habit_id":"h","name":"Daily"}});
        let batch = json!({"resolutions":[decision]});
        assert!(
            server
                .dispatch("authority.resolve_reviewed_batch", &batch)
                .unwrap_err()
                .contains("unscoped")
        );
        assert_eq!(
            server.dispatch("authority.status", &json!({})).unwrap()["unresolved_count"],
            0
        );
    }
    #[test]
    fn scoped_agent_cannot_replace_project_outside_allowlist() {
        let mut server = connected();
        server.policy.projects.insert("allowed".into());
        let decision = json!({"review":review("old","project"),"replacement_id":"new",
            "family":"project","action":{"project_id":"hidden","name":"Secret"}});
        let batch = json!({"resolutions":[decision]});
        assert!(
            server
                .dispatch("authority.resolve_reviewed_batch", &batch)
                .unwrap_err()
                .contains("scope")
        );
        assert_eq!(
            server.dispatch("authority.status", &json!({})).unwrap()["unresolved_count"],
            0
        );
    }
    #[test]
    fn unknown_command_family_is_denied_before_local_staging() {
        let mut server = connected();
        let decision = json!({"review":review("old","other"),"replacement_id":"new",
            "family":"other","action":{"type":"create"}});
        let batch = json!({"resolutions":[decision]});
        assert!(
            server
                .dispatch("authority.resolve_reviewed_batch", &batch)
                .unwrap_err()
                .contains("unsupported")
        );
        assert_eq!(
            server.dispatch("authority.status", &json!({})).unwrap()["unresolved_count"],
            0
        );
    }
    #[test]
    fn unsafe_legacy_alias_requires_the_full_review_token() {
        let mut server = connected();
        let unsafe_args = json!({"rejected_id":"old","replacement_id":"new",
            "family":"task","action":{"type":"create","task_id":"t","title":"T"}});
        assert!(
            server
                .dispatch("authority.resolve", &unsafe_args)
                .unwrap_err()
                .contains("review")
        );
        assert_eq!(
            server.dispatch("authority.status", &json!({})).unwrap()["unresolved_count"],
            0
        );
    }
    #[test]
    fn one_bad_late_resolution_prevents_all_mutations() {
        let mut server = connected();
        let bad = json!({"review":review("bad","unknown"),"replacement_id":"new-bad",
            "family":"unknown","action":{"type":"create"}});
        assert!(
            server
                .dispatch(
                    "authority.resolve_reviewed_batch",
                    &json!({
            "resolutions":[task_resolution(),bad]})
                )
                .is_err()
        );
        assert_eq!(
            server.dispatch("authority.status", &json!({})).unwrap()["unresolved_count"],
            0
        );
    }
    #[test]
    fn malformed_review_token_is_rejected_before_network() {
        let mut server = connected();
        let decision = json!({"review":{"accepted_revision":0},"replacement_id":"new",
            "family":"task","action":{"type":"create","task_id":"t","title":"T"}});
        assert!(
            server
                .dispatch(
                    "authority.resolve_reviewed_batch",
                    &json!({"resolutions":[decision]})
                )
                .is_err()
        );
        assert_eq!(
            server.dispatch("authority.status", &json!({})).unwrap()["unresolved_count"],
            0
        );
    }
}

#[cfg(test)]
mod planning_scenario_mcp_contract_tests {
    use super::*;
    use orbit_runtime::domain_commands::{TaskAction, TaskCommand};

    fn policy(mode: Mode, allow: bool) -> AgentPolicy {
        AgentPolicy {
            agent_id: "planner-agent".into(),
            session_id: "session".into(),
            mode,
            projects: BTreeSet::new(),
            allow_unscoped_execution: allow,
            allow_unscoped_knowledge: false,
        }
    }
    fn local(mode: Mode, allow: bool) -> Server {
        let mut authority = AuthoritativeHost::open(":memory:").unwrap();
        authority
            .task_command(
                "setup",
                TaskCommand {
                    command_id: "plan-create".into(),
                    expected_revision: 0,
                    action: TaskAction::Create {
                        task_id: "plan-task".into(),
                        title: "Plan work".into(),
                        project_id: None,
                    },
                },
            )
            .unwrap();
        authority
            .task_command(
                "setup",
                TaskCommand {
                    command_id: "plan-estimate".into(),
                    expected_revision: 1,
                    action: TaskAction::SetPlanning {
                        task_id: "plan-task".into(),
                        estimated_minutes: Some(30),
                        earliest_start_at_ms: None,
                        deadline_at_ms: None,
                    },
                },
            )
            .unwrap();
        authority
            .task_command(
                "setup",
                TaskCommand {
                    command_id: "plan-ready".into(),
                    expected_revision: 2,
                    action: TaskAction::ChangeStatus {
                        task_id: "plan-task".into(),
                        status: "ready".into(),
                    },
                },
            )
            .unwrap();
        Server {
            authority,
            policy: policy(mode, allow),
        }
    }
    fn scenario() -> Value {
        json!({"comparison":{"expected_revision":3,"baseline":{
            "now_ms":0,"available_minutes":60,"max_tasks":1,
            "project_id":null,"include_unassigned":true,"include_unsized":false,
            "default_minutes":null},"alternatives":[]},
            "scenario_name":"baseline","starts_at_ms":0,"ends_at_ms":3_600_000,
            "command_id":"mcp-reviewed-plan","identity_prefix":"mcp-reviewed-plan"})
    }
    #[test]
    fn read_only_review_has_no_ledger_side_effect() {
        let mut server = local(Mode::Developer, true);
        let result = server
            .dispatch("planning.review_scenario", &json!({"request":scenario()}))
            .unwrap();
        assert_eq!(result["read_only"], true);
        assert_eq!(result["review"]["scheduled_tasks"][0], "plan-task");
        assert_eq!(server.authority.snapshot().unwrap().0, 3);
        assert!(
            server
                .authority
                .application_queries()
                .schedule_between(0, 3_600_000)
                .unwrap()
                .is_empty()
        );
    }
    #[test]
    fn approval_requires_separate_confirmation() {
        let mut server = local(Mode::Developer, true);
        let result = server
            .dispatch("planning.review_scenario", &json!({"request":scenario()}))
            .unwrap();
        let plan = result["plan"].clone();
        let review = result["review"].clone();
        let payload = json!({"request":{"plan":plan,"approval":{"review":review,"approved":true}}});
        assert!(
            server
                .dispatch("planning.apply_approved_scenario", &payload)
                .is_err()
        );
        assert_eq!(server.authority.snapshot().unwrap().0, 3);
    }
    #[test]
    fn approval_commits_and_idempotently_retries() {
        let mut server = local(Mode::Developer, true);
        let review = server
            .dispatch("planning.review_scenario", &json!({"request":scenario()}))
            .unwrap();
        let payload = json!({"request":{"plan":review["plan"],
            "approval":{"review":review["review"],"approved":true}},"confirm":true});
        let first = server
            .dispatch("planning.apply_approved_scenario", &payload)
            .unwrap();
        assert_eq!(first["accepted_revision"], 4);
        assert_eq!(first["replayed"], false);
        let replay = server
            .dispatch("planning.apply_approved_scenario", &payload)
            .unwrap();
        assert_eq!(replay["accepted_revision"], 4);
        assert_eq!(replay["replayed"], true);
        assert_eq!(
            server
                .authority
                .application_queries()
                .schedule_between(0, 3_600_000)
                .unwrap()
                .len(),
            1
        );
    }
    #[test]
    fn rejected_approval_does_not_mutate_calendar() {
        let mut server = local(Mode::Developer, true);
        let review = server
            .dispatch("planning.review_scenario", &json!({"request":scenario()}))
            .unwrap();
        let payload = json!({"request":{"plan":review["plan"],
            "approval":{"review":review["review"],"approved":false}},"confirm":true});
        assert!(
            server
                .dispatch("planning.apply_approved_scenario", &payload)
                .is_err()
        );
        assert!(
            server
                .authority
                .application_queries()
                .schedule_between(0, 3_600_000)
                .unwrap()
                .is_empty()
        );
    }
    #[test]
    fn readonly_agents_cannot_apply_reviewed_schedules() {
        let mut server = local(Mode::ReadOnly, true);
        let review = server
            .dispatch("planning.review_scenario", &json!({"request":scenario()}))
            .unwrap();
        let payload = json!({"request":{"plan":review["plan"],
            "approval":{"review":review["review"],"approved":true}},"confirm":true});
        assert!(
            server
                .dispatch("planning.apply_approved_scenario", &payload)
                .is_err()
        );
        assert_eq!(server.authority.snapshot().unwrap().0, 3);
    }
    #[test]
    fn restricted_agents_cannot_see_full_calendar() {
        let mut server = local(Mode::Developer, false);
        assert!(
            server
                .dispatch("planning.review_scenario", &json!({"request":scenario()}))
                .is_err()
        );
        assert!(
            server
                .dispatch(
                    "planning.apply_approved_scenario",
                    &json!({"confirm":true,"request":{}})
                )
                .is_err()
        );
        server.policy.allow_unscoped_execution = true;
        server.policy.projects.insert("private-project".into());
        assert!(
            server
                .dispatch("planning.review_scenario", &json!({"request":scenario()}))
                .is_err()
        );
    }
    #[test]
    fn tampered_approval_is_rejected_without_partial_reservations() {
        let mut server = local(Mode::Developer, true);
        let review = server
            .dispatch("planning.review_scenario", &json!({"request":scenario()}))
            .unwrap();
        let mut payload = json!({"request":{"plan":review["plan"],
            "approval":{"review":review["review"],"approved":true}},"confirm":true});
        payload["request"]["plan"]["items"][0]["starts_at_ms"] = json!(60_000);
        assert!(
            server
                .dispatch("planning.apply_approved_scenario", &payload)
                .is_err()
        );
        assert_eq!(server.authority.snapshot().unwrap().0, 3);
    }
    #[test]
    fn stale_review_cannot_be_approved_after_concurrent_write() {
        let mut server = local(Mode::Developer, true);
        let review = server
            .dispatch("planning.review_scenario", &json!({"request":scenario()}))
            .unwrap();
        server
            .authority
            .task_command(
                "device-b",
                TaskCommand {
                    command_id: "external-edit".into(),
                    expected_revision: 3,
                    action: TaskAction::Create {
                        task_id: "external".into(),
                        title: "Other".into(),
                        project_id: None,
                    },
                },
            )
            .unwrap();
        let payload = json!({"request":{"plan":review["plan"],
            "approval":{"review":review["review"],"approved":true}},"confirm":true});
        assert!(
            server
                .dispatch("planning.apply_approved_scenario", &payload)
                .is_err()
        );
        assert!(
            server
                .authority
                .application_queries()
                .schedule_between(0, 3_600_000)
                .unwrap()
                .is_empty()
        );
    }
    #[test]
    fn disconnected_connected_mode_cannot_approve_calendar_writes() {
        let mut server = ConnectedMcp {
            client: AuthorityConnection::remote(":memory:", "https://example.invalid", None)
                .unwrap(),
            policy: policy(Mode::Developer, true),
        };
        assert!(
            server
                .dispatch("planning.review_scenario", &json!({"request":scenario()}))
                .is_err()
        );
        assert!(
            server
                .dispatch(
                    "planning.apply_approved_scenario",
                    &json!({"confirm":true,
            "request":{"plan":{},"approval":{}}})
                )
                .is_err()
        );
    }
}
