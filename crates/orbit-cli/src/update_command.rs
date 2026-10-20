//! Safe GitHub release update workflow.
//!
//! `orbit update` discovers a release, verifies its checksum and metadata,
//! backs up the effective database through the read-only SQLite backup API,
//! then reuses the embedded M26 installer for the binary transaction. When the
//! post-install version probe fails, the previous managed binaries are restored.
//!
//! Release checksums come from the same origin as the archive. They prove
//! download integrity; they are not a publisher signature.

use std::{
    cmp::Ordering,
    env, fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use orbit_storage::{
    SUPPORTED_SCHEMA_VERSION, backup_database_readonly, inspect_database_readonly,
};
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{CliError, UpdateArgs, UpdateChannel};

const DEFAULT_API_BASE: &str = "https://api.github.com/repos/xxxiio/orbit";
const USER_AGENT: &str = concat!("orbit-update/", env!("CARGO_PKG_VERSION"));
const MAX_ARCHIVE_BYTES: u64 = 256 * 1024 * 1024;
const MAX_CHECKSUM_BYTES: u64 = 8 * 1024;
const MAX_JSON_BYTES: u64 = 8 * 1024 * 1024;
const MANAGED_BINARIES: [&str; 4] = ["orbit", "orbit-mcp", "orbit-tui", "orbit-server"];
const INSTALL_SCRIPT: &str = include_str!("../../../scripts/install.sh");
const VERIFY_SCRIPT: &str = include_str!("../../../scripts/verify-release.sh");
const CHECKSUM_CAVEAT: &str = "Checksums are fetched from the same release origin and verify download integrity, not publisher authenticity.";

pub(crate) fn run(
    args: UpdateArgs,
    db_override: Option<&Path>,
    config_override: Option<&Path>,
    json: bool,
) -> Result<String, CliError> {
    let target = detect_target()?;
    let prefix = resolve_prefix(args.prefix.as_deref())?;
    let binaries = read_managed_binaries(&prefix)?;
    // Compare against the binary actually installed in the selected prefix,
    // not the currently executing binary. `--prefix` may point at a different
    // installation, and the running process may be a dev build.
    let current_text = current_installed_version(&prefix);
    let current = Version::parse(&current_text).ok_or_else(|| {
        CliError::Usage(format!(
            "cannot determine a valid installed version from {current_text:?}"
        ))
    })?;
    let components = components_for(&binaries);
    let channel = args.channel;

    let requested = match args.version.as_deref() {
        Some(raw) => Some(Version::parse(raw).ok_or_else(|| {
            CliError::Usage(format!(
                "invalid --version {raw:?}; expected a semantic version such as v0.2.0 or 0.2.0"
            ))
        })?),
        None => None,
    };

    if !args.check {
        preflight_tools()?;
    }

    let api = GithubApi::from_env()?;
    let client = Client::builder()
        .timeout(Duration::from_secs(30))
        .user_agent(USER_AGENT)
        .build()
        .map_err(|error| CliError::Io(format!("cannot build HTTP client: {error}")))?;
    let release = match requested {
        Some(version) => resolve_explicit(&client, &api, &target, &version)?,
        None => resolve_channel(&client, &api, &target, channel)?,
    };
    let comparison = release.version.cmp(&current);
    let update_available = comparison == Ordering::Greater;

    if args.check {
        let report = UpdateReport {
            kind: "update",
            current: current_text,
            available: Some(release.version.to_string()),
            tag: Some(release.tag.clone()),
            target,
            prefix: display(&prefix),
            channel: channel.as_str(),
            update_available,
            check: true,
            action: "checked",
            backup_path: None,
            installed_version: None,
        };
        return render(report, json);
    }

    if comparison == Ordering::Equal && !args.force {
        let report = UpdateReport {
            kind: "update",
            current: current_text,
            available: Some(release.version.to_string()),
            tag: Some(release.tag.clone()),
            target,
            prefix: display(&prefix),
            channel: channel.as_str(),
            update_available: false,
            check: false,
            action: "up_to_date",
            backup_path: None,
            installed_version: None,
        };
        return render(report, json);
    }

    if comparison == Ordering::Less && !args.allow_downgrade {
        return Err(CliError::Usage(format!(
            "selected release {} is older than the running version {}; pass --allow-downgrade to install it",
            release.version, current_text
        )));
    }

    install(
        &client,
        &api,
        &release,
        &target,
        &prefix,
        &binaries,
        &components,
        db_override,
        config_override,
        &current_text,
        channel,
        json,
    )
}

#[allow(clippy::too_many_arguments)]
fn install(
    client: &Client,
    api: &GithubApi,
    release: &Release,
    target: &str,
    prefix: &Path,
    binaries: &[String],
    components: &str,
    db_override: Option<&Path>,
    config_override: Option<&Path>,
    current: &str,
    channel: UpdateChannel,
    json: bool,
) -> Result<String, CliError> {
    let workspace = TempWorkspace::new()?;
    let archive_path = workspace.path.join(&release.archive_name);
    let checksum_path = workspace.path.join(&release.checksum_name);

    let archive_url = api.asset_url(
        &release.tag,
        &release.archive_name,
        release.archive_url.as_deref(),
    )?;
    let checksum_url = api.asset_url(
        &release.tag,
        &release.checksum_name,
        release.checksum_url.as_deref(),
    )?;
    download(client, &archive_url, MAX_ARCHIVE_BYTES, &archive_path)?;
    download(client, &checksum_url, MAX_CHECKSUM_BYTES, &checksum_path)?;

    let checksum_text = fs::read_to_string(&checksum_path)
        .map_err(|error| CliError::Io(format!("cannot read downloaded checksum: {error}")))?;
    let expected = parse_checksum(&checksum_text)?;
    let actual = sha256_file(&archive_path)?;
    if !expected.eq_ignore_ascii_case(&actual) {
        return Err(CliError::Usage(format!(
            "checksum mismatch for {}: expected {expected}, computed {actual}; the download was rejected",
            release.archive_name
        )));
    }
    let root = release
        .archive_name
        .strip_suffix(".tar.gz")
        .ok_or_else(|| CliError::Usage("unexpected release archive name".to_owned()))?;
    verify_archive(&archive_path, root, &release.version, target)?;

    // Data safety: inspect and back up before any managed binary is replaced.
    let backup_path = backup_effective_database(db_override, config_override, current)?;

    let previous_dir = workspace.path.join("previous-bin");
    let saved = save_binaries(prefix, binaries, &previous_dir)?;
    let manifest_path = prefix.join("share/orbit/installed-binaries.txt");
    let manifest_snapshot = fs::read(&manifest_path).ok();

    let scripts = workspace.path.join("scripts");
    fs::create_dir_all(&scripts)
        .map_err(|error| CliError::Io(format!("cannot prepare installer scripts: {error}")))?;
    let installer = scripts.join("install.sh");
    let verifier = scripts.join("verify-release.sh");
    fs::write(&installer, INSTALL_SCRIPT)
        .map_err(|error| CliError::Io(format!("cannot embed installer: {error}")))?;
    fs::write(&verifier, VERIFY_SCRIPT)
        .map_err(|error| CliError::Io(format!("cannot embed release verifier: {error}")))?;
    make_executable(&installer)?;
    make_executable(&verifier)?;

    let output = Command::new("bash")
        .arg(&installer)
        .arg("--from-archive")
        .arg(&archive_path)
        .arg("--prefix")
        .arg(prefix)
        .arg("--components")
        .arg(components)
        .output()
        .map_err(|error| {
            CliError::Io(format!(
                "cannot run the embedded installer with bash: {error}; install bash and retry"
            ))
        })?;
    if !output.status.success() {
        restore_binaries(prefix, binaries, &saved, manifest_snapshot.as_deref());
        return Err(CliError::Io(format!(
            "release installation failed; previous binaries were restored: {}",
            tail(&output.stderr)
        )));
    }

    let version_text = release.version.to_string();
    match probe_installed_version(prefix) {
        Ok(reported)
            if reported
                .split_whitespace()
                .any(|token| token == version_text) => {}
        Ok(reported) => {
            restore_binaries(prefix, binaries, &saved, manifest_snapshot.as_deref());
            return Err(CliError::Io(format!(
                "installed binary reports {reported:?}, expected version {version_text}; previous binaries were restored"
            )));
        }
        Err(error) => {
            restore_binaries(prefix, binaries, &saved, manifest_snapshot.as_deref());
            return Err(CliError::Io(format!(
                "{error}; previous binaries were restored"
            )));
        }
    }

    let report = UpdateReport {
        kind: "update",
        current: current.to_owned(),
        available: Some(version_text.clone()),
        tag: Some(release.tag.clone()),
        target: target.to_owned(),
        prefix: display(prefix),
        channel: channel.as_str(),
        update_available: true,
        check: false,
        action: "installed",
        backup_path,
        installed_version: Some(version_text),
    };
    render(report, json)
}

fn render(report: UpdateReport, json: bool) -> Result<String, CliError> {
    if json {
        serde_json::to_string_pretty(&report)
            .map_err(|error| CliError::Serialization(error.to_string()))
    } else {
        Ok(report.human())
    }
}

#[derive(Debug, Serialize)]
struct UpdateReport {
    #[serde(rename = "type")]
    kind: &'static str,
    current: String,
    available: Option<String>,
    tag: Option<String>,
    target: String,
    prefix: String,
    channel: &'static str,
    update_available: bool,
    check: bool,
    action: &'static str,
    backup_path: Option<String>,
    installed_version: Option<String>,
}

impl UpdateReport {
    fn human(&self) -> String {
        let available = self.available.as_deref().unwrap_or("unknown");
        match self.action {
            "checked" => [
                "Orbit update check".to_owned(),
                format!("Current: {}", self.current),
                format!("Available: {available}"),
                format!("Channel: {}", self.channel),
                format!("Target: {}", self.target),
                format!("Prefix: {}", self.prefix),
                format!(
                    "Update available: {}",
                    if self.update_available { "yes" } else { "no" }
                ),
                CHECKSUM_CAVEAT.to_owned(),
            ]
            .join("\n"),
            "up_to_date" => [
                format!(
                    "Orbit {} is up to date (channel {}).",
                    self.current, self.channel
                ),
                format!("Target: {}", self.target),
                format!("Prefix: {}", self.prefix),
            ]
            .join("\n"),
            _ => {
                let mut lines = vec![
                    format!(
                        "Updated Orbit {} -> {}",
                        self.current,
                        self.installed_version.as_deref().unwrap_or(available)
                    ),
                    format!("Tag: {}", self.tag.as_deref().unwrap_or("unknown")),
                    format!("Target: {}", self.target),
                    format!("Prefix: {}", self.prefix),
                ];
                match &self.backup_path {
                    Some(path) => lines.push(format!("Database backup: {path}")),
                    None => lines.push("Database backup: skipped (no database)".to_owned()),
                }
                lines.push(CHECKSUM_CAVEAT.to_owned());
                lines.join("\n")
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Release discovery
// ---------------------------------------------------------------------------

#[derive(Debug)]
struct GithubApi {
    base: String,
    download_base: Option<String>,
}

impl GithubApi {
    fn from_env() -> Result<Self, CliError> {
        let base =
            non_empty_env("ORBIT_GITHUB_API_BASE").unwrap_or_else(|| DEFAULT_API_BASE.to_owned());
        if !(base.starts_with("https://") || base.starts_with("http://")) {
            return Err(CliError::Usage(format!(
                "ORBIT_GITHUB_API_BASE must be an http(s) URL, got {base:?}"
            )));
        }
        let download_base = non_empty_env("ORBIT_GITHUB_DOWNLOAD_BASE");
        Ok(Self {
            base: base.trim_end_matches('/').to_owned(),
            download_base: download_base.map(|value| value.trim_end_matches('/').to_owned()),
        })
    }

    fn releases_url(&self) -> String {
        format!("{}/releases?per_page=100", self.base)
    }

    fn tag_url(&self, tag: &str) -> String {
        format!("{}/releases/tags/{tag}", self.base)
    }

    fn asset_url(&self, tag: &str, name: &str, browser: Option<&str>) -> Result<String, CliError> {
        if let Some(base) = &self.download_base {
            return Ok(format!("{base}/{tag}/{name}"));
        }
        if let Some(url) = browser {
            if !(url.starts_with("https://") || url.starts_with("http://")) {
                return Err(CliError::Usage(format!(
                    "release asset URL is not http(s): {url}"
                )));
            }
            return Ok(url.to_owned());
        }
        Ok(format!("{}/{tag}/{name}", self.derived_download_base()?))
    }

    /// Derive the public release download base for the real GitHub repository
    /// from the API base (`https://api.github.com/repos/{owner}/{repo}`).
    fn derived_download_base(&self) -> Result<String, CliError> {
        let url = reqwest::Url::parse(&self.base).map_err(|error| {
            CliError::Usage(format!(
                "cannot derive release download base from {self:?}: {error}"
            ))
        })?;
        let segments = url
            .path_segments()
            .map(|segments| segments.collect::<Vec<_>>())
            .unwrap_or_default();
        if segments.len() < 3 || segments[0] != "repos" {
            return Err(CliError::Usage(
                "cannot derive the release download URL; set ORBIT_GITHUB_DOWNLOAD_BASE".to_owned(),
            ));
        }
        let (owner, repository) = (segments[1], segments[2]);
        let host = url.host_str().ok_or_else(|| {
            CliError::Usage(format!("release API base has no host: {}", self.base))
        })?;
        let web_host = if host.eq_ignore_ascii_case("api.github.com") {
            "github.com".to_owned()
        } else {
            host.to_owned()
        };
        Ok(format!(
            "{}://{web_host}/{owner}/{repository}/releases/download",
            url.scheme()
        ))
    }
}

#[derive(Debug, Deserialize)]
struct ApiRelease {
    #[serde(default)]
    tag_name: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    assets: Vec<ApiAsset>,
}

#[derive(Debug, Deserialize)]
struct ApiAsset {
    #[serde(default)]
    name: String,
    #[serde(default)]
    browser_download_url: Option<String>,
}

#[derive(Debug, Clone)]
struct Release {
    tag: String,
    version: Version,
    prerelease: bool,
    archive_name: String,
    checksum_name: String,
    archive_url: Option<String>,
    checksum_url: Option<String>,
}

impl ApiRelease {
    fn candidate(&self, target: &str) -> Option<Release> {
        if self.draft {
            return None;
        }
        let version = Version::parse(&self.tag_name)?;
        let version_text = version.to_string();
        let archive_name = format!("orbit-{version_text}-{target}.tar.gz");
        let checksum_name = format!("{archive_name}.sha256");
        let archive = self
            .assets
            .iter()
            .find(|asset| asset.name == archive_name)?;
        let checksum = self
            .assets
            .iter()
            .find(|asset| asset.name == checksum_name)?;
        Some(Release {
            tag: self.tag_name.clone(),
            prerelease: self.prerelease || version.is_prerelease(),
            version,
            archive_name,
            checksum_name,
            archive_url: archive.browser_download_url.clone(),
            checksum_url: checksum.browser_download_url.clone(),
        })
    }
}

fn resolve_channel(
    client: &Client,
    api: &GithubApi,
    target: &str,
    channel: UpdateChannel,
) -> Result<Release, CliError> {
    let value = fetch_json(client, &api.releases_url())?
        .ok_or_else(|| CliError::Io("the GitHub releases endpoint returned no data".to_owned()))?;
    let entries = value.as_array().ok_or_else(|| {
        CliError::Io("the GitHub releases endpoint did not return a JSON array".to_owned())
    })?;
    let mut best: Option<Release> = None;
    for entry in entries {
        let Ok(api_release) = serde_json::from_value::<ApiRelease>(entry.clone()) else {
            continue;
        };
        let Some(candidate) = api_release.candidate(target) else {
            continue;
        };
        if candidate.prerelease && channel == UpdateChannel::Stable {
            continue;
        }
        match &best {
            Some(existing) if candidate.version <= existing.version => {}
            _ => best = Some(candidate),
        }
    }
    best.ok_or_else(|| {
        CliError::Usage(format!(
            "no {} release provides orbit-<version>-{target}.tar.gz; check the network or set ORBIT_GITHUB_API_BASE",
            channel.as_str()
        ))
    })
}

fn resolve_explicit(
    client: &Client,
    api: &GithubApi,
    target: &str,
    requested: &Version,
) -> Result<Release, CliError> {
    let requested_text = requested.to_string();
    for tag in [format!("v{requested_text}"), requested_text.clone()] {
        let Some(value) = fetch_json(client, &api.tag_url(&tag))? else {
            continue;
        };
        let api_release: ApiRelease = serde_json::from_value(value).map_err(|error| {
            CliError::Io(format!(
                "release {tag} has an unexpected JSON shape: {error}"
            ))
        })?;
        if api_release.draft {
            return Err(CliError::Usage(format!(
                "release {tag} is a draft and cannot be installed"
            )));
        }
        let release = api_release.candidate(target).ok_or_else(|| {
            CliError::Usage(format!(
                "release {tag} has no compatible orbit-{requested_text}-{target}.tar.gz asset"
            ))
        })?;
        if release.version != *requested {
            return Err(CliError::Usage(format!(
                "release tag {tag} reports version {} but {requested_text} was requested",
                release.version
            )));
        }
        return Ok(release);
    }
    Err(CliError::Usage(format!(
        "release {requested_text} was not found in the Orbit GitHub releases"
    )))
}

fn fetch_json(client: &Client, url: &str) -> Result<Option<serde_json::Value>, CliError> {
    let response = client
        .get(url)
        .header("Accept", "application/vnd.github+json")
        .send()
        .map_err(|error| CliError::Io(format!("cannot query {url}: {error}")))?;
    let status = response.status();
    if status == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }
    if !status.is_success() {
        return Err(CliError::Io(format!(
            "release API {url} returned HTTP {}",
            status.as_u16()
        )));
    }
    let bytes = read_limited(response, MAX_JSON_BYTES)?;
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(|error| CliError::Io(format!("release API {url} returned invalid JSON: {error}")))
}

// ---------------------------------------------------------------------------
// Downloads and verification
// ---------------------------------------------------------------------------

fn download(client: &Client, url: &str, limit: u64, destination: &Path) -> Result<(), CliError> {
    let response = client
        .get(url)
        .send()
        .map_err(|error| CliError::Io(format!("cannot download {url}: {error}")))?;
    let status = response.status();
    if !status.is_success() {
        return Err(CliError::Io(format!(
            "download {url} returned HTTP {}",
            status.as_u16()
        )));
    }
    if let Some(length) = response.content_length()
        && length > limit
    {
        return Err(CliError::Usage(format!(
            "download {url} is {length} bytes, larger than the {limit} byte limit"
        )));
    }
    let mut reader = response.take(limit + 1);
    let mut file = fs::File::create(destination).map_err(|error| {
        CliError::Io(format!(
            "cannot create download file {}: {error}",
            destination.display()
        ))
    })?;
    let copied = std::io::copy(&mut reader, &mut file)
        .map_err(|error| CliError::Io(format!("cannot save download {url}: {error}")))?;
    file.flush()
        .map_err(|error| CliError::Io(format!("cannot finish download {url}: {error}")))?;
    if copied > limit {
        drop(file);
        let _ = fs::remove_file(destination);
        return Err(CliError::Usage(format!(
            "download {url} exceeded the {limit} byte limit; the download was rejected"
        )));
    }
    Ok(())
}

fn read_limited(mut reader: impl Read, limit: u64) -> Result<Vec<u8>, CliError> {
    let mut buffer = Vec::new();
    reader
        .by_ref()
        .take(limit + 1)
        .read_to_end(&mut buffer)
        .map_err(|error| CliError::Io(format!("cannot read HTTP response: {error}")))?;
    if buffer.len() as u64 > limit {
        return Err(CliError::Usage(
            "HTTP response exceeded the allowed size".to_owned(),
        ));
    }
    Ok(buffer)
}

fn parse_checksum(text: &str) -> Result<String, CliError> {
    let mut lines = text.lines().filter(|line| !line.trim().is_empty());
    let line = lines
        .next()
        .ok_or_else(|| CliError::Usage("release checksum file is empty or malformed".to_owned()))?;
    if lines.next().is_some() {
        return Err(CliError::Usage(
            "release checksum file must contain exactly one digest".to_owned(),
        ));
    }
    let mut parts = line.split_whitespace();
    let digest = parts.next().unwrap_or("");
    if digest.len() != 64
        || !digest
            .chars()
            .all(|character| character.is_ascii_hexdigit())
    {
        return Err(CliError::Usage(
            "release checksum file does not contain a 64-character hexadecimal digest".to_owned(),
        ));
    }
    if parts.nth(1).is_some() {
        return Err(CliError::Usage(
            "release checksum file has unexpected extra fields".to_owned(),
        ));
    }
    Ok(digest.to_ascii_lowercase())
}

fn sha256_file(path: &Path) -> Result<String, CliError> {
    let mut file = fs::File::open(path)
        .map_err(|error| CliError::Io(format!("cannot open {}: {error}", path.display())))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| CliError::Io(format!("cannot read {}: {error}", path.display())))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn verify_archive(
    archive: &Path,
    root: &str,
    version: &Version,
    target: &str,
) -> Result<(), CliError> {
    let listing = Command::new("tar")
        .arg("-tzf")
        .arg(archive)
        .output()
        .map_err(|error| CliError::Io(format!("cannot run tar: {error}; install tar and retry")))?;
    if !listing.status.success() {
        return Err(CliError::Usage(format!(
            "release archive {} cannot be listed; it may be truncated: {}",
            archive.display(),
            tail(&listing.stderr)
        )));
    }
    let listing_text = String::from_utf8_lossy(&listing.stdout);
    let mut entries = listing_text.lines();
    let first = entries.next().unwrap_or("");
    if first != format!("{root}/") {
        return Err(CliError::Usage(format!(
            "release archive root {first:?} does not match expected {root:?}"
        )));
    }
    let prefix = format!("{root}/");
    for entry in listing_text.lines() {
        if !entry.starts_with(&prefix) {
            return Err(CliError::Usage(format!(
                "release archive contains an unexpected path: {entry}"
            )));
        }
    }

    let member = format!("{root}/RELEASE.txt");
    let output = Command::new("tar")
        .arg("-xOzf")
        .arg(archive)
        .arg(&member)
        .output()
        .map_err(|error| CliError::Io(format!("cannot run tar: {error}; install tar and retry")))?;
    if !output.status.success() {
        return Err(CliError::Usage(format!(
            "release archive {} has no readable RELEASE.txt: {}",
            archive.display(),
            tail(&output.stderr)
        )));
    }
    let metadata = String::from_utf8(output.stdout)
        .map_err(|_| CliError::Usage("release RELEASE.txt is not valid UTF-8".to_owned()))?;
    let expected_version = format!("Orbit {version}");
    let expected_target = format!("Target: {target}");
    let mut version_ok = false;
    let mut target_ok = false;
    for line in metadata.lines().map(str::trim) {
        version_ok |= line == expected_version;
        target_ok |= line == expected_target;
    }
    if !version_ok || !target_ok {
        return Err(CliError::Usage(format!(
            "release metadata does not match the selected release; expected {expected_version:?} and {expected_target:?}"
        )));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Database safety
// ---------------------------------------------------------------------------

fn backup_effective_database(
    db_override: Option<&Path>,
    config_override: Option<&Path>,
    current_version: &str,
) -> Result<Option<String>, CliError> {
    let resolved = orbit_client::config::resolve(db_override, config_override)
        .map_err(|error| CliError::Usage(format!("configuration error: {error}")))?;
    let database = resolved.database.clone();
    if !database.exists() {
        return Ok(None);
    }
    let inspection = inspect_database_readonly(&database).map_err(|error| {
        CliError::Repository(format!(
            "refusing to update: database {} is unreadable or corrupt: {error}",
            database.display()
        ))
    })?;
    if !inspection.orbit_schema_present || inspection.schema_version.is_none() {
        return Err(CliError::Usage(format!(
            "refusing to update: {} is not an Orbit database",
            database.display()
        )));
    }
    if !inspection.integrity_ok {
        return Err(CliError::Repository(format!(
            "refusing to update: database {} failed its integrity check",
            database.display()
        )));
    }
    let schema_version = inspection.schema_version.unwrap_or_default();
    if schema_version > SUPPORTED_SCHEMA_VERSION {
        return Err(CliError::Usage(format!(
            "refusing to update: database {} uses schema version {schema_version}, newer than this build supports ({SUPPORTED_SCHEMA_VERSION}); install the matching newer Orbit release instead of downgrading",
            database.display()
        )));
    }

    fs::create_dir_all(&resolved.paths.backups_dir).map_err(|error| {
        CliError::Io(format!(
            "cannot create backup directory {}: {error}",
            resolved.paths.backups_dir.display()
        ))
    })?;
    restrict_backup_dir(&resolved.paths.backups_dir)?;
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let destination = resolved
        .paths
        .backups_dir
        .join(format!("orbit-pre-update-{current_version}-{timestamp}.db"));
    backup_database_readonly(&database, &destination).map_err(|error| {
        CliError::Repository(format!(
            "cannot back up database {} before updating: {error}",
            database.display()
        ))
    })?;
    restrict_backup_file(&destination)?;
    Ok(Some(destination.display().to_string()))
}

#[cfg(unix)]
fn restrict_backup_dir(path: &Path) -> Result<(), CliError> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).map_err(|error| {
        CliError::Io(format!(
            "cannot restrict backup directory {}: {error}",
            path.display()
        ))
    })
}

#[cfg(not(unix))]
fn restrict_backup_dir(_path: &Path) -> Result<(), CliError> {
    Ok(())
}

#[cfg(unix)]
fn restrict_backup_file(path: &Path) -> Result<(), CliError> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600)).map_err(|error| {
        CliError::Io(format!(
            "cannot restrict database backup {}: {error}",
            path.display()
        ))
    })
}

#[cfg(not(unix))]
fn restrict_backup_file(_path: &Path) -> Result<(), CliError> {
    Ok(())
}

// ---------------------------------------------------------------------------
// Managed installation and binary rollback
// ---------------------------------------------------------------------------

fn detect_target() -> Result<String, CliError> {
    if let Some(override_target) = non_empty_env("ORBIT_TARGET") {
        if !override_target
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
        {
            return Err(CliError::Usage(format!(
                "ORBIT_TARGET contains unsupported characters: {override_target:?}"
            )));
        }
        return Ok(override_target);
    }
    let target = match (env::consts::OS, env::consts::ARCH) {
        ("linux", "x86_64") => "x86_64-unknown-linux-gnu",
        ("linux", "aarch64") => "aarch64-unknown-linux-gnu",
        ("macos", "x86_64") => "x86_64-apple-darwin",
        ("macos", "aarch64") => "aarch64-apple-darwin",
        (os, arch) => {
            return Err(CliError::Usage(format!(
                "unsupported platform {os} {arch}; set ORBIT_TARGET to a supported Rust target triple (for example x86_64-unknown-linux-gnu)"
            )));
        }
    };
    Ok(target.to_owned())
}

fn resolve_prefix(explicit: Option<&Path>) -> Result<PathBuf, CliError> {
    let prefix = match explicit {
        Some(prefix) => prefix.to_path_buf(),
        None => {
            let executable = env::current_exe().map_err(|error| {
                CliError::Io(format!("cannot locate the running executable: {error}"))
            })?;
            let bin = executable.parent().ok_or_else(|| {
                CliError::Io("cannot determine the installation prefix".to_owned())
            })?;
            bin.parent()
                .filter(|parent| !parent.as_os_str().is_empty())
                .ok_or_else(|| CliError::Io("cannot determine the installation prefix".to_owned()))?
                .to_path_buf()
        }
    };
    let prefix = if prefix.is_absolute() {
        prefix
    } else {
        env::current_dir()
            .map_err(|error| CliError::Io(error.to_string()))?
            .join(prefix)
    };
    if prefix.exists() {
        fs::canonicalize(&prefix)
            .map_err(|error| CliError::Io(format!("cannot resolve prefix: {error}")))
    } else {
        Ok(prefix)
    }
}

fn read_managed_binaries(prefix: &Path) -> Result<Vec<String>, CliError> {
    let manifest = prefix.join("share/orbit/installed-binaries.txt");
    let not_managed = || {
        CliError::Usage(format!(
            "{} is not a GitHub-managed installation; use your package manager or reinstall Orbit",
            prefix.display()
        ))
    };
    let metadata = fs::symlink_metadata(&manifest).map_err(|error| match error.kind() {
        std::io::ErrorKind::NotFound => not_managed(),
        _ => CliError::Io(format!("cannot inspect {}: {error}", manifest.display())),
    })?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(not_managed());
    }
    let text = fs::read_to_string(&manifest)
        .map_err(|error| CliError::Io(format!("cannot read {}: {error}", manifest.display())))?;
    let mut names: Vec<String> = Vec::new();
    for line in text.lines() {
        if !MANAGED_BINARIES.contains(&line) {
            return Err(not_managed());
        }
        if names.iter().any(|name| name == line) {
            return Err(not_managed());
        }
        names.push(line.to_owned());
    }
    if !names.iter().any(|name| name == "orbit") {
        return Err(not_managed());
    }
    Ok(names)
}

fn components_for(binaries: &[String]) -> String {
    binaries
        .iter()
        .map(|name| match name.as_str() {
            "orbit" => "cli",
            "orbit-mcp" => "mcp",
            "orbit-tui" => "tui",
            "orbit-server" => "server",
            _ => "cli",
        })
        .collect::<Vec<_>>()
        .join(",")
}

fn preflight_tools() -> Result<(), CliError> {
    // The embedded M26 installer and release verifier are shell scripts. Fail
    // before any download or mutation when their expected tools are absent.
    for tool in [
        "bash", "tar", "install", "cp", "mv", "chmod", "mktemp", "grep", "sed", "head", "sort",
        "uniq", "awk", "rm", "mkdir", "basename", "dirname", "cat",
    ] {
        if !tool_available(tool) {
            return Err(CliError::Usage(format!(
                "`{tool}` is required by the embedded release installer; install it and retry"
            )));
        }
    }
    if !tool_available("sha256sum") && !tool_available("shasum") {
        return Err(CliError::Usage(
            "sha256sum or shasum is required to verify a release; install one and retry".to_owned(),
        ));
    }
    Ok(())
}

fn tool_available(name: &str) -> bool {
    let Some(path) = env::var_os("PATH") else {
        return false;
    };
    env::split_paths(&path).any(|directory| {
        let candidate = directory.join(name);
        fs::metadata(&candidate).is_ok_and(|metadata| metadata.is_file())
    })
}

fn save_binaries(
    prefix: &Path,
    binaries: &[String],
    backup_dir: &Path,
) -> Result<Vec<(String, PathBuf)>, CliError> {
    fs::create_dir_all(backup_dir).map_err(|error| {
        CliError::Io(format!(
            "cannot create binary backup directory {}: {error}",
            backup_dir.display()
        ))
    })?;
    let mut saved = Vec::new();
    for name in binaries {
        let source = prefix.join("bin").join(name);
        match fs::symlink_metadata(&source) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(CliError::Usage(format!(
                    "refusing to update symlinked executable {}",
                    source.display()
                )));
            }
            Ok(metadata) if metadata.is_file() => {
                let destination = backup_dir.join(name);
                fs::copy(&source, &destination).map_err(|error| {
                    CliError::Io(format!(
                        "cannot back up {} before updating: {error}",
                        source.display()
                    ))
                })?;
                saved.push((name.clone(), destination));
            }
            Ok(_) => {
                return Err(CliError::Usage(format!(
                    "refusing to update {}: not a regular executable file",
                    source.display()
                )));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(CliError::Io(format!(
                    "cannot inspect {}: {error}",
                    source.display()
                )));
            }
        }
    }
    Ok(saved)
}

fn restore_binaries(
    prefix: &Path,
    binaries: &[String],
    saved: &[(String, PathBuf)],
    manifest_snapshot: Option<&[u8]>,
) {
    for name in binaries {
        let destination = prefix.join("bin").join(name);
        match saved.iter().find(|(saved_name, _)| saved_name == name) {
            Some((_, backup)) => {
                let _ = fs::copy(backup, &destination);
                let _ = make_executable(&destination);
            }
            None => {
                let _ = fs::remove_file(&destination);
            }
        }
    }
    let manifest = prefix.join("share/orbit/installed-binaries.txt");
    match manifest_snapshot {
        Some(bytes) => {
            let _ = fs::write(&manifest, bytes);
        }
        None => {
            let _ = fs::remove_file(&manifest);
        }
    }
}

fn current_installed_version(prefix: &Path) -> String {
    if let Ok(report) = probe_installed_version(prefix) {
        for token in report.split_whitespace() {
            if let Some(version) = Version::parse(token) {
                return version.to_string();
            }
        }
    }
    env!("CARGO_PKG_VERSION").to_owned()
}

fn probe_installed_version(prefix: &Path) -> Result<String, String> {
    let executable = prefix.join("bin").join("orbit");
    let output = Command::new(&executable)
        .arg("--version")
        .output()
        .map_err(|error| format!("cannot run {}: {error}", executable.display()))?;
    if !output.status.success() {
        return Err(format!(
            "{} --version failed: {}",
            executable.display(),
            tail(&output.stderr)
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

#[cfg(unix)]
fn make_executable(path: &Path) -> Result<(), CliError> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).map_err(|error| {
        CliError::Io(format!(
            "cannot mark {} executable: {error}",
            path.display()
        ))
    })
}

#[cfg(not(unix))]
fn make_executable(_path: &Path) -> Result<(), CliError> {
    Ok(())
}

// ---------------------------------------------------------------------------
// Semantic versions
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
struct Version {
    major: u64,
    minor: u64,
    patch: u64,
    prerelease: Vec<String>,
}

impl Version {
    fn parse(raw: &str) -> Option<Self> {
        let raw = raw.trim();
        let raw = raw
            .strip_prefix('v')
            .or_else(|| raw.strip_prefix('V'))
            .unwrap_or(raw);
        let raw = raw.split_once('+').map_or(raw, |(core, _)| core);
        let (core, prerelease) = match raw.split_once('-') {
            Some((core, prerelease)) => (core, Some(prerelease)),
            None => (raw, None),
        };
        let mut parts = core.split('.');
        let major = parts.next()?.parse().ok()?;
        let minor = parts.next()?.parse().ok()?;
        let patch = parts.next()?.parse().ok()?;
        if parts.next().is_some() {
            return None;
        }
        let prerelease = match prerelease {
            Some(value) => {
                let identifiers: Vec<String> = value.split('.').map(str::to_owned).collect();
                if identifiers.iter().any(|identifier| {
                    identifier.is_empty()
                        || !identifier
                            .chars()
                            .all(|character| character.is_ascii_alphanumeric() || character == '-')
                }) {
                    return None;
                }
                identifiers
            }
            None => Vec::new(),
        };
        Some(Self {
            major,
            minor,
            patch,
            prerelease,
        })
    }

    fn is_prerelease(&self) -> bool {
        !self.prerelease.is_empty()
    }
}

impl std::fmt::Display for Version {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}.{}.{}", self.major, self.minor, self.patch)?;
        if !self.prerelease.is_empty() {
            write!(formatter, "-{}", self.prerelease.join("."))?;
        }
        Ok(())
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        (self.major, self.minor, self.patch)
            .cmp(&(other.major, other.minor, other.patch))
            .then_with(|| compare_prerelease(&self.prerelease, &other.prerelease))
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

fn compare_prerelease(left: &[String], right: &[String]) -> Ordering {
    match (left.is_empty(), right.is_empty()) {
        (true, true) => Ordering::Equal,
        // A release always sorts after any prerelease of the same core.
        (true, false) => Ordering::Greater,
        (false, true) => Ordering::Less,
        (false, false) => {
            for (left, right) in left.iter().zip(right) {
                let ordering = match (left.parse::<u64>(), right.parse::<u64>()) {
                    (Ok(left), Ok(right)) => left.cmp(&right),
                    (Ok(_), Err(_)) => Ordering::Less,
                    (Err(_), Ok(_)) => Ordering::Greater,
                    (Err(_), Err(_)) => left.cmp(right),
                };
                if ordering != Ordering::Equal {
                    return ordering;
                }
            }
            left.len().cmp(&right.len())
        }
    }
}

// ---------------------------------------------------------------------------
// Small helpers
// ---------------------------------------------------------------------------

struct TempWorkspace {
    path: PathBuf,
}

impl TempWorkspace {
    fn new() -> Result<Self, CliError> {
        let base = env::temp_dir();
        for attempt in 0..32 {
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos();
            let path = base.join(format!(
                "orbit-update-{}-{nanos}-{attempt}",
                std::process::id()
            ));
            match fs::create_dir(&path) {
                Ok(()) => return Ok(Self { path }),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => {
                    return Err(CliError::Io(format!(
                        "cannot create temporary directory {}: {error}",
                        path.display()
                    )));
                }
            }
        }
        Err(CliError::Io(
            "cannot create a unique temporary directory for the update".to_owned(),
        ))
    }
}

impl Drop for TempWorkspace {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn non_empty_env(name: &str) -> Option<String> {
    env::var(name)
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn display(path: &Path) -> String {
    path.display().to_string()
}

fn tail(bytes: &[u8]) -> String {
    let text = String::from_utf8_lossy(bytes);
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return "no diagnostic output".to_owned();
    }
    let length = trimmed.chars().count();
    if length > 2_000 {
        // Truncate on a character boundary; slicing a byte index here could
        // split a multi-byte UTF-8 sequence and panic.
        let suffix: String = trimmed.chars().skip(length - 2_000).collect();
        format!("...{suffix}")
    } else {
        trimmed.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_parse_with_or_without_a_leading_v() {
        assert_eq!(Version::parse("v0.2.0"), Version::parse("0.2.0"));
        assert_eq!(Version::parse("V1.2.3"), Version::parse("1.2.3"));
        assert_eq!(Version::parse("1.2.3+build.5"), Version::parse("1.2.3"));
        assert!(Version::parse("1.2").is_none());
        assert!(Version::parse("1.2.3.4").is_none());
        assert!(Version::parse("v").is_none());
        assert!(Version::parse("1.2.3-").is_none());
        assert!(Version::parse("1.2.3-rc..1").is_none());
    }

    #[test]
    fn semver_ordering_handles_prerelease_identifiers() {
        let stable = Version::parse("0.2.0").unwrap();
        let beta = Version::parse("0.2.0-beta.1").unwrap();
        let beta2 = Version::parse("0.2.0-beta.2").unwrap();
        let rc = Version::parse("0.2.0-rc.1").unwrap();
        let alpha = Version::parse("0.2.0-alpha").unwrap();
        assert!(stable > beta);
        assert!(rc > beta2);
        assert!(beta2 > beta);
        assert!(beta > alpha);
        assert!(Version::parse("0.10.0").unwrap() > Version::parse("0.9.9").unwrap());
        assert_eq!(
            Version::parse("0.2.0-beta.2").unwrap(),
            Version::parse("v0.2.0-beta.2").unwrap()
        );
    }

    #[test]
    fn checksum_parser_accepts_one_digest_and_rejects_noise() {
        let digest = "a".repeat(64);
        assert_eq!(parse_checksum(&digest).unwrap(), digest);
        assert_eq!(
            parse_checksum(&format!("{digest}  orbit-0.2.0.tar.gz\n")).unwrap(),
            digest
        );
        assert!(parse_checksum("").is_err());
        assert!(parse_checksum("abc").is_err());
        assert!(parse_checksum(&format!("{digest}\n{digest}")).is_err());
        assert!(parse_checksum(&format!("{digest} file extra")).is_err());
        assert!(parse_checksum(&format!("{}g", "a".repeat(63))).is_err());
    }
}
