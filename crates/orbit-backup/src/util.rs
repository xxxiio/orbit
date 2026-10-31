//! Small filesystem, hashing, timestamp and archive-path utilities.
//!
//! Everything here is deliberately dependency-free: the project builds with
//! `--offline`, so the archive format and its support code use `std`, `sha2`
//! and `serde_json` only.

use std::fs;
use std::io::{self, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use sha2::{Digest, Sha256};

use crate::error::{BackupError, Result};

static SEQUENCE: AtomicU64 = AtomicU64::new(1);

/// Current wall-clock time in Unix epoch milliseconds.
pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| i64::try_from(duration.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

/// A process- and call-unique suffix for temporary files and directories.
pub fn unique_suffix() -> String {
    let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.subsec_nanos())
        .unwrap_or(0);
    format!("{}-{}-{nanos}-{sequence}", std::process::id(), now_ms())
}

/// Convert Unix epoch milliseconds to an RFC 3339 UTC timestamp string.
///
/// The civil-date conversion is the standard days-from-epoch algorithm; a
/// dedicated time crate would be a new build dependency for one field.
pub fn format_rfc3339_utc(epoch_ms: i64) -> String {
    let (days, millis_of_day) = split_epoch_ms(epoch_ms);
    let (year, month, day) = civil_from_days(days);
    let seconds_of_day = millis_of_day / 1_000;
    let hour = seconds_of_day / 3_600;
    let minute = (seconds_of_day % 3_600) / 60;
    let second = seconds_of_day % 60;
    let millis = millis_of_day % 1_000;
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}.{millis:03}Z")
}

/// Compact UTC timestamp used in default backup file names.
pub fn format_compact_timestamp(epoch_ms: i64) -> String {
    let (days, millis_of_day) = split_epoch_ms(epoch_ms);
    let (year, month, day) = civil_from_days(days);
    let seconds_of_day = millis_of_day / 1_000;
    format!(
        "{year:04}{month:02}{day:02}T{:02}{:02}{:02}Z",
        seconds_of_day / 3_600,
        (seconds_of_day % 3_600) / 60,
        seconds_of_day % 60
    )
}

fn split_epoch_ms(epoch_ms: i64) -> (i64, i64) {
    let millis = epoch_ms.max(0);
    (millis.div_euclid(86_400_000), millis.rem_euclid(86_400_000))
}

/// Howard Hinnant's civil-from-days algorithm, valid for the supported range.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let day_of_era = z.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = u32::try_from(day_of_year - (153 * month_prime + 2) / 5 + 1).unwrap_or(1);
    let month = u32::try_from(if month_prime < 10 {
        month_prime + 3
    } else {
        month_prime - 9
    })
    .unwrap_or(1);
    let year = if month <= 2 { year + 1 } else { year };
    (year, month, day)
}

/// Stream a reader to EOF, returning the byte count and lowercase SHA-256.
pub fn hash_reader(reader: &mut dyn Read) -> io::Result<(u64, String)> {
    let mut hasher = Sha256::new();
    let mut total = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
        total += read as u64;
    }
    Ok((total, hex_digest(hasher.finalize())))
}

/// Copy a reader to a writer while hashing, returning the byte count and digest.
pub fn copy_and_hash(reader: &mut dyn Read, writer: &mut dyn Write) -> io::Result<(u64, String)> {
    let mut hasher = Sha256::new();
    let mut total = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
        writer.write_all(&buffer[..read])?;
        total += read as u64;
    }
    Ok((total, hex_digest(hasher.finalize())))
}

/// Hash one existing regular file.
pub fn hash_file(path: &Path) -> Result<(u64, String)> {
    let mut file = fs::File::open(path)
        .map_err(|error| BackupError::io(format!("open {}", path.display()), error))?;
    hash_reader(&mut file)
        .map_err(|error| BackupError::io(format!("read {}", path.display()), error))
}

/// Lowercase hexadecimal encoding of a digest.
pub fn hex_digest(digest: impl AsRef<[u8]>) -> String {
    let bytes = digest.as_ref();
    let mut text = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(text, "{byte:02x}");
    }
    text
}

/// fsync one file by re-opening it read-only.
pub fn fsync_file(path: &Path) -> Result<()> {
    let file = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .map_err(|error| BackupError::io(format!("open for sync {}", path.display()), error))?;
    file.sync_all()
        .map_err(|error| BackupError::io(format!("sync {}", path.display()), error))
}

/// fsync a directory entry after a rename or removal where supported.
pub fn fsync_dir(path: &Path) -> Result<()> {
    match fs::File::open(path) {
        Ok(directory) => directory
            .sync_all()
            .map_err(|error| BackupError::io(format!("sync directory {}", path.display()), error)),
        // Directory fsync is not supported on every platform/file system. The
        // data files themselves are always synced, so this stays best-effort.
        Err(error) if error.kind() == io::ErrorKind::PermissionDenied => Ok(()),
        Err(error) => Err(BackupError::io(
            format!("open directory {}", path.display()),
            error,
        )),
    }
}

/// Create the parent directories of `path` when they do not exist.
pub fn ensure_parent(path: &Path) -> Result<()> {
    let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    else {
        return Ok(());
    };
    fs::create_dir_all(parent)
        .map_err(|error| BackupError::io(format!("create {}", parent.display()), error))
}

/// Recursively create a directory with owner-only permissions on Unix.
///
/// Permissions are tightened only when this call *creates* the directory.
/// An existing directory belongs to the operator: backup and restore must
/// never widen or otherwise rewrite its mode (that would be an invisible
/// mutation of operator state and could make a denied destination writable).
pub fn create_private_dir(path: &Path) -> Result<()> {
    let existed = path.exists();
    fs::create_dir_all(path)
        .map_err(|error| BackupError::io(format!("create {}", path.display()), error))?;
    if !existed {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(path, fs::Permissions::from_mode(0o700))
                .map_err(|error| BackupError::io(format!("chmod {}", path.display()), error))?;
        }
    }
    Ok(())
}

/// Rename `from` onto `to`, syncing the parent directory when it succeeds.
pub fn atomic_rename(from: &Path, to: &Path) -> Result<()> {
    fs::rename(from, to)
        .map_err(|error| BackupError::io(format!("rename to {}", to.display()), error))?;
    if let Some(parent) = to.parent().filter(|parent| !parent.as_os_str().is_empty()) {
        fsync_dir(parent)?;
    }
    Ok(())
}

/// Validate that an archive path is relative, normalized and stays inside the
/// restore root. Rejects absolute paths, parents, backslashes, NUL and drive
/// prefixes so a crafted archive can never escape its extraction directory.
pub fn is_safe_relative_path(path: &str) -> bool {
    if path.is_empty() || path.contains('\\') || path.contains('\0') {
        return false;
    }
    if path.starts_with('/') {
        return false;
    }
    let mut has_component = false;
    for component in Path::new(path).components() {
        match component {
            Component::Normal(_) => has_component = true,
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => return false,
        }
    }
    has_component
}

/// Join a validated relative archive path onto a destination root.
pub fn join_archive_path(root: &Path, relative: &str) -> Result<PathBuf> {
    if !is_safe_relative_path(relative) {
        return Err(BackupError::UnsafePath(relative.to_owned()));
    }
    let joined = root.join(relative);
    // Belt and braces: the lexical check above is authoritative, but a path
    // that still resolves outside the root must never be trusted.
    let normalized = normalize_lexically(&joined);
    let normalized_root = normalize_lexically(root);
    if !normalized.starts_with(&normalized_root) {
        return Err(BackupError::UnsafePath(relative.to_owned()));
    }
    Ok(normalized)
}

/// Lexically remove `.` and resolve `..` without touching the filesystem.
pub fn normalize_lexically(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            other => normalized.push(other.as_os_str()),
        }
    }
    normalized
}

/// Convert a filesystem path to a slash-separated relative archive path.
pub fn relative_archive_path(root: &Path, path: &Path) -> Result<String> {
    let relative = path.strip_prefix(root).map_err(|_| {
        BackupError::UnsafePath(format!("{} is outside {}", path.display(), root.display()))
    })?;
    let mut text = String::new();
    for component in relative.components() {
        let Component::Normal(part) = component else {
            return Err(BackupError::UnsafePath(format!(
                "{} is not a normal relative path",
                path.display()
            )));
        };
        if !text.is_empty() {
            text.push('/');
        }
        text.push_str(&part.to_string_lossy());
    }
    Ok(text)
}

/// Best-effort slug for a user-supplied backup label.
pub fn label_slug(label: &str) -> String {
    let mut slug = String::new();
    let mut last_dash = false;
    for character in label.chars() {
        if character.is_ascii_alphanumeric() {
            slug.push(character.to_ascii_lowercase());
            last_dash = false;
        } else if !last_dash && !slug.is_empty() {
            slug.push('-');
            last_dash = true;
        }
        if slug.len() >= 40 {
            break;
        }
    }
    let trimmed = slug.trim_end_matches('-').to_owned();
    if trimmed.is_empty() {
        "backup".to_owned()
    } else {
        trimmed
    }
}

/// A temporary directory removed on drop. Cleanup is best-effort: a failed
/// removal must never mask the operation's real outcome.
#[derive(Debug)]
pub struct TempDir {
    path: PathBuf,
    keep: bool,
}

impl TempDir {
    /// Create a uniquely named temporary directory under `parent`.
    pub fn new(parent: impl AsRef<Path>, prefix: &str) -> Result<Self> {
        let path = parent
            .as_ref()
            .join(format!(".{prefix}-{}", unique_suffix()));
        create_private_dir(&path)?;
        Ok(Self { path, keep: false })
    }

    /// The temporary directory path.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Keep the directory after this value is dropped (diagnostics only).
    pub fn keep(mut self) -> PathBuf {
        self.keep = true;
        self.path.clone()
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        if !self.keep {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rfc3339_matches_known_instants() {
        assert_eq!(format_rfc3339_utc(0), "1970-01-01T00:00:00.000Z");
        assert_eq!(
            format_rfc3339_utc(1_700_000_000_123),
            "2023-11-14T22:13:20.123Z"
        );
        // A leap day, which is where naive day arithmetic breaks.
        assert_eq!(
            format_rfc3339_utc(1_709_164_800_000),
            "2024-02-29T00:00:00.000Z"
        );
    }

    #[test]
    fn compact_timestamp_is_stable() {
        assert_eq!(format_compact_timestamp(0), "19700101T000000Z");
        assert_eq!(
            format_compact_timestamp(1_700_000_000_123),
            "20231114T221320Z"
        );
    }

    #[test]
    fn safe_paths_reject_traversal_and_absolute_paths() {
        for path in [
            "../escape",
            "a/../../escape",
            "/etc/passwd",
            "\\windows\\path",
            "",
            "a\0b",
        ] {
            assert!(!is_safe_relative_path(path), "{path} must be rejected");
        }
        for path in ["manifest.json", "database/orbit.db", "knowledge/Work/a.md"] {
            assert!(is_safe_relative_path(path), "{path} must be accepted");
        }
    }

    #[test]
    fn join_archive_path_keeps_content_inside_root() {
        let root = Path::new("/tmp/orbit-restore");
        assert_eq!(
            join_archive_path(root, "knowledge/a.md").unwrap(),
            PathBuf::from("/tmp/orbit-restore/knowledge/a.md")
        );
        assert!(join_archive_path(root, "../outside").is_err());
        assert!(join_archive_path(root, "/abs").is_err());
    }

    #[test]
    fn hashing_is_streaming_and_matches_sha256() {
        let payload = b"orbit backup";
        let mut reader: &[u8] = payload;
        let (size, digest) = hash_reader(&mut reader).unwrap();
        assert_eq!(size, payload.len() as u64);
        // Known SHA-256 of "orbit backup".
        assert_eq!(
            digest,
            "661a403ee5c754bac0f407f4140aaf24e32087baf2687e2183d2da5162e29c05"
        );
    }

    #[test]
    fn existing_directory_permissions_are_never_rewritten() {
        let directory = tempfile::tempdir().unwrap();
        let target = directory.path().join("operator-dir");
        std::fs::create_dir(&target).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o500)).unwrap();
            create_private_dir(&target).unwrap();
            let mode = std::fs::metadata(&target).unwrap().permissions().mode() & 0o777;
            assert_eq!(
                mode, 0o500,
                "an existing operator directory must keep its mode"
            );
        }
        #[cfg(not(unix))]
        {
            create_private_dir(&target).unwrap();
        }
    }

    #[test]
    fn newly_created_private_directory_gets_owner_only_permissions() {
        let directory = tempfile::tempdir().unwrap();
        let target = directory.path().join("new-private");
        create_private_dir(&target).unwrap();
        assert!(target.is_dir());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&target).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o700);
        }
    }

    #[test]
    fn label_slug_is_bounded_and_readable() {
        assert_eq!(label_slug("Before agent testing"), "before-agent-testing");
        assert_eq!(label_slug("!!!"), "backup");
        assert!(label_slug(&"x".repeat(200)).len() <= 40);
    }

    #[test]
    fn relative_archive_path_uses_forward_slashes() {
        let root = Path::new("/data/orbit");
        let note = Path::new("/data/orbit/knowledge/Work/one.md");
        assert_eq!(
            relative_archive_path(root, note).unwrap(),
            "knowledge/Work/one.md"
        );
    }
}
