//! Compatibility DTO mapping for local CLI/TUI knowledge workflows.
//!
//! Knowledge content and frontmatter live in the physical Markdown vault.
//! This adapter never persists note bodies in SqliteStore; the canonical
//! Markdown command service owns writes, revisions and durable receipts.
use super::{ClientError, ClientErrorCode, KnowledgeItemDto};
use orbit_runtime::knowledge_service::KnowledgeDocumentInfo;

fn invalid(message: impl Into<String>) -> ClientError {
    ClientError {
        code: ClientErrorCode::CorruptData,
        message: message.into(),
    }
}

/// The frontmatter block is part of the Markdown document, not a SQL row.
/// Unknown frontmatter fields are retained on Orbit-managed updates.
fn frontmatter(markdown: &str) -> Result<(&str, &str), ClientError> {
    let rest = markdown
        .strip_prefix("---\n")
        .ok_or_else(|| invalid("missing Markdown frontmatter"))?;
    let at = rest
        .find("\n---\n")
        .ok_or_else(|| invalid("unterminated Markdown frontmatter"))?;
    Ok((&rest[..at], &rest[at + 5..]))
}
fn front_field<'a>(raw: &'a str, key: &str) -> Option<&'a str> {
    raw.lines()
        .find_map(|line| line.strip_prefix(key).and_then(|v| v.strip_prefix(": ")))
}
fn parse_yaml_string(raw: &str) -> String {
    if raw.starts_with('"') {
        serde_json::from_str::<String>(raw).unwrap_or_else(|_| raw.to_owned())
    } else {
        raw.to_owned()
    }
}

/// Reconstruct rich legacy DTOs without creating a second content store.
/// Documents written externally with only orbit_id remain accessible.
pub fn dto(info: &KnowledgeDocumentInfo, markdown: &str) -> Result<KnowledgeItemDto, ClientError> {
    let (fm, body) = frontmatter(markdown)?;
    if front_field(fm, "orbit_id") != Some(info.id.as_str()) {
        return Err(invalid("Markdown frontmatter identity mismatch"));
    }
    let title = front_field(fm, "orbit_title")
        .map(parse_yaml_string)
        .unwrap_or_else(|| info.title.clone());
    Ok(KnowledgeItemDto {
        id: info.id.clone(),
        title,
        kind: front_field(fm, "orbit_kind").unwrap_or("note").to_owned(),
        body_markdown: body.to_owned(),
        status: front_field(fm, "orbit_status")
            .unwrap_or("active")
            .to_owned(),
        provenance: front_field(fm, "orbit_provenance")
            .unwrap_or("human")
            .to_owned(),
        updated_at_ms: front_field(fm, "orbit_updated_at_ms")
            .and_then(|s| s.parse::<i64>().ok())
            .unwrap_or(0),
    })
}

/// Keep user-owned YAML entries; only the `orbit_*` namespace is managed by
/// the runtime. JSON string quoting is a valid YAML scalar representation.
pub fn render(info: &KnowledgeItemDto, extra_frontmatter: Option<&str>) -> String {
    let mut raw = String::from("---\n");
    raw.push_str(&format!("orbit_id: {}\n", info.id));
    raw.push_str(&format!("orbit_kind: {}\n", info.kind));
    raw.push_str(&format!(
        "orbit_title: {}\n",
        serde_json::to_string(&info.title).expect("string serialization")
    ));
    raw.push_str(&format!("orbit_status: {}\n", info.status));
    raw.push_str(&format!("orbit_provenance: {}\n", info.provenance));
    raw.push_str(&format!("orbit_updated_at_ms: {}\n", info.updated_at_ms));
    if let Some(unknown) = extra_frontmatter {
        for line in unknown.lines() {
            if !line.starts_with("orbit_") && !line.starts_with("---") {
                raw.push_str(line);
                raw.push('\n');
            }
        }
    }
    raw.push_str("---\n");
    raw.push_str(&info.body_markdown);
    raw
}

pub fn replace_preserving_metadata(
    original: &str,
    item: &KnowledgeItemDto,
) -> Result<String, ClientError> {
    let (fm, _) = frontmatter(original)?;
    Ok(render(item, Some(fm)))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn markdown_metadata_roundtrip_preserves_user_frontmatter() {
        let item = KnowledgeItemDto {
            id: "note-1".into(),
            kind: "decision".into(),
            title: "Orbit: Architecture".into(),
            body_markdown: "# Body\nHello".into(),
            status: "active".into(),
            provenance: "human".into(),
            updated_at_ms: 10,
        };
        let raw = render(&item, Some("tags: [orbit]\ncustom: true"));
        assert!(raw.contains("tags: [orbit]"));
        let info = KnowledgeDocumentInfo {
            id: "note-1".into(),
            path: "Inbox/note-1.md".into(),
            title: "note-1".into(),
        };
        assert_eq!(dto(&info, &raw).unwrap(), item);
        let mut edited = item.clone();
        edited.status = "resolved".into();
        let updated = replace_preserving_metadata(&raw, &edited).unwrap();
        assert!(updated.contains("tags: [orbit]"));
        assert_eq!(dto(&info, &updated).unwrap().status, "resolved");
    }
}
