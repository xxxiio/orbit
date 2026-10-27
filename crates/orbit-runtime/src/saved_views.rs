//! Saved views: personal presentation preferences over accepted state.
//!
//! They are stored beside the command ledger so every database gains them on
//! open, but deliberately outside the semantic ledger: saving, editing or
//! deleting a view never advances the authority revision and never changes a
//! workflow binding.
use crate::store::{AuthorityError, AuthorityStore};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};

/// Longest accepted view name, in characters.
pub const MAX_NAME_CHARS: usize = 120;
/// Longest accepted query string, in characters.
pub const MAX_QUERY_CHARS: usize = 4096;

const SCOPE_PROJECT: &str = "project";
const SCOPE_INBOX: &str = "inbox";
const SCOPE_HOME: &str = "home";
const DISPLAY_LIST: &str = "list";
const DISPLAY_BOARD: &str = "board";

/// Create the saved-view tables. Called on every authority open so an existing
/// database acquires them without a migration step.
pub fn schema(conn: &Connection) -> Result<(), AuthorityError> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS web_saved_views (\
            id TEXT PRIMARY KEY NOT NULL,\
            actor TEXT NOT NULL,\
            name TEXT NOT NULL,\
            scope TEXT NOT NULL CHECK (scope IN ('project', 'inbox', 'home')),\
            project_id TEXT,\
            query TEXT NOT NULL,\
            display TEXT NOT NULL CHECK (display IN ('list', 'board')),\
            version INTEGER NOT NULL CHECK (version >= 1),\
            created_at_ms INTEGER NOT NULL CHECK (created_at_ms >= 0),\
            updated_at_ms INTEGER NOT NULL CHECK (updated_at_ms >= 0),\
            deleted INTEGER NOT NULL DEFAULT 0 CHECK (deleted IN (0, 1))\
        );\
        CREATE INDEX IF NOT EXISTS idx_web_saved_views_actor ON web_saved_views(actor, deleted);\
        CREATE TABLE IF NOT EXISTS web_saved_view_commands (\
            command_id TEXT PRIMARY KEY NOT NULL,\
            actor TEXT NOT NULL,\
            payload TEXT NOT NULL,\
            result_json TEXT NOT NULL\
        );",
    )?;
    Ok(())
}

/// One saved view as the browser sees it. `deleted` is deliberately absent: the
/// list only ever contains live rows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavedViewRecord {
    pub id: String,
    pub name: String,
    pub scope: String,
    pub project_id: Option<String>,
    pub query: String,
    pub display: String,
    pub version: i64,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

/// Tagged saved-view command union. `expected_version` fences the row; zero
/// means "create".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum SavedViewCommand {
    Save {
        command_id: String,
        view_id: String,
        expected_version: i64,
        name: String,
        scope: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        project_id: Option<String>,
        query: String,
        display: String,
    },
    Rename {
        command_id: String,
        view_id: String,
        expected_version: i64,
        name: String,
    },
    Delete {
        command_id: String,
        view_id: String,
        expected_version: i64,
    },
}

impl SavedViewCommand {
    fn identity(&self) -> (&str, &str, i64) {
        match self {
            Self::Save {
                command_id,
                view_id,
                expected_version,
                ..
            }
            | Self::Rename {
                command_id,
                view_id,
                expected_version,
                ..
            }
            | Self::Delete {
                command_id,
                view_id,
                expected_version,
            } => (command_id, view_id, *expected_version),
        }
    }
}

/// Command outcome. `view` is null for a delete; `duplicate` reports a replayed
/// receipt rather than a newly applied command.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavedViewResult {
    pub duplicate: bool,
    pub view: Option<SavedViewRecord>,
}

impl AuthorityStore {
    /// Live views owned by one actor, most recently changed first.
    pub fn saved_views(&self, actor: &str) -> Result<Vec<SavedViewRecord>, AuthorityError> {
        if actor.trim().is_empty() {
            return Err(AuthorityError::Invalid("invalid saved view command"));
        }
        let mut stmt = self.conn.prepare(
            "SELECT id,name,scope,project_id,query,display,version,created_at_ms,updated_at_ms \
             FROM web_saved_views WHERE actor=?1 AND deleted=0 \
             ORDER BY updated_at_ms DESC,id",
        )?;
        let rows = stmt.query_map([actor], saved_view_row)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    /// Apply one saved-view command. Writes only the view row and the receipt:
    /// no history, projection, command ledger or revision is touched.
    pub fn saved_view_command(
        &mut self,
        actor: &str,
        command: SavedViewCommand,
    ) -> Result<SavedViewResult, AuthorityError> {
        if actor.trim().is_empty() {
            return Err(AuthorityError::Invalid("invalid saved view command"));
        }
        let (command_id, view_id, expected_version) = command.identity();
        if command_id.trim().is_empty()
            || command_id.len() > 128
            || view_id.trim().is_empty()
            || view_id.len() > 256
            || expected_version < 0
        {
            return Err(AuthorityError::Invalid("invalid saved view command"));
        }
        let command_id = command_id.to_owned();
        let view_id = view_id.to_owned();
        let payload = serde_json::to_string(&command)
            .map_err(|_| AuthorityError::Invalid("invalid saved view command"))?;
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        // Receipts are checked before any row or version rule, so a retry keeps
        // answering with the accepted outcome even after later edits.
        let previous: Option<(String, String, String)> = tx
            .query_row(
                "SELECT actor,payload,result_json FROM web_saved_view_commands WHERE command_id=?1",
                [&command_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()?;
        if let Some((saved_actor, saved_payload, result_json)) = previous {
            if saved_actor != actor || saved_payload != payload {
                return Err(AuthorityError::ImmutableConflict);
            }
            let mut result: SavedViewResult = serde_json::from_str(&result_json)
                .map_err(|_| AuthorityError::Invalid("invalid stored saved view receipt"))?;
            result.duplicate = true;
            tx.commit()?;
            return Ok(result);
        }
        let now = now_ms()?;
        let result = match command {
            SavedViewCommand::Save {
                name,
                scope,
                project_id,
                query,
                display,
                ..
            } => {
                let name = validate_name(&name)?;
                let scope = validate_scope(&scope)?;
                let display = validate_display(&display)?;
                validate_query(&query)?;
                let project_id = project_id
                    .map(|id| id.trim().to_owned())
                    .filter(|id| !id.is_empty());
                let view = if expected_version == 0 {
                    // The id is the row's identity, so a tombstone still
                    // occupies it: a create must choose a fresh id.
                    if row_exists(&tx, &view_id)? {
                        return Err(AuthorityError::Invalid("saved view already exists"));
                    }
                    let view = SavedViewRecord {
                        id: view_id.clone(),
                        name,
                        scope: scope.to_owned(),
                        project_id,
                        query,
                        display: display.to_owned(),
                        version: 1,
                        created_at_ms: now,
                        updated_at_ms: now,
                    };
                    tx.execute(
                        "INSERT INTO web_saved_views(id,actor,name,scope,project_id,query,display,version,created_at_ms,updated_at_ms,deleted) \
                         VALUES(?1,?2,?3,?4,?5,?6,?7,1,?8,?8,0)",
                        params![
                            view.id,
                            actor,
                            view.name,
                            view.scope,
                            view.project_id,
                            view.query,
                            view.display,
                            now
                        ],
                    )?;
                    view
                } else {
                    let saved = owned_live_view(&tx, actor, &view_id, expected_version)?;
                    let version = saved.version + 1;
                    tx.execute(
                        "UPDATE web_saved_views SET name=?1,scope=?2,project_id=?3,query=?4,display=?5,version=?6,updated_at_ms=?7 WHERE id=?8",
                        params![name, scope, project_id, query, display, version, now, view_id],
                    )?;
                    SavedViewRecord {
                        name,
                        scope: scope.to_owned(),
                        project_id,
                        query,
                        display: display.to_owned(),
                        version,
                        updated_at_ms: now,
                        ..saved
                    }
                };
                SavedViewResult {
                    duplicate: false,
                    view: Some(view),
                }
            }
            SavedViewCommand::Rename { name, .. } => {
                let name = validate_name(&name)?;
                let saved = owned_live_view(&tx, actor, &view_id, expected_version)?;
                let version = saved.version + 1;
                tx.execute(
                    "UPDATE web_saved_views SET name=?1,version=?2,updated_at_ms=?3 WHERE id=?4",
                    params![name, version, now, view_id],
                )?;
                SavedViewResult {
                    duplicate: false,
                    view: Some(SavedViewRecord {
                        name,
                        version,
                        updated_at_ms: now,
                        ..saved
                    }),
                }
            }
            SavedViewCommand::Delete { .. } => {
                let saved = owned_live_view(&tx, actor, &view_id, expected_version)?;
                // Soft delete: the row is retained so the id keeps its owner and
                // a replay of an earlier receipt stays explainable.
                tx.execute(
                    "UPDATE web_saved_views SET deleted=1,version=?1,updated_at_ms=?2 WHERE id=?3",
                    params![saved.version + 1, now, view_id],
                )?;
                SavedViewResult {
                    duplicate: false,
                    view: None,
                }
            }
        };
        let result_json = serde_json::to_string(&result)
            .map_err(|_| AuthorityError::Invalid("invalid saved view result"))?;
        tx.execute(
            "INSERT INTO web_saved_view_commands(command_id,actor,payload,result_json) VALUES(?1,?2,?3,?4)",
            params![command_id, actor, payload, result_json],
        )?;
        tx.commit()?;
        Ok(result)
    }
}

fn saved_view_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<SavedViewRecord> {
    Ok(SavedViewRecord {
        id: row.get(0)?,
        name: row.get(1)?,
        scope: row.get(2)?,
        project_id: row.get(3)?,
        query: row.get(4)?,
        display: row.get(5)?,
        version: row.get(6)?,
        created_at_ms: row.get(7)?,
        updated_at_ms: row.get(8)?,
    })
}

fn row_exists(tx: &Connection, view_id: &str) -> Result<bool, AuthorityError> {
    Ok(tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM web_saved_views WHERE id=?1)",
        [view_id],
        |row| row.get(0),
    )?)
}

/// Resolve the live row for one actor and fence it against the expected version.
/// A row owned by another actor is reported exactly like a missing one so
/// personal views are never disclosed across devices.
fn owned_live_view(
    tx: &Connection,
    actor: &str,
    view_id: &str,
    expected_version: i64,
) -> Result<SavedViewRecord, AuthorityError> {
    let saved: Option<(String, SavedViewRecord)> = tx
        .query_row(
            "SELECT id,name,scope,project_id,query,display,version,created_at_ms,updated_at_ms,actor \
             FROM web_saved_views WHERE id=?1 AND deleted=0",
            [view_id],
            |row| Ok((row.get(9)?, saved_view_row(row)?)),
        )
        .optional()?;
    let Some((owner, view)) = saved else {
        return Err(AuthorityError::Invalid("saved view does not exist"));
    };
    if owner != actor {
        return Err(AuthorityError::Invalid("saved view does not exist"));
    }
    if view.version != expected_version {
        return Err(AuthorityError::Invalid("saved view changed on the server"));
    }
    Ok(view)
}

fn validate_name(name: &str) -> Result<String, AuthorityError> {
    let trimmed = name.trim();
    if trimmed.is_empty() || trimmed.chars().count() > MAX_NAME_CHARS {
        return Err(AuthorityError::Invalid("invalid saved view name"));
    }
    Ok(trimmed.to_owned())
}

fn validate_scope(scope: &str) -> Result<&'static str, AuthorityError> {
    match scope.trim() {
        SCOPE_PROJECT => Ok(SCOPE_PROJECT),
        SCOPE_INBOX => Ok(SCOPE_INBOX),
        SCOPE_HOME => Ok(SCOPE_HOME),
        _ => Err(AuthorityError::Invalid("invalid saved view scope")),
    }
}

fn validate_display(display: &str) -> Result<&'static str, AuthorityError> {
    match display.trim() {
        DISPLAY_LIST => Ok(DISPLAY_LIST),
        DISPLAY_BOARD => Ok(DISPLAY_BOARD),
        _ => Err(AuthorityError::Invalid("invalid saved view display")),
    }
}

fn validate_query(query: &str) -> Result<(), AuthorityError> {
    if query.chars().count() > MAX_QUERY_CHARS {
        return Err(AuthorityError::Invalid("saved view query too long"));
    }
    Ok(())
}

fn now_ms() -> Result<i64, AuthorityError> {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| AuthorityError::Invalid("clock"))?
        .as_millis()
        .try_into()
        .map_err(|_| AuthorityError::Invalid("clock overflow"))
}
