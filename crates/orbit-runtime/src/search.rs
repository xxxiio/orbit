//! One server-backed search across projects, tasks and knowledge notes.
//!
//! Projects and tasks are matched in SQL against the canonical authority rows;
//! notes are matched by the existing file-backed Markdown search so no second
//! content index is introduced here. Every group is filled from a single
//! authority snapshot, so the revision on the answer describes the rows it
//! carries.
use crate::knowledge_service::KnowledgeDocumentInfo;
use crate::store::AuthorityError;
use rusqlite::{Connection, params};
use serde::Serialize;

/// Largest accepted query, in characters. Bounds the LIKE pattern and keeps the
/// endpoint from being used as a general-purpose scanner.
pub const MAX_QUERY_CHARS: usize = 200;
/// Page size when the caller does not ask for one.
pub const DEFAULT_LIMIT: usize = 10;
/// Largest page size; every group reports at most this many rows.
pub const MAX_LIMIT: usize = 20;
/// `_` and `%` are LIKE wildcards, and `\` is the escape character itself.
const LIKE_ESCAPE: char = '\\';

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SearchKind {
    Project,
    Task,
    Note,
}

/// One hit. `subtitle` is server-composed plain text so clients never have to
/// re-derive a status/project line or a note path to display a result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SearchResult {
    pub kind: SearchKind,
    pub id: String,
    pub title: String,
    pub subtitle: String,
    pub project_id: Option<String>,
    pub path: Option<String>,
}

/// One result group. `truncated` is derived by fetching one row more than asked
/// for, never by counting the matches.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SearchGroup {
    pub kind: SearchKind,
    pub truncated: bool,
    pub results: Vec<SearchResult>,
}

/// The complete answer for one query. Groups are always present and ordered
/// `project`, `task`, `note`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SearchResponse {
    pub revision: i64,
    pub query: String,
    pub limit: usize,
    pub groups: Vec<SearchGroup>,
}

/// The SQL-backed half of a search; notes are composed by the host because they
/// live in the Markdown vault rather than the authority database.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthoritySearch {
    pub revision: i64,
    pub query: String,
    pub projects: SearchGroup,
    pub tasks: SearchGroup,
}

/// Clamp a requested page size into the supported window.
pub fn normalize_limit(limit: usize) -> usize {
    limit.clamp(1, MAX_LIMIT)
}

/// Trim and bound the user query. Empty and oversized queries are refusals, not
/// empty results: a client that sent nothing gets an actionable error.
pub fn normalize_query(query: &str) -> Result<String, AuthorityError> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return Err(AuthorityError::Invalid("search query is required"));
    }
    if trimmed.chars().count() > MAX_QUERY_CHARS {
        return Err(AuthorityError::Invalid("search query is too long"));
    }
    Ok(trimmed.to_owned())
}

/// Whole-string pattern for a case-insensitive substring match.
///
/// The user's wildcards are escaped so a query can only match its own literal
/// text, and the pattern is lowercased because SQLite's `LIKE` folds ASCII only;
/// pairing it with `LOWER(column)` makes the case-insensitivity explicit.
fn like_pattern(query: &str) -> String {
    let mut pattern = String::with_capacity(query.len() + 2);
    pattern.push('%');
    for character in query.chars() {
        if character == LIKE_ESCAPE || character == '%' || character == '_' {
            pattern.push(LIKE_ESCAPE);
        }
        pattern.push(character);
    }
    pattern.push('%');
    pattern.to_lowercase()
}

fn group(kind: SearchKind, mut results: Vec<SearchResult>, limit: usize) -> SearchGroup {
    let truncated = results.len() > limit;
    results.truncate(limit);
    SearchGroup {
        kind,
        truncated,
        results,
    }
}

/// Turn file-backed Markdown hits into the note group. The path is both the
/// subtitle and the navigation target; notes are never narrowed by project in
/// this iteration.
pub fn note_group(hits: Vec<KnowledgeDocumentInfo>, limit: usize) -> SearchGroup {
    let results = hits
        .into_iter()
        .map(|note| SearchResult {
            kind: SearchKind::Note,
            id: note.id,
            title: note.title,
            subtitle: note.path.clone(),
            project_id: None,
            path: Some(note.path),
        })
        .collect();
    group(SearchKind::Note, results, limit)
}

/// Match projects and tasks inside one SQLite snapshot.
pub fn search_authority(
    conn: &Connection,
    query: &str,
    limit: usize,
    project_id: Option<&str>,
) -> Result<AuthoritySearch, AuthorityError> {
    let text = normalize_query(query)?;
    let limit = normalize_limit(limit);
    let pattern = like_pattern(&text);
    // One row beyond the page proves truncation without a COUNT query.
    let fetch = i64::try_from(limit.saturating_add(1))
        .map_err(|_| AuthorityError::Invalid("invalid search limit"))?;
    let tx = conn.unchecked_transaction()?;
    let revision: i64 = tx.query_row(
        "SELECT COALESCE(MAX(cursor),0) FROM authority_history",
        [],
        |row| row.get(0),
    )?;
    let projects = {
        // `project_id` narrows the project group by identity.
        let mut stmt = match project_id {
            Some(_) => tx.prepare(
                "SELECT id,name,status FROM projects \
                 WHERE LOWER(name) LIKE ?1 ESCAPE '\\' AND id=?2 \
                 ORDER BY name,id LIMIT ?3",
            )?,
            None => tx.prepare(
                "SELECT id,name,status FROM projects \
                 WHERE LOWER(name) LIKE ?1 ESCAPE '\\' \
                 ORDER BY name,id LIMIT ?2",
            )?,
        };
        let rows = match project_id {
            Some(id) => stmt.query_map(params![pattern, id, fetch], project_result)?,
            None => stmt.query_map(params![pattern, fetch], project_result)?,
        };
        let results = rows.collect::<Result<Vec<_>, _>>()?;
        group(SearchKind::Project, results, limit)
    };
    let tasks = {
        let mut stmt = match project_id {
            Some(_) => tx.prepare(
                "SELECT t.id,t.title,t.status,t.project_id,p.name FROM tasks t \
                 LEFT JOIN projects p ON p.id=t.project_id \
                 WHERE LOWER(t.title) LIKE ?1 ESCAPE '\\' AND t.project_id=?2 \
                 ORDER BY t.title,t.id LIMIT ?3",
            )?,
            None => tx.prepare(
                "SELECT t.id,t.title,t.status,t.project_id,p.name FROM tasks t \
                 LEFT JOIN projects p ON p.id=t.project_id \
                 WHERE LOWER(t.title) LIKE ?1 ESCAPE '\\' \
                 ORDER BY t.title,t.id LIMIT ?2",
            )?,
        };
        let rows = match project_id {
            Some(id) => stmt.query_map(params![pattern, id, fetch], task_result)?,
            None => stmt.query_map(params![pattern, fetch], task_result)?,
        };
        let results = rows.collect::<Result<Vec<_>, _>>()?;
        group(SearchKind::Task, results, limit)
    };
    // The snapshot is read-only; committing proves no write was attempted.
    tx.commit()?;
    Ok(AuthoritySearch {
        revision,
        query: text,
        projects,
        tasks,
    })
}

fn project_result(row: &rusqlite::Row<'_>) -> rusqlite::Result<SearchResult> {
    let id: String = row.get(0)?;
    let name: String = row.get(1)?;
    let status: String = row.get(2)?;
    Ok(SearchResult {
        kind: SearchKind::Project,
        id: id.clone(),
        title: name,
        subtitle: status,
        project_id: Some(id),
        path: None,
    })
}

fn task_result(row: &rusqlite::Row<'_>) -> rusqlite::Result<SearchResult> {
    let id: String = row.get(0)?;
    let title: String = row.get(1)?;
    let status: String = row.get(2)?;
    let project_id: Option<String> = row.get(3)?;
    let project_name: Option<String> = row.get(4)?;
    // An unassigned task still has a status line; a project name is appended
    // only when the task actually belongs to one.
    let subtitle = match project_name {
        Some(project_name) => format!("{status} · {project_name}"),
        None => status,
    };
    Ok(SearchResult {
        kind: SearchKind::Task,
        id,
        title,
        subtitle,
        project_id,
        path: None,
    })
}
