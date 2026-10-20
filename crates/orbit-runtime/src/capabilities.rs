//! Discoverable, versioned contracts for deterministic planning operations.
//!
//! This is a registry of supported business capabilities rather than transport
//! commands: adapters map the same contract onto local CLI, MCP and future UIs.
//! No authority state or external service is needed to inspect it.
use serde::Serialize;

pub const SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthorityRequirement {
    AcceptedRead,
    Mutation,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Scope {
    Workspace,
    Project,
    Task,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Argument {
    pub name: &'static str,
    pub kind: &'static str,
    pub required: bool,
    pub description: &'static str,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Capability {
    pub id: &'static str,
    pub description: &'static str,
    pub authority: AuthorityRequirement,
    pub scope: Scope,
    pub deterministic: bool,
    pub offline_accepted_snapshot: bool,
    pub cli: &'static str,
    pub mcp: Option<&'static str>,
    pub arguments: &'static [Argument],
}

const NOW: Argument = Argument {
    name: "now_ms",
    kind: "integer",
    required: true,
    description: "Unix epoch milliseconds used for deterministic evaluation",
};
const MAX_TASKS: Argument = Argument {
    name: "max_tasks",
    kind: "integer",
    required: true,
    description: "Maximum number of tasks for this plan",
};
const LIMIT: Argument = Argument {
    name: "limit",
    kind: "integer",
    required: false,
    description: "Maximum number of returned tasks",
};
const PROJECT: Argument = Argument {
    name: "project_id",
    kind: "string",
    required: false,
    description: "Restrict planning to a project",
};
const START: Argument = Argument {
    name: "starts_at_ms",
    kind: "integer",
    required: true,
    description: "Start of planning interval in Unix milliseconds",
};
const END: Argument = Argument {
    name: "ends_at_ms",
    kind: "integer",
    required: true,
    description: "Exclusive end of planning interval in Unix milliseconds",
};
const FOCUS_LIMIT: Argument = Argument {
    name: "max_neighbors",
    kind: "integer",
    required: false,
    description: "Maximum visible related tasks (0..100)",
};
const GROUP_LIMIT: Argument = Argument {
    name: "max_tasks",
    kind: "integer",
    required: false,
    description: "Maximum group tasks and bottlenecks (0..100)",
};
const GROUP_PROJECT: Argument = Argument {
    name: "project_id",
    kind: "string",
    required: true,
    description: "Visible project to inspect",
};
const MILESTONE: Argument = Argument {
    name: "milestone_id",
    kind: "string",
    required: false,
    description: "Restrict project focus to one milestone",
};
const TASK: Argument = Argument {
    name: "task_id",
    kind: "string",
    required: true,
    description: "Stable task ID",
};
const EDIT: Argument = Argument {
    name: "edit",
    kind: "string",
    required: true,
    description: "Proposed operation: add or remove",
};
const COMMAND_ID: Argument = Argument {
    name: "command_id",
    kind: "string",
    required: true,
    description: "Stable idempotent command identifier",
};
const EXPECTED_REVISION: Argument = Argument {
    name: "expected_revision",
    kind: "integer",
    required: true,
    description: "Accepted authority revision to fence this mutation",
};
const PREREQUISITE: Argument = Argument {
    name: "prerequisite_id",
    kind: "string",
    required: true,
    description: "Stable prerequisite task ID",
};
const APPROVAL_REQUEST: Argument = Argument {
    name: "request",
    kind: "object",
    required: true,
    description: "Explicitly approved exact plan and review token",
};
const ACTION_SELECTION: Argument = Argument {
    name: "selection",
    kind: "object",
    required: true,
    description: "Previously displayed accepted action index, kind, target, and reviewed revision",
};
const CONFIRM_APPROVAL: Argument = Argument {
    name: "confirm",
    kind: "boolean",
    required: true,
    description: "Separate explicit approval confirmation",
};
const SCENARIO_REQUEST: Argument = Argument {
    name: "request",
    kind: "object",
    required: true,
    description: "Version-fenced baseline and up to eight named read-only planning alternatives",
};
const MINUTES: Argument = Argument {
    name: "available_minutes",
    kind: "integer",
    required: true,
    description: "Available time budget in minutes",
};
const HORIZON: Argument = Argument {
    name: "horizon_ms",
    kind: "integer",
    required: true,
    description: "Future workload planning horizon",
};
const DEFAULT_MINUTES: Argument = Argument {
    name: "default_minutes",
    kind: "integer",
    required: false,
    description: "Fallback estimate for unsized work",
};

/// The common planning vocabulary exposed to adapters. Stable IDs represent
/// intent, not command-line option spellings or transport implementation.
pub static PLANNING: &[Capability] = &[
    Capability {
        id: "task.dependencies.add",
        description: "Add a prerequisite with cycle rejection and revision fencing",
        authority: AuthorityRequirement::Mutation,
        scope: Scope::Task,
        deterministic: true,
        offline_accepted_snapshot: false,
        cli: "task depends-on",
        mcp: Some("task.dependencies.add"),
        arguments: &[COMMAND_ID, EXPECTED_REVISION, TASK, PREREQUISITE],
    },
    Capability {
        id: "task.dependencies.remove",
        description: "Remove a prerequisite without removing either task",
        authority: AuthorityRequirement::Mutation,
        scope: Scope::Task,
        deterministic: true,
        offline_accepted_snapshot: false,
        cli: "task unblock",
        mcp: Some("task.dependencies.remove"),
        arguments: &[COMMAND_ID, EXPECTED_REVISION, TASK, PREREQUISITE],
    },
    Capability {
        id: "task.dependencies.preview",
        description: "Preview dependency effects without writing accepted or pending state",
        authority: AuthorityRequirement::AcceptedRead,
        scope: Scope::Task,
        deterministic: true,
        offline_accepted_snapshot: true,
        cli: "task preview-dependency",
        mcp: Some("task.dependencies.preview"),
        arguments: &[TASK, PREREQUISITE, EDIT, NOW],
    },
    Capability {
        id: "task.readiness",
        description: "Explain accepted prerequisite blockers and actionable tasks",
        authority: AuthorityRequirement::AcceptedRead,
        scope: Scope::Workspace,
        deterministic: true,
        offline_accepted_snapshot: true,
        cli: "task ready",
        mcp: Some("task.readiness"),
        arguments: &[NOW],
    },
    Capability {
        id: "task.graph",
        description: "Inspect accepted dependencies and transitive downstream effects",
        authority: AuthorityRequirement::AcceptedRead,
        scope: Scope::Workspace,
        deterministic: true,
        offline_accepted_snapshot: true,
        cli: "task graph",
        mcp: Some("task.graph"),
        arguments: &[NOW],
    },
    Capability {
        id: "task.next",
        description: "Rank actionable tasks by urgency and dependency impact",
        authority: AuthorityRequirement::AcceptedRead,
        scope: Scope::Workspace,
        deterministic: true,
        offline_accepted_snapshot: true,
        cli: "task next",
        mcp: Some("task.next"),
        arguments: &[NOW, LIMIT],
    },
    Capability {
        id: "planning.apply_approved_scenario",
        description: "Atomically commit a previously reviewed scenario schedule after explicit approval",
        authority: AuthorityRequirement::Mutation,
        scope: Scope::Workspace,
        deterministic: true,
        offline_accepted_snapshot: false,
        cli: "task apply-reviewed-schedule",
        mcp: Some("planning.apply_approved_scenario"),
        arguments: &[APPROVAL_REQUEST, CONFIRM_APPROVAL],
    },
    Capability {
        id: "planning.review_scenario_options",
        description: "Compare which chosen scenarios can fit real accepted calendar windows",
        authority: AuthorityRequirement::AcceptedRead,
        scope: Scope::Workspace,
        deterministic: true,
        offline_accepted_snapshot: false,
        cli: "task review-scenario-options",
        mcp: Some("planning.review_scenario_options"),
        arguments: &[SCENARIO_REQUEST],
    },
    Capability {
        id: "planning.review_scenario",
        description: "Review concrete time slots from a selected planning scenario without writing calendar state",
        authority: AuthorityRequirement::AcceptedRead,
        scope: Scope::Workspace,
        deterministic: true,
        offline_accepted_snapshot: false,
        cli: "task review-scenario",
        mcp: Some("planning.review_scenario"),
        arguments: &[SCENARIO_REQUEST],
    },
    Capability {
        id: "planning.compare_scenarios",
        description: "Compare pinned, excluded and capacity-adjusted task plans without changing state",
        authority: AuthorityRequirement::AcceptedRead,
        scope: Scope::Project,
        deterministic: true,
        offline_accepted_snapshot: true,
        cli: "task compare-scenarios",
        mcp: Some("planning.compare_scenarios"),
        arguments: &[SCENARIO_REQUEST],
    },
    Capability {
        id: "planning.portfolio_capacity",
        description: "Select cross-project next actions within a time budget without creating reservations",
        authority: AuthorityRequirement::AcceptedRead,
        scope: Scope::Project,
        deterministic: true,
        offline_accepted_snapshot: true,
        cli: "task portfolio-capacity",
        mcp: Some("planning.portfolio_capacity"),
        arguments: &[NOW, MINUTES, MAX_TASKS, PROJECT, DEFAULT_MINUTES],
    },
    Capability {
        id: "planning.portfolio_actions",
        description: "Rank accepted actionable work by downstream impact across projects without leaking scoped foreign identifiers",
        authority: AuthorityRequirement::AcceptedRead,
        scope: Scope::Project,
        deterministic: true,
        offline_accepted_snapshot: true,
        cli: "task portfolio-actions",
        mcp: Some("planning.portfolio_actions"),
        arguments: &[NOW, PROJECT, LIMIT],
    },
    Capability {
        id: "planning.day",
        description: "Propose tasks fitting a time budget without reservations",
        authority: AuthorityRequirement::AcceptedRead,
        scope: Scope::Workspace,
        deterministic: true,
        offline_accepted_snapshot: true,
        cli: "task day-plan",
        mcp: Some("planning.day"),
        arguments: &[NOW, MINUTES, MAX_TASKS, PROJECT, DEFAULT_MINUTES],
    },
    Capability {
        id: "planning.schedule",
        description: "Fit tasks into available time slots around existing commitments",
        authority: AuthorityRequirement::AcceptedRead,
        scope: Scope::Workspace,
        deterministic: true,
        offline_accepted_snapshot: true,
        cli: "task schedule-plan",
        mcp: Some("planning.schedule"),
        arguments: &[START, END, MAX_TASKS, PROJECT, DEFAULT_MINUTES],
    },
    Capability {
        id: "planning.workload",
        description: "Explain workload and dependency bottlenecks by project",
        authority: AuthorityRequirement::AcceptedRead,
        scope: Scope::Workspace,
        deterministic: true,
        offline_accepted_snapshot: true,
        cli: "task workload",
        mcp: Some("planning.workload"),
        arguments: &[NOW, HORIZON],
    },
    Capability {
        id: "planning.handoff_task",
        description: "Revalidate a reviewed action and return an accepted task/project context or a typed, read-only next-step preview",
        authority: AuthorityRequirement::AcceptedRead,
        scope: Scope::Task,
        deterministic: true,
        offline_accepted_snapshot: true,
        cli: "task handoff-planning-action",
        mcp: Some("planning.handoff_task"),
        arguments: &[TASK, NOW, END, LIMIT, PROJECT, ACTION_SELECTION],
    },
    Capability {
        id: "planning.handoff_group",
        description: "Revalidate a reviewed group action and open its scope-safe context or preview",
        authority: AuthorityRequirement::AcceptedRead,
        scope: Scope::Project,
        deterministic: true,
        offline_accepted_snapshot: true,
        cli: "task handoff-planning-group-action",
        mcp: Some("planning.handoff_group"),
        arguments: &[GROUP_PROJECT, MILESTONE, NOW, END, LIMIT, ACTION_SELECTION],
    },
    Capability {
        id: "planning.resolve_task_action",
        description: "Revalidate a reviewed task action and return a non-executable navigation or preview destination",
        authority: AuthorityRequirement::AcceptedRead,
        scope: Scope::Task,
        deterministic: true,
        offline_accepted_snapshot: true,
        cli: "task resolve-planning-action",
        mcp: Some("planning.resolve_task_action"),
        arguments: &[TASK, NOW, END, LIMIT, PROJECT, ACTION_SELECTION],
    },
    Capability {
        id: "planning.resolve_group_action",
        description: "Revalidate a reviewed project action without executing a command",
        authority: AuthorityRequirement::AcceptedRead,
        scope: Scope::Project,
        deterministic: true,
        offline_accepted_snapshot: true,
        cli: "task resolve-planning-group-action",
        mcp: Some("planning.resolve_group_action"),
        arguments: &[GROUP_PROJECT, MILESTONE, NOW, END, LIMIT, ACTION_SELECTION],
    },
    Capability {
        id: "planning.task_actions",
        description: "Discover safe visible task actions without submitting commands",
        authority: AuthorityRequirement::AcceptedRead,
        scope: Scope::Task,
        deterministic: true,
        offline_accepted_snapshot: true,
        cli: "task planning-actions",
        mcp: Some("planning.task_actions"),
        arguments: &[TASK, NOW, END, LIMIT, PROJECT],
    },
    Capability {
        id: "planning.group_actions",
        description: "Discover project and milestone next-step previews from accepted work",
        authority: AuthorityRequirement::AcceptedRead,
        scope: Scope::Project,
        deterministic: true,
        offline_accepted_snapshot: true,
        cli: "task planning-group-actions",
        mcp: Some("planning.group_actions"),
        arguments: &[GROUP_PROJECT, MILESTONE, NOW, END, LIMIT],
    },
    Capability {
        id: "planning.task_focus",
        description: "Navigate accepted task prerequisites, dependents, project and milestone peers",
        authority: AuthorityRequirement::AcceptedRead,
        scope: Scope::Workspace,
        deterministic: true,
        offline_accepted_snapshot: true,
        cli: "task planning-focus",
        mcp: Some("planning.task_focus"),
        arguments: &[TASK, NOW, END, LIMIT, PROJECT, FOCUS_LIMIT],
    },
    Capability {
        id: "planning.group_focus",
        description: "Inspect accepted project/milestone task groups and visible blockers",
        authority: AuthorityRequirement::AcceptedRead,
        scope: Scope::Workspace,
        deterministic: true,
        offline_accepted_snapshot: true,
        cli: "task planning-group",
        mcp: Some("planning.group_focus"),
        arguments: &[GROUP_PROJECT, MILESTONE, NOW, END, LIMIT, GROUP_LIMIT],
    },
    Capability {
        id: "planning.work_brief",
        description: "Explain Today/Next scheduling, recommendations and exclusions",
        authority: AuthorityRequirement::AcceptedRead,
        scope: Scope::Workspace,
        deterministic: true,
        offline_accepted_snapshot: true,
        cli: "task work-brief",
        mcp: Some("planning.work_brief"),
        arguments: &[NOW, END, LIMIT, PROJECT, DEFAULT_MINUTES],
    },
    Capability {
        id: "planning.completion_impact",
        description: "Preview what completing one accepted task would unblock",
        authority: AuthorityRequirement::AcceptedRead,
        scope: Scope::Task,
        deterministic: true,
        offline_accepted_snapshot: true,
        cli: "task completion-impact",
        mcp: Some("planning.completion_impact"),
        arguments: &[TASK, NOW, LIMIT],
    },
];

#[derive(Debug, Clone, Serialize)]
pub struct Registry<'a> {
    pub schema_version: u32,
    pub capabilities: Vec<&'a Capability>,
}

/// Filters are exact matches and unknown IDs return an empty registry.
/// Capabilities are emitted in stable, documented declaration order.
pub fn discover(id: Option<&str>, scope: Option<Scope>, accepted_only: bool) -> Registry<'static> {
    Registry {
        schema_version: SCHEMA_VERSION,
        capabilities: PLANNING
            .iter()
            .filter(|cap| {
                id.is_none_or(|key| key == cap.id)
                    && scope.is_none_or(|scope| scope == cap.scope)
                    && (!accepted_only || cap.authority == AuthorityRequirement::AcceptedRead)
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn capabilities_have_unique_stable_ids_and_cli_paths() {
        let mut ids = HashSet::new();
        let mut cli = HashSet::new();
        for cap in PLANNING {
            assert!(ids.insert(cap.id), "duplicate capability {}", cap.id);
            assert!(cli.insert(cap.cli), "duplicate CLI path {}", cap.cli);
            assert!(cap.deterministic);
            assert!(!cap.description.is_empty());
            assert!(!cap.cli.is_empty());
            let mut arg_names = HashSet::new();
            for arg in cap.arguments {
                assert!(
                    arg_names.insert(arg.name),
                    "duplicate {} argument {}",
                    cap.id,
                    arg.name
                );
                assert!(matches!(
                    arg.kind,
                    "integer" | "string" | "boolean" | "object"
                ));
                assert!(!arg.description.is_empty());
            }
            if cap.scope == Scope::Task {
                assert!(cap.arguments.iter().any(|arg| arg.name == "task_id"));
            }
        }
    }

    #[test]
    fn mutation_never_claims_offline_accepted_snapshot() {
        for cap in PLANNING {
            if cap.authority == AuthorityRequirement::Mutation {
                assert!(!cap.offline_accepted_snapshot);
            }
        }
    }

    #[test]
    fn exact_discovery_and_accepted_filters_are_stable() {
        assert_eq!(
            discover(Some("task.graph"), None, false).capabilities.len(),
            1
        );
        assert!(
            discover(Some("missing"), None, false)
                .capabilities
                .is_empty()
        );
        assert!(
            discover(None, None, true)
                .capabilities
                .iter()
                .all(|cap| cap.authority == AuthorityRequirement::AcceptedRead)
        );
        let task_scoped: Vec<&str> = discover(None, Some(Scope::Task), false)
            .capabilities
            .iter()
            .map(|cap| cap.id)
            .collect();
        assert_eq!(
            task_scoped,
            vec![
                "task.dependencies.add",
                "task.dependencies.remove",
                "task.dependencies.preview",
                "planning.handoff_task",
                "planning.resolve_task_action",
                "planning.task_actions",
                "planning.completion_impact",
            ]
        );
    }

    #[test]
    fn machine_schema_remains_versioned() {
        let json = serde_json::to_value(discover(None, None, false)).unwrap();
        assert_eq!(json["schema_version"], 1);
        assert!(json["capabilities"].as_array().unwrap().len() >= 10);
    }
}

/// JSON Schema projection used by MCP clients. The same argument metadata is
/// returned to command-line discovery, keeping descriptions and types aligned.
pub fn input_schema(cap: &Capability) -> serde_json::Value {
    use serde_json::{Map, Value, json};
    let mut properties = Map::new();
    let mut required = Vec::new();
    for argument in cap.arguments {
        properties.insert(
            argument.name.to_string(),
            json!({
                "type":argument.kind,
                "description":argument.description,
            }),
        );
        if argument.required {
            required.push(argument.name);
        }
    }
    let mut schema = json!({"type":"object","properties":Value::Object(properties),
        "required":required,"additionalProperties":cap.authority!=AuthorityRequirement::Mutation});
    if cap.authority == AuthorityRequirement::AcceptedRead {
        schema["description"] = json!(
            "Read-only; operates on accepted state and does not apply pending connected-client intentions"
        );
    }
    schema
}

pub fn mcp_tool(id: &str) -> Option<serde_json::Value> {
    let capability = PLANNING.iter().find(|cap| cap.mcp == Some(id))?;
    Some(serde_json::json!({
        "name":id,"description":capability.description,
        "inputSchema":input_schema(capability),
    }))
}

#[cfg(test)]
mod schema_tests {
    use super::*;
    #[test]
    fn advertised_mcp_schema_matches_registry_arguments() {
        for capability in PLANNING.iter().filter(|cap| cap.mcp.is_some()) {
            let tool = mcp_tool(capability.mcp.unwrap()).unwrap();
            let schema = &tool["inputSchema"];
            assert_eq!(schema["type"], "object");
            for argument in capability.arguments {
                assert_eq!(schema["properties"][argument.name]["type"], argument.kind);
                assert_eq!(
                    schema["properties"][argument.name]["description"],
                    argument.description
                );
                assert_eq!(
                    schema["required"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|v| v.as_str() == Some(argument.name)),
                    argument.required
                );
            }
        }
    }
    #[test]
    fn unknown_tool_does_not_leak_schema() {
        assert!(mcp_tool("planning.nonexistent").is_none());
        assert!(mcp_tool("task.dependencies.add").is_some());
        assert!(mcp_tool("task.dependencies.remove").is_some());
    }
}

/// Stable, machine-readable argument failures shared by all MCP adapters.
/// Unlike a serde failure, this includes the exact public capability and
/// offending field so agents can correct calls without inspecting internals.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ArgumentViolation {
    pub code: &'static str,
    pub capability: String,
    pub argument: String,
    pub message: String,
}

impl std::fmt::Display for ArgumentViolation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}: {}.{}: {}",
            self.code, self.capability, self.argument, self.message
        )
    }
}

/// Validate the advertised shape before invoking any planning handler.
/// The same contract applies to local and connected MCP transports. Strict
/// top-level types and explicit bounds prevent `as_u64().unwrap_or(default)`
/// from silently turning invalid agent requests into successful queries.
///
/// Unknown arguments are deliberately tolerated for compatible read clients;
/// mutations reject them because an ignored mutation intent is dangerous.
/// Unknown tools return `Ok(())` so non-planning MCP tools keep their existing
/// independent schema and authorization logic.
pub fn validate_call(id: &str, args: &serde_json::Value) -> Result<(), ArgumentViolation> {
    let Some(cap) = PLANNING.iter().find(|cap| cap.mcp == Some(id)) else {
        return Ok(());
    };
    let fail = |code, argument: &str, message: &str| ArgumentViolation {
        code,
        capability: id.to_owned(),
        argument: argument.to_owned(),
        message: message.to_owned(),
    };
    let Some(fields) = args.as_object() else {
        return Err(fail("invalid_arguments", "$", "expected a JSON object"));
    };
    for argument in cap.arguments {
        let value = fields.get(argument.name);
        if value.is_none() && argument.required {
            return Err(fail(
                "missing_argument",
                argument.name,
                "required argument is absent",
            ));
        }
        let Some(value) = value else { continue };
        let valid = match argument.kind {
            "string" => value.as_str().is_some_and(|text| !text.trim().is_empty()),
            "integer" => value.as_i64().is_some(),
            "boolean" => value.is_boolean(),
            "object" => value.is_object(),
            _ => false,
        };
        if !valid {
            return Err(fail(
                "invalid_argument_type",
                argument.name,
                "value does not match the advertised argument type",
            ));
        }
        if let Some(number) = value.as_i64() {
            if number < 0
                && matches!(
                    argument.name,
                    "now_ms"
                        | "starts_at_ms"
                        | "ends_at_ms"
                        | "expected_revision"
                        | "limit"
                        | "max_tasks"
                        | "available_minutes"
                        | "horizon_ms"
                        | "default_minutes"
                )
            {
                return Err(fail(
                    "invalid_argument_value",
                    argument.name,
                    "value cannot be negative",
                ));
            }
            if matches!(argument.name, "limit" | "max_tasks") && number > 200 {
                return Err(fail(
                    "invalid_argument_value",
                    argument.name,
                    "maximum is 200",
                ));
            }
            if argument.name == "available_minutes" && number > 10080 {
                return Err(fail(
                    "invalid_argument_value",
                    argument.name,
                    "maximum is one week",
                ));
            }
        }
    }
    if cap.authority == AuthorityRequirement::Mutation {
        for key in fields.keys() {
            if !cap.arguments.iter().any(|arg| arg.name == key) {
                return Err(fail(
                    "unknown_mutation_argument",
                    key,
                    "unrecognized mutation argument would otherwise be ignored",
                ));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod call_validation_tests {
    use super::*;
    use serde_json::json;

    fn assert_error(id: &str, args: serde_json::Value, code: &str, field: &str) {
        let err = validate_call(id, &args).unwrap_err();
        assert_eq!(err.code, code);
        assert_eq!(err.capability, id);
        assert_eq!(err.argument, field);
        assert!(!err.message.is_empty());
        let encoded = serde_json::to_value(&err).unwrap();
        assert_eq!(encoded["code"], code);
        assert_eq!(encoded["argument"], field);
    }

    #[test]
    fn all_mutation_arguments_are_required_and_typed() {
        for id in ["task.dependencies.add", "task.dependencies.remove"] {
            assert_error(id, json!({}), "missing_argument", "command_id");
            assert_error(
                id,
                json!({"command_id":"a"}),
                "missing_argument",
                "expected_revision",
            );
            assert_error(
                id,
                json!({"command_id":"a","expected_revision":"0"}),
                "invalid_argument_type",
                "expected_revision",
            );
            assert_error(
                id,
                json!({"command_id":"a","expected_revision":0}),
                "missing_argument",
                "task_id",
            );
            assert_error(
                id,
                json!({"command_id":"a","expected_revision":0,"task_id":"t"}),
                "missing_argument",
                "prerequisite_id",
            );
            let valid =
                json!({"command_id":"a","expected_revision":0,"task_id":"t","prerequisite_id":"p"});
            assert!(validate_call(id, &valid).is_ok());
            let mut extra = valid;
            extra["unknown_override"] = json!(true);
            assert_error(id, extra, "unknown_mutation_argument", "unknown_override");
        }
    }

    #[test]
    fn invalid_read_limits_are_not_silently_replaced_by_defaults() {
        for id in ["task.next", "planning.work_brief"] {
            let base = if id == "task.next" {
                json!({"now_ms":1000,"limit":20})
            } else {
                json!({"now_ms":1000,"ends_at_ms":2000,"limit":20})
            };
            assert!(validate_call(id, &base).is_ok());
            let mut invalid = base.clone();
            invalid["limit"] = json!("many");
            assert_error(id, invalid, "invalid_argument_type", "limit");
            let mut huge = base.clone();
            huge["limit"] = json!(201);
            assert_error(id, huge, "invalid_argument_value", "limit");
            let mut negative = base;
            negative["limit"] = json!(-1);
            assert_error(id, negative, "invalid_argument_value", "limit");
        }
    }

    #[test]
    fn required_fields_do_not_accept_null_empty_strings_or_float() {
        assert_error(
            "task.graph",
            json!({"now_ms":null}),
            "invalid_argument_type",
            "now_ms",
        );
        assert_error(
            "task.graph",
            json!({"now_ms":0.5}),
            "invalid_argument_type",
            "now_ms",
        );
        assert_error(
            "task.graph",
            json!({"now_ms":-1}),
            "invalid_argument_value",
            "now_ms",
        );
        assert_error(
            "task.dependencies.add",
            json!({"command_id":"  ","expected_revision":0,
            "task_id":"a","prerequisite_id":"b"}),
            "invalid_argument_type",
            "command_id",
        );
        assert_error(
            "planning.day",
            json!({"now_ms":0,"available_minutes":10081,
            "max_tasks":1}),
            "invalid_argument_value",
            "available_minutes",
        );
    }

    #[test]
    fn unrelated_tools_keep_their_existing_validation_path() {
        assert!(validate_call("authority.refresh", &json!(null)).is_ok());
        assert!(validate_call("nonexistent.tool", &json!(null)).is_ok());
        assert_error("task.graph", json!([]), "invalid_arguments", "$");
    }

    #[test]
    fn schema_and_validator_agree_for_all_required_arguments() {
        for cap in PLANNING.iter().filter(|cap| cap.mcp.is_some()) {
            let name = cap.mcp.unwrap();
            for arg in cap.arguments.iter().filter(|arg| arg.required) {
                let mut map = serde_json::Map::new();
                for other in cap
                    .arguments
                    .iter()
                    .filter(|other| other.required && other.name != arg.name)
                {
                    map.insert(
                        other.name.to_owned(),
                        match other.kind {
                            "string" => json!("example"),
                            "integer" => json!(1),
                            "boolean" => json!(true),
                            "object" => json!({}),
                            _ => unreachable!(),
                        },
                    );
                }
                let err = validate_call(name, &serde_json::Value::Object(map)).unwrap_err();
                assert_eq!(err.argument, arg.name);
                assert_eq!(err.code, "missing_argument");
            }
        }
    }
}

#[cfg(test)]
mod advertised_contract_guards {
    use super::*;
    #[test]
    fn every_advertised_tool_has_a_matching_capability_and_unique_name() {
        let mut seen = std::collections::BTreeSet::new();
        for cap in PLANNING.iter().filter(|cap| cap.mcp.is_some()) {
            let name = cap.mcp.unwrap();
            assert_eq!(cap.id, name);
            assert!(seen.insert(name));
            let tool = mcp_tool(name).unwrap();
            assert_eq!(tool["name"], name);
            assert_eq!(tool["description"], cap.description);
            assert_eq!(tool["inputSchema"]["type"], "object");
            assert_eq!(
                tool["inputSchema"]["additionalProperties"],
                cap.authority != AuthorityRequirement::Mutation
            );
        }
    }
    #[test]
    fn read_only_dependency_preview_never_advertises_write_authority() {
        let capability = discover(Some("task.dependencies.preview"), None, true);
        assert_eq!(capability.capabilities.len(), 1);
        let preview = capability.capabilities[0];
        assert_eq!(preview.authority, AuthorityRequirement::AcceptedRead);
        assert!(preview.offline_accepted_snapshot);
        assert_eq!(preview.scope, Scope::Task);
        assert!(
            preview
                .arguments
                .iter()
                .any(|arg| arg.name == "edit" && arg.required)
        );
        for id in ["task.dependencies.add", "task.dependencies.remove"] {
            assert!(discover(Some(id), None, true).capabilities.is_empty());
            assert_eq!(discover(Some(id), None, false).capabilities.len(), 1);
        }
    }
    #[test]
    fn integer_bounds_are_preserved_through_machine_schema() {
        for id in ["task.graph", "task.next", "task.dependencies.preview"] {
            let cap = discover(Some(id), None, false);
            assert_eq!(cap.capabilities.len(), 1);
            let metadata = mcp_tool(id).unwrap();
            for arg in cap.capabilities[0].arguments {
                assert_eq!(
                    metadata["inputSchema"]["properties"][arg.name]["type"],
                    arg.kind
                );
            }
        }
    }
}

#[cfg(test)]
mod planning_focus_contract_tests {
    use super::*;
    #[test]
    fn task_and_group_focus_are_read_only_accepted_capabilities() {
        for id in ["planning.task_focus", "planning.group_focus"] {
            let cap = discover(Some(id), None, true);
            assert_eq!(cap.capabilities.len(), 1);
            let entry = cap.capabilities[0];
            assert_eq!(entry.authority, AuthorityRequirement::AcceptedRead);
            assert!(entry.deterministic && entry.offline_accepted_snapshot);
            assert_eq!(entry.scope, Scope::Workspace);
            let schema = mcp_tool(id).unwrap();
            assert_eq!(schema["name"], id);
            assert_eq!(schema["inputSchema"]["type"], "object");
        }
    }
    #[test]
    fn focus_schema_requires_accepted_planning_interval() {
        for id in ["planning.task_focus", "planning.group_focus"] {
            let schema = mcp_tool(id).unwrap();
            let required = schema["inputSchema"]["required"].as_array().unwrap();
            for field in ["now_ms", "ends_at_ms"] {
                assert!(required.iter().any(|v| v == field), "{id} missing {field}");
            }
        }
        let task = mcp_tool("planning.task_focus").unwrap();
        assert!(
            task["inputSchema"]["required"]
                .as_array()
                .unwrap()
                .iter()
                .any(|v| v == "task_id")
        );
        let group = mcp_tool("planning.group_focus").unwrap();
        assert!(
            group["inputSchema"]["required"]
                .as_array()
                .unwrap()
                .iter()
                .any(|v| v == "project_id")
        );
    }
}

#[cfg(test)]
mod planning_action_contract_tests {
    use super::*;
    #[test]
    fn action_discovery_is_read_only_and_offline_capable() {
        for id in [
            "planning.task_actions",
            "planning.group_actions",
            "planning.resolve_task_action",
            "planning.resolve_group_action",
        ] {
            let registry = discover(Some(id), None, true);
            assert_eq!(registry.capabilities.len(), 1, "{id} missing");
            let entry = registry.capabilities[0];
            assert_eq!(entry.authority, AuthorityRequirement::AcceptedRead);
            assert!(entry.offline_accepted_snapshot);
            assert!(entry.deterministic);
            assert_eq!(entry.mcp, Some(id));
        }
    }
    #[test]
    fn resolution_tools_require_structured_reviewed_selection() {
        for id in [
            "planning.resolve_task_action",
            "planning.resolve_group_action",
        ] {
            let tool = mcp_tool(id).unwrap();
            assert_eq!(
                tool["inputSchema"]["properties"]["selection"]["type"],
                "object"
            );
            assert!(
                tool["inputSchema"]["required"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|arg| arg == "selection")
            );
            assert_eq!(tool["inputSchema"]["type"], "object");
        }
    }
    #[test]
    fn task_and_project_scope_are_distinct() {
        assert_eq!(
            discover(Some("planning.task_actions"), None, true).capabilities[0].scope,
            Scope::Task
        );
        assert_eq!(
            discover(Some("planning.group_actions"), None, true).capabilities[0].scope,
            Scope::Project
        );
        assert_eq!(
            discover(Some("planning.resolve_task_action"), None, true).capabilities[0].scope,
            Scope::Task
        );
        assert_eq!(
            discover(Some("planning.resolve_group_action"), None, true).capabilities[0].scope,
            Scope::Project
        );
    }
    #[test]
    fn action_menu_inputs_do_not_contain_approval_or_command_identity() {
        for id in [
            "planning.task_actions",
            "planning.group_actions",
            "planning.resolve_task_action",
            "planning.resolve_group_action",
        ] {
            let tool = mcp_tool(id).unwrap();
            let properties = &tool["inputSchema"]["properties"];
            assert!(properties.get("command_id").is_none());
            assert!(properties.get("confirm").is_none());
            assert!(properties.get("approval").is_none());
        }
    }
}
