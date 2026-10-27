//! Authoritative, replayable physical folder renames. A folder move is a single
//! filesystem rename, followed by a transactional SQLite path-index update and
//! command receipt. Pending operations are replayed before serving requests.
//!
//! A pending row is a promise to finish, not a promise to insist. When
//! publication is refused by a precondition this process can see (a missing
//! parent, a taken destination, a missing source), the row is deleted in the
//! same flow: the intent provably never happened, and keeping it would make
//! recovery refuse every later knowledge command with a conflict the caller
//! cannot act on. The cost is the intent itself — a person re-issues the command
//! under a new id — and the win is that one bad name no longer wedges the
//! workspace until the database is rebuilt.
use crate::{
    knowledge_files::VaultError,
    knowledge_operations::KnowledgeOperations,
    store::{AuthorityError, AuthorityStore},
};
use rusqlite::{OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};

/// Fixed refusal text for a folder create whose parent folder is absent.
pub const FOLDER_PARENT_MISSING: &str = "folder parent does not exist";
/// Fixed refusal text for a create or move whose target name is taken.
pub const FOLDER_DESTINATION_EXISTS: &str = "folder destination already exists";
/// Fixed refusal text for a move whose source folder is gone.
pub const FOLDER_SOURCE_MISSING: &str = "folder does not exist";

/// Outcome of trying to publish one pending folder intent to the vault.
///
/// The distinction is the whole point of this type. `Refused` means *this
/// process looked at the vault and proved that the intent cannot be applied*:
/// the parent is missing, the destination name is taken, the source is gone. In
/// that state nothing was written, so the durable pending row is worthless — it
/// records an intention that can never become true, and replaying it before
/// every later command (which is what recovery does) would refuse those later
/// commands forever. `Ambiguous` means the vault could not be read, or what it
/// shows cannot be explained by this intent (an I/O failure, a symlink or a
/// foreign file where a folder should be). Those stay pending: a crash may have
/// published the work, so only a later observation may accept or discard them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FolderPublishOutcome {
    /// Definite, in-process refusal. Carries the human text the transport
    /// returns verbatim, so a caller can fix its own command.
    Refused(&'static str),
    /// Unknown or externally caused state. The intent must remain pending.
    Ambiguous,
}

impl From<FolderPublishOutcome> for AuthorityError {
    /// Both halves are refusals to the caller. `Refused` is the caller's own
    /// mistake, so it becomes the validation error the transport already quotes
    /// as a 400 with its fixed wording. `Ambiguous` keeps the existing generic
    /// conflict: the workspace, not the command, is what failed, and the request
    /// may succeed once the external state is resolved.
    fn from(outcome: FolderPublishOutcome) -> Self {
        match outcome {
            FolderPublishOutcome::Refused(message) => AuthorityError::Invalid(message),
            FolderPublishOutcome::Ambiguous => AuthorityError::ImmutableConflict,
        }
    }
}

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
/// Publish one pending folder rename to the vault.
///
/// The return value tells the caller whether the intent is *spent* (accepted, or
/// provably impossible) or must be retried later. It never decides that on its
/// own: the caller owns the durable row, and only the caller knows whether a
/// refusal means "drop the reservation" or "keep waiting".
pub fn publish(ops: &KnowledgeOperations, mv: &FolderMove) -> Result<(), FolderPublishOutcome> {
    // Recovery of an atomic rename is unambiguous only when the source is gone.
    // Ambiguous external interventions are rejected without clobbering files.
    let source = ops
        .vault()
        .folder_exists(&mv.from)
        .map_err(|_| FolderPublishOutcome::Ambiguous)?;
    let destination = ops
        .vault()
        .folder_exists(&mv.to)
        .map_err(|_| FolderPublishOutcome::Ambiguous)?;
    // A rename is atomic, so an absent source next to a present destination can
    // only be this rename already having landed before its receipt. Accept it:
    // that is the crash window between publication and acceptance.
    if !source && destination {
        return Ok(());
    }
    if !source {
        return Err(FolderPublishOutcome::Refused(FOLDER_SOURCE_MISSING));
    }
    if destination {
        // Both names exist. Nothing was written, and the caller can pick another
        // name, so this is a validation refusal rather than a conflict: an
        // atomic rename can never leave both names in place.
        return Err(FolderPublishOutcome::Refused(FOLDER_DESTINATION_EXISTS));
    }
    ops.vault()
        .move_folder(&mv.from, &mv.to)
        .map_err(|error| match error {
            // The two paths were just observed in the opposite state, so the rename
            // itself raced a foreign writer. Report what the filesystem says now;
            // neither variant can have moved the folder, because `rename` is atomic
            // and these are its two refusal kinds.
            VaultError::NotFound => FolderPublishOutcome::Refused(FOLDER_SOURCE_MISSING),
            VaultError::AlreadyExists => FolderPublishOutcome::Refused(FOLDER_DESTINATION_EXISTS),
            // I/O failures and unreadable/symlinked paths may have partially landed,
            // so the intent stays pending for a later reconciliation.
            _ => FolderPublishOutcome::Ambiguous,
        })
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

/// Release a pending folder-move reservation after a publication that definitely
/// did not happen.
///
/// Only `pending` rows are removed: an accepted row *is* the durable receipt and
/// must never disappear. The lost information is the intent itself — a person
/// must re-issue the move as a new command id. That is cheaper than the
/// alternative, which is a row that can never be satisfied and that recovery
/// replays (and fails on) before every later knowledge write.
pub(crate) fn abandon_move(
    store: &mut AuthorityStore,
    command_id: &str,
) -> Result<(), AuthorityError> {
    store.conn.execute(
        "DELETE FROM markdown_folder_commands WHERE command_id=?1 AND state='pending'",
        [command_id],
    )?;
    Ok(())
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

/// Publish one pending folder create to the vault.
///
/// See [`publish`]: `Refused` means the intent is spent and provably did nothing
/// (its parent is missing, or its name is held by something else), so the caller
/// must release the reservation; `Ambiguous` means the create may already exist
/// and the intent must survive so recovery can accept it later.
pub fn publish_create(
    ops: &KnowledgeOperations,
    cmd: &FolderCreate,
) -> Result<(), FolderPublishOutcome> {
    // Replaying a completed mkdir is safe: the folder must still be a real
    // folder and cannot have been replaced by a symlink or file.
    match ops.vault().folder_exists(&cmd.path) {
        Ok(true) => return Ok(()),
        Ok(false) => {}
        // A file, a symlink or an unreadable entry sits on the requested path.
        // This process cannot tell whether the create landed, so it must not
        // claim the intent is spent.
        Err(_) => return Err(FolderPublishOutcome::Ambiguous),
    }
    match ops.vault().create_folder(&cmd.path) {
        Ok(()) => Ok(()),
        // `folder_exists` just proved the leaf is absent, so the only possible
        // missing piece is an ancestor. Nothing was created: the caller named a
        // parent it never made, and can retry under a parent that exists.
        Err(VaultError::NotFound) => Err(FolderPublishOutcome::Refused(FOLDER_PARENT_MISSING)),
        // The leaf appeared between the check and the mkdir. If it is the folder
        // the caller asked for, the intent is satisfied; anything else (a file,
        // a symlink) is a foreign replacement this intent did not create.
        Err(VaultError::AlreadyExists) => match ops.vault().folder_exists(&cmd.path) {
            Ok(true) => Ok(()),
            _ => Err(FolderPublishOutcome::Refused(FOLDER_DESTINATION_EXISTS)),
        },
        // The parent exists but is not a real folder (a plain file, or a symlink
        // the vault refuses to follow), so there is no folder to hold the new
        // one. Honest and fixable by the caller, like a missing parent.
        Err(VaultError::InvalidPath) => Err(FolderPublishOutcome::Refused(FOLDER_PARENT_MISSING)),
        // I/O failures may have published the directory, so the intent stays
        // pending until a later observation can decide.
        Err(_) => Err(FolderPublishOutcome::Ambiguous),
    }
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

/// Release a pending folder-create reservation after a publication that
/// definitely did not happen. See [`abandon_move`] for why discarding the
/// intent is safe and what it costs.
pub(crate) fn abandon_create(
    store: &mut AuthorityStore,
    command_id: &str,
) -> Result<(), AuthorityError> {
    store.conn.execute(
        "DELETE FROM markdown_folder_creates WHERE command_id=?1 AND state='pending'",
        [command_id],
    )?;
    Ok(())
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

#[cfg(test)]
mod folder_refusal_tests {
    use super::*;
    use crate::AuthoritativeHost;
    use crate::markdown_commands::MarkdownCommand;

    /// Assert a refusal is the caller-fixable, fixed-text kind the transport
    /// answers with 400, and return its message.
    fn refusal<T: std::fmt::Debug>(result: Result<T, AuthorityError>) -> &'static str {
        match result {
            Err(AuthorityError::Invalid(message)) => message,
            other => panic!("expected a validation refusal, got {other:?}"),
        }
    }

    fn temporary_root(tag: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "orbit-folder-refusal-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    /// The reported defect: a create under a parent that does not exist used to
    /// leave its intent pending, and every later knowledge write on that server
    /// was refused by replay. It must now be a plain refusal with a name the
    /// browser can quote, and the queue must stay usable.
    #[test]
    fn refused_create_does_not_poison_later_knowledge_writes() {
        let mut host = AuthoritativeHost::open(":memory:").unwrap();
        assert_eq!(
            refusal(host.execute_folder_create(
                "local",
                "mkdir-ghost",
                FolderCreate {
                    path: "ghost/child".into()
                }
            )),
            FOLDER_PARENT_MISSING
        );
        assert!(pending_creates(&host.authority).unwrap().is_empty());
        assert!(pending(&host.authority).unwrap().is_empty());

        // The next, unrelated create succeeds on the same server.
        host.execute_folder_create(
            "local",
            "mkdir-alpha",
            FolderCreate {
                path: "Alpha".into(),
            },
        )
        .unwrap();
        // And so does a note write, which replays the folder queues first.
        host.execute_markdown_command(
            "local",
            "note-1",
            MarkdownCommand::Create {
                note_id: "note-a".into(),
                path: "Alpha/a.md".into(),
                body: "# A".into(),
            },
        )
        .unwrap();
        assert!(
            host.knowledge_vault
                .vault()
                .root_path()
                .join("Alpha/a.md")
                .is_file()
        );
    }

    /// The same defect for renames: a destination that already exists is the
    /// caller's choice, not a workspace conflict, and must not wedge the queue.
    #[test]
    fn refused_move_does_not_poison_later_knowledge_writes() {
        let mut host = AuthoritativeHost::open(":memory:").unwrap();
        host.knowledge_vault.vault().create_folder("Work").unwrap();
        host.knowledge_vault
            .vault()
            .create_folder("Archive")
            .unwrap();
        host.execute_markdown_command(
            "local",
            "note-1",
            MarkdownCommand::Create {
                note_id: "note-a".into(),
                path: "Work/a.md".into(),
                body: "# A".into(),
            },
        )
        .unwrap();
        assert_eq!(
            refusal(host.execute_folder_move(
                "local",
                "move-work-archive",
                FolderMove {
                    from: "Work".into(),
                    to: "Archive".into()
                }
            )),
            FOLDER_DESTINATION_EXISTS
        );
        assert!(pending(&host.authority).unwrap().is_empty());
        // Nothing moved, and the index still names the real path.
        assert!(
            host.indexed_markdown_knowledge()
                .unwrap()
                .iter()
                .any(|n| n.id == "note-a" && n.path == "Work/a.md")
        );

        // A later, valid move on the same server succeeds and keeps the note.
        host.execute_folder_move(
            "local",
            "move-work-done",
            FolderMove {
                from: "Work".into(),
                to: "Done".into(),
            },
        )
        .unwrap();
        assert!(
            host.indexed_markdown_knowledge()
                .unwrap()
                .iter()
                .any(|n| n.id == "note-a" && n.path == "Done/a.md")
        );
        host.execute_folder_create(
            "local",
            "mkdir-later",
            FolderCreate {
                path: "Later".into(),
            },
        )
        .unwrap();
    }

    /// A refused move whose source is gone names the source, not a conflict.
    #[test]
    fn move_from_a_missing_source_says_the_folder_does_not_exist() {
        let mut host = AuthoritativeHost::open(":memory:").unwrap();
        assert_eq!(
            refusal(host.execute_folder_move(
                "local",
                "move-missing",
                FolderMove {
                    from: "Ghost".into(),
                    to: "Ghost2".into()
                }
            )),
            FOLDER_SOURCE_MISSING
        );
        assert!(pending(&host.authority).unwrap().is_empty());
        host.execute_folder_create(
            "local",
            "mkdir-after-missing",
            FolderCreate {
                path: "Kept".into(),
            },
        )
        .unwrap();
    }

    /// Both refusals must survive a close/open cycle of the same database: a
    /// restart used to replay the pending row and fail on open forever.
    #[test]
    fn folder_refusals_do_not_survive_a_reopen_as_pending_work() {
        let root = temporary_root("reopen");
        std::fs::create_dir_all(&root).unwrap();
        let database = root.join("authority.sqlite");
        {
            let mut host = AuthoritativeHost::open(&database).unwrap();
            assert_eq!(
                refusal(host.execute_folder_create(
                    "local",
                    "mkdir-ghost",
                    FolderCreate {
                        path: "ghost/child".into()
                    }
                )),
                FOLDER_PARENT_MISSING
            );
            host.knowledge_vault.vault().create_folder("Work").unwrap();
            host.knowledge_vault
                .vault()
                .create_folder("Archive")
                .unwrap();
            assert_eq!(
                refusal(host.execute_folder_move(
                    "local",
                    "move-work-archive",
                    FolderMove {
                        from: "Work".into(),
                        to: "Archive".into()
                    }
                )),
                FOLDER_DESTINATION_EXISTS
            );
        }
        {
            // Opening replays both queues; neither may refuse the workspace.
            let mut host = AuthoritativeHost::open(&database).unwrap();
            assert!(pending_creates(&host.authority).unwrap().is_empty());
            assert!(pending(&host.authority).unwrap().is_empty());
            host.execute_folder_create(
                "local",
                "mkdir-beta",
                FolderCreate {
                    path: "Beta".into(),
                },
            )
            .unwrap();
            host.execute_folder_move(
                "local",
                "move-work-work2",
                FolderMove {
                    from: "Work".into(),
                    to: "Work2".into(),
                },
            )
            .unwrap();
        }
        // The refusals were refusals, not partial work: nothing was created or
        // moved behind the caller's back.
        let vault =
            crate::knowledge_files::MarkdownVault::open(root.join("authority.knowledge")).unwrap();
        assert!(vault.folder_exists("Beta").unwrap());
        assert!(vault.folder_exists("Work2").unwrap());
        assert!(!vault.folder_exists("ghost").unwrap());
        assert!(vault.folder_exists("Archive").unwrap());
        std::fs::remove_dir_all(root).unwrap();
    }

    /// Recovery must not abort every caller for one impossible intent, and must
    /// still accept an intent whose work landed before the crash.
    #[test]
    fn recovery_abandons_refused_intents_and_accepts_published_ones() {
        let mut host = AuthoritativeHost::open(":memory:").unwrap();
        // An intent that can never be satisfied: its parent does not exist.
        let impossible = FolderCreate {
            path: "ghost/child".into(),
        };
        enqueue_create(
            &mut host.authority,
            "owner",
            "mkdir-impossible",
            &impossible,
        )
        .unwrap();
        host.recover_folder_creates().unwrap();
        assert!(pending_creates(&host.authority).unwrap().is_empty());

        // An intent whose folder was made before the crash is accepted, not
        // abandoned: the receipt must survive the interruption.
        let landed = FolderCreate {
            path: "Journal".into(),
        };
        enqueue_create(&mut host.authority, "owner", "mkdir-landed", &landed).unwrap();
        host.knowledge_vault
            .vault()
            .create_folder("Journal")
            .unwrap();
        host.recover_folder_creates().unwrap();
        assert!(pending_creates(&host.authority).unwrap().is_empty());
        assert_eq!(
            host.execute_folder_create("owner", "mkdir-landed", landed)
                .unwrap()
                .path,
            "Journal"
        );

        // Renames behave the same way: a move onto a taken name is dropped, a
        // move that landed is accepted with its receipt.
        host.knowledge_vault.vault().create_folder("One").unwrap();
        host.knowledge_vault.vault().create_folder("Two").unwrap();
        let blocked = FolderMove {
            from: "One".into(),
            to: "Two".into(),
        };
        enqueue(&mut host.authority, "owner", "move-blocked", &blocked).unwrap();
        host.recover_folder_commands().unwrap();
        assert!(pending(&host.authority).unwrap().is_empty());

        let moved = FolderMove {
            from: "Two".into(),
            to: "Three".into(),
        };
        enqueue(&mut host.authority, "owner", "move-landed", &moved).unwrap();
        host.knowledge_vault
            .vault()
            .move_folder("Two", "Three")
            .unwrap();
        host.recover_folder_commands().unwrap();
        assert!(pending(&host.authority).unwrap().is_empty());
        assert_eq!(
            host.execute_folder_move("owner", "move-landed", moved)
                .unwrap()
                .command_id,
            "move-landed"
        );
        // And the workspace is still writable afterwards.
        host.execute_folder_create(
            "owner",
            "mkdir-after-recovery",
            FolderCreate {
                path: "AfterRecovery".into(),
            },
        )
        .unwrap();
    }

    /// A grammar or subtree refusal is decided before the intent is durable, so
    /// it can neither poison the queue nor be mistaken for a vault conflict.
    #[test]
    fn invalid_folder_moves_fail_before_enqueue() {
        let mut host = AuthoritativeHost::open(":memory:").unwrap();
        host.knowledge_vault.vault().create_folder("notes").unwrap();
        host.knowledge_vault
            .vault()
            .create_folder("notes/research")
            .unwrap();
        for (id, mv) in [
            (
                "move-into-subtree",
                FolderMove {
                    from: "notes".into(),
                    to: "notes/research/notes".into(),
                },
            ),
            (
                "move-to-itself",
                FolderMove {
                    from: "notes".into(),
                    to: "notes".into(),
                },
            ),
            (
                "move-with-traversal",
                FolderMove {
                    from: "notes".into(),
                    to: "../escape".into(),
                },
            ),
        ] {
            assert_eq!(
                refusal(host.execute_folder_move("local", id, mv)),
                "invalid folder move"
            );
        }
        assert!(pending(&host.authority).unwrap().is_empty());
        host.execute_folder_create(
            "local",
            "mkdir-after-invalid",
            FolderCreate {
                path: "AfterInvalid".into(),
            },
        )
        .unwrap();
    }

    /// An accepted command still answers its identical replay with the same
    /// receipt, and a reused id with a different payload is still a conflict.
    #[test]
    fn accepted_folder_commands_keep_their_receipts() {
        let mut host = AuthoritativeHost::open(":memory:").unwrap();
        let create = FolderCreate {
            path: "Receipts".into(),
        };
        let receipt = host
            .execute_folder_create("local", "mkdir-receipt", create.clone())
            .unwrap();
        assert_eq!(
            host.execute_folder_create("local", "mkdir-receipt", create)
                .unwrap(),
            receipt
        );
        match host.execute_folder_create(
            "local",
            "mkdir-receipt",
            FolderCreate {
                path: "Other".into(),
            },
        ) {
            Err(AuthorityError::ImmutableConflict) => {}
            other => panic!("expected an immutable conflict, got {other:?}"),
        }

        host.knowledge_vault.vault().create_folder("From").unwrap();
        let mv = FolderMove {
            from: "From".into(),
            to: "To".into(),
        };
        let receipt = host
            .execute_folder_move("local", "move-receipt", mv.clone())
            .unwrap();
        assert_eq!(
            host.execute_folder_move("local", "move-receipt", mv)
                .unwrap(),
            receipt
        );
        match host.execute_folder_move(
            "local",
            "move-receipt",
            FolderMove {
                from: "To".into(),
                to: "Elsewhere".into(),
            },
        ) {
            Err(AuthorityError::ImmutableConflict) => {}
            other => panic!("expected an immutable conflict, got {other:?}"),
        }
    }
}
