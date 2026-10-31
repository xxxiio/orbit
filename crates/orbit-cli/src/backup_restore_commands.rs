//! `orbit backup restore` — preflight, isolated restore and maintenance
//! replacement.
//!
//! Every decision is delegated to `orbit_backup::RestoreService`; this module
//! only parses arguments and renders results. Restore never overwrites a live
//! database: replacement is staged and verified, and the previous workspace is
//! preserved for rollback.

use std::path::PathBuf;

use clap::Args;
use serde::Serialize;

use orbit_backup::{
    BackupError, RestoreMode, RestoreOptions, RestoreService, RestoreTarget, WorkspaceLayout,
};
use orbit_client::config::Resolved;
use orbit_client::deployment::Deployment;

use crate::CliError;

/// Arguments for `orbit backup restore`.
#[derive(Debug, Args)]
pub struct RestoreArgs {
    /// Path to a `.orbitbak` archive.
    pub file: PathBuf,
    /// Show exactly what would happen without changing anything.
    #[arg(long, default_value_t = false)]
    pub dry_run: bool,
    /// Restore into this new, isolated directory (never the active workspace).
    #[arg(long)]
    pub to: Option<PathBuf>,
    /// Replace the active workspace described by the resolved database.
    #[arg(long, default_value_t = false)]
    pub replace: bool,
    /// Explicit replacement confirmation: `REPLACE` or the exact target database path.
    #[arg(long)]
    pub confirm: Option<String>,
    /// Skip deep SQLite verification during preflight (not recommended).
    #[arg(long, default_value_t = false)]
    pub fast_preflight: bool,
}

/// Run a restore request.
pub fn run_restore_request(
    args: RestoreArgs,
    deployment: &Deployment,
    db_path: &std::path::Path,
    _resolved: Option<&Resolved>,
    json: bool,
) -> Result<String, CliError> {
    if args.to.is_some() && args.replace {
        return Err(CliError::Usage(
            "--to (isolated restore) and --replace are different recovery paths; choose one"
                .to_owned(),
        ));
    }
    let target = if let Some(directory) = args.to.clone() {
        RestoreTarget::Isolated {
            destination_dir: directory,
        }
    } else if args.replace {
        if matches!(deployment, Deployment::Connected { .. }) {
            return Err(CliError::Usage(
                "a connected client replica cannot be replaced from an authority backup; \
                 restore into an isolated workspace and re-enroll the client instead"
                    .to_owned(),
            ));
        }
        RestoreTarget::Replace {
            layout: WorkspaceLayout::for_database(db_path).map_err(map_error)?,
        }
    } else if args.dry_run {
        // A dry run without an explicit target describes the requested shape
        // only if the caller supplied one; otherwise explain both paths.
        return Err(CliError::Usage(
            "restore --dry-run needs a destination: use --to <dir> for an isolated restore \
             or --replace to inspect a workspace replacement"
                .to_owned(),
        ));
    } else {
        return Err(CliError::Usage(
            "choose a restore destination: --to <dir> restores into an isolated workspace, \
             --replace replaces the active workspace (requires --confirm)"
                .to_owned(),
        ));
    };
    let mode = target.mode();
    let options = RestoreOptions {
        confirmation: args.confirm.clone(),
        deep_verification: !args.fast_preflight,
        // The resolved active database lets preflight canonicalize and refuse an
        // isolated destination that contains the running workspace. Preflight
        // never opens or probes this database.
        active_database: Some(db_path.to_path_buf()),
        ..RestoreOptions::default()
    };
    if args.dry_run {
        let preflight =
            RestoreService::preflight(&args.file, &target, &options).map_err(map_error)?;
        let rendered = if json {
            to_json(&DryRunResponse {
                r#type: "restore_plan",
                admissible: preflight.admissible,
                plan: preflight.plan,
                blockers: preflight.blockers,
                warnings: preflight.warnings,
            })?
        } else {
            render_plan(&preflight)
        };
        if preflight.admissible {
            return Ok(rendered);
        }
        return Err(CliError::Diagnostics(rendered));
    }

    let preflight = RestoreService::preflight(&args.file, &target, &options).map_err(map_error)?;
    if !preflight.admissible {
        let rendered = if json {
            to_json(&DryRunResponse {
                r#type: "restore_refused",
                admissible: false,
                plan: preflight.plan,
                blockers: preflight.blockers,
                warnings: preflight.warnings,
            })?
        } else {
            render_plan(&preflight)
        };
        return Err(CliError::Diagnostics(rendered));
    }
    let mut progress = |_event: &orbit_backup::BackupProgress| {};
    let outcome =
        RestoreService::restore(&args.file, target, &options, &mut progress).map_err(map_error)?;
    let response = RestoreResponse {
        r#type: "restore_complete",
        mode: match mode {
            RestoreMode::Isolated => "isolated",
            RestoreMode::Replace => "replace",
        },
        database: outcome.target.database().display().to_string(),
        knowledge_vault: outcome.target.knowledge_vault().display().to_string(),
        attachment_store: outcome.target.attachment_store().display().to_string(),
        activated: outcome.activated,
        preserved: outcome.preserved.map(|path| path.display().to_string()),
        journal: outcome.journal.map(|path| path.display().to_string()),
        verified: outcome.verification.ok,
        counts: outcome.counts,
    };
    if json {
        return to_json(&response);
    }
    Ok(response.human())
}

fn render_plan(preflight: &orbit_backup::RestorePreflight) -> String {
    let plan = &preflight.plan;
    let mut lines = vec![
        format!(
            "Restore plan ({})",
            match plan.mode {
                RestoreMode::Isolated => "isolated restore",
                RestoreMode::Replace => "workspace replacement",
            }
        ),
        format!("  Archive:        {}", plan.archive),
        format!("  Created:        {}", plan.created_at),
        format!(
            "  Label:          {}",
            plan.label.as_deref().unwrap_or("(none)")
        ),
        format!(
            "  Backup source:  {} ({})",
            plan.backup_source_mode.as_str(),
            if plan.schema_compatible {
                format!("schema {} compatible", plan.backup_schema_version)
            } else {
                format!(
                    "schema {} NOT compatible with build schema {}",
                    plan.backup_schema_version, plan.supported_schema_version
                )
            }
        ),
        format!(
            "  Target:         {} ({})",
            plan.target.database,
            if plan.target.exists {
                "exists"
            } else {
                "will be created"
            }
        ),
        format!("  Vault:          {}", plan.target.knowledge_vault),
        format!("  Attachments:    {}", plan.target.attachment_store),
        format!(
            "  Downtime:       {}",
            if plan.downtime_required {
                "required"
            } else {
                "none"
            }
        ),
    ];
    if plan.schema_upgrade_required {
        lines.push(format!(
            "  Migration:      restored schema {} will be upgraded to {} on first open",
            plan.backup_schema_version, plan.supported_schema_version
        ));
    }
    if !plan.replaces.is_empty() {
        lines.push(format!("  Replaces:       {}", plan.replaces.join(", ")));
        lines.push(format!("  Preserves:      {}", plan.preserves.join(", ")));
    }
    if !plan.connected_client_warnings.is_empty() {
        lines.push("  Connected clients:".to_owned());
        for warning in &plan.connected_client_warnings {
            lines.push(format!("    - {warning}"));
        }
    }
    if !plan.external_dependencies.is_empty() {
        lines.push("  External dependencies (not restored):".to_owned());
        for external in &plan.external_dependencies {
            lines.push(format!("    - {external}"));
        }
    }
    if !plan.excluded_components.is_empty() {
        lines.push("  Archive exclusions:".to_owned());
        for exclusion in &plan.excluded_components {
            lines.push(format!(
                "    - {}: {}",
                exclusion.category, exclusion.detail
            ));
        }
    }
    if !preflight.warnings.is_empty() {
        lines.push("Warnings:".to_owned());
        for warning in &preflight.warnings {
            lines.push(format!("  [{}] {}", warning.code, warning.detail));
        }
    }
    if preflight.admissible {
        lines.push("Preflight: admissible (no changes were made).".to_owned());
        if plan.mode == RestoreMode::Replace {
            lines.push(format!(
                "To apply, re-run without --dry-run and add --confirm {}",
                plan.target.database
            ));
        }
    } else {
        lines.push("Preflight: REFUSED (no changes were made).".to_owned());
        for blocker in &preflight.blockers {
            lines.push(format!("  [{}] {}", blocker.code, blocker.detail));
        }
    }
    lines.join("\n")
}

fn map_error(error: BackupError) -> CliError {
    match error {
        BackupError::InvalidArgument(message)
        | BackupError::UnsafePath(message)
        | BackupError::NotFound(message) => CliError::Usage(message),
        other => CliError::Backup(other.to_string()),
    }
}

fn to_json(value: &impl Serialize) -> Result<String, CliError> {
    serde_json::to_string_pretty(value).map_err(|error| CliError::Serialization(error.to_string()))
}

#[derive(Debug, Serialize)]
struct DryRunResponse {
    r#type: &'static str,
    admissible: bool,
    plan: orbit_backup::RestorePlan,
    blockers: Vec<orbit_backup::RestoreProblem>,
    warnings: Vec<orbit_backup::RestoreProblem>,
}

#[derive(Debug, Serialize)]
struct RestoreResponse {
    r#type: &'static str,
    mode: &'static str,
    database: String,
    knowledge_vault: String,
    attachment_store: String,
    activated: bool,
    preserved: Option<String>,
    journal: Option<String>,
    verified: bool,
    counts: orbit_backup::BackupCounts,
}

impl RestoreResponse {
    fn human(&self) -> String {
        let mut lines = vec![
            format!(
                "Restore complete ({}).",
                if self.mode == "isolated" {
                    "isolated workspace"
                } else {
                    "workspace replaced"
                }
            ),
            format!("  Database:      {}", self.database),
            format!("  Vault:         {}", self.knowledge_vault),
            format!("  Attachments:   {}", self.attachment_store),
            format!(
                "  Verified:      {}",
                if self.verified { "yes" } else { "NO" }
            ),
        ];
        if let Some(preserved) = &self.preserved {
            lines.push(format!("  Previous copy: {preserved}"));
            lines.push(
                "  Rollback:      stop Orbit, move the preserved copy back, and restart."
                    .to_owned(),
            );
        }
        if let Some(journal) = &self.journal {
            lines.push(format!("  Journal:       {journal}"));
        }
        if self.mode == "isolated" {
            lines.push(String::new());
            lines.push(format!(
                "Open the restored workspace with: orbit --db {} status",
                self.database
            ));
        }
        lines.join("\n")
    }
}
