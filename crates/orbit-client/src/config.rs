//! Versioned unified Orbit configuration and path resolution.
//!
//! The configuration file is the single durable description of a deployment.
//! It is optional: an absent file means built-in local defaults. A file that
//! exists but is invalid must fail closed instead of silently degrading to
//! defaults, because that could send commands to the wrong database.
//!
//! All resolvers that matter for tests are environment-independent. The
//! [`ConfigEnv`] value carries everything the process environment supplied so
//! that unit tests can pass explicit values instead of mutating global state.

use std::{
    env, fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::deployment::{Deployment, DeploymentError};

pub const CONFIG_VERSION: u32 = 1;
pub const CONFIG_FILE_NAME: &str = "config.toml";
pub const ORBIT_DIR_NAME: &str = ".orbit";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum AuthorityMode {
    #[default]
    Local,
    Connected,
}

impl AuthorityMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::Connected => "connected",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "local" => Some(Self::Local),
            "connected" => Some(Self::Connected),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StorageConfig {
    /// Optional database path. Relative paths resolve against the config file
    /// directory; `~/...` expands to the current user's home directory.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub database: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModeConfig {
    #[serde(default)]
    pub authority: AuthorityMode,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConnectedConfig {
    pub server: String,
    pub replica_database: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_provider: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OrbitConfig {
    pub version: u32,
    #[serde(default)]
    pub storage: StorageConfig,
    #[serde(default)]
    pub mode: ModeConfig,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connected: Option<ConnectedConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<AgentConfig>,
}

impl Default for OrbitConfig {
    fn default() -> Self {
        Self {
            version: CONFIG_VERSION,
            storage: StorageConfig::default(),
            mode: ModeConfig::default(),
            connected: None,
            agent: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigSource {
    Defaults,
    File(PathBuf),
}

impl ConfigSource {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Defaults => "defaults",
            Self::File(_) => "file",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadedConfig {
    pub path: PathBuf,
    pub config: OrbitConfig,
    pub source: ConfigSource,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrbitPaths {
    pub config_file: PathBuf,
    pub orbit_dir: PathBuf,
    pub default_database: PathBuf,
    pub logs_dir: PathBuf,
    pub state_dir: PathBuf,
    pub backups_dir: PathBuf,
}

impl OrbitPaths {
    /// Directory containing the configuration file. Relative paths inside a
    /// configuration file resolve here, which may differ from `orbit_dir` when
    /// an explicit `--config`/`ORBIT_CONFIG` file is used outside `$HOME`.
    pub fn config_dir(&self) -> &Path {
        self.config_file
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."))
    }

    /// Discover the layout using the process environment. An explicit config
    /// override wins over `ORBIT_CONFIG`, which wins over `$HOME/.orbit`.
    pub fn discover(config_override: Option<&Path>) -> Result<Self, ConfigError> {
        Self::discover_with(
            config_override,
            env_config_file().as_deref(),
            home_dir().as_deref(),
        )
    }

    /// Pure layout resolver used by tests and by [`ConfigEnv`] callers.
    ///
    /// The runtime directories always remain under `$HOME/.orbit` when a home
    /// directory is available. A custom configuration file location only moves
    /// where that file is read from; it never relocates the database or logs.
    pub fn discover_with(
        config_override: Option<&Path>,
        env_config_file: Option<&Path>,
        home: Option<&Path>,
    ) -> Result<Self, ConfigError> {
        let explicit = config_override.or(env_config_file);
        let config_file = match explicit {
            Some(config_file) => config_file.to_path_buf(),
            None => {
                let home = home.ok_or(ConfigError::NoHome)?;
                home.join(ORBIT_DIR_NAME).join(CONFIG_FILE_NAME)
            }
        };
        let orbit_dir = match home {
            Some(home) => home.join(ORBIT_DIR_NAME),
            None => config_file
                .parent()
                .filter(|parent| !parent.as_os_str().is_empty())
                .map(Path::to_path_buf)
                .unwrap_or_else(|| PathBuf::from(".")),
        };
        Ok(Self {
            config_file,
            default_database: orbit_dir.join("orbit.db"),
            logs_dir: orbit_dir.join("logs"),
            state_dir: orbit_dir.join("state"),
            backups_dir: orbit_dir.join("backups"),
            orbit_dir,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigError {
    NoHome,
    Io { path: PathBuf, message: String },
    Parse { path: PathBuf, message: String },
    UnsupportedVersion { found: u32, supported: u32 },
    EmptyPath { field: &'static str },
    InvalidPath { field: &'static str, detail: String },
    EmptyProvider,
    InvalidServer { detail: String },
    MissingConnected { field: &'static str },
    EmptyEnvironment { variable: &'static str },
    Deployment(String),
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoHome => write!(
                formatter,
                "cannot determine the home directory; set HOME (or USERPROFILE) or pass --config"
            ),
            Self::Io { path, message } => {
                write!(
                    formatter,
                    "cannot read configuration {}: {message}",
                    path.display()
                )
            }
            Self::Parse { path, message } => {
                write!(
                    formatter,
                    "invalid configuration {}: {message}; fix the file or run `orbit init --force`",
                    path.display()
                )
            }
            Self::UnsupportedVersion { found, supported } => write!(
                formatter,
                "unsupported configuration version {found}; this Orbit build supports version {supported}"
            ),
            Self::EmptyPath { field } => write!(formatter, "{field} must not be empty"),
            Self::InvalidPath { field, detail } => write!(formatter, "invalid {field}: {detail}"),
            Self::EmptyProvider => write!(formatter, "agent.default_provider must not be empty"),
            Self::InvalidServer { detail } => {
                write!(formatter, "invalid connected.server: {detail}")
            }
            Self::MissingConnected { field } => write!(
                formatter,
                "connected authority requires [{field}]; run `orbit init --authority connected`"
            ),
            Self::EmptyEnvironment { variable } => write!(formatter, "{variable} may not be empty"),
            Self::Deployment(message) => formatter.write_str(message),
        }
    }
}

impl std::error::Error for ConfigError {}

impl From<DeploymentError> for ConfigError {
    fn from(error: DeploymentError) -> Self {
        Self::Deployment(error.to_string())
    }
}

/// Snapshot of the process environment relevant to configuration.
#[derive(Debug, Clone, Default)]
pub struct ConfigEnv {
    pub home: Option<PathBuf>,
    pub config_file: Option<PathBuf>,
    pub database: Option<PathBuf>,
    pub authority_database: Option<String>,
    pub authority_server: Option<String>,
    pub authority_url: Option<String>,
}

impl ConfigEnv {
    pub fn from_env() -> Self {
        Self {
            home: home_dir(),
            config_file: env_config_file(),
            database: non_empty_os("ORBIT_DB").map(PathBuf::from),
            authority_database: env::var("ORBIT_AUTHORITY_DB").ok(),
            authority_server: env::var("ORBIT_AUTHORITY_SERVER").ok(),
            authority_url: env::var("ORBIT_AUTHORITY_URL").ok(),
        }
    }
}

fn home_dir() -> Option<PathBuf> {
    for name in ["HOME", "USERPROFILE"] {
        if let Some(value) = env::var_os(name)
            && !value.is_empty()
        {
            return Some(PathBuf::from(value));
        }
    }
    None
}

fn env_config_file() -> Option<PathBuf> {
    non_empty_os("ORBIT_CONFIG").map(PathBuf::from)
}

fn non_empty_os(name: &str) -> Option<std::ffi::OsString> {
    env::var_os(name).filter(|value| !value.is_empty())
}

impl OrbitConfig {
    /// Load, parse and validate a configuration file. A missing file yields
    /// `Ok(None)`; an existing but invalid file always yields an error.
    pub fn load(path: impl AsRef<Path>) -> Result<Option<LoadedConfig>, ConfigError> {
        let path = path.as_ref();
        let text = match fs::read_to_string(path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => {
                return Err(ConfigError::Io {
                    path: path.to_path_buf(),
                    message: error.to_string(),
                });
            }
        };
        let config: Self = toml::from_str(&text).map_err(|error| ConfigError::Parse {
            path: path.to_path_buf(),
            message: error.to_string(),
        })?;
        config.validate().map_err(|error| match error {
            ConfigError::Parse { .. } => error,
            other => ConfigError::Parse {
                path: path.to_path_buf(),
                message: other.to_string(),
            },
        })?;
        Ok(Some(LoadedConfig {
            path: path.to_path_buf(),
            config,
            source: ConfigSource::File(path.to_path_buf()),
        }))
    }

    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.version != CONFIG_VERSION {
            return Err(ConfigError::UnsupportedVersion {
                found: self.version,
                supported: CONFIG_VERSION,
            });
        }
        if let Some(database) = &self.storage.database {
            validate_path("storage.database", database)?;
        }
        if let Some(connected) = &self.connected {
            validate_server_url(&connected.server)?;
            validate_path("connected.replica_database", &connected.replica_database)?;
        }
        if self.mode.authority == AuthorityMode::Connected && self.connected.is_none() {
            return Err(ConfigError::MissingConnected { field: "connected" });
        }
        if let Some(agent) = &self.agent
            && let Some(provider) = &agent.default_provider
        {
            if provider.trim().is_empty() {
                return Err(ConfigError::EmptyProvider);
            }
            if provider.chars().any(char::is_control) {
                return Err(ConfigError::EmptyProvider);
            }
        }
        Ok(())
    }

    /// Render a deterministic, user-editable configuration document. Optional
    /// absolute user-specific locations are only written when explicitly set.
    pub fn render_toml(&self) -> String {
        let mut output = format!("version = {}\n", self.version);
        if let Some(database) = &self.storage.database {
            output.push_str("\n[storage]\n");
            output.push_str(&format!("database = \"{}\"\n", escape_toml(database)));
        }
        output.push_str("\n[mode]\n");
        output.push_str(&format!(
            "authority = \"{}\"\n",
            self.mode.authority.as_str()
        ));
        if let Some(connected) = &self.connected {
            output.push_str("\n[connected]\n");
            output.push_str(&format!(
                "server = \"{}\"\n",
                escape_toml(&connected.server)
            ));
            output.push_str(&format!(
                "replica_database = \"{}\"\n",
                escape_toml(&connected.replica_database)
            ));
        }
        if let Some(agent) = &self.agent
            && let Some(provider) = &agent.default_provider
        {
            output.push_str("\n[agent]\n");
            output.push_str(&format!(
                "default_provider = \"{}\"\n",
                escape_toml(provider)
            ));
        }
        output
    }
}

/// Resolved configuration and the deployment it selects.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolved {
    pub paths: OrbitPaths,
    pub config: OrbitConfig,
    pub source: ConfigSource,
    /// The database Orbit will actually use (local authority or replica).
    pub database: PathBuf,
    pub deployment: Deployment,
}

/// Internal resolution shared with deployment selection.
#[derive(Debug, Clone)]
pub(crate) struct CoreResolution {
    pub paths: Option<OrbitPaths>,
    pub config: OrbitConfig,
    pub source: ConfigSource,
    /// The local-authority database candidate after config/environment
    /// resolution but before the CLI `--db` override. Connected-mode safety
    /// checks must compare against this value, because `--db` can legitimately
    /// select a replica in connected mode.
    pub authority_database: PathBuf,
    /// The effective local database after the CLI `--db` override.
    pub local_database: PathBuf,
    pub replica_database: Option<PathBuf>,
}

/// Resolve configuration using explicit environment values. Pure and
/// independently testable; no global environment access.
pub fn resolve_with_env(
    local_override: Option<&Path>,
    config_override: Option<&Path>,
    environment: &ConfigEnv,
) -> Result<Resolved, ConfigError> {
    let core = resolve_core(local_override, config_override, environment, false)?;
    let deployment = crate::deployment::resolve_from_core(&core, environment, local_override)?;
    let database = match &deployment {
        Deployment::Local { database } => database.clone(),
        Deployment::Connected { replica, .. } => replica.clone(),
    };
    let paths = core.paths.ok_or(ConfigError::NoHome)?;
    Ok(Resolved {
        paths,
        config: core.config,
        source: core.source,
        database,
        deployment,
    })
}

/// Resolve configuration from the process environment.
pub fn resolve(
    local_override: Option<&Path>,
    config_override: Option<&Path>,
) -> Result<Resolved, ConfigError> {
    resolve_with_env(local_override, config_override, &ConfigEnv::from_env())
}

pub(crate) fn resolve_core(
    local_override: Option<&Path>,
    config_override: Option<&Path>,
    environment: &ConfigEnv,
    allow_no_home: bool,
) -> Result<CoreResolution, ConfigError> {
    let explicit_config = config_override.or(environment.config_file.as_deref());
    if environment.home.is_none() && explicit_config.is_none() {
        if !allow_no_home {
            return Err(ConfigError::NoHome);
        }
        let database_override = explicit_database(local_override, environment)?;
        let authority_database = database_override
            .clone()
            .unwrap_or_else(|| PathBuf::from("orbit.db"));
        return Ok(CoreResolution {
            paths: None,
            config: OrbitConfig::default(),
            source: ConfigSource::Defaults,
            authority_database: authority_database.clone(),
            local_database: authority_database,
            replica_database: None,
        });
    }

    let paths = OrbitPaths::discover_with(
        config_override,
        environment.config_file.as_deref(),
        environment.home.as_deref(),
    )?;
    let loaded = OrbitConfig::load(&paths.config_file)?;
    let (config, source) = match loaded {
        Some(loaded) => (loaded.config, loaded.source),
        None => (OrbitConfig::default(), ConfigSource::Defaults),
    };
    let storage_database = match &config.storage.database {
        Some(raw) => Some(resolve_config_path_with_home(
            raw,
            "storage.database",
            paths.config_dir(),
            environment.home.as_deref(),
        )?),
        None => None,
    };
    let replica_database = match &config.connected {
        Some(connected) => Some(resolve_config_path_with_home(
            &connected.replica_database,
            "connected.replica_database",
            paths.config_dir(),
            environment.home.as_deref(),
        )?),
        None => None,
    };
    let database_override = explicit_database(local_override, environment)?;
    let authority_database = environment
        .database
        .clone()
        .or_else(|| storage_database.clone())
        .unwrap_or_else(|| paths.default_database.clone());
    let local_database = database_override
        .clone()
        .unwrap_or_else(|| authority_database.clone());
    Ok(CoreResolution {
        paths: Some(paths),
        config,
        source,
        authority_database,
        local_database,
        replica_database,
    })
}

fn explicit_database(
    local_override: Option<&Path>,
    environment: &ConfigEnv,
) -> Result<Option<PathBuf>, ConfigError> {
    let path = match local_override {
        Some(path) => Some(path.to_path_buf()),
        None => environment.database.clone(),
    };
    if let Some(path) = &path {
        validate_user_path("database", path)?;
    }
    Ok(path)
}

fn validate_user_path(field: &'static str, path: &Path) -> Result<(), ConfigError> {
    if path.as_os_str().is_empty() {
        return Err(ConfigError::EmptyPath { field });
    }
    if path.to_string_lossy().contains('\0') {
        return Err(ConfigError::InvalidPath {
            field,
            detail: "path must not contain NUL".into(),
        });
    }
    Ok(())
}

fn validate_path(field: &'static str, raw: &str) -> Result<(), ConfigError> {
    if raw.trim().is_empty() {
        return Err(ConfigError::EmptyPath { field });
    }
    if raw.contains('\0') {
        return Err(ConfigError::InvalidPath {
            field,
            detail: "path must not contain NUL".into(),
        });
    }
    if raw.starts_with('~') && raw != "~" && !raw.starts_with("~/") && !raw.starts_with("~\\") {
        return Err(ConfigError::InvalidPath {
            field,
            detail: "only `~/...` home-relative paths are supported".into(),
        });
    }
    Ok(())
}

/// Resolve a configuration path against the config file directory. Relative
/// paths stay relative to the config; `~` expands to the process home.
pub fn resolve_config_path(
    raw: &str,
    field: &'static str,
    config_dir: &Path,
) -> Result<PathBuf, ConfigError> {
    resolve_config_path_with_home(raw, field, config_dir, home_dir().as_deref())
}

/// Environment-independent path resolver. `home` is only consulted for `~`
/// expansion, so callers that supply an alternate home never read the process
/// environment accidentally.
pub fn resolve_config_path_with_home(
    raw: &str,
    field: &'static str,
    config_dir: &Path,
    home: Option<&Path>,
) -> Result<PathBuf, ConfigError> {
    validate_path(field, raw)?;
    let expanded = if raw == "~" {
        home.ok_or(ConfigError::NoHome)?.to_path_buf()
    } else if let Some(rest) = raw.strip_prefix("~/").or_else(|| raw.strip_prefix("~\\")) {
        home.ok_or(ConfigError::NoHome)?.join(rest)
    } else {
        PathBuf::from(raw)
    };
    Ok(if expanded.is_absolute() {
        expanded
    } else {
        config_dir.join(expanded)
    })
}

fn validate_server_url(raw: &str) -> Result<(), ConfigError> {
    let value = raw.trim();
    if value.is_empty() {
        return Err(ConfigError::InvalidServer {
            detail: "server URL must not be empty".into(),
        });
    }
    if value.chars().any(char::is_whitespace) || value.chars().any(char::is_control) {
        return Err(ConfigError::InvalidServer {
            detail: "server URL must not contain whitespace or control characters".into(),
        });
    }
    if value.contains('?') || value.contains('#') {
        return Err(ConfigError::InvalidServer {
            detail: "server URL must not contain a query or fragment".into(),
        });
    }
    let (scheme, rest) = value
        .split_once("://")
        .ok_or_else(|| ConfigError::InvalidServer {
            detail: "server URL must include http:// or https://".into(),
        })?;
    let scheme = scheme.to_ascii_lowercase();
    if scheme != "https" && scheme != "http" {
        return Err(ConfigError::InvalidServer {
            detail: "server URL must use https, or http for loopback only".into(),
        });
    }
    let authority = rest.split('/').next().unwrap_or("");
    if authority.is_empty() {
        return Err(ConfigError::InvalidServer {
            detail: "server URL must include a host".into(),
        });
    }
    if authority.contains('@') {
        return Err(ConfigError::InvalidServer {
            detail: "server URL must not contain credentials".into(),
        });
    }
    let host = if let Some(stripped) = authority.strip_prefix('[') {
        let end = stripped
            .find(']')
            .ok_or_else(|| ConfigError::InvalidServer {
                detail: "invalid IPv6 host".into(),
            })?;
        &stripped[..end]
    } else {
        authority.split(':').next().unwrap_or("")
    };
    if host.is_empty() {
        return Err(ConfigError::InvalidServer {
            detail: "server URL must include a host".into(),
        });
    }
    if scheme == "http" && !is_loopback_host(host) {
        return Err(ConfigError::InvalidServer {
            detail: "http is only allowed for loopback hosts; use https".into(),
        });
    }
    Ok(())
}

fn is_loopback_host(host: &str) -> bool {
    host.eq_ignore_ascii_case("localhost")
        || host == "::1"
        || host == "0:0:0:0:0:0:0:1"
        || host
            .parse::<std::net::Ipv4Addr>()
            .is_ok_and(|address| address.is_loopback())
}

fn escape_toml(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '\\' => escaped.push_str("\\\\"),
            '"' => escaped.push_str("\\\""),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            character if character.is_control() => {
                escaped.push_str(&format!("\\u{:04X}", character as u32));
            }
            character => escaped.push(character),
        }
    }
    escaped
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_config(directory: &Path, text: &str) -> PathBuf {
        let path = directory.join("config.toml");
        fs::write(&path, text).unwrap();
        path
    }

    #[test]
    fn default_configuration_is_valid_and_round_trips() {
        let config = OrbitConfig::default();
        config.validate().unwrap();
        let rendered = config.render_toml();
        assert_eq!(rendered, "version = 1\n\n[mode]\nauthority = \"local\"\n");
        let parsed: OrbitConfig = toml::from_str(&rendered).unwrap();
        assert_eq!(parsed, config);
    }

    #[test]
    fn unknown_fields_are_rejected() {
        assert!(toml::from_str::<OrbitConfig>("version = 1\nsurprise = true\n").is_err());
        assert!(toml::from_str::<OrbitConfig>("version = 1\n[storage]\nother = \"x\"\n").is_err());
        assert!(toml::from_str::<OrbitConfig>("version = 1\n[agent]\nunknown = \"x\"\n").is_err());
    }

    #[test]
    fn unsupported_version_is_rejected_with_a_clear_message() {
        let config: OrbitConfig = toml::from_str("version = 99\n").unwrap();
        assert_eq!(
            config.validate(),
            Err(ConfigError::UnsupportedVersion {
                found: 99,
                supported: CONFIG_VERSION
            })
        );
        assert!(
            config
                .validate()
                .unwrap_err()
                .to_string()
                .contains("version 99")
        );
    }

    #[test]
    fn connected_mode_requires_complete_settings() {
        let missing: OrbitConfig =
            toml::from_str("version = 1\n[mode]\nauthority = \"connected\"\n").unwrap();
        assert_eq!(
            missing.validate(),
            Err(ConfigError::MissingConnected { field: "connected" })
        );
        let incomplete: Result<OrbitConfig, _> = toml::from_str(
            "version = 1\n[mode]\nauthority = \"connected\"\n[connected]\nserver = \"https://orbit.example\"\n",
        );
        assert!(incomplete.is_err());
    }

    #[test]
    fn invalid_servers_and_providers_fail_closed() {
        for server in [
            "",
            "orbit.example",
            "http://orbit.example",
            "https://user:pass@orbit.example",
            "https://orbit.example?token=x",
            "https://orbit.example#fragment",
            "ftp://orbit.example",
        ] {
            let config = OrbitConfig {
                mode: ModeConfig {
                    authority: AuthorityMode::Connected,
                },
                connected: Some(ConnectedConfig {
                    server: server.into(),
                    replica_database: "replica.db".into(),
                }),
                ..OrbitConfig::default()
            };
            assert!(config.validate().is_err(), "server accepted: {server}");
        }
        let empty_provider: OrbitConfig =
            toml::from_str("version = 1\n[agent]\ndefault_provider = \"  \"\n").unwrap();
        assert_eq!(empty_provider.validate(), Err(ConfigError::EmptyProvider));
        let loopback = OrbitConfig {
            mode: ModeConfig {
                authority: AuthorityMode::Connected,
            },
            connected: Some(ConnectedConfig {
                server: "http://localhost:8080".into(),
                replica_database: "replica.db".into(),
            }),
            ..OrbitConfig::default()
        };
        loopback.validate().unwrap();
    }

    #[test]
    fn relative_and_tilde_paths_resolve_against_the_config_directory() {
        let directory = tempfile::tempdir().unwrap();
        let config_dir = directory.path().join("orbit");
        fs::create_dir_all(&config_dir).unwrap();
        let relative =
            resolve_config_path("data/orbit.db", "storage.database", &config_dir).unwrap();
        assert_eq!(relative, config_dir.join("data/orbit.db"));
        let absolute =
            resolve_config_path("/tmp/orbit.db", "storage.database", &config_dir).unwrap();
        assert_eq!(absolute, PathBuf::from("/tmp/orbit.db"));
        let alternate_home = directory.path().join("alternate-home");
        let tilde = resolve_config_path_with_home(
            "~/custom/orbit.db",
            "storage.database",
            &config_dir,
            Some(&alternate_home),
        )
        .unwrap();
        assert_eq!(tilde, alternate_home.join("custom").join("orbit.db"));
        assert!(resolve_config_path("~other/orbit.db", "storage.database", &config_dir).is_err());
        assert!(resolve_config_path("bad\0path", "storage.database", &config_dir).is_err());
    }

    #[test]
    fn custom_config_file_does_not_relocate_runtime_directories() {
        let directory = tempfile::tempdir().unwrap();
        let home = directory.path().join("home");
        fs::create_dir_all(&home).unwrap();
        let custom = directory.path().join("elsewhere/config.toml");
        let paths = OrbitPaths::discover_with(Some(&custom), None, Some(&home)).unwrap();
        assert_eq!(paths.config_file, custom);
        assert_eq!(paths.orbit_dir, home.join(".orbit"));
        assert_eq!(paths.default_database, home.join(".orbit/orbit.db"));
        assert_eq!(paths.logs_dir, home.join(".orbit/logs"));
        assert_eq!(paths.state_dir, home.join(".orbit/state"));
        assert_eq!(paths.backups_dir, home.join(".orbit/backups"));
    }

    #[test]
    fn relative_paths_in_a_custom_config_resolve_against_its_parent() {
        let directory = tempfile::tempdir().unwrap();
        let home = directory.path().join("home");
        fs::create_dir_all(&home).unwrap();
        let config_dir = directory.path().join("elsewhere");
        fs::create_dir_all(&config_dir).unwrap();
        let config_path = config_dir.join("config.toml");
        fs::write(
            &config_path,
            "version = 1\n[storage]\ndatabase = \"data/orbit.db\"\n",
        )
        .unwrap();
        let environment = ConfigEnv {
            home: Some(home.clone()),
            ..ConfigEnv::default()
        };
        let resolved = resolve_with_env(None, Some(&config_path), &environment).unwrap();
        assert_eq!(resolved.database, config_dir.join("data/orbit.db"));
        assert_eq!(resolved.paths.orbit_dir, home.join(".orbit"));
    }

    #[test]
    fn cli_then_environment_then_config_then_default_precedence() {
        let directory = tempfile::tempdir().unwrap();
        let config_path = write_config(
            directory.path(),
            "version = 1\n[storage]\ndatabase = \"from-config.db\"\n",
        );
        let env = ConfigEnv {
            home: Some(directory.path().to_path_buf()),
            ..ConfigEnv::default()
        };

        let resolved = resolve_with_env(None, Some(&config_path), &env).expect("config resolution");
        assert_eq!(resolved.database, directory.path().join("from-config.db"));
        assert_eq!(resolved.source.as_str(), "file");

        let env_with_db = ConfigEnv {
            database: Some(PathBuf::from("/tmp/from-env.db")),
            ..env.clone()
        };
        let resolved =
            resolve_with_env(None, Some(&config_path), &env_with_db).expect("env resolution");
        assert_eq!(resolved.database, PathBuf::from("/tmp/from-env.db"));
        assert_eq!(
            resolved.deployment,
            Deployment::Local {
                database: PathBuf::from("/tmp/from-env.db")
            }
        );

        let resolved = resolve_with_env(
            Some(Path::new("/tmp/from-cli.db")),
            Some(&config_path),
            &env_with_db,
        )
        .expect("cli resolution");
        assert_eq!(resolved.database, PathBuf::from("/tmp/from-cli.db"));
    }

    #[test]
    fn default_database_lives_in_the_orbit_directory() {
        let directory = tempfile::tempdir().unwrap();
        let env = ConfigEnv {
            home: Some(directory.path().to_path_buf()),
            ..ConfigEnv::default()
        };
        let resolved = resolve_with_env(None, None, &env).expect("default resolution");
        assert_eq!(
            resolved.database,
            directory.path().join(".orbit").join("orbit.db")
        );
        assert_eq!(
            resolved.paths.config_file,
            directory.path().join(".orbit").join("config.toml")
        );
    }

    #[test]
    fn invalid_configuration_is_an_error_and_never_defaults() {
        let directory = tempfile::tempdir().unwrap();
        let config_path = write_config(directory.path(), "version = 1\nunknown = true\n");
        let env = ConfigEnv {
            home: Some(directory.path().to_path_buf()),
            ..ConfigEnv::default()
        };
        assert!(resolve_with_env(None, Some(&config_path), &env).is_err());
    }

    #[test]
    fn missing_configuration_uses_defaults() {
        let directory = tempfile::tempdir().unwrap();
        let env = ConfigEnv {
            home: Some(directory.path().to_path_buf()),
            ..ConfigEnv::default()
        };
        let resolved = resolve_with_env(None, None, &env).expect("default resolution");
        assert_eq!(resolved.source, ConfigSource::Defaults);
        assert_eq!(resolved.config.mode.authority, AuthorityMode::Local);
    }

    #[test]
    fn connected_configuration_selects_replica_and_server() {
        let directory = tempfile::tempdir().unwrap();
        let config_path = write_config(
            directory.path(),
            "version = 1\n[mode]\nauthority = \"connected\"\n[connected]\nserver = \"https://orbit.example\"\nreplica_database = \"replica.db\"\n",
        );
        let env = ConfigEnv {
            home: Some(directory.path().to_path_buf()),
            ..ConfigEnv::default()
        };
        let resolved = resolve_with_env(None, Some(&config_path), &env).expect("connected");
        assert_eq!(
            resolved.deployment,
            Deployment::Connected {
                replica: directory.path().join("replica.db"),
                server: "https://orbit.example".into()
            }
        );
        assert_eq!(resolved.database, directory.path().join("replica.db"));
    }

    #[test]
    fn connected_replica_yields_to_environment_database() {
        let directory = tempfile::tempdir().unwrap();
        let config_path = write_config(
            directory.path(),
            "version = 1\n[mode]\nauthority = \"connected\"\n[connected]\nserver = \"https://orbit.example\"\nreplica_database = \"replica.db\"\n",
        );
        let env = ConfigEnv {
            home: Some(directory.path().to_path_buf()),
            database: Some(PathBuf::from("/tmp/replica-override.db")),
            ..ConfigEnv::default()
        };
        let resolved = resolve_with_env(None, Some(&config_path), &env).expect("connected");
        assert_eq!(
            resolved.deployment,
            Deployment::Connected {
                replica: PathBuf::from("/tmp/replica-override.db"),
                server: "https://orbit.example".into()
            }
        );
    }

    #[test]
    fn no_home_keeps_the_legacy_relative_database_fallback() {
        let env = ConfigEnv::default();
        let deployment =
            crate::deployment::resolve_with_env(None, None, &env).expect("legacy fallback");
        assert_eq!(
            deployment,
            Deployment::Local {
                database: PathBuf::from("orbit.db")
            }
        );
    }
}
