//! `orbit backup schedule` / `orbit backup retention` — automatic backup policy.
//!
//! Transport only: the policy schema and retention rules live in
//! `orbit_backup::policy` and `orbit_backup::retention`. This module resolves
//! the backup directory the same way `orbit backup create`/`list` do and
//! renders the shared results as JSON or human text.

use std::path::{Path, PathBuf};

use clap::{Args, Subcommand, ValueEnum};
use serde::Serialize;

use orbit_backup::policy::BackupPolicy;
use orbit_backup::{BackupError, BackupSchedule};
use orbit_client::config::Resolved;

use crate::CliError;

/// `orbit backup schedule <subcommand>`.
#[derive(Debug, Args)]
pub struct ScheduleArgs {
    #[command(subcommand)]
    pub command: ScheduleCommand,
}

/// Schedule policy operations.
#[derive(Debug, Subcommand)]
pub enum ScheduleCommand {
    /// Show the stored automatic-backup and retention policy.
    Show(ScheduleTargetArgs),
    /// Update the policy.
    Set(SetArgs),
}

/// Arguments that only need the backup directory.
#[derive(Debug, Args)]
pub struct ScheduleTargetArgs {
    /// Backup directory (default: the configured Orbit backups directory).
    #[arg(long)]
    pub output: Option<PathBuf>,
}

/// Arguments for `orbit backup schedule set`.
#[derive(Debug, Args)]
pub struct SetArgs {
    /// Backup directory (default: the configured Orbit backups directory).
    #[arg(long)]
    pub output: Option<PathBuf>,
    /// Automatic backup schedule.
    #[arg(long, value_enum)]
    pub mode: Option<ScheduleModeArg>,
    /// Maximum number of local backups to keep.
    #[arg(long)]
    pub max_retained: Option<usize>,
    /// Minimum age in days before a backup may be removed.
    #[arg(long)]
    pub min_retention_days: Option<u32>,
    /// Maximum total bytes of local backups.
    #[arg(long)]
    pub max_total_bytes: Option<u64>,
}

/// Schedule choices accepted on the command line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ScheduleModeArg {
    Disabled,
    Daily,
    Weekly,
    BeforeUpgrade,
}

impl From<ScheduleModeArg> for BackupSchedule {
    fn from(value: ScheduleModeArg) -> Self {
        match value {
            ScheduleModeArg::Disabled => Self::Disabled,
            ScheduleModeArg::Daily => Self::Daily,
            ScheduleModeArg::Weekly => Self::Weekly,
            ScheduleModeArg::BeforeUpgrade => Self::BeforeUpgrade,
        }
    }
}

/// `orbit backup retention <subcommand>`.
#[derive(Debug, Args)]
pub struct RetentionArgs {
    #[command(subcommand)]
    pub command: RetentionCommand,
}

/// Retention operations.
#[derive(Debug, Subcommand)]
pub enum RetentionCommand {
    /// Apply the stored retention policy now.
    Apply(RetentionApplyArgs),
}

/// Arguments for `orbit backup retention apply`.
#[derive(Debug, Args)]
pub struct RetentionApplyArgs {
    /// Backup directory (default: the configured Orbit backups directory).
    #[arg(long)]
    pub output: Option<PathBuf>,
    /// Report what would be removed without deleting anything.
    #[arg(long, default_value_t = false)]
    pub dry_run: bool,
}

/// Run a schedule command.
pub fn run_schedule(
    args: ScheduleArgs,
    resolved: Option<&Resolved>,
    json: bool,
) -> Result<String, CliError> {
    match args.command {
        ScheduleCommand::Show(target) => show_schedule(target, resolved, json),
        ScheduleCommand::Set(set) => set_schedule(set, resolved, json),
    }
}

fn show_schedule(
    args: ScheduleTargetArgs,
    resolved: Option<&Resolved>,
    json: bool,
) -> Result<String, CliError> {
    let directory = backup_directory_for(args.output.as_deref(), resolved)?;
    let policy = load_policy(&directory)?;
    let response = ScheduleResponse {
        r#type: "backup_schedule",
        changed: Vec::new(),
        directory: directory.display().to_string(),
        policy_path: policy_path(&directory).display().to_string(),
        policy: &policy,
    };
    render_schedule(&response, json)
}

fn set_schedule(
    args: SetArgs,
    resolved: Option<&Resolved>,
    json: bool,
) -> Result<String, CliError> {
    let directory = backup_directory_for(args.output.as_deref(), resolved)?;
    let mut policy = load_policy(&directory)?;
    let mut changed = Vec::new();
    if let Some(mode) = args.mode {
        policy.schedule = mode.into();
        changed.push("schedule");
    }
    if let Some(max_retained) = args.max_retained {
        policy.max_retained = Some(max_retained);
        changed.push("max_retained");
    }
    if let Some(days) = args.min_retention_days {
        policy.min_retention_days = Some(days);
        changed.push("min_retention_days");
    }
    if let Some(bytes) = args.max_total_bytes {
        policy.max_total_bytes = Some(bytes);
        changed.push("max_total_bytes");
    }
    if changed.is_empty() {
        return Err(CliError::Usage(
            "no policy change supplied; pass --mode, --max-retained, --min-retention-days \
             or --max-total-bytes"
                .to_owned(),
        ));
    }
    policy.save(&directory).map_err(map_error)?;
    let response = ScheduleResponse {
        r#type: "backup_schedule",
        changed,
        directory: directory.display().to_string(),
        policy_path: policy_path(&directory).display().to_string(),
        policy: &policy,
    };
    render_schedule(&response, json)
}

fn policy_path(directory: &Path) -> PathBuf {
    directory.join(orbit_backup::POLICY_FILE_NAME)
}

/// Run a retention command.
pub fn run_retention(
    args: RetentionArgs,
    resolved: Option<&Resolved>,
    json: bool,
) -> Result<String, CliError> {
    match args.command {
        RetentionCommand::Apply(apply_args) => apply_retention_command(apply_args, resolved, json),
    }
}

fn apply_retention_command(
    args: RetentionApplyArgs,
    resolved: Option<&Resolved>,
    json: bool,
) -> Result<String, CliError> {
    let directory = backup_directory_for(args.output.as_deref(), resolved)?;
    // An invalid policy file fails closed: retention never guesses limits.
    let policy = load_policy(&directory)?;
    let retention = policy.retention_policy();
    let outcome = orbit_backup::apply_retention(
        &directory,
        &retention,
        orbit_backup::now_ms(),
        None,
        args.dry_run,
    )
    .map_err(map_error)?;
    let response = RetentionResponse {
        r#type: "backup_retention",
        directory: directory.display().to_string(),
        dry_run: args.dry_run,
        policy: &policy,
        outcome: &outcome,
    };
    if json {
        return to_json(&response);
    }
    Ok(response.human())
}

/// Resolve the backup directory for policy commands.
///
/// This mirrors `orbit backup create`/`list`: an explicit `--output` wins over
/// the configured Orbit backups directory.
pub fn backup_directory_for(
    explicit: Option<&Path>,
    resolved: Option<&Resolved>,
) -> Result<PathBuf, CliError> {
    if let Some(explicit) = explicit {
        return Ok(explicit.to_path_buf());
    }
    match resolved {
        Some(resolved) => Ok(resolved.paths.backups_dir.clone()),
        None => Err(CliError::Usage(
            "no Orbit home layout is available; pass --output <dir>".to_owned(),
        )),
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

fn load_policy(directory: &Path) -> Result<BackupPolicy, CliError> {
    BackupPolicy::load(directory).map_err(map_error)
}

fn to_json(value: &impl Serialize) -> Result<String, CliError> {
    serde_json::to_string_pretty(value).map_err(|error| CliError::Serialization(error.to_string()))
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

fn describe_limit<T: ToString>(value: Option<T>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| "not set".to_owned())
}

#[derive(Debug, Serialize)]
struct ScheduleResponse<'a> {
    r#type: &'static str,
    changed: Vec<&'static str>,
    directory: String,
    policy_path: String,
    policy: &'a BackupPolicy,
}

impl ScheduleResponse<'_> {
    fn human(&self) -> String {
        let mut lines = Vec::new();
        if self.changed.is_empty() {
            lines.push("Automatic backup policy".to_owned());
        } else {
            lines.push("Automatic backup policy updated".to_owned());
            lines.push(format!("Changed: {}", self.changed.join(", ")));
        }
        lines.push(format!("Backup directory: {}", self.directory));
        lines.push(format!("Policy file:      {}", self.policy_path));
        lines.push(format!(
            "Schedule:         {}",
            self.policy.schedule.as_str()
        ));
        lines.push(format!(
            "Max retained:     {}",
            describe_limit(self.policy.max_retained)
        ));
        lines.push(format!(
            "Min retention:    {}",
            self.policy
                .min_retention_days
                .map(|days| format!("{days} day(s)"))
                .unwrap_or_else(|| "not set".to_owned())
        ));
        lines.push(format!(
            "Max total bytes:  {}",
            self.policy
                .max_total_bytes
                .map(human_bytes)
                .unwrap_or_else(|| "not set".to_owned())
        ));
        lines.push(
            "Retention is applied only after a backup is created and verified; the newest \
             verified recovery point is always kept."
                .to_owned(),
        );
        lines.join("\n")
    }
}

fn render_schedule(response: &ScheduleResponse<'_>, json: bool) -> Result<String, CliError> {
    if json {
        to_json(response)
    } else {
        Ok(response.human())
    }
}

#[derive(Debug, Serialize)]
struct RetentionResponse<'a> {
    r#type: &'static str,
    directory: String,
    dry_run: bool,
    policy: &'a BackupPolicy,
    outcome: &'a orbit_backup::RetentionOutcome,
}

impl RetentionResponse<'_> {
    fn human(&self) -> String {
        let outcome = self.outcome;
        let verb = if self.dry_run {
            "Retention dry run on"
        } else {
            "Retention applied to"
        };
        let mut lines = vec![
            format!("{verb} {}", self.directory),
            format!("Schedule:  {}", self.policy.schedule.as_str()),
            format!(
                "Limits:    max_retained {}, min_retention {}, max_total_bytes {}",
                describe_limit(self.policy.max_retained),
                self.policy
                    .min_retention_days
                    .map(|days| format!("{days} day(s)"))
                    .unwrap_or_else(|| "not set".to_owned()),
                self.policy
                    .max_total_bytes
                    .map(human_bytes)
                    .unwrap_or_else(|| "not set".to_owned())
            ),
            format!(
                "Protected: {}",
                outcome.protected.as_deref().unwrap_or("none")
            ),
            format!(
                "Retained:  {} backup(s), {}",
                outcome.retained,
                if self.dry_run {
                    "would be kept".to_owned()
                } else {
                    "kept".to_owned()
                }
            ),
            format!(
                "Bytes:     {} -> {}{}",
                human_bytes(outcome.bytes_before),
                human_bytes(outcome.bytes_after),
                if self.dry_run { " (projected)" } else { "" }
            ),
        ];
        if outcome.removed.is_empty() {
            lines.push(if self.dry_run {
                "No backups would be removed.".to_owned()
            } else {
                "No backups were removed.".to_owned()
            });
        } else {
            lines.push(format!(
                "{} {}:",
                if self.dry_run {
                    "Would remove"
                } else {
                    "Removed"
                },
                outcome.removed.len()
            ));
            for path in &outcome.removed {
                lines.push(format!("  {path}"));
            }
        }
        if !outcome.retained_reasons.is_empty() {
            lines.push("Retention decisions:".to_owned());
            for reason in &outcome.retained_reasons {
                lines.push(format!("  {reason}"));
            }
        }
        lines.join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schedule_mode_values_map_to_the_stable_policy_names() {
        assert_eq!(
            BackupSchedule::from(ScheduleModeArg::Disabled).as_str(),
            "disabled"
        );
        assert_eq!(
            BackupSchedule::from(ScheduleModeArg::Daily).as_str(),
            "daily"
        );
        assert_eq!(
            BackupSchedule::from(ScheduleModeArg::Weekly).as_str(),
            "weekly"
        );
        assert_eq!(
            BackupSchedule::from(ScheduleModeArg::BeforeUpgrade).as_str(),
            "before_upgrade"
        );
    }

    #[test]
    fn schedule_response_renders_human_limits() {
        let policy = BackupPolicy {
            schedule: BackupSchedule::Daily,
            max_retained: Some(10),
            min_retention_days: Some(7),
            max_total_bytes: Some(5 * 1024 * 1024),
            ..BackupPolicy::default()
        };
        let directory = PathBuf::from("/tmp/orbit/backups");
        let response = ScheduleResponse {
            r#type: "backup_schedule",
            changed: vec!["schedule"],
            directory: directory.display().to_string(),
            policy_path: "/tmp/orbit/backups/policy.json".to_owned(),
            policy: &policy,
        };
        let human = response.human();
        assert!(human.contains("Schedule:         daily"));
        assert!(human.contains("7 day(s)"));
        assert!(human.contains("5.0 MiB"));
        assert!(human.contains("newest verified recovery point"));
    }

    #[test]
    fn human_bytes_is_readable() {
        assert_eq!(human_bytes(512), "512 B");
        assert_eq!(human_bytes(2048), "2.0 KiB");
        assert_eq!(human_bytes(5 * 1024 * 1024), "5.0 MiB");
    }
}
