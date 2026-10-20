//! Deployment selection shared by CLI, terminal UI and headless MCP.
//!
//! A configured connected deployment must never silently fall back to a
//! local authoritative database. Local-only mode remains fully independent.
use std::{
    env,
    path::{Path, PathBuf},
};

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

pub fn from_env() -> Result<Deployment, DeploymentError> {
    let local = setting("ORBIT_DB")?.unwrap_or_else(|| "orbit.db".into());
    let replica = setting("ORBIT_AUTHORITY_DB")?;
    let server = setting("ORBIT_AUTHORITY_SERVER")?;
    let alias = setting("ORBIT_AUTHORITY_URL")?;
    resolve(
        local,
        replica.as_deref(),
        server.as_deref(),
        alias.as_deref(),
    )
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
}
