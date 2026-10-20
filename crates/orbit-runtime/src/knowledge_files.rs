//! Filesystem-backed Markdown vault primitives.
//!
//! The directory tree (including empty folders) is the navigation hierarchy.
//! This module deliberately does not project content into the legacy SQL body
//! column. Switching existing write paths requires a recoverable file/SQL
//! transaction protocol and is tracked separately.
use serde::{Deserialize, Serialize};
use std::{
    fs, io,
    io::Write,
    path::{Component, Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT_STAGE: AtomicU64 = AtomicU64::new(0);

#[derive(Debug)]
pub enum VaultError {
    Io(io::Error),
    InvalidPath,
    AlreadyExists,
    NotFound,
    Conflict,
}
impl From<io::Error> for VaultError {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum KnowledgeNode {
    Folder {
        name: String,
        path: String,
        children: Vec<KnowledgeNode>,
    },
    Note {
        name: String,
        path: String,
    },
}

/// Canonical vault location belongs to the hosting process, never to the UI.
/// No directory traversal or symlink traversal is permitted.
pub struct MarkdownVault {
    root: PathBuf,
}
impl MarkdownVault {
    pub fn open(root: impl AsRef<Path>) -> Result<Self, VaultError> {
        fs::create_dir_all(root.as_ref())?;
        let root = fs::canonicalize(root)?;
        if !root.is_dir() {
            return Err(VaultError::InvalidPath);
        }
        Ok(Self { root })
    }

    pub(crate) fn root_path(&self) -> &Path {
        &self.root
    }

    pub(crate) fn validate_note_path(&self, relative: &str) -> Result<(), VaultError> {
        let path = Self::validate(relative, false)?;
        if !path
            .extension()
            .is_some_and(|x| x.eq_ignore_ascii_case("md"))
        {
            return Err(VaultError::InvalidPath);
        }
        Ok(())
    }

    fn validate(relative: &str, allow_root: bool) -> Result<PathBuf, VaultError> {
        if relative.is_empty() && allow_root {
            return Ok(PathBuf::new());
        }
        if relative.is_empty() || relative.contains('\\') || relative.contains('\0') {
            return Err(VaultError::InvalidPath);
        }
        let p = Path::new(relative);
        if p.components().any(|c| !matches!(c, Component::Normal(_))) {
            return Err(VaultError::InvalidPath);
        }
        if p.to_string_lossy() != relative
            || relative
                .split('/')
                .any(|s| s.is_empty() || s == "." || s == "..")
        {
            return Err(VaultError::InvalidPath);
        }
        Ok(p.to_owned())
    }

    fn checked_existing(&self, relative: &str, allow_root: bool) -> Result<PathBuf, VaultError> {
        let p = Self::validate(relative, allow_root)?;
        let mut current = self.root.clone();
        for part in p.components() {
            current.push(part);
            let m = fs::symlink_metadata(&current).map_err(|e| {
                if e.kind() == io::ErrorKind::NotFound {
                    VaultError::NotFound
                } else {
                    VaultError::Io(e)
                }
            })?;
            if m.file_type().is_symlink() {
                return Err(VaultError::InvalidPath);
            }
        }
        Ok(current)
    }

    /// Resolve a folder without following symlinks. Used by command replay to
    /// distinguish a completed rename from a foreign replacement of its path.
    pub(crate) fn folder_exists(&self, relative: &str) -> Result<bool, VaultError> {
        match self.checked_existing(relative, false) {
            Ok(path) if path.is_dir() => Ok(true),
            Ok(_) => Err(VaultError::InvalidPath),
            Err(VaultError::NotFound) => Ok(false),
            Err(e) => Err(e),
        }
    }

    /// Empty folders are first-class navigation nodes, even without notes.
    pub fn tree(&self) -> Result<KnowledgeNode, VaultError> {
        self.scan(&self.root, String::new(), String::from("Knowledge"))
    }
    fn scan(&self, dir: &Path, path: String, name: String) -> Result<KnowledgeNode, VaultError> {
        let mut children = Vec::new();
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let kind = entry.file_type()?;
            if kind.is_symlink() {
                continue;
            }
            let filename = entry.file_name().to_string_lossy().into_owned();
            if filename.starts_with('.') {
                continue;
            }
            let child_path = if path.is_empty() {
                filename.clone()
            } else {
                format!("{path}/{filename}")
            };
            if kind.is_dir() {
                children.push(self.scan(&entry.path(), child_path, filename)?);
            } else if kind.is_file()
                && entry
                    .path()
                    .extension()
                    .is_some_and(|x| x.eq_ignore_ascii_case("md"))
            {
                children.push(KnowledgeNode::Note {
                    name: filename,
                    path: child_path,
                });
            }
        }
        children.sort_by(|a, b| {
            let key = |n: &KnowledgeNode| match n {
                KnowledgeNode::Folder { name, .. } => (0, name.to_lowercase()),
                KnowledgeNode::Note { name, .. } => (1, name.to_lowercase()),
            };
            key(a).cmp(&key(b))
        });
        Ok(KnowledgeNode::Folder {
            name,
            path,
            children,
        })
    }

    /// Create an empty directory to be shown immediately in the navigation tree.
    pub fn create_folder(&self, relative: &str) -> Result<(), VaultError> {
        let path = Self::validate(relative, false)?;
        let parent = path.parent().unwrap_or(Path::new(""));
        let parent_path = self.checked_existing(&parent.to_string_lossy(), true)?;
        if !parent_path.is_dir() {
            return Err(VaultError::InvalidPath);
        }
        let destination = self.root.join(path);
        fs::create_dir(&destination).map_err(|e| {
            if e.kind() == io::ErrorKind::AlreadyExists {
                VaultError::AlreadyExists
            } else {
                VaultError::Io(e)
            }
        })?;
        // A successful mkdir is not durable until its parent directory entry
        // is synchronized. Without this, SQLite could accept a folder receipt
        // that disappears after sudden power loss on Unix filesystems.
        #[cfg(unix)]
        fs::File::open(&parent_path)?.sync_all()?;
        Ok(())
    }

    /// Stage and synchronize Markdown without writing its contents into SQL.
    /// This is a filesystem primitive, NOT the authoritative domain-command
    /// commit path: cross-resource receipts and recovery are still needed.
    pub fn create_markdown(&self, relative: &str, content: &str) -> Result<(), VaultError> {
        let path = Self::validate(relative, false)?;
        if !path
            .extension()
            .is_some_and(|x| x.eq_ignore_ascii_case("md"))
        {
            return Err(VaultError::InvalidPath);
        }
        let parent = path.parent().unwrap_or(Path::new(""));
        let parent_path = self.checked_existing(&parent.to_string_lossy(), true)?;
        if !parent_path.is_dir() {
            return Err(VaultError::InvalidPath);
        }
        let dest = self.root.join(path);
        // No replacing another note, even if it arrived between checks.
        // A create_new + fully synchronized private staging file prevents
        // exposing partially written Markdown in the knowledge explorer.
        let mut stage_path = None;
        for _ in 0..16 {
            let n = NEXT_STAGE.fetch_add(1, Ordering::Relaxed);
            let p = parent_path.join(format!(".orbit-stage-{}-{n}", std::process::id()));
            match fs::OpenOptions::new().write(true).create_new(true).open(&p) {
                Ok(mut f) => {
                    stage_path = Some(p.clone());
                    let result = (|| -> io::Result<()> {
                        f.write_all(content.as_bytes())?;
                        f.sync_all()?;
                        Ok(())
                    })();
                    if let Err(e) = result {
                        let _ = fs::remove_file(&p);
                        return Err(e.into());
                    }
                    break;
                }
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(e.into()),
            }
        }
        let stage = stage_path.ok_or(VaultError::AlreadyExists)?;
        // A hard link acts as an atomic no-clobber publication. This avoids
        // the overwrite behavior of rename on Unix and Windows.
        let result = fs::hard_link(&stage, &dest);
        let _ = fs::remove_file(&stage);
        match result {
            Ok(()) => {
                fs::File::open(&parent_path)?.sync_all()?;
                Ok(())
            }
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => Err(VaultError::AlreadyExists),
            Err(e) => Err(e.into()),
        }
    }

    /// Atomically replace a document only when its on-disk bytes match the
    /// caller's last observed version. This is a filesystem operation, not a
    /// cross-SQLite authoritative commit. Callers must serialize writes per
    /// vault; external editors can still race the final rename.
    pub fn replace_markdown(
        &self,
        relative: &str,
        expected: &str,
        content: &str,
    ) -> Result<(), VaultError> {
        let path = self.checked_existing(relative, false)?;
        if !path.is_file()
            || !path
                .extension()
                .is_some_and(|x| x.eq_ignore_ascii_case("md"))
        {
            return Err(VaultError::InvalidPath);
        }
        if fs::read(&path)? != expected.as_bytes() {
            return Err(VaultError::Conflict);
        }
        if expected == content {
            return Ok(());
        }
        let parent = path.parent().ok_or(VaultError::InvalidPath)?;
        let mut stage = None;
        for _ in 0..16 {
            let n = NEXT_STAGE.fetch_add(1, Ordering::Relaxed);
            let candidate = parent.join(format!(".orbit-stage-{}-{n}", std::process::id()));
            match fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&candidate)
            {
                Ok(mut file) => {
                    if let Err(e) = file
                        .write_all(content.as_bytes())
                        .and_then(|_| file.sync_all())
                    {
                        let _ = fs::remove_file(&candidate);
                        return Err(VaultError::Io(e));
                    }
                    stage = Some(candidate);
                    break;
                }
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(VaultError::Io(e)),
            }
        }
        let stage = stage.ok_or(VaultError::AlreadyExists)?;
        // Recheck before publication to catch normal intervening edits.
        let result = (|| -> Result<(), VaultError> {
            if fs::read(&path)? != expected.as_bytes() {
                return Err(VaultError::Conflict);
            }
            fs::rename(&stage, &path)?;
            fs::File::open(parent)?.sync_all()?;
            Ok(())
        })();
        let _ = fs::remove_file(stage);
        result
    }

    /// Move a Markdown note without changing its bytes. The caller owns stable
    /// note identity and must update the canonical path index separately.
    /// A hard link and unlink enforce no-clobber publication on the same vault
    /// filesystem; an interrupted unlink may leave two names and must be
    /// reconciled by the future command journal before acknowledging a move.
    pub fn move_markdown(&self, from: &str, to: &str) -> Result<(), VaultError> {
        self.validate_note_path(from)?;
        self.validate_note_path(to)?;
        if from == to
            || from.split('/').any(|part| part.starts_with('.'))
            || to.split('/').any(|part| part.starts_with('.'))
        {
            return Err(VaultError::InvalidPath);
        }
        let source = self.checked_existing(from, false)?;
        if !source.is_file() {
            return Err(VaultError::InvalidPath);
        }
        let to_path = Self::validate(to, false)?;
        let parent = to_path.parent().unwrap_or(Path::new(""));
        let dest_parent = self.checked_existing(&parent.to_string_lossy(), true)?;
        if !dest_parent.is_dir() {
            return Err(VaultError::InvalidPath);
        }
        let destination = self.root.join(to_path);
        // Hard linking publishes without replacing an existing destination.
        fs::hard_link(&source, &destination).map_err(|e| {
            if e.kind() == io::ErrorKind::AlreadyExists {
                VaultError::AlreadyExists
            } else {
                VaultError::Io(e)
            }
        })?;
        fs::File::open(&dest_parent)?.sync_all()?;
        fs::remove_file(&source)?;
        let source_parent = source.parent().ok_or(VaultError::InvalidPath)?;
        fs::File::open(source_parent)?.sync_all()?;
        Ok(())
    }

    /// Finish an interrupted, no-clobber note move. Source and destination
    /// must be byte-identical and share an inode when both names exist; this
    /// avoids deleting an independent external file with matching contents.
    pub fn finish_markdown_move(&self, from: &str, to: &str) -> Result<(), VaultError> {
        self.validate_note_path(from)?;
        self.validate_note_path(to)?;
        if from == to
            || from.split('/').any(|part| part.starts_with('.'))
            || to.split('/').any(|part| part.starts_with('.'))
        {
            return Err(VaultError::InvalidPath);
        }
        let destination = match self.checked_existing(to, false) {
            Ok(path) if path.is_file() => path,
            Ok(_) => return Err(VaultError::InvalidPath),
            Err(VaultError::NotFound) => return self.move_markdown(from, to),
            Err(e) => return Err(e),
        };
        match self.checked_existing(from, false) {
            Err(VaultError::NotFound) => Ok(()),
            Err(e) => Err(e),
            Ok(source) => {
                if !source.is_file() || !destination.is_file() {
                    return Err(VaultError::InvalidPath);
                }
                let a = fs::metadata(&source)?;
                let b = fs::metadata(&destination)?;
                // A cross-platform byte check is conservative but cannot prove
                // common provenance. The durable command recorded the paths;
                // only the same inode is safe to unlink in the crash gap.
                #[cfg(unix)]
                {
                    use std::os::unix::fs::MetadataExt;
                    if a.dev() != b.dev() || a.ino() != b.ino() {
                        return Err(VaultError::Conflict);
                    }
                }
                #[cfg(not(unix))]
                {
                    return Err(VaultError::Conflict);
                }
                if fs::read(&source)? != fs::read(&destination)? {
                    return Err(VaultError::Conflict);
                }
                fs::remove_file(&source)?;
                fs::File::open(source.parent().ok_or(VaultError::InvalidPath)?)?.sync_all()?;
                Ok(())
            }
        }
    }

    /// Rename or move a folder and its descendants, preserving nested paths.
    /// This cannot overwrite an existing target. Folder operations must be
    /// serialized against external writers by the eventual authoritative host.
    pub fn move_folder(&self, from: &str, to: &str) -> Result<(), VaultError> {
        let from_path = Self::validate(from, false)?;
        let to_path = Self::validate(to, false)?;
        if from_path == to_path
            || to_path.starts_with(&from_path)
            || from.split('/').any(|part| part.starts_with('.'))
            || to.split('/').any(|part| part.starts_with('.'))
        {
            return Err(VaultError::InvalidPath);
        }
        if from_path
            .components()
            .next()
            .is_some_and(|x| x.as_os_str() == ".orbit-operations")
            || to_path
                .components()
                .next()
                .is_some_and(|x| x.as_os_str() == ".orbit-operations")
        {
            return Err(VaultError::InvalidPath);
        }
        let source = self.checked_existing(from, false)?;
        if !source.is_dir() {
            return Err(VaultError::InvalidPath);
        }
        let parent = to_path.parent().unwrap_or(Path::new(""));
        let dest_parent = self.checked_existing(&parent.to_string_lossy(), true)?;
        if !dest_parent.is_dir() {
            return Err(VaultError::InvalidPath);
        }
        let destination = self.root.join(to_path);
        if fs::symlink_metadata(&destination).is_ok() {
            return Err(VaultError::AlreadyExists);
        }
        // Renames are atomic on the same filesystem, though they still need
        // an application-level durable intent and path-index reconciliation.
        fs::rename(&source, &destination)?;
        fs::File::open(&dest_parent)?.sync_all()?;
        fs::File::open(source.parent().ok_or(VaultError::InvalidPath)?)?.sync_all()?;
        Ok(())
    }

    /// Read the Markdown file itself, not an SQL snapshot of its body.
    pub fn read_markdown(&self, relative: &str) -> Result<String, VaultError> {
        self.validate_note_path(relative)?;
        if relative.split('/').any(|part| part.starts_with('.')) {
            return Err(VaultError::InvalidPath);
        }
        let path = self.checked_existing(relative, false)?;
        if !path.is_file()
            || !path
                .extension()
                .is_some_and(|x| x.eq_ignore_ascii_case("md"))
        {
            return Err(VaultError::InvalidPath);
        }
        Ok(fs::read_to_string(path)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn markdown_read_follows_real_folder_tree_and_rejects_private_paths() {
        let base = std::env::temp_dir().join(format!(
            "orbit-doc-read-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let vault = MarkdownVault::open(&base).unwrap();
        vault.create_folder("Work").unwrap();
        vault
            .create_markdown("Work/meeting.md", "# Meeting\nActual Markdown\n")
            .unwrap();
        assert_eq!(
            vault.read_markdown("Work/meeting.md").unwrap(),
            "# Meeting\nActual Markdown\n"
        );
        assert!(matches!(
            vault.read_markdown("../Work/meeting.md"),
            Err(VaultError::InvalidPath)
        ));
        assert!(matches!(
            vault.read_markdown(".orbit-operations/private.md"),
            Err(VaultError::InvalidPath)
        ));
        assert!(matches!(
            vault.read_markdown("Work"),
            Err(VaultError::InvalidPath)
        ));
        std::fs::remove_dir_all(base).unwrap();
    }
    #[test]
    fn folders_are_visible_and_markdown_is_read_from_disk() {
        let base = std::env::temp_dir().join(format!(
            "orbit-vault-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let vault = MarkdownVault::open(&base).unwrap();
        vault.create_folder("Work").unwrap();
        vault.create_folder("Work/Projects").unwrap();
        fs::write(base.join("Work/Projects/design.md"), "# Design\n").unwrap();
        assert_eq!(
            vault.read_markdown("Work/Projects/design.md").unwrap(),
            "# Design\n"
        );
        let tree = vault.tree().unwrap();
        assert!(format!("{tree:?}").contains("Work/Projects/design.md"));
        assert!(format!("{tree:?}").contains("Work/Projects"));
        assert!(matches!(
            vault.create_folder("../escape"),
            Err(VaultError::InvalidPath)
        ));
        assert!(matches!(
            vault.read_markdown("/etc/passwd"),
            Err(VaultError::InvalidPath)
        ));
        fs::remove_dir_all(base).unwrap();
    }
    #[test]
    fn replace_markdown_preserves_conflicting_edits_and_tree() {
        let base = std::env::temp_dir().join(format!(
            "orbit-replace-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let vault = MarkdownVault::open(&base).unwrap();
        vault.create_folder("Notes").unwrap();
        vault.create_markdown("Notes/a.md", "# Original\n").unwrap();
        assert!(matches!(
            vault.replace_markdown("Notes/a.md", "wrong", "overwrite"),
            Err(VaultError::Conflict)
        ));
        assert_eq!(vault.read_markdown("Notes/a.md").unwrap(), "# Original\n");
        vault
            .replace_markdown("Notes/a.md", "# Original\n", "# Revised\n")
            .unwrap();
        assert_eq!(vault.read_markdown("Notes/a.md").unwrap(), "# Revised\n");
        assert!(matches!(
            vault.replace_markdown("Notes/a.md", "# Original\n", "bad"),
            Err(VaultError::Conflict)
        ));
        assert!(matches!(
            vault.replace_markdown("../escape.md", "", "bad"),
            Err(VaultError::InvalidPath)
        ));
        assert!(format!("{:?}", vault.tree().unwrap()).contains("Notes/a.md"));
        assert!(!fs::read_dir(base.join("Notes")).unwrap().any(|e| {
            e.unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".orbit-stage-")
        }));
        fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn create_markdown_is_durable_no_clobber_and_nested() {
        let base = std::env::temp_dir().join(format!(
            "orbit-vault-create-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let vault = MarkdownVault::open(&base).unwrap();
        vault.create_folder("Research").unwrap();
        vault
            .create_markdown("Research/plan.md", "# First\n")
            .unwrap();
        assert_eq!(
            vault.read_markdown("Research/plan.md").unwrap(),
            "# First\n"
        );
        assert!(matches!(
            vault.create_markdown("Research/plan.md", "overwritten"),
            Err(VaultError::AlreadyExists)
        ));
        assert_eq!(
            vault.read_markdown("Research/plan.md").unwrap(),
            "# First\n"
        );
        assert!(matches!(
            vault.create_markdown("../escape.md", "bad"),
            Err(VaultError::InvalidPath)
        ));
        assert!(matches!(
            vault.create_markdown("Research/not-markdown.txt", "bad"),
            Err(VaultError::InvalidPath)
        ));
        assert!(!format!("{:?}", vault.tree().unwrap()).contains(".orbit-stage"));
        fs::remove_dir_all(base).unwrap();
    }
    #[cfg(unix)]
    #[test]
    fn create_refuses_symlink_parent() {
        use std::os::unix::fs::symlink;
        let base =
            std::env::temp_dir().join(format!("orbit-vault-create-link-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let vault = MarkdownVault::open(&base).unwrap();
        symlink("/tmp", base.join("outside")).unwrap();
        assert!(matches!(
            vault.create_markdown("outside/escape.md", "bad"),
            Err(VaultError::InvalidPath)
        ));
        fs::remove_dir_all(base).unwrap();
    }
    #[cfg(unix)]
    #[test]
    fn symlinks_are_not_exposed_or_followed() {
        use std::os::unix::fs::symlink;
        let base = std::env::temp_dir().join(format!("orbit-vault-symlink-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let vault = MarkdownVault::open(&base).unwrap();
        symlink("/etc", base.join("outside")).unwrap();
        assert!(matches!(
            vault.read_markdown("outside/passwd.md"),
            Err(VaultError::InvalidPath)
        ));
        assert!(!format!("{:?}", vault.tree().unwrap()).contains("outside"));
        fs::remove_dir_all(base).unwrap();
    }
    #[test]
    fn folder_and_note_moves_preserve_content_and_refuse_overwrite() {
        let base = std::env::temp_dir().join(format!(
            "orbit-move-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let vault = MarkdownVault::open(&base).unwrap();
        vault.create_folder("Work").unwrap();
        vault.create_folder("Personal").unwrap();
        vault
            .create_markdown("Work/idea.md", "# Preserve me\n")
            .unwrap();
        vault
            .move_markdown("Work/idea.md", "Personal/idea.md")
            .unwrap();
        assert_eq!(
            vault.read_markdown("Personal/idea.md").unwrap(),
            "# Preserve me\n"
        );
        assert!(matches!(
            vault.read_markdown("Work/idea.md"),
            Err(VaultError::NotFound)
        ));
        vault.create_markdown("Work/idea.md", "# Other\n").unwrap();
        assert!(matches!(
            vault.move_markdown("Work/idea.md", "Personal/idea.md"),
            Err(VaultError::AlreadyExists)
        ));
        assert_eq!(
            vault.read_markdown("Personal/idea.md").unwrap(),
            "# Preserve me\n"
        );
        vault.move_folder("Personal", "Archive").unwrap();
        assert_eq!(
            vault.read_markdown("Archive/idea.md").unwrap(),
            "# Preserve me\n"
        );
        assert!(matches!(
            vault.move_folder("Archive", "Archive/inner"),
            Err(VaultError::InvalidPath)
        ));
        assert!(matches!(
            vault.move_folder("Archive", "Work"),
            Err(VaultError::AlreadyExists)
        ));
        assert!(matches!(
            vault.move_folder(".orbit-operations", "private"),
            Err(VaultError::InvalidPath)
        ));
        assert!(format!("{:?}", vault.tree().unwrap()).contains("Archive/idea.md"));
        fs::remove_dir_all(base).unwrap();
    }
}
