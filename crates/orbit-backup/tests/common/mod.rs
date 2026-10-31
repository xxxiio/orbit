//! Shared real-workspace fixtures for `orbit-backup` integration tests.
//!
//! Every fixture is a real SQLite database, a real Markdown vault directory and
//! a real content-addressed attachment object store. Nothing is mocked.

#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};

use rusqlite::{Connection, params};
use sha2::{Digest, Sha256};

use orbit_backup::tar_fmt::{TarReader, TarWriter};
use orbit_backup::{BackupOptions, BackupOutcome, BackupService, NoProgress, WorkspaceLayout};

/// One disposable Orbit workspace on disk.
pub struct TestWorkspace {
    directory: tempfile::TempDir,
    pub database: PathBuf,
    pub vault: PathBuf,
    pub attachment_store: PathBuf,
    /// Keep a connection open to the source database (WAL writers keep the WAL
    /// file alive, which is exactly the condition a backup must tolerate).
    pub connection: Option<Connection>,
}

impl TestWorkspace {
    pub fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let database = directory.path().join("orbit.db");
        let vault = directory.path().join("orbit.knowledge");
        let attachment_store = directory.path().join("attachments").join("objects");
        fs::create_dir_all(&vault).unwrap();
        fs::create_dir_all(&attachment_store).unwrap();
        Self {
            directory,
            database,
            vault,
            attachment_store,
            connection: None,
        }
    }

    pub fn root(&self) -> &Path {
        self.directory.path()
    }

    pub fn layout(&self) -> WorkspaceLayout {
        WorkspaceLayout::for_database(&self.database).unwrap()
    }

    pub fn schema_version(&self) -> i64 {
        orbit_backup::supported_schema_version()
    }

    /// Create the authoritative database and keep a WAL-mode connection open.
    pub fn open_database(&mut self) -> &mut Connection {
        let connection = Connection::open(&self.database).unwrap();
        connection
            .busy_timeout(std::time::Duration::from_millis(500))
            .unwrap();
        let schema_version = self.schema_version();
        connection
            .execute_batch(&format!(
                "PRAGMA journal_mode = WAL;
                 PRAGMA foreign_keys = ON;
                 CREATE TABLE IF NOT EXISTS orbit_schema (
                     id INTEGER PRIMARY KEY NOT NULL, version INTEGER NOT NULL);
                 INSERT OR REPLACE INTO orbit_schema(id, version) VALUES (1, {schema_version});
                 CREATE TABLE IF NOT EXISTS authority_projection (
                     id INTEGER PRIMARY KEY NOT NULL, authority_revision INTEGER NOT NULL);
                 INSERT OR REPLACE INTO authority_projection(id, authority_revision) VALUES (1, 7);
                 CREATE TABLE IF NOT EXISTS authority_identity (
                     id INTEGER PRIMARY KEY NOT NULL, generation TEXT NOT NULL);
                 INSERT OR REPLACE INTO authority_identity(id, generation) VALUES (1, 'generation-test');
                 CREATE TABLE IF NOT EXISTS projects (
                     id TEXT PRIMARY KEY NOT NULL, name TEXT NOT NULL);
                 INSERT OR REPLACE INTO projects(id, name) VALUES ('project-1', 'Orbit');
                 CREATE TABLE IF NOT EXISTS tasks (
                     id TEXT PRIMARY KEY NOT NULL, title TEXT NOT NULL);
                 INSERT OR REPLACE INTO tasks(id, title) VALUES ('task-1', 'First task');
                 CREATE TABLE IF NOT EXISTS attachment_objects (
                     object_hash TEXT PRIMARY KEY NOT NULL, size_bytes INTEGER NOT NULL);
                 CREATE TABLE IF NOT EXISTS attachments (
                     id TEXT PRIMARY KEY NOT NULL, object_hash TEXT NOT NULL,
                     owner_kind TEXT NOT NULL, owner_id TEXT NOT NULL,
                     filename TEXT NOT NULL, content_type TEXT NOT NULL,
                     size_bytes INTEGER NOT NULL, checksum TEXT NOT NULL,
                     created_at_ms INTEGER NOT NULL);"
            ))
            .unwrap();
        self.connection = Some(connection);
        self.connection.as_mut().unwrap()
    }

    pub fn connection(&mut self) -> &mut Connection {
        if self.connection.is_none() {
            self.open_database();
        }
        self.connection.as_mut().unwrap()
    }

    /// Write one Markdown note into the vault, creating parent directories.
    pub fn write_note(&self, relative: &str, body: &str) -> PathBuf {
        let path = self.vault.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, body).unwrap();
        path
    }

    /// Create an (empty) vault directory, including parents.
    pub fn create_vault_dir(&self, relative: &str) {
        fs::create_dir_all(self.vault.join(relative)).unwrap();
    }

    /// Store attachment content in the content-addressed object store.
    pub fn store_attachment_file(&self, content: &[u8]) -> String {
        let hash = sha256_hex(content);
        let path = self.object_path(&hash);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, content).unwrap();
        hash
    }

    /// Record one attachment object and one association in the database.
    pub fn record_attachment(&mut self, content: &[u8]) {
        let hash = self.store_attachment_file(content);
        let size = content.len() as i64;
        let connection = self.connection();
        connection
            .execute(
                "INSERT OR REPLACE INTO attachment_objects(object_hash, size_bytes) VALUES (?1, ?2)",
                params![hash, size],
            )
            .unwrap();
        connection
            .execute(
                "INSERT OR REPLACE INTO attachments(
                     id, object_hash, owner_kind, owner_id, filename, content_type,
                     size_bytes, checksum, created_at_ms)
                 VALUES (?1, ?2, 'project', 'project-1', 'spec.pdf', 'application/pdf', ?3, ?4, 0)",
                params![format!("attachment-{hash}"), hash, size, hash],
            )
            .unwrap();
    }

    pub fn object_path(&self, hash: &str) -> PathBuf {
        self.attachment_store.join(&hash[..2]).join(hash)
    }

    pub fn create_backup(&self, destination: &Path, options: BackupOptions) -> BackupOutcome {
        BackupService::new(self.layout())
            .create(destination, options, &mut NoProgress)
            .unwrap()
    }

    pub fn try_create_backup(
        &self,
        destination: &Path,
        options: BackupOptions,
    ) -> orbit_backup::Result<BackupOutcome> {
        BackupService::new(self.layout()).create(destination, options, &mut NoProgress)
    }
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Read every entry (manifest first) from a real archive on disk.
pub fn read_archive_entries(path: &Path) -> Vec<(String, Vec<u8>)> {
    let file = fs::File::open(path).unwrap();
    let mut reader = TarReader::new(std::io::BufReader::new(file));
    let mut entries = Vec::new();
    while let Some(header) = reader.next_header().unwrap() {
        let mut bytes = Vec::new();
        reader.copy_data(header.size, &mut bytes).unwrap();
        entries.push((header.path, bytes));
    }
    entries
}

/// Rewrite an archive from entries (used to craft tampered archives).
pub fn write_archive_entries(path: &Path, entries: &[(String, Vec<u8>)]) {
    let file = fs::File::create(path).unwrap();
    let mut writer = TarWriter::new(std::io::BufWriter::new(file));
    for (entry_path, bytes) in entries {
        writer.append_bytes(entry_path, bytes).unwrap();
    }
    writer.finish().unwrap();
}

/// Rewrite the manifest entry of an archive while keeping every other entry.
pub fn rewrite_manifest(path: &Path, mutate: impl FnOnce(&mut serde_json::Value)) {
    let entries = read_archive_entries(path);
    let mut mutate = Some(mutate);
    let mut rewritten = Vec::with_capacity(entries.len());
    for (entry_path, bytes) in entries {
        if entry_path == orbit_backup::MANIFEST_ENTRY {
            let mut value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            if let Some(mutate) = mutate.take() {
                mutate(&mut value);
            }
            let mut new_bytes = serde_json::to_vec_pretty(&value).unwrap();
            new_bytes.push(b'\n');
            rewritten.push((entry_path, new_bytes));
        } else {
            rewritten.push((entry_path, bytes));
        }
    }
    write_archive_entries(path, &rewritten);
}

/// Files in a directory with a given extension, sorted by name.
pub fn files_with_extension(directory: &Path, extension: &str) -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = fs::read_dir(directory)
        .unwrap()
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.extension() == Some(std::ffi::OsStr::new(extension)))
        .collect();
    paths.sort();
    paths
}
