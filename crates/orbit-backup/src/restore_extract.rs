//! Restore extraction support: archive-to-staging materialization.
//!
//! Owned by the I2 restore implementation. This module stays private support
//! code: it extracts a verified archive into a staging directory with per-path
//! and per-hash validation, verifies staged database/attachment state, and
//! merges content-addressed attachment objects. Public orchestration and types
//! stay in `restore.rs`; workspace preservation, journal and activation live in
//! `restore_replace.rs`.
//!
//! Two invariants shape everything here:
//!
//! - Nothing is written to the active workspace while extracting or verifying;
//!   a fully verified staging tree is the only input to activation.
//! - The manifest inside the archive is re-read and must equal the manifest
//!   verified during preflight, so a swapped archive cannot be restored.

use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};

use rusqlite::{Connection, OpenFlags};
use sha2::{Digest, Sha256};

use crate::error::{BackupError, Result};
use crate::layout::WorkspaceLayout;
use crate::manifest::{
    ATTACHMENT_PREFIX, BackupCounts, BackupManifest, CONFIG_ENTRY, DATABASE_ENTRY, InventoryEntry,
    KNOWLEDGE_PREFIX, MANIFEST_ENTRY,
};
use crate::snapshot;
use crate::tar_fmt::TarReader;
use crate::util;

/// Staged authoritative database file inside a restore staging root.
pub const STAGED_DATABASE: &str = "orbit.db";
/// Staged knowledge vault directory inside a restore staging root.
pub const STAGED_KNOWLEDGE: &str = "orbit.knowledge";
/// Staged attachment object directory (`<aa>/<sha256>` leaves).
pub const STAGED_ATTACHMENTS: &str = "attachments/objects";
/// Staged normalized, non-secret configuration file.
pub const STAGED_CONFIG: &str = "config/config.json";

/// Facts about one completed extraction.
#[derive(Debug, Clone)]
pub struct Extraction {
    /// SHA-256 of the `manifest.json` bytes actually extracted.
    pub manifest_sha256: String,
    /// Number of inventoried files materialized.
    pub files: u64,
    /// Total content bytes materialized.
    pub bytes: u64,
}

/// Digesting writer used while streaming tar content to a staging file.
struct HashingWriter<W: Write> {
    inner: W,
    hasher: Sha256,
    written: u64,
}

impl<W: Write> HashingWriter<W> {
    fn new(inner: W) -> Self {
        Self {
            inner,
            hasher: Sha256::new(),
            written: 0,
        }
    }

    fn finish(mut self) -> Result<(u64, String)> {
        self.inner
            .flush()
            .map_err(|error| BackupError::io("flush staged file", error))?;
        Ok((self.written, util::hex_digest(self.hasher.finalize())))
    }
}

impl<W: Write> Write for HashingWriter<W> {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        let written = self.inner.write(buffer)?;
        self.hasher.update(&buffer[..written]);
        self.written += written as u64;
        Ok(written)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

/// Extract a verified archive into an empty staging root.
///
/// The manifest is re-read and must equal the preflighted manifest; every entry
/// must be declared exactly once, stay inside the staging root, and match its
/// declared size and SHA-256. Undeclared, duplicate, directory-typed, missing
/// and traversal entries are refused.
pub fn extract_into(root: &Path, archive: &Path, expected: &BackupManifest) -> Result<Extraction> {
    if !root.is_dir() {
        return Err(BackupError::InvalidArgument(format!(
            "restore staging root {} is not a directory",
            root.display()
        )));
    }
    let file = File::open(archive)
        .map_err(|error| BackupError::io(format!("open archive {}", archive.display()), error))?;
    let mut reader = TarReader::new(BufReader::new(file));
    let manifest_sha256 = read_archive_manifest(&mut reader, expected)?;
    let inventory: BTreeMap<&str, &InventoryEntry> = expected
        .inventory
        .iter()
        .map(|entry| (entry.path.as_str(), entry))
        .collect();
    let mut seen: BTreeMap<String, ()> = BTreeMap::new();
    let mut files = 0_u64;
    let mut bytes = 0_u64;
    while let Some(header) = reader.next_header()? {
        if header.is_directory {
            return Err(BackupError::Archive(format!(
                "archive entry {} is a directory; .orbitbak archives contain regular files only",
                header.path
            )));
        }
        let entry = inventory.get(header.path.as_str()).copied().ok_or_else(|| {
            BackupError::Archive(format!(
                "archive contains undeclared entry {}; it is not listed in the manifest inventory",
                header.path
            ))
        })?;
        if seen.insert(header.path.clone(), ()).is_some() {
            return Err(BackupError::Archive(format!(
                "archive contains duplicate entry {}",
                header.path
            )));
        }
        if header.size != entry.size_bytes {
            return Err(BackupError::Integrity(format!(
                "entry {} is {} bytes but the manifest records {}",
                header.path, header.size, entry.size_bytes
            )));
        }
        let staged = staged_relative_path(&header.path)?;
        let destination = util::join_archive_path(root, &staged)?;
        util::ensure_parent(&destination)?;
        let output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&destination)
            .map_err(|error| {
                BackupError::io(
                    format!("create staged file {}", destination.display()),
                    error,
                )
            })?;
        let mut writer = HashingWriter::new(BufWriter::new(output));
        reader.copy_data(header.size, &mut writer)?;
        let (written, digest) = writer.finish()?;
        if written != entry.size_bytes || digest != entry.sha256 {
            return Err(BackupError::Integrity(format!(
                "staged content for {} does not match the manifest ({} bytes, sha256 {digest})",
                header.path, written
            )));
        }
        util::fsync_file(&destination)?;
        files += 1;
        bytes = bytes.saturating_add(written);
    }
    if files as usize != expected.inventory.len() {
        let missing = expected
            .inventory
            .iter()
            .find(|entry| !seen.contains_key(entry.path.as_str()))
            .map(|entry| entry.path.as_str())
            .unwrap_or("unknown");
        return Err(BackupError::Integrity(format!(
            "archive ended after {files} of {} inventoried files (first missing: {missing})",
            expected.inventory.len()
        )));
    }
    create_required_directories(root, expected)?;
    util::fsync_dir(root)?;
    Ok(Extraction {
        manifest_sha256,
        files,
        bytes,
    })
}

fn read_archive_manifest<R: Read>(
    reader: &mut TarReader<R>,
    expected: &BackupManifest,
) -> Result<String> {
    let header = reader
        .next_header()?
        .ok_or_else(|| BackupError::Archive("archive contains no entries".to_owned()))?;
    if header.path != MANIFEST_ENTRY {
        return Err(BackupError::Archive(format!(
            "the first archive entry must be {MANIFEST_ENTRY}, found {}",
            header.path
        )));
    }
    let mut bytes = Vec::new();
    reader.copy_data(header.size, &mut bytes)?;
    let manifest = BackupManifest::from_json(&bytes)?;
    manifest.validate()?;
    if manifest != *expected {
        return Err(BackupError::Integrity(
            "the manifest inside the archive differs from the manifest verified during preflight; \
             the archive changed underneath the restore"
                .to_owned(),
        ));
    }
    Ok(util::hex_digest(Sha256::digest(&bytes)))
}

/// Map one archive path to its path inside a restore staging root.
pub fn staged_relative_path(path: &str) -> Result<String> {
    if path == DATABASE_ENTRY {
        return Ok(STAGED_DATABASE.to_owned());
    }
    if let Some(rest) = path.strip_prefix(KNOWLEDGE_PREFIX) {
        if rest.is_empty() {
            return Err(BackupError::Archive(format!(
                "archive entry {path} has an empty vault-relative path"
            )));
        }
        return Ok(format!("{STAGED_KNOWLEDGE}/{rest}"));
    }
    if let Some(rest) = path.strip_prefix(ATTACHMENT_PREFIX) {
        if rest.is_empty() {
            return Err(BackupError::Archive(format!(
                "archive entry {path} has an empty attachment object path"
            )));
        }
        return Ok(format!("{STAGED_ATTACHMENTS}/{rest}"));
    }
    if path == CONFIG_ENTRY {
        return Ok(STAGED_CONFIG.to_owned());
    }
    Err(BackupError::Archive(format!(
        "archive entry {path} is not a permitted workspace component"
    )))
}

fn create_required_directories(root: &Path, manifest: &BackupManifest) -> Result<()> {
    for relative in [STAGED_KNOWLEDGE, STAGED_ATTACHMENTS] {
        util::create_private_dir(&util::join_archive_path(root, relative)?)?;
    }
    for directory in &manifest.directories {
        let trimmed = directory.trim_end_matches('/');
        let rest = trimmed
            .strip_prefix(KNOWLEDGE_PREFIX)
            .ok_or_else(|| BackupError::UnsafePath(directory.clone()))?;
        if rest.is_empty() {
            continue;
        }
        util::create_private_dir(&util::join_archive_path(
            root,
            &format!("{STAGED_KNOWLEDGE}/{rest}"),
        )?)?;
    }
    Ok(())
}

/// Verify a fully extracted staging tree before it may be activated.
pub fn verify_staged_workspace(root: &Path, manifest: &BackupManifest) -> Result<BackupCounts> {
    verify_workspace_components(
        &root.join(STAGED_DATABASE),
        &root.join(STAGED_KNOWLEDGE),
        &root.join(STAGED_ATTACHMENTS),
        manifest,
    )
}

/// Verify an activated workspace after database, vault and attachments exist.
pub fn verify_activated_workspace(
    layout: &WorkspaceLayout,
    manifest: &BackupManifest,
) -> Result<BackupCounts> {
    verify_workspace_components(
        layout.database(),
        layout.knowledge_vault(),
        layout.attachment_store(),
        manifest,
    )
}

fn verify_workspace_components(
    database: &Path,
    vault: &Path,
    attachments: &Path,
    manifest: &BackupManifest,
) -> Result<BackupCounts> {
    let supported = crate::supported_schema_version();
    if manifest.schema_version > supported {
        return Err(BackupError::IncompatibleSchema {
            backup: manifest.schema_version,
            supported,
        });
    }
    let inspection = snapshot::inspect_snapshot(database)?;
    if !inspection.orbit_schema_present {
        return Err(BackupError::Integrity(format!(
            "restored database {} has no orbit_schema table; it is not an Orbit workspace",
            database.display()
        )));
    }
    if !inspection.integrity_ok {
        return Err(BackupError::Integrity(format!(
            "PRAGMA integrity_check failed for restored database {}",
            database.display()
        )));
    }
    if inspection.schema_version != Some(manifest.schema_version) {
        return Err(BackupError::Integrity(format!(
            "restored database schema {:?} does not match the archive schema {}",
            inspection.schema_version, manifest.schema_version
        )));
    }
    let tables = table_counts(database)?;
    for (table, expected) in &manifest.counts.tables {
        let actual = tables.get(table).copied().unwrap_or(0);
        if actual != *expected {
            return Err(BackupError::Integrity(format!(
                "restored table {table} has {actual} row(s); the archive manifest records {expected}"
            )));
        }
    }
    let references = attachment_references(database)?;
    verify_attachment_presence(attachments, &references)?;
    let notes = count_note_files(vault)?;
    let declared_notes = manifest.knowledge_paths().count() as u64;
    if notes != declared_notes {
        return Err(BackupError::Integrity(format!(
            "restored vault has {notes} Markdown note(s); the archive inventory declares {declared_notes}"
        )));
    }
    let objects = manifest.attachment_paths().count() as u64;
    if manifest.counts.attachment_objects != 0 && objects != manifest.counts.attachment_objects {
        return Err(BackupError::Integrity(format!(
            "archive inventory lists {objects} attachment object(s); the manifest counts {}",
            manifest.counts.attachment_objects
        )));
    }
    Ok(assemble_counts(manifest, tables, notes, objects))
}

fn assemble_counts(
    manifest: &BackupManifest,
    tables: BTreeMap<String, u64>,
    notes: u64,
    attachments: u64,
) -> BackupCounts {
    let mut counts = manifest.counts.clone();
    counts.tables = tables;
    if counts.knowledge_notes == 0 {
        counts.knowledge_notes = notes;
    }
    if counts.attachment_objects == 0 {
        counts.attachment_objects = attachments;
    }
    counts
}

/// Count rows in every non-internal table of a database, read-only.
pub fn table_counts(database: &Path) -> Result<BTreeMap<String, u64>> {
    let connection = open_readonly(database)?;
    let mut statement = connection
        .prepare(
            "SELECT name FROM sqlite_master WHERE type = 'table' \
             AND name NOT LIKE 'sqlite_%' ORDER BY name",
        )
        .map_err(|error| readonly_error("list tables", error))?;
    let names: Vec<String> = statement
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|error| readonly_error("query table names", error))?
        .collect::<std::result::Result<_, _>>()
        .map_err(|error| readonly_error("read table name", error))?;
    let mut counts = BTreeMap::new();
    for name in names {
        let quoted = name.replace('"', "\"\"");
        let count: i64 = connection
            .query_row(&format!("SELECT COUNT(*) FROM \"{quoted}\""), [], |row| {
                row.get(0)
            })
            .map_err(|error| readonly_error("count table rows", error))?;
        counts.insert(name, count.max(0) as u64);
    }
    Ok(counts)
}

/// Read attachment object hashes and declared sizes from a database, read-only.
pub fn attachment_references(database: &Path) -> Result<Vec<(String, u64)>> {
    let connection = open_readonly(database)?;
    if !table_exists(&connection, "attachment_objects")? {
        return Ok(Vec::new());
    }
    let mut statement = connection
        .prepare("SELECT object_hash, size_bytes FROM attachment_objects ORDER BY object_hash")
        .map_err(|error| readonly_error("prepare attachment object query", error))?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?.max(0) as u64,
            ))
        })
        .map_err(|error| readonly_error("query attachment objects", error))?;
    let mut references = Vec::new();
    for row in rows {
        references.push(row.map_err(|error| readonly_error("read attachment object", error))?);
    }
    Ok(references)
}

fn verify_attachment_presence(store: &Path, references: &[(String, u64)]) -> Result<()> {
    for (object_hash, size_bytes) in references {
        if object_hash.len() < 2 {
            return Err(BackupError::Integrity(format!(
                "database records an invalid attachment object hash {object_hash:?}"
            )));
        }
        let relative = format!("{}/{object_hash}", &object_hash[..2]);
        let path = util::join_archive_path(store, &relative)?;
        let metadata = fs::metadata(&path).map_err(|_| {
            BackupError::Integrity(format!(
                "restored attachment store is missing object {object_hash} referenced by the database"
            ))
        })?;
        if !metadata.is_file() || metadata.len() != *size_bytes {
            return Err(BackupError::Integrity(format!(
                "restored attachment object {object_hash} is not a {size_bytes}-byte file"
            )));
        }
    }
    Ok(())
}

/// Count Markdown notes in a vault tree without following symlinks.
pub fn count_note_files(vault: &Path) -> Result<u64> {
    let mut count = 0_u64;
    for path in walk_files(vault)? {
        if path.extension().is_some_and(|extension| extension == "md") {
            count += 1;
        }
    }
    Ok(count)
}

/// Recursively list regular files under `root` in a stable order.
pub fn walk_files(root: &Path) -> Result<Vec<PathBuf>> {
    if !root.exists() {
        return Ok(Vec::new());
    }
    if !root.is_dir() {
        return Err(BackupError::InvalidArgument(format!(
            "{} is not a directory",
            root.display()
        )));
    }
    let mut files = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        let entries = fs::read_dir(&directory)
            .map_err(|error| BackupError::io(format!("list {}", directory.display()), error))?;
        for entry in entries {
            let entry = entry.map_err(|error| {
                BackupError::io(format!("read directory {}", directory.display()), error)
            })?;
            let file_type = entry.file_type().map_err(|error| {
                BackupError::io(format!("read file type {}", entry.path().display()), error)
            })?;
            if file_type.is_dir() {
                pending.push(entry.path());
            } else if file_type.is_file() {
                files.push(entry.path());
            } else {
                return Err(BackupError::Integrity(format!(
                    "workspace contains an unexpected non-regular entry {}",
                    entry.path().display()
                )));
            }
        }
    }
    files.sort();
    Ok(files)
}

/// Merge verified staged attachment objects into a workspace object store.
///
/// Objects are immutable and content-addressed: an existing object is verified
/// (and never overwritten), a missing object is hard-linked or copied in. No
/// object that was already in the store is ever removed. Returns the number of
/// newly added objects.
pub fn merge_attachment_objects(staged: &Path, store: &Path) -> Result<u64> {
    fs::create_dir_all(store)
        .map_err(|error| BackupError::io(format!("create {}", store.display()), error))?;
    let mut merged = 0_u64;
    for path in walk_files(staged)? {
        let relative = util::relative_archive_path(staged, &path)?;
        let target = util::join_archive_path(store, &relative)?;
        if target.exists() {
            verify_identical_object(&path, &target)?;
            continue;
        }
        util::ensure_parent(&target)?;
        if fs::hard_link(&path, &target).is_err() {
            fs::copy(&path, &target).map_err(|error| {
                BackupError::io(
                    format!("copy attachment object to {}", target.display()),
                    error,
                )
            })?;
            util::fsync_file(&target)?;
        }
        merged += 1;
    }
    if merged > 0 {
        util::fsync_dir(store)?;
    }
    Ok(merged)
}

fn verify_identical_object(staged: &Path, existing: &Path) -> Result<()> {
    let (staged_size, staged_hash) = util::hash_file(staged)?;
    let (existing_size, existing_hash) = util::hash_file(existing)?;
    if staged_size != existing_size || staged_hash != existing_hash {
        return Err(BackupError::Integrity(format!(
            "attachment store already holds a different object at {}; refusing to overwrite an \
             immutable content-addressed object",
            existing.display()
        )));
    }
    Ok(())
}

fn open_readonly(database: &Path) -> Result<Connection> {
    Connection::open_with_flags(database, OpenFlags::SQLITE_OPEN_READ_ONLY).map_err(|error| {
        BackupError::Integrity(format!(
            "open restored database {} read-only: {error}",
            database.display()
        ))
    })
}

fn table_exists(connection: &Connection, name: &str) -> Result<bool> {
    let exists: bool = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1)",
            [name],
            |row| row.get(0),
        )
        .map_err(|error| readonly_error("check table existence", error))?;
    Ok(exists)
}

fn readonly_error(context: &str, error: rusqlite::Error) -> BackupError {
    BackupError::Integrity(format!("{context}: {error}"))
}
