//! Markdown-authoritative knowledge attributes and explicit references.
//! Edits are prepared from the current file and committed by the durable
//! Markdown command journal; this module does not write SQLite content.
use super::markdown_cli::{authority_response, knowledge_error};
use super::*;
use orbit_runtime::markdown_commands::MarkdownCommand;

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum NoteKind {
    Note,
    Decision,
    Finding,
    Assumption,
    Risk,
    Reference,
    Concept,
    Resource,
    MeetingNote,
    DailyNote,
    ProjectContext,
}
impl NoteKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Note => "note",
            Self::Decision => "decision",
            Self::Finding => "finding",
            Self::Assumption => "assumption",
            Self::Risk => "risk",
            Self::Reference => "reference",
            Self::Concept => "concept",
            Self::Resource => "resource",
            Self::MeetingNote => "meeting_note",
            Self::DailyNote => "daily_note",
            Self::ProjectContext => "project_context",
        }
    }
}
#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum NoteStatus {
    Active,
    Resolved,
    Superseded,
    Archived,
}
impl NoteStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Resolved => "resolved",
            Self::Superseded => "superseded",
            Self::Archived => "archived",
        }
    }
}

fn malformed(message: impl Into<String>) -> CliError {
    CliError::Usage(message.into())
}

/// Strict, scalar YAML subset: avoid parsing or overwriting nested YAML, folded
/// strings or document bodies when manipulating user-supplied frontmatter.
fn frontmatter_bounds(markdown: &str) -> Result<usize, CliError> {
    if !markdown.starts_with("---\n") {
        return Err(malformed("Markdown requires YAML frontmatter"));
    }
    markdown[4..]
        .find("\n---\n")
        .map(|offset| 4 + offset)
        .or_else(|| {
            markdown[4..]
                .ends_with("\n---")
                .then_some(markdown.len() - 4)
        })
        .ok_or_else(|| malformed("Markdown frontmatter has no closing delimiter"))
}
fn read_scalar(markdown: &str, key: &str) -> Result<Option<String>, CliError> {
    let end = frontmatter_bounds(markdown)?;
    let mut found = None;
    for line in markdown[4..end].lines() {
        if let Some((field, value)) = line.split_once(':')
            && field == key
        {
            if found.is_some() {
                return Err(malformed(format!("duplicate {key} in frontmatter")));
            }
            let value = value.trim();
            if value.is_empty()
                || value
                    .chars()
                    .next()
                    .is_some_and(|ch| matches!(ch, '[' | '{' | '>' | '|' | '\'' | '"'))
            {
                return Err(malformed(format!("{key} must be a plain scalar")));
            }
            found = Some(value.to_owned());
        }
    }
    Ok(found)
}
fn set_scalar(markdown: &str, key: &str, value: &str) -> Result<String, CliError> {
    if !matches!(key, "orbit_kind" | "orbit_status")
        || value.is_empty()
        || !value.bytes().all(|b| b.is_ascii_lowercase() || b == b'_')
    {
        return Err(malformed("unsupported frontmatter key/value"));
    }
    let end = frontmatter_bounds(markdown)?;
    let old = read_scalar(markdown, key)?;
    let mut result = String::from("---\n");
    let mut changed = false;
    for line in markdown[4..end].lines() {
        if line.split_once(':').is_some_and(|(field, _)| field == key) {
            result.push_str(&format!("{key}: {value}\n"));
            changed = true;
        } else {
            result.push_str(line);
            result.push('\n');
        }
    }
    if !changed {
        result.push_str(&format!("{key}: {value}\n"));
    }
    result.push_str(&markdown[end..]);
    if old.as_deref() == Some(value) {
        return Ok(markdown.to_owned());
    }
    Ok(result)
}
fn valid_target(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}
fn link_token(id: &str) -> String {
    format!("[[{id}]]")
}
fn has_link(body: &str, target: &str) -> bool {
    let mut remainder = body;
    while let Some(index) = remainder.find("[[") {
        remainder = &remainder[index + 2..];
        if let Some(end) = remainder.find("]]") {
            if remainder[..end].split('|').next() == Some(target) {
                return true;
            }
            remainder = &remainder[end + 2..];
        } else {
            break;
        }
    }
    false
}
fn remove_links(body: &str, target: &str) -> String {
    let mut output = String::with_capacity(body.len());
    let mut remaining = body;
    while let Some(start) = remaining.find("[[") {
        output.push_str(&remaining[..start]);
        remaining = &remaining[start..];
        if let Some(end) = remaining.find("]]") {
            if remaining[2..end].split('|').next() == Some(target) {
                remaining = &remaining[end + 2..];
                continue;
            }
            output.push_str(&remaining[..end + 2]);
            remaining = &remaining[end + 2..];
        } else {
            break;
        }
    }
    output.push_str(remaining);
    output
}

pub fn mutate_note(
    host: &mut orbit_runtime::AuthoritativeHost,
    id: String,
    operation: NoteMetadataOperation,
    command_id: Option<String>,
) -> Result<Response, CliError> {
    if !valid_target(&id) {
        return Err(malformed("invalid note identity"));
    }
    let (_, before) = host
        .markdown_by_id(&id)
        .map_err(knowledge_error)?
        .ok_or_else(|| malformed(format!("note not found: {id}")))?;
    let after = match operation {
        NoteMetadataOperation::Kind(kind) => set_scalar(&before, "orbit_kind", kind.as_str())?,
        NoteMetadataOperation::Status(status) => {
            set_scalar(&before, "orbit_status", status.as_str())?
        }
        NoteMetadataOperation::AddLink(target) => {
            if !valid_target(&target) {
                return Err(malformed("invalid target identity"));
            }
            if host
                .markdown_by_id(&target)
                .map_err(knowledge_error)?
                .is_none()
            {
                return Err(malformed("link target not found"));
            }
            if has_link(&before, &target) {
                before.clone()
            } else {
                let mut text = before.clone();
                if !text.ends_with('\n') {
                    text.push('\n');
                }
                text.push_str(&format!("\n{}\n", link_token(&target)));
                text
            }
        }
        NoteMetadataOperation::AddTypedReference(target) => {
            validate_typed_target(&target)?;
            let token = format!("[{}](orbit://{})", target, target);
            if before.contains(&token) {
                before.clone()
            } else {
                let mut text = before.clone();
                if !text.ends_with('\n') {
                    text.push('\n');
                }
                text.push_str(&format!("\n{}\n", token));
                text
            }
        }
        NoteMetadataOperation::RemoveTypedReference(target) => {
            validate_typed_target(&target)?;
            before.replace(&format!("[{}](orbit://{})", target, target), "")
        }
        NoteMetadataOperation::RemoveLink(target) => {
            if !valid_target(&target) {
                return Err(malformed("invalid target identity"));
            }
            remove_links(&before, &target)
        }
    };
    if after == before {
        return authority_response(serde_json::json!({"id":id,"changed":false}));
    }
    let receipt = host
        .execute_markdown_command(
            "orbit-cli",
            &command_id.unwrap_or_else(|| generate_id("command")),
            MarkdownCommand::Update {
                note_id: id,
                expected: before,
                replacement: after,
            },
        )
        .map_err(knowledge_error)?;
    authority_response(serde_json::json!({"changed":true,"receipt":receipt}))
}
#[derive(Debug, serde::Serialize)]
pub struct NoteMetadata {
    pub id: String,
    pub path: String,
    pub title: String,
    pub kind: String,
    pub status: String,
    pub links: Vec<String>,
}

/// Build a metadata index exclusively from actual Markdown files, not the
/// historical SQLite knowledge-content schema. Unrecognized existing kinds
/// are preserved as data rather than rewritten as application defaults.
pub fn inspect_notes(
    host: &orbit_runtime::AuthoritativeHost,
    kind: Option<NoteKind>,
    status: Option<NoteStatus>,
) -> Result<Response, CliError> {
    let mut results = Vec::new();
    for info in host.markdown_knowledge_index().map_err(knowledge_error)? {
        let (_, body) = host
            .markdown_by_id(&info.id)
            .map_err(knowledge_error)?
            .ok_or_else(|| malformed("indexed Markdown note disappeared"))?;
        let actual_kind = read_scalar(&body, "orbit_kind")?.unwrap_or_else(|| "note".into());
        let actual_status = read_scalar(&body, "orbit_status")?.unwrap_or_else(|| "active".into());
        if kind.is_some_and(|k| k.as_str() != actual_kind)
            || status.is_some_and(|s| s.as_str() != actual_status)
        {
            continue;
        }
        let relations = host
            .markdown_relations(&info.id)
            .map_err(knowledge_error)?
            .ok_or_else(|| malformed("indexed note relation disappeared"))?;
        results.push(NoteMetadata {
            id: info.id,
            path: info.path,
            title: info.title,
            kind: actual_kind,
            status: actual_status,
            links: relations.outgoing,
        });
    }
    authority_response(results)
}

pub(super) fn validate_typed_target(target: &str) -> Result<(), CliError> {
    let (kind, id) = target
        .split_once(':')
        .ok_or_else(|| malformed("typed target requires project:, task:, or habit:"))?;
    if !matches!(kind, "project" | "task" | "habit") || !valid_target(id) {
        return Err(malformed("invalid typed target"));
    }
    Ok(())
}
pub enum NoteMetadataOperation {
    Kind(NoteKind),
    Status(NoteStatus),
    AddLink(String),
    RemoveLink(String),
    AddTypedReference(String),
    RemoveTypedReference(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    const BASE: &str = "---\norbit_id: note_1\n---\n# Example\n";
    #[test]
    fn metadata_preserves_body_and_identity() {
        let changed = set_scalar(BASE, "orbit_status", "archived").unwrap();
        assert!(changed.contains("orbit_status: archived"));
        assert!(changed.ends_with("# Example\n"));
        assert_eq!(
            read_scalar(&changed, "orbit_id").unwrap(),
            Some("note_1".into())
        );
        let next = set_scalar(&changed, "orbit_status", "active").unwrap();
        assert_eq!(next.matches("orbit_status:").count(), 1);
    }
    #[test]
    fn links_handle_labeled_and_plain_references() {
        let source = format!("{BASE}[[note_2|hello]] and [[note_2]] [[note_20]]\n");
        assert!(has_link(&source, "note_2"));
        let removed = remove_links(&source, "note_2");
        assert!(!has_link(&removed, "note_2"));
        assert!(has_link(&removed, "note_20"));
    }
    #[test]
    fn rejects_duplicate_metadata_fields() {
        let source = "---\norbit_id: note_1\norbit_status: active\norbit_status: archived\n---\n";
        assert!(set_scalar(source, "orbit_status", "active").is_err());
    }
}
