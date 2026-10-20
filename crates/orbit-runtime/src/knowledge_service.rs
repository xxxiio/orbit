//! File-authoritative knowledge identity and discovery service.
//!
//! This is the first domain-level slice of the Markdown cutover. It deliberately
//! does not issue accepted commands: SQL revisions and receipts are not yet
//! coordinated with filesystem publication. File contents remain authoritative.
use crate::{
    knowledge_files::{KnowledgeNode, VaultError},
    knowledge_operations::{KnowledgeOperations, OperationError},
};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct KnowledgeDocumentInfo {
    pub id: String,
    pub path: String,
    pub title: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct KnowledgeFileRelations {
    pub outgoing: Vec<String>,
    pub backlinks: Vec<String>,
    pub unresolved: Vec<String>,
}

/// Deliberately narrow link grammar: [[stable-id]] or [[stable-id|label]].
/// IDs with unsupported characters are not treated as reliable references.
fn explicit_links(markdown: &str) -> std::collections::BTreeSet<String> {
    let mut links = std::collections::BTreeSet::new();
    let mut remaining = markdown;
    while let Some(start) = remaining.find("[[") {
        remaining = &remaining[start + 2..];
        let Some(end) = remaining.find("]]") else {
            break;
        };
        let value = remaining[..end].split('|').next().unwrap_or("");
        if !value.is_empty()
            && value.len() <= 128
            && value
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        {
            links.insert(value.to_owned());
        }
        remaining = &remaining[end + 2..];
    }
    links
}

#[derive(Debug)]
pub enum KnowledgeServiceError {
    Vault(VaultError),
    Operation(OperationError),
    InvalidIdentity,
    DuplicateIdentity(String),
    IdentityConflict,
    MissingIdentity,
}
impl From<VaultError> for KnowledgeServiceError {
    fn from(value: VaultError) -> Self {
        Self::Vault(value)
    }
}
impl From<OperationError> for KnowledgeServiceError {
    fn from(value: OperationError) -> Self {
        Self::Operation(value)
    }
}

/// Stable identity lives in Markdown frontmatter; physical folders determine
/// organization. An eventual SQLite index may cache these values, but must be
/// rebuilt/reconciled from these files rather than serve as a rival content store.
pub struct KnowledgeService<'a> {
    operations: &'a KnowledgeOperations,
}
impl<'a> KnowledgeService<'a> {
    pub fn new(operations: &'a KnowledgeOperations) -> Self {
        Self { operations }
    }

    fn valid_id(id: &str) -> bool {
        !id.is_empty()
            && id.len() <= 128
            && id
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
    }

    pub(crate) fn identity(markdown: &str) -> Result<String, KnowledgeServiceError> {
        let mut lines = markdown.lines();
        if lines.next() != Some("---") {
            return Err(KnowledgeServiceError::MissingIdentity);
        }
        let mut id = None;
        let mut closed = false;
        for line in lines {
            if line == "---" {
                closed = true;
                break;
            }
            if let Some(value) = line.strip_prefix("orbit_id: ") {
                if id.is_some() || !Self::valid_id(value) {
                    return Err(KnowledgeServiceError::InvalidIdentity);
                }
                id = Some(value.to_owned());
            }
        }
        if !closed {
            return Err(KnowledgeServiceError::InvalidIdentity);
        }
        id.ok_or(KnowledgeServiceError::MissingIdentity)
    }

    fn collect(
        &self,
        node: &KnowledgeNode,
        seen: &mut BTreeMap<String, KnowledgeDocumentInfo>,
    ) -> Result<(), KnowledgeServiceError> {
        match node {
            KnowledgeNode::Folder { children, .. } => {
                for child in children {
                    self.collect(child, seen)?;
                }
            }
            KnowledgeNode::Note { name, path } => {
                let body = self.operations.vault().read_markdown(path)?;
                let id = Self::identity(&body)?;
                let info = KnowledgeDocumentInfo {
                    id: id.clone(),
                    path: path.clone(),
                    title: name.strip_suffix(".md").unwrap_or(name).to_owned(),
                };
                if seen.insert(id.clone(), info).is_some() {
                    return Err(KnowledgeServiceError::DuplicateIdentity(id));
                }
            }
        }
        Ok(())
    }

    /// Index content by stable note ID. Reject duplicate identities, including
    /// the two paths left by an interrupted hard-link move: never silently pick
    /// one of two candidates for an accepted note identity.
    pub fn discover(&self) -> Result<Vec<KnowledgeDocumentInfo>, KnowledgeServiceError> {
        let mut seen = BTreeMap::new();
        self.collect(&self.operations.vault().tree()?, &mut seen)?;
        Ok(seen.into_values().collect())
    }

    /// Resolve a note by stable frontmatter identity, never by a stale cached path.
    /// A duplicate identity or malformed document anywhere in the vault fails
    /// closed before any document content is returned.
    pub fn read_by_id(
        &self,
        note_id: &str,
    ) -> Result<Option<(KnowledgeDocumentInfo, String)>, KnowledgeServiceError> {
        if !Self::valid_id(note_id) {
            return Err(KnowledgeServiceError::InvalidIdentity);
        }
        let note = self.discover()?.into_iter().find(|note| note.id == note_id);
        match note {
            Some(info) => {
                let markdown = self.operations.vault().read_markdown(&info.path)?;
                // Detect external changes between the discovery scan and read.
                if Self::identity(&markdown)? != info.id {
                    return Err(KnowledgeServiceError::IdentityConflict);
                }
                Ok(Some((info, markdown)))
            }
            None => Ok(None),
        }
    }

    /// A content-search baseline, always reading real Markdown bytes rather
    /// than the transitional SQL body column. Results are stable-ID keyed.
    pub fn search(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<KnowledgeDocumentInfo>, KnowledgeServiceError> {
        if query.trim().is_empty() {
            return Ok(Vec::new());
        }
        let needle = query.to_lowercase();
        let mut found = Vec::new();
        for info in self.discover()? {
            let markdown = self.operations.vault().read_markdown(&info.path)?;
            if Self::identity(&markdown)? != info.id {
                return Err(KnowledgeServiceError::IdentityConflict);
            }
            if info.title.to_lowercase().contains(&needle)
                || markdown.to_lowercase().contains(&needle)
            {
                found.push(info);
                if found.len() >= limit.min(200) {
                    break;
                }
            }
        }
        if limit == 0 {
            found.clear();
        }
        Ok(found)
    }

    /// Resolve explicit [[stable-id]] references without relying on paths.
    /// Unresolved targets are reported, and backlink queries read actual files.
    pub fn relations(
        &self,
        note_id: &str,
    ) -> Result<Option<KnowledgeFileRelations>, KnowledgeServiceError> {
        if !Self::valid_id(note_id) {
            return Err(KnowledgeServiceError::InvalidIdentity);
        }
        let notes = self.discover()?;
        if !notes.iter().any(|note| note.id == note_id) {
            return Ok(None);
        }
        let ids: std::collections::BTreeSet<String> = notes.iter().map(|n| n.id.clone()).collect();
        let mut outgoing = std::collections::BTreeSet::new();
        let mut backlinks = std::collections::BTreeSet::new();
        for note in &notes {
            let markdown = self.operations.vault().read_markdown(&note.path)?;
            if Self::identity(&markdown)? != note.id {
                return Err(KnowledgeServiceError::IdentityConflict);
            }
            for target in explicit_links(&markdown) {
                if note.id == note_id {
                    outgoing.insert(target.clone());
                }
                if target == note_id && note.id != note_id {
                    backlinks.insert(note.id.clone());
                }
            }
        }
        let unresolved = outgoing
            .iter()
            .filter(|id| !ids.contains(*id))
            .cloned()
            .collect();
        Ok(Some(KnowledgeFileRelations {
            outgoing: outgoing.into_iter().collect(),
            backlinks: backlinks.into_iter().collect(),
            unresolved,
        }))
    }

    /// Edit an existing note by identity, never by caller-supplied physical path.
    /// The expected Markdown bytes provide an optimistic concurrency guard.
    /// This is a filesystem operation, NOT a canonical accepted SQL command.
    pub fn stage_update_by_id(
        &self,
        operation_id: &str,
        note_id: &str,
        expected: &str,
        replacement: &str,
    ) -> Result<(), KnowledgeServiceError> {
        if !Self::valid_id(note_id) {
            return Err(KnowledgeServiceError::InvalidIdentity);
        }
        if Self::identity(expected)? != note_id || Self::identity(replacement)? != note_id {
            return Err(KnowledgeServiceError::IdentityConflict);
        }
        let (note, current) = self
            .read_by_id(note_id)?
            .ok_or(KnowledgeServiceError::IdentityConflict)?;
        if current != expected {
            return Err(KnowledgeServiceError::IdentityConflict);
        }
        self.operations
            .update(operation_id, &note.path, expected, replacement)?;
        Ok(())
    }

    /// Move a note by stable identity. The frontmatter identity stays unchanged.
    /// This is not yet a revisioned or recoverable authoritative move command.
    pub fn move_by_id(
        &self,
        note_id: &str,
        destination: &str,
    ) -> Result<(), KnowledgeServiceError> {
        let (note, _) = self
            .read_by_id(note_id)?
            .ok_or(KnowledgeServiceError::IdentityConflict)?;
        self.operations
            .vault()
            .move_markdown(&note.path, destination)?;
        Ok(())
    }

    /// Complete an authority-journaled move, including the crash gap where
    /// hard-link publication succeeded but removing the old name did not.
    /// Do not invoke without a durable command intent.
    pub fn complete_move(
        &self,
        note_id: &str,
        from: &str,
        to: &str,
        expected: &str,
    ) -> Result<(), KnowledgeServiceError> {
        if !Self::valid_id(note_id) {
            return Err(KnowledgeServiceError::InvalidIdentity);
        }
        // Verify the named paths directly; discovery fails on duplicate IDs
        // during the hard-link/unlink interruption window.
        let read_optional = |path: &str| -> Result<Option<String>, KnowledgeServiceError> {
            match self.operations.vault().read_markdown(path) {
                Ok(markdown) => Ok(Some(markdown)),
                Err(VaultError::NotFound) => Ok(None),
                Err(err) => Err(err.into()),
            }
        };
        let destination = read_optional(to)?;
        let source = read_optional(from)?;
        if Self::identity(expected)? != note_id {
            return Err(KnowledgeServiceError::IdentityConflict);
        }
        match (&source, &destination) {
            (Some(old), Some(new)) => {
                if Self::identity(old)? != note_id
                    || Self::identity(new)? != note_id
                    || old != expected
                    || new != expected
                {
                    return Err(KnowledgeServiceError::IdentityConflict);
                }
            }
            (Some(old), None) if Self::identity(old)? == note_id && old == expected => {}
            (None, Some(new)) if Self::identity(new)? == note_id && new == expected => {
                return Ok(());
            }
            _ => return Err(KnowledgeServiceError::IdentityConflict),
        }
        self.operations.vault().finish_markdown_move(from, to)?;
        Ok(())
    }

    /// Pre-commit filesystem creation only. Do not use as an authoritative
    /// command acknowledgement until SQLite acceptance/recovery is integrated.
    pub fn stage_create(
        &self,
        operation_id: &str,
        note_id: &str,
        path: &str,
        body: &str,
    ) -> Result<(), KnowledgeServiceError> {
        if !Self::valid_id(note_id) {
            return Err(KnowledgeServiceError::InvalidIdentity);
        }
        if self
            .discover()?
            .iter()
            .any(|note| note.id == note_id || note.path == path)
        {
            return Err(KnowledgeServiceError::IdentityConflict);
        }
        // The caller provides the Markdown body; identity is kept in file
        // frontmatter, so paths can change without breaking references.
        let text = if body.starts_with("---\n") {
            if Self::identity(body)?.as_str() != note_id {
                return Err(KnowledgeServiceError::IdentityConflict);
            }
            body.to_owned()
        } else {
            format!("---\norbit_id: {note_id}\n---\n{body}")
        };
        self.operations.create(operation_id, path, &text)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };
    fn root() -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "orbit-knowledge-service-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }
    #[test]
    fn identity_survives_folder_move_and_duplicate_identity_fails_closed() {
        let dir = root();
        let ops = KnowledgeOperations::open(&dir).unwrap();
        ops.vault().create_folder("Work").unwrap();
        ops.vault().create_folder("Personal").unwrap();
        let service = KnowledgeService::new(&ops);
        service
            .stage_create("create1", "note-123", "Work/orbit.md", "# Orbit\n")
            .unwrap();
        assert_eq!(service.discover().unwrap()[0].id, "note-123");
        ops.vault()
            .move_markdown("Work/orbit.md", "Personal/orbit.md")
            .unwrap();
        assert_eq!(service.discover().unwrap()[0].path, "Personal/orbit.md");
        assert!(matches!(
            service.stage_create("create2", "note-123", "Work/other.md", "x"),
            Err(KnowledgeServiceError::IdentityConflict)
        ));
        fs::hard_link(dir.join("Personal/orbit.md"), dir.join("Work/duplicate.md")).unwrap();
        assert!(matches!(
            service.discover(),
            Err(KnowledgeServiceError::DuplicateIdentity(_))
        ));
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn malformed_or_missing_identity_is_not_inferred_from_filename() {
        let dir = root();
        let ops = KnowledgeOperations::open(&dir).unwrap();
        ops.vault()
            .create_markdown("orphan.md", "# No metadata\n")
            .unwrap();
        let service = KnowledgeService::new(&ops);
        assert!(matches!(
            service.discover(),
            Err(KnowledgeServiceError::MissingIdentity)
        ));
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn file_backed_read_search_and_relations_follow_moves() {
        let dir = root();
        let ops = KnowledgeOperations::open(&dir).unwrap();
        ops.vault().create_folder("Work").unwrap();
        ops.vault().create_folder("Research").unwrap();
        let service = KnowledgeService::new(&ops);
        service
            .stage_create(
                "createA",
                "alpha",
                "Work/alpha.md",
                "# Alpha\nSee [[beta]] and [[missing]].\n",
            )
            .unwrap();
        service
            .stage_create(
                "createB",
                "beta",
                "Research/beta.md",
                "# Beta\nSee [[alpha]].\n",
            )
            .unwrap();
        assert!(
            service
                .read_by_id("alpha")
                .unwrap()
                .unwrap()
                .1
                .contains("[[beta]]")
        );
        assert_eq!(service.search("Alpha", 10).unwrap().len(), 2); // title and backlink
        assert!(service.search("Alpha", 0).unwrap().is_empty());
        let relations = service.relations("alpha").unwrap().unwrap();
        assert_eq!(relations.outgoing, vec!["beta", "missing"]);
        assert_eq!(relations.backlinks, vec!["beta"]);
        assert_eq!(relations.unresolved, vec!["missing"]);
        ops.vault()
            .move_markdown("Work/alpha.md", "Research/alpha.md")
            .unwrap();
        assert_eq!(
            service.read_by_id("alpha").unwrap().unwrap().0.path,
            "Research/alpha.md"
        );
        fs::remove_dir_all(dir).unwrap();
    }
}
