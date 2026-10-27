//! Local-authority blueprint management. All mutations go through AuthoritativeHost;
//! transports never open another SQLite connection.
use super::*;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Args)]
pub struct BlueprintCommand {
    /// Queue a mutation without contacting the authority (connected mode only).
    #[arg(long, conflicts_with = "require_online")]
    pub offline: bool,
    /// Require server acceptance; do not create a new pending command when unavailable.
    #[arg(long, conflicts_with = "offline")]
    pub require_online: bool,
    #[command(subcommand)]
    pub command: BlueprintSubcommand,
}

#[derive(Debug, Subcommand)]
pub enum BlueprintSubcommand {
    /// List pending or conflicted blueprint mutations.
    Pending,
    /// Replay the canonical global outbox (including earlier non-blueprint commands).
    Flush,
    /// List available bundled workflow packages (Basic is the default).
    Bundled,
    /// List installed, immutable workflow versions.
    List {
        #[arg(long)]
        id: Option<String>,
    },
    /// Inspect a published workflow definition.
    Show { id: String, version: u32 },
    /// Install a bundled workflow without changing existing projects.
    Install { id: String },
    /// Validate a workflow file locally without publishing it or contacting the server.
    Validate { file: PathBuf },
    /// Publish a workflow document in JSON or YAML format.
    Import { file: PathBuf },
    /// Export a published workflow as JSON or YAML (inferred from destination extension).
    Export {
        id: String,
        version: u32,
        #[arg(long)]
        file: Option<PathBuf>,
    },
    /// Explicitly pin a project to an installed workflow version. Fails if the
    /// project is already bound; use `apply` to change an existing binding.
    Bind {
        project: String,
        id: String,
        version: u32,
    },
    /// Change a project's workflow, replacing the current binding.
    Apply {
        project: String,
        id: String,
        /// Omit to use the bundled descriptor or latest published version.
        #[arg(long)]
        version: Option<u32>,
    },
    /// Preview an explicit workflow version upgrade. Read-only: nothing is bound.
    UpgradePreview {
        project: String,
        #[arg(long)]
        id: String,
        #[arg(long)]
        version: u32,
    },
    /// Apply a previewed workflow version upgrade. Requires `--confirm` and the
    /// exact `--fingerprint` printed by `upgrade-preview`.
    Upgrade {
        project: String,
        #[arg(long)]
        id: String,
        #[arg(long)]
        version: u32,
        /// Stable fingerprint of the approved preview.
        #[arg(long)]
        fingerprint: String,
        /// Explicit approval; the authority never upgrades on its own.
        #[arg(long)]
        confirm: bool,
    },
    /// Remove a project's workflow binding so generated default views are used.
    Unbind { project: String },
    /// Show a project's pinned workflow (if any).
    Binding { project: String },
    /// Create a typed work item using the project's pinned workflow.
    CreateItem {
        project: String,
        id: String,
        kind: String,
        #[arg(long)]
        properties: Option<PathBuf>,
    },
    /// Inspect a workflow-defined work item.
    Item { id: String },
    /// Transition an item using its workflow-defined states.
    Transition {
        id: String,
        state: String,
        #[arg(long)]
        revision: u64,
    },
    /// Update work-item properties using a JSON object and revision check.
    SetProperties {
        id: String,
        #[arg(long)]
        revision: u64,
        #[arg(long)]
        file: PathBuf,
    },
    /// Instantiate a reusable workflow template atomically.
    ApplyTemplate {
        project: String,
        template: String,
        namespace: String,
    },
    /// Inspect, install, authorize, revoke and uninstall trusted capabilities.
    Capability {
        #[command(subcommand)]
        command: CapabilitySubcommand,
    },
    /// Explicit capability action offers, requests, execution and receipts.
    Action {
        #[command(subcommand)]
        command: CapabilityActionSubcommand,
    },
}

/// Trusted capability administration. Installation never grants authority:
/// `install` records a contract, `authorize` creates one project grant.
#[derive(Debug, Subcommand)]
pub enum CapabilitySubcommand {
    /// List durable installations joined with the trusted composition.
    List {
        #[arg(long)]
        project: Option<String>,
    },
    /// Install the trusted composition's contract for one capability.
    Install { capability_id: String },
    /// Remove an installation and every project grant it had.
    Uninstall { capability_id: String },
    /// Authorize an installed capability for exactly one project.
    Authorize {
        capability_id: String,
        #[arg(long)]
        project: String,
        /// Explicit user confirmation is required to create a grant.
        #[arg(long)]
        confirm: bool,
    },
    /// Revoke one project's authorization.
    Revoke {
        capability_id: String,
        #[arg(long)]
        project: String,
    },
}

/// Explicit capability action lifecycle. Every step is a separate authority
/// operation: offers are read-only, requests are durable intents, and only
/// `execute` dispatches the trusted executor.
#[derive(Debug, Subcommand)]
pub enum CapabilityActionSubcommand {
    /// Show the actions offered for the item's current state.
    Offers { item_id: String },
    /// Record an explicit request (authorization is checked, never inferred).
    Request {
        item_id: String,
        #[arg(long)]
        action: String,
        #[arg(long)]
        revision: u64,
        /// Stable command ID; echoed so scripts can replay the exact request.
        #[arg(long)]
        command: Option<String>,
        /// JSON object of user-supplied values overlaid over declared defaults.
        #[arg(long)]
        input: Option<String>,
        /// Required for effectful or destructive actions.
        #[arg(long)]
        confirm: bool,
    },
    /// Claim and execute a recorded request, printing the durable receipt.
    Execute { command_id: String },
    /// Inspect one action request and its receipt.
    Show { command_id: String },
    /// List recorded action requests.
    List {
        #[arg(long)]
        item: Option<String>,
        #[arg(long)]
        state: Option<String>,
        #[arg(long)]
        limit: Option<usize>,
    },
    /// Explicitly resolve an uncertain action. Never re-executes anything.
    Reconcile {
        command_id: String,
        #[arg(long, conflicts_with = "fail")]
        succeed: bool,
        #[arg(long, conflicts_with = "succeed")]
        fail: bool,
        #[arg(long)]
        confirm: bool,
    },
}

/// Connected transports must fail closed for provider execution: only an
/// explicitly enrolled executor process runs provider code. The authority
/// persists the intent and the receipt and never dispatches anything.
pub(super) const CONNECTED_EXECUTE_REFUSAL: &str = "capability execution is performed by an enrolled executor, not by the authority: run `orbit capability serve --once` (or keep your executor runner running). The connected authority persists the request and the receipt; it never dispatches provider code";

/// Install/authorize/revoke change what the whole workspace may execute and are
/// operator writes; the connected CLI does not expose them in this build.
pub(super) const CONNECTED_CAPABILITY_WRITE_REFUSAL: &str = "installing, authorizing and revoking capabilities are operator writes in connected mode; use the operator API or the web console. Capability actions themselves run in the enrolled executor (`orbit capability serve --once`)";

pub(super) fn is_yaml(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|s| s.to_str()),
        Some("yaml" | "yml")
    )
}

fn parse_blueprint_file(
    path: &Path,
    document: &str,
    installed: &BTreeSet<String>,
) -> Result<orbit_blueprint::Blueprint, CliError> {
    if is_yaml(path) {
        orbit_blueprint::Blueprint::from_yaml(document, installed)
            .map_err(|e| CliError::Usage(e.to_string()))
    } else {
        orbit_blueprint::Blueprint::from_json(document, installed)
            .map_err(|e| CliError::Usage(e.to_string()))
    }
}

fn json_file(path: &Path) -> Result<BTreeMap<String, serde_json::Value>, CliError> {
    let document = fs::read_to_string(path).map_err(|e| CliError::Io(e.to_string()))?;
    serde_json::from_str(&document)
        .map_err(|e| CliError::Usage(format!("expected JSON properties object: {e}")))
}

fn value<T: Serialize>(item: T) -> Result<Response, CliError> {
    Ok(Response::Authority {
        result: serde_json::to_value(item).map_err(|e| CliError::Serialization(e.to_string()))?,
    })
}

/// Keep the authority's refusal reason readable on stderr instead of printing a
/// debug-wrapped enum, and classify it as a repository/state conflict.
fn upgrade_failure(error: orbit_application::RepositoryError) -> CliError {
    match error {
        orbit_application::RepositoryError::Other(message) => CliError::Repository(message),
        other => CliError::repository(other),
    }
}

/// Validate a blueprint file without opening an authority database. This keeps
/// read-only authoring checks from creating an empty `orbit.db` in the caller's
/// working directory.
pub(super) fn validate_file(file: &Path) -> Result<Response, CliError> {
    let document = fs::read_to_string(file).map_err(|e| CliError::Io(e.to_string()))?;
    // Local authoring validation has no workspace, so no capability can be
    // treated as installed. Manifests that require capabilities are validated
    // against the durable installation set by `import`.
    let blueprint = parse_blueprint_file(file, &document, &BTreeSet::new())?;
    value(serde_json::json!({"valid": true, "id": blueprint.id,
        "version": blueprint.version, "work_item_types": blueprint.work_items.len()}))
}

pub(super) fn run_blueprint(
    host: &mut orbit_runtime::AuthoritativeHost,
    command: BlueprintSubcommand,
) -> Result<Response, CliError> {
    match command {
        BlueprintSubcommand::Pending | BlueprintSubcommand::Flush => Err(CliError::Usage(
            "offline blueprint queue requires connected mode".into(),
        )),
        BlueprintSubcommand::Bundled => value(
            host.bundled_project_blueprints()
                .iter()
                .map(|b| b.id)
                .collect::<Vec<_>>(),
        ),
        BlueprintSubcommand::List { id } => value(
            host.available_blueprint_versions(id.as_deref())
                .map_err(CliError::repository)?,
        ),
        BlueprintSubcommand::Show { id, version } => value(
            host.project_blueprint(&id, version)
                .map_err(CliError::repository)?
                .ok_or_else(|| CliError::Usage("blueprint version not found".into()))?,
        ),
        BlueprintSubcommand::Install { id } => value(
            host.install_bundled_project_blueprint(&id)
                .map_err(CliError::repository)?,
        ),
        BlueprintSubcommand::Validate { file } => {
            let document = fs::read_to_string(&file).map_err(|e| CliError::Io(e.to_string()))?;
            let blueprint = parse_blueprint_file(&file, &document, &BTreeSet::new())?;
            value(serde_json::json!({"valid": true, "id": blueprint.id,
                "version": blueprint.version, "work_item_types": blueprint.work_items.len()}))
        }
        BlueprintSubcommand::Import { file } => {
            let composition =
                orbit_runtime::capability_composition::TrustedComposition::with_builtin_reference();
            // Publication is gated by durable installation, not by a manifest
            // declaration: a capability must be installed before a manifest that
            // requires it can be published.
            let installed = host
                .installed_capability_ids(&composition)
                .map_err(CliError::repository)?;
            let document = fs::read_to_string(&file).map_err(|e| CliError::Io(e.to_string()))?;
            let blueprint = parse_blueprint_file(&file, &document, &installed)?;
            host.publish_project_blueprint(&blueprint, &installed)
                .map_err(CliError::repository)?;
            value(blueprint)
        }
        BlueprintSubcommand::Export { id, version, file } => {
            let document = host
                .export_project_blueprint_json(&id, version)
                .map_err(CliError::repository)?
                .ok_or_else(|| CliError::Usage("blueprint version not found".into()))?;
            if let Some(path) = file {
                let encoded = if is_yaml(&path) {
                    let blueprint: orbit_blueprint::Blueprint = serde_json::from_str(&document)
                        .map_err(|e| CliError::Serialization(e.to_string()))?;
                    blueprint
                        .to_yaml()
                        .map_err(|e| CliError::Usage(e.to_string()))?
                } else {
                    document
                };
                fs::write(&path, encoded.as_bytes()).map_err(|e| CliError::Io(e.to_string()))?;
                value(serde_json::json!({"exported": path, "id": id, "version": version}))
            } else {
                let manifest: serde_json::Value = serde_json::from_str(&document)
                    .map_err(|e| CliError::Serialization(e.to_string()))?;
                value(manifest)
            }
        }
        BlueprintSubcommand::Bind {
            project,
            id,
            version,
        } => value(
            host.activate_project_blueprint(&project, &id, version)
                .map_err(CliError::repository)?,
        ),
        BlueprintSubcommand::Apply {
            project,
            id,
            version,
        } => {
            let project_id = orbit_domain::ProjectId::new(project).map_err(CliError::domain)?;
            let revision = host.snapshot().map_err(CliError::repository)?.0;
            let (_, _, project_id) = host
                .set_project_workflow_accepted(
                    "orbit-cli",
                    &generate_id("command"),
                    revision,
                    project_id,
                    id,
                    version,
                )
                .map_err(CliError::repository)?;
            value(
                host.active_project_blueprint(&project_id)
                    .map_err(CliError::repository)?,
            )
        }
        BlueprintSubcommand::UpgradePreview {
            project,
            id,
            version,
        } => {
            // Pure read: the preview never binds, migrates or writes an item.
            value(
                host.preview_blueprint_upgrade(&project, &id, version)
                    .map_err(upgrade_failure)?,
            )
        }
        BlueprintSubcommand::Upgrade {
            project,
            id,
            version,
            fingerprint,
            confirm,
        } => {
            // Missing confirmation is a usage error (exit 2); every other
            // refusal (blocker, stale fingerprint) is an authority conflict
            // (exit 4). The authority re-checks both fences itself.
            if !confirm {
                return Err(CliError::Usage(
                    "workflow upgrade requires explicit --confirm".into(),
                ));
            }
            value(
                host.apply_blueprint_upgrade(&project, &id, version, &fingerprint, confirm)
                    .map_err(upgrade_failure)?,
            )
        }
        BlueprintSubcommand::Unbind { project } => {
            let project_id = orbit_domain::ProjectId::new(project).map_err(CliError::domain)?;
            let project_label = project_id.to_string();
            let revision = host.snapshot().map_err(CliError::repository)?.0;
            let (_, _, removed) = host
                .remove_project_workflow_accepted(
                    "orbit-cli",
                    &generate_id("command"),
                    revision,
                    project_id,
                )
                .map_err(CliError::repository)?;
            value(serde_json::json!({"project_id": project_label, "removed": removed}))
        }
        BlueprintSubcommand::Binding { project } => value(
            host.active_project_blueprint(&project)
                .map_err(CliError::repository)?,
        ),
        BlueprintSubcommand::CreateItem {
            project,
            id,
            kind,
            properties,
        } => value(
            host.create_project_work_item(
                &id,
                &project,
                &kind,
                match properties {
                    Some(file) => json_file(&file)?,
                    None => BTreeMap::new(),
                },
            )
            .map_err(CliError::repository)?,
        ),
        BlueprintSubcommand::Item { id } => {
            let item = host
                .project_work_item(&id)
                .map_err(CliError::repository)?
                .ok_or_else(|| CliError::Usage("work item not found".into()))?;
            // Durable capability effects are part of the item's observable state
            // for inspection scripts; they are read-only projections.
            let capability_notes = host
                .reference_work_item_notes(&id)
                .map_err(CliError::repository)?;
            value(serde_json::json!({
                "item": item,
                "capability_notes": capability_notes,
            }))
        }
        BlueprintSubcommand::Transition {
            id,
            state,
            revision,
        } => value(
            // A guarded transition becomes usable through the project's durable
            // capability grants, never through renderer-supplied authority.
            host.transition_project_work_item_authorized(&id, &state, revision)
                .map_err(CliError::repository)?,
        ),
        BlueprintSubcommand::SetProperties { id, revision, file } => value(
            host.update_project_work_item_properties(&id, revision, json_file(&file)?)
                .map_err(CliError::repository)?,
        ),
        BlueprintSubcommand::ApplyTemplate {
            project,
            template,
            namespace,
        } => {
            let plan = host
                .apply_project_blueprint_template(&project, &template, &namespace)
                .map_err(CliError::repository)?;
            value(serde_json::json!({"items": plan.items, "prerequisites": plan.prerequisites}))
        }
        BlueprintSubcommand::Capability { command } => {
            let composition =
                orbit_runtime::capability_composition::TrustedComposition::with_builtin_reference();
            run_capability(host, &composition, command)
        }
        BlueprintSubcommand::Action { command } => {
            let mut composition =
                orbit_runtime::capability_composition::TrustedComposition::with_builtin_reference();
            run_capability_action(host, &mut composition, command)
        }
    }
}

fn run_capability(
    host: &mut orbit_runtime::AuthoritativeHost,
    composition: &orbit_runtime::capability_composition::TrustedComposition,
    command: CapabilitySubcommand,
) -> Result<Response, CliError> {
    match command {
        CapabilitySubcommand::List { project } => value(
            host.capability_status(composition, project.as_deref())
                .map_err(CliError::repository)?,
        ),
        CapabilitySubcommand::Install { capability_id } => {
            if composition.capability(&capability_id).is_none() {
                let known: Vec<&str> = composition
                    .capabilities()
                    .map(|capability| capability.descriptor.id.as_str())
                    .collect();
                return Err(CliError::Usage(format!(
                    "unknown capability: {capability_id}; known capabilities: {}",
                    known.join(", ")
                )));
            }
            value(
                host.install_trusted_capability(composition, &capability_id, now_ms())
                    .map_err(CliError::repository)?,
            )
        }
        CapabilitySubcommand::Uninstall { capability_id } => {
            let removed = host
                .uninstall_trusted_capability(&capability_id)
                .map_err(CliError::repository)?;
            value(serde_json::json!({"capability_id": capability_id, "removed": removed}))
        }
        CapabilitySubcommand::Authorize {
            capability_id,
            project,
            confirm,
        } => {
            if !confirm {
                return Err(CliError::Usage(
                    "capability authorization requires --confirm".into(),
                ));
            }
            value(
                host.authorize_trusted_capability(&capability_id, &project, "orbit-cli", now_ms())
                    .map_err(CliError::repository)?,
            )
        }
        CapabilitySubcommand::Revoke {
            capability_id,
            project,
        } => {
            let revoked = host
                .revoke_trusted_capability(&capability_id, &project)
                .map_err(CliError::repository)?;
            value(serde_json::json!({
                "capability_id": capability_id,
                "project_id": project,
                "revoked": revoked,
            }))
        }
    }
}

pub(super) fn parse_action_inputs(
    raw: Option<&str>,
) -> Result<BTreeMap<String, serde_json::Value>, CliError> {
    let Some(raw) = raw else {
        return Ok(BTreeMap::new());
    };
    let value: serde_json::Value = serde_json::from_str(raw)
        .map_err(|error| CliError::Usage(format!("--input must be a JSON object: {error}")))?;
    match value {
        serde_json::Value::Object(map) => Ok(map.into_iter().collect()),
        _ => Err(CliError::Usage("--input must be a JSON object".into())),
    }
}

fn run_capability_action(
    host: &mut orbit_runtime::AuthoritativeHost,
    composition: &mut orbit_runtime::capability_composition::TrustedComposition,
    command: CapabilityActionSubcommand,
) -> Result<Response, CliError> {
    use orbit_runtime::capability_composition::{
        CapabilityActionAvailability, CapabilityActionRequest, CapabilityReconcileDecision,
    };
    match command {
        CapabilityActionSubcommand::Offers { item_id } => value(
            host.capability_action_offers(composition, &item_id)
                .map_err(CliError::repository)?,
        ),
        CapabilityActionSubcommand::Request {
            item_id,
            action,
            revision,
            command,
            input,
            confirm,
        } => {
            let inputs = parse_action_inputs(input.as_deref())?;
            let command_id = command.unwrap_or_else(|| generate_id("action"));
            // Offers are advisory; the authority re-checks everything below.
            let offers = host
                .capability_action_offers(composition, &item_id)
                .map_err(CliError::repository)?;
            let offer = offers
                .iter()
                .find(|offer| offer.action_id == action)
                .ok_or_else(|| {
                    CliError::Usage(format!(
                        "action {action} is not offered for work item {item_id}"
                    ))
                })?;
            if let CapabilityActionAvailability::Unavailable { code, reason } = &offer.availability
            {
                return Err(CliError::Usage(format!(
                    "capability action unavailable: {code}: {reason}"
                )));
            }
            let destructive = composition
                .capability(&offer.capability)
                .and_then(|capability| capability.action(&offer.action_id))
                .map_or(offer.external_effect, |spec| spec.is_destructive());
            if destructive && !confirm {
                return Err(CliError::Usage(format!(
                    "capability action {} has external effects; re-run with --confirm",
                    offer.action_id
                )));
            }
            let request = host
                .request_capability_action(
                    composition,
                    CapabilityActionRequest {
                        item_id,
                        expected_revision: revision,
                        action_id: action,
                        command_id,
                        inputs,
                    },
                )
                .map_err(CliError::repository)?;
            value(request)
        }
        CapabilityActionSubcommand::Execute { command_id } => value(
            host.execute_capability_action(composition, &command_id, now_ms())
                .map_err(CliError::repository)?,
        ),
        CapabilityActionSubcommand::Show { command_id } => {
            let state = host
                .capability_action_receipt(&command_id)
                .map_err(CliError::repository)?
                .ok_or_else(|| {
                    CliError::Usage(format!("unknown capability action: {command_id}"))
                })?;
            value(state)
        }
        CapabilityActionSubcommand::List { item, state, limit } => value(
            host.list_capability_actions(item.as_deref(), state.as_deref(), limit.unwrap_or(50))
                .map_err(CliError::repository)?,
        ),
        CapabilityActionSubcommand::Reconcile {
            command_id,
            succeed,
            fail,
            confirm,
        } => {
            let decision = match (succeed, fail) {
                (true, false) => CapabilityReconcileDecision::ConfirmSucceeded,
                (false, true) => CapabilityReconcileDecision::ConfirmFailed,
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
            let receipt = host
                .reconcile_capability_action(&command_id, decision, "orbit-cli", now_ms())
                .map_err(CliError::repository)?;
            // Surface which ambiguous state was resolved (`claimed` for a lost
            // acknowledgement, `uncertain` for an ambiguous provider result).
            let recovered_from = receipt
                .detail
                .get("recovered_from")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("settled")
                .to_owned();
            value(serde_json::json!({
                "recovered_from": recovered_from,
                "receipt": receipt,
            }))
        }
    }
}

#[cfg(test)]
mod blueprint_authoring_tests {
    use super::*;

    #[test]
    fn bundled_json_can_be_validated_without_authority() {
        let source = include_str!("../../../blueprints/development.json");
        let blueprint =
            parse_blueprint_file(Path::new("development.json"), source, &BTreeSet::new()).unwrap();
        assert_eq!(blueprint.id, "development");
    }

    #[test]
    fn json_and_yaml_are_equivalent_for_authoring() {
        let source = include_str!("../../../blueprints/learning.json");
        let original =
            parse_blueprint_file(Path::new("learning.json"), source, &BTreeSet::new()).unwrap();
        let yaml = original.to_yaml().unwrap();
        let parsed =
            parse_blueprint_file(Path::new("learning.yaml"), &yaml, &BTreeSet::new()).unwrap();
        assert_eq!(parsed, original);
        assert!(is_yaml(Path::new("learning.yml")));
        assert!(!is_yaml(Path::new("learning.json")));
    }

    #[test]
    fn capability_manifest_requires_installed_capability() {
        let source = include_str!("../../../blueprints/examples/capability-reference.json");
        // Publication/import requires the durable installation set; authoring
        // validation without a workspace must refuse the manifest.
        assert!(
            parse_blueprint_file(
                Path::new("capability-reference.json"),
                source,
                &BTreeSet::new()
            )
            .is_err()
        );
        let installed = BTreeSet::from(["orbit.reference.work-item-note".to_owned()]);
        let blueprint =
            parse_blueprint_file(Path::new("capability-reference.json"), source, &installed)
                .unwrap();
        assert_eq!(blueprint.id, "capability-reference");
    }
}

#[cfg(test)]
mod capability_cli_tests {
    use super::*;
    use orbit_runtime::capability_composition::{
        CapabilityActionRequest, REFERENCE_CAPABILITY_ID, TrustedComposition,
    };

    fn host() -> orbit_runtime::AuthoritativeHost {
        let mut host = orbit_runtime::AuthoritativeHost::open(":memory:").unwrap();
        host.create_project_accepted(
            "orbit-cli",
            "cmd-project",
            0,
            CreateProjectWithActivityCommand {
                project: CreateProjectCommand {
                    id: ProjectId::new("project-1").unwrap(),
                    name: "Capability".into(),
                },
                activity_id: ActivityId::new("activity-1").unwrap(),
                origin: ActivityOrigin::Human,
                summary: "project created".into(),
            },
        )
        .unwrap();
        host
    }

    /// A request that was claimed but never acknowledged, then recovered through
    /// the public CLI reconcile command.
    #[test]
    fn reconcile_surfaces_the_recovered_state_and_requires_confirmation() {
        let mut host = host();
        let composition = TrustedComposition::with_builtin_reference();
        run_blueprint(
            &mut host,
            BlueprintSubcommand::Capability {
                command: CapabilitySubcommand::Install {
                    capability_id: REFERENCE_CAPABILITY_ID.into(),
                },
            },
        )
        .unwrap();
        let installed = host.installed_capability_ids(&composition).unwrap();
        let blueprint = parse_blueprint_file(
            Path::new("capability-reference.json"),
            include_str!("../../../blueprints/examples/capability-reference.json"),
            &installed,
        )
        .unwrap();
        host.publish_project_blueprint(&blueprint, &installed)
            .unwrap();
        run_blueprint(
            &mut host,
            BlueprintSubcommand::Apply {
                project: "project-1".into(),
                id: "capability-reference".into(),
                version: Some(1),
            },
        )
        .unwrap();
        run_blueprint(
            &mut host,
            BlueprintSubcommand::Capability {
                command: CapabilitySubcommand::Authorize {
                    capability_id: REFERENCE_CAPABILITY_ID.into(),
                    project: "project-1".into(),
                    confirm: true,
                },
            },
        )
        .unwrap();
        host.create_project_work_item("item-1", "project-1", "note-task", BTreeMap::new())
            .unwrap();
        host.request_capability_action(
            &composition,
            CapabilityActionRequest {
                item_id: "item-1".into(),
                expected_revision: 0,
                action_id: "append_note".into(),
                command_id: "cmd-lost".into(),
                inputs: BTreeMap::from([("note".to_owned(), serde_json::json!("hello"))]),
            },
        )
        .unwrap();
        // Lost acknowledgement: durable claim, no recorded result.
        host.claim_project_work_item_action("cmd-lost").unwrap();

        let mut reconcile = |confirm: bool| {
            run_blueprint(
                &mut host,
                BlueprintSubcommand::Action {
                    command: CapabilityActionSubcommand::Reconcile {
                        command_id: "cmd-lost".into(),
                        succeed: false,
                        fail: true,
                        confirm,
                    },
                },
            )
        };
        assert!(matches!(reconcile(false), Err(CliError::Usage(_))));
        let Response::Authority { result } = reconcile(true).unwrap() else {
            panic!("expected authority response");
        };
        assert_eq!(result["recovered_from"], "claimed");
        assert_eq!(result["receipt"]["outcome"], "failed");
        assert_eq!(result["receipt"]["command_id"], "cmd-lost");
        assert_eq!(
            result["receipt"]["detail"],
            serde_json::json!({
                "reconciled": true,
                "reconciled_by": "orbit-cli",
                "recovered_from": "claimed",
            })
        );
        // The identical decision replays to the same receipt.
        let Response::Authority { result: replay } = reconcile(true).unwrap() else {
            panic!("expected authority response");
        };
        assert_eq!(replay, result);
        let settled = host.project_work_item_action("cmd-lost").unwrap().unwrap();
        assert_eq!(settled.state, "failed");
        assert!(
            host.capability_action_receipt("cmd-lost")
                .unwrap()
                .unwrap()
                .receipt
                .is_some()
        );
    }
}

#[cfg(test)]
mod blueprint_upgrade_cli_tests {
    use super::*;

    const DEMO_ID: &str = "upgrade-demo";

    fn host() -> orbit_runtime::AuthoritativeHost {
        let mut host = orbit_runtime::AuthoritativeHost::open(":memory:").unwrap();
        host.create_project_accepted(
            "orbit-cli",
            "cmd-project",
            0,
            CreateProjectWithActivityCommand {
                project: CreateProjectCommand {
                    id: ProjectId::new("project-1").unwrap(),
                    name: "Upgrade".into(),
                },
                activity_id: ActivityId::new("activity-1").unwrap(),
                origin: ActivityOrigin::Human,
                summary: "project created".into(),
            },
        )
        .unwrap();
        host
    }

    fn manifest(version: u32) -> orbit_blueprint::Blueprint {
        let mut definition = serde_json::json!({
            "id": DEMO_ID,
            "version": version,
            "name": "Upgrade demo",
            "work_items": [{
                "id": "feature",
                "name": "Feature",
                "initial_state": "backlog",
                "states": ["backlog", "ready"],
                "fields": [{"id": "notes", "kind": {"type": "text"}}],
                "transitions": [{"from": "backlog", "to": "ready"}]
            }]
        });
        if version >= 2 {
            definition["work_items"][0]["states"]
                .as_array_mut()
                .unwrap()
                .push(serde_json::json!("done"));
            definition["work_items"][0]["transitions"]
                .as_array_mut()
                .unwrap()
                .push(serde_json::json!({"from": "ready", "to": "done"}));
            definition["work_items"][0]["fields"]
                .as_array_mut()
                .unwrap()
                .push(serde_json::json!({"id": "owner", "kind": {"type": "text"}}));
        }
        if version >= 3 {
            definition["work_items"] = serde_json::json!([{
                "id": "task",
                "name": "Task",
                "initial_state": "open",
                "states": ["open"]
            }]);
        }
        serde_json::from_value(definition).unwrap()
    }

    fn publish(host: &mut orbit_runtime::AuthoritativeHost, version: u32) {
        host.publish_project_blueprint(&manifest(version), &BTreeSet::new())
            .unwrap();
    }

    fn authority_result(response: Response) -> serde_json::Value {
        match response {
            Response::Authority { result } => result,
            other => panic!("expected authority response, received {other:?}"),
        }
    }

    fn preview(host: &mut orbit_runtime::AuthoritativeHost, version: u32) -> serde_json::Value {
        authority_result(
            run_blueprint(
                host,
                BlueprintSubcommand::UpgradePreview {
                    project: "project-1".into(),
                    id: DEMO_ID.into(),
                    version,
                },
            )
            .unwrap(),
        )
    }

    #[test]
    fn preview_then_apply_reports_the_binding_and_refuses_unsafe_replays() {
        let mut host = host();
        publish(&mut host, 1);
        publish(&mut host, 2);
        // Move the project off its automatic `basic` binding with the existing
        // (non-upgrade) workflow command.
        run_blueprint(
            &mut host,
            BlueprintSubcommand::Apply {
                project: "project-1".into(),
                id: DEMO_ID.into(),
                version: Some(1),
            },
        )
        .unwrap();

        let report = preview(&mut host, 2);
        assert_eq!(report["current"]["blueprint_id"], DEMO_ID);
        assert_eq!(report["current"]["version"], 1);
        assert_eq!(report["target"]["version"], 2);
        assert_eq!(report["blockers"], serde_json::json!([]));
        assert!(!report["kinds"].as_array().unwrap().is_empty());
        assert!(report["views"]["added"].is_array());
        assert!(report["views"]["removed"].is_array());
        assert!(report["views"]["changed"].is_array());
        assert!(!report["warnings"].as_array().unwrap().is_empty());
        let fingerprint = report["fingerprint"].as_str().unwrap().to_owned();
        assert_eq!(fingerprint.len(), 64);
        assert!(report["project_revision"].is_i64());
        // The preview never binds: this is a read-only report.
        assert_eq!(
            host.active_project_blueprint("project-1")
                .unwrap()
                .unwrap()
                .version,
            1
        );

        // Missing --confirm exits 2 and writes nothing.
        let error = run_blueprint(
            &mut host,
            BlueprintSubcommand::Upgrade {
                project: "project-1".into(),
                id: DEMO_ID.into(),
                version: 2,
                fingerprint: fingerprint.clone(),
                confirm: false,
            },
        )
        .unwrap_err();
        assert!(matches!(error, CliError::Usage(_)), "{error:?}");
        assert_eq!(error.exit_code(), 2);

        // A fabricated fingerprint exits 4 and writes nothing.
        let error = run_blueprint(
            &mut host,
            BlueprintSubcommand::Upgrade {
                project: "project-1".into(),
                id: DEMO_ID.into(),
                version: 2,
                fingerprint: "0".repeat(64),
                confirm: true,
            },
        )
        .unwrap_err();
        assert!(matches!(error, CliError::Repository(_)), "{error:?}");
        assert_eq!(error.exit_code(), 4);
        assert!(
            error.to_string().contains("fingerprint mismatch"),
            "{error}"
        );
        assert_eq!(
            host.active_project_blueprint("project-1")
                .unwrap()
                .unwrap()
                .version,
            1
        );

        // The approved fingerprint applies and exposes the report and binding.
        let applied = authority_result(
            run_blueprint(
                &mut host,
                BlueprintSubcommand::Upgrade {
                    project: "project-1".into(),
                    id: DEMO_ID.into(),
                    version: 2,
                    fingerprint: fingerprint.clone(),
                    confirm: true,
                },
            )
            .unwrap(),
        );
        assert_eq!(applied["binding"]["project_id"], "project-1");
        assert_eq!(applied["binding"]["version"], 2);
        assert_eq!(applied["previous"]["version"], 1);
        assert_eq!(applied["already_applied"], false);
        assert_eq!(applied["fingerprint"], fingerprint);
        assert!(applied["kinds"].is_array());

        // Repeating the same command is idempotent, not a second upgrade.
        let replay = authority_result(
            run_blueprint(
                &mut host,
                BlueprintSubcommand::Upgrade {
                    project: "project-1".into(),
                    id: DEMO_ID.into(),
                    version: 2,
                    fingerprint,
                    confirm: true,
                },
            )
            .unwrap(),
        );
        assert_eq!(replay["already_applied"], true);
        assert_eq!(replay["binding"]["version"], 2);
    }

    #[test]
    fn blocked_upgrade_preview_exits_zero_and_apply_exits_four() {
        let mut host = host();
        publish(&mut host, 1);
        run_blueprint(
            &mut host,
            BlueprintSubcommand::Apply {
                project: "project-1".into(),
                id: DEMO_ID.into(),
                version: Some(1),
            },
        )
        .unwrap();
        let item = authority_result(
            run_blueprint(
                &mut host,
                BlueprintSubcommand::CreateItem {
                    project: "project-1".into(),
                    id: "item-1".into(),
                    kind: "feature".into(),
                    properties: None,
                },
            )
            .unwrap(),
        );
        assert_eq!(item["kind"], "feature");
        publish(&mut host, 3);

        let report = preview(&mut host, 3);
        let blockers = report["blockers"].as_array().unwrap();
        assert_eq!(blockers.len(), 1, "{blockers:?}");
        assert_eq!(blockers[0]["code"], "kind_removed_with_live_items");
        assert_eq!(blockers[0]["subjects"], serde_json::json!(["item-1"]));
        let fingerprint = report["fingerprint"].as_str().unwrap().to_owned();

        let error = run_blueprint(
            &mut host,
            BlueprintSubcommand::Upgrade {
                project: "project-1".into(),
                id: DEMO_ID.into(),
                version: 3,
                fingerprint,
                confirm: true,
            },
        )
        .unwrap_err();
        assert!(matches!(error, CliError::Repository(_)), "{error:?}");
        assert_eq!(error.exit_code(), 4);
        assert!(
            error.to_string().contains("kind_removed_with_live_items"),
            "{error}"
        );
        assert_eq!(
            host.active_project_blueprint("project-1")
                .unwrap()
                .unwrap()
                .version,
            1
        );
    }
}
