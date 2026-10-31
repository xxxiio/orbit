//! Archive inspection: read the manifest without extracting or mutating.
//!
//! Inspection is cheap: the manifest is the first archive entry, so an
//! operator can list contents, sizes and compatibility without paying for
//! hash verification. Use [`crate::verify_archive`] for verification.

use std::fs;
use std::io::BufReader;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::error::{BackupError, Result};
use crate::manifest::{BackupManifest, MANIFEST_ENTRY};
use crate::tar_fmt::TarReader;
use crate::verify::VerificationSidecar;

/// Result of inspecting one archive.
#[derive(Debug, Clone, Serialize)]
pub struct BackupInspection {
    /// Archive path as supplied by the caller.
    pub archive_path: String,
    /// Archive file size in bytes.
    pub archive_bytes: u64,
    /// Parsed manifest.
    pub manifest: BackupManifest,
    /// Number of inventoried files.
    pub inventory_files: usize,
    /// Bytes of inventoried content (database, vault, attachments, config).
    pub content_bytes: u64,
    /// Verification sidecar recorded at creation, when present and readable.
    pub creation_verification: Option<VerificationSidecar>,
}

/// Read only the manifest entry from an archive.
///
/// The manifest is parsed and validated (format compatibility, path safety and
/// internal consistency), so a corrupt or newer archive fails with an
/// actionable error instead of being reported as inspectable.
pub fn read_manifest_only(path: &Path) -> Result<BackupManifest> {
    let file = fs::File::open(path)
        .map_err(|error| BackupError::io(format!("open archive {}", path.display()), error))?;
    let mut reader = TarReader::new(BufReader::new(file));
    let header = match reader.next_header()? {
        Some(header) => header,
        None => {
            return Err(BackupError::Archive(format!(
                "{} is an empty archive (no {} entry)",
                path.display(),
                MANIFEST_ENTRY
            )));
        }
    };
    if header.path != MANIFEST_ENTRY {
        return Err(BackupError::Archive(format!(
            "{} is not an Orbit backup: first entry is {}, expected {}",
            path.display(),
            header.path,
            MANIFEST_ENTRY
        )));
    }
    let mut bytes = Vec::new();
    reader.copy_data(header.size, &mut bytes)?;
    let manifest = BackupManifest::from_json(&bytes)?;
    manifest.validate()?;
    manifest.check_schema_compatibility(crate::supported_schema_version())?;
    Ok(manifest)
}

/// Inspect an archive without full verification.
pub fn inspect_archive(path: &Path) -> Result<BackupInspection> {
    let archive_bytes = fs::metadata(path)
        .map_err(|error| BackupError::io(format!("open archive {}", path.display()), error))?
        .len();
    let manifest = read_manifest_only(path)?;
    let creation_verification = read_sidecar(path);
    Ok(BackupInspection {
        archive_path: path.display().to_string(),
        archive_bytes,
        inventory_files: manifest.inventory.len(),
        content_bytes: manifest.totals.content_bytes,
        manifest,
        creation_verification,
    })
}

/// Read the verification sidecar beside an archive, when present and valid.
pub(crate) fn read_sidecar(archive: &Path) -> Option<VerificationSidecar> {
    let path = sidecar_path(archive);
    let bytes = fs::read(path).ok()?;
    serde_json::from_slice(&bytes).ok()
}

/// Path of the verification sidecar for an archive.
pub(crate) fn sidecar_path(archive: &Path) -> PathBuf {
    let mut text = archive.as_os_str().to_owned();
    text.push(".");
    text.push(crate::manifest::VERIFICATION_SIDECAR_SUFFIX);
    PathBuf::from(text)
}
