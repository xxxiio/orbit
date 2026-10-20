//! Read-only `orbit doctor` diagnostics.
//!
//! Doctor never opens a workspace for writing, never migrates, and never
//! changes configuration. It reports actionable findings in human or stable
//! JSON form and exits non-zero only when `--strict` sees an error.

use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

use orbit_client::config::{self, CONFIG_VERSION, ConfigError, OrbitConfig, OrbitPaths};
use orbit_storage::{SUPPORTED_SCHEMA_VERSION, inspect_database_readonly};
use serde::Serialize;

use crate::{CliError, DoctorArgs};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
enum Severity {
    Ok,
    Info,
    Advice,
    Error,
}

impl Severity {
    fn human(self) -> &'static str {
        match self {
            Self::Ok => "OK",
            Self::Info => "INFO",
            Self::Advice => "ADVICE",
            Self::Error => "ERROR",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
struct Finding {
    severity: Severity,
    area: &'static str,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    remediation: Option<String>,
}

impl Finding {
    fn new(
        severity: Severity,
        area: &'static str,
        message: impl Into<String>,
        remediation: Option<&str>,
    ) -> Self {
        Self {
            severity,
            area,
            message: message.into(),
            remediation: remediation.map(ToOwned::to_owned),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
struct ExecutableSection {
    path: Option<String>,
    version: Option<String>,
    path_resolution: Option<String>,
    path_matches_current: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
struct ConfigSection {
    path: String,
    exists: bool,
    valid: bool,
    source: &'static str,
    error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct DatabaseSection {
    path: String,
    exists: bool,
    regular_file: bool,
    parent: String,
    parent_writable: bool,
    readable: bool,
    schema_version: Option<i64>,
    supported_schema_version: i64,
    orbit_schema_present: Option<bool>,
    table_count: Option<i64>,
    integrity_ok: Option<bool>,
    error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct AuthoritySection {
    mode: &'static str,
    server: Option<String>,
    replica: Option<String>,
    replica_exists: Option<bool>,
    credentials: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct ToolSection {
    name: &'static str,
    present: bool,
    version: Option<String>,
    path: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct OptionalSection {
    tools: Vec<ToolSection>,
    siblings: Vec<ToolSection>,
}

#[derive(Debug, Clone, Serialize)]
struct DoctorReport {
    #[serde(rename = "type")]
    kind: &'static str,
    status: &'static str,
    findings: Vec<Finding>,
    executable: ExecutableSection,
    config: ConfigSection,
    database: DatabaseSection,
    authority: AuthoritySection,
    optional: OptionalSection,
}

impl DoctorReport {
    fn human(&self) -> String {
        let mut lines = Vec::new();
        for finding in &self.findings {
            lines.push(format!("{}: {}", finding.severity.human(), finding.message));
            if let Some(remediation) = &finding.remediation {
                lines.push(format!("  remediation: {remediation}"));
            }
        }
        lines.push(format!("Status: {}", self.status));
        lines.join("\n")
    }
}

pub(crate) fn run(
    args: DoctorArgs,
    db_override: Option<&Path>,
    config_override: Option<&Path>,
    json: bool,
) -> Result<String, CliError> {
    let paths = OrbitPaths::discover(config_override).map_err(|error| match error {
        ConfigError::NoHome => CliError::Usage(error.to_string()),
        other => CliError::Usage(format!("configuration error: {other}")),
    })?;

    let mut findings = Vec::new();

    // Executable and PATH resolution.
    let current_exe = env::current_exe().ok();
    let canonical_exe = current_exe
        .as_deref()
        .and_then(|path| path.canonicalize().ok());
    let version = current_exe
        .as_deref()
        .and_then(|path| command_version(path, &["--version"]));
    match (&current_exe, &version) {
        (Some(path), Some(version)) => {
            findings.push(Finding::new(
                Severity::Ok,
                "executable",
                format!("{} ({version})", path.display()),
                None,
            ));
        }
        (Some(path), None) => findings.push(Finding::new(
            Severity::Error,
            "executable",
            format!("{} did not report a version", path.display()),
            Some("reinstall Orbit from a verified release archive"),
        )),
        (None, _) => findings.push(Finding::new(
            Severity::Error,
            "executable",
            "cannot determine the running executable path",
            Some("run Orbit from an installed binary or reinstall it"),
        )),
    }
    let path_orbit = find_on_path("orbit");
    let path_matches = match (&path_orbit, &canonical_exe) {
        (Some(found), Some(current)) => {
            found.canonicalize().ok().as_deref() == Some(current.as_path())
        }
        _ => false,
    };
    match (&path_orbit, &canonical_exe) {
        (Some(found), Some(_)) if path_matches => findings.push(Finding::new(
            Severity::Ok,
            "executable",
            format!("`orbit` on PATH resolves to {}", found.display()),
            None,
        )),
        (Some(found), Some(current)) => findings.push(Finding::new(
            Severity::Advice,
            "executable",
            format!(
                "`orbit` on PATH is {} but this process is {}",
                found.display(),
                current.display()
            ),
            Some("remove the stale binary or adjust PATH so the installed release is first"),
        )),
        (Some(found), None) => findings.push(Finding::new(
            Severity::Info,
            "executable",
            format!("`orbit` on PATH resolves to {}", found.display()),
            None,
        )),
        (None, _) => findings.push(Finding::new(
            Severity::Info,
            "executable",
            "`orbit` is not on PATH",
            Some("add the installation directory to PATH for shorter commands"),
        )),
    }

    // Configuration.
    let config_exists = paths.config_file.is_file();
    let loaded = OrbitConfig::load(&paths.config_file);
    let (config, config_valid, config_error, config_source) = match &loaded {
        Ok(Some(loaded)) => (loaded.config.clone(), true, None, "file"),
        Ok(None) => (OrbitConfig::default(), true, None, "defaults"),
        Err(error) => (
            OrbitConfig::default(),
            false,
            Some(error.to_string()),
            "defaults",
        ),
    };
    match (&loaded, config_exists) {
        (Err(error), _) => findings.push(Finding::new(
            Severity::Error,
            "configuration",
            format!("{} is invalid: {error}", paths.config_file.display()),
            Some("fix the file or run `orbit init --force` to replace it"),
        )),
        (Ok(Some(_)), true) => findings.push(Finding::new(
            Severity::Ok,
            "configuration",
            format!(
                "{} (version {})",
                paths.config_file.display(),
                CONFIG_VERSION
            ),
            None,
        )),
        (Ok(None), false) => findings.push(Finding::new(
            Severity::Info,
            "configuration",
            format!(
                "{} does not exist; built-in defaults are in use",
                paths.config_file.display()
            ),
            Some("run `orbit init` to create the documented layout"),
        )),
        _ => {}
    }

    // Effective database selection and read-only inspection.
    let resolved = config::resolve(db_override, config_override).ok();
    let deployment = resolved
        .as_ref()
        .map(|resolved| resolved.deployment.clone());
    let effective_database = resolved
        .as_ref()
        .map(|resolved| resolved.database.clone())
        .or_else(|| db_override.map(Path::to_path_buf))
        .or_else(|| {
            env::var_os("ORBIT_DB")
                .filter(|value| !value.is_empty())
                .map(PathBuf::from)
        })
        .unwrap_or_else(|| paths.default_database.clone());
    let database = inspect_database(&effective_database, &mut findings);

    // Authority mode and connected settings.
    let (authority_mode, server, replica) = match &deployment {
        Some(orbit_client::deployment::Deployment::Connected { replica, server }) => {
            ("connected", Some(server.clone()), Some(replica.clone()))
        }
        Some(orbit_client::deployment::Deployment::Local { .. }) => ("local", None, None),
        None => (
            config.mode.authority.as_str(),
            config
                .connected
                .as_ref()
                .map(|connected| connected.server.clone()),
            config.connected.as_ref().map(|connected| {
                config::resolve_config_path(
                    &connected.replica_database,
                    "connected.replica_database",
                    paths.config_dir(),
                )
                .unwrap_or_else(|_| PathBuf::from(&connected.replica_database))
            }),
        ),
    };
    let credentials = credential_state();
    match authority_mode {
        "connected" => {
            findings.push(Finding::new(
                Severity::Info,
                "authority",
                format!(
                    "connected authority: {}",
                    server.as_deref().unwrap_or("(server not configured)")
                ),
                None,
            ));
            if let Some(replica) = &replica {
                let exists = replica.is_file();
                findings.push(Finding::new(
                    if exists {
                        Severity::Ok
                    } else {
                        Severity::Advice
                    },
                    "authority",
                    format!(
                        "replica database: {} ({})",
                        replica.display(),
                        if exists { "exists" } else { "missing" }
                    ),
                    if exists {
                        None
                    } else {
                        Some("run `orbit sync replica status` after configuring connected mode")
                    },
                ));
            }
        }
        _ => findings.push(Finding::new(
            Severity::Ok,
            "authority",
            "local authority: no remote server required",
            None,
        )),
    }
    if credentials.is_some() {
        findings.push(Finding::new(
            Severity::Info,
            "authority",
            "connected credentials are configured (value hidden)",
            None,
        ));
    }

    // Optional dependencies are never required.
    let mut tools = Vec::new();
    for (name, program) in [("herdr", "herdr"), ("python3", "python3")] {
        let path = find_on_path(program);
        let version = path
            .as_deref()
            .and_then(|path| command_version(path, &["--version"]));
        let present = path.is_some();
        let tool = ToolSection {
            name,
            present,
            version: version.clone(),
            path: path.as_ref().map(|path| path.display().to_string()),
        };
        let description = if let Some(version) = &version {
            version.clone()
        } else if let Some(path) = &path {
            path.display().to_string()
        } else {
            "not found (not required)".to_owned()
        };
        findings.push(Finding::new(
            Severity::Info,
            "optional",
            format!("{name}: {description}"),
            None,
        ));
        tools.push(tool);
    }
    let mut siblings = Vec::new();
    if let Some(directory) = current_exe.as_deref().and_then(Path::parent) {
        for name in ["orbit-mcp", "orbit-tui", "orbit-server"] {
            let path = directory.join(format!("{name}{}", std::env::consts::EXE_SUFFIX));
            let present = path.is_file();
            findings.push(Finding::new(
                Severity::Info,
                "optional",
                format!(
                    "{name}: {}",
                    if present {
                        "installed"
                    } else {
                        "not found next to orbit"
                    }
                ),
                None,
            ));
            siblings.push(ToolSection {
                name,
                present,
                version: None,
                path: present.then(|| path.display().to_string()),
            });
        }
    }

    let status = if findings
        .iter()
        .any(|finding| finding.severity == Severity::Error)
    {
        "errors"
    } else if findings
        .iter()
        .any(|finding| finding.severity == Severity::Advice)
    {
        "warnings"
    } else {
        "ok"
    };
    let report = DoctorReport {
        kind: "doctor",
        status,
        findings,
        executable: ExecutableSection {
            path: current_exe.as_ref().map(|path| path.display().to_string()),
            version,
            path_resolution: path_orbit.as_ref().map(|path| path.display().to_string()),
            path_matches_current: path_orbit.as_ref().map(|_| path_matches),
        },
        config: ConfigSection {
            path: paths.config_file.display().to_string(),
            exists: config_exists,
            valid: config_valid,
            source: config_source,
            error: config_error,
        },
        database,
        authority: AuthoritySection {
            mode: authority_mode,
            server,
            replica: replica.as_ref().map(|path| path.display().to_string()),
            replica_exists: replica.as_ref().map(|path| path.is_file()),
            credentials,
        },
        optional: OptionalSection { tools, siblings },
    };
    let rendered = if json {
        serde_json::to_string_pretty(&report)
            .map_err(|error| CliError::Serialization(error.to_string()))?
    } else {
        report.human()
    };
    if args.strict && status == "errors" {
        return Err(CliError::Diagnostics(rendered));
    }
    Ok(rendered)
}

fn inspect_database(path: &Path, findings: &mut Vec<Finding>) -> DatabaseSection {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let exists = path.exists();
    let regular_file = fs::metadata(path)
        .map(|metadata| metadata.file_type().is_file())
        .unwrap_or(false);
    let parent_exists = parent.is_dir();
    let parent_writable = parent_exists && directory_writable(parent);
    let readable = exists && fs::File::open(path).is_ok();
    let mut section = DatabaseSection {
        path: path.display().to_string(),
        exists,
        regular_file,
        parent: parent.display().to_string(),
        parent_writable,
        readable,
        schema_version: None,
        supported_schema_version: SUPPORTED_SCHEMA_VERSION,
        orbit_schema_present: None,
        table_count: None,
        integrity_ok: None,
        error: None,
    };
    if !exists {
        findings.push(Finding::new(
            Severity::Info,
            "database",
            format!(
                "{} does not exist yet; it will be created on first write",
                path.display()
            ),
            Some("run `orbit init` to create it now"),
        ));
        if parent_exists && !parent_writable {
            findings.push(Finding::new(
                Severity::Error,
                "database",
                format!("database directory {} is not writable", parent.display()),
                Some("choose a writable --db path or fix directory permissions"),
            ));
        } else if !parent_exists {
            findings.push(Finding::new(
                Severity::Info,
                "database",
                format!("database directory {} does not exist yet", parent.display()),
                Some("run `orbit init` to create the layout"),
            ));
        }
        return section;
    }
    if !regular_file {
        findings.push(Finding::new(
            Severity::Error,
            "database",
            format!("{} is not a regular file", path.display()),
            Some("choose a database file path instead of a directory or device"),
        ));
        return section;
    }
    if !readable {
        findings.push(Finding::new(
            Severity::Error,
            "database",
            format!("{} is not readable", path.display()),
            Some("fix file permissions or run Orbit as the owning user"),
        ));
        return section;
    }
    match inspect_database_readonly(path) {
        Err(error) => {
            section.error = Some(error.to_string());
            findings.push(Finding::new(
                Severity::Error,
                "database",
                format!(
                    "{} is not a readable Orbit database: {error}",
                    path.display()
                ),
                Some("restore a backup or choose a different --db path; do not edit the file"),
            ));
        }
        Ok(inspection) => {
            section.schema_version = inspection.schema_version;
            section.orbit_schema_present = Some(inspection.orbit_schema_present);
            section.table_count = Some(inspection.table_count);
            section.integrity_ok = Some(inspection.integrity_ok);
            if !inspection.orbit_schema_present {
                findings.push(Finding::new(
                    Severity::Error,
                    "database",
                    format!(
                        "{} is a SQLite file but not an Orbit workspace",
                        path.display()
                    ),
                    Some("run `orbit init` for a fresh workspace or restore an Orbit backup"),
                ));
            } else {
                match inspection.schema_version {
                    Some(version) if version > SUPPORTED_SCHEMA_VERSION => {
                        findings.push(Finding::new(
                            Severity::Error,
                            "database",
                            format!(
                                "schema version {version} is newer than supported version {SUPPORTED_SCHEMA_VERSION}"
                            ),
                            Some("install the newer Orbit release that created this database"),
                        ));
                    }
                    Some(version) if version < SUPPORTED_SCHEMA_VERSION => {
                        findings.push(Finding::new(
                            Severity::Advice,
                            "database",
                            format!(
                                "schema version {version} will be migrated to {SUPPORTED_SCHEMA_VERSION} on the next write"
                            ),
                            Some("back up the database before upgrading"),
                        ));
                    }
                    Some(_) => findings.push(Finding::new(
                        Severity::Ok,
                        "database",
                        format!(
                            "{} (schema {}, {} tables)",
                            path.display(),
                            SUPPORTED_SCHEMA_VERSION,
                            inspection.table_count
                        ),
                        None,
                    )),
                    None => findings.push(Finding::new(
                        Severity::Error,
                        "database",
                        "Orbit schema ledger exists but has no version row",
                        Some("restore a backup; do not edit the database manually"),
                    )),
                }
                if !inspection.integrity_ok {
                    findings.push(Finding::new(
                        Severity::Error,
                        "database",
                        format!("{} failed SQLite integrity_check", path.display()),
                        Some("restore a validated backup before continuing"),
                    ));
                } else {
                    findings.push(Finding::new(
                        Severity::Ok,
                        "database",
                        "SQLite integrity check passed",
                        None,
                    ));
                }
            }
        }
    }
    section
}

fn directory_writable(path: &Path) -> bool {
    fs::metadata(path)
        .map(|metadata| !metadata.permissions().readonly())
        .unwrap_or(false)
}

fn find_on_path(program: &str) -> Option<PathBuf> {
    let path = env::var_os("PATH")?;
    for directory in env::split_paths(&path) {
        let candidate = directory.join(format!("{program}{}", std::env::consts::EXE_SUFFIX));
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

fn command_version(program: &Path, arguments: &[&str]) -> Option<String> {
    let output = Command::new(program).args(arguments).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let first = text.lines().next().unwrap_or("").trim();
    (!first.is_empty()).then(|| first.to_owned())
}

fn credential_state() -> Option<String> {
    const VARIABLES: [&str; 4] = [
        "ORBIT_SYNC_HTTP_TOKEN",
        "ORBIT_AUTHORITY_TOKEN",
        "ORBIT_AUTHORITY_DEVICE",
        "ORBIT_AUTHORITY_API_KEY",
    ];
    VARIABLES
        .iter()
        .any(|name| env::var_os(name).is_some_and(|value| !value.is_empty()))
        .then(|| "configured (value hidden)".to_owned())
}
