//! Typed failures for workspace backup, verification and recovery.
//!
//! Errors are deliberately explicit because a backup or restore failure is an
//! operational event: the caller must be able to tell a busy workspace apart
//! from an unsupported format, a corrupt archive or an unsafe restore target.
//! No variant carries credentials or filesystem content.

use std::fmt;
use std::io;

/// Result alias for the backup crate.
pub type Result<T> = std::result::Result<T, BackupError>;

/// A structured backup/restore failure.
#[derive(Debug)]
pub enum BackupError {
    /// A filesystem operation failed. The context names the operation and path.
    Io { context: String, source: io::Error },
    /// A caller supplied an argument the operation cannot honour.
    InvalidArgument(String),
    /// The archive declares a format newer or older than this build understands.
    UnsupportedArchiveFormat {
        found: u32,
        supported: u32,
        min_reader: u32,
    },
    /// The snapshot schema is newer than this build can open.
    IncompatibleSchema { backup: i64, supported: i64 },
    /// The manifest is missing, unreadable or internally inconsistent.
    Manifest(String),
    /// The archive container itself is malformed or truncated.
    Archive(String),
    /// A checksum, size or SQLite integrity check failed.
    Integrity(String),
    /// An archive or restore path escapes the intended directory.
    UnsafePath(String),
    /// The snapshot references attachment objects whose content is missing.
    MissingAttachmentObjects(Vec<String>),
    /// Another process holds the write fence longer than the caller allows.
    WorkspaceBusy(String),
    /// A restore was refused because the target is active or otherwise unsafe.
    RestoreRefused(String),
    /// The requested backup, workspace or object does not exist.
    NotFound(String),
    /// The operation conflicts with existing state that it must not overwrite.
    Conflict(String),
    /// The operation is not supported in the current mode or build.
    Unavailable(String),
    /// An iteration-level invariant was violated. This is a bug, not user error.
    Internal(String),
}

impl BackupError {
    /// Build an I/O error with a stable, actionable context string.
    pub fn io(context: impl Into<String>, source: io::Error) -> Self {
        Self::Io {
            context: context.into(),
            source,
        }
    }
}

impl fmt::Display for BackupError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { context, source } => write!(formatter, "{context}: {source}"),
            Self::InvalidArgument(detail) => write!(formatter, "invalid argument: {detail}"),
            Self::UnsupportedArchiveFormat {
                found,
                supported,
                min_reader,
            } => write!(
                formatter,
                "backup format version {found} is not supported by this Orbit build \
                 (supports up to format {supported}, readable from format {min_reader}); \
                 upgrade Orbit to restore this backup"
            ),
            Self::IncompatibleSchema { backup, supported } => write!(
                formatter,
                "backup database schema {backup} is newer than this Orbit build supports \
                 (schema {supported}); install a newer Orbit or restore with a compatible build"
            ),
            Self::Manifest(detail) => write!(formatter, "invalid backup manifest: {detail}"),
            Self::Archive(detail) => write!(formatter, "invalid backup archive: {detail}"),
            Self::Integrity(detail) => write!(formatter, "backup integrity check failed: {detail}"),
            Self::UnsafePath(detail) => write!(formatter, "unsafe archive path: {detail}"),
            Self::MissingAttachmentObjects(objects) => write!(
                formatter,
                "{} referenced attachment object(s) are missing from the attachment store; \
                 run attachment cleanup or repair before backing up (first: {})",
                objects.len(),
                objects.first().map(String::as_str).unwrap_or("unknown")
            ),
            Self::WorkspaceBusy(detail) => write!(
                formatter,
                "workspace is busy and the snapshot fence could not be acquired: {detail}"
            ),
            Self::RestoreRefused(detail) => write!(formatter, "restore refused: {detail}"),
            Self::NotFound(detail) => write!(formatter, "not found: {detail}"),
            Self::Conflict(detail) => write!(formatter, "conflict: {detail}"),
            Self::Unavailable(detail) => write!(formatter, "unavailable: {detail}"),
            Self::Internal(detail) => write!(formatter, "internal backup error: {detail}"),
        }
    }
}

impl std::error::Error for BackupError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

impl From<io::Error> for BackupError {
    fn from(error: io::Error) -> Self {
        Self::Io {
            context: "filesystem operation failed".to_owned(),
            source: error,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unsupported_format_message_is_actionable() {
        let message = BackupError::UnsupportedArchiveFormat {
            found: 7,
            supported: 1,
            min_reader: 1,
        }
        .to_string();
        assert!(message.contains("format version 7"));
        assert!(message.contains("upgrade Orbit"));
    }

    #[test]
    fn schema_message_never_suggests_downgrading_a_database() {
        let message = BackupError::IncompatibleSchema {
            backup: 40,
            supported: 20,
        }
        .to_string();
        assert!(message.contains("newer"));
        assert!(message.contains("schema 20"));
    }

    #[test]
    fn io_context_is_preserved() {
        let error = BackupError::io("write manifest", io::Error::other("disk full"));
        assert_eq!(
            error.to_string(),
            "write manifest: disk full",
            "the operation and cause must both be visible"
        );
    }
}
