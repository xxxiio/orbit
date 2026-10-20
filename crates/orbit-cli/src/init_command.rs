//! Non-interactive `orbit init`: create the documented home layout.
//!
//! Init never deletes configuration or data. An existing configuration is
//! parsed and retained unless `--force` is supplied; databases are opened with
//! the normal authoritative host so schema creation uses the same code path as
//! every other command.

use std::{
    env, fs,
    io::Write,
    path::{Path, PathBuf},
    process,
    time::{SystemTime, UNIX_EPOCH},
};

use orbit_client::config::{
    AgentConfig, AuthorityMode, ConfigEnv, ConfigError, ConnectedConfig, ModeConfig, OrbitConfig,
    OrbitPaths, StorageConfig,
};
use serde::Serialize;

use crate::{AuthorityArg, CliError, InitArgs};

pub(crate) fn run(
    args: InitArgs,
    db_override: Option<&Path>,
    config_override: Option<&Path>,
    json: bool,
) -> Result<String, CliError> {
    let paths = OrbitPaths::discover(config_override).map_err(config_error)?;

    let existing = OrbitConfig::load(&paths.config_file).map_err(config_error)?;
    let (config, config_action, write_config) = match (existing, args.force) {
        (Some(loaded), false) => {
            // Retaining the file also retains its authority mode, database and
            // agent defaults. Refuse conflicting flags rather than silently
            // ignoring user intent; `--force` is the explicit rewrite path.
            let conflicting = [
                ("--database", args.database.is_some()),
                ("--authority", args.authority.is_some()),
                ("--server", args.server.is_some()),
                ("--replica-database", args.replica_database.is_some()),
                ("--agent-provider", args.agent_provider.is_some()),
            ]
            .into_iter()
            .filter_map(|(flag, supplied)| supplied.then_some(flag))
            .collect::<Vec<_>>();
            if !conflicting.is_empty() {
                return Err(CliError::Usage(format!(
                    "configuration {} already exists; pass --force to rewrite it with {}",
                    paths.config_file.display(),
                    conflicting.join(", ")
                )));
            }
            (loaded.config, "retained", false)
        }
        (_, true) | (None, false) => {
            let config = build_config(&args)?;
            config.validate().map_err(config_error)?;
            (config, "created", true)
        }
    };

    let database = effective_database(&paths, &config, db_override)?;
    let database_existed = database.exists();
    create_layout(&paths)?;
    create_parent(&database)?;

    // Initialise data first so a failure leaves any new configuration file
    // unwritten. Existing databases are opened in place and are never moved.
    orbit_runtime::AuthoritativeHost::open(&database).map_err(|error| {
        CliError::Repository(format!(
            "cannot initialize database {}: {error:?}",
            database.display()
        ))
    })?;

    if write_config {
        write_config_atomically(&paths.config_file, &config.render_toml())?;
    }

    let authority = match config.mode.authority {
        AuthorityMode::Local => "local",
        AuthorityMode::Connected => "connected",
    };
    let report = InitReport {
        kind: "init",
        config_path: display(&paths.config_file),
        config_action,
        database: display(&database),
        database_action: if database_existed {
            "existing"
        } else {
            "created"
        },
        authority,
        logs: display(&paths.logs_dir),
        state: display(&paths.state_dir),
        backups: display(&paths.backups_dir),
        server: config
            .connected
            .as_ref()
            .map(|connected| connected.server.clone()),
        next: if config.mode.authority == AuthorityMode::Connected {
            "orbit sync replica status".into()
        } else {
            "orbit task add \"My first task\"".into()
        },
    };
    if json {
        serde_json::to_string_pretty(&report)
            .map_err(|error| CliError::Serialization(error.to_string()))
    } else {
        Ok(report.human())
    }
}

fn build_config(args: &InitArgs) -> Result<OrbitConfig, CliError> {
    let authority = match args.authority.unwrap_or(AuthorityArg::Local) {
        AuthorityArg::Local => AuthorityMode::Local,
        AuthorityArg::Connected => AuthorityMode::Connected,
    };
    let storage = match &args.database {
        Some(database) => StorageConfig {
            database: Some(absolute_path(database)?.to_string_lossy().into_owned()),
        },
        None => StorageConfig::default(),
    };
    let connected = if authority == AuthorityMode::Connected {
        let server = args.server.as_deref().map(str::trim).unwrap_or("");
        if server.is_empty() {
            return Err(CliError::Usage(
                "--server is required with --authority connected".into(),
            ));
        }
        let replica = match &args.replica_database {
            Some(database) => absolute_path(database)?.to_string_lossy().into_owned(),
            None => format!("~/.orbit/{}", "replica.db"),
        };
        Some(ConnectedConfig {
            server: server.to_owned(),
            replica_database: replica,
        })
    } else {
        None
    };
    let agent = match &args.agent_provider {
        Some(provider) => {
            if provider.trim().is_empty() {
                return Err(CliError::Usage("--agent-provider must not be empty".into()));
            }
            Some(AgentConfig {
                default_provider: Some(provider.trim().to_owned()),
            })
        }
        None => None,
    };
    let config = OrbitConfig {
        version: orbit_client::config::CONFIG_VERSION,
        storage,
        mode: ModeConfig { authority },
        connected,
        agent,
    };
    // The layout is authoritative for default database placement; a caller
    // that supplied no explicit database must not receive a recorded path.
    Ok(config)
}

fn effective_database(
    paths: &OrbitPaths,
    config: &OrbitConfig,
    db_override: Option<&Path>,
) -> Result<PathBuf, CliError> {
    let environment = ConfigEnv::from_env();
    let explicit = match db_override {
        Some(database) => Some(absolute_path(database)?),
        None => environment.database.clone(),
    };
    match config.mode.authority {
        AuthorityMode::Local => {
            if let Some(database) = explicit {
                return Ok(database);
            }
            let storage = match &config.storage.database {
                Some(raw) => Some(
                    orbit_client::config::resolve_config_path_with_home(
                        raw,
                        "storage.database",
                        paths.config_dir(),
                        environment.home.as_deref(),
                    )
                    .map_err(config_error)?,
                ),
                None => None,
            };
            Ok(storage.unwrap_or_else(|| paths.default_database.clone()))
        }
        AuthorityMode::Connected => {
            if let Some(database) = db_override {
                return absolute_path(database);
            }
            if let Some(database) = environment.authority_database {
                if database.trim().is_empty() {
                    return Err(CliError::Usage(
                        "ORBIT_AUTHORITY_DB may not be empty".into(),
                    ));
                }
                return Ok(PathBuf::from(database));
            }
            if let Some(database) = environment.database {
                return Ok(database);
            }
            let connected = config.connected.as_ref().ok_or_else(|| {
                CliError::Usage("connected authority requires [connected] settings".into())
            })?;
            orbit_client::config::resolve_config_path_with_home(
                &connected.replica_database,
                "connected.replica_database",
                paths.config_dir(),
                environment.home.as_deref(),
            )
            .map_err(config_error)
        }
    }
}

fn absolute_path(path: &Path) -> Result<PathBuf, CliError> {
    let text = path.to_string_lossy();
    let expanded = if text == "~" {
        home()
            .ok_or_else(|| CliError::Usage("cannot expand `~`; set HOME or USERPROFILE".into()))?
    } else if let Some(rest) = text.strip_prefix("~/") {
        home()
            .ok_or_else(|| CliError::Usage("cannot expand `~`; set HOME or USERPROFILE".into()))?
            .join(rest)
    } else {
        path.to_path_buf()
    };
    if expanded.is_absolute() {
        Ok(expanded)
    } else {
        Ok(env::current_dir()
            .map_err(|error| CliError::Io(error.to_string()))?
            .join(expanded))
    }
}

fn home() -> Option<PathBuf> {
    env::var_os("HOME")
        .filter(|value| !value.is_empty())
        .or_else(|| env::var_os("USERPROFILE").filter(|value| !value.is_empty()))
        .map(PathBuf::from)
}

fn create_layout(paths: &OrbitPaths) -> Result<(), CliError> {
    for directory in [
        &paths.orbit_dir,
        &paths.logs_dir,
        &paths.state_dir,
        &paths.backups_dir,
    ] {
        refuse_symlink(directory)?;
        fs::create_dir_all(directory).map_err(|error| CliError::Io(error.to_string()))?;
        restrict_directory(directory)?;
    }
    Ok(())
}

fn refuse_symlink(path: &Path) -> Result<(), CliError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(CliError::Io(format!(
            "refusing to follow symlinked Orbit directory {}",
            path.display()
        ))),
        Ok(metadata) if !metadata.is_dir() => Err(CliError::Io(format!(
            "Orbit layout path {} exists but is not a directory",
            path.display()
        ))),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(CliError::Io(error.to_string())),
    }
}

fn create_parent(path: &Path) -> Result<(), CliError> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent).map_err(|error| CliError::Io(error.to_string()))?;
    }
    Ok(())
}

#[cfg(unix)]
fn restrict_directory(path: &Path) -> Result<(), CliError> {
    use std::os::unix::fs::PermissionsExt;
    let mut permissions = fs::metadata(path)
        .map_err(|error| CliError::Io(error.to_string()))?
        .permissions();
    permissions.set_mode(0o700);
    fs::set_permissions(path, permissions).map_err(|error| CliError::Io(error.to_string()))
}

#[cfg(not(unix))]
fn restrict_directory(_path: &Path) -> Result<(), CliError> {
    Ok(())
}

fn write_config_atomically(path: &Path, contents: &str) -> Result<(), CliError> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).map_err(|error| CliError::Io(error.to_string()))?;
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("config.toml");
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let temporary = parent.join(format!(".{file_name}.init-{}-{unique}.tmp", process::id()));
    let result = (|| -> Result<(), CliError> {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|error| CliError::Io(error.to_string()))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            file.set_permissions(fs::Permissions::from_mode(0o600))
                .map_err(|error| CliError::Io(error.to_string()))?;
        }
        file.write_all(contents.as_bytes())
            .map_err(|error| CliError::Io(error.to_string()))?;
        file.sync_all()
            .map_err(|error| CliError::Io(error.to_string()))?;
        drop(file);
        fs::rename(&temporary, path).map_err(|error| CliError::Io(error.to_string()))?;
        #[cfg(unix)]
        {
            if let Ok(directory) = fs::File::open(parent) {
                let _ = directory.sync_all();
            }
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn config_error(error: ConfigError) -> CliError {
    CliError::Usage(format!("configuration error: {error}"))
}

fn display(path: &Path) -> String {
    path.display().to_string()
}

#[derive(Debug, Serialize)]
struct InitReport {
    #[serde(rename = "type")]
    kind: &'static str,
    config_path: String,
    config_action: &'static str,
    database: String,
    database_action: &'static str,
    authority: &'static str,
    logs: String,
    state: String,
    backups: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    server: Option<String>,
    next: String,
}

impl InitReport {
    fn human(&self) -> String {
        let mut lines = vec![
            "Orbit initialized".to_owned(),
            format!("Config: {} ({})", self.config_path, self.config_action),
            format!("Authority: {}", self.authority),
            format!("Database: {} ({})", self.database, self.database_action),
            format!("Logs: {}", self.logs),
            format!("State: {}", self.state),
            format!("Backups: {}", self.backups),
        ];
        if let Some(server) = &self.server {
            lines.push(format!("Server: {server}"));
        }
        lines.push(format!("Next: {}", self.next));
        lines.join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn custom_config_location_keeps_runtime_directories_under_home() {
        let root = tempfile::tempdir().unwrap();
        let home = root.path().join("home");
        fs::create_dir_all(&home).unwrap();
        let custom = root.path().join("elsewhere/config.toml");
        let paths = OrbitPaths::discover_with(Some(&custom), None, Some(&home)).unwrap();
        assert_eq!(paths.config_file, custom);
        assert_eq!(paths.config_dir(), root.path().join("elsewhere"));
        assert_eq!(paths.orbit_dir, home.join(".orbit"));
        assert_eq!(paths.default_database, home.join(".orbit/orbit.db"));
        assert_eq!(paths.logs_dir, home.join(".orbit/logs"));
        assert_eq!(paths.state_dir, home.join(".orbit/state"));
        assert_eq!(paths.backups_dir, home.join(".orbit/backups"));
    }

    #[cfg(unix)]
    #[test]
    fn create_layout_refuses_symlinked_orbit_directories() {
        use std::os::unix::fs::symlink;
        let root = tempfile::tempdir().unwrap();
        let home = root.path().join("home");
        fs::create_dir_all(&home).unwrap();
        let target = root.path().join("target");
        fs::create_dir_all(&target).unwrap();
        let orbit_dir = home.join(".orbit");
        symlink(&target, &orbit_dir).unwrap();
        let paths = OrbitPaths {
            config_file: orbit_dir.join("config.toml"),
            default_database: orbit_dir.join("orbit.db"),
            logs_dir: orbit_dir.join("logs"),
            state_dir: orbit_dir.join("state"),
            backups_dir: orbit_dir.join("backups"),
            orbit_dir,
        };
        assert!(create_layout(&paths).is_err());
        assert!(!target.join("logs").exists());
    }
}
