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
    /// List available bundled blueprint packages.
    Bundled,
    /// List installed, immutable blueprint versions.
    List {
        #[arg(long)]
        id: Option<String>,
    },
    /// Inspect a published blueprint definition.
    Show { id: String, version: u32 },
    /// Install a bundled blueprint without changing existing projects.
    Install { id: String },
    /// Validate a blueprint file locally without publishing it or contacting the server.
    Validate { file: PathBuf },
    /// Publish a blueprint document in JSON or YAML format.
    Import { file: PathBuf },
    /// Export a published blueprint as JSON or YAML (inferred from destination extension).
    Export {
        id: String,
        version: u32,
        #[arg(long)]
        file: Option<PathBuf>,
    },
    /// Explicitly pin a project to an installed blueprint version.
    Bind {
        project: String,
        id: String,
        version: u32,
    },
    /// Show a project's pinned blueprint (if any).
    Binding { project: String },
    /// Create a typed work item using the project's pinned blueprint.
    CreateItem {
        project: String,
        id: String,
        kind: String,
        #[arg(long)]
        properties: Option<PathBuf>,
    },
    /// Inspect a blueprint-defined work item.
    Item { id: String },
    /// Transition an item using its blueprint-defined workflow.
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
    /// Instantiate a reusable blueprint template atomically.
    ApplyTemplate {
        project: String,
        template: String,
        namespace: String,
    },
}

pub(super) fn is_yaml(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|s| s.to_str()),
        Some("yaml" | "yml")
    )
}

fn parse_blueprint_file(
    path: &Path,
    document: &str,
) -> Result<orbit_blueprint::Blueprint, CliError> {
    let installed = BTreeSet::new();
    if is_yaml(path) {
        orbit_blueprint::Blueprint::from_yaml(document, &installed)
            .map_err(|e| CliError::Usage(e.to_string()))
    } else {
        orbit_blueprint::Blueprint::from_json(document, &installed)
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

/// Validate a blueprint file without opening an authority database. This keeps
/// read-only authoring checks from creating an empty `orbit.db` in the caller's
/// working directory.
pub(super) fn validate_file(file: &Path) -> Result<Response, CliError> {
    let document = fs::read_to_string(file).map_err(|e| CliError::Io(e.to_string()))?;
    let blueprint = parse_blueprint_file(file, &document)?;
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
            let blueprint = parse_blueprint_file(&file, &document)?;
            value(serde_json::json!({"valid": true, "id": blueprint.id,
                "version": blueprint.version, "work_item_types": blueprint.work_items.len()}))
        }
        BlueprintSubcommand::Import { file } => {
            let document = fs::read_to_string(&file).map_err(|e| CliError::Io(e.to_string()))?;
            let blueprint = parse_blueprint_file(&file, &document)?;
            host.publish_project_blueprint(&blueprint, &BTreeSet::new())
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
            value(host.project_work_item(&id).map_err(CliError::repository)?)
        }
        BlueprintSubcommand::Transition {
            id,
            state,
            revision,
        } => value(
            host.transition_project_work_item(&id, &state, revision, &BTreeSet::new())
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
    }
}

#[cfg(test)]
mod blueprint_authoring_tests {
    use super::*;

    #[test]
    fn bundled_json_can_be_validated_without_authority() {
        let source = include_str!("../../../blueprints/development.json");
        let blueprint = parse_blueprint_file(Path::new("development.json"), source).unwrap();
        assert_eq!(blueprint.id, "development");
    }

    #[test]
    fn json_and_yaml_are_equivalent_for_authoring() {
        let source = include_str!("../../../blueprints/learning.json");
        let original = parse_blueprint_file(Path::new("learning.json"), source).unwrap();
        let yaml = original.to_yaml().unwrap();
        let parsed = parse_blueprint_file(Path::new("learning.yaml"), &yaml).unwrap();
        assert_eq!(parsed, original);
        assert!(is_yaml(Path::new("learning.yml")));
        assert!(!is_yaml(Path::new("learning.json")));
    }
}
