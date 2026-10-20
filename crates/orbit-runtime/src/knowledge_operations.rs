//! Recoverable filesystem-side create operations for the future canonical
//! knowledge command pipeline. SQLite receipts are NOT yet wired to this log.
//! An intent remains on disk until publication is verified. Recovery never
//! overwrites conflicting external content.
use crate::knowledge_files::{MarkdownVault, VaultError};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
};

#[derive(Debug)]
pub enum OperationError {
    Io(io::Error),
    Vault(VaultError),
    CorruptIntent,
    ConflictingFile,
}
impl From<io::Error> for OperationError {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}
impl From<VaultError> for OperationError {
    fn from(e: VaultError) -> Self {
        Self::Vault(e)
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CreateIntent {
    operation_id: String,
    relative_path: String,
    body: String,
    sha256: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct UpdateIntent {
    operation_id: String,
    relative_path: String,
    expected: String,
    expected_sha256: String,
    body: String,
    sha256: String,
}

enum JournalEntry {
    Create(CreateIntent),
    Update(UpdateIntent),
}

/// Filesystem recovery component. The private journal is intentionally excluded
/// from the knowledge tree, and is owned by the same authoritative host as the
/// Markdown vault. The caller must serialize mutations and recovery.
pub struct KnowledgeOperations {
    vault: MarkdownVault,
    journal: PathBuf,
}
impl KnowledgeOperations {
    pub fn open(root: impl AsRef<Path>) -> Result<Self, OperationError> {
        let vault = MarkdownVault::open(root)?;
        let journal = vault.root_path().join(".orbit-operations");
        if journal.exists() {
            let metadata = fs::symlink_metadata(&journal)?;
            if !metadata.is_dir() || metadata.file_type().is_symlink() {
                return Err(OperationError::CorruptIntent);
            }
        } else {
            fs::create_dir(&journal)?;
        }
        fs::File::open(vault.root_path())?.sync_all()?;
        let result = Self { vault, journal };
        result.recover()?;
        Ok(result)
    }

    fn digest(body: &str) -> String {
        format!("{:x}", Sha256::digest(body.as_bytes()))
    }
    fn valid_id(id: &str) -> bool {
        !id.is_empty()
            && id.len() <= 128
            && id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    }
    fn sync_dir(dir: &Path) -> Result<(), OperationError> {
        fs::File::open(dir)?.sync_all()?;
        Ok(())
    }
    fn intent_path(&self, id: &str) -> PathBuf {
        self.journal.join(format!("{id}.json"))
    }

    /// The durable intent contains the Markdown text temporarily, solely so a
    /// crash can be rolled forward. It is deleted after publication succeeds.
    /// This is NOT an accepted domain command / receipt.
    pub fn create(
        &self,
        operation_id: &str,
        relative_path: &str,
        body: &str,
    ) -> Result<(), OperationError> {
        if !Self::valid_id(operation_id) || operation_id.starts_with("update-") {
            return Err(OperationError::CorruptIntent);
        }
        self.vault.validate_note_path(relative_path)?;
        let intent = CreateIntent {
            operation_id: operation_id.into(),
            relative_path: relative_path.into(),
            body: body.into(),
            sha256: Self::digest(body),
        };
        let path = self.intent_path(operation_id);
        let encoded = serde_json::to_vec(&intent).map_err(|_| OperationError::CorruptIntent)?;
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(mut f) => {
                if let Err(e) = f.write_all(&encoded).and_then(|_| f.sync_all()) {
                    return Err(e.into());
                }
                Self::sync_dir(&self.journal)?;
            }
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
                let previous = self.read_intent(&path)?;
                if previous.relative_path != relative_path
                    || previous.sha256 != intent.sha256
                    || previous.body != body
                {
                    return Err(OperationError::ConflictingFile);
                }
            }
            Err(e) => return Err(e.into()),
        }
        // A deterministic pre-publication failure (missing parent folder, bad
        // path) must not leave a journal entry behind: recovery would replay a
        // command the caller was explicitly told had failed, and could block
        // every later authority open. Crash-ambiguous outcomes and conflicting
        // external content still retain the intent for operator inspection.
        match self.publish(&intent) {
            Ok(()) => {
                fs::remove_file(path)?;
                Self::sync_dir(&self.journal)
            }
            Err(
                error @ (OperationError::Vault(VaultError::NotFound)
                | OperationError::Vault(VaultError::InvalidPath)),
            ) => {
                fs::remove_file(&path)?;
                Self::sync_dir(&self.journal)?;
                Err(error)
            }
            Err(error) => Err(error),
        }
    }

    /// Persist the expected and desired bytes before touching the original.
    /// Replay only if the note still matches either version; conflicting
    /// external changes halt startup rather than being overwritten.
    pub fn update(
        &self,
        operation_id: &str,
        relative_path: &str,
        expected: &str,
        body: &str,
    ) -> Result<(), OperationError> {
        if !Self::valid_id(operation_id) {
            return Err(OperationError::CorruptIntent);
        }
        self.vault.validate_note_path(relative_path)?;
        let intent = UpdateIntent {
            operation_id: operation_id.into(),
            relative_path: relative_path.into(),
            expected: expected.into(),
            expected_sha256: Self::digest(expected),
            body: body.into(),
            sha256: Self::digest(body),
        };
        // Distinct journal namespace avoids collisions with legacy create IDs.
        let path = self.journal.join(format!("update-{operation_id}.json"));
        let encoded = serde_json::to_vec(&intent).map_err(|_| OperationError::CorruptIntent)?;
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(mut f) => {
                f.write_all(&encoded)?;
                f.sync_all()?;
                Self::sync_dir(&self.journal)?;
            }
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
                let previous = self.read_update_intent(&path)?;
                if previous.relative_path != relative_path
                    || previous.expected != expected
                    || previous.body != body
                {
                    return Err(OperationError::ConflictingFile);
                }
            }
            Err(e) => return Err(e.into()),
        }
        self.publish_update(&intent)?;
        fs::remove_file(path)?;
        Self::sync_dir(&self.journal)
    }

    fn read_update_intent(&self, path: &Path) -> Result<UpdateIntent, OperationError> {
        if fs::symlink_metadata(path)?.file_type().is_symlink() {
            return Err(OperationError::CorruptIntent);
        }
        let intent: UpdateIntent =
            serde_json::from_slice(&fs::read(path)?).map_err(|_| OperationError::CorruptIntent)?;
        if !Self::valid_id(&intent.operation_id)
            || Self::digest(&intent.body) != intent.sha256
            || Self::digest(&intent.expected) != intent.expected_sha256
            || self
                .journal
                .join(format!("update-{}.json", intent.operation_id))
                != path
        {
            return Err(OperationError::CorruptIntent);
        }
        self.vault.validate_note_path(&intent.relative_path)?;
        Ok(intent)
    }

    fn publish_update(&self, intent: &UpdateIntent) -> Result<(), OperationError> {
        let actual = match self.vault.read_markdown(&intent.relative_path) {
            Ok(body) => body,
            Err(VaultError::NotFound) => return Err(OperationError::ConflictingFile),
            Err(err) => return Err(err.into()),
        };
        if actual == intent.body {
            return Ok(());
        }
        if actual != intent.expected {
            return Err(OperationError::ConflictingFile);
        }
        match self
            .vault
            .replace_markdown(&intent.relative_path, &intent.expected, &intent.body)
        {
            Ok(()) => Ok(()),
            Err(VaultError::Conflict) => Err(OperationError::ConflictingFile),
            Err(err) => Err(err.into()),
        }
    }

    fn read_intent(&self, path: &Path) -> Result<CreateIntent, OperationError> {
        if fs::symlink_metadata(path)?.file_type().is_symlink() {
            return Err(OperationError::CorruptIntent);
        }
        let bytes = fs::read(path)?;
        let intent: CreateIntent =
            serde_json::from_slice(&bytes).map_err(|_| OperationError::CorruptIntent)?;
        if !Self::valid_id(&intent.operation_id)
            || intent.operation_id.starts_with("update-")
            || Self::digest(&intent.body) != intent.sha256
        {
            return Err(OperationError::CorruptIntent);
        }
        self.vault.validate_note_path(&intent.relative_path)?;
        if self.intent_path(&intent.operation_id) != path {
            return Err(OperationError::CorruptIntent);
        }
        Ok(intent)
    }

    fn publish(&self, intent: &CreateIntent) -> Result<(), OperationError> {
        match self
            .vault
            .create_markdown(&intent.relative_path, &intent.body)
        {
            Ok(()) => Ok(()),
            Err(VaultError::AlreadyExists) => {
                let actual = self.vault.read_markdown(&intent.relative_path)?;
                if actual.as_bytes() == intent.body.as_bytes() {
                    Ok(())
                } else {
                    Err(OperationError::ConflictingFile)
                }
            }
            Err(e) => Err(e.into()),
        }
    }

    /// Inspect the complete journal without modifying files. Malformed or
    /// conflicting entries must stop recovery *before* any earlier intent is
    /// published. This also rejects two different operations on the same path:
    /// their order is not a safe substitute for a domain-level revision.
    fn preflight(&self) -> Result<Vec<JournalEntry>, OperationError> {
        let mut entries = fs::read_dir(&self.journal)?.collect::<Result<Vec<_>, _>>()?;
        entries.sort_by_key(|e| e.file_name());
        let mut result = Vec::with_capacity(entries.len());
        let mut paths = std::collections::HashSet::new();
        for entry in entries {
            if !entry.file_type()?.is_file() || entry.path().extension().is_none_or(|x| x != "json")
            {
                return Err(OperationError::CorruptIntent);
            }
            let file_name = entry.file_name();
            let name = file_name.to_str().ok_or(OperationError::CorruptIntent)?;
            let item = if name.starts_with("update-") {
                JournalEntry::Update(self.read_update_intent(&entry.path())?)
            } else {
                JournalEntry::Create(self.read_intent(&entry.path())?)
            };
            let target = match &item {
                JournalEntry::Create(v) => &v.relative_path,
                JournalEntry::Update(v) => &v.relative_path,
            };
            if !paths.insert(target.clone()) {
                return Err(OperationError::CorruptIntent);
            }
            // Reject divergent external edits in preflight rather than after
            // replaying preceding valid entries. An already-published desired
            // document is safe to acknowledge on restart.
            let existing = self.vault.read_markdown(target);
            match (&item, existing) {
                (JournalEntry::Create(v), Ok(actual)) if actual == v.body => {}
                (JournalEntry::Create(_), Ok(_)) => return Err(OperationError::ConflictingFile),
                (JournalEntry::Create(_), Err(VaultError::NotFound)) => {}
                (JournalEntry::Update(v), Ok(actual))
                    if actual == v.expected || actual == v.body => {}
                (JournalEntry::Update(_), Ok(_)) => return Err(OperationError::ConflictingFile),
                (JournalEntry::Update(_), Err(VaultError::NotFound)) => {
                    return Err(OperationError::ConflictingFile);
                }
                (_, Err(e)) => return Err(e.into()),
            }
            result.push(item);
        }
        Ok(result)
    }

    /// Recover only after every queued operation passes integrity, path,
    /// identity and expected-content checks. Failed preflight leaves all
    /// journal entries and all documents untouched for operator inspection.
    pub fn recover(&self) -> Result<(), OperationError> {
        let entries = self.preflight()?;
        for item in entries {
            let path = match &item {
                JournalEntry::Create(v) => {
                    self.publish(v)?;
                    self.intent_path(&v.operation_id)
                }
                JournalEntry::Update(v) => {
                    self.publish_update(v)?;
                    self.journal.join(format!("update-{}.json", v.operation_id))
                }
            };
            fs::remove_file(path)?;
            Self::sync_dir(&self.journal)?;
        }
        Ok(())
    }

    pub fn vault(&self) -> &MarkdownVault {
        &self.vault
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn base() -> PathBuf {
        std::env::temp_dir().join(format!(
            "orbit-op-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }
    #[test]
    fn rejects_create_ids_colliding_with_update_journal_namespace() {
        let root = base();
        let service = KnowledgeOperations::open(&root).unwrap();
        assert!(matches!(
            service.create("update-edit1", "note.md", "text"),
            Err(OperationError::CorruptIntent)
        ));
        assert!(!root.join("note.md").exists());
        assert_eq!(
            fs::read_dir(root.join(".orbit-operations"))
                .unwrap()
                .count(),
            0
        );
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn rejected_nested_create_does_not_poison_later_authority_opens() {
        let root = base();
        {
            let service = KnowledgeOperations::open(&root).unwrap();
            assert!(
                service
                    .create("nested-note", "missing/note.md", "body")
                    .is_err()
            );
            assert_eq!(
                fs::read_dir(root.join(".orbit-operations"))
                    .unwrap()
                    .count(),
                0,
                "a rejected create must not leave a replayable intent"
            );
        }
        let service = KnowledgeOperations::open(&root).unwrap();
        service.create("root-note", "note.md", "body").unwrap();
        assert_eq!(service.vault().read_markdown("note.md").unwrap(), "body");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn startup_rejects_ambiguous_create_intent_without_publishing() {
        let root = base();
        let service = KnowledgeOperations::open(&root).unwrap();
        let intent = CreateIntent {
            operation_id: "update-fake".into(),
            relative_path: "note.md".into(),
            body: "secret".into(),
            sha256: KnowledgeOperations::digest("secret"),
        };
        fs::write(
            service.intent_path("update-fake"),
            serde_json::to_vec(&intent).unwrap(),
        )
        .unwrap();
        assert!(matches!(
            service.recover(),
            Err(OperationError::CorruptIntent)
        ));
        assert!(!root.join("note.md").exists());
        assert!(service.intent_path("update-fake").exists());
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn creates_real_markdown_and_clears_intent() {
        let root = base();
        let service = KnowledgeOperations::open(&root).unwrap();
        service.vault().create_folder("Work").unwrap();
        service
            .create("create_1", "Work/note.md", "# Hello")
            .unwrap();
        assert_eq!(
            fs::read_to_string(root.join("Work/note.md")).unwrap(),
            "# Hello"
        );
        assert_eq!(
            fs::read_dir(root.join(".orbit-operations"))
                .unwrap()
                .count(),
            0
        );
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn replays_interrupted_publication_without_overwrite() {
        let root = base();
        let service = KnowledgeOperations::open(&root).unwrap();
        service.vault().create_folder("Work").unwrap();
        let intent = CreateIntent {
            operation_id: "recovery".into(),
            relative_path: "Work/recover.md".into(),
            body: "test".into(),
            sha256: KnowledgeOperations::digest("test"),
        };
        fs::write(
            service.intent_path("recovery"),
            serde_json::to_vec(&intent).unwrap(),
        )
        .unwrap();
        drop(service);
        let service = KnowledgeOperations::open(&root).unwrap();
        assert_eq!(
            service.vault().read_markdown("Work/recover.md").unwrap(),
            "test"
        );
        assert!(!service.intent_path("recovery").exists());
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn update_recovery_rolls_forward_and_clears_intent() {
        let root = base();
        let service = KnowledgeOperations::open(&root).unwrap();
        service.create("init", "note.md", "old").unwrap();
        let intent = UpdateIntent {
            operation_id: "edit1".into(),
            relative_path: "note.md".into(),
            expected: "old".into(),
            expected_sha256: KnowledgeOperations::digest("old"),
            body: "new".into(),
            sha256: KnowledgeOperations::digest("new"),
        };
        fs::write(
            service.journal.join("update-edit1.json"),
            serde_json::to_vec(&intent).unwrap(),
        )
        .unwrap();
        drop(service);
        let service = KnowledgeOperations::open(&root).unwrap();
        assert_eq!(service.vault().read_markdown("note.md").unwrap(), "new");
        assert_eq!(fs::read_dir(&service.journal).unwrap().count(), 0);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn update_recovery_refuses_conflicting_external_edit() {
        let root = base();
        let service = KnowledgeOperations::open(&root).unwrap();
        service.create("init", "note.md", "old").unwrap();
        let intent = UpdateIntent {
            operation_id: "edit1".into(),
            relative_path: "note.md".into(),
            expected: "old".into(),
            expected_sha256: KnowledgeOperations::digest("old"),
            body: "new".into(),
            sha256: KnowledgeOperations::digest("new"),
        };
        fs::write(
            service.journal.join("update-edit1.json"),
            serde_json::to_vec(&intent).unwrap(),
        )
        .unwrap();
        fs::write(root.join("note.md"), "external").unwrap();
        assert!(matches!(
            service.recover(),
            Err(OperationError::ConflictingFile)
        ));
        assert_eq!(
            fs::read_to_string(root.join("note.md")).unwrap(),
            "external"
        );
        assert!(service.journal.join("update-edit1.json").exists());
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn conflicting_external_edit_preserves_intent() {
        let root = base();
        let service = KnowledgeOperations::open(&root).unwrap();
        let intent = CreateIntent {
            operation_id: "collision".into(),
            relative_path: "collision.md".into(),
            body: "first".into(),
            sha256: KnowledgeOperations::digest("first"),
        };
        fs::write(
            service.intent_path("collision"),
            serde_json::to_vec(&intent).unwrap(),
        )
        .unwrap();
        fs::write(root.join("collision.md"), "external").unwrap();
        assert!(matches!(
            service.recover(),
            Err(OperationError::ConflictingFile)
        ));
        assert!(service.intent_path("collision").exists());
        assert_eq!(
            fs::read_to_string(root.join("collision.md")).unwrap(),
            "external"
        );
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn corrupt_later_entry_does_not_publish_earlier_valid_entry() {
        let root = base();
        let service = KnowledgeOperations::open(&root).unwrap();
        let body = "hello";
        let intent = CreateIntent {
            operation_id: "a-valid".into(),
            relative_path: "good.md".into(),
            body: body.into(),
            sha256: KnowledgeOperations::digest(body),
        };
        fs::write(
            service.intent_path("a-valid"),
            serde_json::to_vec(&intent).unwrap(),
        )
        .unwrap();
        fs::write(service.intent_path("z-corrupt"), b"not json").unwrap();
        assert!(matches!(
            service.recover(),
            Err(OperationError::CorruptIntent)
        ));
        assert!(!root.join("good.md").exists());
        assert!(service.intent_path("a-valid").exists());
        fs::remove_file(service.intent_path("z-corrupt")).unwrap();
        service.recover().unwrap();
        assert_eq!(fs::read_to_string(root.join("good.md")).unwrap(), body);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn late_external_conflict_does_not_publish_earlier_entry() {
        let root = base();
        let service = KnowledgeOperations::open(&root).unwrap();
        let first = CreateIntent {
            operation_id: "a-create".into(),
            relative_path: "fresh.md".into(),
            body: "new".into(),
            sha256: KnowledgeOperations::digest("new"),
        };
        fs::write(
            service.intent_path("a-create"),
            serde_json::to_vec(&first).unwrap(),
        )
        .unwrap();
        fs::write(root.join("existing.md"), "external").unwrap();
        let second = UpdateIntent {
            operation_id: "z-update".into(),
            relative_path: "existing.md".into(),
            expected: "before".into(),
            expected_sha256: KnowledgeOperations::digest("before"),
            body: "after".into(),
            sha256: KnowledgeOperations::digest("after"),
        };
        fs::write(
            service.journal.join("update-z-update.json"),
            serde_json::to_vec(&second).unwrap(),
        )
        .unwrap();
        assert!(matches!(
            service.recover(),
            Err(OperationError::ConflictingFile)
        ));
        assert!(!root.join("fresh.md").exists());
        assert_eq!(
            fs::read_to_string(root.join("existing.md")).unwrap(),
            "external"
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn ambiguous_multiple_intents_for_one_note_fail_before_publication() {
        let root = base();
        let service = KnowledgeOperations::open(&root).unwrap();
        for id in ["first", "second"] {
            let intent = CreateIntent {
                operation_id: id.into(),
                relative_path: "same.md".into(),
                body: "identical".into(),
                sha256: KnowledgeOperations::digest("identical"),
            };
            fs::write(
                service.intent_path(id),
                serde_json::to_vec(&intent).unwrap(),
            )
            .unwrap();
        }
        assert!(matches!(
            service.recover(),
            Err(OperationError::CorruptIntent)
        ));
        assert!(!root.join("same.md").exists());
        let _ = fs::remove_dir_all(root);
    }
}
