//! Attachment associations over the host-owned content-addressed store.
//!
//! Content is immutable and addressed by hash, so the same bytes attached to
//! two owners share exactly one object. Clients only ever receive bytes; they
//! never execute attachment content and never derive filesystem paths from a
//! display filename.
//!
//! The `host_*` helpers take an already-open host so the unified
//! `AuthorityConnection` embedded transport and the `OrbitClient` facade share
//! exactly one implementation.
use std::fs;
use std::io::Write;
use std::path::Path;

use orbit_application::{ImportAttachmentCommand, RemoveAttachmentCommand};
use orbit_domain::{AttachmentId, DEFAULT_ATTACHMENT_MAX_BYTES, validate_attachment_filename};
use orbit_runtime::AuthoritativeHost;

use crate::facade_conversions::{
    authority_client_error, generate_id, invalid_input, new_activity_id,
};
use crate::task_details::parse_attachment_owner;
use crate::{AttachmentDto, ClientError, ClientErrorCode, OrbitClient};

impl OrbitClient {
    /// Attachment metadata for one owner in stable creation order.
    pub fn list_attachments(
        &self,
        owner_kind: &str,
        owner_id: &str,
    ) -> Result<Vec<AttachmentDto>, ClientError> {
        host_list_attachments(&self.runtime, owner_kind, owner_id)
    }

    /// Import a local regular file as a new attachment association.
    ///
    /// Symbolic links and non-regular files are rejected explicitly: importing
    /// follows no link and reads no device. The authority enforces the
    /// configured size limit and records the semantic activity.
    pub fn import_attachment(
        &mut self,
        owner_kind: String,
        owner_id: String,
        path: String,
        description: Option<String>,
    ) -> Result<AttachmentDto, ClientError> {
        host_import_attachment(
            &mut self.runtime,
            "orbit-client",
            owner_kind,
            owner_id,
            path,
            description,
        )
    }

    /// Read attachment metadata and verified content bytes.
    pub fn attachment_content(&self, id: &str) -> Result<(AttachmentDto, Vec<u8>), ClientError> {
        host_attachment_content(&self.runtime, id)
    }

    /// Copy attachment content to a new local file.
    ///
    /// The destination is opened with `create_new`, so an existing file is
    /// never overwritten. The content is written as inert bytes and is never
    /// executed, imported, or interpreted by Orbit.
    pub fn export_attachment(
        &self,
        id: &str,
        destination: String,
    ) -> Result<AttachmentDto, ClientError> {
        host_export_attachment(&self.runtime, id, destination)
    }

    /// Remove one association. Returns true when its shared content object was
    /// also removed because no other attachment referenced it.
    pub fn remove_attachment(&mut self, id: String) -> Result<bool, ClientError> {
        host_remove_attachment(&mut self.runtime, "orbit-client", id)
    }

    /// Remove every content object no attachment references any more.
    pub fn purge_attachment_content(&mut self) -> Result<usize, ClientError> {
        host_purge_attachment_content(&mut self.runtime)
    }

    /// Root directory of the managed content store. Diagnostics only; clients
    /// must read content through `attachment_content`, never from this path.
    pub fn attachment_store_root(&self) -> std::path::PathBuf {
        self.runtime.attachment_store_root().to_path_buf()
    }
}

pub(crate) fn host_list_attachments(
    host: &AuthoritativeHost,
    owner_kind: &str,
    owner_id: &str,
) -> Result<Vec<AttachmentDto>, ClientError> {
    let owner = parse_attachment_owner(owner_kind, owner_id)?;
    let attachments = host.list_attachments(&owner).map_err(ClientError::from)?;
    Ok(attachments.iter().map(AttachmentDto::from).collect())
}

pub(crate) fn host_import_attachment(
    host: &mut AuthoritativeHost,
    actor: &str,
    owner_kind: String,
    owner_id: String,
    path: String,
    description: Option<String>,
) -> Result<AttachmentDto, ClientError> {
    let owner = parse_attachment_owner(&owner_kind, &owner_id)?;
    let (filename, content_type, content) = read_attachment_source(&path)?;
    let id = AttachmentId::new(generate_id("attachment")).map_err(invalid_input)?;
    let revision = host.snapshot().map_err(authority_client_error)?.0;
    host.import_attachment_accepted(
        actor,
        &generate_id("command"),
        revision,
        ImportAttachmentCommand {
            id: id.clone(),
            owner,
            filename,
            content_type: Some(content_type),
            content,
            description,
            activity_id: new_activity_id()?,
            origin: orbit_domain::ActivityOrigin::Human,
        },
    )
    .map_err(attachment_command_error)?;
    let (attachment, _) = host.attachment_content(&id).map_err(ClientError::from)?;
    Ok(AttachmentDto::from(&attachment))
}

/// Client-visible attachment size limit. One configuration surface governs the
/// client limit and the server limit, so a too-large file is rejected before
/// any bytes are staged or transmitted.
pub(crate) fn attachment_max_bytes() -> u64 {
    std::env::var("ORBIT_ATTACHMENT_MAX_BYTES")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(DEFAULT_ATTACHMENT_MAX_BYTES)
}

/// Read one attachment source file without following symbolic links.
///
/// The file name is metadata, never a path: callers must not rebuild filesystem
/// paths from it. The bytes are inert data and are never interpreted by Orbit.
pub(crate) fn read_attachment_source(path: &str) -> Result<(String, String, Vec<u8>), ClientError> {
    let source = Path::new(path);
    let metadata = fs::symlink_metadata(source).map_err(|error| ClientError {
        code: ClientErrorCode::InvalidInput,
        message: format!("cannot read attachment source {path:?}: {error}"),
    })?;
    if metadata.file_type().is_symlink() {
        return Err(ClientError {
            code: ClientErrorCode::InvalidInput,
            message: format!(
                "attachment source {path:?} is a symbolic link; import the real file instead"
            ),
        });
    }
    if !metadata.is_file() {
        return Err(ClientError {
            code: ClientErrorCode::InvalidInput,
            message: format!(
                "attachment source {path:?} is not a regular file; directories and devices cannot be attached"
            ),
        });
    }
    let filename = source
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| ClientError {
            code: ClientErrorCode::InvalidInput,
            message: format!("attachment source {path:?} has no portable file name"),
        })?
        .to_owned();
    validate_attachment_filename(&filename).map_err(invalid_input)?;
    let limit = attachment_max_bytes();
    if metadata.len() > limit {
        return Err(ClientError {
            code: ClientErrorCode::InvalidInput,
            message: format!(
                "attachment source {path:?} is {} bytes, exceeding the configured {limit} byte limit (ORBIT_ATTACHMENT_MAX_BYTES)",
                metadata.len()
            ),
        });
    }
    let content = fs::read(source).map_err(|error| ClientError {
        code: ClientErrorCode::InvalidInput,
        message: format!("cannot read attachment source {path:?}: {error}"),
    })?;
    if content.len() as u64 > limit {
        return Err(ClientError {
            code: ClientErrorCode::InvalidInput,
            message: format!(
                "attachment source {path:?} grew beyond the configured {limit} byte limit while being read (ORBIT_ATTACHMENT_MAX_BYTES)"
            ),
        });
    }
    let content_type = orbit_domain::content_type_for_filename(&filename).to_owned();
    Ok((filename, content_type, content))
}

pub(crate) fn host_attachment_content(
    host: &AuthoritativeHost,
    id: &str,
) -> Result<(AttachmentDto, Vec<u8>), ClientError> {
    let id = AttachmentId::new(id.to_owned()).map_err(invalid_input)?;
    let (attachment, content) = host.attachment_content(&id).map_err(ClientError::from)?;
    Ok((AttachmentDto::from(&attachment), content))
}

pub(crate) fn host_export_attachment(
    host: &AuthoritativeHost,
    id: &str,
    destination: String,
) -> Result<AttachmentDto, ClientError> {
    let (attachment, content) = host_attachment_content(host, id)?;
    let destination_path = Path::new(&destination);
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination_path)
        .map_err(|error| ClientError {
            code: if error.kind() == std::io::ErrorKind::AlreadyExists {
                ClientErrorCode::Conflict
            } else {
                ClientErrorCode::InvalidInput
            },
            message: format!(
                "cannot create export destination {destination:?}: {error}; existing files are never overwritten"
            ),
        })?;
    if let Err(error) = file.write_all(&content).and_then(|()| file.sync_all()) {
        drop(file);
        let _ = fs::remove_file(destination_path);
        return Err(ClientError {
            code: ClientErrorCode::Unavailable,
            message: format!("cannot write export destination {destination:?}: {error}"),
        });
    }
    Ok(attachment)
}

pub(crate) fn host_remove_attachment(
    host: &mut AuthoritativeHost,
    actor: &str,
    id: String,
) -> Result<bool, ClientError> {
    let id = AttachmentId::new(id).map_err(invalid_input)?;
    let revision = host.snapshot().map_err(authority_client_error)?.0;
    let (_, _, content_removed) = host
        .remove_attachment_accepted(
            actor,
            &generate_id("command"),
            revision,
            RemoveAttachmentCommand {
                id,
                activity_id: new_activity_id()?,
                origin: orbit_domain::ActivityOrigin::Human,
            },
        )
        .map_err(authority_client_error)?;
    Ok(content_removed)
}

pub(crate) fn host_purge_attachment_content(
    host: &mut AuthoritativeHost,
) -> Result<usize, ClientError> {
    host.purge_attachment_content().map_err(ClientError::from)
}

/// Map an authority rejection of an attachment command to an actionable error.
///
/// The authority deliberately flattens application failures, so the size limit
/// cannot be distinguished from other metadata rules here. The message names
/// both the likely cause and the configuration surface.
fn attachment_command_error(error: orbit_runtime::AuthorityError) -> ClientError {
    match &error {
        orbit_runtime::AuthorityError::Invalid(_) => ClientError {
            code: ClientErrorCode::InvalidInput,
            message: format!(
                "the authority rejected the attachment: the file may exceed the configured attachment size limit (default {DEFAULT_ATTACHMENT_MAX_BYTES} bytes, ORBIT_ATTACHMENT_MAX_BYTES) or violate attachment metadata rules ({error:?})"
            ),
        },
        _ => authority_client_error(error),
    }
}
