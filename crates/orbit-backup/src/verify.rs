//! Archive verification: structure, manifest, hashes, SQLite and coverage.
//!
//! Verification answers "is this archive internally sound and compatible with
//! this build?". It never implies that a restore has happened or that the
//! recovered application behaves correctly. A valid checksum is not a
//! successful restore.
//!
//! Contract used by restore preflight, retention and the CLI:
//! [`verify_archive`] returns `Ok(VerificationReport)` for every *content*
//! problem (bad manifest, unsafe path, hash/size mismatch, SQLite corruption,
//! schema incompatibility, missing attachment coverage, bad tar magic,
//! truncation). It returns `Err(BackupError)` only when the archive cannot be
//! opened at all (missing/unreadable file).

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{self, BufReader, Write};
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::error::{BackupError, Result};
use crate::manifest::{BackupManifest, DATABASE_ENTRY, InventoryEntry, MANIFEST_ENTRY};
use crate::tar_fmt::TarReader;
use crate::util::{self, TempDir};
use crate::verify_support::{CheckSet, coverage_checks};

pub use crate::verify_types::{
    CheckStatus, VerificationCheck, VerificationProblem, VerificationReport, VerificationSidecar,
    VerifyOptions,
};

type ArchiveReader = TarReader<BufReader<fs::File>>;

/// Path of the verification sidecar for an archive.
pub(crate) fn sidecar_path(archive: &Path) -> PathBuf {
    let mut text = archive.as_os_str().to_owned();
    text.push(".");
    text.push(crate::manifest::VERIFICATION_SIDECAR_SUFFIX);
    PathBuf::from(text)
}

/// Verify a `.orbitbak` archive.
pub fn verify_archive(path: &Path, options: &VerifyOptions) -> Result<VerificationReport> {
    let metadata = fs::metadata(path)
        .map_err(|error| BackupError::io(format!("open archive {}", path.display()), error))?;
    if !metadata.is_file() {
        return Err(BackupError::NotFound(format!(
            "backup archive is not a regular file: {}",
            path.display()
        )));
    }
    let archive_bytes = metadata.len();
    let file = fs::File::open(path)
        .map_err(|error| BackupError::io(format!("open archive {}", path.display()), error))?;
    let mut reader = TarReader::new(BufReader::new(file));

    let mut checks = CheckSet::new();
    let manifest = read_manifest_entry(&mut reader, &mut checks);
    let extraction = match options.deep {
        true => Some(TempDir::new(std::env::temp_dir(), "orbit-verify")?),
        false => None,
    };
    let scan = scan_entries(
        &mut reader,
        manifest.as_ref(),
        &mut checks,
        extraction.as_ref().map(TempDir::path),
    );
    coverage_checks(manifest.as_ref(), &scan, &mut checks, options.deep);

    let mut report = VerificationReport::empty();
    report.archive_bytes = archive_bytes;
    if let Some(manifest) = &manifest {
        report.format_version = manifest.format_version;
        report.schema_version = Some(manifest.schema_version);
        report.verified_files = manifest
            .inventory
            .iter()
            .filter(|entry| scan.verified.contains(&entry.path))
            .cloned()
            .collect();
    }
    report.manifest = manifest;
    checks.finish(&mut report);
    Ok(report)
}

// ---------------------------------------------------------------------------
// Streaming pass: manifest first, then every declared file.
// ---------------------------------------------------------------------------

pub(crate) struct ScanOutcome {
    pub(crate) extracted_database: Option<PathBuf>,
    pub(crate) verified: BTreeSet<String>,
}

fn read_manifest_entry(
    reader: &mut ArchiveReader,
    checks: &mut CheckSet,
) -> Option<BackupManifest> {
    let header = match reader.next_header() {
        Ok(Some(header)) => header,
        Ok(None) => {
            checks.fail("structure", "archive contains no entries");
            checks.fail("manifest", format!("archive has no {MANIFEST_ENTRY} entry"));
            return None;
        }
        Err(error) => {
            checks.fail(
                "structure",
                format!("cannot read the first archive entry: {error}"),
            );
            checks.fail("manifest", format!("cannot read {MANIFEST_ENTRY}: {error}"));
            return None;
        }
    };
    if header.path != MANIFEST_ENTRY {
        checks.fail(
            "structure",
            format!(
                "the first archive entry must be {MANIFEST_ENTRY}, found {}",
                header.path
            ),
        );
        checks.fail(
            "manifest",
            format!("{MANIFEST_ENTRY} is not the first archive entry"),
        );
        let _ = reader.skip_data(header.size);
        return None;
    }
    let mut bytes = Vec::new();
    if let Err(error) = reader.copy_data(header.size, &mut bytes) {
        checks.fail("structure", format!("read {MANIFEST_ENTRY}: {error}"));
        return None;
    }
    let manifest = match BackupManifest::from_json(&bytes) {
        Ok(manifest) => manifest,
        Err(error) => {
            checks.fail("manifest", error.to_string());
            return None;
        }
    };
    if let Err(error) = manifest.check_format_compatibility() {
        checks.fail("manifest", error.to_string());
    } else if let Err(error) = manifest.validate() {
        checks.fail("manifest", error.to_string());
    }
    checks.checked("manifest", 1);
    if checks.failure_count("manifest") == 0 {
        checks.pass(
            "manifest",
            format!(
                "manifest v{} (min reader v{}) with schema {} validated",
                manifest.format_version, manifest.min_reader_version, manifest.schema_version
            ),
        );
    }
    Some(manifest)
}

fn scan_entries(
    reader: &mut ArchiveReader,
    manifest: Option<&BackupManifest>,
    checks: &mut CheckSet,
    extraction_root: Option<&Path>,
) -> ScanOutcome {
    let declared: Option<BTreeMap<&str, &InventoryEntry>> = manifest.map(|manifest| {
        manifest
            .inventory
            .iter()
            .map(|entry| (entry.path.as_str(), entry))
            .collect()
    });
    let mut seen = BTreeSet::new();
    let mut verified = BTreeSet::new();
    let mut extracted_database = None;
    let mut entry_count = 0_u64;
    let mut truncated = false;
    loop {
        let header = match reader.next_header() {
            Ok(Some(header)) => header,
            Ok(None) => break,
            Err(BackupError::UnsafePath(detail)) => {
                checks.fail("paths", format!("unsafe archive entry: {detail}"));
                truncated = true;
                break;
            }
            Err(error) => {
                checks.fail("structure", format!("read archive entry: {error}"));
                truncated = true;
                break;
            }
        };
        entry_count += 1;
        if header.is_directory {
            checks.fail(
                "structure",
                format!(
                    "directory entry {} is not permitted; directories are described by the manifest",
                    header.path
                ),
            );
            if reader.skip_data(header.size).is_err() {
                truncated = true;
                break;
            }
            continue;
        }
        if header.path == MANIFEST_ENTRY {
            checks.fail("structure", "archive contains a second manifest.json entry");
            if reader.skip_data(header.size).is_err() {
                truncated = true;
                break;
            }
            continue;
        }
        let Some(declared) = declared.as_ref() else {
            if reader.skip_data(header.size).is_err() {
                truncated = true;
                break;
            }
            continue;
        };
        let Some(entry) = declared.get(header.path.as_str()).copied() else {
            checks.fail(
                "paths",
                format!(
                    "archive entry {} is not declared in the manifest inventory",
                    header.path
                ),
            );
            if reader.skip_data(header.size).is_err() {
                truncated = true;
                break;
            }
            continue;
        };
        if !seen.insert(header.path.clone()) {
            checks.fail(
                "structure",
                format!("archive entry {} appears more than once", header.path),
            );
            if reader.skip_data(header.size).is_err() {
                truncated = true;
                break;
            }
            continue;
        }
        let digest = if header.path == DATABASE_ENTRY && extraction_root.is_some() {
            let destination = extraction_root
                .map(|root| root.join(DATABASE_ENTRY))
                .unwrap_or_else(|| PathBuf::from(DATABASE_ENTRY));
            match extract_and_hash(reader, header.size, &destination) {
                Ok(result) => {
                    extracted_database = Some(destination);
                    result
                }
                Err(error) => {
                    checks.fail(
                        "structure",
                        format!("extract the database snapshot: {error}"),
                    );
                    truncated = true;
                    break;
                }
            }
        } else {
            match reader.hash_data(header.size) {
                Ok(digest) => (header.size, digest),
                Err(error) => {
                    checks.fail(
                        "structure",
                        format!("read content for {}: {error}", header.path),
                    );
                    truncated = true;
                    break;
                }
            }
        };
        let (size, sha256) = digest;
        let size_ok = size == entry.size_bytes;
        let hash_ok = sha256.eq_ignore_ascii_case(&entry.sha256);
        if size_ok && hash_ok {
            verified.insert(header.path.clone());
        } else {
            checks.fail(
                "file_hashes",
                format!(
                    "{}: declared {} bytes / {}, found {} bytes / {}",
                    entry.path,
                    entry.size_bytes,
                    short_digest(&entry.sha256),
                    size,
                    short_digest(&sha256)
                ),
            );
        }
    }

    if let Some(manifest) = manifest {
        for entry in &manifest.inventory {
            if !seen.contains(&entry.path) && !truncated {
                checks.fail(
                    "file_hashes",
                    format!("declared file {} is missing from the archive", entry.path),
                );
            }
        }
        checks.checked("file_hashes", verified.len() as u64);
        if checks.failure_count("file_hashes") == 0 {
            if truncated {
                // Never let a failed stream look like a hash pass: only some
                // (possibly zero) declared files were checked before the
                // structure error stopped the scan.
                checks.skip(
                    "file_hashes",
                    format!(
                        "only {} of {} declared file(s) could be checked before the archive structure failed",
                        verified.len(),
                        manifest.inventory.len()
                    ),
                );
            } else {
                checks.pass(
                    "file_hashes",
                    format!(
                        "{} file(s) match their declared size and SHA-256",
                        verified.len()
                    ),
                );
            }
        }
    } else {
        checks.skip(
            "file_hashes",
            "no manifest inventory is available to check file hashes",
        );
    }

    if declarations_missing(manifest) {
        checks.fail(
            "paths",
            "no manifest inventory is available to validate archive entries",
        );
        checks.skip(
            "file_hashes",
            "no manifest inventory is available to check file hashes",
        );
    } else if checks.failure_count("paths") == 0 {
        checks.pass("paths", "every archive entry is declared, safe and unique");
    }
    if !truncated && checks.failure_count("structure") == 0 {
        checks.pass(
            "structure",
            format!(
                "ustar archive with {entry_count} content entr{} and a valid end-of-archive marker",
                if entry_count == 1 { "y" } else { "ies" }
            ),
        );
    }
    ScanOutcome {
        extracted_database,
        verified,
    }
}

fn declarations_missing(manifest: Option<&BackupManifest>) -> bool {
    manifest.is_none()
}

fn short_digest(digest: &str) -> String {
    digest.chars().take(12).collect()
}

fn extract_and_hash(
    reader: &mut ArchiveReader,
    size: u64,
    destination: &Path,
) -> Result<(u64, String)> {
    util::ensure_parent(destination)?;
    let file = fs::File::create(destination)
        .map_err(|error| BackupError::io(format!("create {}", destination.display()), error))?;
    let mut sink = HashingWriter {
        inner: file,
        hasher: Sha256::new(),
        size: 0,
    };
    reader.copy_data(size, &mut sink)?;
    let (size, digest) = sink.finish();
    Ok((size, digest))
}

struct HashingWriter {
    inner: fs::File,
    hasher: Sha256,
    size: u64,
}

impl HashingWriter {
    fn finish(mut self) -> (u64, String) {
        let _ = self.inner.flush();
        (self.size, util::hex_digest(self.hasher.finalize()))
    }
}

impl Write for HashingWriter {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        self.inner.write_all(buffer)?;
        self.hasher.update(buffer);
        self.size += buffer.len() as u64;
        Ok(buffer.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}
