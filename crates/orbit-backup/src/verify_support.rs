//! Deep verification coverage: schema compatibility, SQLite integrity,
//! attachment presence and vault consistency, plus the check bookkeeping that
//! turns individual failures into a [`VerificationReport`](crate::verify::VerificationReport).

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use rusqlite::{Connection, OpenFlags};

use crate::manifest::{ATTACHMENT_PREFIX, BackupManifest, InventoryEntry, KNOWLEDGE_PREFIX};
use crate::snapshot::inspect_snapshot;
use crate::util;
use crate::verify::{
    CheckStatus, ScanOutcome, VerificationCheck, VerificationProblem, VerificationReport,
};

// ---------------------------------------------------------------------------
// Coverage checks: schema, SQLite integrity, attachments, vault consistency.
// ---------------------------------------------------------------------------

pub(crate) fn coverage_checks(
    manifest: Option<&BackupManifest>,
    scan: &ScanOutcome,
    checks: &mut CheckSet,
    deep: bool,
) {
    let Some(manifest) = manifest else {
        checks.skip(
            "schema_compatibility",
            "no manifest to check the schema against",
        );
        checks.skip("vault_consistency", "no manifest describes the vault");
        if deep {
            checks.fail(
                "sqlite_integrity",
                "no database snapshot is available to inspect",
            );
            checks.fail(
                "attachment_presence",
                "no database snapshot is available to inspect",
            );
        } else {
            checks.skip(
                "sqlite_integrity",
                "no database snapshot is available to inspect",
            );
            checks.skip(
                "attachment_presence",
                "no database snapshot is available to inspect",
            );
        }
        return;
    };
    check_schema_compatibility(manifest, checks);
    check_vault_consistency(manifest, checks);
    if !deep {
        checks.skip(
            "sqlite_integrity",
            "deep verification disabled; only the declared schema was checked",
        );
        checks.skip("attachment_presence", "deep verification disabled");
        return;
    }
    let Some(database) = &scan.extracted_database else {
        checks.fail(
            "sqlite_integrity",
            "the database/orbit.db snapshot is missing from the archive",
        );
        checks.fail(
            "attachment_presence",
            "the database/orbit.db snapshot is missing",
        );
        return;
    };
    inspect_extracted_database(database, manifest, checks);
    check_attachment_presence(database, manifest, checks);
}

fn check_schema_compatibility(manifest: &BackupManifest, checks: &mut CheckSet) {
    let supported = crate::supported_schema_version();
    match manifest.check_schema_compatibility(supported) {
        Ok(()) => checks.pass(
            "schema_compatibility",
            format!(
                "schema {} is supported by this build (up to {supported})",
                manifest.schema_version
            ),
        ),
        Err(error) => checks.fail("schema_compatibility", error.to_string()),
    }
}

fn inspect_extracted_database(database: &Path, manifest: &BackupManifest, checks: &mut CheckSet) {
    let inspection = match inspect_snapshot(database) {
        Ok(inspection) => inspection,
        Err(error) => {
            checks.fail("sqlite_integrity", error.to_string());
            return;
        }
    };
    checks.checked(
        "sqlite_integrity",
        u64::try_from(inspection.table_count).unwrap_or(0),
    );
    if !inspection.orbit_schema_present {
        checks.fail(
            "sqlite_integrity",
            "the snapshot database has no orbit_schema table; it is not an Orbit workspace",
        );
    } else if !inspection.integrity_ok {
        checks.fail(
            "sqlite_integrity",
            "PRAGMA integrity_check did not return ok",
        );
    } else {
        checks.pass(
            "sqlite_integrity",
            format!(
                "PRAGMA integrity_check passed ({} tables)",
                inspection.table_count
            ),
        );
    }
    let supported = crate::supported_schema_version();
    match inspection.schema_version {
        Some(version) if version != manifest.schema_version => checks.fail(
            "schema_compatibility",
            format!(
                "the snapshot database schema {version} does not match the manifest schema {}",
                manifest.schema_version
            ),
        ),
        Some(version) if version > supported => checks.fail(
            "schema_compatibility",
            format!(
                "the snapshot database schema {version} is newer than this build supports (schema {supported}); install a newer Orbit"
            ),
        ),
        Some(version) => checks.pass(
            "schema_compatibility",
            format!("the snapshot database schema {version} is supported (up to {supported})"),
        ),
        None => checks.fail(
            "schema_compatibility",
            "the snapshot database does not record a schema version",
        ),
    }
}

fn check_attachment_presence(database: &Path, manifest: &BackupManifest, checks: &mut CheckSet) {
    let before = checks.failure_count("attachment_presence");
    let inventory: BTreeMap<&str, &InventoryEntry> = manifest
        .attachment_paths()
        .map(|entry| (entry.path.as_str(), entry))
        .collect();
    let connection = match Connection::open_with_flags(database, OpenFlags::SQLITE_OPEN_READ_ONLY) {
        Ok(connection) => connection,
        Err(error) => {
            checks.fail(
                "attachment_presence",
                format!("open the snapshot database: {error}"),
            );
            return;
        }
    };
    let table_exists: bool = match connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'attachment_objects')",
        [],
        |row| row.get(0),
    ) {
        Ok(exists) => exists,
        Err(error) => {
            checks.fail("attachment_presence", format!("inspect attachment_objects: {error}"));
            return;
        }
    };
    if !table_exists {
        if inventory.is_empty() {
            checks.pass(
                "attachment_presence",
                "no attachment objects are referenced",
            );
        } else {
            checks.fail(
                "attachment_presence",
                format!(
                    "the snapshot has no attachment_objects table but {} object(s) are archived",
                    inventory.len()
                ),
            );
        }
        return;
    }
    let mut objects = 0_u64;
    let query = "SELECT object_hash, size_bytes FROM attachment_objects ORDER BY object_hash";
    let mut statement = match connection.prepare(query) {
        Ok(statement) => statement,
        Err(error) => {
            checks.fail(
                "attachment_presence",
                format!("query attachment_objects: {error}"),
            );
            return;
        }
    };
    let rows = match statement.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, i64>(1)?.max(0) as u64,
        ))
    }) {
        Ok(rows) => rows,
        Err(error) => {
            checks.fail(
                "attachment_presence",
                format!("query attachment_objects: {error}"),
            );
            return;
        }
    };
    for row in rows {
        let (object_hash, size_bytes) = match row {
            Ok(row) => row,
            Err(error) => {
                checks.fail(
                    "attachment_presence",
                    format!("read attachment_objects: {error}"),
                );
                return;
            }
        };
        objects += 1;
        let Some(relative) = attachment_relative_path(&object_hash) else {
            checks.fail(
                "attachment_presence",
                format!("attachment object identifier is not a SHA-256 digest: {object_hash:?}"),
            );
            continue;
        };
        let path = format!("{ATTACHMENT_PREFIX}{relative}");
        match inventory.get(path.as_str()) {
            None => checks.fail(
                "attachment_presence",
                format!("referenced attachment object {object_hash} is missing from the archive"),
            ),
            Some(entry) if entry.size_bytes != size_bytes => checks.fail(
                "attachment_presence",
                format!(
                    "attachment object {object_hash} declares {size_bytes} bytes in the snapshot but {} bytes in the archive",
                    entry.size_bytes
                ),
            ),
            Some(_) => {}
        }
    }
    checks.checked("attachment_presence", objects);
    if checks.failure_count("attachment_presence") == before {
        checks.pass(
            "attachment_presence",
            format!("{objects} referenced attachment object(s) are present and size-matched"),
        );
    }
}

fn check_vault_consistency(manifest: &BackupManifest, checks: &mut CheckSet) {
    let before = checks.failure_count("vault_consistency");
    let notes: Vec<&InventoryEntry> = manifest.knowledge_paths().collect();
    let mut directories = BTreeSet::new();
    for directory in &manifest.directories {
        let normalized = directory.trim_end_matches('/');
        let acceptable = util::is_safe_relative_path(normalized)
            && normalized.starts_with(KNOWLEDGE_PREFIX)
            && normalized.len() > KNOWLEDGE_PREFIX.len();
        if !acceptable {
            checks.fail(
                "vault_consistency",
                format!("vault directory {directory} is not a safe knowledge path"),
            );
            continue;
        }
        if !directories.insert(normalized.to_owned()) {
            checks.fail(
                "vault_consistency",
                format!("vault directory {directory} is listed twice"),
            );
        }
    }
    for note in &notes {
        let mut parent = note
            .path
            .rsplit_once('/')
            .map(|(parent, _)| parent)
            .unwrap_or("");
        while !parent.is_empty() && parent != "knowledge" {
            if !directories.contains(parent) {
                checks.fail(
                    "vault_consistency",
                    format!(
                        "note {} is recorded under undeclared directory {parent}",
                        note.path
                    ),
                );
                break;
            }
            parent = parent.rsplit_once('/').map(|(rest, _)| rest).unwrap_or("");
        }
    }
    if manifest.counts.knowledge_notes != notes.len() as u64 {
        checks.fail(
            "vault_consistency",
            format!(
                "manifest counts {} note(s) but the inventory lists {}",
                manifest.counts.knowledge_notes,
                notes.len()
            ),
        );
    }
    if manifest.counts.knowledge_folders != manifest.directories.len() as u64 {
        checks.fail(
            "vault_consistency",
            format!(
                "manifest counts {} folder(s) but lists {} directories",
                manifest.counts.knowledge_folders,
                manifest.directories.len()
            ),
        );
    }
    let knowledge_bytes: u64 = notes.iter().map(|note| note.size_bytes).sum();
    if manifest.totals.knowledge_bytes != knowledge_bytes {
        checks.fail(
            "vault_consistency",
            format!(
                "manifest declares {} knowledge bytes but the inventory sums to {knowledge_bytes}",
                manifest.totals.knowledge_bytes
            ),
        );
    }
    checks.checked("vault_consistency", notes.len() as u64);
    if checks.failure_count("vault_consistency") == before {
        checks.pass(
            "vault_consistency",
            format!(
                "{} note(s) in {} declared folder(s) are consistent with the manifest",
                notes.len(),
                directories.len()
            ),
        );
    }
}

fn attachment_relative_path(object_hash: &str) -> Option<String> {
    let valid = object_hash.len() == 64 && object_hash.bytes().all(|byte| byte.is_ascii_hexdigit());
    valid.then(|| format!("{}/{}", &object_hash[..2], object_hash))
}

// ---------------------------------------------------------------------------
// Check bookkeeping.
// ---------------------------------------------------------------------------

const CHECK_NAMES: [&str; 8] = [
    "structure",
    "manifest",
    "paths",
    "file_hashes",
    "sqlite_integrity",
    "schema_compatibility",
    "attachment_presence",
    "vault_consistency",
];

struct CheckState {
    name: &'static str,
    status: CheckStatus,
    detail: String,
    checked: Option<u64>,
    failures: Vec<String>,
}

pub(crate) struct CheckSet {
    states: Vec<CheckState>,
    problems: Vec<VerificationProblem>,
}

impl CheckSet {
    pub(crate) fn new() -> Self {
        Self {
            states: CHECK_NAMES
                .iter()
                .map(|name| CheckState {
                    name,
                    status: CheckStatus::Skipped,
                    detail: String::new(),
                    checked: None,
                    failures: Vec::new(),
                })
                .collect(),
            problems: Vec::new(),
        }
    }

    fn state(&mut self, name: &'static str) -> &mut CheckState {
        self.states
            .iter_mut()
            .find(|state| state.name == name)
            .expect("verification check names are fixed")
    }

    pub(crate) fn pass(&mut self, name: &'static str, detail: impl Into<String>) {
        let state = self.state(name);
        if state.status != CheckStatus::Fail {
            state.status = CheckStatus::Pass;
            state.detail = detail.into();
        }
    }

    pub(crate) fn skip(&mut self, name: &'static str, detail: impl Into<String>) {
        let state = self.state(name);
        if state.status != CheckStatus::Fail {
            state.status = CheckStatus::Skipped;
            state.detail = detail.into();
        }
    }

    pub(crate) fn checked(&mut self, name: &'static str, checked: u64) {
        self.state(name).checked = Some(checked);
    }

    pub(crate) fn fail(&mut self, name: &'static str, detail: impl Into<String>) {
        let detail = detail.into();
        {
            let state = self.state(name);
            state.status = CheckStatus::Fail;
            if state.detail.is_empty() {
                state.detail = detail.clone();
            }
            state.failures.push(detail.clone());
        }
        self.problems.push(VerificationProblem {
            check: name.to_owned(),
            detail,
        });
    }

    pub(crate) fn failure_count(&self, name: &'static str) -> usize {
        self.states
            .iter()
            .find(|state| state.name == name)
            .map(|state| state.failures.len())
            .unwrap_or(0)
    }

    pub(crate) fn finish(self, report: &mut VerificationReport) {
        for state in self.states {
            let status = if state.failures.is_empty() {
                state.status
            } else {
                CheckStatus::Fail
            };
            let detail = if state.failures.is_empty() {
                if state.detail.is_empty() {
                    "not applicable".to_owned()
                } else {
                    state.detail
                }
            } else {
                let mut detail = state.failures[0].clone();
                if state.failures.len() > 1 {
                    detail.push_str(&format!(" (and {} more)", state.failures.len() - 1));
                }
                detail
            };
            report.checks.push(VerificationCheck {
                name: state.name.to_owned(),
                status,
                detail,
                checked: state.checked,
            });
        }
        report.problems = self.problems;
        report.ok =
            report.problems.is_empty() && report.checks.iter().all(|check| check.status.is_ok());
    }
}
