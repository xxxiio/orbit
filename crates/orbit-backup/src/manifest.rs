//! `.orbitbak` manifest: the portable, versioned description of a backup.
//!
//! The manifest is the first entry of every archive. It carries everything a
//! restore needs to decide whether the archive is complete, compatible and
//! safe: format version, schema version, source mode, snapshot revision, an
//! inventory with sizes and SHA-256 hashes, totals, row counts and the
//! exclusions that keep the format honest about what it does not contain.
//!
//! It never contains absolute source paths, credentials or environment values.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::error::{BackupError, Result};
use crate::util::is_safe_relative_path;

/// Current `.orbitbak` format version written by this build.
pub const BACKUP_FORMAT_VERSION: u32 = 1;
/// Oldest format version this build can read.
pub const BACKUP_MIN_READER_VERSION: u32 = 1;
/// Portable archive file extension (without the leading dot).
pub const BACKUP_EXTENSION: &str = "orbitbak";
/// Manifest archive path, always the first entry.
pub const MANIFEST_ENTRY: &str = "manifest.json";
/// Snapshot database archive path.
pub const DATABASE_ENTRY: &str = "database/orbit.db";
/// Knowledge vault archive path prefix.
pub const KNOWLEDGE_PREFIX: &str = "knowledge/";
/// Attachment object archive path prefix.
pub const ATTACHMENT_PREFIX: &str = "attachments/objects/";
/// Normalized non-secret configuration archive path.
pub const CONFIG_ENTRY: &str = "config/config.json";
/// File written beside an archive after it is verified (not part of the
/// portable archive itself).
pub const VERIFICATION_SIDECAR_SUFFIX: &str = "verify.json";

/// Which authority produced the snapshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BackupSourceMode {
    /// A local-only workspace database was authoritative.
    Local,
    /// A self-hosted server database was authoritative.
    Server,
    /// A connected client replica was snapshotted (never a full authority
    /// backup; recorded so a restore can refuse it as an authority source).
    Replica,
}

impl BackupSourceMode {
    /// Stable lowercase name used in output and the manifest.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::Server => "server",
            Self::Replica => "replica",
        }
    }
}

/// Where and from which revision the snapshot was taken.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackupSource {
    /// Authority mode of the snapshotted database.
    pub mode: BackupSourceMode,
    /// Stable workspace identifier (`authority_identity.generation`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<String>,
    /// Authority revision at the snapshot instant, when recorded by the store.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision: Option<i64>,
    /// Human description of the source, never an absolute path.
    pub described_as: String,
    /// Whether client-local pending data was explicitly excluded.
    pub excluded_client_pending_data: bool,
}

/// Kind of component stored in the archive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InventoryKind {
    Database,
    KnowledgeNote,
    AttachmentObject,
    Configuration,
}

/// One file recorded in the archive inventory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InventoryEntry {
    /// Relative slash-separated archive path.
    pub path: String,
    /// Component category.
    pub kind: InventoryKind,
    /// Content size in bytes.
    pub size_bytes: u64,
    /// Lowercase hexadecimal SHA-256 of the content.
    pub sha256: String,
}

/// Normalized, non-secret configuration captured with the backup.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackupConfigSnapshot {
    /// Configuration schema version.
    pub config_version: u32,
    /// `local` or `connected`.
    pub authority_mode: String,
    /// Configured default agent provider, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_agent_provider: Option<String>,
    /// Connected server URL, if this workspace is a replica. No credentials.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connected_server: Option<String>,
}

/// An explicit statement that a component is outside the backup boundary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackupExclusion {
    /// Stable category, e.g. `client_pending_data`.
    pub category: String,
    /// Operator-readable explanation.
    pub detail: String,
}

/// Byte totals for the recorded components.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackupTotals {
    pub database_bytes: u64,
    pub knowledge_bytes: u64,
    pub attachment_bytes: u64,
    pub config_bytes: u64,
    /// Sum of all inventoried content bytes.
    pub content_bytes: u64,
    /// Exact size of the published archive file in bytes.
    pub archive_bytes: u64,
}

/// Recorded entity counts, so an operator can compare before and after restore.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackupCounts {
    pub knowledge_notes: u64,
    pub knowledge_folders: u64,
    pub attachment_objects: u64,
    pub attachment_associations: u64,
    /// Row count per SQLite table present in the snapshot.
    pub tables: BTreeMap<String, u64>,
}

/// The complete portable manifest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackupManifest {
    pub format_version: u32,
    pub min_reader_version: u32,
    /// Orbit package version that produced the backup.
    pub orbit_version: String,
    /// Optional build identifier supplied at build time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub orbit_build: Option<String>,
    /// SQLite schema version of the snapshot.
    pub schema_version: i64,
    /// Creation time, Unix epoch milliseconds.
    pub created_at_ms: i64,
    /// Creation time, RFC 3339 UTC.
    pub created_at: String,
    /// Optional user label.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// Whether the archive contains every component it claims to contain.
    pub complete: bool,
    pub source: BackupSource,
    /// Non-secret normalized configuration, when included.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config: Option<BackupConfigSnapshot>,
    /// Components deliberately not archived.
    #[serde(default)]
    pub exclusions: Vec<BackupExclusion>,
    /// Each archived regular file except the manifest itself.
    #[serde(default)]
    pub inventory: Vec<InventoryEntry>,
    /// Relative vault directories (including empty folders).
    #[serde(default)]
    pub directories: Vec<String>,
    #[serde(default)]
    pub totals: BackupTotals,
    #[serde(default)]
    pub counts: BackupCounts,
}

impl BackupManifest {
    /// Serialize the manifest deterministically as pretty JSON.
    pub fn to_json(&self) -> Result<Vec<u8>> {
        let mut text = serde_json::to_string_pretty(self)
            .map_err(|error| BackupError::Manifest(format!("serialize manifest: {error}")))?;
        text.push('\n');
        Ok(text.into_bytes())
    }

    /// Parse a manifest from archive bytes.
    pub fn from_json(bytes: &[u8]) -> Result<Self> {
        serde_json::from_slice(bytes)
            .map_err(|error| BackupError::Manifest(format!("parse manifest JSON: {error}")))
    }

    /// Reject a manifest produced by an incompatible format version.
    pub fn check_format_compatibility(&self) -> Result<()> {
        if self.format_version > BACKUP_FORMAT_VERSION {
            return Err(BackupError::UnsupportedArchiveFormat {
                found: self.format_version,
                supported: BACKUP_FORMAT_VERSION,
                min_reader: BACKUP_MIN_READER_VERSION,
            });
        }
        if self.format_version < BACKUP_MIN_READER_VERSION {
            return Err(BackupError::UnsupportedArchiveFormat {
                found: self.format_version,
                supported: BACKUP_FORMAT_VERSION,
                min_reader: BACKUP_MIN_READER_VERSION,
            });
        }
        if self.min_reader_version > BACKUP_FORMAT_VERSION {
            return Err(BackupError::UnsupportedArchiveFormat {
                found: self.min_reader_version,
                supported: BACKUP_FORMAT_VERSION,
                min_reader: BACKUP_MIN_READER_VERSION,
            });
        }
        Ok(())
    }

    /// Reject a snapshot whose schema this build cannot safely open.
    pub fn check_schema_compatibility(&self, supported_schema: i64) -> Result<()> {
        if self.schema_version > supported_schema {
            return Err(BackupError::IncompatibleSchema {
                backup: self.schema_version,
                supported: supported_schema,
            });
        }
        Ok(())
    }

    /// Validate internal consistency and path safety.
    pub fn validate(&self) -> Result<()> {
        self.check_format_compatibility()?;
        if self.schema_version <= 0 {
            return Err(BackupError::Manifest(format!(
                "invalid schema version {}",
                self.schema_version
            )));
        }
        if self.created_at_ms < 0 {
            return Err(BackupError::Manifest("negative creation time".to_owned()));
        }
        let mut seen = std::collections::BTreeSet::new();
        let mut content_bytes = 0_u64;
        let mut database_entries = 0_usize;
        for entry in &self.inventory {
            if !is_safe_relative_path(&entry.path) {
                return Err(BackupError::UnsafePath(entry.path.clone()));
            }
            if entry.path == MANIFEST_ENTRY {
                return Err(BackupError::Manifest(
                    "manifest must not list itself in the inventory".to_owned(),
                ));
            }
            if !seen.insert(entry.path.as_str()) {
                return Err(BackupError::Manifest(format!(
                    "duplicate inventory path {}",
                    entry.path
                )));
            }
            if entry.sha256.len() != 64
                || !entry.sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
            {
                return Err(BackupError::Manifest(format!(
                    "invalid SHA-256 for {}",
                    entry.path
                )));
            }
            match entry.kind {
                InventoryKind::Database => {
                    database_entries += 1;
                    if entry.path != DATABASE_ENTRY {
                        return Err(BackupError::Manifest(format!(
                            "database entry must be {DATABASE_ENTRY}, found {}",
                            entry.path
                        )));
                    }
                }
                InventoryKind::KnowledgeNote => {
                    if !entry.path.starts_with(KNOWLEDGE_PREFIX) {
                        return Err(BackupError::Manifest(format!(
                            "knowledge entry outside {KNOWLEDGE_PREFIX}: {}",
                            entry.path
                        )));
                    }
                    if !entry.path.ends_with(".md") {
                        return Err(BackupError::Manifest(format!(
                            "knowledge entry is not Markdown: {}",
                            entry.path
                        )));
                    }
                }
                InventoryKind::AttachmentObject => {
                    if !entry.path.starts_with(ATTACHMENT_PREFIX) {
                        return Err(BackupError::Manifest(format!(
                            "attachment entry outside {ATTACHMENT_PREFIX}: {}",
                            entry.path
                        )));
                    }
                }
                InventoryKind::Configuration => {
                    if entry.path != CONFIG_ENTRY {
                        return Err(BackupError::Manifest(format!(
                            "configuration entry must be {CONFIG_ENTRY}, found {}",
                            entry.path
                        )));
                    }
                }
            }
            content_bytes = content_bytes.saturating_add(entry.size_bytes);
        }
        if database_entries != 1 {
            return Err(BackupError::Manifest(format!(
                "expected exactly one database snapshot, found {database_entries}"
            )));
        }
        for directory in &self.directories {
            let normalized = if directory.ends_with('/') {
                directory.trim_end_matches('/').to_owned()
            } else {
                directory.clone()
            };
            if !is_safe_relative_path(&normalized) || !normalized.starts_with(KNOWLEDGE_PREFIX) {
                return Err(BackupError::UnsafePath(directory.clone()));
            }
        }
        if !self.complete && self.exclusions.is_empty() {
            return Err(BackupError::Manifest(
                "an incomplete backup must list its exclusions".to_owned(),
            ));
        }
        if self.totals.content_bytes != content_bytes {
            return Err(BackupError::Manifest(format!(
                "totals.content_bytes {} does not match inventory sum {content_bytes}",
                self.totals.content_bytes
            )));
        }
        Ok(())
    }

    /// The database inventory entry, which must exist in a validated manifest.
    pub fn database_entry(&self) -> Option<&InventoryEntry> {
        self.inventory
            .iter()
            .find(|entry| entry.kind == InventoryKind::Database)
    }

    /// Attachment object paths recorded in the archive.
    pub fn attachment_paths(&self) -> impl Iterator<Item = &InventoryEntry> {
        self.inventory
            .iter()
            .filter(|entry| entry.kind == InventoryKind::AttachmentObject)
    }

    /// Knowledge note paths recorded in the archive.
    pub fn knowledge_paths(&self) -> impl Iterator<Item = &InventoryEntry> {
        self.inventory
            .iter()
            .filter(|entry| entry.kind == InventoryKind::KnowledgeNote)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_manifest() -> BackupManifest {
        let database = InventoryEntry {
            path: DATABASE_ENTRY.to_owned(),
            kind: InventoryKind::Database,
            size_bytes: 10,
            sha256: "a".repeat(64),
        };
        let note = InventoryEntry {
            path: "knowledge/Work/one.md".to_owned(),
            kind: InventoryKind::KnowledgeNote,
            size_bytes: 5,
            sha256: "b".repeat(64),
        };
        BackupManifest {
            format_version: BACKUP_FORMAT_VERSION,
            min_reader_version: BACKUP_MIN_READER_VERSION,
            orbit_version: "0.1.0".to_owned(),
            orbit_build: None,
            schema_version: 20,
            created_at_ms: 1_700_000_000_000,
            created_at: "2023-11-14T22:13:20.000Z".to_owned(),
            label: Some("Before agent testing".to_owned()),
            complete: true,
            source: BackupSource {
                mode: BackupSourceMode::Local,
                workspace_id: Some("generation-1".to_owned()),
                revision: Some(42),
                described_as: "local workspace".to_owned(),
                excluded_client_pending_data: false,
            },
            config: None,
            exclusions: Vec::new(),
            inventory: vec![database, note],
            directories: vec!["knowledge/Archive".to_owned()],
            totals: BackupTotals {
                database_bytes: 10,
                knowledge_bytes: 5,
                attachment_bytes: 0,
                config_bytes: 0,
                content_bytes: 15,
                archive_bytes: 0,
            },
            counts: BackupCounts::default(),
        }
    }

    #[test]
    fn manifest_round_trips_through_json() {
        let manifest = sample_manifest();
        manifest.validate().unwrap();
        let json = manifest.to_json().unwrap();
        let parsed = BackupManifest::from_json(&json).unwrap();
        assert_eq!(parsed, manifest);
    }

    #[test]
    fn newer_format_is_rejected_with_an_actionable_error() {
        let mut manifest = sample_manifest();
        manifest.format_version = BACKUP_FORMAT_VERSION + 1;
        manifest.min_reader_version = BACKUP_FORMAT_VERSION + 1;
        let error = manifest.validate().unwrap_err();
        assert!(error.to_string().contains("upgrade Orbit"));
    }

    #[test]
    fn newer_schema_is_rejected() {
        let manifest = sample_manifest();
        manifest.check_schema_compatibility(20).unwrap();
        let error = manifest.check_schema_compatibility(19).unwrap_err();
        assert!(matches!(error, BackupError::IncompatibleSchema { .. }));
    }

    #[test]
    fn traversal_inventory_path_is_rejected() {
        let mut manifest = sample_manifest();
        manifest.inventory[1].path = "knowledge/../../escape.md".to_owned();
        let error = manifest.validate().unwrap_err();
        assert!(matches!(error, BackupError::UnsafePath(_)));
    }

    #[test]
    fn duplicate_inventory_paths_are_rejected() {
        let mut manifest = sample_manifest();
        manifest.inventory.push(manifest.inventory[0].clone());
        manifest.totals.content_bytes += 10;
        assert!(manifest.validate().is_err());
    }

    #[test]
    fn inventory_totals_must_agree() {
        let mut manifest = sample_manifest();
        manifest.totals.content_bytes = 999;
        assert!(manifest.validate().is_err());
    }

    #[test]
    fn incomplete_backup_must_declare_exclusions() {
        let mut manifest = sample_manifest();
        manifest.complete = false;
        assert!(manifest.validate().is_err());
        manifest.exclusions.push(BackupExclusion {
            category: "client_pending_data".to_owned(),
            detail: "two disconnected clients hold unaccepted commands".to_owned(),
        });
        manifest.validate().unwrap();
    }

    #[test]
    fn manifest_never_requires_absolute_paths() {
        let manifest = sample_manifest();
        let json = String::from_utf8(manifest.to_json().unwrap()).unwrap();
        assert!(!json.contains("/home/"), "no source absolute path may leak");
    }
}
