//! Authoritative, replayable physical folder renames. A folder move is a single
//! filesystem rename, followed by a transactional SQLite path-index update and
//! command receipt. Pending operations are replayed before serving requests.
use crate::{
    knowledge_operations::KnowledgeOperations,
    store::{AuthorityError, AuthorityStore},
};
use rusqlite::{OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FolderMove {
    pub from: String,
    pub to: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FolderReceipt {
    pub command_id: String,
    pub from: String,
    pub to: String,
}

fn valid_folder(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 1024
        && !s.contains('\\')
        && !s.contains('\0')
        && s.split('/')
            .all(|v| !v.is_empty() && v != "." && v != ".." && !v.starts_with('.'))
}
fn valid_id(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 128
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}
pub(crate) fn schema(store: &AuthorityStore) -> Result<(), AuthorityError> {
    store.conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS markdown_folder_commands (
        command_id TEXT PRIMARY KEY NOT NULL, actor TEXT NOT NULL,
        from_path TEXT NOT NULL, to_path TEXT NOT NULL,
        state TEXT NOT NULL CHECK(state IN ('pending','accepted')),
        receipt TEXT
    );",
    )?;
    Ok(())
}
pub(crate) fn pending(store: &AuthorityStore) -> Result<Vec<(String, FolderMove)>, AuthorityError> {
    let mut stmt = store.conn.prepare("SELECT command_id,from_path,to_path FROM markdown_folder_commands WHERE state='pending' ORDER BY rowid")?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get(0)?,
            FolderMove {
                from: r.get(1)?,
                to: r.get(2)?,
            },
        ))
    })?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}
pub(crate) fn enqueue(
    store: &mut AuthorityStore,
    actor: &str,
    id: &str,
    mv: &FolderMove,
) -> Result<Option<FolderReceipt>, AuthorityError> {
    if !valid_id(actor)
        || !valid_id(id)
        || !valid_folder(&mv.from)
        || !valid_folder(&mv.to)
        || mv.from == mv.to
        || mv.to.starts_with(&(mv.from.clone() + "/"))
    {
        return Err(AuthorityError::Invalid("invalid folder move"));
    }
    let tx = store
        .conn
        .transaction_with_behavior(TransactionBehavior::Immediate)?;
    let existing: Option<(String,String,String,String,Option<String>)> = tx.query_row(
        "SELECT actor,from_path,to_path,state,receipt FROM markdown_folder_commands WHERE command_id=?1",
        [id], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?))
    ).optional()?;
    if let Some((a, f, t, state, receipt)) = existing {
        if a != actor || f != mv.from || t != mv.to {
            return Err(AuthorityError::ImmutableConflict);
        }
        return if state == "accepted" {
            Ok(Some(
                serde_json::from_str(
                    &receipt.ok_or(AuthorityError::Invalid("missing folder receipt"))?,
                )
                .map_err(|_| AuthorityError::Invalid("invalid folder receipt"))?,
            ))
        } else {
            Ok(None)
        };
    }
    // Do not race an unresolved Markdown write or another folder rename.
    let busy: i64 = tx.query_row("SELECT (SELECT COUNT(*) FROM markdown_command_log WHERE state='pending') + (SELECT COUNT(*) FROM markdown_folder_commands WHERE state='pending') + (SELECT COUNT(*) FROM markdown_folder_creates WHERE state='pending')",[],|r|r.get(0))?;
    if busy != 0 {
        return Err(AuthorityError::StaleRevision);
    }
    tx.execute("INSERT INTO markdown_folder_commands(command_id,actor,from_path,to_path,state) VALUES(?1,?2,?3,?4,'pending')",
        params![id,actor,mv.from,mv.to])?;
    tx.commit()?;
    Ok(None)
}
pub fn publish(ops: &KnowledgeOperations, mv: &FolderMove) -> Result<(), AuthorityError> {
    // Recovery of an atomic rename is unambiguous only when the source is gone.
    // Ambiguous external interventions are rejected without clobbering files.
    let source = ops
        .vault()
        .folder_exists(&mv.from)
        .map_err(|_| AuthorityError::ImmutableConflict)?;
    let destination = ops
        .vault()
        .folder_exists(&mv.to)
        .map_err(|_| AuthorityError::ImmutableConflict)?;
    if !source && destination {
        return Ok(());
    }
    if !source || destination {
        return Err(AuthorityError::ImmutableConflict);
    }
    ops.vault()
        .move_folder(&mv.from, &mv.to)
        .map_err(|_| AuthorityError::ImmutableConflict)
}
pub(crate) fn accept(
    store: &mut AuthorityStore,
    id: &str,
    mv: &FolderMove,
    notes: &[crate::knowledge_service::KnowledgeDocumentInfo],
) -> Result<FolderReceipt, AuthorityError> {
    let tx = store
        .conn
        .transaction_with_behavior(TransactionBehavior::Immediate)?;
    // The folder command may change every descendant path; rebuild the index
    // in the same transaction that records the receipt.
    tx.execute("DELETE FROM markdown_note_index", [])?;
    for note in notes {
        tx.execute(
            "INSERT INTO markdown_note_index(id,path,title) VALUES(?1,?2,?3)",
            params![note.id, note.path, note.title],
        )?;
    }
    let receipt = FolderReceipt {
        command_id: id.to_owned(),
        from: mv.from.clone(),
        to: mv.to.clone(),
    };
    let n=tx.execute("UPDATE markdown_folder_commands SET state='accepted',receipt=?2 WHERE command_id=?1 AND state='pending'",
        params![id,serde_json::to_string(&receipt).map_err(|_|AuthorityError::Invalid("folder receipt"))?])?;
    if n != 1 {
        return Err(AuthorityError::ImmutableConflict);
    }
    tx.commit()?;
    Ok(receipt)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::AuthoritativeHost;

    #[test]
    fn folder_rename_updates_paths_and_returns_repeat_receipt() {
        let mut host = AuthoritativeHost::open(":memory:").unwrap();
        host.knowledge_vault.vault().create_folder("Work").unwrap();
        host.knowledge_vault
            .vault()
            .create_folder("Archive")
            .unwrap();
        host.execute_markdown_command(
            "local",
            "create-note-a",
            crate::markdown_commands::MarkdownCommand::Create {
                note_id: "note-a".into(),
                path: "Work/a.md".into(),
                body: "# A".into(),
            },
        )
        .unwrap();
        let mv = FolderMove {
            from: "Work".into(),
            to: "ArchivedWork".into(),
        };
        let r = host
            .execute_folder_move("local", "move-folder-1", mv.clone())
            .unwrap();
        assert_eq!(
            r,
            host.execute_folder_move("local", "move-folder-1", mv.clone())
                .unwrap()
        );
        let indexed = host.indexed_markdown_knowledge().unwrap();
        assert!(
            indexed
                .iter()
                .any(|n| n.id == "note-a" && n.path == "ArchivedWork/a.md")
        );
        assert!(
            host.execute_folder_move(
                "local",
                "move-folder-1",
                FolderMove {
                    from: "Archive".into(),
                    to: "Another".into()
                }
            )
            .is_err()
        );
    }

    #[test]
    fn folder_move_replays_post_rename_crash() {
        let mut host = AuthoritativeHost::open(":memory:").unwrap();
        host.knowledge_vault.vault().create_folder("Work").unwrap();
        let mv = FolderMove {
            from: "Work".into(),
            to: "NewWork".into(),
        };
        enqueue(&mut host.authority, "local", "move-1", &mv).unwrap();
        publish(&host.knowledge_vault, &mv).unwrap();
        host.recover_folder_commands().unwrap();
        assert_eq!(pending(&host.authority).unwrap().len(), 0);
        assert!(
            host.knowledge_vault
                .vault()
                .root_path()
                .join("NewWork")
                .is_dir()
        );
    }
}

/// A durable create command is distinct from rename because it does not alter
/// note paths or revisions. Empty folders survive note deletion and must be
/// visible to desktop/mobile/web explorers.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FolderCreate {
    pub path: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FolderCreateReceipt {
    pub command_id: String,
    pub path: String,
}

pub(crate) fn create_schema(store: &AuthorityStore) -> Result<(), AuthorityError> {
    store.conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS markdown_folder_creates (
        command_id TEXT PRIMARY KEY NOT NULL,
        actor TEXT NOT NULL,
        path TEXT NOT NULL,
        state TEXT NOT NULL CHECK(state IN ('pending','accepted')),
        receipt TEXT
    );",
    )?;
    Ok(())
}

pub(crate) fn pending_creates(
    store: &AuthorityStore,
) -> Result<Vec<(String, FolderCreate)>, AuthorityError> {
    let mut stmt = store.conn.prepare(
        "SELECT command_id,path FROM markdown_folder_creates WHERE state='pending' ORDER BY rowid",
    )?;
    let result = stmt.query_map([], |r| Ok((r.get(0)?, FolderCreate { path: r.get(1)? })))?;
    Ok(result.collect::<Result<Vec<_>, _>>()?)
}

pub(crate) fn enqueue_create(
    store: &mut AuthorityStore,
    actor: &str,
    id: &str,
    cmd: &FolderCreate,
) -> Result<Option<FolderCreateReceipt>, AuthorityError> {
    if !valid_id(actor) || !valid_id(id) || !valid_folder(&cmd.path) {
        return Err(AuthorityError::Invalid("invalid folder creation"));
    }
    let tx = store
        .conn
        .transaction_with_behavior(TransactionBehavior::Immediate)?;
    let previous: Option<(String, String, String, Option<String>)> = tx
        .query_row(
            "SELECT actor,path,state,receipt FROM markdown_folder_creates WHERE command_id=?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .optional()?;
    if let Some((a, path, state, receipt)) = previous {
        if a != actor || path != cmd.path {
            return Err(AuthorityError::ImmutableConflict);
        }
        return if state == "accepted" {
            Ok(Some(
                serde_json::from_str(
                    &receipt.ok_or(AuthorityError::Invalid("missing folder receipt"))?,
                )
                .map_err(|_| AuthorityError::Invalid("invalid folder receipt"))?,
            ))
        } else {
            Ok(None)
        };
    }
    // Serialize all physical namespace changes, including cross-command races.
    let busy: i64 = tx.query_row(
        "SELECT
        (SELECT COUNT(*) FROM markdown_command_log WHERE state='pending') +
        (SELECT COUNT(*) FROM markdown_folder_commands WHERE state='pending') +
        (SELECT COUNT(*) FROM markdown_folder_creates WHERE state='pending')",
        [],
        |r| r.get(0),
    )?;
    if busy != 0 {
        return Err(AuthorityError::StaleRevision);
    }
    tx.execute("INSERT INTO markdown_folder_creates(command_id,actor,path,state) VALUES(?1,?2,?3,'pending')",
        params![id,actor,cmd.path])?;
    tx.commit()?;
    Ok(None)
}

pub fn publish_create(ops: &KnowledgeOperations, cmd: &FolderCreate) -> Result<(), AuthorityError> {
    // Replaying a completed mkdir is safe: the folder must still be a real
    // folder and cannot have been replaced by a symlink or file.
    if ops
        .vault()
        .folder_exists(&cmd.path)
        .map_err(|_| AuthorityError::ImmutableConflict)?
    {
        return Ok(());
    }
    ops.vault()
        .create_folder(&cmd.path)
        .map_err(|_| AuthorityError::ImmutableConflict)
}

pub(crate) fn accept_create(
    store: &mut AuthorityStore,
    id: &str,
    cmd: &FolderCreate,
) -> Result<FolderCreateReceipt, AuthorityError> {
    let receipt = FolderCreateReceipt {
        command_id: id.into(),
        path: cmd.path.clone(),
    };
    let tx = store
        .conn
        .transaction_with_behavior(TransactionBehavior::Immediate)?;
    let n = tx.execute(
        "UPDATE markdown_folder_creates SET state='accepted',receipt=?2
        WHERE command_id=?1 AND state='pending'",
        params![
            id,
            serde_json::to_string(&receipt)
                .map_err(|_| AuthorityError::Invalid("folder receipt"))?
        ],
    )?;
    if n != 1 {
        return Err(AuthorityError::ImmutableConflict);
    }
    tx.commit()?;
    Ok(receipt)
}

#[cfg(test)]
mod folder_creation_acceptance_tests {
    use super::*;
    use crate::AuthoritativeHost;
    #[test]
    fn empty_folder_is_visible_and_idempotent() {
        let mut host = AuthoritativeHost::open(":memory:").unwrap();
        let cmd = FolderCreate {
            path: "Work".into(),
        };
        let receipt = host
            .execute_folder_create("local", "mkdir-1", cmd.clone())
            .unwrap();
        assert_eq!(
            receipt,
            host.execute_folder_create("local", "mkdir-1", cmd).unwrap()
        );
        let tree = serde_json::to_string(&host.knowledge_tree().unwrap()).unwrap();
        assert!(tree.contains("Work"));
        assert!(
            host.execute_folder_create(
                "local",
                "mkdir-1",
                FolderCreate {
                    path: "Different".into()
                }
            )
            .is_err()
        );
    }
    #[test]
    fn interrupted_creation_is_replayed_before_new_commands() {
        let mut host = AuthoritativeHost::open(":memory:").unwrap();
        let cmd = FolderCreate {
            path: "Journal".into(),
        };
        enqueue_create(&mut host.authority, "owner", "mkdir-2", &cmd).unwrap();
        publish_create(&host.knowledge_vault, &cmd).unwrap();
        host.recover_folder_creates().unwrap();
        assert!(pending_creates(&host.authority).unwrap().is_empty());
        assert_eq!(
            host.execute_folder_create("owner", "mkdir-2", cmd)
                .unwrap()
                .path,
            "Journal"
        );
    }
}
