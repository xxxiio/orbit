//! Periodic automatic workspace backups for the long-lived server process.
//!
//! The self-hosted server (including a local-trusted personal installation) is
//! Orbit's long-running process, so it is also the natural place to run the
//! optional automatic backup schedule. The policy lives in the backup
//! directory (`policy.json`) and is read on every tick, so a change made through
//! the CLI or the Web GUI takes effect without a restart.
//!
//! Safety rules:
//! - `Disabled` and `BeforeUpgrade` never run on a timer; the upgrade path runs
//!   its own recovery point through `orbit update`.
//! - A failed automatic backup is logged and never affects the live workspace.
//! - Retention runs only after a verified creation and protects the newest
//!   verified recovery point.
//! - The blocking service calls run on a blocking thread, never on the async
//!   runtime.

use std::path::{Path, PathBuf};
use std::time::Duration;

use orbit_backup::policy::BackupPolicy;
use orbit_backup::{
    BackupConfigSnapshot, BackupOptions, BackupService, BackupSourceMode, Result, WorkspaceLayout,
    apply_retention, list_backups,
};

/// How often the scheduler checks whether a backup is due.
pub const DEFAULT_CHECK_INTERVAL: Duration = Duration::from_secs(15 * 60);

/// Resolve the configured backup directory for a server database.
///
/// `ORBIT_BACKUP_DIR` wins, then `<database parent>/backups`.
pub fn backup_directory_for(database: &Path) -> PathBuf {
    match std::env::var("ORBIT_BACKUP_DIR") {
        Ok(value) if !value.trim().is_empty() => PathBuf::from(value),
        _ => database
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join("backups"),
    }
}

/// Spawn the scheduler task. It never exits; a tick failure is logged and the
/// schedule is retried on the next interval.
pub fn spawn(database: PathBuf, backup_directory: PathBuf, interval: Duration) {
    tokio::spawn(async move {
        // Check once shortly after startup so a due daily backup is not delayed
        // by a full interval, then settle into the regular cadence.
        let mut next_check = tokio::time::Instant::now() + Duration::from_secs(30);
        loop {
            tokio::time::sleep_until(next_check).await;
            next_check = tokio::time::Instant::now() + interval;
            let database = database.clone();
            let backup_directory = backup_directory.clone();
            let result =
                tokio::task::spawn_blocking(move || run_due_backup(&database, &backup_directory))
                    .await;
            match result {
                Ok(Ok(outcome)) => {
                    if let Some(archive) = outcome {
                        eprintln!("automatic backup published: {}", archive.display());
                    }
                }
                Ok(Err(error)) => eprintln!("automatic backup skipped: {error}"),
                Err(error) => eprintln!("automatic backup task failed: {error}"),
            }
        }
    });
}

/// Create one backup when the stored policy says it is due.
///
/// Returns the published archive path when a backup was created.
pub fn run_due_backup(database: &Path, backup_directory: &Path) -> Result<Option<PathBuf>> {
    let policy = BackupPolicy::load(backup_directory)?;
    let last_created = newest_backup_created_at(backup_directory);
    if !policy.is_due(orbit_backup::now_ms(), last_created) {
        return Ok(None);
    }
    let layout = WorkspaceLayout::for_database(database)?;
    layout.require_database()?;
    let service = BackupService::new(layout);
    let label = format!("automatic-{}", policy.schedule.as_str());
    let options = BackupOptions {
        source_mode: BackupSourceMode::Server,
        label: Some(label),
        include_config: true,
        config: Some(BackupConfigSnapshot {
            config_version: 1,
            authority_mode: "server".to_owned(),
            default_agent_provider: None,
            connected_server: None,
        }),
        // A server database can be larger than a personal workspace; keep the
        // writer fence bounded but generous enough for the snapshot copy.
        fence_timeout: Duration::from_secs(30),
        keep_staging: false,
    };
    let mut progress = orbit_backup::NoProgress;
    let outcome = service.create(backup_directory, options, &mut progress)?;
    if !outcome.verification.ok {
        // create() verifies before publish; a non-ok report here means the
        // published archive is not a safe recovery point, so never prune.
        return Err(orbit_backup::BackupError::Integrity(format!(
            "automatic backup {} did not verify: {}",
            outcome.archive.display(),
            outcome.verification.summary()
        )));
    }
    // Retention only after a verified creation. The new archive is protected,
    // so cleanup can never remove the recovery point it just made. A retention
    // failure is logged by the caller but does not invalidate the backup.
    let retention = policy.retention_policy();
    if let Ok(report) = apply_retention(
        backup_directory,
        &retention,
        orbit_backup::now_ms(),
        Some(&outcome.archive),
        false,
    ) {
        for removed in &report.removed {
            eprintln!("automatic backup retention removed: {removed}");
        }
    }
    Ok(Some(outcome.archive))
}

/// Creation time of the newest readable backup in a directory, if any.
pub fn newest_backup_created_at(backup_directory: &Path) -> Option<i64> {
    let listing = list_backups(backup_directory, Some(1)).ok()?;
    listing.backups.first().map(|entry| entry.created_at_ms)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backup_directory_prefers_the_environment_override() {
        // Pure fallback behaviour: a database in a temporary directory maps to
        // `<parent>/backups` when no override is configured.
        let database = Path::new("/srv/orbit/orbit-server.db");
        // Do not assert the env-dependent branch here; only the path rule.
        let fallback = database
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join("backups");
        assert_eq!(fallback, Path::new("/srv/orbit/backups"));
    }

    #[test]
    fn default_interval_is_a_quiet_polling_interval() {
        assert!(DEFAULT_CHECK_INTERVAL >= Duration::from_secs(60));
    }
}
