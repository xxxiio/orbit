//! Durable connected-mode attachment uploads.
//!
//! The server owns attachment acceptance. A connected client may only stage
//! bytes locally, retry the same immutable command ID, and read accepted
//! server state back. Everything in this module is written so that a staged
//! upload can never be mistaken for an accepted attachment:
//!
//! * staged bytes live outside the accepted cache under `attachment-staging/`;
//! * a pending row is reported by `staged_attachments`, never merged into the
//!   accepted `list_attachments` result;
//! * only a validated receipt (matching attachment ID, checksum, size and
//!   revision) moves a row to `accepted`.
use std::fs;
use std::io::Write;
use std::path::Path;

use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::authority::{AttachmentUploadRow, AuthorityError, AuthorityHttp, AuthorityReplica};
use crate::{AttachmentDto, PendingAttachmentDto};

/// Staged content is read from the replica's own directory only when an
/// accepted server read is impossible; the message must stay actionable.
pub(crate) const ACCEPTED_CONTENT_REQUIRES_SERVER: &str = "offline: accepted attachment content requires a server connection; only uploads staged on this replica can be read offline";

fn encode_query(input: &str) -> String {
    let mut encoded = String::new();
    for byte in input.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            encoded.push(char::from(byte));
        } else {
            use std::fmt::Write as _;
            write!(&mut encoded, "%{byte:02X}").expect("write to String");
        }
    }
    encoded
}

pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn is_checksum(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}

fn staged_dto(row: &AttachmentUploadRow) -> AttachmentDto {
    AttachmentDto {
        id: row.attachment_id.clone(),
        owner_kind: row.owner_kind.clone(),
        owner_id: row.owner_id.clone(),
        filename: row.filename.clone(),
        content_type: row.content_type.clone(),
        size_bytes: row.size_bytes,
        checksum: row.checksum.clone(),
        created_at_ms: row.created_at_ms,
        description: row.description.clone(),
        is_image: row.content_type.starts_with("image/"),
    }
}

fn pending_dto(row: &AttachmentUploadRow) -> PendingAttachmentDto {
    PendingAttachmentDto {
        attachment: staged_dto(row),
        state: row.state.clone(),
        last_error: row.last_error.clone(),
    }
}

/// Parse the canonical attachment projection returned by the import route.
///
/// Server-supplied display metadata is untrusted: a filename or media type that
/// could be mistaken for a path or smuggle a header is rejected here, so nothing
/// downstream can build a filesystem path or HTTP header from it.
fn dto_from_wire(value: &Value) -> Result<AttachmentDto, AuthorityError> {
    let field = |name: &str| {
        value
            .get(name)
            .ok_or_else(|| AuthorityError::Invalid(format!("attachment receipt missing {name}")))
    };
    let text = |name: &str| -> Result<String, AuthorityError> {
        field(name)?.as_str().map(str::to_owned).ok_or_else(|| {
            AuthorityError::Invalid(format!("attachment receipt {name} is not text"))
        })
    };
    let size_bytes = field("size_bytes")?
        .as_u64()
        .ok_or_else(|| AuthorityError::Invalid("attachment receipt size is not a number".into()))?;
    let created_at_ms = field("created_at_ms")?
        .as_i64()
        .ok_or_else(|| AuthorityError::Invalid("attachment receipt timestamp is invalid".into()))?;
    let checksum = text("checksum")?;
    if !is_checksum(&checksum) {
        return Err(AuthorityError::Invalid(
            "attachment receipt checksum is not SHA-256".into(),
        ));
    }
    let filename = text("filename")?;
    orbit_domain::validate_attachment_filename(&filename).map_err(|_| {
        AuthorityError::Invalid(format!(
            "server attachment filename {filename:?} is not a portable display name (path separators and control characters are rejected)"
        ))
    })?;
    let content_type = text("content_type")?;
    orbit_domain::validate_attachment_content_type(&content_type).map_err(|_| {
        AuthorityError::Invalid(format!(
            "server attachment content type {content_type:?} is not a valid media type"
        ))
    })?;
    Ok(AttachmentDto {
        id: text("id")?,
        owner_kind: text("owner_kind")?,
        owner_id: text("owner_id")?,
        filename,
        content_type,
        size_bytes,
        checksum,
        created_at_ms,
        description: value
            .get("description")
            .and_then(Value::as_str)
            .map(str::to_owned),
        is_image: field("is_image")?.as_bool().ok_or_else(|| {
            AuthorityError::Invalid("attachment receipt is_image is invalid".into())
        })?,
    })
}

/// Parse the accepted owner listing `{"attachments":[...]}`.
pub(crate) fn dtos_from_wire(value: &Value) -> Result<Vec<AttachmentDto>, AuthorityError> {
    let items = value
        .get("attachments")
        .and_then(Value::as_array)
        .ok_or_else(|| AuthorityError::Invalid("attachment listing lacks attachments".into()))?;
    items.iter().map(dto_from_wire).collect()
}

fn ensure_staging_directory(root: &Path) -> Result<(), AuthorityError> {
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(root).map_err(|error| {
        AuthorityError::Unavailable(format!(
            "cannot create attachment staging directory {}: {error}",
            root.display()
        ))
    })
}

fn staging_path(root: &Path, checksum: &str, attachment_id: &str) -> std::path::PathBuf {
    // Content-addressed name plus the attachment identity: identical bytes are
    // stored once per attachment and two pending rows can never share a file
    // that one of them deletes.
    root.join(format!("{checksum}-{attachment_id}"))
}

/// Atomically persist staged bytes: no partial file is ever visible under the
/// final content-addressed name, and the bytes are fsynced before the rename.
fn write_staged_bytes(
    root: &Path,
    checksum: &str,
    attachment_id: &str,
    bytes: &[u8],
) -> Result<std::path::PathBuf, AuthorityError> {
    ensure_staging_directory(root)?;
    let final_path = staging_path(root, checksum, attachment_id);
    let temporary = root.join(format!(".{attachment_id}.staging"));
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&temporary).map_err(|error| {
        AuthorityError::Unavailable(format!(
            "cannot create attachment staging file {}: {error}",
            temporary.display()
        ))
    })?;
    if let Err(error) = file.write_all(bytes).and_then(|()| file.sync_all()) {
        drop(file);
        let _ = fs::remove_file(&temporary);
        return Err(AuthorityError::Unavailable(format!(
            "cannot persist staged attachment bytes: {error}"
        )));
    }
    drop(file);
    if let Err(error) = fs::rename(&temporary, &final_path) {
        let _ = fs::remove_file(&temporary);
        return Err(AuthorityError::Unavailable(format!(
            "cannot publish staged attachment file {}: {error}",
            final_path.display()
        )));
    }
    Ok(final_path)
}

fn discard_staged_file(path: &str) {
    match fs::remove_file(path) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => { /* staged bytes already unreadable; the row is durable */ }
    }
}

/// Read staged bytes back and prove they still match the recorded identity.
/// Corruption is reported instead of silently uploading different content.
fn read_verified_staged(row: &AttachmentUploadRow) -> Result<Vec<u8>, AuthorityError> {
    let bytes = fs::read(&row.staging_path).map_err(|error| {
        AuthorityError::Unavailable(format!(
            "staged attachment bytes are unavailable at {}: {error}; remove the staged upload and import the file again",
            row.staging_path
        ))
    })?;
    if bytes.len() as u64 != row.size_bytes {
        return Err(AuthorityError::Conflict(format!(
            "staged attachment size changed ({} != {}); remove the staged upload and import the file again",
            bytes.len(),
            row.size_bytes
        )));
    }
    if sha256_hex(&bytes) != row.checksum {
        return Err(AuthorityError::Conflict(
            "staged attachment checksum changed; remove the staged upload and import the file again"
                .into(),
        ));
    }
    Ok(bytes)
}

/// Stage one file for upload. The caller validates the file and the client
/// limit; this function owns hashing, durable bytes and the pending row.
#[allow(clippy::too_many_arguments)]
pub(crate) fn stage_upload(
    replica: &mut AuthorityReplica,
    owner_kind: &str,
    owner_id: &str,
    filename: &str,
    content_type: &str,
    bytes: &[u8],
    description: Option<String>,
) -> Result<AttachmentUploadRow, AuthorityError> {
    let checksum = sha256_hex(bytes);
    if !is_checksum(&checksum) {
        return Err(AuthorityError::Invalid("attachment checksum failed".into()));
    }
    let attachment_id = crate::facade_conversions::generate_id("attachment");
    let path = write_staged_bytes(
        replica.attachment_staging_root(),
        &checksum,
        &attachment_id,
        bytes,
    )?;
    let row = AttachmentUploadRow {
        command_id: crate::facade_conversions::generate_id("command"),
        attachment_id,
        activity_id: crate::facade_conversions::generate_id("activity"),
        owner_kind: owner_kind.to_owned(),
        owner_id: owner_id.to_owned(),
        filename: filename.to_owned(),
        content_type: content_type.to_owned(),
        size_bytes: bytes.len() as u64,
        checksum,
        staging_path: path.to_string_lossy().into_owned(),
        description,
        created_at_ms: crate::facade_conversions::now_ms(),
        state: "pending".into(),
        receipt: None,
        last_error: None,
    };
    if let Err(error) = replica.insert_attachment_upload(&row) {
        discard_staged_file(&row.staging_path);
        return Err(error);
    }
    Ok(row)
}

/// Outcome of one upload attempt. `Pending` is never an acceptance report.
pub(crate) enum UploadOutcome {
    Accepted(AttachmentDto),
    /// The server durably acknowledged that this attachment identity no longer
    /// exists (removed by another client). The local staging row is resolved
    /// and the bytes are gone, but nothing may be cached as accepted.
    RemovedByOtherClient(String),
    Pending,
}

fn record_failure(
    replica: &mut AuthorityReplica,
    row: &AttachmentUploadRow,
    state: &str,
    message: String,
) -> Result<UploadOutcome, AuthorityError> {
    replica.record_attachment_upload_failure(&row.command_id, state, &message)?;
    Ok(UploadOutcome::Pending)
}

/// A 409 is either a refreshable revision fence or a permanent receipt
/// conflict. Only the first is retryable; retrying the second can never succeed
/// and would wedge every later upload.
fn retryable_revision_conflict(detail: &str) -> bool {
    let detail = detail.to_ascii_lowercase();
    detail.contains("stale") || detail.contains("revision")
}

/// Attempt exactly one staged upload.
///
/// The expected revision is re-read immediately before the attempt, so a
/// queued command accepted by another client since the last flush cannot make
/// the upload stale. A transport failure leaves the row pending with an
/// actionable error instead of discarding durable user intent.
pub(crate) fn attempt_upload(
    replica: &mut AuthorityReplica,
    http: &AuthorityHttp,
    row: &AttachmentUploadRow,
) -> Result<UploadOutcome, AuthorityError> {
    let bytes = match read_verified_staged(row) {
        Ok(bytes) => bytes,
        // Staged bytes that no longer match their identity can never be
        // uploaded by retrying; require an explicit local re-import instead of
        // retrying forever.
        Err(error) => {
            return record_failure(replica, row, "conflict", format!("{error:?}"));
        }
    };
    let expected_revision = match http
        .get_json("/v1/authoritative/snapshot")
        .and_then(|snapshot| {
            snapshot
                .get("revision")
                .and_then(Value::as_i64)
                .ok_or_else(|| AuthorityError::Invalid("server lacks revision".into()))
        }) {
        Ok(revision) => revision,
        Err(error @ AuthorityError::Transport(_)) => {
            return record_failure(
                replica,
                row,
                "pending",
                format!("server unreachable; the upload retries on the next flush ({error:?})"),
            );
        }
        Err(error) => return Err(error),
    };
    let mut path = format!(
        "/v1/authoritative/attachment-import?command_id={}&expected_revision={expected_revision}&attachment_id={}&activity_id={}&owner_kind={}&owner_id={}&filename={}&content_type={}",
        encode_query(&row.command_id),
        encode_query(&row.attachment_id),
        encode_query(&row.activity_id),
        encode_query(&row.owner_kind),
        encode_query(&row.owner_id),
        encode_query(&row.filename),
        encode_query(&row.content_type),
    );
    if let Some(description) = row.description.as_deref() {
        use std::fmt::Write as _;
        write!(&mut path, "&description={}", encode_query(description)).expect("write to String");
    }
    let response = match http.post_bytes_with_query(&path, &row.content_type, &bytes) {
        Ok(response) => response,
        Err(error @ AuthorityError::Transport(_)) => {
            return record_failure(
                replica,
                row,
                "pending",
                format!("server unreachable; the upload retries on the next flush ({error:?})"),
            );
        }
        Err(error) => return Err(error),
    };
    if response.is_success() {
        let receipt: Value = serde_json::from_slice(&response.bytes).map_err(|error| {
            AuthorityError::Invalid(format!("invalid attachment acceptance receipt: {error}"))
        })?;
        let duplicate = receipt
            .get("duplicate")
            .and_then(Value::as_bool)
            .ok_or_else(|| AuthorityError::Invalid("attachment receipt lacks duplicate".into()))?;
        // The attachment may have been removed by another client after this
        // upload was committed. The server then reports the immutable command
        // receipt with no attachment; keeping the staged row would retry a file
        // the server has already deleted forever.
        if receipt.get("removed").and_then(Value::as_bool) == Some(true)
            && receipt.get("attachment").is_none_or(Value::is_null)
        {
            replica.mark_attachment_upload_accepted(&row.command_id, &receipt)?;
            discard_staged_file(&row.staging_path);
            return Ok(UploadOutcome::RemovedByOtherClient(
                "the attachment was removed by another client after this upload was accepted; the staged copy was discarded".into(),
            ));
        }
        let accepted_revision = receipt
            .get("revision")
            .and_then(Value::as_i64)
            .ok_or_else(|| AuthorityError::Invalid("attachment receipt lacks revision".into()))?;
        let attachment = receipt
            .get("attachment")
            .ok_or_else(|| AuthorityError::Invalid("attachment receipt lacks metadata".into()))?;
        let dto = dto_from_wire(attachment)?;
        // A successful HTTP status is not acceptance. The receipt must name
        // exactly the bytes this replica staged, and a first acceptance must
        // extend the revision read immediately before the attempt. A duplicate
        // reports the original revision, which is immutable history.
        if dto.id != row.attachment_id
            || dto.checksum != row.checksum
            || dto.size_bytes != row.size_bytes
            || dto.owner_kind != row.owner_kind
            || dto.owner_id != row.owner_id
            || (!duplicate && accepted_revision != expected_revision + 1)
            || accepted_revision < 1
        {
            return Err(AuthorityError::Invalid(
                "attachment acceptance receipt does not match the staged upload".into(),
            ));
        }
        replica.mark_attachment_upload_accepted(&row.command_id, &receipt)?;
        replica.cache_accepted_attachments(std::slice::from_ref(&dto))?;
        discard_staged_file(&row.staging_path);
        return Ok(UploadOutcome::Accepted(dto));
    }
    let detail = response
        .error_message()
        .unwrap_or_else(|| format!("server returned {}", response.status));
    if matches!(response.status, 401 | 403) {
        // Credentials are not a command verdict: never mark the durable intent
        // conflicted, and never retry it blindly on the next flush either.
        return Err(AuthorityError::Unauthorized(format!(
            "server refused the device credentials: {detail}"
        )));
    }
    match response.status {
        404 => record_failure(
            replica,
            row,
            "pending",
            format!(
                "unknown attachment owner: {detail}; the owner may still be queued in the outbox, so the upload stays staged and retries after it is accepted"
            ),
        ),
        409 if retryable_revision_conflict(&detail) => record_failure(
            replica,
            row,
            "pending",
            format!(
                "{detail}; the upload stays staged and retries on the next flush after refreshing"
            ),
        ),
        // A receipt/metadata conflict is permanent: the same immutable command
        // can never be accepted with different bytes.
        409 => record_failure(
            replica,
            row,
            "conflict",
            format!("{detail}; remove this staged upload and import a corrected file"),
        ),
        // Permanent metadata/size rejections must stop automatic retries.
        400 | 413 | 422 => record_failure(
            replica,
            row,
            "conflict",
            format!("{detail}; remove this staged upload and import a corrected file"),
        ),
        status => record_failure(
            replica,
            row,
            "pending",
            format!("server returned {status}: {detail}"),
        ),
    }
}

/// Aggregate result of one attachment flush. Conflicts are reported to the
/// caller so a queue holding only rejected uploads is never called "empty".
pub(crate) struct UploadFlush {
    pub accepted: usize,
    pub conflicts: Vec<(String, String)>,
}

/// Upload every pending row in creation order. Already `accepted` rows send
/// nothing, so repeating a flush can never create a second server attachment.
/// `conflict` rows are skipped until the user removes or corrects them.
pub(crate) fn flush_pending_uploads(
    replica: &mut AuthorityReplica,
    http: &AuthorityHttp,
) -> Result<UploadFlush, AuthorityError> {
    let pending = replica.pending_attachment_uploads()?;
    let mut accepted = 0;
    for row in pending {
        if row.state == "conflict" {
            continue;
        }
        let Some(current) = replica.attachment_upload(&row.command_id)? else {
            continue;
        };
        if current.state != "pending" {
            continue;
        }
        match attempt_upload(replica, http, &current)? {
            UploadOutcome::Accepted(_) => accepted += 1,
            UploadOutcome::RemovedByOtherClient(_) => {}
            UploadOutcome::Pending => {}
        }
    }
    Ok(UploadFlush {
        accepted,
        conflicts: replica.attachment_upload_conflicts()?,
    })
}

/// Stage bytes and immediately offer them to the server when a transport is
/// configured. The returned DTO is canonical server metadata only when the
/// server accepted the upload; otherwise it describes the staged copy and the
/// caller must consult `has_pending_attachment_uploads`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn import_attachment(
    replica: &mut AuthorityReplica,
    http: Option<&AuthorityHttp>,
    owner_kind: &str,
    owner_id: &str,
    filename: &str,
    content_type: &str,
    bytes: &[u8],
    description: Option<String>,
) -> Result<AttachmentDto, AuthorityError> {
    let row = stage_upload(
        replica,
        owner_kind,
        owner_id,
        filename,
        content_type,
        bytes,
        description,
    )?;
    if let Some(http) = http {
        match attempt_upload(replica, http, &row)? {
            UploadOutcome::Accepted(dto) => return Ok(dto),
            UploadOutcome::RemovedByOtherClient(message) => {
                return Err(AuthorityError::Conflict(message));
            }
            UploadOutcome::Pending => {}
        }
    }
    Ok(staged_dto(&row))
}

/// Unacknowledged uploads for one owner. These are never merged into the
/// accepted list.
pub(crate) fn staged_attachments(
    replica: &AuthorityReplica,
    owner_kind: &str,
    owner_id: &str,
) -> Result<Vec<PendingAttachmentDto>, AuthorityError> {
    Ok(replica
        .pending_attachment_uploads_for(Some((owner_kind, owner_id)))?
        .iter()
        .map(pending_dto)
        .collect())
}

/// All unacknowledged uploads across owners, in creation order.
pub(crate) fn all_staged_attachments(
    replica: &AuthorityReplica,
) -> Result<Vec<PendingAttachmentDto>, AuthorityError> {
    Ok(replica
        .pending_attachment_uploads()?
        .iter()
        .map(pending_dto)
        .collect())
}

fn unavailable(message: String) -> AuthorityError {
    AuthorityError::Unavailable(message)
}

/// Read verified content for a staged or accepted attachment.
///
/// Staged bytes are verified against the staged row. Accepted bytes always come
/// from the server and are verified against `x-orbit-checksum`, `x-orbit-size`
/// and the cached accepted metadata; corrupted bytes are never returned.
pub(crate) fn attachment_content(
    replica: &AuthorityReplica,
    http: Option<&AuthorityHttp>,
    id: &str,
) -> Result<(AttachmentDto, Vec<u8>), AuthorityError> {
    if let Some(row) = replica.attachment_upload_by_attachment(id)? {
        if row.state != "accepted" {
            let bytes = read_verified_staged(&row)?;
            return Ok((staged_dto(&row), bytes));
        }
        return download_verified(replica, http, id, Some(&row));
    }
    download_verified(replica, http, id, None)
}

fn download_verified(
    replica: &AuthorityReplica,
    http: Option<&AuthorityHttp>,
    id: &str,
    accepted_row: Option<&AttachmentUploadRow>,
) -> Result<(AttachmentDto, Vec<u8>), AuthorityError> {
    let http = http.ok_or_else(|| unavailable(ACCEPTED_CONTENT_REQUIRES_SERVER.to_owned()))?;
    let metadata = match accepted_row {
        Some(row) => Some(staged_dto(row)),
        None => replica.cached_attachment(id)?,
    };
    let metadata = metadata.ok_or_else(|| {
        AuthorityError::Invalid(format!(
            "attachment {id} is not staged on this replica and its accepted metadata is not cached; list the owner's attachments before reading content"
        ))
    })?;
    if metadata.id != id {
        return Err(AuthorityError::Invalid(
            "cached attachment metadata has the wrong identity".into(),
        ));
    }
    let response = http.get_binary(&format!(
        "/v1/authoritative/attachments/{}/content",
        encode_query(id)
    ))?;
    if !response.is_success() {
        let detail = response
            .error_message()
            .unwrap_or_else(|| format!("server returned {}", response.status));
        return Err(match response.status {
            404 => AuthorityError::Invalid(format!("attachment {id} not found: {detail}")),
            _ => unavailable(format!("server returned {}: {detail}", response.status)),
        });
    }
    let checksum = response.checksum.ok_or_else(|| {
        AuthorityError::Invalid("attachment content response lacks x-orbit-checksum".into())
    })?;
    if !is_checksum(&checksum) || checksum != metadata.checksum {
        return Err(AuthorityError::Conflict(
            "attachment content checksum does not match the accepted metadata".into(),
        ));
    }
    let size = response.size.ok_or_else(|| {
        AuthorityError::Invalid("attachment content response lacks x-orbit-size".into())
    })?;
    if size != metadata.size_bytes || response.bytes.len() as u64 != size {
        return Err(AuthorityError::Conflict(
            "attachment content size does not match the accepted metadata".into(),
        ));
    }
    if sha256_hex(&response.bytes) != checksum {
        return Err(AuthorityError::Conflict(
            "attachment content failed its checksum verification".into(),
        ));
    }
    Ok((metadata, response.bytes))
}

/// Copy verified content into a new file. Existing files are never
/// overwritten, and a partially written export is removed on failure.
pub(crate) fn export_attachment(
    replica: &AuthorityReplica,
    http: Option<&AuthorityHttp>,
    id: &str,
    destination: &str,
) -> Result<AttachmentDto, AuthorityError> {
    // Report the overwrite refusal before any network verification so an
    // existing destination is never rejected only because the server is
    // unreachable.
    if fs::symlink_metadata(Path::new(destination)).is_ok() {
        return Err(AuthorityError::Conflict(format!(
            "export destination {destination:?} already exists; existing files are never overwritten"
        )));
    }
    let (metadata, bytes) = attachment_content(replica, http, id)?;
    write_new_export(Path::new(destination), &bytes)?;
    Ok(metadata)
}

/// Copy verified content into `directory`, naming the file from the accepted
/// (validated) display filename. The derived path can never leave the
/// directory: a filename with separators, control characters or `..` is
/// rejected instead of being joined.
pub(crate) fn export_attachment_to_directory(
    replica: &AuthorityReplica,
    http: Option<&AuthorityHttp>,
    id: &str,
    directory: &str,
) -> Result<AttachmentDto, AuthorityError> {
    let (metadata, bytes) = attachment_content(replica, http, id)?;
    let path = export_destination(Path::new(directory), &metadata.filename)?;
    if fs::symlink_metadata(&path).is_ok() {
        return Err(AuthorityError::Conflict(format!(
            "export destination {path:?} already exists; existing files are never overwritten"
        )));
    }
    write_new_export(&path, &bytes)?;
    Ok(metadata)
}

/// Resolve one server-supplied display filename inside an export directory.
///
/// This is the only place a filename becomes a path, so it is also the place
/// that proves the result stays inside `directory`.
pub(crate) fn export_destination(
    directory: &Path,
    filename: &str,
) -> Result<std::path::PathBuf, AuthorityError> {
    orbit_domain::validate_attachment_filename(filename).map_err(|_| {
        AuthorityError::Invalid(format!(
            "server attachment filename {filename:?} cannot be used as an export path"
        ))
    })?;
    let candidate = directory.join(filename);
    if candidate.parent() != Some(directory) || candidate.file_name() != Some(filename.as_ref()) {
        return Err(AuthorityError::Invalid(format!(
            "attachment filename {filename:?} would escape the export directory"
        )));
    }
    Ok(candidate)
}

pub(crate) fn write_new_export(path: &Path, bytes: &[u8]) -> Result<(), AuthorityError> {
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    let mut file = options.open(path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::AlreadyExists {
            AuthorityError::Conflict(format!(
                "export destination {path:?} already exists; existing files are never overwritten"
            ))
        } else {
            AuthorityError::Invalid(format!(
                "cannot create export destination {path:?}: {error}"
            ))
        }
    })?;
    if let Err(error) = file.write_all(bytes).and_then(|()| file.sync_all()) {
        drop(file);
        let _ = fs::remove_file(path);
        return Err(unavailable(format!(
            "cannot write export destination {path:?}: {error}"
        )));
    }
    Ok(())
}

/// Remove one attachment.
///
/// A staged upload is local intent only, so removing it deletes the staged row
/// and bytes without contacting the server. An accepted attachment can only be
/// removed while online: this build does not queue offline removals.
///
/// The command and activity identities are derived from the attachment
/// identity, so a removal replayed after a lost acknowledgement carries exactly
/// the same immutable receipt identity instead of deleting twice.
pub(crate) fn remove_attachment(
    replica: &mut AuthorityReplica,
    http: Option<&AuthorityHttp>,
    id: &str,
) -> Result<bool, AuthorityError> {
    orbit_domain::AttachmentId::new(id.to_owned())
        .map_err(|_| AuthorityError::Invalid(format!("invalid attachment id {id:?}")))?;
    let staged = replica
        .attachment_upload_by_attachment(id)?
        .filter(|row| row.state != "accepted");
    if let Some(row) = staged {
        replica.remove_attachment_upload(id)?;
        discard_staged_file(&row.staging_path);
        return Ok(false);
    }
    let http = http.ok_or_else(|| {
        unavailable(
            "offline: removing an accepted attachment requires a server connection; staged uploads can be discarded locally, accepted ones cannot"
                .into(),
        )
    })?;
    let command_id = format!("remove-{id}");
    let activity_id = format!("activity-remove-{id}");
    let revision = http
        .get_json("/v1/authoritative/snapshot")
        .map_err(offline_removal)?
        .get("revision")
        .and_then(Value::as_i64)
        .ok_or_else(|| AuthorityError::Invalid("server lacks revision".into()))?;
    let response = http
        .post_json(
            "/v1/authoritative/attachment-remove",
            &serde_json::json!({
                "command_id": command_id,
                "expected_revision": revision,
                "attachment_id": id,
                "activity_id": activity_id,
            }),
        )
        .map_err(offline_removal)?;
    let accepted_revision = response
        .get("revision")
        .and_then(Value::as_i64)
        .ok_or_else(|| {
            AuthorityError::Invalid("attachment removal receipt lacks revision".into())
        })?;
    let duplicate = response
        .get("duplicate")
        .and_then(Value::as_bool)
        .ok_or_else(|| {
            AuthorityError::Invalid("attachment removal receipt lacks duplicate".into())
        })?;
    // `already_removed` means another client has already deleted the
    // association. The association is gone either way, so this is success with
    // no local content removal, not a retryable failure.
    let already_removed = response
        .get("already_removed")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if !duplicate && !already_removed && accepted_revision != revision + 1 {
        return Err(AuthorityError::Invalid(
            "attachment removal receipt revision is invalid".into(),
        ));
    }
    let content_removed = if already_removed {
        false
    } else {
        response
            .get("content_removed")
            .and_then(Value::as_bool)
            .ok_or_else(|| {
                AuthorityError::Invalid("attachment removal receipt lacks content_removed".into())
            })?
    };
    if let Some(row) = replica.attachment_upload_by_attachment(id)? {
        replica.remove_attachment_upload(id)?;
        if row.state != "accepted" {
            discard_staged_file(&row.staging_path);
        }
    }
    replica.forget_cached_attachment(id)?;
    Ok(content_removed)
}

/// An accepted removal is an online-only operation in this build. A transport
/// failure must be actionable rather than a generic debug string.
fn offline_removal(error: AuthorityError) -> AuthorityError {
    match error {
        AuthorityError::Transport(cause) => unavailable(format!(
            "offline: removing an accepted attachment requires a server connection ({cause}); nothing was removed"
        )),
        other => other,
    }
}
