//! Deployment selection shared by CLI, terminal UI and headless MCP.
//!
//! A configured connected deployment must never silently fall back to a
//! local authoritative database. Local-only mode remains fully independent.
use std::{
    env,
    path::{Path, PathBuf},
};

use crate::config::{self, ConfigEnv, CoreResolution};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Deployment {
    Local { database: PathBuf },
    Connected { replica: PathBuf, server: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeploymentError {
    MissingReplica,
    MissingServer,
    ConflictingServers,
    SameDatabase,
    EmptyValue(&'static str),
    Config(String),
}

impl std::fmt::Display for DeploymentError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingReplica => write!(f, "connected mode requires ORBIT_AUTHORITY_DB"),
            Self::MissingServer => write!(
                f,
                "connected mode requires ORBIT_AUTHORITY_SERVER or ORBIT_AUTHORITY_URL"
            ),
            Self::ConflictingServers => {
                write!(f, "ORBIT_AUTHORITY_SERVER and ORBIT_AUTHORITY_URL disagree")
            }
            Self::SameDatabase => write!(
                f,
                "authoritative database and replica must be different files"
            ),
            Self::EmptyValue(var) => write!(f, "{var} may not be empty"),
            Self::Config(message) => write!(f, "{message}"),
        }
    }
}

fn setting(name: &'static str) -> Result<Option<String>, DeploymentError> {
    match env::var(name) {
        Ok(value) if value.trim().is_empty() => Err(DeploymentError::EmptyValue(name)),
        Ok(value) => Ok(Some(value)),
        Err(env::VarError::NotPresent) => Ok(None),
        Err(env::VarError::NotUnicode(_)) => Err(DeploymentError::EmptyValue(name)),
    }
}

/// Pure configuration resolver, independently testable without global env.
/// `local_db` is only used for standalone local authority.
pub fn resolve(
    local_db: impl AsRef<Path>,
    replica: Option<&str>,
    server: Option<&str>,
    server_alias: Option<&str>,
) -> Result<Deployment, DeploymentError> {
    fn trim(value: &str) -> &str {
        value.trim()
    }
    for (name, value) in [
        ("ORBIT_AUTHORITY_DB", replica),
        ("ORBIT_AUTHORITY_SERVER", server),
        ("ORBIT_AUTHORITY_URL", server_alias),
    ] {
        if value.is_some_and(|value| trim(value).is_empty()) {
            return Err(DeploymentError::EmptyValue(name));
        }
    }
    if let (Some(a), Some(b)) = (server, server_alias)
        && trim(a) != trim(b)
    {
        return Err(DeploymentError::ConflictingServers);
    }
    let remote = server.or(server_alias);
    match (replica, remote) {
        (None, None) => Ok(Deployment::Local {
            database: local_db.as_ref().to_path_buf(),
        }),
        (None, Some(_)) => Err(DeploymentError::MissingReplica),
        (Some(_), None) => Err(DeploymentError::MissingServer),
        (Some(replica), Some(server)) => {
            let path = PathBuf::from(replica);
            // This is an early error, supplementing the SQLite schema guards.
            // Never use a relative-path comparison as a substitute for the
            // schema checks: aliases and symlinks can denote the same file.
            if path == local_db.as_ref() {
                return Err(DeploymentError::SameDatabase);
            }
            Ok(Deployment::Connected {
                replica: path,
                server: server.to_owned(),
            })
        }
    }
}

/// Resolve config and environment together. CLI `--db` wins over `ORBIT_DB`,
/// which wins over the config file, which wins over the default database.
pub fn resolve_deployment(
    local_override: Option<&Path>,
    config_override: Option<&Path>,
) -> Result<Deployment, DeploymentError> {
    resolve_with_env(local_override, config_override, &ConfigEnv::from_env())
}

/// Environment-independent deployment resolver used by the unit tests.
pub fn resolve_with_env(
    local_override: Option<&Path>,
    config_override: Option<&Path>,
    environment: &ConfigEnv,
) -> Result<Deployment, DeploymentError> {
    let core = config::resolve_core(local_override, config_override, environment, true)
        .map_err(|error| DeploymentError::Config(error.to_string()))?;
    resolve_from_core(&core, environment, local_override)
}

pub(crate) fn resolve_from_core(
    core: &CoreResolution,
    environment: &ConfigEnv,
    local_override: Option<&Path>,
) -> Result<Deployment, DeploymentError> {
    for (name, value) in [
        (
            "ORBIT_AUTHORITY_DB",
            environment.authority_database.as_deref(),
        ),
        (
            "ORBIT_AUTHORITY_SERVER",
            environment.authority_server.as_deref(),
        ),
        ("ORBIT_AUTHORITY_URL", environment.authority_url.as_deref()),
    ] {
        if value.is_some_and(|value| value.trim().is_empty()) {
            return Err(DeploymentError::EmptyValue(name));
        }
    }
    if environment
        .database
        .as_ref()
        .is_some_and(|path| path.as_os_str().is_empty())
    {
        return Err(DeploymentError::EmptyValue("ORBIT_DB"));
    }
    let server = environment.authority_server.as_deref();
    let alias = environment.authority_url.as_deref();
    if let (Some(a), Some(b)) = (server, alias)
        && a.trim() != b.trim()
    {
        return Err(DeploymentError::ConflictingServers);
    }
    let remote = server
        .or(alias)
        .map(str::trim)
        .filter(|value| !value.is_empty());

    match core.config.mode.authority {
        crate::config::AuthorityMode::Connected => {
            let server = remote
                .map(ToOwned::to_owned)
                .or_else(|| {
                    core.config
                        .connected
                        .as_ref()
                        .map(|connected| connected.server.clone())
                })
                .ok_or(DeploymentError::MissingServer)?;
            let replica = local_override
                .map(Path::to_path_buf)
                .or_else(|| environment.authority_database.as_ref().map(PathBuf::from))
                .or_else(|| environment.database.clone())
                .or_else(|| core.replica_database.clone())
                .ok_or(DeploymentError::MissingReplica)?;
            // In connected mode there is no separate local authority file, so
            // the replica may legitimately equal an explicit ORBIT_DB override.
            Ok(Deployment::Connected { replica, server })
        }
        crate::config::AuthorityMode::Local => {
            // Local mode with neither a server nor a replica override is a
            // standalone deployment. In environment-selected connected mode,
            // `--db` may name the replica (the documented connected-mode
            // behavior); the local authority candidate is compared without
            // that override.
            if environment.authority_database.is_none() && remote.is_none() {
                return Ok(Deployment::Local {
                    database: core.local_database.clone(),
                });
            }
            let replica = local_override
                .map(Path::to_path_buf)
                .or_else(|| environment.authority_database.as_ref().map(PathBuf::from));
            match (replica, remote) {
                (None, Some(_)) => Err(DeploymentError::MissingReplica),
                (Some(_), None) => Err(DeploymentError::MissingServer),
                (Some(replica), Some(server)) => {
                    if replica == core.authority_database {
                        return Err(DeploymentError::SameDatabase);
                    }
                    Ok(Deployment::Connected {
                        replica,
                        server: server.to_owned(),
                    })
                }
                (None, None) => unreachable!("standalone local mode returned above"),
            }
        }
    }
}

pub fn from_env() -> Result<Deployment, DeploymentError> {
    let local = setting("ORBIT_DB")?;
    let replica = setting("ORBIT_AUTHORITY_DB")?;
    let server = setting("ORBIT_AUTHORITY_SERVER")?;
    let alias = setting("ORBIT_AUTHORITY_URL")?;
    let environment = ConfigEnv {
        home: std::env::var_os("HOME")
            .filter(|value| !value.is_empty())
            .or_else(|| std::env::var_os("USERPROFILE").filter(|value| !value.is_empty()))
            .map(PathBuf::from),
        config_file: std::env::var_os("ORBIT_CONFIG")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from),
        database: local.map(PathBuf::from),
        authority_database: replica,
        authority_server: server,
        authority_url: alias,
    };
    resolve_with_env(None, None, &environment)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn offline_local_mode_needs_no_server() {
        assert_eq!(
            resolve("local.db", None, None, None).unwrap(),
            Deployment::Local {
                database: "local.db".into()
            }
        );
    }
    #[test]
    fn connected_mode_requires_both_replica_and_endpoint() {
        assert_eq!(
            resolve("local.db", None, Some("http://localhost"), None),
            Err(DeploymentError::MissingReplica)
        );
        assert_eq!(
            resolve("local.db", Some("replica.db"), None, None),
            Err(DeploymentError::MissingServer)
        );
    }
    #[test]
    fn independent_replica_cannot_use_authority_file() {
        assert_eq!(
            resolve("local.db", Some("local.db"), Some("http://localhost"), None),
            Err(DeploymentError::SameDatabase)
        );
    }
    #[test]
    fn conflicting_endpoint_environment_is_rejected() {
        assert_eq!(
            resolve(
                "local.db",
                Some("replica.db"),
                Some("http://one"),
                Some("http://two")
            ),
            Err(DeploymentError::ConflictingServers)
        );
    }
    #[test]
    fn resolves_endpoint_alias() {
        assert_eq!(
            resolve("local.db", Some("replica.db"), None, Some("http://remote")).unwrap(),
            Deployment::Connected {
                replica: "replica.db".into(),
                server: "http://remote".into()
            }
        );
    }

    #[test]
    fn deployment_reads_connected_settings_from_config() {
        let directory = tempfile::tempdir().unwrap();
        let config = directory.path().join("config.toml");
        std::fs::write(
            &config,
            "version = 1\n[mode]\nauthority = \"connected\"\n[connected]\nserver = \"https://orbit.example\"\nreplica_database = \"replica.db\"\n",
        )
        .unwrap();
        let environment = ConfigEnv {
            home: Some(directory.path().to_path_buf()),
            ..ConfigEnv::default()
        };
        assert_eq!(
            resolve_with_env(None, Some(&config), &environment).unwrap(),
            Deployment::Connected {
                replica: directory.path().join("replica.db"),
                server: "https://orbit.example".into()
            }
        );
    }

    #[test]
    fn environment_server_overrides_connected_config() {
        let directory = tempfile::tempdir().unwrap();
        let config = directory.path().join("config.toml");
        std::fs::write(
            &config,
            "version = 1\n[mode]\nauthority = \"connected\"\n[connected]\nserver = \"https://orbit.example\"\nreplica_database = \"replica.db\"\n",
        )
        .unwrap();
        let environment = ConfigEnv {
            home: Some(directory.path().to_path_buf()),
            authority_server: Some("https://override.example".into()),
            authority_database: Some("/tmp/override-replica.db".into()),
            ..ConfigEnv::default()
        };
        assert_eq!(
            resolve_with_env(None, Some(&config), &environment).unwrap(),
            Deployment::Connected {
                replica: PathBuf::from("/tmp/override-replica.db"),
                server: "https://override.example".into()
            }
        );
    }

    #[test]
    fn cli_db_selects_the_replica_in_environment_connected_mode() {
        let directory = tempfile::tempdir().unwrap();
        let unused_local = directory.path().join("unused-local.db");
        let replica = directory.path().join("replica.db");
        let environment = ConfigEnv {
            home: Some(directory.path().to_path_buf()),
            database: Some(unused_local.clone()),
            authority_database: Some(replica.display().to_string()),
            authority_server: Some("http://localhost:8787".into()),
            ..ConfigEnv::default()
        };
        assert_eq!(
            resolve_with_env(Some(&replica), None, &environment).unwrap(),
            Deployment::Connected {
                replica: replica.clone(),
                server: "http://localhost:8787".into()
            }
        );
        // A replica that is the same file as the local-authority candidate is
        // still rejected, even when `--db` is present.
        assert_eq!(
            resolve_with_env(Some(&unused_local), None, &environment),
            Err(DeploymentError::SameDatabase)
        );
    }

    #[test]
    fn invalid_configuration_never_falls_back() {
        let directory = tempfile::tempdir().unwrap();
        let config = directory.path().join("config.toml");
        std::fs::write(&config, "version = 1\nunknown = true\n").unwrap();
        let environment = ConfigEnv {
            home: Some(directory.path().to_path_buf()),
            ..ConfigEnv::default()
        };
        assert!(matches!(
            resolve_with_env(None, Some(&config), &environment),
            Err(DeploymentError::Config(_))
        ));
    }
}
