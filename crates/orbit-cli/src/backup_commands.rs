//! `orbit backup` — create, list, inspect and verify workspace backups.
//!
//! Transport only: every decision is delegated to `orbit_backup`, the same
//! shared service the Web GUI uses. The CLI never copies databases or SQLite
//! sidecars itself.

use std::path::{Path, PathBuf};

use clap::{Args, Subcommand, ValueEnum};
use serde::Serialize;

use orbit_backup::{
    BackupConfigSnapshot, BackupError, BackupListEntry, BackupOptions, BackupService,
    BackupSourceMode, VerifyOptions, WorkspaceLayout, create::default_archive_name,
    inspect_archive, list_backups, verify_archive,
};
use orbit_client::config::Resolved;
use orbit_client::deployment::Deployment;

use crate::CliError;
use crate::backup_restore_commands::{RestoreArgs, run_restore_request};

/// `orbit backup <subcommand>`.
#[derive(Debug, Args)]
pub struct BackupArgs {
    #[command(subcommand)]
    pub command: BackupCommand,
}

/// One `orbit backup` subcommand.
#[derive(Debug, Subcommand)]
pub enum BackupCommand {
    /// Create one complete, verified workspace backup.
    Create(CreateArgs),
    /// List known backups in a directory.
    List(ListArgs),
    /// Show a backup's manifest and contents without verifying it.
    Inspect(InspectArgs),
    /// Verify an archive's structure, hashes, SQLite integrity and coverage.
    Verify(VerifyArgs),
    /// Restore an archive into an isolated location or replace a workspace.
    Restore(RestoreArgs),
    /// Configure the optional automatic-backup schedule and retention policy.
    Schedule(crate::backup_policy_commands::ScheduleArgs),
    /// Apply the stored retention policy to local backups.
    Retention(crate::backup_policy_commands::RetentionArgs),
}

/// Which authority produced the workspace being backed up.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum SourceModeArg {
    /// A local-only workspace database.
    Local,
    /// A self-hosted server database.
    Server,
    /// A connected client replica.
    Replica,
}

impl From<SourceModeArg> for BackupSourceMode {
    fn from(value: SourceModeArg) -> Self {
        match value {
            SourceModeArg::Local => Self::Local,
            SourceModeArg::Server => Self::Server,
            SourceModeArg::Replica => Self::Replica,
        }
    }
}

/// Arguments for `orbit backup create`.
#[derive(Debug, Args)]
pub struct CreateArgs {
    /// Destination directory (default: the configured Orbit backups directory).
    #[arg(long)]
    pub output: Option<PathBuf>,
    /// Optional human label recorded in the manifest and file name.
    #[arg(long)]
    pub label: Option<String>,
    /// Authority mode of the workspace being backed up.
    #[arg(long, value_enum, default_value_t = SourceModeArg::Local)]
    pub source_mode: SourceModeArg,
    /// Omit the normalized non-secret configuration snapshot.
    #[arg(long, default_value_t = false)]
    pub no_config: bool,
}

/// Arguments for `orbit backup list`.
#[derive(Debug, Args)]
pub struct ListArgs {
    /// Directory to scan (default: the configured Orbit backups directory).
    #[arg(long)]
    pub output: Option<PathBuf>,
    /// At most this many entries, newest first.
    #[arg(long)]
    pub limit: Option<usize>,
}

/// Arguments for `orbit backup inspect`.
#[derive(Debug, Args)]
pub struct InspectArgs {
    /// Path to a `.orbitbak` archive.
    pub file: PathBuf,
}

/// Arguments for `orbit backup verify`.
#[derive(Debug, Args)]
pub struct VerifyArgs {
    /// Path to a `.orbitbak` archive.
    pub file: PathBuf,
    /// Skip the SQLite integrity/schema extraction check.
    #[arg(long, default_value_t = false)]
    pub structural_only: bool,
}

/// Run one backup subcommand and render its output.
pub fn run(
    command: BackupCommand,
    deployment: &Deployment,
    db_path: &Path,
    resolved: Option<&Resolved>,
    json: bool,
) -> Result<String, CliError> {
    match command {
        BackupCommand::Create(args) => run_create(args, deployment, db_path, resolved, json),
        BackupCommand::List(args) => run_list(args, resolved, json),
        BackupCommand::Inspect(args) => run_inspect(args, json),
        BackupCommand::Verify(args) => run_verify(args, json),
        BackupCommand::Restore(args) => {
            run_restore_request(args, deployment, db_path, resolved, json)
        }
        BackupCommand::Schedule(args) => {
            crate::backup_policy_commands::run_schedule(args, resolved, json)
        }
        BackupCommand::Retention(args) => {
            crate::backup_policy_commands::run_retention(args, resolved, json)
        }
    }
}

fn run_create(
    args: CreateArgs,
    deployment: &Deployment,
    db_path: &Path,
    resolved: Option<&Resolved>,
    json: bool,
) -> Result<String, CliError> {
    let mut source_mode: BackupSourceMode = args.source_mode.into();
    match deployment {
        Deployment::Connected { .. } => {
            // A connected CLI knows only its replica. It must be told explicitly
            // what it is backing up, because a replica is not an authority.
            if source_mode == BackupSourceMode::Local {
                return Err(CliError::Usage(
                    "connected deployments have no local authority database; \
                     run `orbit backup create --source-mode server` on the server host \
                     (with the server workspace as --db) or `--source-mode replica` \
                     for a replica snapshot"
                        .to_owned(),
                ));
            }
            if source_mode == BackupSourceMode::Server {
                return Err(CliError::Usage(
                    "a connected client cannot snapshot the server database remotely; \
                     run the backup on the server host with `--source-mode server`"
                        .to_owned(),
                ));
            }
        }
        Deployment::Local { .. } => {
            if source_mode == BackupSourceMode::Replica {
                return Err(CliError::Usage(
                    "--source-mode replica requires a connected deployment".to_owned(),
                ));
            }
        }
    }
    if source_mode == BackupSourceMode::Replica {
        source_mode = BackupSourceMode::Replica;
    }
    let layout = WorkspaceLayout::for_database(db_path).map_err(map_error)?;
    layout.require_database().map_err(map_error)?;
    let destination = backup_directory(args.output.as_deref(), resolved, &layout)?;
    let options = BackupOptions {
        source_mode,
        label: args.label.clone(),
        include_config: !args.no_config,
        config: if args.no_config {
            None
        } else {
            resolved.map(config_snapshot)
        },
        ..BackupOptions::default()
    };
    let service = BackupService::new(layout.clone());
    let mut progress = |event: &orbit_backup::BackupProgress| {
        if !json {
            eprintln!("{}", progress_line(event));
        }
    };
    let outcome = service
        .create(&destination, options, &mut progress)
        .map_err(map_error)?;
    let response = CreateResponse {
        r#type: "backup_created",
        backup: outcome.archive.display().to_string(),
        created_at: outcome.manifest.created_at.clone(),
        label: outcome.manifest.label.clone(),
        source: outcome.manifest.source.described_as.clone(),
        source_mode: outcome.manifest.source.mode.as_str(),
        workspace_id: outcome.manifest.source.workspace_id.clone(),
        revision: outcome.manifest.source.revision,
        schema_version: outcome.manifest.schema_version,
        format_version: outcome.manifest.format_version,
        components: component_names(&outcome.manifest),
        database_bytes: outcome.manifest.totals.database_bytes,
        knowledge_bytes: outcome.manifest.totals.knowledge_bytes,
        attachment_bytes: outcome.manifest.totals.attachment_bytes,
        archive_bytes: outcome.manifest.totals.archive_bytes,
        verified: outcome.verification.ok,
        verification_summary: outcome.verification.summary(),
        elapsed_ms: outcome.elapsed_ms,
    };
    if json {
        return to_json(&response);
    }
    Ok(response.human())
}

fn run_list(args: ListArgs, resolved: Option<&Resolved>, json: bool) -> Result<String, CliError> {
    let directory = match args.output {
        Some(directory) => directory,
        None => default_backup_directory(resolved),
    };
    let listing = list_backups(&directory, args.limit).map_err(map_error)?;
    if json {
        return to_json(&listing);
    }
    let mut lines = Vec::new();
    lines.push(format!("Backups in {}", listing.directory));
    if listing.backups.is_empty() {
        lines.push("No backups found.".to_owned());
    } else {
        lines.push(format!(
            "{:<24}  {:<11}  {:>10}  {:<12}  {}",
            "CREATED", "FORMAT", "SIZE", "VERIFIED", "LABEL / FILE"
        ));
        for entry in &listing.backups {
            lines.push(render_list_row(entry));
        }
    }
    if !listing.skipped.is_empty() {
        lines.push(String::new());
        lines.push(format!("{} unreadable file(s):", listing.skipped.len()));
        for skipped in &listing.skipped {
            lines.push(format!("  {} — {}", skipped.path, skipped.reason));
        }
    }
    Ok(lines.join("\n"))
}

fn run_inspect(args: InspectArgs, json: bool) -> Result<String, CliError> {
    let inspection = inspect_archive(&args.file).map_err(map_error)?;
    if json {
        return to_json(&inspection);
    }
    let manifest = &inspection.manifest;
    let mut lines = vec![
        format!("Archive: {}", inspection.archive_path),
        format!("Format:  {}", manifest.format_version),
        format!("Created: {}", manifest.created_at),
        format!("Label:   {}", manifest.label.as_deref().unwrap_or("(none)")),
        format!(
            "Source:  {} ({})",
            manifest.source.described_as,
            manifest.source.mode.as_str()
        ),
        format!(
            "Revision: {}",
            manifest
                .source
                .revision
                .map(|value| value.to_string())
                .unwrap_or_else(|| "not recorded".to_owned())
        ),
        format!("Schema:  {}", manifest.schema_version),
        format!("Orbit:   {}", manifest.orbit_version),
        format!("Complete: {}", manifest.complete),
        String::new(),
        "Components:".to_owned(),
        format!(
            "  database              {:>12} bytes",
            manifest.totals.database_bytes
        ),
        format!(
            "  knowledge vault       {:>12} bytes  ({} note(s), {} folder(s))",
            manifest.totals.knowledge_bytes,
            manifest.counts.knowledge_notes,
            manifest.counts.knowledge_folders
        ),
        format!(
            "  attachments           {:>12} bytes  ({} object(s))",
            manifest.totals.attachment_bytes, manifest.counts.attachment_objects
        ),
        format!(
            "  configuration         {:>12} bytes",
            manifest.totals.config_bytes
        ),
        format!(
            "  archive total         {:>12} bytes  ({} file(s))",
            inspection.archive_bytes, inspection.inventory_files
        ),
    ];
    if !manifest.counts.tables.is_empty() {
        lines.push(String::new());
        lines.push("Records in the snapshot:".to_owned());
        for (table, count) in &manifest.counts.tables {
            lines.push(format!("  {table:<28} {count}"));
        }
    }
    if !manifest.exclusions.is_empty() {
        lines.push(String::new());
        lines.push("Excluded from this backup:".to_owned());
        for exclusion in &manifest.exclusions {
            lines.push(format!("  {}: {}", exclusion.category, exclusion.detail));
        }
    }
    if let Some(sidecar) = &inspection.creation_verification {
        lines.push(String::new());
        lines.push(format!(
            "Verified at creation: {} ({} file(s))",
            if sidecar.ok { "yes" } else { "no" },
            sidecar.verified_files
        ));
    }
    Ok(lines.join("\n"))
}

fn run_verify(args: VerifyArgs, json: bool) -> Result<String, CliError> {
    let options = VerifyOptions {
        deep: !args.structural_only,
    };
    let report = verify_archive(&args.file, &options).map_err(map_error)?;
    let rendered = if json {
        to_json(&report)?
    } else {
        render_verification(&args.file, &report)
    };
    if report.ok {
        Ok(rendered)
    } else {
        // A failed verification is a nonzero exit that still prints the report.
        Err(CliError::Diagnostics(rendered))
    }
}

fn render_verification(path: &Path, report: &orbit_backup::VerificationReport) -> String {
    let mut lines = vec![
        format!("Verifying {}", path.display()),
        format!("Result: {}", report.summary()),
    ];
    for check in &report.checks {
        let status = match check.status {
            orbit_backup::CheckStatus::Pass => "pass",
            orbit_backup::CheckStatus::Fail => "FAIL",
            orbit_backup::CheckStatus::Skipped => "skip",
        };
        lines.push(format!("  [{status}] {}: {}", check.name, check.detail));
    }
    if !report.problems.is_empty() {
        lines.push(String::new());
        lines.push("Problems:".to_owned());
        for problem in &report.problems {
            lines.push(format!("  {}: {}", problem.check, problem.detail));
        }
    }
    if !report.warnings.is_empty() {
        lines.push(String::new());
        lines.push("Warnings:".to_owned());
        for warning in &report.warnings {
            lines.push(format!("  {warning}"));
        }
    }
    lines.join("\n")
}

fn render_list_row(entry: &BackupListEntry) -> String {
    let label = entry
        .label
        .as_deref()
        .map(|label| label.to_owned())
        .unwrap_or_else(|| entry.file_name.clone());
    let verified = match entry.verification {
        orbit_backup::VerificationStatus::Verified => "verified",
        orbit_backup::VerificationStatus::Failed => "FAILED",
        orbit_backup::VerificationStatus::Unknown => "unknown",
    };
    format!(
        "{:<24}  {:<11}  {:>10}  {:<12}  {}",
        entry.created_at,
        format!("v{}", entry.format_version),
        human_bytes(entry.size_bytes),
        verified,
        label
    )
}

fn progress_line(progress: &orbit_backup::BackupProgress) -> String {
    match progress {
        orbit_backup::BackupProgress::Fencing => "Fencing workspace writers...".to_owned(),
        orbit_backup::BackupProgress::DatabaseSnapshotted { bytes } => {
            format!("Database snapshot: {} bytes", bytes)
        }
        orbit_backup::BackupProgress::VaultStaged { files, directories } => {
            format!("Vault staged: {files} file(s), {directories} folder(s)")
        }
        orbit_backup::BackupProgress::AttachmentsStaged { objects } => {
            format!("Attachments staged: {objects} object(s)")
        }
        orbit_backup::BackupProgress::FenceReleased => "Writer fence released.".to_owned(),
        orbit_backup::BackupProgress::WritingArchive { content_bytes } => {
            format!(
                "Writing archive: {} of content...",
                human_bytes(*content_bytes)
            )
        }
        orbit_backup::BackupProgress::VerifyingArchive => {
            "Verifying assembled archive...".to_owned()
        }
        orbit_backup::BackupProgress::Published {
            path,
            archive_bytes,
        } => format!(
            "Published {} ({})",
            path.display(),
            human_bytes(*archive_bytes)
        ),
    }
}

fn default_backup_directory(resolved: Option<&Resolved>) -> PathBuf {
    resolved
        .map(|resolved| resolved.paths.backups_dir.clone())
        .unwrap_or_else(|| PathBuf::from("backups"))
}

fn backup_directory(
    explicit: Option<&Path>,
    resolved: Option<&Resolved>,
    layout: &WorkspaceLayout,
) -> Result<PathBuf, CliError> {
    if let Some(explicit) = explicit {
        return Ok(explicit.to_path_buf());
    }
    // Without a resolved home layout (explicit --db in a home-less process),
    // keep the backups beside the workspace instead of guessing $HOME.
    match resolved {
        Some(resolved) => Ok(resolved.paths.backups_dir.clone()),
        None => Ok(layout.parent().join("backups")),
    }
}

fn config_snapshot(resolved: &Resolved) -> BackupConfigSnapshot {
    BackupConfigSnapshot {
        config_version: resolved.config.version,
        authority_mode: resolved.config.mode.authority.as_str().to_owned(),
        default_agent_provider: resolved
            .config
            .agent
            .as_ref()
            .and_then(|agent| agent.default_provider.clone()),
        connected_server: resolved
            .config
            .connected
            .as_ref()
            .map(|connected| connected.server.clone()),
    }
}

fn component_names(manifest: &orbit_backup::BackupManifest) -> Vec<String> {
    let mut components = vec!["database".to_owned()];
    if manifest.counts.knowledge_notes > 0 || !manifest.directories.is_empty() {
        components.push("knowledge".to_owned());
    }
    if manifest.counts.attachment_objects > 0 {
        components.push("attachments".to_owned());
    }
    if manifest.config.is_some() {
        components.push("configuration".to_owned());
    }
    components
}

fn human_bytes(bytes: u64) -> String {
    const KIB: f64 = 1024.0;
    let value = bytes as f64;
    if value >= KIB * KIB * KIB {
        format!("{:.1} GiB", value / (KIB * KIB * KIB))
    } else if value >= KIB * KIB {
        format!("{:.1} MiB", value / (KIB * KIB))
    } else if value >= KIB {
        format!("{:.1} KiB", value / KIB)
    } else {
        format!("{bytes} B")
    }
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
struct CreateResponse {
    r#type: &'static str,
    backup: String,
    created_at: String,
    label: Option<String>,
    source: String,
    source_mode: &'static str,
    workspace_id: Option<String>,
    revision: Option<i64>,
    schema_version: i64,
    format_version: u32,
    components: Vec<String>,
    database_bytes: u64,
    knowledge_bytes: u64,
    attachment_bytes: u64,
    archive_bytes: u64,
    verified: bool,
    verification_summary: String,
    elapsed_ms: u64,
}

impl CreateResponse {
    fn human(&self) -> String {
        let mut lines = vec![
            format!("Backup created: {}", self.backup),
            format!("Created:        {}", self.created_at),
            format!(
                "Label:          {}",
                self.label.as_deref().unwrap_or("(none)")
            ),
            format!("Source:         {} ({})", self.source, self.source_mode),
            format!(
                "Revision:       {}",
                self.revision
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "not recorded".to_owned())
            ),
            format!("Schema:         {}", self.schema_version),
            format!("Format:         v{}", self.format_version),
            format!("Components:     {}", self.components.join(", ")),
            format!(
                "Content:        database {}, vault {}, attachments {}",
                human_bytes(self.database_bytes),
                human_bytes(self.knowledge_bytes),
                human_bytes(self.attachment_bytes)
            ),
            format!("Archive size:   {}", human_bytes(self.archive_bytes)),
            format!(
                "Verification:   {}",
                if self.verified { "passed" } else { "FAILED" }
            ),
            format!("Details:        {}", self.verification_summary),
            format!("Duration:       {} ms", self.elapsed_ms),
        ];
        // A backup file name is only useful when it is printable; the default
        // name is derived from the label.
        if self.label.is_none() {
            lines.push(format!(
                "Hint:           add --label to name future backups, e.g. {}",
                default_archive_name(orbit_backup::now_ms(), Some("before agent testing"))
            ));
        }
        lines.join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn human_bytes_is_readable() {
        assert_eq!(human_bytes(512), "512 B");
        assert_eq!(human_bytes(2048), "2.0 KiB");
        assert_eq!(human_bytes(5 * 1024 * 1024), "5.0 MiB");
    }

    #[test]
    fn help_text_documents_all_backup_commands() {
        use clap::CommandFactory;
        let command = crate::Cli::command();
        let backup = command
            .get_subcommands()
            .find(|subcommand| subcommand.get_name() == "backup")
            .expect("backup subcommand must exist");
        let names: Vec<_> = backup
            .get_subcommands()
            .map(|subcommand| subcommand.get_name())
            .collect();
        for expected in ["create", "list", "inspect", "verify", "restore"] {
            assert!(
                names.contains(&expected),
                "backup {expected} must be documented: {names:?}"
            );
        }
    }
}
