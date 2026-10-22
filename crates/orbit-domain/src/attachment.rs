use crate::{AttachmentId, DomainError};

/// Default maximum attachment size (64 MiB), matching the Markdown vault asset
/// limit so one configuration surface governs both.
pub const DEFAULT_ATTACHMENT_MAX_BYTES: u64 = 64 * 1024 * 1024;
/// Maximum attachment description length in characters.
pub const MAX_ATTACHMENT_DESCRIPTION_CHARS: usize = 1_000;

/// Entity kinds that can own an attachment association.
///
/// This is deliberately a closed enum: adding a surface requires an explicit
/// domain decision instead of accepting arbitrary table names from clients.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AttachmentOwnerKind {
    Task,
    Project,
    WorkItem,
    Knowledge,
    AgentJob,
}

impl AttachmentOwnerKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Task => "task",
            Self::Project => "project",
            Self::WorkItem => "work_item",
            Self::Knowledge => "knowledge",
            Self::AgentJob => "agent_job",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "task" => Some(Self::Task),
            "project" => Some(Self::Project),
            "work_item" => Some(Self::WorkItem),
            "knowledge" => Some(Self::Knowledge),
            "agent_job" => Some(Self::AgentJob),
            _ => None,
        }
    }
}

/// Typed owner reference. `id` is opaque to the attachment subsystem; callers
/// still resolve permissions through their own project/entity scope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttachmentOwner {
    kind: AttachmentOwnerKind,
    id: String,
}

impl AttachmentOwner {
    pub fn new(kind: AttachmentOwnerKind, id: impl Into<String>) -> Result<Self, DomainError> {
        let id = id.into();
        if id.trim().is_empty() {
            return Err(DomainError::EmptyAttachmentOwner);
        }
        Ok(Self { kind, id })
    }

    pub fn task(id: impl Into<String>) -> Result<Self, DomainError> {
        Self::new(AttachmentOwnerKind::Task, id)
    }

    pub fn project(id: impl Into<String>) -> Result<Self, DomainError> {
        Self::new(AttachmentOwnerKind::Project, id)
    }

    pub fn kind(&self) -> AttachmentOwnerKind {
        self.kind
    }

    pub fn id(&self) -> &str {
        &self.id
    }
}

/// Metadata for one attachment association plus its immutable content object.
///
/// The original filename is display metadata only. It is never used to build a
/// filesystem path; content is addressed by `object` (a content hash).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attachment {
    id: AttachmentId,
    owner: AttachmentOwner,
    filename: String,
    content_type: String,
    size_bytes: u64,
    checksum: String,
    object: String,
    created_at_ms: i64,
    description: Option<String>,
}

impl Attachment {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: AttachmentId,
        owner: AttachmentOwner,
        filename: impl Into<String>,
        content_type: impl Into<String>,
        size_bytes: u64,
        checksum: impl Into<String>,
        object: impl Into<String>,
        created_at_ms: i64,
        description: Option<String>,
    ) -> Result<Self, DomainError> {
        let filename = validate_filename(filename.into())?;
        let content_type = validate_content_type(content_type.into())?;
        let checksum = validate_checksum(checksum.into())?;
        let object = object.into();
        if object.trim().is_empty() || object.contains('\0') {
            return Err(DomainError::EmptyAttachmentChecksum);
        }
        if created_at_ms < 0 {
            return Err(DomainError::InvalidAttachmentChecksum);
        }
        let description = validate_description(description)?;
        Ok(Self {
            id,
            owner,
            filename,
            content_type,
            size_bytes,
            checksum,
            object,
            created_at_ms,
            description,
        })
    }

    pub fn id(&self) -> &AttachmentId {
        &self.id
    }

    pub fn owner(&self) -> &AttachmentOwner {
        &self.owner
    }

    pub fn filename(&self) -> &str {
        &self.filename
    }

    pub fn content_type(&self) -> &str {
        &self.content_type
    }

    pub fn size_bytes(&self) -> u64 {
        self.size_bytes
    }

    pub fn checksum(&self) -> &str {
        &self.checksum
    }

    /// Content-addressed object identity inside the managed object store.
    pub fn object(&self) -> &str {
        &self.object
    }

    pub fn created_at_ms(&self) -> i64 {
        self.created_at_ms
    }

    pub fn description(&self) -> Option<&str> {
        self.description.as_deref()
    }

    pub fn is_image(&self) -> bool {
        self.content_type.starts_with("image/")
    }
}

fn validate_filename(filename: String) -> Result<String, DomainError> {
    validate_attachment_filename(&filename)?;
    Ok(filename.trim().to_owned())
}

/// Validate an attachment filename without storing it. Filenames are display
/// metadata only and must never be usable as a path.
pub fn validate_attachment_filename(filename: &str) -> Result<(), DomainError> {
    let filename = filename.trim();
    if filename.is_empty() {
        return Err(DomainError::EmptyAttachmentFilename);
    }
    if filename == "." || filename == ".." || filename.chars().count() > 255 {
        return Err(DomainError::InvalidAttachmentFilename);
    }
    if filename.contains('/') || filename.contains('\\') || filename.chars().any(|c| c.is_control())
    {
        return Err(DomainError::InvalidAttachmentFilename);
    }
    Ok(())
}

/// Validate and normalise a media type. Lowercased for stable comparison.
pub fn validate_attachment_content_type(content_type: &str) -> Result<String, DomainError> {
    let content_type = content_type.trim().to_ascii_lowercase();
    if content_type.is_empty() {
        return Err(DomainError::EmptyAttachmentContentType);
    }
    let valid_shape = content_type
        .split_once('/')
        .is_some_and(|(kind, subtype)| !kind.is_empty() && !subtype.is_empty())
        && content_type.chars().count() <= 128
        && !content_type.chars().any(|c| c.is_control() || c == ' ');
    if !valid_shape {
        return Err(DomainError::InvalidAttachmentContentType);
    }
    Ok(content_type)
}

fn validate_content_type(content_type: String) -> Result<String, DomainError> {
    validate_attachment_content_type(&content_type)
}

fn validate_checksum(checksum: String) -> Result<String, DomainError> {
    let checksum = checksum.trim().to_ascii_lowercase();
    if checksum.is_empty() {
        return Err(DomainError::EmptyAttachmentChecksum);
    }
    if checksum.len() != 64 || !checksum.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(DomainError::InvalidAttachmentChecksum);
    }
    Ok(checksum)
}

fn validate_description(description: Option<String>) -> Result<Option<String>, DomainError> {
    match description {
        None => Ok(None),
        Some(description) => {
            let description = description.trim().to_owned();
            if description.is_empty() {
                return Ok(None);
            }
            if description.chars().count() > MAX_ATTACHMENT_DESCRIPTION_CHARS
                || description
                    .chars()
                    .any(|c| c.is_control() && c != '\n' && c != '\t')
            {
                return Err(DomainError::InvalidAttachmentDescription);
            }
            Ok(Some(description))
        }
    }
}

/// Best-effort media type from a filename extension. Unknown extensions fall
/// back to `application/octet-stream` instead of guessing or executing content.
pub fn content_type_for_filename(filename: &str) -> &'static str {
    let extension = filename
        .rsplit_once('.')
        .map(|(_, extension)| extension.to_ascii_lowercase())
        .unwrap_or_default();
    match extension.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "bmp" => "image/bmp",
        "pdf" => "application/pdf",
        "txt" => "text/plain",
        "md" | "markdown" => "text/markdown",
        "json" => "application/json",
        "csv" => "text/csv",
        "html" | "htm" => "text/html",
        "zip" => "application/zip",
        "gz" => "application/gzip",
        "tar" => "application/x-tar",
        "log" => "text/plain",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn attachment_id() -> AttachmentId {
        AttachmentId::new("attachment-1").expect("valid id")
    }

    fn owner() -> AttachmentOwner {
        AttachmentOwner::task("task-1").expect("valid owner")
    }

    fn checksum() -> String {
        "a".repeat(64)
    }

    #[test]
    fn attachments_reject_path_like_filenames() {
        for filename in ["../secret", "dir/file.txt", "dir\\file.txt", "..", ""] {
            let result = Attachment::new(
                attachment_id(),
                owner(),
                filename,
                "text/plain",
                3,
                checksum(),
                checksum(),
                1,
                None,
            );
            assert!(
                matches!(
                    result,
                    Err(DomainError::EmptyAttachmentFilename
                        | DomainError::InvalidAttachmentFilename)
                ),
                "filename {filename} should be rejected"
            );
        }
    }

    #[test]
    fn attachment_metadata_preserves_display_name_and_object_identity() {
        let attachment = Attachment::new(
            attachment_id(),
            owner(),
            "screenshot.png",
            "IMAGE/PNG",
            128,
            checksum(),
            checksum(),
            42,
            Some("Failure screenshot".to_owned()),
        )
        .expect("valid attachment");

        assert_eq!(attachment.filename(), "screenshot.png");
        assert_eq!(attachment.content_type(), "image/png");
        assert_eq!(attachment.object(), checksum());
        assert!(attachment.is_image());
        assert_eq!(attachment.description(), Some("Failure screenshot"));
    }

    #[test]
    fn invalid_checksums_are_rejected() {
        let result = Attachment::new(
            attachment_id(),
            owner(),
            "note.txt",
            "text/plain",
            1,
            "not-a-hash",
            checksum(),
            1,
            None,
        );
        assert_eq!(result, Err(DomainError::InvalidAttachmentChecksum));
    }

    #[test]
    fn content_types_are_guessed_without_executing_content() {
        assert_eq!(content_type_for_filename("a.PNG"), "image/png");
        assert_eq!(content_type_for_filename("notes.md"), "text/markdown");
        assert_eq!(
            content_type_for_filename("payload.exe"),
            "application/octet-stream"
        );
    }
}
