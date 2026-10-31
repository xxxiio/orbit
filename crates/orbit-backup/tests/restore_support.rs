#![allow(dead_code)]
//! Shared fixtures for restore integration tests.
//!
//! Every restore test starts from a real workspace created with `rusqlite` and
//! turns it into a real `.orbitbak` archive through `BackupService::create`
//! (the production creation path), so restore is exercised end-to-end against
//! the same format the CLI and server write.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use orbit_backup::create::{
    BackupOptions, BackupOutcome, BackupProgressSink, BackupService, NoProgress,
};
use orbit_backup::layout::WorkspaceLayout;
use orbit_backup::manifest::{
    BackupConfigSnapshot, BackupManifest, BackupSourceMode, MANIFEST_ENTRY,
};
use orbit_backup::snapshot::FencedConnection;
use orbit_backup::tar_fmt::{TarReader, TarWriter};
use orbit_backup::util;
use rusqlite::params;

/// Description of the disposable workspace a test wants to create.
pub struct WorkspaceSpec<'a> {
    /// Number of `tasks` rows.
    pub tasks: usize,
    /// Content of `orbit.knowledge/Work/note.md`.
    pub note: &'a str,
    /// Optional content of one attachment object referenced by the database.
    pub attachment: Option<&'a [u8]>,
    /// SQLite journal mode (`delete` for a clean full-tree mutation check).
    pub journal_mode: &'a str,
    /// Pending (unexported) `sync_outbox` rows.
    pub outbox_rows: usize,
    /// Already-exported `sync_outbox` rows (history that must not block).
    pub exported_outbox_rows: usize,
    /// Enrolled `devices` rows.
    pub devices: usize,
    /// `web_sessions` rows.
    pub web_sessions: usize,
    /// Override the recorded `orbit_schema.version` (for compatibility tests).
    pub schema_version: Option<i64>,
    /// Add the connected-replica marker tables the runtime uses to detect a replica.
    pub replica_markers: bool,
}

impl<'a> Default for WorkspaceSpec<'a> {
    fn default() -> Self {
        Self {
            tasks: 1,
            note: "# Note\n",
            attachment: None,
            journal_mode: "delete",
            outbox_rows: 0,
            exported_outbox_rows: 0,
            devices: 0,
            web_sessions: 0,
            schema_version: None,
            replica_markers: false,
        }
    }
}

/// A disposable workspace on disk.
pub struct Fixture {
    pub root: PathBuf,
    pub layout: WorkspaceLayout,
    pub attachment_hash: Option<String>,
}

/// Create one disposable authoritative-looking workspace.
pub fn build_workspace(root: &Path, spec: &WorkspaceSpec<'_>) -> Fixture {
    fs::create_dir_all(root).expect("create workspace dir");
    let database = root.join("orbit.db");
    let connection = rusqlite::Connection::open(&database).expect("open workspace db");
    connection
        .execute_batch(&format!("PRAGMA journal_mode={};", spec.journal_mode))
        .expect("set journal mode");
    let schema = spec
        .schema_version
        .unwrap_or_else(orbit_backup::supported_schema_version);
    connection
        .execute_batch(&format!(
            "CREATE TABLE orbit_schema (id INTEGER PRIMARY KEY, version INTEGER);
             INSERT INTO orbit_schema(id, version) VALUES (1, {schema});
             CREATE TABLE authority_identity (id INTEGER PRIMARY KEY, generation TEXT NOT NULL);
             INSERT INTO authority_identity VALUES (1, 'test-generation');
             CREATE TABLE authority_projection (id INTEGER PRIMARY KEY, authority_revision INTEGER NOT NULL, view_json TEXT NOT NULL);
             INSERT INTO authority_projection VALUES (1, 7, '{{}}');
             CREATE TABLE tasks (id TEXT PRIMARY KEY, title TEXT NOT NULL);
             CREATE TABLE attachment_objects (object_hash TEXT PRIMARY KEY, size_bytes INTEGER NOT NULL);
             CREATE TABLE attachments (id TEXT PRIMARY KEY, object_hash TEXT NOT NULL);
             CREATE TABLE sync_outbox (
                 device_id TEXT NOT NULL,
                 sequence INTEGER NOT NULL,
                 recorded_at_ms INTEGER NOT NULL,
                 entity_kind TEXT NOT NULL,
                 entity_id TEXT NOT NULL,
                 exported_at_ms INTEGER,
                 PRIMARY KEY(device_id, sequence)
             );
             CREATE TABLE devices (
                 id TEXT PRIMARY KEY NOT NULL,
                 token_hash BLOB NOT NULL,
                 revoked INTEGER NOT NULL DEFAULT 0
             );
             CREATE TABLE web_sessions (
                 token_hash TEXT PRIMARY KEY NOT NULL,
                 device_id TEXT NOT NULL,
                 created_at_ms INTEGER NOT NULL,
                 expires_at_ms INTEGER NOT NULL,
                 last_seen_ms INTEGER NOT NULL,
                 absolute_expires_at_ms INTEGER NOT NULL
             );"
        ))
        .expect("create workspace schema");
    for index in 0..spec.tasks {
        connection
            .execute(
                "INSERT INTO tasks(id, title) VALUES (?1, ?2)",
                params![format!("task-{index}"), format!("Task {index}")],
            )
            .expect("insert task");
    }
    for index in 0..spec.outbox_rows {
        connection
            .execute(
                "INSERT INTO sync_outbox(device_id, sequence, recorded_at_ms, entity_kind, entity_id, exported_at_ms) \
                 VALUES ('device-a', ?1, 0, 'task', ?2, NULL)",
                params![(index + 1) as i64, format!("pending-{index}")],
            )
            .expect("insert outbox row");
    }
    for index in 0..spec.exported_outbox_rows {
        connection
            .execute(
                "INSERT INTO sync_outbox(device_id, sequence, recorded_at_ms, entity_kind, entity_id, exported_at_ms) \
                 VALUES ('device-a', ?1, 0, 'task', ?2, 1)",
                params![(index + 1) as i64, format!("exported-{index}")],
            )
            .expect("insert exported outbox row");
    }
    for index in 0..spec.devices {
        connection
            .execute(
                "INSERT INTO devices(id, token_hash, revoked) VALUES (?1, X'00', 0)",
                params![format!("device-{index}")],
            )
            .expect("insert device");
    }
    for index in 0..spec.web_sessions {
        connection
            .execute(
                "INSERT INTO web_sessions(token_hash, device_id, created_at_ms, expires_at_ms, last_seen_ms, absolute_expires_at_ms) \
                 VALUES (?1, 'device-0', 0, 1, 0, 1)",
                params![format!("session-{index}")],
            )
            .expect("insert web session");
    }
    if spec.replica_markers {
        connection
            .execute_batch("CREATE TABLE authority_outbox (id INTEGER PRIMARY KEY, payload TEXT);")
            .expect("create replica marker table");
    }
    let mut attachment_hash = None;
    if let Some(content) = spec.attachment {
        let (_, hash) = hash_bytes(content);
        let relative = format!("{}/{hash}", &hash[..2]);
        let store = root.join("attachments").join("objects");
        fs::create_dir_all(store.join(&hash[..2])).expect("create attachment store");
        fs::write(store.join(&relative), content).expect("write attachment object");
        connection
            .execute(
                "INSERT INTO attachment_objects(object_hash, size_bytes) VALUES (?1, ?2)",
                params![hash, content.len() as i64],
            )
            .expect("insert attachment object row");
        connection
            .execute(
                "INSERT INTO attachments(id, object_hash) VALUES ('attachment-1', ?1)",
                params![hash],
            )
            .expect("insert attachment association");
        attachment_hash = Some(hash);
    }
    connection
        .execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
        .expect("checkpoint workspace");
    drop(connection);
    remove_sidecars(&database);

    let vault = root.join("orbit.knowledge");
    fs::create_dir_all(vault.join("Work")).expect("create vault Work");
    fs::create_dir_all(vault.join("Archive")).expect("create vault Archive");
    fs::write(vault.join("Work").join("note.md"), spec.note).expect("write note");
    Fixture {
        root: root.to_path_buf(),
        layout: WorkspaceLayout::for_database(&database).expect("workspace layout"),
        attachment_hash,
    }
}

fn remove_sidecars(database: &Path) {
    for suffix in ["-wal", "-shm"] {
        let mut text = database.as_os_str().to_owned();
        text.push(suffix);
        let _ = fs::remove_file(PathBuf::from(text));
    }
}

/// Create a real archive for a fixture through the production creation service.
pub fn create_archive(
    fixture: &Fixture,
    destination_dir: &Path,
    source_mode: BackupSourceMode,
    label: &str,
) -> BackupOutcome {
    let service = BackupService::new(fixture.layout.clone());
    let options = BackupOptions {
        source_mode,
        label: Some(label.to_owned()),
        include_config: true,
        config: Some(BackupConfigSnapshot {
            config_version: 1,
            authority_mode: match source_mode {
                BackupSourceMode::Replica => "connected".to_owned(),
                _ => "local".to_owned(),
            },
            default_agent_provider: None,
            connected_server: None,
        }),
        fence_timeout: Duration::from_secs(5),
        keep_staging: false,
    };
    let mut progress = NoProgress;
    service
        .create(destination_dir, options, &mut progress)
        .expect("create backup archive")
}

/// A backup progress sink that records nothing, usable where a crate-private
/// `NoProgress` type would need to be named.
pub fn silent_progress() -> impl BackupProgressSink {
    NoProgress
}

/// SHA-256 of a byte slice, matching the archive's lowercase hex digests.
pub fn hash_bytes(content: &[u8]) -> (u64, String) {
    let mut reader = content;
    util::hash_reader(&mut reader).expect("hash bytes")
}

/// SHA-256 of a file.
pub fn hash_file(path: &Path) -> String {
    util::hash_file(path).expect("hash file").1
}

/// Recursively hash every regular file under `root` into a stable relative map.
pub fn hash_tree(root: &Path) -> BTreeMap<String, String> {
    let mut entries = BTreeMap::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        let names = fs::read_dir(&directory)
            .unwrap_or_else(|error| panic!("read {}: {error}", directory.display()));
        for name in names {
            let path = name.expect("directory entry").path();
            if path.is_dir() {
                pending.push(path);
            } else {
                let relative = path
                    .strip_prefix(root)
                    .expect("relative path")
                    .to_string_lossy()
                    .replace('\\', "/");
                entries.insert(relative, hash_file(&path));
            }
        }
    }
    entries
}

/// Count rows in one table of a database file.
pub fn count_rows(database: &Path, table: &str) -> u64 {
    let connection = rusqlite::Connection::open(database).expect("open database for counting");
    let quoted = table.replace('"', "\"\"");
    let count: i64 = connection
        .query_row(&format!("SELECT COUNT(*) FROM \"{quoted}\""), [], |row| {
            row.get(0)
        })
        .expect("count rows");
    count.max(0) as u64
}

/// Read the ordered task titles from a restored database.
pub fn task_titles(database: &Path) -> Vec<String> {
    let connection = rusqlite::Connection::open(database).expect("open database");
    let mut statement = connection
        .prepare("SELECT title FROM tasks ORDER BY id")
        .expect("prepare task query");
    statement
        .query_map([], |row| row.get::<_, String>(0))
        .expect("query tasks")
        .collect::<Result<Vec<_>, _>>()
        .expect("collect tasks")
}

/// Read a Markdown note from a vault.
pub fn read_note(vault: &Path, relative: &str) -> String {
    fs::read_to_string(vault.join(relative))
        .unwrap_or_else(|error| panic!("read note {relative}: {error}"))
}

/// Hold the SQLite write fence on a workspace database for the duration of a test.
pub fn hold_write_fence(database: &Path) -> FencedConnection {
    FencedConnection::acquire(database, Duration::from_secs(1)).expect("acquire test fence")
}

/// Write an archive that contains only a manifest (used to prove preflight
/// blocks an incompatible schema without any successful extraction).
pub fn write_manifest_only_archive(path: &Path, manifest: &BackupManifest) {
    let manifest_json = manifest.to_json().expect("serialize manifest");
    let file = fs::File::create(path).expect("create archive");
    let mut writer = TarWriter::new(std::io::BufWriter::new(file));
    writer
        .append_bytes(MANIFEST_ENTRY, &manifest_json)
        .expect("append manifest");
    writer.finish().expect("finish archive");
}

/// Read a manifest without extracting the archive.
pub fn read_manifest(archive: &Path) -> BackupManifest {
    orbit_backup::read_manifest_only(archive).expect("read manifest")
}

/// Rewrite the archive's manifest from its own entries, without touching content.
///
/// Used to model a format-valid archive that declares a source mode the current
/// creation service refuses to produce (e.g. a replica snapshot), so the restore
/// engine's refusal can be tested independently of creation policy.
pub fn rewrite_manifest<F>(archive: &Path, mutate: F)
where
    F: FnOnce(&mut BackupManifest),
{
    use std::io::BufReader;

    let mut updated = read_manifest(archive);
    mutate(&mut updated);
    updated.validate().expect("mutated manifest must validate");
    let temporary = archive.with_extension("rewrite.tmp");
    {
        let input = fs::File::open(archive).expect("open archive for rewrite");
        let mut reader = TarReader::new(BufReader::new(input));
        let output = fs::File::create(&temporary).expect("create rewritten archive");
        let mut writer = TarWriter::new(std::io::BufWriter::new(output));
        let header = reader
            .next_header()
            .expect("read manifest header")
            .expect("manifest header present");
        let mut original = Vec::new();
        reader
            .copy_data(header.size, &mut original)
            .expect("read original manifest");
        writer
            .append_bytes(
                orbit_backup::manifest::MANIFEST_ENTRY,
                &updated.to_json().expect("serialize updated manifest"),
            )
            .expect("write updated manifest");
        while let Some(header) = reader.next_header().expect("read entry header") {
            let mut content = Vec::new();
            reader
                .copy_data(header.size, &mut content)
                .expect("read entry content");
            writer
                .append_bytes(&header.path, &content)
                .expect("write entry content");
        }
        writer.finish().expect("finish rewritten archive");
    }
    fs::rename(&temporary, archive).expect("publish rewritten archive");
}

/// Flip one byte inside the content of a declared archive entry, keeping the tar
/// structure readable so only the per-file hash check can notice.
pub fn corrupt_entry_content(archive: &Path, manifest: &BackupManifest, entry_path: &str) -> bool {
    let mut bytes = fs::read(archive).expect("read archive");
    let mut offset = tar_padding(manifest.to_json().expect("serialize manifest").len());
    for entry in &manifest.inventory {
        offset += 512; // entry header
        if entry.path == entry_path {
            if entry.size_bytes == 0 {
                return false;
            }
            let middle = offset + (entry.size_bytes as usize / 2);
            bytes[middle] ^= 0xff;
            fs::write(archive, &bytes).expect("write corrupted archive");
            return true;
        }
        offset += entry.size_bytes as usize + tar_padding(entry.size_bytes as usize);
    }
    false
}

/// Rewrite one declared archive entry's tar path (and its header checksum).
///
/// Used to prove that a traversal attempt is rejected by verification and by
/// restore before anything is extracted.
pub fn rewrite_entry_path(
    archive: &Path,
    manifest: &BackupManifest,
    entry_path: &str,
    new_path: &str,
) -> bool {
    let mut bytes = fs::read(archive).expect("read archive");
    let mut offset = tar_padding(manifest.to_json().expect("serialize manifest").len());
    for entry in &manifest.inventory {
        if entry.path == entry_path {
            let header = &mut bytes[offset..offset + 512];
            for byte in &mut header[..100] {
                *byte = 0;
            }
            header[..new_path.len()].copy_from_slice(new_path.as_bytes());
            for byte in &mut header[148..156] {
                *byte = b' ';
            }
            let checksum: u64 = header.iter().map(|byte| u64::from(*byte)).sum();
            let text = format!("{checksum:06o}\0 ");
            header[148..156].copy_from_slice(text.as_bytes());
            fs::write(archive, &bytes).expect("write traversal archive");
            return true;
        }
        offset += 512 + entry.size_bytes as usize + tar_padding(entry.size_bytes as usize);
    }
    false
}

fn tar_padding(length: usize) -> usize {
    if length.is_multiple_of(512) {
        0
    } else {
        512 - length % 512
    }
}
