//! Workspace layout: which directories make up one authoritative Orbit workspace.
//!
//! The layout convention is shared by the runtime, the CLI and the server:
//!
//! - database: the authoritative SQLite file (or a client replica);
//! - knowledge vault: `<database>.knowledge/` beside it;
//! - attachment object store: `<database-parent>/attachments/objects/`.
//!
//! One function defines the convention so a backup can never capture a
//! different set of files than the running runtime would use. Paths are never
//! stored inside a portable archive.

use std::path::{Path, PathBuf};

use crate::error::{BackupError, Result};

/// Directory suffix of the Markdown knowledge vault.
pub const KNOWLEDGE_VAULT_SUFFIX: &str = "knowledge";
/// Directory name of the managed attachment store beside the database.
pub const ATTACHMENT_STORE_DIR: &str = "attachments";
/// Leaf directory of content-addressed attachment objects.
pub const ATTACHMENT_OBJECTS_DIR: &str = "objects";

/// The three filesystem resources that make up an authoritative workspace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceLayout {
    database: PathBuf,
    knowledge_vault: PathBuf,
    attachment_store: PathBuf,
}

impl WorkspaceLayout {
    /// Derive the layout for an on-disk authoritative database.
    ///
    /// This mirrors `AuthoritativeHost::open`: the vault is
    /// `<database>.knowledge` and the attachment objects live in
    /// `<database-parent>/attachments/objects`.
    pub fn for_database(database: impl Into<PathBuf>) -> Result<Self> {
        let database = database.into();
        if database.as_os_str().is_empty() {
            return Err(BackupError::InvalidArgument(
                "database path is empty".to_owned(),
            ));
        }
        if database == Path::new(":memory:") {
            return Err(BackupError::Unavailable(
                "an in-memory workspace has no durable files to back up".to_owned(),
            ));
        }
        let knowledge_vault = database.with_extension(KNOWLEDGE_VAULT_SUFFIX);
        let parent = database.parent().unwrap_or_else(|| Path::new("."));
        let attachment_store = parent
            .join(ATTACHMENT_STORE_DIR)
            .join(ATTACHMENT_OBJECTS_DIR);
        Ok(Self {
            database,
            knowledge_vault,
            attachment_store,
        })
    }

    /// Authoritative SQLite database path.
    pub fn database(&self) -> &Path {
        &self.database
    }

    /// Markdown knowledge vault directory.
    pub fn knowledge_vault(&self) -> &Path {
        &self.knowledge_vault
    }

    /// Content-addressed attachment object directory.
    pub fn attachment_store(&self) -> &Path {
        &self.attachment_store
    }

    /// The directory that contains the database and its sibling resources.
    pub fn parent(&self) -> &Path {
        self.database.parent().unwrap_or_else(|| Path::new("."))
    }

    /// Whether the authoritative database file exists.
    pub fn database_exists(&self) -> bool {
        self.database.is_file()
    }

    /// Whether the vault directory exists.
    pub fn knowledge_vault_exists(&self) -> bool {
        self.knowledge_vault.is_dir()
    }

    /// Whether the attachment object store exists.
    pub fn attachment_store_exists(&self) -> bool {
        self.attachment_store.is_dir()
    }

    /// Require an existing database file, failing with an actionable message.
    pub fn require_database(&self) -> Result<()> {
        if self.database_exists() {
            Ok(())
        } else {
            Err(BackupError::NotFound(format!(
                "no Orbit workspace database at {}",
                self.database.display()
            )))
        }
    }

    /// A description safe to show in CLI output and web responses.
    pub fn describe(&self) -> WorkspaceDescription {
        WorkspaceDescription {
            database: self.database.display().to_string(),
            knowledge_vault: self.knowledge_vault.display().to_string(),
            attachment_store: self.attachment_store.display().to_string(),
            knowledge_vault_present: self.knowledge_vault_exists(),
            attachment_store_present: self.attachment_store_exists(),
        }
    }
}

/// Human-readable workspace facts for CLI and API responses.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct WorkspaceDescription {
    pub database: String,
    pub knowledge_vault: String,
    pub attachment_store: String,
    pub knowledge_vault_present: bool,
    pub attachment_store_present: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_matches_the_runtime_convention() {
        let layout = WorkspaceLayout::for_database("/data/orbit/orbit-local.db").unwrap();
        assert_eq!(layout.database(), Path::new("/data/orbit/orbit-local.db"));
        assert_eq!(
            layout.knowledge_vault(),
            Path::new("/data/orbit/orbit-local.knowledge")
        );
        assert_eq!(
            layout.attachment_store(),
            Path::new("/data/orbit/attachments/objects")
        );
    }

    #[test]
    fn layout_handles_relative_database_paths() {
        let layout = WorkspaceLayout::for_database("orbit.db").unwrap();
        assert_eq!(layout.knowledge_vault(), Path::new("orbit.knowledge"));
        assert_eq!(layout.attachment_store(), Path::new("attachments/objects"));
    }

    #[test]
    fn in_memory_workspaces_are_refused() {
        let error = WorkspaceLayout::for_database(":memory:").unwrap_err();
        assert!(error.to_string().contains("in-memory"));
    }

    #[test]
    fn missing_database_is_reported_as_not_found() {
        let layout = WorkspaceLayout::for_database("/nonexistent/orbit.db").unwrap();
        assert!(!layout.database_exists());
        assert!(layout.require_database().is_err());
    }
}
