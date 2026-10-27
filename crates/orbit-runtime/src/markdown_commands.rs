//! Coordinated SQLite intent + Markdown publication, with durable replay.
//! The host is the only writer; file-backed content, not SQLite, owns note bodies.
use crate::{
    knowledge_service::KnowledgeService,
    store::{AuthorityError, AuthorityStore},
};
use rusqlite::{OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", deny_unknown_fields)]
pub enum MarkdownCommand {
    Create {
        note_id: String,
        path: String,
        body: String,
    },
    Update {
        note_id: String,
        expected: String,
        replacement: String,
    },
    Move {
        note_id: String,
        from: String,
        to: String,
        expected: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MarkdownReceipt {
    pub command_id: String,
    pub note_id: String,
    pub revision: i64,
}

/// Reject permanently unreplayable commands before persisting an intent.
/// Path and frontmatter checks are duplicated at the vault boundary as a
/// second line of defense against traversal or external modifications.
fn valid_path(value: &str) -> bool {
    if value.is_empty()
        || value.contains('\\')
        || value.contains('\0')
        || !value.to_ascii_lowercase().ends_with(".md")
    {
        return false;
    }
    value
        .split('/')
        .all(|segment| !segment.is_empty() && !segment.starts_with('.') && segment != "..")
}

/// Accept full portable Markdown (including frontmatter) or a plain body.
/// The stable ID must match the command and is never inferred from a path.
fn created_document(note_id: &str, body: &str) -> Result<String, AuthorityError> {
    if body.starts_with("---\n") {
        if KnowledgeService::identity(body).ok().as_deref() != Some(note_id) {
            return Err(AuthorityError::Invalid("Markdown frontmatter ID mismatch"));
        }
        Ok(body.to_owned())
    } else {
        Ok(format!("---\norbit_id: {note_id}\n---\n{body}"))
    }
}

fn validate_command(command: &MarkdownCommand) -> Result<(), AuthorityError> {
    use crate::knowledge_service::KnowledgeService;
    match command {
        MarkdownCommand::Create {
            note_id,
            path,
            body,
        } => {
            if !safe_id(note_id) || !valid_path(path) {
                return Err(AuthorityError::Invalid("invalid markdown create"));
            }
            created_document(note_id, body)?;
        }
        MarkdownCommand::Update {
            note_id,
            expected,
            replacement,
        } => {
            if !safe_id(note_id)
                || KnowledgeService::identity(expected).ok().as_deref() != Some(note_id.as_str())
                || KnowledgeService::identity(replacement).ok().as_deref() != Some(note_id.as_str())
            {
                return Err(AuthorityError::Invalid("invalid markdown update identity"));
            }
        }
        MarkdownCommand::Move {
            note_id,
            from,
            to,
            expected,
        } => {
            if !safe_id(note_id)
                || !valid_path(from)
                || !valid_path(to)
                || from == to
                || KnowledgeService::identity(expected).ok().as_deref() != Some(note_id.as_str())
            {
                return Err(AuthorityError::Invalid("invalid markdown move"));
            }
        }
    }
    Ok(())
}

pub(crate) fn schema(store: &AuthorityStore) -> Result<(), AuthorityError> {
    store.conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS markdown_command_log (
        command_id TEXT PRIMARY KEY NOT NULL,
        actor TEXT NOT NULL,
        payload TEXT NOT NULL,
        state TEXT NOT NULL CHECK(state IN ('pending', 'accepted')),
        receipt TEXT
    );
    CREATE TABLE IF NOT EXISTS markdown_note_revisions (
        note_id TEXT PRIMARY KEY NOT NULL,
        revision INTEGER NOT NULL CHECK(revision > 0)
    );",
    )?;
    Ok(())
}

pub(crate) fn safe_id(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 128
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// One command id names exactly one intent in every durable ledger, so the
/// identity rule lives here rather than being restated by each command type.
pub(crate) fn valid_command_id(command_id: &str) -> bool {
    // `update-` is reserved for the internally derived Markdown updates, which
    // callers must never be able to collide with.
    safe_id(command_id) && !command_id.starts_with("update-")
}

pub(crate) fn enqueue(
    store: &mut AuthorityStore,
    actor: &str,
    command_id: &str,
    command: &MarkdownCommand,
) -> Result<Option<MarkdownReceipt>, AuthorityError> {
    if !safe_id(actor) || !valid_command_id(command_id) {
        return Err(AuthorityError::Invalid("invalid markdown command identity"));
    }
    validate_command(command)?;
    let payload = serde_json::to_string(command)
        .map_err(|_| AuthorityError::Invalid("invalid markdown payload"))?;
    // Never accept a note write while a folder rename is awaiting reconciliation.
    let moving: i64 = store.conn.query_row("SELECT (SELECT COUNT(*) FROM markdown_folder_commands WHERE state='pending') + (SELECT COUNT(*) FROM markdown_folder_creates WHERE state='pending')", [], |r|r.get(0))?;
    if moving != 0 {
        return Err(AuthorityError::StaleRevision);
    }
    let tx = store
        .conn
        .transaction_with_behavior(TransactionBehavior::Immediate)?;
    let prior: Option<(String, String, String, Option<String>)> = tx
        .query_row(
            "SELECT actor,payload,state,receipt FROM markdown_command_log WHERE command_id=?1",
            [command_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .optional()?;
    if let Some((old_actor, old_payload, state, receipt)) = prior {
        if old_actor != actor || old_payload != payload {
            return Err(AuthorityError::ImmutableConflict);
        }
        if state == "accepted" {
            let receipt: MarkdownReceipt =
                serde_json::from_str(&receipt.ok_or(AuthorityError::Invalid("missing receipt"))?)
                    .map_err(|_| AuthorityError::Invalid("invalid receipt"))?;
            return Ok(Some(receipt));
        }
        return Ok(None);
    }
    // A pending command is a durable reservation for this note. No second
    // command may bypass it and race on the same document before recovery.
    let note_id = match command {
        MarkdownCommand::Create { note_id, .. }
        | MarkdownCommand::Update { note_id, .. }
        | MarkdownCommand::Move { note_id, .. } => note_id,
    };
    if !safe_id(note_id) {
        return Err(AuthorityError::Invalid("invalid note identity"));
    }
    let pending = {
        let mut statement =
            tx.prepare("SELECT payload FROM markdown_command_log WHERE state='pending'")?;
        statement
            .query_map([], |r| r.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?
    };
    for value in pending {
        let other: MarkdownCommand = serde_json::from_str(&value)
            .map_err(|_| AuthorityError::Invalid("corrupt pending command"))?;
        let other_id = match &other {
            MarkdownCommand::Create { note_id, .. }
            | MarkdownCommand::Update { note_id, .. }
            | MarkdownCommand::Move { note_id, .. } => note_id,
        };
        if other_id == note_id {
            return Err(AuthorityError::StaleRevision);
        }
    }
    tx.execute("INSERT INTO markdown_command_log(command_id,actor,payload,state) VALUES(?1,?2,?3,'pending')",
        params![command_id,actor,payload])?;
    tx.commit()?;
    Ok(None)
}

/// Release a pending note reservation after a command failed before any
/// publication. Accepted or already-replayed commands are never removed.
pub(crate) fn abandon_pending(
    store: &mut AuthorityStore,
    command_id: &str,
) -> Result<(), AuthorityError> {
    store.conn.execute(
        "DELETE FROM markdown_command_log WHERE command_id=?1 AND state='pending'",
        [command_id],
    )?;
    Ok(())
}

pub(crate) fn pending(
    store: &AuthorityStore,
) -> Result<Vec<(String, MarkdownCommand)>, AuthorityError> {
    let mut stmt = store.conn.prepare(
        "SELECT command_id,payload FROM markdown_command_log WHERE state='pending' ORDER BY rowid",
    )?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
    rows.collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .map(|(id, payload)| {
            Ok((
                id,
                serde_json::from_str(&payload)
                    .map_err(|_| AuthorityError::Invalid("invalid pending markdown command"))?,
            ))
        })
        .collect()
}

pub fn publish(
    service: &KnowledgeService<'_>,
    command_id: &str,
    command: &MarkdownCommand,
) -> Result<(), AuthorityError> {
    match command {
        MarkdownCommand::Create {
            note_id,
            path,
            body,
        } => {
            // After a crash, an already-published identical file must be
            // recognized, not treated as a second create.
            if let Some((info, existing)) = service
                .read_by_id(note_id)
                .map_err(|_| AuthorityError::Invalid("markdown identity discovery failed"))?
            {
                let desired = created_document(note_id, body)?;
                if info.path == *path && existing == desired {
                    return Ok(());
                }
                return Err(AuthorityError::ImmutableConflict);
            }
            service
                .stage_create(command_id, note_id, path, body)
                .map_err(|_| AuthorityError::Invalid("markdown publication failed"))
        }
        MarkdownCommand::Update {
            note_id,
            expected,
            replacement,
        } => {
            let (_, existing) = service
                .read_by_id(note_id)
                .map_err(|_| AuthorityError::Invalid("markdown identity discovery failed"))?
                .ok_or(AuthorityError::Invalid("markdown document missing"))?;
            if existing == *replacement {
                return Ok(());
            }
            service
                .stage_update_by_id(command_id, note_id, expected, replacement)
                .map_err(|_| AuthorityError::StaleRevision)
        }
        MarkdownCommand::Move {
            note_id,
            from,
            to,
            expected,
        } => service
            .complete_move(note_id, from, to, expected)
            .map_err(|_| AuthorityError::ImmutableConflict),
    }
}

pub(crate) fn accept(
    store: &mut AuthorityStore,
    command_id: &str,
    command: &MarkdownCommand,
    notes: &[crate::knowledge_service::KnowledgeDocumentInfo],
    observed_body: &str,
) -> Result<MarkdownReceipt, AuthorityError> {
    let note_id = match command {
        MarkdownCommand::Create { note_id, .. }
        | MarkdownCommand::Update { note_id, .. }
        | MarkdownCommand::Move { note_id, .. } => note_id,
    };
    // Never accept a receipt solely because a file with the expected ID exists.
    // Verify exact published bytes first, including frontmatter on creation.
    let expected_body = match command {
        MarkdownCommand::Create { note_id, body, .. } => created_document(note_id, body)?,
        MarkdownCommand::Update { replacement, .. } => replacement.clone(),
        MarkdownCommand::Move { expected, .. } => expected.clone(),
    };
    if observed_body != expected_body {
        return Err(AuthorityError::ImmutableConflict);
    }
    let tx = store
        .conn
        .transaction_with_behavior(TransactionBehavior::Immediate)?;
    let (state, receipt): (String, Option<String>) = tx.query_row(
        "SELECT state,receipt FROM markdown_command_log WHERE command_id=?1",
        [command_id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    if state == "accepted" {
        return serde_json::from_str(&receipt.ok_or(AuthorityError::Invalid("missing receipt"))?)
            .map_err(|_| AuthorityError::Invalid("invalid receipt"));
    }
    // A matching stable ID is not sufficient: on a create, the accepted
    // indexed path must be the path requested by the durable command.
    // Otherwise a concurrent move could acknowledge a different location.
    let indexed = notes
        .iter()
        .find(|n| n.id == *note_id)
        .ok_or(AuthorityError::Invalid("published note not indexed"))?;
    match command {
        MarkdownCommand::Create { path, .. } if indexed.path != *path => {
            return Err(AuthorityError::ImmutableConflict);
        }
        MarkdownCommand::Move { to, .. } if indexed.path != *to => {
            return Err(AuthorityError::ImmutableConflict);
        }
        _ => {}
    }
    let previous: Option<i64> = tx
        .query_row(
            "SELECT revision FROM markdown_note_revisions WHERE note_id=?1",
            [note_id],
            |r| r.get(0),
        )
        .optional()?;
    if matches!(command, MarkdownCommand::Create { .. }) && previous.is_some() {
        return Err(AuthorityError::ImmutableConflict);
    }
    if matches!(
        command,
        MarkdownCommand::Update { .. } | MarkdownCommand::Move { .. }
    ) && previous.is_none()
    {
        return Err(AuthorityError::Invalid("cannot update unknown revision"));
    }
    let revision = previous.unwrap_or(0) + 1;
    tx.execute(
        "INSERT INTO markdown_note_revisions(note_id,revision) VALUES(?1,?2)
        ON CONFLICT(note_id) DO UPDATE SET revision=excluded.revision",
        params![note_id, revision],
    )?;
    // Rebuild the *derived* projection inside the acceptance transaction.
    tx.execute("DELETE FROM markdown_note_index", [])?;
    for n in notes {
        tx.execute(
            "INSERT INTO markdown_note_index(id,path,title) VALUES(?1,?2,?3)",
            params![n.id, n.path, n.title],
        )?;
    }
    let result = MarkdownReceipt {
        command_id: command_id.into(),
        note_id: note_id.clone(),
        revision,
    };
    let encoded = serde_json::to_string(&result)
        .map_err(|_| AuthorityError::Invalid("receipt serialization failed"))?;
    tx.execute(
        "UPDATE markdown_command_log SET state='accepted',receipt=?2 WHERE command_id=?1",
        params![command_id, encoded],
    )?;
    // The older local activity-feed API and the canonical file-backed note
    // service share this one SQLite activity journal. Appending here (not in
    // a second transaction) prevents accepted Markdown changes from becoming
    // invisible to clients that poll the workspace activity cursor.
    let (kind, summary) = match command {
        MarkdownCommand::Create { .. } => {
            ("created", format!("Markdown knowledge {note_id} created"))
        }
        MarkdownCommand::Update { .. } => {
            ("updated", format!("Markdown knowledge {note_id} updated"))
        }
        MarkdownCommand::Move { .. } => ("updated", format!("Markdown knowledge {note_id} moved")),
    };
    let actor: String = tx.query_row(
        "SELECT actor FROM markdown_command_log WHERE command_id=?1",
        [command_id],
        |r| r.get(0),
    )?;
    let timestamp: i64 = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| AuthorityError::Invalid("clock"))?
        .as_millis()
        .try_into()
        .map_err(|_| AuthorityError::Invalid("clock overflow"))?;
    tx.execute("INSERT INTO activities(id,subject_kind,subject_id,activity_kind,activity_kind_value,
        origin_kind,origin_value,occurred_at_ms,summary) VALUES (?1,'knowledge',?2,?3,NULL,'external',?4,?5,?6)",
        params![format!("markdown-{command_id}"),note_id,kind,actor,timestamp,summary])?;
    tx.commit()?;
    Ok(result)
}

#[cfg(test)]
mod acceptance_guards {
    use super::*;

    /// `enqueue` checks folder-command reservations too, so tests must install
    /// the same schema set the authoritative host installs on open.
    fn schemas(store: &AuthorityStore) {
        schema(store).unwrap();
        crate::folder_commands::schema(store).unwrap();
        crate::folder_commands::create_schema(store).unwrap();
    }
    use std::path::Path;

    #[test]
    fn never_accept_unverified_markdown_bytes() {
        let mut store = AuthorityStore::open(Path::new(":memory:")).unwrap();
        schemas(&store);
        let command = MarkdownCommand::Create {
            note_id: "note-one".into(),
            path: "note.md".into(),
            body: "# correct".into(),
        };
        assert!(
            enqueue(&mut store, "owner", "cmd-one", &command)
                .unwrap()
                .is_none()
        );
        let indexed = vec![crate::knowledge_service::KnowledgeDocumentInfo {
            id: "note-one".into(),
            path: "note.md".into(),
            title: "note".into(),
        }];
        assert!(matches!(
            accept(
                &mut store,
                "cmd-one",
                &command,
                &indexed,
                "---\norbit_id: note-one\n---\n# tampered"
            ),
            Err(AuthorityError::ImmutableConflict)
        ));
        assert_eq!(pending(&store).unwrap().len(), 1);
        let receipt = accept(
            &mut store,
            "cmd-one",
            &command,
            &indexed,
            "---\norbit_id: note-one\n---\n# correct",
        )
        .unwrap();
        assert_eq!(receipt.revision, 1);
    }

    #[test]
    fn create_receipt_requires_matching_filesystem_path() {
        let mut store = AuthorityStore::open(Path::new(":memory:")).unwrap();
        schemas(&store);
        let command = MarkdownCommand::Create {
            note_id: "note-one".into(),
            path: "Work/note.md".into(),
            body: "hello".into(),
        };
        enqueue(&mut store, "owner", "cmd-one", &command).unwrap();
        let moved = vec![crate::knowledge_service::KnowledgeDocumentInfo {
            id: "note-one".into(),
            path: "Personal/note.md".into(),
            title: "note".into(),
        }];
        let published = "---\norbit_id: note-one\n---\nhello";
        assert!(matches!(
            accept(&mut store, "cmd-one", &command, &moved, published),
            Err(AuthorityError::ImmutableConflict)
        ));
        assert_eq!(pending(&store).unwrap().len(), 1);
        let expected = vec![crate::knowledge_service::KnowledgeDocumentInfo {
            id: "note-one".into(),
            path: "Work/note.md".into(),
            title: "note".into(),
        }];
        assert_eq!(
            accept(&mut store, "cmd-one", &command, &expected, published)
                .unwrap()
                .revision,
            1
        );
    }

    #[test]
    fn rejected_publication_releases_the_note_reservation() {
        let mut store = AuthorityStore::open(Path::new(":memory:")).unwrap();
        schemas(&store);
        let rejected = MarkdownCommand::Create {
            note_id: "note-one".into(),
            path: "missing/note.md".into(),
            body: "# rejected".into(),
        };
        assert!(
            enqueue(&mut store, "owner", "cmd-bad", &rejected)
                .unwrap()
                .is_none()
        );
        abandon_pending(&mut store, "cmd-bad").unwrap();
        assert!(pending(&store).unwrap().is_empty());
        let replacement = MarkdownCommand::Create {
            note_id: "note-one".into(),
            path: "note.md".into(),
            body: "# accepted".into(),
        };
        assert!(
            enqueue(&mut store, "owner", "cmd-good", &replacement)
                .unwrap()
                .is_none()
        );
    }
}
